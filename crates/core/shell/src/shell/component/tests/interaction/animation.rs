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
                button: 0x110,
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
fn completed_transition_keeps_final_sample_in_retained_dirty_roots() {
    use mesh_core_animation::transition::AnimatableStyle;
    let mut component = test_frontend_component("<template><box /></template>");
    let mut tree = transition_test_tree(mesh_core_elements::TransitionProperties {
        opacity: true,
        ..mesh_core_elements::TransitionProperties::none()
    });
    let child = &mut tree.children[0];
    child.computed_style.opacity = 0.1;
    let previous = AnimatableStyle::from_node(child);
    child.computed_style.opacity = 0.9;
    let id = child.id;
    assert!(component.transitions.step_node(
        id,
        child,
        previous,
        std::time::Instant::now() - std::time::Duration::from_secs(1)
    ));
    component.retained_tree.update(&tree);
    let before = component.retained_tree.generation();
    let dirty = component.apply_style_animations_with_previous(
        &mut tree,
        &HashMap::new(),
        &component.surface_css_props(),
    );
    assert!(!component.transitions.has_active(std::time::Instant::now()));
    assert_eq!(tree.children[0].computed_style.opacity, 0.9);
    assert!(
        dirty.contains(&id),
        "the completed animation still changed its last sample"
    );
    component
        .retained_tree
        .update_for_dirty_roots(&tree, &dirty);
    assert!(component.retained_tree.generation() > before);
    assert_eq!(component.retained_tree.render_dirty().opacity, 1);
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
fn component_keyframe_easing_reaches_shell_animation_state() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { animation: pulse 1000ms linear infinite; }
@keyframes pulse {
  0% { opacity: 0; animation-timing-function: ease-in; }
  50% { opacity: 0.5; animation-timing-function: steps(4, jump-start); }
  100% { opacity: 1; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    let rule = &component
        .keyframe_rules
        .values()
        .next()
        .expect("lowered shell keyframe rule")
        .rule;
    assert!(matches!(
        rule.stops[0].easing,
        Some(mesh_core_animation::Easing::EaseIn)
    ));
    assert!(matches!(
        rule.stops[1].easing,
        Some(mesh_core_animation::Easing::Steps(
            4,
            mesh_core_elements::StepPosition::JumpStart
        ))
    ));
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
                properties: mesh_core_elements::TransitionProperties::of_property("width"),
            },
            mesh_core_animation::keyframes::KeyframeStop {
                offset: 1.0,
                style,
                easing: None,
                properties: mesh_core_elements::TransitionProperties::of_property("width"),
            },
        ],
    };

    assert_eq!(
        crate::shell::component::animation::keyframe_rule_animation_bucket(&rule, &style),
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(96, 32), &mut buffer, 1.0)
        .unwrap();
    // Every surface content-measures now; the first paint records measured_size
    // and requests one surface-config settle frame. Paint again so it stabilises.
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(96, 32), &mut buffer, 1.0)
        .unwrap();
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
        .unwrap();

    let key = *component
        .keyframe_animations
        .keys()
        .next()
        .expect("active keyframe animation");
    let preserved_start = Instant::now()
        .checked_sub(Duration::from_millis(400))
        .expect("monotonic instant subtraction");
    component
        .keyframe_animations
        .get_mut(&key)
        .expect("active keyframe animation")
        .started_at = preserved_start;

    component.call_namespaced_handler("onClick", &[]).unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
        .unwrap();

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
fn keyframe_pause_resume_keeps_progress_across_an_iteration_boundary() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel {
  animation: pulse 1000ms linear infinite alternate running;
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    let key = *component
        .keyframe_animations
        .keys()
        .next()
        .expect("active keyframe animation");
    let timeline_start = Instant::now()
        .checked_sub(Duration::from_millis(1250))
        .expect("monotonic instant subtraction");
    component
        .keyframe_animations
        .get_mut(&key)
        .expect("active keyframe animation")
        .started_at = timeline_start;

    let mut paused_tree = component.build_tree(&theme, 120, 40);
    paused_tree.children[0].computed_style.animations[0].play_state =
        mesh_core_elements::style::AnimationPlayState::Paused;
    component.apply_style_animations(&mut paused_tree);
    let paused_opacity = paused_tree.children[0].computed_style.opacity;
    assert!(
        component
            .keyframe_animations
            .get(&key)
            .and_then(|animation| animation.paused_at)
            .is_some()
    );

    let synthetic_pause_start = Instant::now()
        .checked_sub(Duration::from_secs(30))
        .expect("monotonic instant subtraction");
    let synthetic_timeline_start = synthetic_pause_start
        .checked_sub(Duration::from_millis(1250))
        .expect("monotonic instant subtraction");
    let paused_animation = component
        .keyframe_animations
        .get_mut(&key)
        .expect("paused keyframe animation");
    paused_animation.started_at = synthetic_timeline_start;
    paused_animation.paused_at = Some(synthetic_pause_start);

    let mut resumed_tree = component.build_tree(&theme, 120, 40);
    resumed_tree.children[0].computed_style.animations[0].play_state =
        mesh_core_elements::style::AnimationPlayState::Running;
    component.apply_style_animations(&mut resumed_tree);
    let resumed = component
        .keyframe_animations
        .get(&key)
        .expect("resumed keyframe animation");

    assert!(
        resumed
            .started_at
            .saturating_duration_since(synthetic_timeline_start)
            >= Duration::from_secs(29)
    );
    assert_eq!(resumed.paused_at, None);
    assert!((resumed_tree.children[0].computed_style.opacity - paused_opacity).abs() < 0.01);
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    assert!(component.wants_render());

    let key = *component
        .keyframe_animations
        .keys()
        .next()
        .expect("active keyframe animation");
    component
        .keyframe_animations
        .get_mut(&key)
        .expect("active finite keyframe animation")
        .started_at = Instant::now()
        .checked_sub(Duration::from_millis(200))
        .expect("monotonic instant subtraction");
    component.dirty = false;
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    component.dirty = false;

    assert!(!component.wants_render());
    assert_eq!(
        component
            .keyframe_animation_lifecycles
            .get(&(key.node_id, key.list_index)),
        Some(&mesh_core_animation::AnimationLifecycle::Completed)
    );
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
        .unwrap();

    let original_start = Instant::now()
        .checked_sub(Duration::from_millis(400))
        .expect("monotonic instant subtraction");
    let original_id = *component
        .keyframe_animations
        .keys()
        .next()
        .expect("initial keyframe animation");
    component
        .keyframe_animations
        .get_mut(&original_id)
        .expect("initial keyframe animation")
        .started_at = original_start;

    let mut tree = component.build_tree(&theme, 160, 48);
    tree.children[0].computed_style.animations[0].name = Some("pulse-b".into());
    component.apply_style_animations(&mut tree);
    component.last_tree = Some(tree);

    assert_eq!(
        component
            .keyframe_animation_lifecycles
            .get(&(original_id.node_id, original_id.list_index)),
        Some(&mesh_core_animation::AnimationLifecycle::Replaced)
    );
    assert!(!component.keyframe_animations.contains_key(&original_id));
    let replacement_id = *component
        .keyframe_animations
        .keys()
        .next()
        .expect("replacement keyframe animation");
    assert_ne!(replacement_id, original_id);
    assert_eq!(replacement_id.node_id, original_id.node_id);
    assert_eq!(replacement_id.list_index, original_id.list_index);
    assert_ne!(
        replacement_id.declaration_generation,
        original_id.declaration_generation
    );
    assert_ne!(
        component
            .keyframe_animations
            .get(&replacement_id)
            .expect("replacement keyframe animation")
            .started_at,
        original_start
    );
}

