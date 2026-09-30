use super::*;

#[test]
fn hidden_child_surface_tracks_retained_tree_changes() {
    let mut component = test_frontend_component("<template><box /></template>");
    let mut child = event_node("box", "root/popup", 0.0, 0.0, 40.0, 30.0, &[]);
    child.mark_promoted_popover();
    child.attributes.insert("hidden".into(), "true".into());
    child.computed_style.background_color = mesh_core_elements::style::Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let child_id = child.id;
    let mut tree = root_with(vec![child]);
    component.retained_tree.update(&tree);
    component
        .retained_display_list
        .update(&tree, 120, 80, false, true);
    component.last_tree = Some(tree.clone());
    let first = component
        .child_surface_paint_generation("root/popup")
        .unwrap();
    let mut buffer = PixelBuffer::new(40, 30);
    component
        .paint_child_surface("root/popup", &mut buffer, 1.0, (0, 0), false)
        .unwrap();
    assert_eq!(buffer.get_pixel(10, 10).g, 0);
    assert_eq!(buffer.get_pixel(10, 10).r, 255);
    let first_list = component
        .child_display_lists
        .borrow()
        .get(child_id)
        .unwrap()
        .generation();

    tree.children[0].computed_style.background_color.g = 255;
    component.retained_tree.update(&tree);
    component
        .retained_display_list
        .update(&tree, 120, 80, false, true);
    component.last_tree = Some(tree);
    assert!(
        component
            .child_surface_paint_generation("root/popup")
            .unwrap()
            > first
    );
    component
        .paint_child_surface("root/popup", &mut buffer, 1.0, (0, 0), false)
        .unwrap();
    assert_eq!(buffer.get_pixel(10, 10).g, 255);
    assert!(
        component
            .child_display_lists
            .borrow()
            .get(child_id)
            .unwrap()
            .generation()
            > first_list
    );
    // Repeating the same child frame must retain its commands.
    let generation = component
        .child_display_lists
        .borrow()
        .get(child_id)
        .unwrap()
        .generation();
    component
        .paint_child_surface("root/popup", &mut buffer, 1.0, (0, 0), false)
        .unwrap();
    assert_eq!(
        component
            .child_display_lists
            .borrow()
            .get(child_id)
            .unwrap()
            .generation(),
        generation
    );
}

#[test]
fn frontend_publication_preserves_current_requests_after_runtime_advances() {
    let mut component = test_frontend_component("<template><box /></template>");
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 80), &mut buffer, 1.0)
        .unwrap();
    component.record_frontend_host_effects(vec![CoreRequest::ShowSurface {
        surface_id: "old".into(),
    }]);
    component.runtime_generations.borrow_mut().sync("root", 100);
    component.record_frontend_host_effects(vec![CoreRequest::ShowSurface {
        surface_id: "current".into(),
    }]);
    component.publish_frontend_frame();
    let frame = component.frontend_frame().unwrap();
    assert!(
        matches!(frame.effects().host_requests(), [CoreRequest::ShowSurface { surface_id }] if surface_id == "current")
    );
    assert_eq!(
        frame.effects().host_request_revisions(),
        &[Some(frame.revisions().effect_revision())]
    );
}

#[test]
fn paint_publishes_one_interaction_frame_across_all_phases() {
    let mut component = test_frontend_component("<template><box /></template>");
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 80);

    component
        .paint(&theme, SurfaceExtent::unpadded(120, 80), &mut buffer, 1.0)
        .unwrap();

    let frame = &component.interaction_frame;
    assert!(frame.is_publishable());
    assert_eq!(
        frame.phase(),
        Some(mesh_core_interaction::InteractionFramePhase::SemanticsReady)
    );
    assert_eq!(frame.tree_revision(), component.frame_revision);
    assert!(frame.tree_snapshot().is_some());
    for phase in [
        mesh_core_interaction::InteractionFramePhase::InputResolved,
        mesh_core_interaction::InteractionFramePhase::StateUpdated,
        mesh_core_interaction::InteractionFramePhase::StyleInvalidated,
        mesh_core_interaction::InteractionFramePhase::LayoutReady,
        mesh_core_interaction::InteractionFramePhase::AnimationSampled,
        mesh_core_interaction::InteractionFramePhase::PaintReady,
        mesh_core_interaction::InteractionFramePhase::SemanticsReady,
    ] {
        assert_eq!(
            frame.phase_stamp(phase).map(|stamp| stamp.revision()),
            Some(frame.revision())
        );
    }

    let frontend_frame = component
        .frontend_frame()
        .expect("completed paint publishes a frontend frame");
    assert_eq!(
        frontend_frame.revisions().tree,
        frontend_frame.tree().expect("frame tree").revision()
    );
    assert_eq!(
        frontend_frame.revisions().services,
        frontend_frame.services().revision()
    );
    assert!(frontend_frame.invalidation().profiling().is_some());
    assert!(
        frontend_frame
            .effects()
            .host_requests()
            .iter()
            .any(|request| matches!(request, CoreRequest::PublishDiagnostics { .. }))
    );
}

