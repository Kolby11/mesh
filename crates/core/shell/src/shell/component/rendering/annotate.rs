use super::*;

pub(super) fn collect_hover_changed_ids(
    previous: &[NodeId],
    current: &[NodeId],
    changed_ids: &mut HashSet<NodeId>,
) {
    let shared_prefix_len = previous
        .iter()
        .zip(current)
        .take_while(|(previous, current)| previous == current)
        .count();
    changed_ids.extend(
        previous[shared_prefix_len..]
            .iter()
            .chain(&current[shared_prefix_len..])
            .copied(),
    );
}

pub(super) fn runtime_style_diagnostic_inputs_changed(
    previous: &mut Option<RuntimeStyleDiagnosticFingerprint>,
    current: RuntimeStyleDiagnosticFingerprint,
) -> bool {
    if *previous == Some(current) {
        return false;
    }
    *previous = Some(current);
    true
}

pub(super) const DIAGNOSTIC_FNV_OFFSET: u64 = 0xcbf29ce484222325;

pub(super) const DIAGNOSTIC_FNV_PRIME: u64 = 0x100000001b3;

pub(super) fn diagnostic_hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(DIAGNOSTIC_FNV_PRIME);
    }
    // Separate adjacent fields, including absent and empty strings.
    *hash ^= 0xff;
    *hash = hash.wrapping_mul(DIAGNOSTIC_FNV_PRIME);
}

pub(super) fn runtime_style_diagnostic_props_fingerprint(props: &SurfaceCssProps) -> u64 {
    use mesh_core_component::style::StyleValue;

    // Hash entries independently and combine commutatively: SurfaceCssProps is
    // rebuilt as a randomized HashMap each paint, so iteration order is not a
    // stable input and sorting would allocate on every diagnostic check.
    let mut combined = (props.len() as u64).wrapping_mul(DIAGNOSTIC_FNV_PRIME);
    for (name, value) in props {
        let mut entry = DIAGNOSTIC_FNV_OFFSET;
        diagnostic_hash_bytes(&mut entry, name.as_bytes());
        match value {
            StyleValue::Literal(value) => {
                diagnostic_hash_bytes(&mut entry, &[0]);
                diagnostic_hash_bytes(&mut entry, value.as_bytes());
            }
            StyleValue::Var(value) => {
                diagnostic_hash_bytes(&mut entry, &[1]);
                diagnostic_hash_bytes(&mut entry, value.as_bytes());
            }
            StyleValue::Prop(value) => {
                diagnostic_hash_bytes(&mut entry, &[2]);
                diagnostic_hash_bytes(&mut entry, value.as_bytes());
            }
        }
        combined ^= entry.rotate_left((entry & 63) as u32);
    }
    combined
}

pub(super) fn apply_runtime_attribute_state(node: &mut WidgetNode) {
    apply_hidden_attribute_layout(node);
    for child in &mut node.children {
        apply_runtime_attribute_state(child);
    }
}

pub(super) fn apply_hidden_attribute_layout(node: &mut WidgetNode) {
    let authored = node.authored_payload();
    let hidden = authored.attributes.get("hidden").is_some_and(|value| {
        matches!(
            value.as_str(),
            "" | "true" | "1" | "hidden" | "disabled" | "checked"
        )
    });
    if hidden && !node.is_promoted_popover() {
        node.computed_style.display = mesh_core_elements::style::Display::None;
    }
}

pub(super) fn apply_runtime_attribute_state_for_ids(
    node: &mut WidgetNode,
    affected_ids: &HashSet<NodeId>,
) -> bool {
    let node_affected = affected_ids.contains(&node.id);
    if node_affected {
        apply_runtime_attribute_state(node);
        return true;
    }
    let mut descendant_affected = false;
    for child in &mut node.children {
        descendant_affected |= apply_runtime_attribute_state_for_ids(child, affected_ids);
    }
    descendant_affected
}

