use super::*;

#[derive(Clone)]
pub(super) struct TextMeasureData {
    pub(super) content: Arc<str>,
    pub(super) font_family: Arc<str>,
    pub(super) font_size: f32,
    pub(super) font_weight: u16,
    pub(super) font_style: crate::FontStyle,
    pub(super) letter_spacing: f32,
    pub(super) line_height: f32,
    pub(super) text_direction: crate::TextDirection,
    pub(super) white_space: crate::WhiteSpace,
    pub(super) language: Arc<str>,
    pub(super) shaping_features: Arc<str>,
}

impl TextMeasureData {
    pub(super) fn from_node(node: &WidgetNode) -> Self {
        Self {
            content: node
                .attributes
                .get("content")
                .map(|content| Arc::<str>::from(content.as_str()))
                .unwrap_or_default(),
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
                .get("lang")
                .map(|value| Arc::<str>::from(value.as_str()))
                .unwrap_or_default(),
            shaping_features: node
                .attributes
                .get("font-features")
                .map(|value| Arc::<str>::from(value.as_str()))
                .unwrap_or_default(),
        }
    }

    pub(super) fn context<'a>(
        &'a self,
        max_width: Option<f32>,
        revisions: TextMeasureRevisions,
    ) -> TextMeasureContext<'a> {
        TextMeasureContext {
            text: &self.content,
            font_family: &self.font_family,
            font_size: self.font_size,
            font_weight: self.font_weight,
            font_style: self.font_style,
            letter_spacing: self.letter_spacing,
            line_height: self.line_height,
            text_direction: self.text_direction,
            white_space: self.white_space,
            language: &self.language,
            shaping_features: &self.shaping_features,
            max_width,
            revisions,
        }
    }
}

pub(super) fn taffy_dimension(dimension: Dimension) -> taffy_style::Dimension {
    match dimension {
        Dimension::Auto => taffy_style::Dimension::auto(),
        Dimension::Px(value) => taffy_style::Dimension::length(value),
        Dimension::Percent(value) => taffy_style::Dimension::percent(value / 100.0),
        Dimension::Content => taffy_style::Dimension::auto(),
        Dimension::Fit => taffy_style::Dimension::auto(),
    }
}

pub(super) fn taffy_length_percentage(value: Option<f32>) -> taffy_style::LengthPercentageAuto {
    value
        .map(taffy_style::LengthPercentageAuto::length)
        .unwrap_or_else(taffy_style::LengthPercentageAuto::auto)
}

pub(super) fn taffy_length(value: f32) -> taffy_style::LengthPercentage {
    taffy_style::LengthPercentage::length(value)
}