#[test]
fn duplicate_keyframe_names_use_distinct_slots_and_replace_only_changed_declarations() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { animation: pulse 1000ms linear infinite, pulse 2000ms linear infinite; }
@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    assert_eq!(component.keyframe_animations.len(), 2);
    let mut initial = component
        .keyframe_animations
        .keys()
        .copied()
        .collect::<Vec<_>>();
    initial.sort_by_key(|id| id.list_index);
    assert_eq!(initial[0].list_index, 0);
    assert_eq!(initial[1].list_index, 1);
    let first_start = Instant::now()
        .checked_sub(Duration::from_millis(300))
        .expect("monotonic instant subtraction");
    for id in &initial {
        component
            .keyframe_animations
            .get_mut(id)
            .expect("initial animation")
            .started_at = first_start;
    }

    let mut tree = component.build_tree(&theme, 120, 40);
    tree.children[0].computed_style.animations[1].duration_ms = 3000;
    component.apply_style_animations(&mut tree);

    assert_eq!(component.keyframe_animations.len(), 2);
    let replacement = component
        .keyframe_animations
        .keys()
        .find(|id| id.list_index == 1)
        .copied()
        .expect("replacement animation");
    assert_ne!(
        replacement.declaration_generation,
        initial[1].declaration_generation
    );
    assert_eq!(
        component
            .keyframe_animation_lifecycles
            .get(&(replacement.node_id, replacement.list_index)),
        Some(&mesh_core_animation::AnimationLifecycle::Replaced)
    );
    let first = component
        .keyframe_animations
        .keys()
        .find(|id| id.list_index == 0)
        .copied()
        .expect("unchanged animation");
    assert_eq!(first, initial[0]);
    assert_eq!(
        component
            .keyframe_animations
            .get(&first)
            .expect("unchanged timeline")
            .started_at,
        first_start
    );
}

#[test]
fn keyframe_disappearance_cancels_the_instance_and_timeline() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { animation: pulse 1000ms linear infinite; }
@keyframes pulse { 0% { opacity: 0; } 100% { opacity: 1; } }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    let id = *component
        .keyframe_animations
        .keys()
        .next()
        .expect("active animation");

    let mut tree = component.build_tree(&theme, 120, 40);
    tree.children[0].computed_style.animations.clear();
    component.apply_style_animations(&mut tree);

    assert!(component.keyframe_animations.is_empty());
    assert!(component.keyframe_rules.is_empty());
    assert_eq!(
        component
            .keyframe_animation_lifecycles
            .get(&(id.node_id, id.list_index)),
        Some(&mesh_core_animation::AnimationLifecycle::Cancelled)
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    let key = *component
        .keyframe_animations
        .keys()
        .next()
        .expect("active keyframe animation");
    component
        .keyframe_animations
        .get_mut(&key)
        .expect("active infinite keyframe animation")
        .started_at = Instant::now()
        .checked_sub(Duration::from_millis(200))
        .expect("monotonic instant subtraction");
    component.dirty = false;
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    component.dirty = false;

    assert!(component.wants_render());
}

fn panel_opacity(component: &FrontendSurfaceComponent) -> f32 {
    fn find(node: &WidgetNode) -> Option<f32> {
        if node.class_tokens().iter().any(|class| class == "panel") {
            return Some(node.computed_style.opacity);
        }
        node.children.iter().find_map(find)
    }
    find(component.last_tree.as_ref().expect("painted tree")).expect("panel node")
}

/// Paint one animation-only frame with the keyframe timeline at `elapsed`.
fn paint_keyframe_frame_at(component: &mut FrontendSurfaceComponent, elapsed: Duration) {
    let started_at = Instant::now()
        .checked_sub(elapsed)
        .expect("monotonic instant subtraction");
    for animation in component.keyframe_animations.values_mut() {
        animation.started_at = started_at;
    }
    component.dirty = false;
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(
            &default_theme(),
            SurfaceExtent::unpadded(120, 40),
            &mut buffer,
            1.0,
        )
        .unwrap();
}

/// Animation-only frames reuse the previous computed style. Sampling from it
/// would start each frame from the last animated value instead of the
/// authored one, so a missing 100% stop drifts toward the start.
#[test]
fn keyframes_sample_the_authored_value_across_animation_only_frames() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { opacity: 0.8; animation: fade 1000ms linear infinite; }
@keyframes fade {
  0% { opacity: 0; }
}
</style>
"#,
    );
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(
            &default_theme(),
            SurfaceExtent::unpadded(120, 40),
            &mut buffer,
            1.0,
        )
        .unwrap();

    paint_keyframe_frame_at(&mut component, Duration::from_millis(250));
    paint_keyframe_frame_at(&mut component, Duration::from_millis(500));

    assert!(
        (panel_opacity(&component) - 0.4).abs() < 0.02,
        "{}",
        panel_opacity(&component)
    );
}

