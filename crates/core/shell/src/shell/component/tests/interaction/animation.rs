use super::*;

#[test]
fn navigation_volume_slider_handler_error_records_diagnostic_and_keeps_last_tree() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
function onVolumeChange(value)
    error("slider handler error")
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "slider",
        "root/0",
        0.0,
        0.0,
        100.0,
        20.0,
        &[("change", "onVolumeChange")],
    )]));
    component.dirty = false;

    let theme = default_theme();
    let requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 50.0,
                y: 10.0,
                pressed: true,
            },
        )
        .unwrap();

    assert!(requests.is_empty());
    assert!(
        component.last_tree.is_some(),
        "last successfully rendered tree should remain available after slider handler error"
    );
    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 1);
}

#[test]
fn handler_without_state_change_does_not_force_rebuild() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button onclick={onClick}>{label}</button>
</template>

<script lang="luau">
label = "Ready"

function onClick()
    label = "Ready"
end
</script>
"#,
    );
    component.clear_runtime_dirty_states();
    component.dirty = false;

    component.call_namespaced_handler("onClick", &[]).unwrap();

    assert!(!component.wants_render());
}

#[test]
fn handler_state_change_rebuilds_next_paint() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button onclick={onClick}>{label}</button>
</template>

<script lang="luau">
label = "Ready"

function onClick()
    label = "Clicked"
end
</script>
"#,
    );
    component.clear_runtime_dirty_states();
    component.dirty = false;

    component.call_namespaced_handler("onClick", &[]).unwrap();
    assert!(component.wants_render());

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(96, 32);
    component.paint(&theme, 96, 32, &mut buffer).unwrap();
    component.dirty = false;

    assert!(
        !component
            .runtimes
            .lock()
            .unwrap()
            .get(component.id())
            .unwrap()
            .script_ctx
            .state()
            .is_dirty()
    );
    assert!(!component.wants_render());
}

#[test]
fn keyframe_animation_continues_across_rebuild() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button class="panel" onclick={onClick}>{label}</button>
</template>

<script lang="luau">
label = "Ready"

function onClick()
    label = "Updated"
end
</script>

<style>
.panel {
  animation: pulse 1000ms linear infinite;
}

@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 48);
    component.paint(&theme, 160, 48, &mut buffer).unwrap();

    let key = "root/0::pulse".to_string();
    let preserved_start = Instant::now()
        .checked_sub(Duration::from_millis(400))
        .expect("monotonic instant subtraction");
    component
        .keyframe_animations
        .get_mut(&key)
        .expect("active keyframe animation")
        .started_at = preserved_start;

    component.call_namespaced_handler("onClick", &[]).unwrap();
    component.paint(&theme, 160, 48, &mut buffer).unwrap();

    assert_eq!(
        component
            .keyframe_animations
            .get(&key)
            .expect("preserved keyframe animation")
            .started_at,
        preserved_start
    );
}

#[test]
fn navigation_bar_keyframe_animation_continues_across_rebuild() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 31,
                "muted": false
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(420, 80);
    component.paint(&theme, 420, 80, &mut buffer).unwrap();
    let first_tree = component
        .last_tree
        .as_ref()
        .expect("initial navigation tree");
    let first_status_accent =
        first_node_with_attr(first_tree, "class", "status-accent").expect("status accent node");
    assert_eq!(
        first_status_accent.computed_style.animation.name.as_deref(),
        Some("status-pulse")
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 64,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, 420, 80, &mut buffer).unwrap();
    let rebuilt_tree = component
        .last_tree
        .as_ref()
        .expect("rebuilt navigation tree");
    let rebuilt_status_accent = first_node_with_attr(rebuilt_tree, "class", "status-accent")
        .expect("rebuilt status accent node");

    assert_eq!(
        rebuilt_status_accent
            .computed_style
            .animation
            .name
            .as_deref(),
        Some("status-pulse")
    );
}