#[test]
fn keyboard_regression_buttons_sliders_inputs_and_pointer_modality() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
button_count = 0
slider_seen = 0
input_seen = ""

function onButtonClick()
    button_count = button_count + 1
end

function onSliderChange(value)
    slider_seen = value
end

function onInputChange(value)
    input_seen = value
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![
        event_node(
            "input",
            "root/0",
            0.0,
            0.0,
            80.0,
            24.0,
            &[("change", "onInputChange")],
        ),
        event_node(
            "button",
            "root/1",
            0.0,
            32.0,
            80.0,
            24.0,
            &[("click", "onButtonClick")],
        ),
        event_node_with_attrs(
            "slider",
            "root/2",
            0.0,
            64.0,
            120.0,
            24.0,
            &[
                ("min", "0"),
                ("max", "1"),
                ("step", "0.1"),
                ("value", "0.5"),
            ],
            &[("change", "onSliderChange")],
        ),
    ]));
    let input_id = find_node_by_key(component.last_tree.as_ref().unwrap(), "root/0")
        .unwrap()
        .id;
    component.input_values.insert(input_id, "ab".into());
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
    assert_eq!(component.focus_visible_key.as_deref(), Some("root/0"));
    assert_eq!(component.interaction_frame.decisions().len(), 2);
    assert!(
        component
            .interaction_frame
            .invalidation()
            .contains(mesh_core_interaction::InteractionInvalidation::FOCUS)
    );
    assert!(
        component
            .interaction_frame
            .invalidation()
            .contains(mesh_core_interaction::InteractionInvalidation::POINTER_CAPTURE)
    );
    assert!(
        component
            .interaction_frame
            .invalidation()
            .contains(mesh_core_interaction::InteractionInvalidation::PRESS_ORIGIN)
    );

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Backspace".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "input_seen"),
        Some(serde_json::Value::String("a".into()))
    );

    component.focused_key = Some("root/1".into());
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(runtime_number(&component, "button_count"), 1.0);

    component.focused_key = Some("root/2".into());
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "ArrowRight".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!((runtime_number(&component, "slider_seen") - 0.6).abs() < 0.001);
}

#[test]
fn pseudo_state_restyle_preserves_runtime_instances_and_local_state() {
    let mut component = test_frontend_component(
        r#"
<style>
input:focus {
  background-color: #404040;
}
input:checked {
  background-color: #606060;
}
</style>
<template>
  <column>
    <input value="initial" />
    <checkbox checked="false" />
  </column>
</template>
<script lang="luau">
render_count = 0
function render()
    render_count = render_count + 1
end
</script>
"#,
    );
    let runtime_count_before = component.runtimes.lock().unwrap().len();
    component
        .input_values
        .insert(runtime_node_id_for_key("root/0/0"), "local".into());
    component
        .checked_values
        .insert(runtime_node_id_for_key("root/0/1"), true);
    component.focused_key = Some("root/0/0".into());

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 120);
    component
        .paint(&theme, SurfaceExtent::unpadded(240, 120), &mut buffer, 1.0)
        .unwrap();
    let render_count_after_first = runtime_number(&component, "render_count");
    let runtime_count_after_first = component.runtimes.lock().unwrap().len();

    component.hovered_path = ["root", "root/0", "root/0/1"]
        .map(runtime_node_id_for_key)
        .to_vec();
    component.hovered_key = Some("root/0/1".into());
    component.dirty = true;
    component
        .paint(&theme, SurfaceExtent::unpadded(240, 120), &mut buffer, 1.0)
        .unwrap();

    assert_eq!(runtime_count_before, runtime_count_after_first);
    assert_eq!(
        runtime_count_before,
        component.runtimes.lock().unwrap().len()
    );
    assert_eq!(
        runtime_number(&component, "render_count"),
        render_count_after_first
    );

    let tree = component.last_tree.as_ref().unwrap();
    assert_eq!(
        node_by_mesh_key(tree, "root/0/0")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("local")
    );
    assert!(node_by_mesh_key(tree, "root/0/1").state.checked);
}

