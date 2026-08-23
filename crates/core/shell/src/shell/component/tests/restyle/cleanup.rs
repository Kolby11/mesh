use super::*;

#[test]
fn restyle_state_cleanup_focus_cleared_when_node_removed() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    <button />
  </column>
</template>
<script lang="luau">
</script>
"#,
    );

    // Set a focused_key that does not exist in this single-button tree.
    component.focused_key = Some("root/0/1".into()); // stale — no such node
    component.dirty = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();

    assert!(
        component.focused_key.is_none(),
        "focused_key must be cleared when the focused node is absent from the final tree"
    );

    // The existing button must not gain accidental focus.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        !node_by_mesh_key(tree, "root/0/0").state.focused,
        "existing button must not inherit stale focused_key"
    );
}

/// When the active (pointer-down) node is removed from the tree, the
/// `pointer_down_id` must be cleared deterministically.
#[test]
fn restyle_state_cleanup_active_cleared_when_node_removed() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    <button />
  </column>
</template>
<script lang="luau">
</script>
"#,
    );

    // Set a stale pointer_down_id pointing to a non-existent node.
    component.pointer_down_id = Some(runtime_node_id_for_key("root/0/99"));
    component.dirty = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();

    assert!(
        component.pointer_down_id.is_none(),
        "pointer_down_id must be cleared when the active node is absent from the final tree"
    );

    // Existing button must not show stale active styling.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        !node_by_mesh_key(tree, "root/0/0").state.active,
        "existing button must not inherit stale active (pointer-down) state"
    );
}

/// Valid interaction targets whose keys exist in the final tree must NOT be
/// cleared — prune only removes absent keys.
#[test]
fn restyle_state_cleanup_preserves_valid_interaction_targets() {
    let mut component = test_frontend_component(
        r#"
<style>
button:focus {
  width: 80px;
}
button:hover {
  height: 30px;
}
</style>
<template>
  <column>
    <button />
    <button />
  </column>
</template>
<script lang="luau">
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);

    component
        .paint(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();

    // Both keys are valid — set focus on first, hover on second.
    component.focused_key = Some("root/0/0".into());
    component.hovered_key = Some("root/0/1".into());
    component.hovered_path = ["root", "root/0", "root/0/1"]
        .map(runtime_node_id_for_key)
        .to_vec();
    component.pointer_down_id = Some(runtime_node_id_for_key("root/0/0"));
    component.dirty = true;
    component
        .paint(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();

    // All valid targets must survive pruning.
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/0/0"),
        "focused_key for a present node must not be pruned"
    );
    assert_eq!(
        component.hovered_key.as_deref(),
        Some("root/0/1"),
        "hovered_key for a present node must not be pruned"
    );
    assert_eq!(
        component.pointer_down_id,
        Some(runtime_node_id_for_key("root/0/0")),
        "pointer_down_id for a present node must not be pruned"
    );

    // State flags must be applied correctly.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(node_by_mesh_key(tree, "root/0/0").state.focused);
    assert!(node_by_mesh_key(tree, "root/0/1").state.hovered);
    assert!(node_by_mesh_key(tree, "root/0/0").state.active);
}

#[test]
fn interaction_target_cleanup_tracks_nested_node_ids_through_reorder_and_removal() {
    let mut component = test_frontend_component("<template><box /></template>");
    let mut nested_component = event_node("box", "nested-component", 0.0, 0.0, 120.0, 32.0, &[]);
    let dragged_slider = event_node(
        "slider",
        "nested-component/volume",
        0.0,
        0.0,
        120.0,
        20.0,
        &[],
    );
    let dragged_id = dragged_slider.id;
    nested_component.children = vec![dragged_slider].into();
    let sibling = event_node("button", "list-item", 0.0, 36.0, 120.0, 20.0, &[]);

    component.pointer_down_id = Some(dragged_id);
    component.active_slider_id = Some(dragged_id);

    // Reordering an enclosing keyed-list item must keep the nested drag target.
    let reordered = root_with(vec![sibling, nested_component]);
    component.prune_stale_interaction_targets(&reordered);
    assert_eq!(component.pointer_down_id, Some(dragged_id));
    assert_eq!(component.active_slider_id, Some(dragged_id));

    // Once the item is removed, both identities are cleared together.
    let removed = root_with(vec![]);
    component.prune_stale_interaction_targets(&removed);
    assert!(component.pointer_down_id.is_none());
    assert!(component.active_slider_id.is_none());
}

#[test]
fn selection_boundaries_ignore_selectable_text_inside_controls() {
    let mut component = test_frontend_component("<template><box /></template>");
    let mut button = event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        120.0,
        32.0,
        &[("click", "noop")],
    );
    button
        .children
        .push(text_node("root/0/0", 4.0, 4.0, 100.0, 20.0, true));
    component.last_tree = Some(root_with(vec![button]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                button: 0x110,
                x: 8.0,
                y: 8.0,
                pressed: true,
            },
        )
        .unwrap();

    assert!(
        component.selection.is_none(),
        "selectable text nested inside controls must not start Phase 10 selection"
    );
    assert_eq!(
        component.pointer_down_id,
        Some(component.last_tree.as_ref().unwrap().children[0].id),
        "control pointer handling should still win when text lives inside a button"
    );
}

#[test]
fn selection_boundaries_clamp_drag_to_same_text_node() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.last_tree = Some(root_with(vec![
        text_node("root/0", 0.0, 0.0, 100.0, 20.0, true),
        text_node("root/1", 120.0, 0.0, 100.0, 20.0, true),
    ]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                button: 0x110,
                x: 8.0,
                y: 8.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 140.0, y: 8.0 },
        )
        .unwrap();

    let selection = component
        .selection
        .as_ref()
        .expect("selection should start");
    assert_eq!(selection.anchor.node_key, "root/0");
    assert_eq!(
        selection.focus.node_key, "root/0",
        "Phase 10 selection must stay within the first selectable text node"
    );
}