#[test]
fn keyframe_animation_finite_completion_stops_render_requests() {
    let mut component = test_frontend_component(
        r#"
<template>
  <box class="panel" />
</template>

<style>
.panel {
  animation: pulse 50ms linear 1 forwards;
}

@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component.paint(&theme, 120, 40, &mut buffer).unwrap();
    assert!(component.wants_render());

    let key = "root/0::pulse".to_string();
    component
        .keyframe_animations
        .get_mut(&key)
        .expect("active finite keyframe animation")
        .started_at = Instant::now()
        .checked_sub(Duration::from_millis(200))
        .expect("monotonic instant subtraction");
    component.dirty = false;
    component.paint(&theme, 120, 40, &mut buffer).unwrap();
    component.dirty = false;

    assert!(!component.wants_render());
}

#[test]
fn keyframe_animation_name_change_restarts_timeline() {
    let mut component = test_frontend_component(
        r#"
<template>
  <button class="panel">Pulse</button>
</template>

<style>
.panel {
  animation-name: pulse-a;
  animation-duration: 1000ms;
}

@keyframes pulse-a {
  0% { opacity: 0; }
  100% { opacity: 1; }
}

@keyframes pulse-b {
  0% { opacity: 1; }
  100% { opacity: 0; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 48);
    component.paint(&theme, 160, 48, &mut buffer).unwrap();

    let original_start = Instant::now()
        .checked_sub(Duration::from_millis(400))
        .expect("monotonic instant subtraction");
    component
        .keyframe_animations
        .get_mut("root/0::pulse-a")
        .expect("initial keyframe animation")
        .started_at = original_start;

    let mut tree = component.build_tree(&theme, 160, 48);
    tree.children[0].computed_style.animation.name = Some("pulse-b".into());
    component.apply_style_animations(&mut tree);
    component.last_tree = Some(tree);

    assert!(
        !component
            .keyframe_animations
            .contains_key("root/0::pulse-a")
    );
    assert!(
        component
            .keyframe_animations
            .contains_key("root/0::pulse-b")
    );
    assert_ne!(
        component
            .keyframe_animations
            .get("root/0::pulse-b")
            .expect("replacement keyframe animation")
            .started_at,
        original_start
    );
}

#[test]
fn keyframe_animation_infinite_keeps_render_requests_active() {
    let mut component = test_frontend_component(
        r#"
<template>
  <box class="panel" />
</template>

<style>
.panel {
  animation: pulse 50ms linear infinite;
}

@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component.paint(&theme, 120, 40, &mut buffer).unwrap();

    component
        .keyframe_animations
        .get_mut("root/0::pulse")
        .expect("active infinite keyframe animation")
        .started_at = Instant::now()
        .checked_sub(Duration::from_millis(200))
        .expect("monotonic instant subtraction");
    component.dirty = false;
    component.paint(&theme, 120, 40, &mut buffer).unwrap();
    component.dirty = false;

    assert!(component.wants_render());
}

#[test]
fn keyframe_animation_missing_name_records_diagnostic() {
    let mut component = test_frontend_component(
        r#"
<template>
  <box class="panel" />
</template>

<style>
.panel {
  animation-name: pulse-missing;
  animation-duration: 120ms;
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component.paint(&theme, 120, 40, &mut buffer).unwrap();

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 1);
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Error(message)
            if message.contains("unresolved animation 'pulse-missing'")
    ));
}

#[test]
fn animation_token_runtime_diagnostic_reaches_component() {
    let mut component = test_frontend_component(
        r#"
<template>
  <box class="panel" />
</template>

<style>
.panel {
  animation-name: pulse;
  animation-duration: token(animation.duration.fastest);
}

@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component.paint(&theme, 120, 40, &mut buffer).unwrap();

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert!(diagnostics.error_count() >= 1);
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Error(message)
            if message.contains("animation.duration.fastest")
    ));
}