#[test]
fn container_size_restyle_preserves_runtime_and_local_state() {
    let mut component = test_frontend_component(
        r#"
<style>
.panel {
  width: 100%;
  height: 100%;
  background-color: #222222;
  gap: 4px;
}
scroll {
  height: 20px;
  overflow-y: auto;
}
text {
  height: 100px;
}
@container (min-width: 400px) {
  .panel {
    background-color: #eeeeee;
    gap: 16px;
  }
  input {
    width: 180px;
  }
}
@container (max-width: 399px) {
  input {
    width: 90px;
  }
}
</style>
<template>
  <column class="panel">
    <input value="initial" />
    <slider min="0" max="100" value="25" />
    <checkbox checked="false" />
    <scroll>
      <text>Scrollable content</text>
    </scroll>
  </column>
</template>
<script lang="luau">
render_count = 0
function render()
    render_count = render_count + 1
end
</script>
"#,
    );
    component
        .input_values
        .insert(runtime_node_id_for_key("root/0/0"), "local".into());
    component
        .slider_values
        .insert(runtime_node_id_for_key("root/0/1"), 73.0);
    component
        .checked_values
        .insert(runtime_node_id_for_key("root/0/2"), true);
    component.scroll_offsets.insert(
        runtime_node_id_for_key("root/0/3"),
        ScrollOffsetState { x: 3.0, y: 14.0 },
    );

    let theme = default_theme();
    let mut wide_buffer = PixelBuffer::new(420, 160);
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(420, 160),
            &mut wide_buffer,
            1.0,
        )
        .unwrap();
    let render_count_after_wide = runtime_number(&component, "render_count");
    let runtime_count_after_wide = component.runtimes.lock().unwrap().len();
    let wide_tree = component.last_tree.as_ref().unwrap();
    assert_eq!(
        node_by_mesh_key(wide_tree, "root/0")
            .computed_style
            .background_color,
        Color::from_hex("#eeeeee").unwrap()
    );
    assert_eq!(
        node_by_mesh_key(wide_tree, "root/0/0")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("local")
    );

    // Every surface content-measures now; the wide paint recorded measured_size
    // and requested one surface-config settle frame. Paint again so it stabilises
    // before asserting the component has quiesced.
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(420, 160),
            &mut wide_buffer,
            1.0,
        )
        .unwrap();
    component.dirty = false;
    assert!(
        !component.surface_size_changed(420, 160),
        "identical consecutive dimensions should not mark the component dirty"
    );
    assert!(!component.wants_render());

    assert!(component.surface_size_changed(260, 160));
    assert!(component.wants_render());
    let mut narrow_buffer = PixelBuffer::new(260, 160);
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(260, 160),
            &mut narrow_buffer,
            1.0,
        )
        .unwrap();

    assert_eq!(
        runtime_count_after_wide,
        component.runtimes.lock().unwrap().len(),
        "size restyles must reuse the existing runtime"
    );
    assert_eq!(
        render_count_after_wide,
        runtime_number(&component, "render_count"),
        "size restyles should not rerun frontend render hooks"
    );

    let narrow_tree = component.last_tree.as_ref().unwrap();
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0")
            .computed_style
            .background_color,
        Color::from_hex("#222222").unwrap()
    );
    let input_width = node_by_mesh_key(narrow_tree, "root/0/0")
        .computed_style
        .width;
    assert!(
        matches!(input_width, mesh_core_elements::Dimension::Px(px) if (px - 90.0).abs() < f32::EPSILON)
    );
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0/0")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("local")
    );
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0/1")
            .attributes
            .get("value")
            .map(String::as_str),
        Some("73.00")
    );
    assert!(node_by_mesh_key(narrow_tree, "root/0/2").state.checked);
    let retained_scroll = component
        .scroll_offsets
        .get(&runtime_node_id_for_key("root/0/3"))
        .expect("scroll offset must survive a container-size restyle");
    assert!(
        retained_scroll.y > 0.0,
        "a resized viewport may clamp the offset, but must not reset it"
    );
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0/3")
            .resolved_scroll_metrics()
            .y,
        retained_scroll.y,
        "the restyled scroll node must expose the retained, clamped offset"
    );
}

