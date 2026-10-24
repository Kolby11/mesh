use super::*;
use crate::Prop;

pub(super) fn compute_fresh_retained_layout(
    root: &mut WidgetNode,
    state: &mut PerSurfaceLayoutState,
    available_width: f32,
    available_height: f32,
    intrinsic_cache: &mut IntrinsicLayoutCache,
    measurer: Option<&dyn TextMeasurer>,
) {
    let mut report = TaffyLayoutReport::default();

    state.tree = unrounded_taffy_tree();
    state.node_map.clear();
    state.stable_node_ids.clear();
    state.text_nodes.clear();

    match build_taffy_tree(
        root,
        &mut state.tree,
        &mut state.node_map,
        &mut state.text_nodes,
        &mut report,
    ) {
        Ok(root_id) => {
            collect_stable_node_ids(root, &mut state.stable_node_ids);
            let available_space = taffy_available_space(available_width, available_height);
            let (tree, text_nodes) = (&mut state.tree, &state.text_nodes);
            if let Err(error) = tree.compute_layout_with_measure(
                root_id,
                available_space,
                |known_dimensions, available_space, _node_id, context, _style| {
                    measure_taffy_node(
                        known_dimensions,
                        available_space,
                        context.map(|node_id| *node_id),
                        text_nodes,
                        intrinsic_cache,
                        measurer,
                    )
                },
            ) {
                tracing::warn!(
                    target: "mesh::layout",
                    error = %error,
                    "retained taffy fresh layout computation failed"
                );
                state.valid = false;
            } else {
                sparse::write_full_layout(root, state, available_width, available_height);
                state.last_available = (available_width, available_height);
                state.text_measure_revisions = measurer
                    .map(|measurer| measurer.revisions())
                    .unwrap_or_default();
                state.valid = true;
            }
        }
        Err(error) => {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "retained taffy tree construction failed"
            );
            state.valid = false;
        }
    }

    log_taffy_report(&report);
}

pub(super) fn compute_structural_retained_layout(
    root: &mut WidgetNode,
    state: &mut PerSurfaceLayoutState,
    available_width: f32,
    available_height: f32,
    intrinsic_cache: &mut IntrinsicLayoutCache,
    measurer: Option<&dyn TextMeasurer>,
) {
    let mut report = TaffyLayoutReport::default();
    let mut present_ids = HashSet::new();
    collect_node_ids(root, &mut present_ids);
    // An unkeyed node may happen to retain a runtime ID in a caller-owned
    // tree, but that ID does not express structural identity. Save its old
    // Taffy node before reconciliation so it is removed after replacement.
    let obsolete_taffy_ids = state
        .node_map
        .iter()
        .filter(|(node_id, _)| {
            !state.stable_node_ids.contains(node_id) || !present_ids.contains(node_id)
        })
        .map(|(_, taffy_id)| *taffy_id)
        .collect::<HashSet<_>>();

    match reconcile_retained_taffy_node(root, state, &mut report) {
        Ok(root_id) => {
            let stale_roots = obsolete_taffy_ids
                .iter()
                .filter(|taffy_id| {
                    !state
                        .tree
                        .parent(**taffy_id)
                        .is_some_and(|parent| obsolete_taffy_ids.contains(&parent))
                })
                .copied()
                .collect::<Vec<_>>();
            for taffy_id in stale_roots {
                if let Err(error) = remove_taffy_subtree(&mut state.tree, taffy_id) {
                    tracing::warn!(
                        target: "mesh::layout",
                        error = %error,
                        "failed to remove stale retained layout subtree"
                    );
                    state.valid = false;
                    log_taffy_report(&report);
                    return;
                }
            }
            state
                .node_map
                .retain(|node_id, _| present_ids.contains(node_id));
            state.stable_node_ids.clear();
            collect_stable_node_ids(root, &mut state.stable_node_ids);
            state
                .text_nodes
                .retain(|node_id, _| state.node_map.contains_key(node_id));

            let available_space = taffy_available_space(available_width, available_height);
            let (tree, text_nodes) = (&mut state.tree, &state.text_nodes);
            if let Err(error) = tree.compute_layout_with_measure(
                root_id,
                available_space,
                |known_dimensions, available_space, _node_id, context, _style| {
                    measure_taffy_node(
                        known_dimensions,
                        available_space,
                        context.map(|node_id| *node_id),
                        text_nodes,
                        intrinsic_cache,
                        measurer,
                    )
                },
            ) {
                tracing::warn!(
                    target: "mesh::layout",
                    error = %error,
                    "retained taffy structural layout computation failed"
                );
                state.valid = false;
            } else {
                sparse::write_full_layout(root, state, available_width, available_height);
                state.last_available = (available_width, available_height);
                state.text_measure_revisions = measurer
                    .map(|measurer| measurer.revisions())
                    .unwrap_or_default();
                state.valid = true;
            }
        }
        Err(error) => {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "retained taffy structural reconciliation failed"
            );
            state.valid = false;
        }
    }

    log_taffy_report(&report);
}