#[test]
fn a_finished_keyframe_animation_without_fill_restores_the_authored_value() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { opacity: 0.8; animation: fade 1000ms linear 1; }
@keyframes fade {
  0% { opacity: 0; }
  100% { opacity: 0.2; }
}
</style>
"#,
    );
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(
            &default_theme(),
            SurfaceExtent::unpadded(120, 40),
            &mut buffer,
            1.0,
        )
        .unwrap();
    paint_keyframe_frame_at(&mut component, Duration::from_millis(500));
    assert!((panel_opacity(&component) - 0.1).abs() < 0.02);

    paint_keyframe_frame_at(&mut component, Duration::from_millis(2000));

    assert_eq!(panel_opacity(&component), 0.8);
}

#[test]
fn a_keyframe_animation_leaves_transitions_on_other_properties_running() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<style>
@keyframes fade {
  0% { opacity: 0; }
}
</style>
"#,
    );
    let mut previous = transition_test_tree(mesh_core_elements::TransitionProperties {
        background_color: true,
        opacity: true,
        ..mesh_core_elements::TransitionProperties::none()
    });
    previous.children[0].computed_style.animations =
        vec![mesh_core_elements::style::AnimationStyle {
            name: Some("fade".into()),
            duration_ms: 1000,
            iteration_count: mesh_core_elements::style::AnimationIterationCount::Infinite,
            ..mesh_core_elements::style::AnimationStyle::default()
        }];
    previous.children[0].computed_style.background_color = mesh_core_elements::style::Color::BLACK;
    let mut next = previous.clone();
    next.children[0].computed_style.background_color = mesh_core_elements::style::Color::WHITE;
    let id = next.children[0].id;

    component.last_tree = Some(previous);
    component.apply_style_animations(&mut next);

    let transition = component
        .transitions
        .unfinished(id, Instant::now())
        .next()
        .expect("background transition");
    assert!(transition.source.properties.background_color);
    assert!(!component.keyframe_animations.is_empty());
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    assert!(component.animation_only_dirty);
    component.set_profiling_enabled(true);
    component.take_profiling_records();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    assert!(component.retained_tree.last_update_was_scoped());
    assert!(component.take_profiling_records().iter().any(|record| {
        record.stage == mesh_core_debug::ProfilingStage::StyleRestyle
            && record.trigger_kind.as_deref() == Some("waste:empty_restyle_avoided")
    }));
}

/// A surface can be painted twice inside one frame (the render loop re-runs a
/// pass to honour a measured size). The animation pass compares against a
/// baseline captured during that same pass, so on the second pass a still
/// running animation reports no change even though the tree it is painting has
/// moved since the retained tree last recorded it. Leaving the node out of the
/// retained dirty roots holds the retained generation still while the tree
/// changes, which hands the display list one generation for two different
/// trees. A node whose animation is still running is dirty by definition.
///
/// The transition delay makes that pass-local "nothing changed" deterministic:
/// the transition is unmistakably live, but its value is pinned to the start
/// for the whole delay window.
#[test]
fn a_live_transition_stays_dirty_while_its_delay_pins_the_value() {
    let mut component = test_frontend_component("<template><box /></template>");
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(80, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(80, 40), &mut buffer, 1.0)
        .unwrap();

    let mut node = event_node("box", "root/0", 0.0, 0.0, 80.0, 20.0, &[]);
    node.computed_style.transitions[0] = mesh_core_elements::TransitionStyle {
        duration_ms: 200,
        delay_ms: 60_000,
        properties: mesh_core_elements::TransitionProperties {
            background_color: true,
            ..mesh_core_elements::TransitionProperties::none()
        },
        ..mesh_core_elements::TransitionStyle::default()
    };
    node.computed_style.background_color = mesh_core_elements::style::Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let node_id = node.id;
    let mut tree = root_with(vec![node]);
    let surface_css_props = component.surface_css_props();

    // First pass: the baseline differs from the node's target, so a transition
    // starts and immediately parks on its start value for the delay window.
    let mut baseline = crate::shell::component::animation::collect_visual_styles(&tree);
    if let Some(style) = baseline.get_mut(&node_id) {
        style.background_color = mesh_core_elements::style::Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        };
    }
    component.apply_style_animations_with_previous(&mut tree, &baseline, &surface_css_props);

    // What pass one left on screen: the transition's start value, pinned by the
    // delay. This is the baseline the next pass in the same frame compares to.
    let held = crate::shell::component::animation::collect_visual_styles(&tree);

    // Every pass re-resolves the authored style before animating, so the
    // transition target is still the authored colour rather than what pass one
    // painted.
    tree.children[0].computed_style.background_color = mesh_core_elements::style::Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };

    // Second pass, the one a re-run frame performs: the delay keeps the value
    // pinned, so nothing "changed" during this pass even though the transition
    // is still running.
    let dirty =
        component.apply_style_animations_with_previous(&mut tree, &held, &surface_css_props);

    assert!(
        dirty.contains(&node_id),
        "a running transition must keep its node in the retained dirty roots \
         even when the pass-local baseline is unchanged"
    );
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(80, 40), &mut buffer, 1.0)
        .unwrap();

    component.scroll_animations.insert(
        runtime_node_id_for_key("root/0"),
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(80, 40), &mut buffer, 1.0)
        .unwrap();

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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    assert!(component.animation_only_dirty);

    component.invalidate_surface_config();
    assert!(!component.animation_only_dirty);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

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
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(120, 40),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint_raster(
        &theme,
        SurfaceExtent::unpadded(120, 40),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

    // The first paints leave measurement dirt behind. Settle it so the frame
    // that follows is driven by the animation alone.
    scoped
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(120, 40),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint_raster(
        &theme,
        SurfaceExtent::unpadded(120, 40),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

    // Mark only the scoped side as an animation tick, as the frame timer does,
    // and advance both timelines together: a sample at the same instant changes
    // nothing, so the scoped path needs a visible animation step to run.
    scoped.invalidate_animation_style_path(ComponentDirtyFlags::VISUAL_REPAINT);
    full.animation_only_dirty = false;
    paint_keyframe_frame_at(&mut scoped, Duration::from_millis(250));
    paint_keyframe_frame_at(&mut full, Duration::from_millis(250));

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
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(64, 16),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint_raster(
        &theme,
        SurfaceExtent::unpadded(64, 16),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

    let iterations = 250;
    let mut scoped_time = Duration::ZERO;
    let mut full_time = Duration::ZERO;
    for iteration in 0..iterations {
        if iteration % 2 == 0 {
            let started = Instant::now();
            scoped
                .paint_raster(
                    &theme,
                    SurfaceExtent::unpadded(64, 16),
                    &mut scoped_buffer,
                    1.0,
                )
                .unwrap();
            scoped_time += started.elapsed();

            full.animation_only_dirty = false;
            let started = Instant::now();
            full.paint_raster(
                &theme,
                SurfaceExtent::unpadded(64, 16),
                &mut full_buffer,
                1.0,
            )
            .unwrap();
            full_time += started.elapsed();
        } else {
            full.animation_only_dirty = false;
            let started = Instant::now();
            full.paint_raster(
                &theme,
                SurfaceExtent::unpadded(64, 16),
                &mut full_buffer,
                1.0,
            )
            .unwrap();
            full_time += started.elapsed();

            let started = Instant::now();
            scoped
                .paint_raster(
                    &theme,
                    SurfaceExtent::unpadded(64, 16),
                    &mut scoped_buffer,
                    1.0,
                )
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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

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
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert!(diagnostics.error_count() >= 1);
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Error(message)
            if message.contains("animation.duration.fastest")
    ));
}

#[test]
fn animation_token_diagnostic_follows_a_non_structural_class_change() {
    let mut component = test_frontend_component(
        r#"
<template>
  <box class="panel">
    <box class="{itemClass}" />
    <text content="{label}" />
  </box>
</template>

<script lang="luau">
itemClass = "item"
label = "one"

function relabel()
    label = "two"
end

function breakItem()
    itemClass = "item broken"
end
</script>

<style>
.panel { width: 40px; height: 20px; }
.item { width: 10px; height: 10px; }
.broken { animation-duration: var(--animation-duration-fastest); }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    let extent = SurfaceExtent::unpadded(120, 40);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let diagnostics = component.diagnostics.clone().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 0);

    // An unrelated change diagnoses only its own node.
    component.call_namespaced_handler("relabel", &[]).unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(diagnostics.error_count(), 0);

    // The class change keeps the structure and dirties one node; that node
    // must still be diagnosed.
    component.call_namespaced_handler("breakItem", &[]).unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Error(message)
            if message.contains("animation.duration.fastest")
    ));
}