#[test]
fn slider_change_handler_receives_number_on_pointer_move() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
slider_seen = -1
function onSliderChange(value)
    slider_seen = value
end
</script>
"#,
    );
    let mut slider = event_node(
        "slider",
        "root/0",
        0.0,
        0.0,
        100.0,
        20.0,
        &[("change", "onSliderChange")],
    );
    slider.attributes.insert("min".into(), "0".into());
    slider.attributes.insert("max".into(), "1".into());
    slider.attributes.insert("value".into(), "0".into());
    component.last_tree = Some(root_with(vec![slider]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                button: 0x110,
                x: 0.0,
                y: 10.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 75.0, y: 10.0 },
        )
        .unwrap();

    assert!((runtime_number(&component, "slider_seen") - 0.75).abs() < 0.001);
}

#[test]
fn navigation_volume_slider_proves_event_state_render_flow() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <slider min="0" max="1" value="{slider_value}" onchange={onVolumeChange} />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end

audio_percent = 0
slider_value = 0.0
icon_name = "audio-volume-muted"
audio_tooltip = "Volume unavailable"
handler_value_type = "unset"

local function clamp_volume(value)
    local numeric = tonumber(value) or 0
    if numeric < 0 then return 0.0 end
    if numeric > 1 then return 1.0 end
    return numeric
end

local function update_audio_copy(percent, muted)
    audio_percent = percent
    slider_value = clamp_volume(percent / 100)
    if muted or percent == 0 then
        icon_name = "audio-volume-muted"
    elseif percent < 34 then
        icon_name = "audio-volume-low"
    elseif percent < 67 then
        icon_name = "audio-volume-medium"
    else
        icon_name = "audio-volume-high"
    end
    if muted then
        audio_tooltip = string.format("Volume muted at %d%%", percent)
    else
        audio_tooltip = string.format("Volume %d%%", percent)
    end
end

function render()
    if not audio_ok or not audio then
        icon_name = "audio-volume-muted"
        audio_tooltip = "Audio service unavailable"
        audio_percent = 0
        slider_value = 0.0
        return
    end
    local percent = math.floor(tonumber(audio.percent) or 0)
    local muted = audio.muted or false
    update_audio_copy(percent, muted)
end

function onVolumeChange(value)
    handler_value_type = type(value)
    local normalized = clamp_volume(value)
    local percent = math.floor((normalized * 100) + 0.5)
    slider_value = normalized
    update_audio_copy(percent, false)
    if audio_ok and audio then
        audio.set_volume("default", percent)
    end
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read", "service.audio.control"],
    );
    {
        let mut runtimes = component.runtimes.lock().unwrap();
        let runtime = runtimes.get_mut(component.id()).unwrap();
        runtime.script_ctx.apply_service_payload(
            "audio",
            &serde_json::json!({ "percent": 20, "muted": false }),
        );
        runtime.script_ctx.call_handler("render", &[]).unwrap();
    }
    component.render_hooks_pending = false;

    let mut slider = event_node(
        "slider",
        "root/0",
        0.0,
        0.0,
        100.0,
        20.0,
        &[("change", "onVolumeChange")],
    );
    slider.attributes.insert("min".into(), "0".into());
    slider.attributes.insert("max".into(), "1".into());
    slider.attributes.insert("value".into(), "0.2".into());
    component.last_tree = Some(root_with(vec![slider]));
    component.clear_runtime_dirty_states();
    component.dirty = false;

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                button: 0x110,
                x: 80.0,
                y: 10.0,
                pressed: true,
            },
        )
        .unwrap();
    let requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 50.0, y: 10.0 },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "handler_value_type"),
        Some(serde_json::json!("number"))
    );
    assert_eq!(
        runtime_value(&component, "audio_percent"),
        Some(serde_json::json!(50))
    );
    assert!((runtime_number(&component, "slider_value") - 0.5).abs() < 0.001);
    assert_eq!(
        runtime_value(&component, "icon_name"),
        Some(serde_json::json!("audio-volume-medium"))
    );
    assert_eq!(
        runtime_value(&component, "audio_tooltip"),
        Some(serde_json::json!("Volume 50%"))
    );
    assert!(
        component.wants_render(),
        "changed reactive globals should mark dirty"
    );

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            }
            | CoreRequest::ServiceCall {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.audio");
            assert_eq!(command, "set_volume");
            assert_eq!(
                payload,
                &serde_json::json!({ "device_id": "default", "percent": 50 })
            );
        }
        other => panic!("expected one mesh.audio.set_volume request, got {other:?}"),
    }

    let mut buffer = PixelBuffer::new(240, 40);
    component
        .paint(&theme, SurfaceExtent::unpadded(240, 40), &mut buffer, 1.0)
        .unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("paint should cache tree");
    let slider = first_node_by_tag(tree, "slider").expect("painted tree should contain slider");
    let rendered_value = slider
        .attributes
        .get("value")
        .and_then(|value| value.parse::<f64>().ok())
        .expect("painted slider value should be numeric");
    assert!(
        (rendered_value - 0.5).abs() < 0.001,
        "next paint should rebuild from the updated reactive slider state"
    );
    assert!(
        !component
            .runtimes
            .lock()
            .unwrap()
            .get(component.id())
            .unwrap()
            .script_ctx
            .state()
            .is_dirty(),
        "paint should consume runtime dirty state after rebuilding"
    );
}