pub(super) fn reconcile_retained_taffy_node(
    node: &WidgetNode,
    state: &mut PerSurfaceLayoutState,
    report: &mut TaffyLayoutReport,
) -> Result<TaffyNodeId, taffy::TaffyError> {
    crate::frame_work::visit(crate::frame_work::Pass::LayoutBuild);
    let style = taffy_style_for_node(node, report);
    let retained = node.has_mesh_key();
    let taffy_id = if retained {
        if let Some(existing) = state.node_map.get(&node.id).copied() {
            if state.tree.style(existing)? != &style {
                state.tree.set_style(existing, style)?;
            }
            existing
        } else {
            let created = state.tree.new_leaf(style)?;
            state.node_map.insert(node.id, created);
            created
        }
    } else {
        // Unkeyed nodes cannot be retained safely across TREE_REBUILD passes:
        // there is no stable identity to reconcile against (RESEARCH.md Pitfall 3).
        state.tree.new_leaf(style)?
    };

    update_text_context(node, &mut state.tree, taffy_id, &mut state.text_nodes)?;
    state.node_map.insert(node.id, taffy_id);

    let child_ids = node
        .children
        .iter()
        .map(|child| reconcile_retained_taffy_node(child, state, report))
        .collect::<Result<Vec<_>, _>>()?;
    if state.tree.children(taffy_id)? != child_ids {
        state.tree.set_children(taffy_id, &child_ids)?;
    }
    Ok(taffy_id)
}

pub(super) fn update_retained_node_styles(
    node: &WidgetNode,
    state: &mut PerSurfaceLayoutState,
    mark_dirty: bool,
    dirty_node_ids: Option<&HashSet<NodeId>>,
    report: &mut TaffyLayoutReport,
) -> Result<(), taffy::TaffyError> {
    crate::frame_work::visit(crate::frame_work::Pass::LayoutSync);
    if let Some(taffy_id) = retained_taffy_id(node, state) {
        let node_dirty = dirty_node_ids.is_none_or(|ids| ids.contains(&node.id));
        if node_dirty {
            let style = taffy_style_for_node(node, report);
            if let Err(error) = state.tree.set_style(taffy_id, style) {
                tracing::warn!(
                    target: "mesh::layout",
                    error = %error,
                    "failed to update retained taffy style"
                );
                return Err(error);
            }
            if mark_dirty && let Err(error) = state.tree.mark_dirty(taffy_id) {
                tracing::warn!(
                    target: "mesh::layout",
                    error = %error,
                    "failed to mark retained taffy node dirty"
                );
                return Err(error);
            }
        }
        update_text_context(node, &mut state.tree, taffy_id, &mut state.text_nodes)?;
    }

    for child in &node.children {
        update_retained_node_styles(child, state, mark_dirty, dirty_node_ids, report)?;
    }
    Ok(())
}

/// Synchronize only the nodes selected by the retained-tree layout diff.
///
/// The ordinary ID-based entry point remains available for callers that only
/// have a dirty set. The shell's retained tree can provide sparse owned node
/// snapshots, however, which lets this path avoid walking clean descendants
/// just to resolve their Taffy IDs.
pub(super) fn update_retained_node_snapshots(
    state: &mut PerSurfaceLayoutState,
    mark_dirty: bool,
    dirty_nodes: &[WidgetNode],
    report: &mut TaffyLayoutReport,
) -> Result<(), taffy::TaffyError> {
    for node in dirty_nodes {
        crate::frame_work::visit(crate::frame_work::Pass::LayoutSync);
        let Some(taffy_id) = retained_taffy_id(node, state) else {
            continue;
        };
        let style = taffy_style_for_node(node, report);
        if let Err(error) = state.tree.set_style(taffy_id, style) {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "failed to update retained taffy style from dirty snapshot"
            );
            return Err(error);
        }
        if mark_dirty && let Err(error) = state.tree.mark_dirty(taffy_id) {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "failed to mark retained taffy node dirty from dirty snapshot"
            );
            return Err(error);
        }
        update_text_context(node, &mut state.tree, taffy_id, &mut state.text_nodes)?;
    }
    Ok(())
}

