use super::*;

#[test]
fn immediate_rerender_preserves_present_damage_from_first_pass() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box>
    <text>{theme_label}</text>
  </box>
</template>
<script lang="luau">
theme_label = "dark"

local theme_ok, theme = pcall(function()
    return require("mesh.theme")
end)
if not theme_ok then theme = nil end

function onRender()
    if theme and theme.is_dark == false then
        theme_label = "light"
    else
        theme_label = "dark"
    end
end
</script>
<style>
box {
  width: 140px;
  height: 40px;
  background-color: token(color.surface-container);
}
text {
  width: 100px;
  height: 24px;
  color: token(color.on-surface);
}
</style>
"#,
        audio_network_catalog(),
        &["theme.read"],
    );
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
    let mut buffer = PixelBuffer::new(160, 48);

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
    for _ in 0..2 {
        component.paint(&dark, 160, 48, &mut buffer).unwrap();
        if !component.wants_immediate_rerender() {
            break;
        }
    }
    component.take_present_damage();

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
    component.paint(&light, 160, 48, &mut buffer).unwrap();
    assert!(
        component.wants_immediate_rerender(),
        "onRender state sync should request the same-frame rerender that used to erase damage"
    );
    component.paint(&light, 160, 48, &mut buffer).unwrap();

    assert!(
        !component.take_present_damage().is_empty(),
        "same-frame rerender must preserve first-pass damage so the shell still presents"
    );
}

#[test]
fn navigation_volume_slider_repaints_after_consecutive_drag_paints() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <slider min="0" max="1" value="{slider_value}" onchange={onVolumeChange} />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end

slider_value = 0.0

local function clamp_volume(value)
    local numeric = tonumber(value) or 0
    if numeric < 0 then return 0.0 end
    if numeric > 1 then return 1.0 end
    return numeric
end

function onVolumeChange(value)
    local normalized = clamp_volume(value)
    slider_value = normalized
    if audio_ok and audio then
        audio.set_volume("default", normalized)
    end
end
</script>
<style>
slider {
  width: 220px;
  height: 40px;
  color: #ffffff;
}
</style>
"#,
        audio_network_catalog(),
        &["service.audio.read", "service.audio.control"],
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);

    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    component
        .handle_input(
            &theme,
            240,
            40,
            ComponentInput::PointerButton {
                x: 200.0,
                y: 20.0,
                pressed: true,
            },
        )
        .unwrap();
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    let after_first_drag = buffer.data.clone();

    component
        .handle_input(
            &theme,
            240,
            40,
            ComponentInput::PointerMove { x: 60.0, y: 20.0 },
        )
        .unwrap();
    component.paint(&theme, 240, 40, &mut buffer).unwrap();

    assert_ne!(
        buffer.data, after_first_drag,
        "subsequent drag paints should track the latest slider position"
    );
}

#[test]
fn audio_popover_keeps_drag_value_visible_until_backend_catches_up() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let width = 280;
    let height = 180;
    let mut buffer = PixelBuffer::new(width, height);

    component.paint(&theme, width, height, &mut buffer).unwrap();
    let requests = component
        .call_namespaced_handler("onToggleMute", &[])
        .unwrap();
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { command, payload, .. }
                if command == "set_muted"
                    && payload["device_id"] == serde_json::json!("default")
                    && payload["muted"] == serde_json::json!(true)
        )),
        "mute action should dispatch when the audio proxy exists but the first state payload has not arrived: {requests:?}"
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 20,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let slider = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .expect("audio popover slider");
    let drag_x = slider.layout.x + slider.layout.width * 0.8;
    let drag_y = slider.layout.y + slider.layout.height * 0.5;
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: true,
            },
        )
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 20,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let rendered_value = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .and_then(|slider| slider.attributes.get("value"))
        .and_then(|value| value.parse::<f32>().ok())
        .expect("painted slider value");
    assert!(
        rendered_value > 0.7,
        "in-flight slider drag should stay visible instead of snapping back to stale backend value, got {rendered_value}"
    );
}

#[test]
fn audio_popover_first_slider_grab_dispatches_change() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let width = 280;
    let height = 180;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 20,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let slider = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .expect("audio popover slider");
    let drag_x = slider.layout.x + slider.layout.width * 0.7;
    let drag_y = slider.layout.y + slider.layout.height * 0.5;
    let requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: true,
            },
        )
        .unwrap();

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
            let volume = payload["volume"].as_f64().expect("numeric volume payload");
            assert!(
                (volume - 0.7).abs() < 0.03,
                "first slider grab should dispatch the grabbed value, got {volume}"
            );
        }
        other => panic!("expected one set_volume request on first grab, got {other:?}"),
    }
}