#[test]
fn slider_drag_repaints_across_multiple_pointer_moves() {
    let mut component = test_frontend_component(
        r#"
<style>
slider {
  width: 220px;
  height: 40px;
  color: #ffffff;
}
</style>
<template>
  <slider min="0" max="100" value="0" />
</template>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);

    component
        .paint(&theme, SurfaceExtent::unpadded(240, 40), &mut buffer, 1.0)
        .unwrap();
    let initial = buffer.data().to_vec();

    component
        .handle_input(
            &theme,
            240,
            40,
            ComponentInput::PointerButton {
                button: 0x110,
                x: 24.0,
                y: 20.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            40,
            ComponentInput::PointerMove { x: 200.0, y: 20.0 },
        )
        .unwrap();
    component
        .paint(&theme, SurfaceExtent::unpadded(240, 40), &mut buffer, 1.0)
        .unwrap();
    let after_first_drag = buffer.data().to_vec();

    component
        .handle_input(
            &theme,
            240,
            40,
            ComponentInput::PointerMove { x: 60.0, y: 20.0 },
        )
        .unwrap();
    component
        .paint(&theme, SurfaceExtent::unpadded(240, 40), &mut buffer, 1.0)
        .unwrap();
    let after_second_drag = buffer.data().to_vec();

    assert_ne!(after_first_drag, initial);
    assert_ne!(
        after_second_drag, after_first_drag,
        "each pointer move while dragging should repaint the slider immediately"
    );
    let rendered_value = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .and_then(|slider| slider.attributes.get("value"))
        .and_then(|value| value.parse::<f32>().ok())
        .expect("painted slider value");
    assert!(
        rendered_value < 40.0,
        "second drag should move the painted slider back toward the left, got {rendered_value}"
    );
}

#[test]
fn theme_change_repaints_token_styled_content() {
    let mut component = test_frontend_component(
        r#"
<style>
box {
  width: 48px;
  height: 24px;
  background-color: var(--color-primary);
}
</style>
<template>
  <box />
</template>
"#,
    );
    let dark = themed_primary("test-dark", "#112233");
    let light = themed_primary("test-light", "#c0ffee");
    let mut buffer = PixelBuffer::new(64, 32);

    component
        .paint(&dark, SurfaceExtent::unpadded(64, 32), &mut buffer, 1.0)
        .unwrap();
    let dark_pixel = buffer_pixel(&buffer, 12, 12);

    component.theme_changed().unwrap();
    component
        .paint(&light, SurfaceExtent::unpadded(64, 32), &mut buffer, 1.0)
        .unwrap();
    let light_pixel = buffer_pixel(&buffer, 12, 12);

    assert_ne!(dark_pixel, light_pixel);
    assert_eq!(light_pixel, [0xee, 0xff, 0xc0, 0xff]);
}

#[test]
fn theme_change_repaints_scrollbar_colors() {
    let mut component = test_frontend_component(
        r#"
<style>
scroll {
  width: 48px;
  height: 48px;
  overflow-y: scroll;
  color: var(--color-primary);
}
box {
  width: 48px;
  height: 96px;
  flex-shrink: 0;
}
</style>
<template>
  <scroll><box /></scroll>
</template>
"#,
    );
    let dark = themed_primary("test-dark", "#112233");
    let light = themed_primary("test-light", "#c0ffee");
    let mut buffer = PixelBuffer::new(64, 64);

    component
        .paint(&dark, SurfaceExtent::unpadded(64, 64), &mut buffer, 1.0)
        .unwrap();
    let dark_scrollbar = buffer_pixel(&buffer, 40, 32);

    component.theme_changed().unwrap();
    component
        .paint(&light, SurfaceExtent::unpadded(64, 64), &mut buffer, 1.0)
        .unwrap();
    let light_scrollbar = buffer_pixel(&buffer, 40, 32);

    assert_ne!(dark_scrollbar, light_scrollbar);
    assert_ne!(
        light_scrollbar,
        [0x8f, 0x87, 0x9c, 0xff],
        "scrollbars must use the active theme instead of the former fixed thumb color"
    );
}

#[test]
fn real_navigation_bar_repaints_when_theme_changes() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let dark = default_theme();
    let mut light = default_theme();
    light.id = "mesh-default-light".into();
    light.name = "mesh-default-light".into();
    light.tokens_mut().insert(
        "color.surface-container".into(),
        mesh_core_theme::TokenValue::String("#f0f0f0".into()),
    );
    light.tokens_mut().insert(
        "color.on-surface".into(),
        mesh_core_theme::TokenValue::String("#111111".into()),
    );
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .paint(
            &dark,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let dark_snapshot = buffer.data().to_vec();

    component.theme_changed().unwrap();
    component
        .paint(
            &light,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    assert_ne!(
        buffer.data(),
        dark_snapshot,
        "navigation bar should repaint when the active theme changes"
    );
}

#[test]
fn real_navigation_bar_repaints_existing_transition_state_when_theme_changes_back_to_dark() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let dark = test_theme("mesh-default-dark");
    let light = test_theme("mesh-default-light");
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .paint(
            &dark,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    component.theme_changed().unwrap();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "current": "mesh-default-light",
                "theme_id": "mesh-default-light",
                "is_dark": false
            }),
        })
        .unwrap();
    for _ in 0..2 {
        component
            .paint(
                &light,
                SurfaceExtent::unpadded(width, height),
                &mut buffer,
                1.0,
            )
            .unwrap();
        if !component.wants_immediate_rerender() {
            break;
        }
    }

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let theme_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/slot:end/default-2::onThemeToggle",
    )
    .expect("rendered theme button");
    let button_center_x = theme_button.layout.x + theme_button.layout.width * 0.5;
    let button_center_y = theme_button.layout.y + theme_button.layout.height * 0.5;

    component
        .handle_input(
            &light,
            width,
            height,
            ComponentInput::PointerMove {
                x: button_center_x,
                y: button_center_y,
            },
        )
        .unwrap();
    component
        .handle_input(
            &light,
            width,
            height,
            ComponentInput::PointerButton {
                button: 0x110,
                x: button_center_x,
                y: button_center_y,
                pressed: true,
            },
        )
        .unwrap();
    component
        .paint(
            &light,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    assert!(
        !component.transitions.is_empty(),
        "pressing the theme button should leave transition state to invalidate"
    );

    // Nav controls are bare glyphs with no fill in any state, so the button
    // contributes no opaque pixel to sample. Its palette still has to follow
    // the theme, which the resolved colour shows without depending on where a
    // transition happens to be mid-flight.
    let light_button_color = first_node_with_click_handler(
        component.last_tree.as_ref().expect("tree after press"),
        "__mesh_embed__::@mesh/navigation-bar/slot:end/default-2::onThemeToggle",
    )
    .expect("theme button after press")
    .computed_style
    .color;

    component.theme_changed().unwrap();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "current": "mesh-default-dark",
                "theme_id": "mesh-default-dark",
                "is_dark": true
            }),
        })
        .unwrap();
    component
        .paint(
            &dark,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    // The shipped nav-shell background is translucent (rgba(10,10,14,0.36)),
    // so the immediate dark repaint resolves to this value rather than the old
    // opaque surface color. The point is that it repaints dark (not stale light).
    assert_eq!(
        buffer_pixel(&buffer, 8, 8),
        [5, 4, 4, 92],
        "already-painted navigation shell should repaint to dark surface immediately"
    );
    let dark_button_color = first_node_with_click_handler(
        component
            .last_tree
            .as_ref()
            .expect("tree after dark repaint"),
        "__mesh_embed__::@mesh/navigation-bar/slot:end/default-2::onThemeToggle",
    )
    .expect("theme button after dark repaint")
    .computed_style
    .color;
    assert_ne!(
        dark_button_color, light_button_color,
        "theme button should resolve against the dark palette, not stale light colors"
    );
}