pub(super) fn update_text_context(
    node: &WidgetNode,
    tree: &mut TaffyTree<NodeId>,
    taffy_id: TaffyNodeId,
    text_nodes: &mut HashMap<NodeId, TextMeasureData>,
) -> Result<(), taffy::TaffyError> {
    if node.tag == "text" {
        let content = node
            .attributes
            .get(Prop::Content)
            .map(String::as_str)
            .unwrap_or_default();
        let unchanged = text_nodes.get(&node.id).is_some_and(|existing| {
            existing.content.as_ref() == content
                && existing.font_family == node.computed_style.font_family
                && existing.font_size == node.computed_style.font_size
                && existing.font_weight == node.computed_style.font_weight
                && existing.font_style == node.computed_style.font_style
                && existing.letter_spacing == node.computed_style.letter_spacing
                && existing.line_height == node.computed_style.line_height
                && existing.text_direction == node.computed_style.text_direction
                && existing.white_space == node.computed_style.white_space
                && existing.language.as_ref()
                    == node
                        .attributes
                        .get(Prop::Lang)
                        .map(String::as_str)
                        .unwrap_or_default()
                && existing.shaping_features.as_ref()
                    == node
                        .attributes
                        .get(Prop::FontFeatures)
                        .map(String::as_str)
                        .unwrap_or_default()
        });
        if !unchanged {
            text_nodes.insert(
                node.id,
                TextMeasureData {
                    probes: std::cell::RefCell::new(Some(Vec::new())),
                    content: Arc::from(content),
                    font_family: node.computed_style.font_family.clone(),
                    font_size: node.computed_style.font_size,
                    font_weight: node.computed_style.font_weight,
                    font_style: node.computed_style.font_style,
                    letter_spacing: node.computed_style.letter_spacing,
                    line_height: node.computed_style.line_height,
                    text_direction: node.computed_style.text_direction,
                    white_space: node.computed_style.white_space,
                    language: node
                        .attributes
                        .get(Prop::Lang)
                        .map(|value| Arc::from(value.as_str()))
                        .unwrap_or_default(),
                    shaping_features: node
                        .attributes
                        .get(Prop::FontFeatures)
                        .map(|value| Arc::from(value.as_str()))
                        .unwrap_or_default(),
                },
            );
            // Taffy caches the old measurement until the node is dirtied;
            // structural reconciliation keeps the node and its context ID.
            tree.mark_dirty(taffy_id)?;
        }
        if tree.get_node_context(taffy_id) != Some(&node.id) {
            tree.set_node_context(taffy_id, Some(node.id))?;
        }
    } else if tree.get_node_context(taffy_id).is_some() {
        tree.set_node_context(taffy_id, None)?;
    }
    Ok(())
}

/// Retain Taffy's cached geometry only when every previously queried intrinsic
/// size is unchanged. The final box alone is insufficient for wrapped or
/// flex-shrunk text: ancestors also cache unconstrained measurement results.
pub(super) fn preserve_text_patch_geometry(
    state: &mut PerSurfaceLayoutState,
    nodes: &[WidgetNode],
    cache: &mut IntrinsicLayoutCache,
    measurer: &dyn TextMeasurer,
) -> bool {
    let revisions = measurer.revisions();
    let mut replacements = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Some(id) = retained_taffy_id(node, state) else {
            return false;
        };
        if node.tag != "text" || !node.children.is_empty() {
            return false;
        }
        let mut report = TaffyLayoutReport::default();
        if state.tree.style(id).ok() != Some(&taffy_style_for_node(node, &mut report)) {
            return false;
        }
        let Some(old) = state.text_nodes.get(&node.id) else {
            return false;
        };
        let next = TextMeasureData::from_node(node);
        // Content alone may change; font, language and shaping changes use layout.
        if old.measure_key(None, revisions) != {
            let mut key = next.measure_key(None, revisions);
            key.content = old.content.clone();
            key
        } {
            return false;
        }
        let probes = old.probes.borrow();
        let Some(entries) = probes.as_ref().filter(|entries| !entries.is_empty()) else {
            return false;
        };
        for probe in entries {
            let width = probe.max_width;
            let previous = probe.size;
            let key = next.measure_key(width, revisions);
            let measured = if let Some(value) = cache.get_text_measurement(&key) {
                value
            } else {
                let value = measurer.measure_text(&next.context(width, revisions));
                cache.insert_text_measurement(key, value);
                value
            };
            let size = (
                probe.known_dimensions.width.unwrap_or(measured.0.ceil()),
                probe.known_dimensions.height.unwrap_or(measured.1),
            );
            if !size.0.is_finite() || !size.1.is_finite() || size != previous {
                return false;
            }
        }
        *next.probes.borrow_mut() = Some(entries.clone());
        replacements.push((node.id, next));
    }
    state.text_nodes.extend(replacements);
    state.changed_nodes = Some(HashSet::new());
    true
}
