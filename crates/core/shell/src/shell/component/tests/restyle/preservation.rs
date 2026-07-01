use super::*;

#[test]
fn state_preservation_restyle_service_payload_survives_hover_restyle() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button />
</template>
<script lang="luau">
-- Track whenever a reactive global is updated to detect accidental wipes.
vol_pct = -1
function render()
    -- Read directly from the service state table if it exists.
    if __mesh_svc_audio then
        vol_pct = __mesh_svc_audio.percent or -1
    end
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);

    // First paint — no service payload yet.
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();

    // Apply a service payload directly to the ScriptContext, simulating a
    // backend service emit reaching the frontend runtime.
    {
        let mut runtimes = component.runtimes.lock().unwrap();
        let runtime = runtimes.get_mut(component.id()).unwrap();
        runtime
            .script_ctx
            .apply_service_payload("audio", &serde_json::json!({ "percent": 72 }));
        // Mark render hooks pending so render fires on next paint.
    }
    component.render_hooks_pending = true;
    component.dirty = true;

    // First paint with the service payload — render fires, vol_pct == 72.
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();
    let pct_after_payload = runtime_number(&component, "vol_pct");
    assert!(
        (pct_after_payload - 72.0).abs() < 0.1,
        "vol_pct should be 72 after service payload applied, got {pct_after_payload}"
    );

    // Trigger a pseudo-state restyle by setting hover (no service re-emit).
    component.hovered_key = Some("root/0".into());
    component.hovered_path = vec!["root".into(), "root/0".into()];
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();

    // vol_pct must still reflect the last service update, not be wiped.
    let pct_after_hover_restyle = runtime_number(&component, "vol_pct");
    assert!(
        (pct_after_hover_restyle - 72.0).abs() < 0.1,
        "service payload must survive a hover-triggered restyle; vol_pct={pct_after_hover_restyle}"
    );

    // The hovered button should show :hover state in the tree.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        node_by_mesh_key(tree, "root/0").state.hovered,
        "button must be marked hovered after restyle"
    );
}

/// Pseudo-state restyles (hover, focus) must not increment the runtime
/// instance count — the same `EmbeddedFrontendRuntime` must be reused.
/// Reusing the runtime also implicitly preserves all Lua global state
/// (reactive variables, imported service proxies, etc.).
#[test]
fn state_preservation_restyle_does_not_reinitialize_runtime() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button />
</template>
<script lang="luau">
init_count = 0
init_count = init_count + 1
</script>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);

    // First paint — runtime is initialized, init_count == 1.
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();
    let count_after_first = runtime_number(&component, "init_count");
    let runtime_instances_after_first = component.runtimes.lock().unwrap().len();
    assert_eq!(
        count_after_first as u32, 1,
        "init_count should be 1 after first paint"
    );
    assert_eq!(
        runtime_instances_after_first, 1,
        "should have exactly 1 runtime after first paint"
    );

    // Trigger a pseudo-state restyle by focusing.
    component.focused_key = Some("root/0".into());
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();

    let count_after_focus = runtime_number(&component, "init_count");
    let runtime_instances_after_focus = component.runtimes.lock().unwrap().len();

    // init_count must still be 1 — the top-level Luau block must not run again.
    assert_eq!(
        count_after_focus as u32, 1,
        "pseudo-state restyle must not re-execute the top-level Luau block (init_count={count_after_focus})"
    );
    // Runtime instance count must not grow.
    assert_eq!(
        runtime_instances_after_focus, runtime_instances_after_first,
        "pseudo-state restyle must reuse the existing runtime (expected {runtime_instances_after_first}, got {runtime_instances_after_focus})"
    );
}