/// Collapses promoted `<popover>` wrappers to a zero-size, overflow-visible box so
/// their (still full-size) popover subtree does not push trigger-row siblings around.
/// A zero flex-basis contributes nothing to the parent's layout, while the overflowing
/// popover content keeps its real size and stays anchored at the wrapper's in-flow
/// position — which child-surface paint and input translation rely on to locate the
/// promoted subtree. (Out-of-flow `position: absolute` would instead relocate the
/// subtree's layout coordinates, breaking that translation.) See
/// the node's typed composition metadata.
pub(super) fn collapse_promoted_popover_wrappers(node: &mut WidgetNode) {
    if node.is_promoted_popover() {
        node.computed_style.width = mesh_core_elements::Dimension::Px(0.0);
        node.computed_style.height = mesh_core_elements::Dimension::Px(0.0);
        node.computed_style.min_width = mesh_core_elements::Dimension::Px(0.0);
        node.computed_style.min_height = mesh_core_elements::Dimension::Px(0.0);
        node.computed_style.overflow_x = mesh_core_elements::style::Overflow::Visible;
        node.computed_style.overflow_y = mesh_core_elements::style::Overflow::Visible;
    }
    for child in &mut node.children {
        collapse_promoted_popover_wrappers(child);
    }
}

/// Keeps generated component-error content from taking over its host layout.
/// These constraints are shell-owned and must be restored after CSS restyling,
/// just like promoted-popover geometry above.
pub(in crate::shell::component) fn constrain_error_placeholders(node: &mut WidgetNode) {
    if node
        .authored_payload()
        .attributes
        .contains_key(ERROR_PLACEHOLDER_MARKER)
    {
        node.computed_style.min_width = mesh_core_elements::Dimension::Px(0.0);
        node.computed_style.max_width =
            mesh_core_elements::Dimension::Px(ERROR_PLACEHOLDER_MAX_WIDTH);
        node.computed_style.flex_shrink = 1.0;
        node.computed_style.overflow_x = mesh_core_elements::style::Overflow::Hidden;
        node.computed_style.overflow_y = mesh_core_elements::style::Overflow::Hidden;
        node.computed_style.white_space = mesh_core_elements::style::WhiteSpace::Nowrap;
        node.computed_style.text_overflow = mesh_core_elements::style::TextOverflow::Ellipsis;
    }
    for child in &mut node.children {
        constrain_error_placeholders(child);
    }
}

#[derive(Default)]
pub(in crate::shell::component) struct InteractionChangedNodeIds {
    pub(in crate::shell::component) affected: HashSet<NodeId>,
}

pub(super) fn direct_interaction_changed_node_ids(
    changed_ids: HashSet<NodeId>,
) -> InteractionChangedNodeIds {
    // `changed_ids` already contains exactly the nodes whose state toggled and
    // can serve directly as both targeted-restyle IDs and runtime-state roots.
    // When targets are nested, the ancestor runtime-state application consumes
    // its whole subtree and returns before the redundant descendant probe.
    InteractionChangedNodeIds {
        affected: changed_ids,
    }
}

pub(super) fn selector_contains_state(selector: &mesh_core_component::style::Selector) -> bool {
    use mesh_core_component::style::Selector;

    match selector {
        Selector::State(_, _) => true,
        Selector::Compound(parts) => parts.iter().any(selector_contains_state),
        Selector::Tag(_) | Selector::Class(_) | Selector::Id(_) | Selector::Universal => false,
    }
}

pub(super) fn collect_selector_state_dependencies(
    selector: &mesh_core_component::style::Selector,
    dependencies: &mut StyleStateDependencies,
) {
    use mesh_core_component::style::Selector;

    match selector {
        Selector::State(_, state) => {
            dependencies.any = true;
            match state.as_str() {
                "hover" | "hovered" => dependencies.hover = true,
                "focus" | "focused" => dependencies.focus = true,
                "focus-visible" => dependencies.focus_visible = true,
                "active" => dependencies.active = true,
                "disabled" => dependencies.disabled = true,
                "checked" => dependencies.checked = true,
                // Window states are ambient and change through
                // `observe_window_states`, which invalidates the whole surface;
                // they never take the targeted interaction-restyle path, so
                // only `any` matters here.
                _ => {}
            }
        }
        Selector::Compound(parts) => {
            for part in parts {
                collect_selector_state_dependencies(part, dependencies);
            }
        }
        Selector::Tag(_) | Selector::Class(_) | Selector::Id(_) | Selector::Universal => {}
    }
}

