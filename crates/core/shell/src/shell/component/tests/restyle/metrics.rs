use super::*;

// ---- 09-03: post-restyle synchronization tests ----

/// D-04: Hit testing uses final post-restyle bounds.
///
/// When a hover restyle changes an element's size (e.g., from 40px to 80px),
/// the next pointer event must resolve against the updated layout, not the
/// pre-restyle bounds. This proves that `build_tree` recomputes layout after
/// `restyle_subtree_cached`.
#[test]
fn restyle_hit_test_uses_post_restyle_bounds() {
    // The button starts at width: 40px.  On hover the style rule widens it to 80px.
    // We set the hovered path so the restyle fires immediately on the first paint.
    // After paint, a pointer click at x=60 (inside 80px, outside 40px) must find
    // a click handler on the button, proving the post-restyle bounds were used.
    let mut component = test_frontend_component(
        r#"
<style>
button {
  width: 40px;
  height: 20px;
  background-color: #111111;
}
button:hover {
  width: 80px;
}
</style>
<template>
  <button onclick={onClick} />
</template>
<script lang="luau">
clicked = false
function onClick()
    clicked = true
end
</script>
"#,
    );
    // Pre-hover paint: button is 40px wide, no hover state yet.
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();

    // Simulate a hover over the button region.  The button key is "root/0/0"
    // (surface → column/row → button, index 0 in the single-child template).
    component.hovered_path = vec!["root".into(), "root/0".into()];
    component.hovered_key = Some("root/0".into());
    component.dirty = true;
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();

    // After the hover restyle the button should be 80px wide.
    let tree = component.last_tree.as_ref().unwrap();
    let button = node_by_mesh_key(tree, "root/0");
    assert!(
        button.state.hovered,
        "button should be annotated as hovered"
    );
    assert!(
        button.layout.width >= 79.0,
        "post-restyle layout width should be ~80px, got {}",
        button.layout.width
    );

    // Click at x=60 — inside the restyled 80px bounds but outside the original 40px.
    // The handler must fire, confirming hit testing used the post-restyle bounds.
    component
        .handle_input(
            &theme,
            200,
            60,
            ComponentInput::PointerButton {
                x: 60.0,
                y: 5.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            200,
            60,
            ComponentInput::PointerButton {
                x: 60.0,
                y: 5.0,
                pressed: false,
            },
        )
        .unwrap();
    assert!(
        runtime_bool(&component, "clicked"),
        "click at x=60 should land inside the post-restyle 80px button"
    );
}

/// D-11: Ref and element metrics reflect final post-restyle bounds.
///
/// When a pseudo-state restyle changes an element's computed width, the
/// `refs` / `elements` host values published to the Lua context must report
/// the new width, not the pre-restyle one.
#[test]
fn restyle_metrics_reflect_post_restyle_bounds() {
    let mut component = test_frontend_component(
        r#"
<style>
button {
  width: 40px;
  height: 20px;
}
button:focus {
  width: 80px;
}
</style>
<template>
  <button ref="btn" />
</template>
<script lang="luau">
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);

    // First paint: no focus — button width should be 40px in metrics.
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();
    let width_before = {
        let runtimes = component.runtimes.lock().unwrap();
        let state = runtimes.get(component.id()).unwrap().script_ctx.state();
        state
            .get("refs")
            .and_then(|v| v.get("btn").and_then(|b| b.get("width")).cloned())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32
    };

    // Focus the button and repaint.
    component.focused_key = Some("root/0".into());
    component.dirty = true;
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();
    let width_after = {
        let runtimes = component.runtimes.lock().unwrap();
        let state = runtimes.get(component.id()).unwrap().script_ctx.state();
        state
            .get("refs")
            .and_then(|v| v.get("btn").and_then(|b| b.get("width")).cloned())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32
    };

    assert!(
        (width_before - 40.0).abs() < 2.0,
        "unfocused metrics width should be ~40px, got {width_before}"
    );
    assert!(
        (width_after - 80.0).abs() < 2.0,
        "focused metrics width should be ~80px after restyle, got {width_after}"
    );
}

/// D-13: Accessibility data stays synchronized with focused/checked state
/// and final layout bounds after a restyle.
///
/// When a `:focus` style rule widens a button, the `AccessibilityTree`
/// built from the post-restyle widget tree must report the wider bounds.
#[test]
fn accessibility_data_synchronized_after_restyle() {
    use mesh_core_elements::accessibility::AccessibilityTree;

    let mut component = test_frontend_component(
        r#"
<style>
button {
  width: 60px;
  height: 24px;
}
button:focus {
  width: 120px;
}
</style>
<template>
  <button aria-label="Save" />
</template>
<script lang="luau">
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(300, 80);

    // Paint without focus.
    component.paint(&theme, 300, 80, &mut buffer, 1.0).unwrap();
    let tree_unfocused = component.last_tree.as_ref().unwrap().clone();
    let a11y_unfocused = AccessibilityTree::from_widget_tree(&tree_unfocused);

    // Find the button by its role (Button) in the a11y tree.
    let btn_unfocused_width = a11y_unfocused
        .nodes
        .iter()
        .find(|n| {
            matches!(
                n.info.role,
                mesh_core_elements::accessibility::AccessibilityRole::Button
            )
        })
        .map(|n| n.bounds.width)
        .unwrap_or(0.0);

    // Focus the button and repaint.
    component.focused_key = Some("root/0".into());
    component.dirty = true;
    component.paint(&theme, 300, 80, &mut buffer, 1.0).unwrap();
    let tree_focused = component.last_tree.as_ref().unwrap().clone();
    let a11y_focused = AccessibilityTree::from_widget_tree(&tree_focused);

    // After `:focus` restyle the button bounds must be wider (120px).
    let btn_focused_width = a11y_focused
        .nodes
        .iter()
        .find(|n| {
            matches!(
                n.info.role,
                mesh_core_elements::accessibility::AccessibilityRole::Button
            )
        })
        .map(|n| n.bounds.width)
        .unwrap_or(0.0);

    assert!(
        (btn_unfocused_width - 60.0).abs() < 2.0,
        "unfocused a11y bounds width should be ~60px, got {btn_unfocused_width}"
    );
    assert!(
        btn_focused_width >= 119.0,
        "focused a11y bounds width should be ~120px after restyle, got {btn_focused_width}"
    );
    assert!(
        btn_focused_width > btn_unfocused_width,
        "focused a11y bounds ({btn_focused_width}) must exceed unfocused ({btn_unfocused_width})"
    );

    // Confirm the focused node state flag is set in the widget tree itself
    // (separate from AccessibilityInfo which is populated statically from tag).
    let focused_button = node_by_mesh_key(&tree_focused, "root/0");
    assert!(
        focused_button.state.focused,
        "WidgetNode.state.focused must be true for the focused button"
    );
}