/// A parent surface whose trigger cell embeds `@mesh/test-popover` (rendered from
/// `popover_src`) near the surface's right edge.
fn component_with_edge_popover(popover_src: &str) -> FrontendSurfaceComponent {
    use crate::shell::ComponentContext;
    use crate::shell::component::catalog::{FrontendCatalog, FrontendCatalogEntry};
    use mesh_core_component::parse_component;
    use mesh_core_frontend::CompiledFrontendModule;

    const PARENT: &str = "@test/popover-host";
    let mut parent_manifest = minimal_test_manifest(PARENT);
    parent_manifest.dependencies.modules.insert(
        "@mesh/test-popover".into(),
        mesh_core_module::manifest::DependencySpec::Simple(">=0.1.0".into()),
    );
    let mut popover_manifest = minimal_test_manifest("@mesh/test-popover");
    popover_manifest.package.module_type = mesh_core_module::ModuleType::Component;
    let parent = CompiledFrontendModule {
        manifest: parent_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(
            r#"
<template>
    <row class="bar">
        <box class="spacer" />
        <box class="cell" ref="cell"><EdgePopover /></box>
    </row>
</template>
<script lang="luau">
import EdgePopover from "@mesh/test-popover"
</script>
<style>
.bar { width: 200px; height: 30px; }
.spacer { flex-grow: 1; }
.cell { width: 24px; height: 24px; flex-shrink: 0; align-items: center; justify-content: center; }
</style>
"#,
        )
        .unwrap(),
        public_props: Default::default(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::from([("EdgePopover".into(), "@mesh/test-popover".into())]),
        watched_paths: Vec::new(),
    };
    let popover = CompiledFrontendModule {
        manifest: popover_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(popover_src).unwrap(),
        public_props: Default::default(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                PARENT.into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: parent.clone().into(),
                },
            ),
            (
                "@mesh/test-popover".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: popover.into(),
                },
            ),
        ]),
        diagnostics: Default::default(),
        extension_point_contributions: HashMap::new(),
        extension_point_entries: HashMap::new(),
        node_slot_placements: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new_for_test(
        parent,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: PARENT.into(),
            surface_id: PARENT.into(),
            diagnostics: mesh_core_diagnostics::Diagnostics::new(PARENT),
        })
        .unwrap();
    component.visible = true;
    let theme = mesh_core_theme::default_theme();
    let mut buffer = PixelBuffer::new(200, 30);
    component
        .paint(&theme, SurfaceExtent::unpadded(200, 30), &mut buffer, 1.0)
        .unwrap();
    component
}

