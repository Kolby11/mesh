use super::*;

#[test]
fn keyboard_activation_focused_input_backspace_edits_value() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
input_seen = ""
function onInputChange(value)
    input_seen = value
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "input",
        "root/0",
        0.0,
        0.0,
        120.0,
        24.0,
        &[("change", "onInputChange")],
    )]));
    component.focused_key = Some("root/0".into());
    component.input_values.insert("root/0".into(), "ab".into());

    let theme = default_theme();
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
        component.input_values.get("root/0").map(String::as_str),
        Some("a")
    );
    assert_eq!(
        runtime_value(&component, "input_seen"),
        Some(serde_json::Value::String("a".into()))
    );
}

#[test]
fn keyboard_handlers_keydown_and_keyup_payloads_route_to_focused_node() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
keydown_key = ""
keydown_ctrl = false
keydown_target = ""
keydown_surface = ""
keyup_key = ""
keyup_shift = false

function onKeyDown(event)
    keydown_key = event.key
    keydown_ctrl = event.modifiers.ctrl
    keydown_target = event.current.key
    keydown_surface = event.surface.id
end

function onKeyUp(event)
    keyup_key = event.key
    keyup_shift = event.modifiers.shift
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("keydown", "onKeyDown"), ("keyup", "onKeyUp")],
    )]));
    component.focused_key = Some("root/0".into());

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Enter".into(),
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: false,
                    alt: false,
                },
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers {
                    ctrl: false,
                    shift: true,
                    alt: false,
                },
            },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "keydown_key"),
        Some(serde_json::Value::String("Enter".into()))
    );
    assert_eq!(
        runtime_value(&component, "keydown_target"),
        Some(serde_json::Value::String("root/0".into()))
    );
    assert_eq!(
        runtime_value(&component, "keydown_surface"),
        Some(serde_json::Value::String("@test/reactive-surface".into()))
    );
    assert!(runtime_bool(&component, "keydown_ctrl"));
    assert_eq!(
        runtime_value(&component, "keyup_key"),
        Some(serde_json::Value::String("Enter".into()))
    );
    assert!(runtime_bool(&component, "keyup_shift"));
}

#[test]
fn keyboard_handlers_ctrl_c_selection_still_wins_over_focused_button() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
button_count = 0
function onButtonClick()
    button_count = button_count + 1
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![
        text_node("root/0", 0.0, 0.0, 180.0, 40.0, true),
        event_node(
            "button",
            "root/1",
            0.0,
            48.0,
            40.0,
            24.0,
            &[("click", "onButtonClick")],
        ),
    ]));
    component.focused_key = Some("root/1".into());
    component.selection = Some(TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 0.0,
            y: 0.0,
        },
        focus: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 1000.0,
            y: 1000.0,
        },
        dragging: false,
    });

    let theme = default_theme();
    let requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "c".into(),
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: false,
                    alt: false,
                },
            },
        )
        .unwrap();

    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::WriteClipboard { text }] if text == "Selectable text"
    ));
    assert_eq!(runtime_number(&component, "button_count"), 0.0);
}

#[test]
fn keyboard_handlers_stale_focus_is_pruned_before_dispatch() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
keydown_count = 0
function onKeyDown()
    keydown_count = keydown_count + 1
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("keydown", "onKeyDown")],
    )]));
    component.focused_key = Some("root/missing".into());
    component.focus_visible_key = Some("root/missing".into());

    let theme = default_theme();
    let requests = component
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

    assert!(requests.is_empty());
    assert!(component.focused_key.is_none());
    assert!(component.focus_visible_key.is_none());
    assert_eq!(runtime_number(&component, "keydown_count"), 0.0);
}

#[test]
fn keyboard_shortcuts_surface_handler_runs_and_metadata_matches_binding() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
mute_count = 0
function onMuteShortcut()
    mute_count = mute_count + 1