pub(super) fn taffy_style_for_node(
    node: &WidgetNode,
    report: &mut TaffyLayoutReport,
) -> taffy_style::Style {
    let style = &node.computed_style;

    if matches!(style.width, Dimension::Content) || matches!(style.height, Dimension::Content) {
        record_taffy_diagnostic(report, node, CONTENT_DIMENSION_TAFFY_DIAGNOSTIC);
    }

    let mut taffy = taffy_style::Style {
        display: match style.display {
            Display::Flex => taffy_style::Display::Flex,
            Display::None => taffy_style::Display::None,
        },
        direction: match style.text_direction {
            TextDirection::Ltr => taffy_style::Direction::Ltr,
            TextDirection::Rtl => taffy_style::Direction::Rtl,
        },
        overflow: TaffyPoint {
            x: match style.overflow_x {
                Overflow::Visible => taffy_style::Overflow::Visible,
                Overflow::Hidden | Overflow::Auto => taffy_style::Overflow::Hidden,
                Overflow::Scroll => taffy_style::Overflow::Scroll,
            },
            y: match style.overflow_y {
                Overflow::Visible => taffy_style::Overflow::Visible,
                Overflow::Hidden | Overflow::Auto => taffy_style::Overflow::Hidden,
                Overflow::Scroll => taffy_style::Overflow::Scroll,
            },
        },
        position: match style.position {
            Position::Static | Position::Relative => taffy_style::Position::Relative,
            Position::Absolute | Position::Fixed => taffy_style::Position::Absolute,
        },
        inset: TaffyRect {
            left: taffy_length_percentage(style.inset_left),
            right: taffy_length_percentage(style.inset_right),
            top: taffy_length_percentage(style.inset_top),
            bottom: taffy_length_percentage(style.inset_bottom),
        },
        size: TaffySize {
            width: taffy_dimension(style.width),
            height: taffy_dimension(style.height),
        },
        min_size: TaffySize {
            width: taffy_dimension(style.min_width),
            height: taffy_dimension(style.min_height),
        },
        max_size: TaffySize {
            width: taffy_dimension(style.max_width),
            height: taffy_dimension(style.max_height),
        },
        margin: if node.is_surface_root() {
            // A top-level component root has no authored parent box. Its CSS
            // margins are surface placement, lowered by the shell after
            // layout; applying them here would both move the pixels inside
            // the buffer and apply the compositor margin a second time.
            TaffyRect {
                left: taffy_style::LengthPercentageAuto::auto(),
                right: taffy_style::LengthPercentageAuto::auto(),
                top: taffy_style::LengthPercentageAuto::auto(),
                bottom: taffy_style::LengthPercentageAuto::auto(),
            }
        } else {
            TaffyRect {
                left: taffy_style::LengthPercentageAuto::length(style.margin.left),
                right: taffy_style::LengthPercentageAuto::length(style.margin.right),
                top: taffy_style::LengthPercentageAuto::length(style.margin.top),
                bottom: taffy_style::LengthPercentageAuto::length(style.margin.bottom),
            }
        },
        padding: TaffyRect {
            left: taffy_length(style.padding.left),
            right: taffy_length(style.padding.right),
            top: taffy_length(style.padding.top),
            bottom: taffy_length(style.padding.bottom),
        },
        border: TaffyRect {
            left: taffy_length(style.border_width.left),
            right: taffy_length(style.border_width.right),
            top: taffy_length(style.border_width.top),
            bottom: taffy_length(style.border_width.bottom),
        },
        align_items: Some(match style.align_items {
            AlignItems::Start => taffy_style::AlignItems::FlexStart,
            AlignItems::End => taffy_style::AlignItems::FlexEnd,
            AlignItems::Center => taffy_style::AlignItems::Center,
            AlignItems::Stretch => taffy_style::AlignItems::Stretch,
        }),
        align_content: Some(match style.align_content {
            AlignContent::Start => taffy_style::AlignContent::FlexStart,
            AlignContent::End => taffy_style::AlignContent::FlexEnd,
            AlignContent::Center => taffy_style::AlignContent::Center,
            AlignContent::SpaceBetween => taffy_style::AlignContent::SpaceBetween,
            AlignContent::SpaceAround => taffy_style::AlignContent::SpaceAround,
            AlignContent::Stretch => taffy_style::AlignContent::Stretch,
        }),
        align_self: match style.align_self {
            AlignSelf::Auto => None,
            AlignSelf::Start => Some(taffy_style::AlignSelf::FlexStart),
            AlignSelf::End => Some(taffy_style::AlignSelf::FlexEnd),
            AlignSelf::Center => Some(taffy_style::AlignSelf::Center),
            AlignSelf::Stretch => Some(taffy_style::AlignSelf::Stretch),
            AlignSelf::Baseline => Some(taffy_style::AlignSelf::Baseline),
        },
        justify_content: Some(match style.justify_content {
            JustifyContent::Start => taffy_style::JustifyContent::FlexStart,
            JustifyContent::End => taffy_style::JustifyContent::FlexEnd,
            JustifyContent::Center => taffy_style::JustifyContent::Center,
            JustifyContent::SpaceBetween => taffy_style::JustifyContent::SpaceBetween,
            JustifyContent::SpaceAround => taffy_style::JustifyContent::SpaceAround,
        }),
        gap: TaffySize {
            width: taffy_length(style.gap),
            height: taffy_length(style.gap),
        },
        flex_direction: match style.direction {
            FlexDirection::Row => taffy_style::FlexDirection::Row,
            FlexDirection::Column => taffy_style::FlexDirection::Column,
        },
        flex_basis: taffy_dimension(style.flex_basis),
        aspect_ratio: style.aspect_ratio,
        flex_grow: style.flex_grow.max(0.0),
        flex_shrink: style.flex_shrink.max(0.0),
        ..Default::default()
    };

    taffy.flex_wrap = match style.flex_wrap {
        crate::style::FlexWrap::NoWrap => taffy_style::FlexWrap::NoWrap,
        crate::style::FlexWrap::Wrap => taffy_style::FlexWrap::Wrap,
        crate::style::FlexWrap::WrapReverse => taffy_style::FlexWrap::WrapReverse,
    };
    taffy
}