#[test]
fn promoted_popover_keeps_its_intrinsic_size_beyond_the_parent_edge() {
    // The popover wrapper collapses to 0x0 in its trigger cell. Its promoted
    // root must still size to its own content: a fixed-width panel must not
    // shrink, and text must not wrap to the 0px the wrapper offers.
    let component = component_with_edge_popover(
        r#"
<template>
    <popover open="true" class="shell" anchor-ref="cell" anchor="bottom" gravity="bottom">
        <column class="panel"><text content="A fairly long popover title" /></column>
        <text class="note" content="Wide unwrapped explanatory text line" />
    </popover>
</template>
<style>
.shell { width: fit; height: fit; }
.panel { width: 300px; }
.note { text-wrap: wrap; }
</style>
"#,
    );
    let requests = component.child_surface_requests();
    let [request] = requests.as_slice() else {
        panic!("the open popover should author one child surface: {requests:?}");
    };
    let tree = component.last_tree.as_ref().expect("tree");
    let panel = first_node_by_class(tree, "panel").expect("panel");
    assert_eq!(panel.layout.width, 300.0, "a fixed-width panel must not shrink");
    let note = first_node_by_class(tree, "note").expect("note");
    assert!(
        note.layout.height < 2.0 * 22.0,
        "wrappable text must lay out on one line, got {}x{}",
        note.layout.width,
        note.layout.height
    );
    assert!(request.content_size.0 >= 300, "{:?}", request.content_size);
}