end
</script>
"#,
    );
    component.settings_json = serde_json::json!({
        "keyboard": {
            "shortcuts": {
                "mute": {
                    "key": "m",
                    "handler": "onMuteShortcut",
                    "target_ref": "volume-button"
                }
            }
        }
    });
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("ref", "volume-button")],
        &[],
    )]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "m".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(runtime_number(&component, "mute_count"), 1.0);

    let mut tree = root_with(vec![event_node_with_attrs(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("ref", "volume-button")],
        &[],
    )]);
    annotate_runtime_tree(
        &mut tree,
        "root".to_string(),
        &None,
        &None,
        &[],
        &None,
        &None,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );
    component.annotate_surface_shortcuts(&mut tree);
    assert_eq!(
        node_by_mesh_key(&tree, "root/0")
            .accessibility
            .keyboard_shortcut
            .as_deref(),
        Some("m")
    );
}

#[test]
fn keyboard_shortcuts_manifest_keybind_action_resolves_user_override_by_action_id() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
mute_count = 0
function onMuteShortcut()
    mute_count = mute_count + 1
end
</script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            handler: "onMuteShortcut".into(),
            target_ref: Some("volume-button".into()),
            scope: mesh_core_module::KeybindScope::Surface,
            label: None,
            label_i18n_key: Some("nav.volume".into()),
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
        },
    );
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("ref", "volume-button")],
        &[],
    )]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "m".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(runtime_number(&component, "mute_count"), 1.0);

    let keyboard_settings = mesh_core_config::KeyboardSettings {
        surface_shortcuts: HashMap::from([(
            "@test/reactive-surface".into(),
            HashMap::from([(
                "mute".into(),
                mesh_core_config::SurfaceShortcutOverride {
                    key: Some("u".into()),
                },
            )]),
        )]),
        ..mesh_core_config::KeyboardSettings::default()
    };
    let resolved = component.resolved_surface_shortcuts(&keyboard_settings);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].action_id, "mute");
    assert_eq!(resolved[0].key, "u");
    assert_eq!(resolved[0].handler, "onMuteShortcut");
    assert_eq!(resolved[0].target_ref.as_deref(), Some("volume-button"));
}

#[test]
fn navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component.paint(&theme, width, height, &mut buffer).unwrap();
    let shortcut_requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyPressed {
                key: "m".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(matches!(
        shortcut_requests.as_slice(),
        [CoreRequest::ServiceCommand { interface, command, payload, .. }]
            if interface == "mesh.audio"
                && command == "set_muted"
                && payload["device_id"] == serde_json::json!("default")
                && payload["muted"] == serde_json::json!(true)
    ));

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let theme_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeToggle",
    )
    .expect("rendered theme button");
    let theme_key = theme_button
        .attributes
        .get("_mesh_key")
        .expect("theme button mesh key")
        .clone();
    component.focused_key = Some(theme_key.clone());
    component.focus_visible_key = Some(theme_key);

    let activation_requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(matches!(
        activation_requests.as_slice(),
        [CoreRequest::SetTheme { theme_id }] if theme_id == "mesh-default-light"
    ));

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
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let activation_requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(matches!(
        activation_requests.as_slice(),
        [CoreRequest::SetTheme { theme_id }] if theme_id == "mesh-default-dark"
    ));
}

