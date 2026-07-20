use super::*;

fn transition_test_tree(property: mesh_core_elements::TransitionProperties) -> WidgetNode {
    let mut node = event_node("box", "root/0", 0.0, 0.0, 100.0, 20.0, &[]);
    node.computed_style.transitions[0] = mesh_core_elements::TransitionStyle {
        duration_ms: 100,
        properties: property,
        ..mesh_core_elements::TransitionStyle::default()
    };
    root_with(vec![node])
}

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
fn animation_transition_dirty_uses_visual_repaint_for_paint_only_changes() {
    let mut component = test_frontend_component("<template><box /></template>");
    let mut previous = transition_test_tree(mesh_core_elements::TransitionProperties {
        opacity: true,
        ..mesh_core_elements::TransitionProperties::none()
    });
    previous.children[0].computed_style.opacity = 0.1;

    let mut next = previous.clone();
    next.children[0].computed_style.opacity = 0.9;

    component.last_tree = Some(previous);
    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();
    component.apply_style_animations(&mut next);

    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(!requires_tree_rebuild);
    assert!(can_use_retained_path);
    assert!(flags.contains(ComponentDirtyFlags::STYLE));
    assert!(flags.contains(ComponentDirtyFlags::PAINT));
    assert!(!flags.contains(ComponentDirtyFlags::LAYOUT));
}

#[test]
fn animation_transition_dirty_uses_relayout_for_geometry_changes() {
    let mut component = test_frontend_component("<template><box /></template>");
    let mut previous = transition_test_tree(mesh_core_elements::TransitionProperties {
        width: true,
        ..mesh_core_elements::TransitionProperties::none()
    });
    previous.children[0].computed_style.width = mesh_core_elements::Dimension::Px(80.0);

    let mut next = previous.clone();
    next.children[0].computed_style.width = mesh_core_elements::Dimension::Px(140.0);

    component.last_tree = Some(previous);
    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();
    component.apply_style_animations(&mut next);

    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(!requires_tree_rebuild);
    assert!(can_use_retained_path);
    assert!(flags.contains(ComponentDirtyFlags::STYLE));
    assert!(flags.contains(ComponentDirtyFlags::LAYOUT));
    assert!(flags.contains(ComponentDirtyFlags::PAINT));
}

#[test]
fn keyframe_animation_paint_only_rule_uses_visual_repaint() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { animation: pulse 1000ms linear infinite; }
@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut tree = component.build_tree(&theme, 120, 40);
    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();
    component.apply_style_animations(&mut tree);

    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(!requires_tree_rebuild);
    assert!(can_use_retained_path);
    assert!(flags.contains(ComponentDirtyFlags::STYLE));
    assert!(flags.contains(ComponentDirtyFlags::PAINT));
    assert!(!flags.contains(ComponentDirtyFlags::LAYOUT));
}

#[test]
fn keyframe_animation_layout_rule_uses_relayout() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { animation: grow 1000ms linear infinite; }
@keyframes grow {
  0% { width: 40px; }
  100% { width: 80px; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut tree = component.build_tree(&theme, 120, 40);
    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();
    component.apply_style_animations(&mut tree);

    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(!requires_tree_rebuild);
    assert!(can_use_retained_path);
    assert!(flags.contains(ComponentDirtyFlags::STYLE));
    assert!(flags.contains(ComponentDirtyFlags::LAYOUT));
    assert!(flags.contains(ComponentDirtyFlags::PAINT));
}

#[test]
fn keyframe_animation_unknown_rule_stays_conservative() {
    let node = mesh_core_elements::WidgetNode::new("box");
    let style = mesh_core_animation::transition::AnimatableStyle::from_node(&node);
    let rule = mesh_core_animation::keyframes::KeyframeRule {
        name: "unknown".into(),
        stops: vec![
            mesh_core_animation::keyframes::KeyframeStop {
                offset: 0.0,
                style,
                easing: None,
            },
            mesh_core_animation::keyframes::KeyframeStop {
                offset: 1.0,
                style,
                easing: None,
            },
        ],
    };

    assert_eq!(
        crate::shell::component::animation::keyframe_rule_animation_bucket(&rule),
        mesh_core_elements::style::AnimationPropertyBucket::None
    );
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
    component.paint(&theme, 96, 32, &mut buffer, 1.0).unwrap();
    // Every surface content-measures now; the first paint records measured_size
    // and requests one surface-config settle frame. Paint again so it stabilises.
    component.paint(&theme, 96, 32, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 160, 48, &mut buffer, 1.0).unwrap();

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
    component.paint(&theme, 160, 48, &mut buffer, 1.0).unwrap();

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
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 160, 48, &mut buffer, 1.0).unwrap();

    let original_start = Instant::now()
        .checked_sub(Duration::from_millis(400))
        .expect("monotonic instant subtraction");
    component
        .keyframe_animations
        .get_mut("root/0::pulse-a")
        .expect("initial keyframe animation")
        .started_at = original_start;

    let mut tree = component.build_tree(&theme, 160, 48);
    tree.children[0].computed_style.animations[0].name = Some("pulse-b".into());
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
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();

    component
        .keyframe_animations
        .get_mut("root/0::pulse")
        .expect("active infinite keyframe animation")
        .started_at = Instant::now()
        .checked_sub(Duration::from_millis(200))
        .expect("monotonic instant subtraction");
    component.dirty = false;
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();
    component.dirty = false;

    assert!(component.wants_render());
}