pub(super) fn is_expected_taffy_measurement_diagnostic(reason: &str) -> bool {
    reason == CONTENT_DIMENSION_TAFFY_DIAGNOSTIC
}

pub(super) fn build_taffy_tree(
    node: &WidgetNode,
    tree: &mut TaffyTree<NodeId>,
    node_map: &mut HashMap<NodeId, TaffyNodeId>,
    text_nodes: &mut HashMap<NodeId, TextMeasureData>,
    report: &mut TaffyLayoutReport,
) -> Result<TaffyNodeId, taffy::TaffyError> {
    let style = taffy_style_for_node(node, report);
    let taffy_node = if node.children.is_empty() {
        if node.tag == "text" {
            text_nodes.insert(node.id, TextMeasureData::from_node(node));
        }
        tree.new_leaf_with_context(style, node.id)?
    } else {
        let children = node
            .children
            .iter()
            .map(|child| build_taffy_tree(child, tree, node_map, text_nodes, report))
            .collect::<Result<Vec<_>, _>>()?;
        tree.new_with_children(style, &children)?
    };

    node_map.insert(node.id, taffy_node);
    Ok(taffy_node)
}

pub(super) fn collect_stable_node_ids(node: &WidgetNode, ids: &mut HashSet<NodeId>) {
    if node.has_mesh_key() {
        ids.insert(node.id);
    }
    for child in &node.children {
        collect_stable_node_ids(child, ids);
    }
}

pub(super) fn collect_node_ids(node: &WidgetNode, ids: &mut HashSet<NodeId>) {
    ids.insert(node.id);
    for child in &node.children {
        collect_node_ids(child, ids);
    }
}

pub(super) fn retained_taffy_id(
    node: &WidgetNode,
    state: &PerSurfaceLayoutState,
) -> Option<TaffyNodeId> {
    state.node_map.get(&node.id).copied()
}

#[cfg(test)]
pub(super) fn collect_taffy_node_map(
    node: &WidgetNode,
    state: &PerSurfaceLayoutState,
    node_map: &mut HashMap<NodeId, TaffyNodeId>,
) {
    if let Some(taffy_id) = retained_taffy_id(node, state) {
        node_map.insert(node.id, taffy_id);
    }
    for child in &node.children {
        collect_taffy_node_map(child, state, node_map);
    }
}

pub(super) fn taffy_available_space(width: f32, height: f32) -> TaffySize<TaffyAvailableSpace> {
    TaffySize {
        width: TaffyAvailableSpace::Definite(width),
        height: TaffyAvailableSpace::Definite(height),
    }
}

pub(super) fn log_taffy_report(report: &TaffyLayoutReport) {
    for diagnostic in &report.diagnostics {
        if is_expected_taffy_measurement_diagnostic(&diagnostic.reason) {
            tracing::debug!(
                target: "mesh::layout",
                node_id = diagnostic.node_id,
                tag = %diagnostic.tag,
                reason = %diagnostic.reason,
                "taffy layout diagnostic"
            );
        } else {
            tracing::warn!(
                target: "mesh::layout",
                node_id = diagnostic.node_id,
                tag = %diagnostic.tag,
                reason = %diagnostic.reason,
                "taffy layout diagnostic"
            );
        }
    }
}