#[test]
fn navigation_buttons_animate_shape_from_squircle_to_circle_with_transform() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);

    component.paint(&theme, width, height, &mut buffer).unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeToggle",
    )
    .expect("navigation theme button");
    let button_key = button
        .attributes
        .get("_mesh_key")
        .expect("theme button mesh key")
        .clone();
    let nav_shell = first_node_with_attr(tree, "class", "nav-shell").expect("navigation shell");
    let control_cluster =
        first_node_with_attr(tree, "ref", "control-cluster").expect("control cluster");
    assert!(
        nav_shell.computed_style.background_color.a > 0,
        "navigation shell should resolve a nontransparent background"
    );
    let shell_center_alpha = alpha_at(
        &buffer,
        (nav_shell.layout.x + nav_shell.layout.width * 0.5).round() as u32,
        (nav_shell.layout.y + nav_shell.layout.height * 0.5).round() as u32,
    );
    assert!(
        shell_center_alpha > 0,
        "navigation shell center should paint an opaque background"
    );
    assert!(
        ((control_cluster.layout.y + control_cluster.layout.height * 0.5)
            - (nav_shell.layout.y + nav_shell.layout.height * 0.5))
            .abs()
            <= 1.0,
        "navigation control cluster should be vertically centered in the shell"
    );
    let button_key_for_bounds = button
        .attributes
        .get("_mesh_key")
        .expect("theme button mesh key");
    let (_button_left, button_top, _button_right, button_bottom) =
        find_node_bounds_by_key(tree, button_key_for_bounds, 0.0, 0.0).expect("button bounds");
    assert!(
        (((button_top + button_bottom) * 0.5)
            - (nav_shell.layout.y + nav_shell.layout.height * 0.5))
            .abs()
            <= 1.0,
        "navigation button should be vertically centered in the shell"
    );
    assert_eq!(button.computed_style.border_radius.top_left, 16.0);
    assert_eq!(button.computed_style.transform.scale_x, 1.0);
    assert_eq!(button.computed_style.transform.scale_y, 1.0);
    let visible_pixels = nontransparent_pixels(&buffer);
    assert!(
        visible_pixels > 40_000,
        "navigation bar should paint visible Skia backgrounds, got only {visible_pixels} nontransparent pixels"
    );
    assert!(button.computed_style.transition.duration_ms > 0);
    assert!(
        button
            .computed_style
            .transition
            .properties
            .animates_border_radius()
    );
    assert!(
        button
            .computed_style
            .transition
            .properties
            .animates_transform()
    );

    let hover_x = button.layout.x + button.layout.width * 0.5;
    let hover_y = button.layout.y + button.layout.height * 0.5;
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerMove {
                x: hover_x,
                y: hover_y,
            },
        )
        .unwrap();
    component.paint(&theme, width, height, &mut buffer).unwrap();
    let hovered_tree = component
        .last_tree
        .as_ref()
        .expect("hovered navigation tree");
    let hovered_button = node_by_mesh_key(hovered_tree, &button_key);

    assert_eq!(hovered_button.computed_style.border_radius.top_left, 9999.0);
    assert_eq!(hovered_button.computed_style.transform.translate_y, -1.0);
    assert!((hovered_button.computed_style.transform.scale_x - 1.04).abs() < 0.001);
    assert!((hovered_button.computed_style.transform.scale_y - 1.04).abs() < 0.001);
    let hovered_visible_pixels = nontransparent_pixels(&buffer);
    assert!(
        hovered_visible_pixels > 40_000,
        "hover repaint should preserve visible Skia backgrounds, got only {hovered_visible_pixels} nontransparent pixels"
    );
    let center_alpha = alpha_at(&buffer, hover_x.round() as u32, hover_y.round() as u32);
    assert!(
        center_alpha > 0,
        "hovered navigation button center should remain visible after transition repaint"
    );
}

fn nontransparent_pixels(buffer: &PixelBuffer) -> usize {
    buffer
        .data
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .count()
}

fn alpha_at(buffer: &PixelBuffer, x: u32, y: u32) -> u8 {
    let x = x.min(buffer.width.saturating_sub(1));
    let y = y.min(buffer.height.saturating_sub(1));
    let offset = (y * buffer.stride + x * 4) as usize;
    buffer.data[offset + 3]
}

#[test]
fn navigation_bar_pointer_click_updates_real_surface_focus_diagnostic() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let settings_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:SettingsButton::onSettingsClick",
    )
    .expect("rendered settings button");
    let settings_key = settings_button
        .attributes
        .get("_mesh_key")
        .expect("settings button mesh key")
        .clone();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &settings_key, 0.0, 0.0).expect("settings bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;
    component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: true,
            },
        )
        .unwrap();

    assert_eq!(
        component.focused_key.as_deref(),
        Some(settings_key.as_str())
    );
}

#[test]
fn navigation_bar_real_surface_keeps_status_copy_non_selectable() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
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

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(420, 80);
    component.paint(&theme, 420, 80, &mut buffer).unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    assert_eq!(
        count_selectable_text_nodes(tree),
        0,
        "the shipped nav bar should not expose selectable passive text nodes"
    );

    let status_primary =
        first_node_with_attr(tree, "ref", "status-primary").expect("status-primary text node");
    assert_eq!(status_primary.tag, "text");
    assert_eq!(
        status_primary.attributes.get("content").map(String::as_str),
        Some("Shell surface active")
    );
}