#[test]
fn audio_popover_drag_keeps_fractional_slider_value_visible() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let width = 280;
    let height = 180;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 20,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let slider = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .expect("audio popover slider");
    let drag_x = slider.layout.x + slider.layout.width * 0.735;
    let drag_y = slider.layout.y + slider.layout.height * 0.5;
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: true,
            },
        )
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let rendered_value = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .and_then(|slider| slider.attributes.get("value"))
        .and_then(|value| value.parse::<f32>().ok())
        .expect("painted slider value");
    assert!(
        (rendered_value - 0.735).abs() < 0.01,
        "drag should keep the fractional slider position visible, got {rendered_value}"
    );
}

#[test]
fn audio_popover_button_volume_updates_slider_after_drag() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let width = 280;
    let height = 180;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let slider = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .expect("audio popover slider");
    let drag_x = slider.layout.x + slider.layout.width * 0.8;
    let drag_y = slider.layout.y + slider.layout.height * 0.5;
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: false,
            },
        )
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let requests = component
        .call_namespaced_handler("onVolumeDown", &[])
        .unwrap();
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { command, payload, .. }
                if command == "set_volume"
                    && (payload["volume"].as_f64().unwrap_or_default() - 0.75).abs() < 0.03
        )),
        "volume-down button should send precise set_volume after drag: {requests:?}"
    );
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let rendered_value = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .and_then(|slider| slider.attributes.get("value"))
        .and_then(|value| value.parse::<f32>().ok())
        .expect("painted slider value");
    assert!(
        (rendered_value - 0.75).abs() < 0.03,
        "button volume change should move visible slider after drag, got {rendered_value}"
    );
}

#[test]
fn audio_popover_backend_update_moves_slider_after_preserved_value_clears() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let width = 280;
    let height = 180;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let slider = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .expect("audio popover slider");
    let drag_x = slider.layout.x + slider.layout.width * 0.8;
    let drag_y = slider.layout.y + slider.layout.height * 0.5;
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: false,
            },
        )
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 80,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 35,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let rendered_value = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .and_then(|slider| slider.attributes.get("value"))
        .and_then(|value| value.parse::<f32>().ok())
        .expect("painted slider value");
    assert!(
        (rendered_value - 0.35).abs() < 0.03,
        "backend update should move visible slider after preserved state clears, got {rendered_value}"
    );
}

#[test]
fn audio_popover_mute_renders_shell_normalized_state() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let width = 280;
    let height = 180;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 42,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let requests = component
        .call_namespaced_handler("onToggleMute", &[])
        .unwrap();
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { command, payload, .. }
                if command == "set_muted"
                    && payload["device_id"] == serde_json::json!("default")
                    && payload["muted"] == serde_json::json!(true)
        )),
        "mute action should dispatch idempotent set_muted(true): {requests:?}"
    );
    component.paint(&theme, width, height, &mut buffer).unwrap();
    let pre_optimistic_text = rendered_text(&component);
    assert!(
        pre_optimistic_text.iter().any(|text| text == "Mute"),
        "popover should wait for shell-normalized optimistic service state: {pre_optimistic_text:?}"
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 42,
                "muted": true
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();
    let optimistic_text = rendered_text(&component);
    assert!(
        optimistic_text.iter().any(|text| text == "Unmute"),
        "shell-normalized mute state should drive popover text: {optimistic_text:?}"
    );

    let requests = component
        .call_namespaced_handler("onToggleMute", &[])
        .unwrap();
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { command, payload, .. }
                if command == "set_muted"
                    && payload["device_id"] == serde_json::json!("default")
                    && payload["muted"] == serde_json::json!(false)
        )),
        "second mute action should dispatch idempotent set_muted(false): {requests:?}"
    );
    component.paint(&theme, width, height, &mut buffer).unwrap();
    let pre_unmute_text = rendered_text(&component);
    assert!(
        pre_unmute_text.iter().any(|text| text == "Unmute"),
        "popover should keep rendering canonical muted state until shell optimistic update arrives: {pre_unmute_text:?}"
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 42,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();
    let optimistic_unmute_text = rendered_text(&component);
    assert!(
        optimistic_unmute_text.iter().any(|text| text == "Mute"),
        "shell-normalized unmute state should drive popover text: {optimistic_unmute_text:?}"
    );
}