pub(super) fn measure_taffy_node(
    known_dimensions: TaffySize<Option<f32>>,
    available_space: TaffySize<TaffyAvailableSpace>,
    node_id: Option<NodeId>,
    text_nodes: &HashMap<NodeId, TextMeasureData>,
    intrinsic_cache: &mut IntrinsicLayoutCache,
    measurer: Option<&dyn TextMeasurer>,
) -> TaffySize<f32> {
    let Some(node_id) = node_id else {
        return TaffySize::ZERO;
    };
    let Some(text) = text_nodes.get(&node_id) else {
        return TaffySize::ZERO;
    };
    let Some(measurer) = measurer else {
        return TaffySize {
            width: known_dimensions.width.unwrap_or(0.0),
            height: known_dimensions.height.unwrap_or(0.0),
        };
    };

    let max_width = if text.white_space == crate::WhiteSpace::Nowrap {
        known_dimensions.width
    } else {
        known_dimensions
            .width
            .or_else(|| available_space_to_option(available_space.width))
    };
    let revisions = measurer.revisions();
    intrinsic_cache.invalidate_text_measurements_if_needed(revisions);
    let context = text.context(max_width, revisions);
    let measure_key = TextMeasureKey::new(&context);
    let (measured_width, measured_height) =
        if let Some(measured) = intrinsic_cache.get_text_measurement(&measure_key) {
            measured
        } else {
            let measured = measurer.measure_text(&context);
            intrinsic_cache.insert_text_measurement(measure_key, measured);
            measured
        };

    TaffySize {
        width: known_dimensions.width.unwrap_or(measured_width),
        height: known_dimensions.height.unwrap_or(measured_height),
    }
}

pub(super) fn available_space_to_option(value: TaffyAvailableSpace) -> Option<f32> {
    match value {
        TaffyAvailableSpace::Definite(value) => Some(value),
        TaffyAvailableSpace::MinContent | TaffyAvailableSpace::MaxContent => None,
    }
}

pub(super) fn write_taffy_layout(
    node: &mut WidgetNode,
    tree: &TaffyTree<NodeId>,
    node_map: &HashMap<NodeId, TaffyNodeId>,
    viewport_w: f32,
    viewport_h: f32,
) {
    write_taffy_layout_with_parent(node, tree, node_map, None, 0.0, 0.0, viewport_w, viewport_h);
    resolve_fit_sizing(node);
}

/// Grow every `width/height: fit` node to cover the union bounding box of
/// *all* its descendants — including absolutely/fixed-positioned ones, which
/// Taffy's own flex sizing (and standard `fit-content`) never counts, since
/// out-of-flow elements don't contribute to intrinsic sizing per spec. Runs
/// bottom-up after `write_taffy_layout_with_parent` has already resolved
/// every node's absolute on-screen box, so a `fit` node whose own child is
/// also `fit` picks up that child's corrected size.
///
/// This only *extends* the box Taffy already computed (`.max(...)`) — normal
/// in-flow content is already covered by Taffy's own auto-sizing and this is
/// a no-op for it. It does not re-run flex layout, so a `fit` node's own
/// position and its flex siblings' positions are unaffected: this corrects
/// the node's own box in place, it does not reflow its container.
pub(super) fn resolve_fit_sizing(node: &mut WidgetNode) {
    for child in &mut node.children {
        resolve_fit_sizing(child);
    }

    let fit_width = matches!(node.computed_style.width, Dimension::Fit);
    let fit_height = matches!(node.computed_style.height, Dimension::Fit);
    if !fit_width && !fit_height {
        return;
    }

    let mut max_right = node.layout.x + node.layout.width;
    let mut max_bottom = node.layout.y + node.layout.height;
    accumulate_descendant_extents(node, &mut max_right, &mut max_bottom);

    if fit_width {
        node.layout.width = (max_right - node.layout.x).max(0.0);
    }
    if fit_height {
        node.layout.height = (max_bottom - node.layout.y).max(0.0);
    }
}

/// Accumulate the furthest right/bottom screen-space edge across all
/// descendants of `node` (any depth). `position: fixed` subtrees are
/// skipped: `write_taffy_layout_with_parent` positions those relative to the
/// viewport origin, not to `node`, so their coordinates aren't comparable to
/// `node`'s own box and would otherwise blow the bounding box out arbitrarily.
pub(super) fn accumulate_descendant_extents(
    node: &WidgetNode,
    max_right: &mut f32,
    max_bottom: &mut f32,
) {
    for child in &node.children {
        // Promoted popovers are retained below a zero-size wrapper so the
        // child surface can paint and interact with them. Their visual bounds
        // are intentionally out of the parent's layout contract, however;
        // including them here makes a fit-sized control row grow to the
        // popup's right edge and can push ordinary controls off-screen.
        if child.is_promoted_popover() {
            continue;
        }
        if child.computed_style.position == Position::Fixed {
            continue;
        }
        *max_right = max_right.max(child.layout.x + child.layout.width);
        *max_bottom = max_bottom.max(child.layout.y + child.layout.height);
        accumulate_descendant_extents(child, max_right, max_bottom);
    }
}