#[test]
fn navigation_bar_keyboard_activation_opens_volume_surface_on_real_surface() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .attributes
        .get("_mesh_key")
        .expect("volume button mesh key")
        .clone();

    component.focused_key = Some(volume_key.clone());
    component.focus_visible_key = Some(volume_key);

    let requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ActivatePopover { surface_id, focus, .. }
                if surface_id == "@mesh/audio-popover" && *focus
        )),
        "Enter on the focused volume button should activate the audio popover: {requests:?}"
    );
}

#[test]
fn navigation_bar_pointer_activation_opens_volume_surface_without_stealing_focus() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .attributes
        .get("_mesh_key")
        .expect("volume button mesh key")
        .clone();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;

    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: true,
            },
        )
        .unwrap();
    let requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: false,
            },
        )
        .unwrap();

    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ActivatePopover { surface_id, focus, .. }
                if surface_id == "@mesh/audio-popover" && !*focus
        )),
        "pointer click should show/register the popover without transferring focus: {requests:?}"
    );
}

#[test]
fn navigation_bar_same_hover_volume_trigger_closes_popover_immediately() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component.paint(&theme, width, height, &mut buffer).unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .attributes
        .get("_mesh_key")
        .expect("volume button mesh key")
        .clone();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;

    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: true,
            },
        )
        .unwrap();
    let open_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: false,
            },
        )
        .unwrap();
    assert!(
        open_requests.iter().any(|request| matches!(
            request,
            CoreRequest::ActivatePopover { surface_id, .. }
                if surface_id == "@mesh/audio-popover"
        )),
        "first click should open the audio popover: {open_requests:?}"
    );

    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: true,
            },
        )
        .unwrap();
    let close_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: false,
            },
        )
        .unwrap();
    assert!(
        close_requests.iter().any(|request| matches!(
            request,
            CoreRequest::HideSurface { surface_id } if surface_id == "@mesh/audio-popover"
        )),
        "second click at the same hovered coordinates should hide immediately: {close_requests:?}"
    );
}

#[test]
fn navigation_bar_keyboard_audio_popover_slider_responds_to_arrow_keys() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
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

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(320, 220);
    component.paint(&theme, 320, 220, &mut buffer).unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered audio popover");
    let slider = first_node_by_tag(tree, "slider").expect("slider node");
    let slider_key = slider
        .attributes
        .get("_mesh_key")
        .expect("slider key")
        .clone();
    component.focused_key = Some(slider_key);

    let requests = component
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::KeyPressed {
                key: "ArrowRight".into(),
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
            assert_eq!(payload["device_id"], serde_json::json!("default"));
            let volume = payload["volume"].as_f64().expect("numeric volume payload");
            assert!(
                (volume - 0.55).abs() < 0.001,
                "expected slider keyboard step near 0.55, got {volume}"
            );
        }
        other => panic!("expected one audio set_volume request, got {other:?}"),
    }
}

#[test]
fn navigation_bar_compact_width_hides_secondary_status_before_controls() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 58,
                "muted": false
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut wide_buffer = PixelBuffer::new(920, 80);
    component.paint(&theme, 920, 80, &mut wide_buffer).unwrap();
    let wide_tree = component.last_tree.as_ref().expect("wide navigation tree");
    let mut wide_text = Vec::new();
    collect_text_content(wide_tree, &mut wide_text);
    assert!(
        wide_text
            .iter()
            .any(|content| content == "Audio steady at 58%"),
        "wide nav bar should show secondary audio status text: {wide_text:?}"
    );
    assert!(
        count_tag(wide_tree, "button") >= 3,
        "wide nav bar should retain the three primary controls"
    );

    let mut compact_buffer = PixelBuffer::new(240, 80);
    component
        .paint(&theme, 240, 80, &mut compact_buffer)
        .unwrap();
    let compact_tree = component
        .last_tree
        .as_ref()
        .expect("compact navigation tree");
    let mut compact_text = Vec::new();
    collect_text_content(compact_tree, &mut compact_text);
    let compact_secondary = first_node_with_attr(compact_tree, "class", "status-secondary")
        .expect("compact secondary status node");
    assert!(
        compact_secondary.computed_style.display == Display::None,
        "compact nav bar should hide the secondary status node before controls"
    );
    assert!(
        count_tag(compact_tree, "button") >= 3,
        "compact nav bar must keep the primary controls available"
    );
}