/// Input, slider, and checked state must be preserved through a pseudo-state
/// (focus) restyle — all three shell-side maps must survive unchanged.
/// Scroll offset maps are also preserved; the annotated `_mesh_scroll_y`
/// value is clamped by `annotate_overflow_tree` to the actual overflow range,
/// so preservation of the raw map entry is verified instead.
#[test]
fn state_preservation_restyle_user_input_state_survives_focus_restyle() {
    let mut component = test_frontend_component(
        r#"
<style>
scroll {
  height: 20px;
  overflow-y: auto;
}
text {
  height: 100px;
}
</style>
<template>
  <column>
    <input value="initial" />
    <slider min="0" max="100" value="25" />
    <checkbox checked="false" />
    <scroll><text>scrollable content long enough to overflow</text></scroll>
  </column>
</template>
<script lang="luau">
</script>
"#,
    );
    // Seed shell-side interaction state maps directly.
    component
        .input_values
        .insert("root/0/0".into(), "typed-text".into());
    component.slider_values.insert("root/0/1".into(), 88.0);
    component.checked_values.insert("root/0/2".into(), true);
    component
        .scroll_offsets
        .insert("root/0/3".into(), ScrollOffsetState { x: 0.0, y: 10.0 });

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 300);

    // First paint to establish baseline.
    component.paint(&theme, 240, 300, &mut buffer, 1.0).unwrap();

    // Trigger a focus-driven pseudo-state restyle.
    component.focused_key = Some("root/0/0".into());
    component.dirty = true;
    component.paint(&theme, 240, 300, &mut buffer, 1.0).unwrap();

    let tree = component.last_tree.as_ref().unwrap();

    // Input value must survive the restyle.
    assert_eq!(
        node_by_mesh_key(tree, "root/0/0")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("typed-text"),
        "input value must survive focus restyle"
    );

    // Slider value must survive.
    assert_eq!(
        node_by_mesh_key(tree, "root/0/1")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("88.00"),
        "slider value must survive focus restyle"
    );

    // Checked state must survive.
    assert!(
        node_by_mesh_key(tree, "root/0/2").state.checked,
        "checked state must survive focus restyle"
    );

    // Scroll offset raw map entry must survive (the annotated _mesh_scroll_y is
    // clamp-bounded by annotate_overflow_tree to the actual overflow range).
    assert!(
        component.scroll_offsets.contains_key("root/0/3"),
        "scroll_offsets map must retain the entry for root/0/3 across focus restyle"
    );
}

// -----------------------------------------------------------------------
// 09-04-02: Clear invalid interaction targets deterministically
// -----------------------------------------------------------------------

/// When a conditionally rendered node (removed from the tree by restyle) was
/// the hovered target, the hover state must be cleared deterministically after
/// the next paint. Valid siblings must retain their state.
#[test]
fn restyle_state_cleanup_hover_cleared_when_node_removed() {
    let mut component = test_frontend_component(
        r#"
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

    // First paint to establish the tree structure.
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();

    // Simulate hovering the second button.
    component.hovered_key = Some("root/0/1".into());
    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/1".into()];
    component.hover_start = Some(std::time::Instant::now());
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();

    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        node_by_mesh_key(tree, "root/0/1").state.hovered,
        "second button must be hovered before removal"
    );
    assert!(
        component.hovered_key.is_some(),
        "hovered_key must be set before node removal"
    );

    // Now simulate node removal: pretend the second button is gone by manually
    // removing its key from the tree. We do this by injecting a component
    // that only has one button, so "root/0/1" will not appear in the final tree.
    let component2 = test_frontend_component(
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
    // Transplant the hovered state into the new component to test cleanup.
    let mut component = component2;
    component.hovered_key = Some("root/0/1".into()); // stale key
    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/1".into()];
    component.hover_start = Some(std::time::Instant::now());
    component.dirty = true;
    component.paint(&theme, 240, 80, &mut buffer, 1.0).unwrap();

    // After the paint, the stale hovered_key must be cleared.
    assert!(
        component.hovered_key.is_none(),
        "hovered_key must be cleared after the hovered node is removed from the tree"
    );
    assert!(
        component.hovered_path.is_empty(),
        "hovered_path must be cleared when hovered node is removed"
    );
    assert!(
        component.hover_start.is_none(),
        "hover_start must be cleared when hovered node is removed"
    );

    // The remaining sibling must not be affected.
    let tree = component.last_tree.as_ref().unwrap();
    assert!(
        !node_by_mesh_key(tree, "root/0/0").state.hovered,
        "remaining sibling must not inherit stale hover state"
    );
}