/// The render loop paints a surface again before presenting whenever
/// `wants_immediate_rerender` is true. A running animation re-arms the next
/// frame; it must not also ask for that corrective pass, or every animated
/// frame is restyled, laid out, and painted twice.
#[test]
fn a_running_animation_rearms_the_next_frame_without_a_second_pass() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { animation: pulse 1000ms linear infinite; }
@keyframes pulse { 0% { opacity: 0; } 100% { opacity: 1; } }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    // The first paint measures the surface and legitimately owes a configure.
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    for _ in 0..2 {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
            .unwrap();
        assert!(
            component.wants_render(),
            "the animation keeps frames coming"
        );
        assert!(!component.wants_immediate_rerender());
    }

    // Unrelated dirt on top of the re-arm is resolved in the same frame.
    component.invalidate_paint();
    assert!(component.wants_immediate_rerender());
}

const MOMENTUM_SCROLL_SOURCE: &str = r#"
<style>
scroll { width: 100px; height: 100px; overflow-y: auto; }
.content { width: 100px; height: 400px; flex-shrink: 0; }
</style>
<template><scroll><box class="content" /></scroll></template>
"#;

/// Momentum only moves a scroll offset. Its ticks must take the paint-only
/// path direct finger input takes, not a full restyle.
#[test]
fn momentum_ticks_are_paint_only_and_do_not_request_a_second_pass() {
    let mut component = test_frontend_component(MOMENTUM_SCROLL_SOURCE);
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(160, 120);
    let mut buffer = PixelBuffer::new(160, 120);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();

    for _ in 0..3 {
        component
            .handle_input(
                &theme,
                160,
                120,
                ComponentInput::TwoFingerScroll {
                    x: 20.0,
                    y: 20.0,
                    dx: 0.0,
                    dy: -12.0,
                },
            )
            .unwrap();
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
    }
    let node_id = *component
        .scroll_inertia
        .keys()
        .next()
        .expect("finger input arms momentum");

    // Inside momentum's start delay the tick moves nothing: it only keeps
    // frames coming.
    assert_eq!(component.dirty_types, ComponentDirtyFlags::PAINT);
    assert!(!component.wants_immediate_rerender());

    // After the delay the tick moves the offset.
    let before = component.scroll_offsets[&node_id].y;
    let inertia = component.scroll_inertia.get_mut(&node_id).unwrap();
    inertia.last_input -= Duration::from_millis(100);
    inertia.last_tick -= Duration::from_millis(16);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(component.last_dirty_types, ComponentDirtyFlags::PAINT);
    assert!(component.scroll_offsets[&node_id].y > before);
    assert!(component.wants_render());
    assert_eq!(
        component.dirty_types,
        ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS,
        "a tick that moved the offset re-arms a paint plus scroll metrics, never STYLE"
    );
    assert!(!component.wants_immediate_rerender());

    // The next frame paints the moved offset without restyling.
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert!(
        !component
            .last_dirty_types
            .contains(ComponentDirtyFlags::STYLE)
    );
}

#[test]
fn smooth_scroll_ticks_are_paint_only_and_do_not_request_a_second_pass() {
    let mut component = test_frontend_component(MOMENTUM_SCROLL_SOURCE);
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(160, 120);
    let mut buffer = PixelBuffer::new(160, 120);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();

    component.scroll_animations.insert(
        runtime_node_id_for_key("root/0"),
        ScrollAnimation {
            start: ScrollOffsetState::default(),
            target: ScrollOffsetState { x: 0.0, y: 80.0 },
            start_time: Instant::now()
                .checked_sub(Duration::from_millis(50))
                .unwrap(),
            duration: Duration::from_secs(60),
        },
    );
    component.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();

    assert_eq!(
        component.dirty_types,
        ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS
    );
    assert!(component.wants_render());
    assert!(!component.wants_immediate_rerender());
}

