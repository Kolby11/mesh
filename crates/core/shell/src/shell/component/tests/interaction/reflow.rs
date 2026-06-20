use super::*;

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
    component.input_values.insert("root/0".into(), "ab".into());
    let theme = default_theme();

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 8.0,
                y: 8.0,
                pressed: true,
            },
        )
        .unwrap();
    assert_eq!(component.focus_visible_key.as_deref(), Some("root/0"));

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
function onRender()
    render_count = render_count + 1
end
</script>
"#,
    );
    let runtime_count_before = component.runtimes.lock().unwrap().len();
    component
        .input_values
        .insert("root/0/0".into(), "local".into());
    component.checked_values.insert("root/0/1".into(), true);
    component.focused_key = Some("root/0/0".into());

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 120);
    component.paint(&theme, 240, 120, &mut buffer, 1.0).unwrap();
    let render_count_after_first = runtime_number(&component, "render_count");
    let runtime_count_after_first = component.runtimes.lock().unwrap().len();

    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/1".into()];
    component.hovered_key = Some("root/0/1".into());
    component.dirty = true;
    component.paint(&theme, 240, 120, &mut buffer, 1.0).unwrap();

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
function onRender()
    render_count = render_count + 1
end
</script>
"#,
    );
    component.surface_layout.width = 0;
    component.surface_layout.height = 0;
    component
        .input_values
        .insert("root/0/0".into(), "local".into());
    component.slider_values.insert("root/0/1".into(), 73.0);
    component.checked_values.insert("root/0/2".into(), true);
    component
        .scroll_offsets
        .insert("root/0/3".into(), ScrollOffsetState { x: 3.0, y: 14.0 });

    let theme = default_theme();
    let mut wide_buffer = PixelBuffer::new(420, 160);
    component
        .paint(&theme, 420, 160, &mut wide_buffer, 1.0)
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
        .paint(&theme, 260, 160, &mut narrow_buffer, 1.0)
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
    assert_eq!(
        node_by_mesh_key(narrow_tree, "root/0/3")
            .attributes
            .get("_mesh_scroll_y")
            .map(String::as_str),
        Some("14.00")
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

function onRender()
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
        audio.set_volume("default", normalized)
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
        runtime.script_ctx.call_handler("onRender", &[]).unwrap();
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
            },
        ] => {
            assert_eq!(interface, "mesh.audio");
            assert_eq!(command, "set_volume");
            assert_eq!(
                payload,
                &serde_json::json!({ "device_id": "default", "volume": 0.5 })
            );
        }
        other => panic!("expected one mesh.audio.set_volume request, got {other:?}"),
    }

    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer, 1.0).unwrap();
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

    component.paint(&theme, 240, 40, &mut buffer, 1.0).unwrap();
    let initial = buffer.data.clone();

    component
        .handle_input(
            &theme,
            240,
            40,
            ComponentInput::PointerButton {
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
    component.paint(&theme, 240, 40, &mut buffer, 1.0).unwrap();
    let after_first_drag = buffer.data.clone();

    component
        .handle_input(
            &theme,
            240,
            40,
            ComponentInput::PointerMove { x: 60.0, y: 20.0 },
        )
        .unwrap();
    component.paint(&theme, 240, 40, &mut buffer, 1.0).unwrap();
    let after_second_drag = buffer.data.clone();

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
  background-color: token(color.primary);
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

    component.paint(&dark, 64, 32, &mut buffer, 1.0).unwrap();
    let dark_pixel = buffer_pixel(&buffer, 12, 12);

    component.theme_changed().unwrap();
    component.paint(&light, 64, 32, &mut buffer, 1.0).unwrap();
    let light_pixel = buffer_pixel(&buffer, 12, 12);

    assert_ne!(dark_pixel, light_pixel);
    assert_eq!(light_pixel, [0xee, 0xff, 0xc0, 0xff]);
}

#[test]
fn real_navigation_bar_repaints_when_theme_changes() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let dark = default_theme();
    let mut light = default_theme();
    light.id = "mesh-default-light".into();
    light.name = "mesh-default-light".into();
    light.tokens.insert(
        "color.surface-container".into(),
        mesh_core_theme::TokenValue::String("#f0f0f0".into()),
    );
    light.tokens.insert(
        "color.on-surface".into(),
        mesh_core_theme::TokenValue::String("#111111".into()),
    );
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .paint(&dark, width, height, &mut buffer, 1.0)
        .unwrap();
    let dark_snapshot = buffer.data.clone();

    component.theme_changed().unwrap();
    component
        .paint(&light, width, height, &mut buffer, 1.0)
        .unwrap();

    assert_ne!(
        buffer.data, dark_snapshot,
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
        .paint(&dark, width, height, &mut buffer, 1.0)
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
            .paint(&light, width, height, &mut buffer, 1.0)
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
        "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeToggle",
    )
    .expect("rendered theme button");
    let button_sample_x = (theme_button.layout.x + 6.0).round() as u32;
    let button_sample_y = (theme_button.layout.y + 6.0).round() as u32;

    component
        .handle_input(
            &light,
            width,
            height,
            ComponentInput::PointerMove {
                x: theme_button.layout.x + theme_button.layout.width * 0.5,
                y: theme_button.layout.y + theme_button.layout.height * 0.5,
            },
        )
        .unwrap();
    component
        .paint(&light, width, height, &mut buffer, 1.0)
        .unwrap();
    assert!(
        !component.transitions.is_empty(),
        "hovering the theme button should leave transition state to invalidate"
    );

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
        .paint(&dark, width, height, &mut buffer, 1.0)
        .unwrap();

    assert_eq!(
        buffer_pixel(&buffer, 8, 8),
        [0x1f, 0x1b, 0x1c, 0xff],
        "already-painted navigation shell should repaint to dark surface immediately"
    );
    assert_eq!(
        buffer_pixel(&buffer, button_sample_x, button_sample_y),
        [0x58, 0x44, 0x4a, 0xff],
        "theme button hover state should repaint with the dark hover palette, not stale light colors"
    );
}