pub(super) fn append_class(node: &mut WidgetNode, class_name: &str) {
    let class = node.attributes.entry("class".into()).or_default();
    let has_class = class
        .split_whitespace()
        .any(|candidate| candidate == class_name);
    if has_class {
        return;
    }
    if !class.is_empty() {
        class.push(' ');
    }
    class.push_str(class_name);
}

pub(in crate::shell::component) fn append_class_recursive(node: &mut WidgetNode, class_name: &str) {
    append_class(node, class_name);
    for child in &mut node.children {
        append_class_recursive(child, class_name);
    }
}

pub(super) fn find_node_by_key_mut<'a>(
    node: &'a mut WidgetNode,
    key: &str,
) -> Option<&'a mut WidgetNode> {
    if node.mesh_key().is_some_and(|value| value == key) {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_node_by_key_mut(child, key) {
            return Some(found);
        }
    }
    None
}

pub(super) fn annotate_selected_text_node(
    node: &mut WidgetNode,
    selection: &TextSelectionState,
    selection_background: &str,
    selection_foreground: &str,
) -> bool {
    let matches_selection = node.tag == "text"
        && node
            .attributes
            .get("selectable")
            .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1"));
    if matches_selection {
        node.attributes.insert(
            "_mesh_selection_background".into(),
            selection_background.to_string(),
        );
        node.attributes.insert(
            "_mesh_selection_foreground".into(),
            selection_foreground.to_string(),
        );
        node.attributes.insert(
            "_mesh_selection_anchor_x".into(),
            format!("{:.2}", selection.anchor.x),
        );
        node.attributes.insert(
            "_mesh_selection_anchor_y".into(),
            format!("{:.2}", selection.anchor.y),
        );
        node.attributes.insert(
            "_mesh_selection_focus_x".into(),
            format!("{:.2}", selection.focus.x),
        );
        node.attributes.insert(
            "_mesh_selection_focus_y".into(),
            format!("{:.2}", selection.focus.y),
        );
        let text_x = node.layout.x + node.computed_style.padding.left;
        let text_y = node.layout.y + node.computed_style.padding.top;
        node.attributes
            .insert("_mesh_selection_text_x".into(), format!("{text_x:.2}"));
        node.attributes
            .insert("_mesh_selection_text_y".into(), format!("{text_y:.2}"));
        return true;
    }

    false
}

pub(in crate::shell::component) fn narrow_expand_ancestors(
    tree: &WidgetNode,
    affected: &HashSet<NodeId>,
    full_affected: &mut HashSet<NodeId>,
) {
    let mut ancestors = Vec::new();
    narrow_collect_ancestors(tree, affected, &mut ancestors, full_affected);
}

#[cfg(test)]
pub(super) fn count_shared_authored_nodes(current: &WidgetNode, previous: &WidgetNode) -> usize {
    usize::from(current.shares_authored_payload_with(previous))
        + current
            .children
            .iter()
            .zip(&previous.children)
            .map(|(current, previous)| count_shared_authored_nodes(current, previous))
            .sum::<usize>()
}

pub(super) fn narrow_collect_ancestors(
    node: &WidgetNode,
    affected: &HashSet<NodeId>,
    ancestors: &mut Vec<NodeId>,
    full_affected: &mut HashSet<NodeId>,
) {
    if affected.contains(&node.id) {
        full_affected.extend(ancestors.iter().copied());
    }
    ancestors.push(node.id);
    for child in &node.children {
        narrow_collect_ancestors(child, affected, ancestors, full_affected);
    }
    ancestors.pop();
}