#[test]
fn animation_only_tick_uses_scoped_retained_fingerprinting() {
    let mut component = test_frontend_component(
        r#"
<template>
  <row>
    <box class="animated" />
    <box />
  </row>
</template>
<style>
.animated { animation: pulse 1000ms linear infinite; }
@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();

    assert!(component.animation_only_dirty);
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();

    assert!(component.retained_tree.last_update_was_scoped());
}

#[test]
fn smooth_scroll_animation_uses_scoped_retained_fingerprinting() {
    let mut component = test_frontend_component(
        r#"
<template><scroll><box /></scroll></template>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(80, 40);
    component.paint(&theme, 80, 40, &mut buffer, 1.0).unwrap();

    component.scroll_animations.insert(
        "root/0".into(),
        ScrollAnimation {
            start: ScrollOffsetState::default(),
            target: ScrollOffsetState { x: 0.0, y: 80.0 },
            start_time: Instant::now()
                .checked_sub(Duration::from_millis(50))
                .unwrap(),
            duration: Duration::from_millis(200),
        },
    );
    component.invalidate_animation_style_path(ComponentDirtyFlags::VISUAL_REPAINT);
    component.paint(&theme, 80, 40, &mut buffer, 1.0).unwrap();

    assert!(component.retained_tree.last_update_was_scoped());
}

#[test]
fn external_invalidation_cancels_animation_only_retained_scope() {
    let mut component = test_frontend_component(
        r#"
<template><box class="animated" /></template>
<style>
.animated { animation: pulse 1000ms linear infinite; }
@keyframes pulse { 0% { opacity: 0; } 100% { opacity: 1; } }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();
    assert!(component.animation_only_dirty);

    component.invalidate_surface_config();
    assert!(!component.animation_only_dirty);
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();

    assert!(!component.retained_tree.last_update_was_scoped());
}

#[test]
fn animation_scoped_retained_diff_matches_full_pipeline() {
    let source = r#"
<template><row><box class="animated" /><box /></row></template>
<style>
.animated { animation: pulse 1000ms linear infinite; }
@keyframes pulse { 0% { opacity: 0; } 100% { opacity: 1; } }
</style>
"#;
    let mut scoped = test_frontend_component(source);
    let mut full = test_frontend_component(source);
    let theme = default_theme();
    let mut scoped_buffer = PixelBuffer::new(120, 40);
    let mut full_buffer = PixelBuffer::new(120, 40);
    scoped
        .paint(&theme, 120, 40, &mut scoped_buffer, 1.0)
        .unwrap();
    full.paint(&theme, 120, 40, &mut full_buffer, 1.0).unwrap();

    full.animation_only_dirty = false;
    scoped
        .paint(&theme, 120, 40, &mut scoped_buffer, 1.0)
        .unwrap();
    full.paint(&theme, 120, 40, &mut full_buffer, 1.0).unwrap();

    assert!(scoped.retained_tree.last_update_was_scoped());
    assert!(!full.retained_tree.last_update_was_scoped());
    assert_eq!(
        scoped.retained_tree.last_dirty(),
        full.retained_tree.last_dirty()
    );
    assert_eq!(
        scoped.retained_tree.dirty_node_ids(),
        full.retained_tree.dirty_node_ids()
    );
}

// cargo test -p mesh-core-shell --release -- animation_scoped_retained_end_to_end_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only end-to-end animation retained-scope benchmark"]
fn animation_scoped_retained_end_to_end_benchmark() {
    let mut source = String::from("<template><row><box class=\"animated\" />");
    for _ in 0..1_024 {
        source.push_str("<box />");
    }
    source.push_str(
        r#"</row></template>
<style>
.animated { animation: pulse 1000ms linear infinite; }
@keyframes pulse { 0% { opacity: 0; } 100% { opacity: 1; } }
</style>"#,
    );

    let mut scoped = test_frontend_component(&source);
    let mut full = test_frontend_component(&source);
    let theme = default_theme();
    let mut scoped_buffer = PixelBuffer::new(64, 16);
    let mut full_buffer = PixelBuffer::new(64, 16);
    scoped
        .paint(&theme, 64, 16, &mut scoped_buffer, 1.0)
        .unwrap();
    full.paint(&theme, 64, 16, &mut full_buffer, 1.0).unwrap();

    let iterations = 250;
    let mut scoped_time = Duration::ZERO;
    let mut full_time = Duration::ZERO;
    for iteration in 0..iterations {
        if iteration % 2 == 0 {
            let started = Instant::now();
            scoped
                .paint(&theme, 64, 16, &mut scoped_buffer, 1.0)
                .unwrap();
            scoped_time += started.elapsed();

            full.animation_only_dirty = false;
            let started = Instant::now();
            full.paint(&theme, 64, 16, &mut full_buffer, 1.0).unwrap();
            full_time += started.elapsed();
        } else {
            full.animation_only_dirty = false;
            let started = Instant::now();
            full.paint(&theme, 64, 16, &mut full_buffer, 1.0).unwrap();
            full_time += started.elapsed();

            let started = Instant::now();
            scoped
                .paint(&theme, 64, 16, &mut scoped_buffer, 1.0)
                .unwrap();
            scoped_time += started.elapsed();
        }
    }

    let speedup = full_time.as_secs_f64() / scoped_time.as_secs_f64();
    eprintln!(
        "end-to-end animation paints over {iterations} one-node-animated 1,026-node frames: full retained fingerprints {full_time:?}; scoped {scoped_time:?}; ratio {speedup:.3}x"
    );
    eprintln!("MESH_PERF metric=animation_frame_speedup value={speedup:.6}");
    assert!(scoped_time < full_time);
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
  width: 10px;
  height: 10px;
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();

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
  animation-duration: var(--animation-duration-fastest);
  width: 10px;
  height: 10px;
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
    component.paint(&theme, 120, 40, &mut buffer, 1.0).unwrap();

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert!(diagnostics.error_count() >= 1);
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Error(message)
            if message.contains("animation.duration.fastest")
    ));
}