#[test]
fn audio_popover_slider_keyboard_still_steps_after_mouse_drag() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let width = 280;
    let height = 180;
    let mut buffer = PixelBuffer::new(width, height);

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let slider = first_node_by_tag(component.last_tree.as_ref().unwrap(), "slider")
        .expect("audio popover slider");
    let drag_x = slider.layout.x + slider.layout.width * 0.8;
    let drag_y = slider.layout.y + slider.layout.height * 0.5;
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: drag_x,
                y: drag_y,
                pressed: false,
            },
        )
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyPressed {
                key: "ArrowDown".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

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
            let volume = payload["volume"].as_f64().expect("numeric volume payload");
            assert!(
                (volume - 0.75).abs() < 0.001,
                "keyboard step after mouse drag should decrement from the drag value, got {volume}"
            );
        }
        other => panic!("expected one audio set_volume request, got {other:?}"),
    }
}

#[test]
fn text_input_change_handler_receives_current_string() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
text_seen = ""
function onTextChange(value)
    text_seen = value
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "input",
        "root/0",
        0.0,
        0.0,
        100.0,
        24.0,
        &[("change", "onTextChange")],
    )]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 4.0,
                y: 4.0,
                pressed: true,
            },
        )
        .unwrap();
    component
        .handle_input(&theme, 240, 160, ComponentInput::Char { ch: 'A' })
        .unwrap();

    assert_eq!(
        runtime_value(&component, "text_seen"),
        Some(serde_json::json!("A"))
    );
}

#[test]
fn switch_change_handler_receives_boolean_on_click() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
switch_seen = false
function onSwitchChange(value)
    switch_seen = value
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "switch",
        "root/0",
        0.0,
        0.0,
        48.0,
        24.0,
        &[("change", "onSwitchChange")],
    )]));

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

    assert_eq!(
        runtime_value(&component, "switch_seen"),
        Some(serde_json::json!(true))
    );
}

#[test]
fn slider_release_handler_fires_once_with_current_number() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
release_count = 0
released_value = -1
function onSliderRelease(value)
    release_count = release_count + 1
    released_value = value
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
        &[("release", "onSliderRelease")],
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
                x: 10.0,
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
            ComponentInput::PointerMove { x: 60.0, y: 10.0 },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 60.0,
                y: 10.0,
                pressed: false,
            },
        )
        .unwrap();

    assert_eq!(runtime_number(&component, "release_count"), 1.0);
    assert!((runtime_number(&component, "released_value") - 0.6).abs() < 0.001);
}

#[test]
fn click_handler_keeps_current_target_position_payload() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
click_left = -1
click_bottom = -1
function onButtonClick(event)
    click_left = event.current_target.position.margin_left
    click_bottom = event.current_target.position.margin_bottom
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        32.0,
        4.0,
        80.0,
        24.0,
        &[("click", "onButtonClick")],
    )]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 40.0,
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
            ComponentInput::PointerButton {
                x: 40.0,
                y: 10.0,
                pressed: false,
            },
        )
        .unwrap();

    assert_eq!(runtime_number(&component, "click_left"), 32.0);
    assert_eq!(runtime_number(&component, "click_bottom"), 28.0);
}

#[test]
fn pointer_release_without_requests_still_clears_active_state() {
    let mut component =
        test_frontend_component("<template><button class=\"pressable\" /></template>");
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        48.0,
        24.0,
        &[],
    )]));

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

    assert!(component.wants_render(), "press should dirty the component");
    component.dirty = false;

    let release_requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 8.0,
                y: 8.0,
                pressed: false,
            },
        )
        .unwrap();

    assert!(
        release_requests.is_empty(),
        "plain button release should not synthesize service requests"
    );
    assert!(
        component.wants_render(),
        "release must dirty the component so :active styling is cleared"
    );
    assert!(component.pointer_down_key.is_none());
    assert!(component.active_slider_key.is_none());
}

#[test]
fn focus_handler_fires_when_node_becomes_focused() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
focus_count = 0
function onInputFocus()
    focus_count = focus_count + 1
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "input",
        "root/0",
        0.0,
        0.0,
        100.0,
        24.0,
        &[("focus", "onInputFocus")],
    )]));

    let theme = default_theme();
    for _ in 0..2 {
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
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 8.0,
                    y: 8.0,
                    pressed: false,
                },
            )
            .unwrap();
    }

    assert_eq!(runtime_number(&component, "focus_count"), 1.0);
}