/// A declared transition makes every frame eligible for the style-animation
/// pass, but a paint-only frame reuses the retained styles and cannot start
/// one. With nothing live it skips the pass; a restyle still runs it.
#[test]
fn paint_only_frames_skip_the_style_animation_pass_when_nothing_is_live() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { width: 40px; height: 20px; opacity: 1; transition: opacity 200ms linear; }
.panel:hover { opacity: 0.5; }
</style>
"#,
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(120, 40);
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert!(component.transitions.is_empty());

    let before = component.style_animation_passes;
    component.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(component.style_animation_passes, before);

    // Hover restyles, so the pass runs and starts the transition.
    component
        .handle_input(
            &theme,
            120,
            40,
            ComponentInput::PointerMove { x: 10.0, y: 10.0 },
        )
        .unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(component.style_animation_passes, before + 1);
    assert!(
        !component.transitions.is_empty(),
        "the hover transition starts"
    );

    // While it runs, even a paint-only frame samples it.
    component.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(component.style_animation_passes, before + 2);
}

const SCROLL_LIST_SOURCE: &str = r#"
<style>
scroll { width: 100px; height: 100px; overflow-y: auto; }
.row { width: 100px; height: 20px; flex-shrink: 0; background-color: #336699; }
.row.alt { background-color: #994433; }
</style>
<template>
  <scroll>
    <box class="row" /><box class="row alt" /><box class="row" /><box class="row alt" />
    <box class="row" /><box class="row alt" /><box class="row" /><box class="row alt" />
    <box class="row" /><box class="row alt" /><box class="row" /><box class="row alt" />
  </scroll>
</template>
"#;

fn scroll_by(component: &mut FrontendSurfaceComponent, theme: &Theme, dy: f32) {
    component
        .handle_input(
            theme,
            120,
            120,
            ComponentInput::TwoFingerScroll {
                x: 20.0,
                y: 20.0,
                dx: 0.0,
                dy,
            },
        )
        .unwrap();
}

/// Consecutive scroll frames change nothing but a scroll offset, so they skip
/// the full annotation walk — and must paint exactly what the full walk paints.
#[test]
fn consecutive_scroll_frames_annotate_only_scroll_offsets() {
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(120, 120);
    let mut fast = test_frontend_component(SCROLL_LIST_SOURCE);
    let mut full = test_frontend_component(SCROLL_LIST_SOURCE);
    let mut fast_buffer = PixelBuffer::new(120, 120);
    let mut full_buffer = PixelBuffer::new(120, 120);
    for component in [&mut fast, &mut full] {
        component.motion_policy.reduced_motion = true;
    }
    full.force_full_interaction_frames = true;
    for _ in 0..2 {
        fast.paint_raster(&theme, extent, &mut fast_buffer, 1.0)
            .unwrap();
        full.paint_raster(&theme, extent, &mut full_buffer, 1.0)
            .unwrap();
    }

    for _ in 0..6 {
        scroll_by(&mut fast, &theme, -9.0);
        scroll_by(&mut full, &theme, -9.0);
        fast.paint_raster(&theme, extent, &mut fast_buffer, 1.0)
            .unwrap();
        full.paint_raster(&theme, extent, &mut full_buffer, 1.0)
            .unwrap();
        assert_eq!(fast_buffer.data(), full_buffer.data());
    }
    assert!(fast.scoped_annotation_frames >= 4);
    assert_eq!(full.scoped_annotation_frames, 0);
    let viewport = runtime_node_id_for_key("root/0");
    let offset = fast.scroll_offsets[&viewport];
    assert!(offset.y > 40.0);
    let metrics = first_node_by_tag(fast.last_tree.as_ref().unwrap(), "scroll")
        .unwrap()
        .resolved_scroll_metrics();
    assert_eq!(metrics.y, offset.y);
}

/// A paint-only frame can still change interaction state (e.g. a text
/// selection clears the pressed node). The changed node is projected.
#[test]
fn paint_only_frame_with_changed_interaction_state_projects_the_changed_node() {
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(120, 120);
    let mut component = test_frontend_component(SCROLL_LIST_SOURCE);
    let mut buffer = PixelBuffer::new(120, 120);
    // The first paint owes a configure pass; settle before the paint-only frame.
    for _ in 0..2 {
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
    }
    component.invalidate_paint();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert!(component.scoped_annotation_inputs.is_some());

    let row = first_node_with_class_token(component.last_tree.as_ref().unwrap(), "row")
        .unwrap()
        .id;
    component.pointer_down_id = Some(row);
    component.invalidate_paint();
    let before = component.scoped_annotation_frames;
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(component.scoped_annotation_frames, before + 1);
    let active = first_node_with_class_token(component.last_tree.as_ref().unwrap(), "row").unwrap();
    assert!(
        active.state.contains(NodeState::ACTIVE),
        "the pressed node is projected as :active"
    );
}

fn hovered_tile_scale(source: &str) -> f32 {
    let mut component = test_frontend_component(source);
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(400, 80);
    let mut buffer = PixelBuffer::new(400, 80);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let tree = component.last_tree.as_ref().expect("tree");
    let key = first_node_by_class(tree, "pick")
        .and_then(WidgetNode::mesh_key)
        .expect("picked tile")
        .to_owned();
    let (left, top, right, bottom) = find_node_bounds_by_key(tree, &key, 0.0, 0.0).expect("bounds");
    component
        .handle_input(
            &theme,
            400,
            80,
            ComponentInput::PointerMove {
                x: (left + right) / 2.0,
                y: (top + bottom) / 2.0,
            },
        )
        .unwrap();
    for _ in 0..3 {
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
    }
    let tree = component.last_tree.as_ref().expect("tree");
    let tile = first_node_by_class(tree, "pick").expect("picked tile");
    tile.computed_style.transform.scale_x
}

const TILE_STYLE: &str = r#"
<style>
.dock { width: 400px; height: 80px; gap: 8px; }
.tile { width: 60px; height: 60px; padding: 0; transform: scale(1); }
.tile:hover { transform: scale(1.5); }
.glyph { width: 40px; height: 40px; }
</style>
"#;

/// Dock tiles are generated by `{#for}`; a hovered generated button must match
/// `:hover` exactly like a hand-written one.
#[test]
fn hover_restyles_buttons_generated_by_a_for_loop() {
    let looped = format!(
        r#"
<template>
  <row class="dock">
    {{#for app in apps key={{app.id}}}}
      <button class="tile" class:pick={{app.id == "b"}} title={{app.id}}>
        <icon class="glyph" name="audio-volume-high" size="40" />
      </button>
    {{/for}}
  </row>
</template>
<script lang="luau">
apps = {{ {{ id = "a" }}, {{ id = "b" }}, {{ id = "c" }} }}
</script>
{TILE_STYLE}"#
    );
    let fixed = format!(
        r#"
<template>
  <row class="dock">
    <button class="tile" title="a"><icon class="glyph" name="audio-volume-high" size="40" /></button>
    <button class="tile pick" title="b"><icon class="glyph" name="audio-volume-high" size="40" /></button>
    <button class="tile" title="c"><icon class="glyph" name="audio-volume-high" size="40" /></button>
  </row>
</template>
{TILE_STYLE}"#
    );
    assert!(
        hovered_tile_scale(&fixed) > 1.0,
        "a static hovered button scales"
    );
    assert!(
        hovered_tile_scale(&looped) > 1.0,
        "a for-loop hovered button scales"
    );
}

// cargo test -p mesh-core-shell --release --lib -- keyframe_animation_frame_profile --ignored --nocapture
#[test]
#[ignore = "release-only keyframe animation frame profile"]
fn keyframe_animation_frame_profile() {
    for nodes in [1usize, 10, 100] {
        let mut source = String::from("<template><row>");
        for _ in 0..nodes {
            source.push_str("<box class=\"spin\" />");
        }
        source.push_str(
            r#"</row></template>
<style>
.spin { width: 4px; height: 4px; animation: spin 1000ms linear infinite; }
@keyframes spin {
  0% { opacity: 0.2; transform: translateX(0px); }
  50% { opacity: 1; transform: translateX(2px); }
  100% { opacity: 0.2; transform: translateX(0px); }
}
</style>"#,
        );
        let mut component = test_frontend_component(&source);
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(512, 8);
        for _ in 0..4 {
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(512, 8), &mut buffer, 1.0)
                .unwrap();
        }
        let frames = 300u32;
        let started = Instant::now();
        for _ in 0..frames {
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(512, 8), &mut buffer, 1.0)
                .unwrap();
        }
        let elapsed = started.elapsed();
        let compiles = component.keyframe_rule_compilations;
        eprintln!(
            "{nodes} animated nodes: {frames} frames {elapsed:?} ({:.1} us/frame); {compiles} rule compilations",
            elapsed.as_secs_f64() * 1e6 / f64::from(frames)
        );
        let per_instance = compiles as f64 / nodes as f64;
        if nodes == 100 {
            println!("MESH_PERF metric=keyframe_compilations_per_instance value={per_instance:.6}");
        }
        assert!(
            per_instance <= 1.0,
            "keyframe rules compile once per instance, not per frame"
        );
    }
}

#[test]
fn hover_and_transition_damage_matches_full_repaint() {
    let theme = default_theme();
    for scale in [1.0, 1.25, 2.0] {
        for hover_style in [
            "background: #58a4dc; border-radius: 12px; box-shadow: 3px 2px 4px #0008;",
            "opacity: 0.4; transform: translateX(8px) scale(1.1);",
            "width: 84px; padding: 4px;",
        ] {
            let source = format!(
                r#"
<template><column class="scene"><box class="target" /><box class="other" /></column></template>
<style>
.scene {{ width: 240px; height: 160px; padding: 20px; gap: 16px; background: linear-gradient(#1c2840, #697885); }}
.target {{ width: 64px; height: 40px; background: #d35770; border: 2px solid #eee; border-radius: 6px; transition: all 60ms linear; }}
.target:hover {{ {hover_style} }}
.other {{ width: 140px; height: 30px; background: #32a589; border-radius: 9px; }}
</style>"#
            );
            let mut component = test_frontend_component(&source);
            let width = (240.0 * scale) as u32;
            let height = (160.0 * scale) as u32;
            let extent = SurfaceExtent::unpadded(240, 160);
            let mut buffer = PixelBuffer::new(width, height);
            for _ in 0..3 {
                component
                    .paint_raster(&theme, extent, &mut buffer, scale)
                    .unwrap();
            }
            component.take_present_damage();
            let mut partial_frames = 0;
            for hovered in [true, false, true, false] {
                let event = if hovered {
                    ComponentInput::PointerMove { x: 40.0, y: 35.0 }
                } else {
                    ComponentInput::PointerLeave
                };
                component.handle_input(&theme, 240, 160, event).unwrap();
                assert!(
                    !component.surface_pixels_invalid,
                    "interaction keeps retained pixels"
                );
                for frame in 0..9 {
                    component
                        .paint_raster(&theme, extent, &mut buffer, scale)
                        .unwrap();
                    assert!(
                        !component.surface_pixels_invalid,
                        "animation keeps retained pixels"
                    );
                    let selected = component.retained_display_list.select_paint_commands(
                        Some(DamageRect {
                            x: 0,
                            y: 0,
                            width: 240,
                            height: 160,
                        }),
                        mesh_core_render::DisplayListRepaintPolicy::FullSurface,
                    );
                    let mut fresh = PixelBuffer::new(width, height);
                    mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics(
                        &selected,
                        &mut fresh,
                        scale,
                        None,
                        None,
                        None,
                        Some(component.id()),
                    );
                    let differing = buffer
                        .data()
                        .chunks(4)
                        .zip(fresh.data().chunks(4))
                        .filter(|(painted, full)| painted != full)
                        .count();
                    assert_eq!(
                        differing, 0,
                        "scale {scale}, hover {hovered}, frame {frame}, {hover_style}"
                    );
                    if component
                        .take_present_damage()
                        .iter()
                        .any(|rect| rect.width < 240 || rect.height < 160)
                    {
                        partial_frames += 1;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            assert!(
                partial_frames > 0,
                "retained diff must produce partial damage"
            );
        }
    }
}

#[test]
fn paint_only_hover_skips_layout_and_layout_rules_still_reflow() {
    let theme = default_theme();
    for (property, needs_layout) in [
        (
            "background: #e34; opacity: 0.7; transform: scale(1.1);",
            false,
        ),
        ("width: 90px;", true),
        ("font-size: 24px;", true),
        ("--child-width: 90px;", true),
    ] {
        let source = format!(
            r#"
<template><row><box class="target" /><box class="sibling" /></row></template>
<style>
row {{ width: 240px; height: 80px; }}
.target {{ width: 60px; height: 40px; background: #48a; }}
.target:hover {{ {property} }}
.sibling {{ width: 20px; height: 20px; background: #6b5; }}
</style>"#
        );
        let mut component = test_frontend_component(&source);
        let mut buffer = PixelBuffer::new(240, 80);
        let extent = SurfaceExtent::unpadded(240, 80);
        for _ in 0..3 {
            component
                .paint_raster(&theme, extent, &mut buffer, 1.0)
                .unwrap();
        }
        component.set_profiling_enabled(true);
        component.take_profiling_records();
        component
            .handle_input(
                &theme,
                240,
                80,
                ComponentInput::PointerMove { x: 20.0, y: 20.0 },
            )
            .unwrap();
        assert_eq!(
            component.dirty_types.contains(ComponentDirtyFlags::LAYOUT),
            needs_layout,
            "{property}"
        );
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
        assert_eq!(
            component
                .take_profiling_records()
                .iter()
                .any(|record| record.stage == mesh_core_debug::ProfilingStage::Layout),
            needs_layout,
            "{property}"
        );
        if property.starts_with("width") {
            let target =
                first_node_by_class(component.last_tree.as_ref().unwrap(), "target").unwrap();
            assert_eq!(target.layout.width, 90.0);
        }
    }
}

#[test]
fn user_part_hover_rules_schedule_restyle_and_classify_layout() {
    for (declarations, needs_layout) in [("opacity: 0.4", false), ("width: 90px", true)] {
        let mut theme = default_theme();
        theme
            .set_user_stylesheet(&format!(
                "@test/reactive-surface::part(target):hover {{ {declarations}; }}"
            ))
            .unwrap();
        let mut component = test_frontend_component(
            r#"
<template><row><box part="target" class="target"/><box class="sibling"/></row></template>
<style>row { width: 240px; height: 80px; } .target { width: 60px; height: 40px; } .sibling { width: 20px; height: 20px; }</style>"#,
        );
        let mut buffer = PixelBuffer::new(240, 80);
        let extent = SurfaceExtent::unpadded(240, 80);
        for _ in 0..3 {
            component
                .paint_raster(&theme, extent, &mut buffer, 1.0)
                .unwrap();
        }
        component.take_profiling_records();
        component
            .handle_input(
                &theme,
                240,
                80,
                ComponentInput::PointerMove { x: 20.0, y: 20.0 },
            )
            .unwrap();
        assert_eq!(
            component.dirty_types.contains(ComponentDirtyFlags::LAYOUT),
            needs_layout
        );
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
        let target = first_node_by_class(component.last_tree.as_ref().unwrap(), "target").unwrap();
        if needs_layout {
            assert_eq!(target.layout.width, 90.0);
        } else {
            assert_eq!(target.computed_style.opacity, 0.4);
        }
    }
}

#[test]
fn stepped_animation_sleeps_through_its_plateaus() {
    let mut component = test_frontend_component(
        r#"
<template><box class="caret" /></template>
<style>
.caret { width: 2px; height: 20px; background: white; animation: blink 400ms steps(1) infinite; }
@keyframes blink {
  50% { opacity: 0; }
  100% { opacity: 0; }
}
</style>
"#,
    );
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(40, 40);
    let mut buffer = PixelBuffer::new(40, 40);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    // The first paint schedules its own follow-up frame.
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let (until, _) = component
        .animation_hold
        .expect("a stepped animation holds until its next step");
    assert!(until > std::time::Instant::now());
    assert!(
        !component.wants_render(),
        "a plateau must not request a frame"
    );
    assert!(component.wants_tick());
    assert_eq!(component.next_tick_deadline(), Some(until));

    while std::time::Instant::now() < until {
        std::thread::sleep(until - std::time::Instant::now());
    }
    assert!(
        component.wants_render(),
        "the step boundary requests a frame"
    );
    component.tick().unwrap();
    let opacity_before = component.last_tree.as_ref().unwrap().children[0]
        .computed_style
        .opacity;
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let opacity_after = component.last_tree.as_ref().unwrap().children[0]
        .computed_style
        .opacity;
    assert_ne!(
        opacity_before, opacity_after,
        "the woken frame shows the step"
    );
    assert!(
        component
            .animation_hold
            .is_some_and(|(next, _)| next > until)
    );
}

#[test]
fn continuous_animation_keeps_requesting_frames() {
    let mut component = test_frontend_component(
        r#"
<template><box class="panel" /></template>
<style>
.panel { width: 10px; height: 10px; animation: fade 400ms linear infinite; }
@keyframes fade { 100% { opacity: 0; } }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(40, 40);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(40, 40), &mut buffer, 1.0)
        .unwrap();
    assert!(component.animation_hold.is_none());
    assert!(component.wants_render());
}

#[test]
fn animation_frame_visits_registered_nodes_and_full_diff_finds_no_omissions() {
    let mut source = String::from(r#"<template><row><box class="animated" />"#);
    for _ in 0..1024 {
        source.push_str("<box />");
    }
    source.push_str(
        r#"</row></template><style>
.animated { width: 10px; height: 10px; animation: pulse 1000ms linear infinite; }
@keyframes pulse { 0% { opacity: 0; } 100% { opacity: 1; } }
</style>"#,
    );
    let mut component = test_frontend_component(&source);
    let theme = default_theme();
    let mut pixels = PixelBuffer::new(120, 40);
    for _ in 0..3 {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(120, 40), &mut pixels, 1.0)
            .unwrap();
    }
    assert!(
        component
            .take_invalidation_snapshot()
            .unwrap()
            .node_visits
            .is_empty()
    );
    component.set_profiling_enabled(true);
    for elapsed in [100, 250, 500, 750] {
        component.invalidate_animation_style_path(ComponentDirtyFlags::VISUAL_REPAINT);
        paint_keyframe_frame_at(&mut component, Duration::from_millis(elapsed));
        let counts = component.take_invalidation_snapshot().unwrap().node_visits;
        assert_eq!(counts["animation_sample"], 1);
        assert_eq!(counts["annotation"], 0);
        assert_eq!(counts["restyle_scan"], 0);
        assert_eq!(counts["animation_previous_style"], 1);
        assert_eq!(counts["retained_discovery"], 0);
        assert_eq!(counts["validation"], 0);
        assert!(counts["retained_sync"] <= 3);
        assert!(counts["animation_lookup"] <= 8);
        assert_eq!(component.animation_nodes_visited, 1);
        assert!(component.retained_tree.last_selected_node_count <= 3);
        let tree = component.last_tree.clone().unwrap();
        let missed = component.retained_tree.update(&tree);
        assert!(!missed.any(), "sparse animation missed {missed:?}");
        assert!(component.retained_tree.render_dirty_node_ids().is_empty());
    }
}

fn hover_scope_source(clean_siblings: usize) -> String {
    let mut source = String::from(
        r#"<template><row class="strip"><box class="tile"><text class="label">a</text></box><box class="tile"><text class="label">b</text></box>"#,
    );
    for _ in 0..clean_siblings {
        source.push_str(r#"<box class="clean" />"#);
    }
    source.push_str(
        r#"</row></template><style>
.strip { width: 120px; height: 40px; }
.tile { width: 20px; height: 20px; color: #000000; }
.tile:hover { background-color: #ff0000; color: #0000ff; }
.clean { width: 0px; height: 0px; }
</style>"#,
    );
    source
}

fn hover_tile(component: &mut FrontendSurfaceComponent, theme: &Theme, index: usize) {
    let tree = component.last_tree.as_ref().unwrap();
    let key = first_node_by_tag(tree, "row").unwrap().children[index]
        .mesh_key()
        .unwrap()
        .to_owned();
    let (left, top, right, bottom) = find_node_bounds_by_key(tree, &key, 0.0, 0.0).unwrap();
    component
        .handle_input(
            theme,
            120,
            40,
            ComponentInput::PointerMove {
                x: (left + right) / 2.0,
                y: (top + bottom) / 2.0,
            },
        )
        .unwrap();
}

fn computed_styles(
    node: &WidgetNode,
    styles: &mut Vec<(NodeId, mesh_core_elements::ComputedStyle)>,
) {
    styles.push((node.id, node.computed_style.clone()));
    for child in &node.children {
        computed_styles(child, styles);
    }
}

/// Hover frames visit the hovered item's nodes, not the tree: annotation,
/// restyle, semantic and retained work stay flat as clean siblings grow. A
/// full diff finds nothing missed, a twin on the full walks paints and styles
/// identically, and a rebuild reproduces the twin's styles.
#[test]
fn hover_frame_visits_scale_with_the_hovered_item_not_the_tree() {
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(120, 40);
    let mut per_size = Vec::new();
    for clean_siblings in [16, 1024] {
        let source = hover_scope_source(clean_siblings);
        let mut scoped = test_frontend_component(&source);
        let mut full = test_frontend_component(&source);
        full.force_full_interaction_frames = true;
        let mut scoped_pixels = PixelBuffer::new(120, 40);
        let mut full_pixels = PixelBuffer::new(120, 40);
        for component in [&mut scoped, &mut full] {
            component.motion_policy.reduced_motion = true;
        }
        for _ in 0..3 {
            scoped
                .paint_raster(&theme, extent, &mut scoped_pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
        }
        scoped.set_profiling_enabled(true);
        let _ = scoped.take_invalidation_snapshot();
        let mut visits = Vec::new();
        for tile in [0, 1, 0] {
            hover_tile(&mut scoped, &theme, tile);
            hover_tile(&mut full, &theme, tile);
            scoped
                .paint_raster(&theme, extent, &mut scoped_pixels, 1.0)
                .unwrap();
            full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
                .unwrap();
            let counts = scoped.take_invalidation_snapshot().unwrap().node_visits;
            assert_eq!(counts["retained_discovery"], 0, "{counts:?}");
            visits.push([
                counts["annotation"],
                counts["restyle_scan"],
                counts["retained_lookup"],
                counts["retained_sync"],
                counts["accessibility_project"],
            ]);

            let tree = scoped.last_tree.clone().unwrap();
            let missed = scoped.retained_tree.update(&tree);
            assert!(!missed.any(), "scoped hover missed {missed:?}");
            assert_eq!(scoped_pixels.data(), full_pixels.data());
            let (mut scoped_styles, mut full_styles) = (Vec::new(), Vec::new());
            computed_styles(&tree, &mut scoped_styles);
            computed_styles(full.last_tree.as_ref().unwrap(), &mut full_styles);
            assert!(scoped_styles == full_styles, "computed styles diverged");
        }
        let hovered = first_node_by_tag(scoped.last_tree.as_ref().unwrap(), "row")
            .unwrap()
            .children[0]
            .clone();
        assert!(hovered.state.contains(NodeState::HOVERED));
        assert!(scoped.scoped_annotation_frames >= 3);
        // Targeted restyles keep the surface root's inherited text values.
        let mut retained_styles = Vec::new();
        computed_styles(full.last_tree.as_ref().unwrap(), &mut retained_styles);
        full.invalidate(ComponentDirtyFlags::TREE_REBUILD);
        full.paint_raster(&theme, extent, &mut full_pixels, 1.0)
            .unwrap();
        let mut rebuilt_styles = Vec::new();
        computed_styles(full.last_tree.as_ref().unwrap(), &mut rebuilt_styles);
        assert!(
            retained_styles == rebuilt_styles,
            "targeted restyle drifted from a rebuild"
        );
        per_size.push(visits);
    }
    assert_eq!(
        per_size[0], per_size[1],
        "hover visits grew with clean siblings"
    );
}