pub(super) fn write_taffy_layout_with_parent(
    node: &mut WidgetNode,
    tree: &TaffyTree<NodeId>,
    node_map: &HashMap<NodeId, TaffyNodeId>,
    parent_padding: Option<Edges>,
    parent_x: f32,
    parent_y: f32,
    viewport_w: f32,
    viewport_h: f32,
) {
    if node.computed_style.display == Display::None {
        zero_layout_subtree(node);
        return;
    }

    if let Some(taffy_node) = node_map.get(&node.id)
        && let Ok(layout) = tree.layout(*taffy_node)
    {
        node.layout = LayoutRect {
            x: parent_x + layout.location.x,
            y: parent_y + layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        };

        if node.computed_style.position == Position::Absolute
            && let Some(padding) = parent_padding
        {
            if node.computed_style.inset_left.is_some() {
                node.layout.x += padding.left;
            }
            if node.computed_style.inset_top.is_some() {
                node.layout.y += padding.top;
            }
            if node.computed_style.inset_left.is_some() && node.computed_style.inset_right.is_some()
            {
                node.layout.width = (node.layout.width - padding.horizontal()).max(0.0);
            }
            if node.computed_style.inset_top.is_some() && node.computed_style.inset_bottom.is_some()
            {
                node.layout.height = (node.layout.height - padding.vertical()).max(0.0);
            }
        }

        // Fixed: positioned relative to the viewport, ignoring scroll and transforms.
        // Override x/y (and size when both edges constrain it) using viewport dimensions.
        if node.computed_style.position == Position::Fixed {
            let w = node.layout.width;
            let h = node.layout.height;
            let s = &node.computed_style;
            match (s.inset_left, s.inset_right) {
                (Some(l), Some(r)) => {
                    node.layout.x = l;
                    node.layout.width = (viewport_w - l - r).max(0.0);
                }
                (Some(l), None) => node.layout.x = l,
                (None, Some(r)) => node.layout.x = (viewport_w - w - r).max(0.0),
                (None, None) => node.layout.x = 0.0,
            }
            match (s.inset_top, s.inset_bottom) {
                (Some(t), Some(b)) => {
                    node.layout.y = t;
                    node.layout.height = (viewport_h - t - b).max(0.0);
                }
                (Some(t), None) => node.layout.y = t,
                (None, Some(b)) => node.layout.y = (viewport_h - h - b).max(0.0),
                (None, None) => node.layout.y = 0.0,
            }
        }
    }

    let padding = node.computed_style.padding;
    for child in &mut node.children {
        // Fixed children are positioned from the viewport origin, not the parent.
        let (child_parent_x, child_parent_y, child_parent_padding) =
            if child.computed_style.position == Position::Fixed {
                (0.0, 0.0, None)
            } else {
                (node.layout.x, node.layout.y, Some(padding))
            };
        write_taffy_layout_with_parent(
            child,
            tree,
            node_map,
            child_parent_padding,
            child_parent_x,
            child_parent_y,
            viewport_w,
            viewport_h,
        );
    }
}

pub(super) fn zero_layout_subtree(node: &mut WidgetNode) {
    node.layout = LayoutRect::default();
    for child in &mut node.children {
        zero_layout_subtree(child);
    }
}

/// Remove a Taffy node and all its descendants, post-order.
///
/// [`TaffyTree::remove`] only detaches the parent and orphans its
/// children — it does NOT recurse.  This helper walks children first
/// (post-order) so no orphan TaffyNodeIds accumulate (LAYOUT-04).
pub fn remove_taffy_subtree(
    tree: &mut TaffyTree<NodeId>,
    node_id: TaffyNodeId,
) -> Result<(), taffy::TaffyError> {
    // Snapshot children before any mutation — once we remove the parent,
    // the children handles become invalid.
    let children = tree.children(node_id).unwrap_or_default();
    // Post-order: remove children first so no orphan TaffyNodeIds accumulate.
    for child in children {
        remove_taffy_subtree(tree, child)?;
    }
    tree.remove(node_id)?;
    Ok(())
}
