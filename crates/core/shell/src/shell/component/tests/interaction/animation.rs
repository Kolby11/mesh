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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    let rule = component
        .keyframe_rules
        .values()
        .next()
        .expect("lowered shell keyframe rule");
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
    component
        .paint(&theme, SurfaceExtent::unpadded(96, 32), &mut buffer, 1.0)
        .unwrap();
    // Every surface content-measures now; the first paint records measured_size
    // and requests one surface-config settle frame. Paint again so it stabilises.
    component
        .paint(&theme, SurfaceExtent::unpadded(96, 32), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(160, 48), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
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
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    assert!(component.animation_only_dirty);
    component.set_profiling_enabled(true);
    component.take_profiling_records();
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(80, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(80, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(80, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    assert!(component.animation_only_dirty);

    component.invalidate_surface_config();
    assert!(!component.animation_only_dirty);
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(
            &theme,
            SurfaceExtent::unpadded(120, 40),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint(
        &theme,
        SurfaceExtent::unpadded(120, 40),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

    full.animation_only_dirty = false;
    scoped
        .paint(
            &theme,
            SurfaceExtent::unpadded(120, 40),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint(
        &theme,
        SurfaceExtent::unpadded(120, 40),
        &mut full_buffer,
        1.0,
    )
    .unwrap();

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
        .paint(
            &theme,
            SurfaceExtent::unpadded(64, 16),
            &mut scoped_buffer,
            1.0,
        )
        .unwrap();
    full.paint(
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
                .paint(
                    &theme,
                    SurfaceExtent::unpadded(64, 16),
                    &mut scoped_buffer,
                    1.0,
                )
                .unwrap();
            scoped_time += started.elapsed();

            full.animation_only_dirty = false;
            let started = Instant::now();
            full.paint(
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
            full.paint(
                &theme,
                SurfaceExtent::unpadded(64, 16),
                &mut full_buffer,
                1.0,
            )
            .unwrap();
            full_time += started.elapsed();

            let started = Instant::now();
            scoped
                .paint(
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
    let diagnostics = component.diagnostics.clone().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 0);

    // An unrelated change diagnoses only its own node.
    component.call_namespaced_handler("relabel", &[]).unwrap();
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
    assert_eq!(diagnostics.error_count(), 0);

    // The class change keeps the structure and dirties one node; that node
    // must still be diagnosed.
    component.call_namespaced_handler("breakItem", &[]).unwrap();
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
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
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    for _ in 0..2 {
        component
            .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
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
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();

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
        component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();

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
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();

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
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
    assert!(component.transitions.is_empty());

    let before = component.style_animation_passes;
    component.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
    assert_eq!(component.style_animation_passes, before + 1);
    assert!(
        !component.transitions.is_empty(),
        "the hover transition starts"
    );

    // While it runs, even a paint-only frame samples it.
    component.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
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

/// Consecutive scroll frames change nothing but a scroll offset, so after the
/// first paint-only frame they skip the full annotation walk — and must paint
/// exactly what the full walk paints.
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
    for _ in 0..2 {
        fast.paint(&theme, extent, &mut fast_buffer, 1.0).unwrap();
        full.paint(&theme, extent, &mut full_buffer, 1.0).unwrap();
    }

    for _ in 0..6 {
        scroll_by(&mut fast, &theme, -9.0);
        scroll_by(&mut full, &theme, -9.0);
        // No stored inputs forces the full walk.
        full.paint_only_annotation_inputs = None;
        fast.paint(&theme, extent, &mut fast_buffer, 1.0).unwrap();
        full.paint(&theme, extent, &mut full_buffer, 1.0).unwrap();
        assert_eq!(fast_buffer.data(), full_buffer.data());
    }
    assert!(fast.scroll_only_annotation_frames >= 4);
    assert_eq!(full.scroll_only_annotation_frames, 0);
    let viewport = runtime_node_id_for_key("root/0");
    let offset = fast.scroll_offsets[&viewport];
    assert!(offset.y > 40.0);
    let metrics = first_node_by_tag(fast.last_tree.as_ref().unwrap(), "scroll")
        .unwrap()
        .resolved_scroll_metrics();
    assert_eq!(metrics.y, offset.y);
}

/// A paint-only frame can still change interaction state (e.g. a text
/// selection clears the pressed node). Changed inputs take the full walk.
#[test]
fn paint_only_frame_with_changed_interaction_state_takes_the_full_annotation() {
    let theme = default_theme();
    let extent = SurfaceExtent::unpadded(120, 120);
    let mut component = test_frontend_component(SCROLL_LIST_SOURCE);
    let mut buffer = PixelBuffer::new(120, 120);
    // The first paint owes a configure pass; settle before the paint-only frame.
    for _ in 0..2 {
        component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
    }
    component.invalidate_paint();
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
    assert!(component.paint_only_annotation_inputs.is_some());

    let row = first_node_with_class_token(component.last_tree.as_ref().unwrap(), "row")
        .unwrap()
        .id;
    component.pointer_down_id = Some(row);
    component.invalidate_paint();
    let before = component.scroll_only_annotation_frames;
    component.paint(&theme, extent, &mut buffer, 1.0).unwrap();
    assert_eq!(component.scroll_only_annotation_frames, before);
    let active = first_node_with_class_token(component.last_tree.as_ref().unwrap(), "row").unwrap();
    assert!(
        active.state.active,
        "the pressed node is projected as :active"
    );
}
