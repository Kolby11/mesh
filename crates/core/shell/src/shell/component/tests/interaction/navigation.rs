use super::*;
use mesh_core_frontend_host::ShellComponent;

fn rect_matches_bounds(rect: (i32, i32, i32, i32), bounds: (f32, f32, f32, f32)) -> bool {
    let left = bounds.0.floor() as i32;
    let top = bounds.1.floor() as i32;
    let right = bounds.2.ceil() as i32;
    let bottom = bounds.3.ceil() as i32;
    rect == (left, top, (right - left).max(1), (bottom - top).max(1))
}

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
fn hovered_target_is_interactive_for_clickable_ancestor_label() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    let mut button = event_node(
        "button",
        "root/0",
        10.0,
        10.0,
        80.0,
        28.0,
        &[("click", "onTap")],
    );
    button
        .children
        .push(event_node("text", "root/0/0", 20.0, 14.0, 12.0, 12.0, &[]));
    component.last_tree = Some(root_with(vec![button]));
    let theme = default_theme();

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 24.0, y: 18.0 },
        )
        .unwrap();

    assert!(
        component.hovered_target_is_interactive(),
        "hovering a text label inside a clickable button should request an interactive cursor"
    );
}

#[test]
fn phase88_source_variant_input_dispatches_input_and_change_handlers() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
input_seen = ""
change_seen = ""
function onInput(value)
    input_seen = value
end
function onChange(value)
    change_seen = value
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "input",
        "root/0",
        0.0,
        0.0,
        120.0,
        24.0,
        &[("data-mesh-element", "search"), ("type", "search")],
        &[("input", "onInput"), ("change", "onChange")],
    )]));
    component.focused_key = Some("root/0".into());
    component.input_values.insert("root/0".into(), "me".into());

    let theme = default_theme();
    component
        .handle_input(&theme, 240, 160, ComponentInput::Char { ch: 's' })
        .unwrap();

    assert_eq!(
        component.input_values.get("root/0").map(String::as_str),
        Some("mes")
    );
    assert_eq!(
        runtime_value(&component, "input_seen"),
        Some(serde_json::Value::String("mes".into()))
    );
    assert_eq!(
        runtime_value(&component, "change_seen"),
        Some(serde_json::Value::String("mes".into()))
    );
}

#[test]
fn phase89_option_activation_dispatches_parent_select_change() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
selected_locale = ""
function onLocaleChange(value)
    selected_locale = value
end
</script>
"#,
    );
    let mut select = event_node_with_attrs(
        "input",
        "root/0",
        0.0,
        0.0,
        120.0,
        64.0,
        &[("data-mesh-element", "select"), ("value", "en")],
        &[("change", "onLocaleChange")],
    );
    select.children.push(event_node_with_attrs(
        "input",
        "root/0/0",
        0.0,
        24.0,
        120.0,
        20.0,
        &[("data-mesh-element", "option"), ("value", "en")],
        &[],
    ));
    select.children.push(event_node_with_attrs(
        "input",
        "root/0/1",
        0.0,
        44.0,
        120.0,
        20.0,
        &[("data-mesh-element", "option"), ("value", "sk")],
        &[],
    ));
    component.last_tree = Some(root_with(vec![select]));
    component.focused_key = Some("root/0/1".into());

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "selected_locale"),
        Some(serde_json::Value::String("sk".into()))
    );
    assert_eq!(
        component.input_values.get("root/0").map(String::as_str),
        Some("sk")
    );
}

#[test]
fn phase89_menu_item_activation_uses_activate_handler() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
activated = false
function onActivate()
    activated = true
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "row",
        "root/0",
        0.0,
        0.0,
        120.0,
        24.0,
        &[("data-mesh-element", "menu-item")],
        &[("activate", "onActivate")],
    )]));
    component.focused_key = Some("root/0".into());

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "activated"),
        Some(serde_json::Value::Bool(true))
    );
}

#[test]
fn phase90_tab_and_list_item_keyboard_activation_use_activate_handler() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
activated = ""
function onTab()
    activated = "tab"
end
function onListItem()
    activated = "list-item"
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![
        event_node_with_attrs(
            "box",
            "root/0",
            0.0,
            0.0,
            120.0,
            24.0,
            &[("data-mesh-element", "tab")],
            &[("activate", "onTab")],
        ),
        event_node_with_attrs(
            "row",
            "root/1",
            0.0,
            32.0,
            120.0,
            24.0,
            &[("data-mesh-element", "list-item")],
            &[("activate", "onListItem")],
        ),
    ]));
    component.focused_key = Some("root/0".into());

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "activated"),
        Some(serde_json::Value::String("tab".into()))
    );

    component.focused_key = Some("root/1".into());
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "activated"),
        Some(serde_json::Value::String("list-item".into()))
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
    // Surface shortcuts resolve from `mesh.keybinds` declarations; the legacy
    // `settings.keyboard.shortcuts` form is migration-only and no longer
    // dispatches (it only records a diagnostic).
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("keybind", "mute")],
        &[("keybind", "onMuteShortcut")],
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
        &[("keybind", "mute")],
        &[("keybind", "onMuteShortcut")],
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
fn keyboard_shortcuts_manifest_keybind_subscriber_resolves_user_override_by_id() {
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
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("keybind", "mute")],
        &[("keybind", "onMuteShortcut")],
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
    assert_eq!(resolved[0].keybind_id, "mute");
    assert_eq!(resolved[0].key, "u");
    assert_eq!(
        resolved[0].trigger_kind,
        mesh_core_module::KeybindTriggerKind::Shortcut
    );
    assert_eq!(resolved[0].source, KeybindResolutionSource::UserOverride);
    let tree = component.last_tree.as_ref().unwrap();
    let subscribers = component.keybind_subscribers(tree);
    assert_eq!(subscribers.len(), 1);
    assert_eq!(subscribers[0].keybind_id, "mute");
    assert_eq!(subscribers[0].handler, "onMuteShortcut");
}

#[test]
fn keyboard_shortcuts_manifest_keybind_requires_declared_modifiers() {
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
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: vec!["ctrl".into()],
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("keybind", "mute")],
        &[("keybind", "onMuteShortcut")],
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
    assert_eq!(runtime_number(&component, "mute_count"), 0.0);

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "m".into(),
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: false,
                    alt: false,
                },
            },
        )
        .unwrap();
    assert_eq!(runtime_number(&component, "mute_count"), 1.0);

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());
    assert_eq!(resolved[0].modifiers, vec!["ctrl".to_string()]);

    let mut tree = root_with(vec![event_node_with_attrs(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("keybind", "mute")],
        &[("keybind", "onMuteShortcut")],
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
        Some("Control+m")
    );
}

#[test]
fn keyboard_shortcuts_manifest_keybind_dispatches_only_to_runtime_subscribers() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
mute_count = 0
keydown_count = 0
keydown_key = ""

function onMuteShortcut()
    mute_count = mute_count + 1
end

function onKeyDown(event)
    keydown_count = keydown_count + 1
    keydown_key = event.key
end
</script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("keybind", "mute")],
        &[("keybind", "onMuteShortcut")],
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
    assert_eq!(runtime_number(&component, "keydown_count"), 0.0);

    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[("keydown", "onKeyDown")],
    )]));
    component.focused_key = Some("root/0".into());

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
    assert_eq!(runtime_number(&component, "keydown_count"), 1.0);
    assert_eq!(
        runtime_value(&component, "keydown_key"),
        Some(serde_json::Value::String("m".into()))
    );
}

#[test]
fn keyboard_shortcuts_bare_printable_does_not_steal_focused_text_input() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
mute_count = 0
keydown_key = ""
input_seen = ""

function onMuteShortcut()
    mute_count = mute_count + 1
end

function onInputKeyDown(event)
    keydown_key = event.key
end

function onInputChange(value)
    input_seen = value
end
</script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "input",
        "root/0",
        0.0,
        0.0,
        100.0,
        24.0,
        &[("keybind", "mute")],
        &[
            ("keybind", "onMuteShortcut"),
            ("keydown", "onInputKeyDown"),
            ("change", "onInputChange"),
        ],
    )]));
    component.focused_key = Some("root/0".into());

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
    assert_eq!(runtime_number(&component, "mute_count"), 0.0);
    assert_eq!(
        runtime_value(&component, "keydown_key"),
        Some(serde_json::Value::String("m".into()))
    );

    component
        .handle_input(&theme, 240, 160, ComponentInput::Char { ch: 'm' })
        .unwrap();
    assert_eq!(
        runtime_value(&component, "input_seen"),
        Some(serde_json::Value::String("m".into()))
    );
}

#[test]
fn keyboard_shortcuts_manifest_declaration_wins_over_legacy_settings_same_id() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    component.settings_json = serde_json::json!({
        "keyboard": {
            "shortcuts": {
                "mute": {
                    "key": "z"
                }
            }
        }
    });

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].keybind_id, "mute");
    assert_eq!(resolved[0].key, "m");
    assert_eq!(resolved[0].source, KeybindResolutionSource::ModuleDefault);
    assert_keybind_diagnostic(
        &component,
        "mute",
        "legacy settings shortcut is ignored because mesh.keybinds declares this action",
    );
}

#[test]
fn keyboard_shortcuts_legacy_settings_only_declaration_is_migration_diagnostic() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component.settings_json = serde_json::json!({
        "keyboard": {
            "shortcuts": {
                "mute": {
                    "key": "z"
                }
            }
        }
    });

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());

    assert!(resolved.is_empty());
    assert_keybind_diagnostic(
        &component,
        "mute",
        "legacy settings shortcut declarations are migration-only; declare this action in mesh.keybinds",
    );
}

#[test]
fn manifest_descriptor_resolves_keybind_localized_text() {
    let mut manifest = minimal_test_manifest("@test/keybind-descriptor");
    manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            label: Some(mesh_core_module::LocalizedText::Translation {
                key: "keybind.mute.label".into(),
                fallback: "Mute".into(),
            }),
            description: Some(mesh_core_module::LocalizedText::Translation {
                key: "keybind.mute.description".into(),
                fallback: "Toggle audio output".into(),
            }),
            category: Some(mesh_core_module::LocalizedText::Translation {
                key: "keybind.category.audio".into(),
                fallback: "Audio".into(),
            }),
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            ..mesh_core_module::KeybindAction::default()
        },
    );
    let mut component = test_frontend_component_with_manifest(
        r#"
<template>
  <box>
    <text>{this.keybinds.mute.label}</text>
    <text>{this.keybinds.mute.label_key}</text>
    <text>{this.keybinds.mute.label_fallback}</text>
    <text>{lua_label}</text>
    <text>{this.keybinds.mute.trigger.key}</text>
  </box>
</template>
<script lang="luau">
lua_label = this.keybinds.mute.label
</script>
"#,
        manifest,
    );
    component
        .locale
        .load_translations(mesh_core_locale::TranslationSet {
            locale: "sk".into(),
            messages: HashMap::from([
                ("keybind.mute.label".into(), "Stlmit".into()),
                (
                    "keybind.mute.description".into(),
                    "Prepnúť zvukový výstup".into(),
                ),
                ("keybind.category.audio".into(), "Zvuk".into()),
            ]),
        });
    component.locale.set_locale("sk");
    component.runtimes.lock().unwrap().clear();
    component.init_root_runtime().unwrap();

    let tree = component.build_tree(&default_theme(), 240, 160);
    let mut text = Vec::new();
    collect_text_content(&tree, &mut text);

    assert_eq!(
        runtime_value(&component, "lua_label").and_then(|value| value.as_str().map(str::to_string)),
        Some("Stlmit".into())
    );
    assert!(text.iter().any(|line| line == "Stlmit"));
    assert!(text.iter().any(|line| line == "keybind.mute.label"));
    assert!(text.iter().any(|line| line == "Mute"));
    assert!(text.iter().any(|line| line == "m"));
}

#[test]
fn manifest_descriptor_missing_translation_uses_fallback_and_diagnostic() {
    let mut manifest = minimal_test_manifest("@test/keybind-descriptor");
    manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            label: Some(mesh_core_module::LocalizedText::Translation {
                key: "keybind.mute.label".into(),
                fallback: "Mute".into(),
            }),
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            ..mesh_core_module::KeybindAction::default()
        },
    );
    let mut component = test_frontend_component_with_manifest(
        r#"
<template>
  <box>
    <text>{this.keybinds.mute.label}</text>
    <text>{this.keybinds.mute.label_key}</text>
  </box>
</template>
<script lang="luau">
lua_label = this.keybinds.mute.label
</script>
"#,
        manifest,
    );
    component.locale.set_locale("sk");

    let tree = component.build_tree(&default_theme(), 240, 160);
    let mut text = Vec::new();
    collect_text_content(&tree, &mut text);

    assert_eq!(
        runtime_value(&component, "lua_label").and_then(|value| value.as_str().map(str::to_string)),
        Some("Mute".into())
    );
    assert!(text.iter().any(|line| line == "Mute"));
    assert!(text.iter().any(|line| line == "keybind.mute.label"));

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    let mesh_core_diagnostics::HealthStatus::Degraded(message) = diagnostics.health() else {
        panic!("expected degraded missing translation diagnostic");
    };
    assert!(
        message.contains("missing localized manifest text"),
        "diagnostic should describe missing manifest text: {message}"
    );
    assert!(
        message.contains("module_id='@test/keybind-descriptor'"),
        "diagnostic should include module id: {message}"
    );
    assert!(
        message.contains("field_path='mesh.keybinds.mute.label'"),
        "diagnostic should include field path: {message}"
    );
    assert!(
        message.contains("key='keybind.mute.label'"),
        "diagnostic should include key: {message}"
    );
    assert!(
        message.contains("fallback='Mute'"),
        "diagnostic should include fallback: {message}"
    );
}

#[test]
fn keybind_locale_exact_locale_wins_over_parent_and_generic() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
accept_count = 0
function onAccept()
    accept_count = accept_count + 1
end
</script>
"#,
    );
    component.locale.set_locale("sk-SK");
    component.compiled.manifest.keybinds.actions.insert(
        "accept".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                key: Some("a".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::from([
                (
                    "sk".into(),
                    mesh_core_module::KeybindTrigger {
                        kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                        key: Some("p".into()),
                        modifiers: Vec::new(),
                    },
                ),
                (
                    "sk-SK".into(),
                    mesh_core_module::KeybindTrigger {
                        kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                        key: Some("r".into()),
                        modifiers: Vec::new(),
                    },
                ),
            ]),
            ..mesh_core_module::KeybindAction::default()
        },
    );

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].key, "r");
    assert_eq!(
        resolved[0].trigger_kind,
        mesh_core_module::KeybindTriggerKind::AccessKey
    );
    assert_eq!(
        resolved[0].source,
        KeybindResolutionSource::LocaleDefault {
            locale: "sk-SK".into()
        }
    );
}

#[test]
fn keybind_locale_parent_locale_wins_over_generic() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
accept_count = 0
function onAccept()
    accept_count = accept_count + 1
end
</script>
"#,
    );
    component.locale.set_locale("sk-SK");
    component.compiled.manifest.keybinds.actions.insert(
        "accept".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                key: Some("a".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::from([(
                "sk".into(),
                mesh_core_module::KeybindTrigger {
                    kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                    key: Some("p".into()),
                    modifiers: Vec::new(),
                },
            )]),
            ..mesh_core_module::KeybindAction::default()
        },
    );

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].key, "p");
    assert_eq!(
        resolved[0].source,
        KeybindResolutionSource::LocaleDefault {
            locale: "sk".into()
        }
    );
}

#[test]
fn keybind_locale_user_override_wins_over_locale_and_generic() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
accept_count = 0
function onAccept()
    accept_count = accept_count + 1
end
</script>
"#,
    );
    component.locale.set_locale("sk-SK");
    component.compiled.manifest.keybinds.actions.insert(
        "accept".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                key: Some("a".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::from([(
                "sk".into(),
                mesh_core_module::KeybindTrigger {
                    kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                    key: Some("p".into()),
                    modifiers: Vec::new(),
                },
            )]),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    let keyboard_settings = mesh_core_config::KeyboardSettings {
        surface_shortcuts: HashMap::from([(
            "@test/reactive-surface".into(),
            HashMap::from([(
                "accept".into(),
                mesh_core_config::SurfaceShortcutOverride {
                    key: Some("x".into()),
                },
            )]),
        )]),
        ..mesh_core_config::KeyboardSettings::default()
    };

    let resolved = component.resolved_surface_shortcuts(&keyboard_settings);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].key, "x");
    assert_eq!(resolved[0].source, KeybindResolutionSource::UserOverride);
}

#[test]
fn keybind_override_cannot_create_missing_manifest_declaration() {
    let component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    let keyboard_settings = mesh_core_config::KeyboardSettings {
        surface_shortcuts: HashMap::from([(
            "@test/reactive-surface".into(),
            HashMap::from([(
                "missing".into(),
                mesh_core_config::SurfaceShortcutOverride {
                    key: Some("x".into()),
                },
            )]),
        )]),
        ..mesh_core_config::KeyboardSettings::default()
    };

    let resolved = component.resolved_surface_shortcuts(&keyboard_settings);

    assert!(
        resolved
            .iter()
            .all(|shortcut| shortcut.keybind_id != "missing"),
        "unknown override action ids must not create resolved shortcuts"
    );
}

#[test]
fn keybind_diagnostic_reports_unresolved_override_action() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    let keyboard_settings = mesh_core_config::KeyboardSettings {
        surface_shortcuts: HashMap::from([(
            "@test/reactive-surface".into(),
            HashMap::from([(
                "missing".into(),
                mesh_core_config::SurfaceShortcutOverride {
                    key: Some("u".into()),
                },
            )]),
        )]),
        ..mesh_core_config::KeyboardSettings::default()
    };

    let resolved = component.resolved_surface_shortcuts(&keyboard_settings);

    assert_eq!(resolved.len(), 1);
    assert_keybind_diagnostic(
        &component,
        "missing",
        "user override references undeclared keybind action",
    );
}

#[test]
fn keybind_diagnostic_reports_malformed_declaration() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some(" ".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());

    assert!(resolved.is_empty());
    assert_keybind_diagnostic(&component, "mute", "trigger has empty key");
}

#[test]
fn keybind_diagnostic_reports_unsupported_modifier() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: vec!["meta".into()],
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());

    assert!(resolved.is_empty());
    assert_keybind_diagnostic(
        &component,
        "mute",
        "trigger contains unsupported modifier 'meta'",
    );
}

#[test]
fn keybind_diagnostic_reports_duplicate_effective_binding_and_dispatches_deterministically() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
first_count = 0
second_count = 0
function onFirst()
    first_count = first_count + 1
end
function onSecond()
    second_count = second_count + 1
end
</script>
"#,
    );
    for action_id in ["first", "second"] {
        component.compiled.manifest.keybinds.actions.insert(
            action_id.into(),
            mesh_core_module::KeybindAction {
                trigger: mesh_core_module::KeybindTrigger {
                    kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                    key: Some("m".into()),
                    modifiers: Vec::new(),
                },
                localized_triggers: HashMap::new(),
                ..mesh_core_module::KeybindAction::default()
            },
        );
    }
    component.last_tree = Some(root_with(vec![
        event_node_with_attrs(
            "button",
            "root/0",
            0.0,
            0.0,
            40.0,
            24.0,
            &[("keybind", "first")],
            &[("keybind", "onFirst")],
        ),
        event_node_with_attrs(
            "button",
            "root/1",
            40.0,
            0.0,
            40.0,
            24.0,
            &[("keybind", "second")],
            &[("keybind", "onSecond")],
        ),
    ]));

    component
        .handle_input(
            &default_theme(),
            240,
            160,
            ComponentInput::KeyPressed {
                key: "m".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

    assert_eq!(runtime_number(&component, "first_count"), 1.0);
    assert_eq!(runtime_number(&component, "second_count"), 0.0);
    assert_keybind_diagnostic(
        &component,
        "second",
        "duplicate effective binding with action 'first'",
    );
}

#[test]
fn keybind_diagnostic_rejects_unsafe_user_override() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    let keyboard_settings = mesh_core_config::KeyboardSettings {
        surface_shortcuts: HashMap::from([(
            "@test/reactive-surface".into(),
            HashMap::from([(
                "mute".into(),
                mesh_core_config::SurfaceShortcutOverride {
                    key: Some("Tab".into()),
                },
            )]),
        )]),
        ..mesh_core_config::KeyboardSettings::default()
    };

    let resolved = component.resolved_surface_shortcuts(&keyboard_settings);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].key, "m");
    assert_eq!(resolved[0].source, KeybindResolutionSource::ModuleDefault);
    assert_keybind_diagnostic(
        &component,
        "mute",
        "user override uses a shell-owned traversal, cancel, or activation key",
    );
}

#[test]
fn keybind_diagnostic_reports_missing_runtime_subscriber() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        40.0,
        24.0,
        &[],
    )]));

    let requests = component
        .handle_input(
            &default_theme(),
            240,
            160,
            ComponentInput::KeyPressed {
                key: "m".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

    assert!(requests.is_empty());
    assert_keybind_diagnostic(
        &component,
        "mute",
        "resolved keybind has no runtime subscribers on focused surface",
    );
}

#[test]
fn keybind_debug_metadata_matches_resolved_accessibility_shortcut() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: vec!["ctrl".into()],
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );

    let keybinds = component.debug_surface_keybinds();

    assert_eq!(keybinds.len(), 1);
    assert_eq!(keybinds[0].surface_id, "@test/reactive-surface");
    assert_eq!(keybinds[0].module_id, "@test/reactive-surface");
    assert_eq!(keybinds[0].action_id, "mute");
    assert_eq!(keybinds[0].key, "m");
    assert_eq!(keybinds[0].modifiers, vec!["ctrl".to_string()]);
    assert_eq!(keybinds[0].trigger_kind, "shortcut");
    assert_eq!(keybinds[0].source, "module_default");
    assert_eq!(keybinds[0].accessibility_shortcut, "Control+m");
}

#[test]
fn keybind_debug_metadata_includes_resolved_manifest_text() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    component
        .locale
        .load_translations(mesh_core_locale::TranslationSet {
            locale: "sk".into(),
            messages: HashMap::from([
                ("keybind.mute.label".into(), "Stlmit".into()),
                (
                    "keybind.mute.description".into(),
                    "Prepnúť zvukový výstup".into(),
                ),
                ("keybind.category.audio".into(), "Zvuk".into()),
            ]),
        });
    component.locale.set_locale("sk");
    component.compiled.manifest.keybinds.actions.insert(
        "mute".into(),
        mesh_core_module::KeybindAction {
            label: Some(mesh_core_module::LocalizedText::Translation {
                key: "keybind.mute.label".into(),
                fallback: "Mute".into(),
            }),
            description: Some(mesh_core_module::LocalizedText::Translation {
                key: "keybind.mute.description".into(),
                fallback: "Toggle audio output".into(),
            }),
            category: Some(mesh_core_module::LocalizedText::Translation {
                key: "keybind.category.audio".into(),
                fallback: "Audio".into(),
            }),
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: vec!["ctrl".into()],
            },
            localized_triggers: HashMap::new(),
            ..mesh_core_module::KeybindAction::default()
        },
    );

    let keybinds = component.debug_surface_keybinds();

    assert_eq!(keybinds.len(), 1);
    assert_eq!(keybinds[0].label.as_deref(), Some("Stlmit"));
    assert_eq!(
        keybinds[0].description.as_deref(),
        Some("Prepnúť zvukový výstup")
    );
    assert_eq!(keybinds[0].category.as_deref(), Some("Zvuk"));
    assert_eq!(keybinds[0].label_key.as_deref(), Some("keybind.mute.label"));
    assert_eq!(
        keybinds[0].description_key.as_deref(),
        Some("keybind.mute.description")
    );
    assert_eq!(
        keybinds[0].category_key.as_deref(),
        Some("keybind.category.audio")
    );
    assert_eq!(keybinds[0].accessibility_shortcut, "Control+m");
}

#[test]
fn keybind_locale_shortcut_keeps_generic_trigger_without_user_override() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
accept_count = 0
function onAccept()
    accept_count = accept_count + 1
end
</script>
"#,
    );
    component.locale.set_locale("sk");
    component.compiled.manifest.keybinds.actions.insert(
        "accept".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                key: Some("a".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::from([(
                "sk".into(),
                mesh_core_module::KeybindTrigger {
                    kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                    key: Some("p".into()),
                    modifiers: Vec::new(),
                },
            )]),
            ..mesh_core_module::KeybindAction::default()
        },
    );

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].key, "a");
    assert_eq!(
        resolved[0].trigger_kind,
        mesh_core_module::KeybindTriggerKind::Shortcut
    );
    assert_eq!(resolved[0].source, KeybindResolutionSource::ModuleDefault);
}

#[test]
fn keybind_locale_blank_localized_trigger_falls_back_to_generic() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
accept_count = 0
function onAccept()
    accept_count = accept_count + 1
end
</script>
"#,
    );
    component.locale.set_locale("sk");
    component.compiled.manifest.keybinds.actions.insert(
        "accept".into(),
        mesh_core_module::KeybindAction {
            trigger: mesh_core_module::KeybindTrigger {
                kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                key: Some("a".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::from([(
                "sk".into(),
                mesh_core_module::KeybindTrigger {
                    kind: mesh_core_module::KeybindTriggerKind::AccessKey,
                    key: Some(" ".into()),
                    modifiers: Vec::new(),
                },
            )]),
            ..mesh_core_module::KeybindAction::default()
        },
    );

    let resolved =
        component.resolved_surface_shortcuts(&mesh_core_config::KeyboardSettings::default());

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].key, "a");
    assert_eq!(resolved[0].source, KeybindResolutionSource::ModuleDefault);
}

#[test]
fn navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    {
        let tree = component
            .last_tree
            .as_ref()
            .expect("rendered navigation tree");
        let subscribers = component.keybind_subscribers(tree);
        assert!(
            subscribers
                .iter()
                .any(|subscriber| subscriber.keybind_id == "mute"
                    && subscriber.handler.contains("onMuteShortcut")),
            "navigation mute keybind should expose its subscribed handler"
        );
    }
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
        .mesh_key()
        .expect("theme button mesh key")
        .to_owned();
    let theme_bounds =
        find_node_bounds_by_key(tree, &theme_key, 0.0, 0.0).expect("theme button bounds");
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
    assert!(
        activation_requests.is_empty(),
        "embedded theme selector should open through component state, not legacy surface requests: {activation_requests:?}"
    );
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    let child_requests = component.child_surface_requests();
    assert_eq!(
        child_requests.len(),
        1,
        "keyboard activation should derive one promoted theme selector popup: {child_requests:?}"
    );
    assert_eq!(child_requests[0].content_size, (112, 74));
    assert!(
        rect_matches_bounds(child_requests[0].anchor_rect, theme_bounds),
        "theme selector popup should anchor to the theme trigger bounds {:?}, got {:?}",
        theme_bounds,
        child_requests[0].anchor_rect
    );

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
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

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
    assert!(
        activation_requests.is_empty(),
        "embedded theme selector should close through component state, not legacy hide requests: {activation_requests:?}"
    );
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    assert!(
        component.child_surface_requests().is_empty(),
        "re-activating an already-open theme trigger should close the derived popup"
    );
}

#[test]
fn navigation_language_button_opens_language_popover_on_real_surface() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "locale": "en",
                "current": "en"
            }),
        })
        .unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let language_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageToggle",
    )
    .expect("language menu button");
    let language_key = language_button
        .mesh_key()
        .expect("language menu button mesh key")
        .to_owned();
    let language_bounds =
        find_node_bounds_by_key(tree, &language_key, 0.0, 0.0).expect("language button bounds");
    component.focused_key = Some(language_key.clone());
    component.focus_visible_key = Some(language_key);

    let open_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        open_requests.is_empty(),
        "embedded language popover should open through component state, not legacy surface requests: {open_requests:?}"
    );
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    let child_requests = component.child_surface_requests();
    assert_eq!(
        child_requests.len(),
        1,
        "keyboard activation should derive one promoted language popup: {child_requests:?}"
    );
    assert_eq!(child_requests[0].content_size, (112, 74));
    assert!(
        rect_matches_bounds(child_requests[0].anchor_rect, language_bounds),
        "language popup should anchor to the language trigger bounds {:?}, got {:?}",
        language_bounds,
        child_requests[0].anchor_rect
    );

    let close_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        close_requests.is_empty(),
        "embedded language popover should close through component state, not legacy hide requests: {close_requests:?}"
    );
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    assert!(
        component.child_surface_requests().is_empty(),
        "re-activating an already-open language trigger should close the derived popup"
    );
}

#[test]
fn navigation_theme_and_language_popovers_close_when_trigger_hover_leaves() {
    let theme = default_theme();
    let width = 960;
    let height = 80;

    for (enter_handler, leave_handler) in [
        (
            "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeEnter",
            "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeLeave",
        ),
        (
            "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageEnter",
            "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageLeave",
        ),
    ] {
        let mut component =
            real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
        let mut buffer = PixelBuffer::new(width, height);
        component
            .handle_service_event(&ServiceEvent::Updated {
                service: "mesh.locale".into(),
                source_module: "@mesh/shell".into(),
                payload: serde_json::json!({ "locale": "en", "current": "en" }),
            })
            .unwrap();
        component
            .paint(&theme, width, height, &mut buffer, 1.0)
            .unwrap();

        component
            .call_namespaced_handler(enter_handler, &[])
            .unwrap();
        component
            .paint(&theme, width, height, &mut buffer, 1.0)
            .unwrap();
        assert_eq!(
            component.child_surface_requests().len(),
            1,
            "{enter_handler} should open one embedded popover"
        );

        component
            .call_namespaced_handler(leave_handler, &[])
            .unwrap();
        component
            .paint(&theme, width, height, &mut buffer, 1.0)
            .unwrap();
        assert_eq!(
            component.child_surface_requests().len(),
            1,
            "{leave_handler} should keep its embedded popover open during the hover bridge"
        );

        std::thread::sleep(Duration::from_millis(220));
        component.tick().unwrap();
        component
            .paint(&theme, width, height, &mut buffer, 1.0)
            .unwrap();
        assert!(
            component.child_surface_requests().is_empty(),
            "{leave_handler} should close its embedded popover after the hover bridge expires"
        );
    }
}

#[test]
fn navigation_language_popover_closes_when_pointer_leaves_promoted_popup() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({ "locale": "en", "current": "en" }),
        })
        .unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let language_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageToggle",
    )
    .expect("language menu button");
    let language_key = language_button
        .mesh_key()
        .expect("language menu button mesh key")
        .to_owned();
    component.focused_key = Some(language_key.clone());
    component.focus_visible_key = Some(language_key);

    // Open the popover.
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    let child_requests = component.child_surface_requests();
    assert_eq!(
        child_requests.len(),
        1,
        "popover should be open: {child_requests:?}"
    );
    let node_key = child_requests[0].node_key.clone();
    let (cw, ch) = child_requests[0].content_size;

    // Pointer moves into the promoted popup (cancels the trigger's close bridge).
    component
        .handle_child_surface_input(
            &node_key,
            &theme,
            cw,
            ch,
            ComponentInput::PointerMove {
                x: cw as f32 / 2.0,
                y: ch as f32 / 2.0,
            },
        )
        .unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        component.child_surface_requests().len(),
        1,
        "popover must stay open while the pointer is over the promoted popup"
    );

    // Pointer leaves the promoted popup — the popover must close itself.
    component
        .handle_child_surface_input(&node_key, &theme, cw, ch, ComponentInput::PointerLeave)
        .unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    assert!(
        component.child_surface_requests().is_empty(),
        "language popover must close when the pointer leaves the promoted popup"
    );
}

#[test]
fn navigation_shipped_i18n_covers_all_template_translation_keys() {
    fn collect_keys(source: &str, keys: &mut Vec<String>) {
        for (index, _) in source.match_indices("t(") {
            if index > 0 {
                let previous = source[..index].chars().next_back().unwrap_or(' ');
                if previous.is_ascii_alphanumeric() || previous == '_' {
                    continue;
                }
            }
            let fragment = &source[index + 2..];
            let Some(end) = fragment.find(')') else {
                continue;
            };
            let raw = fragment[..end].trim();
            let quoted = raw
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .or_else(|| {
                    raw.strip_prefix('\'')
                        .and_then(|value| value.strip_suffix('\''))
                });
            if let Some(key) = quoted {
                keys.push(key.to_string());
            }
        }
    }

    let mut keys = Vec::new();
    for source in [
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/main.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/battery-button.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/language-button.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/meta-label.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/meta-pill.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/settings-button.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/theme-button.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
        )),
    ] {
        collect_keys(source, &mut keys);
    }
    keys.sort();
    keys.dedup();

    let en: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/navigation-bar/config/i18n/en.json"
    )))
    .unwrap();
    let sk: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/navigation-bar/config/i18n/sk.json"
    )))
    .unwrap();

    for key in keys {
        assert!(
            en.get(&key).is_some(),
            "missing English nav translation for {key}"
        );
        assert!(
            sk.get(&key).is_some(),
            "missing Slovak nav translation for {key}"
        );
    }
}

#[test]
fn navigation_shipped_keybind_metadata_resolves_from_i18n_catalogs() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.locale.set_locale("sk");
    component.runtimes.lock().unwrap().clear();
    component.init_root_runtime().unwrap();

    let keybinds = component.debug_surface_keybinds();
    let mute = keybinds
        .iter()
        .find(|entry| entry.action_id == "mute")
        .expect("navigation mute debug keybind");

    assert_eq!(mute.label.as_deref(), Some("Stlmit zvuk"));
    assert_eq!(mute.description.as_deref(), Some("Prepnut stlmenie zvuku"));
    assert_eq!(mute.category.as_deref(), Some("Zvuk"));
    assert_eq!(mute.label_key.as_deref(), Some("keybind.mute.label"));
    assert_eq!(
        mute.description_key.as_deref(),
        Some("keybind.mute.description")
    );
    assert_eq!(mute.category_key.as_deref(), Some("keybind.category.audio"));
    assert_eq!(mute.accessibility_shortcut, "m");
}

#[test]
fn navigation_bar_pointer_click_updates_real_surface_focus_diagnostic() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let settings_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:SettingsButton::onSettingsToggle",
    )
    .expect("rendered settings button");
    let settings_key = settings_button
        .mesh_key()
        .expect("settings button mesh key")
        .to_owned();
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
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
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
    component.paint(&theme, 420, 80, &mut buffer, 1.0).unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    assert_eq!(
        count_selectable_text_nodes(tree),
        0,
        "the shipped nav bar should not expose selectable passive text nodes"
    );
}

#[test]
fn navigation_bar_keyboard_activation_opens_volume_surface_on_real_surface() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();

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
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
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
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
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
            CoreRequest::HidePopover {
                surface_id,
                defer_for_hover_bridge: false,
            } if surface_id == "@mesh/audio-popover"
        )),
        "second click at the same hovered coordinates should hide immediately: {close_requests:?}"
    );
}

#[test]
fn navigation_bar_volume_trigger_reopens_after_rapid_toggle_cycle() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;

    for expected_open in [true, false, true] {
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

        if expected_open {
            assert!(
                requests.iter().any(|request| matches!(
                    request,
                    CoreRequest::ActivatePopover { surface_id, .. }
                        if surface_id == "@mesh/audio-popover"
                )),
                "expected rapid click to open the audio popover: {requests:?}"
            );
            component
                .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
                    surface_id: "@mesh/audio-popover".into(),
                    visible: true,
                })
                .unwrap();
        } else {
            assert!(
                requests.iter().any(|request| matches!(
                    request,
                    CoreRequest::HidePopover {
                        surface_id,
                        defer_for_hover_bridge: false,
                    } if surface_id == "@mesh/audio-popover"
                )),
                "expected rapid click to hide the audio popover: {requests:?}"
            );
            component
                .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
                    surface_id: "@mesh/audio-popover".into(),
                    visible: false,
                })
                .unwrap();
        }

        component
            .paint(&theme, width, height, &mut buffer, 1.0)
            .unwrap();
    }
}

/// A popover opened from a nested child-component trigger can be dismissed by
/// a route that never runs the trigger's own close handler — selecting an
/// option inside the popover, clicking away, or the compositor dismissing the
/// xdg_popup. In all those cases the shell emits `SurfaceVisibilityChanged`
/// with `visible = false`, and the trigger's `*_surface_hidden` portal binding
/// must be written back into the *child* component's runtime (not the surface
/// root's), or the next click hits the dead hide-branch and the popover only
/// re-opens on the click after that. Regression test for that desync.
#[test]
fn navigation_bar_volume_trigger_reopens_after_external_close() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;

    let click = |component: &mut FrontendSurfaceComponent| {
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
        component
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
            .unwrap()
    };

    // First click opens the popover.
    let requests = click(&mut component);
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ActivatePopover { surface_id, .. }
                if surface_id == "@mesh/audio-popover"
        )),
        "expected first click to open the audio popover: {requests:?}"
    );
    component
        .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
            surface_id: "@mesh/audio-popover".into(),
            visible: true,
        })
        .unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    // Popover is dismissed externally (option select / click-away / popup
    // dismiss) — the trigger's own close handler never ran.
    component
        .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
            surface_id: "@mesh/audio-popover".into(),
            visible: false,
        })
        .unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    // The very next click must re-open, not no-op on a stale "open" flag.
    let requests = click(&mut component);
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ActivatePopover { surface_id, .. }
                if surface_id == "@mesh/audio-popover"
        )),
        "expected click after external close to re-open the popover, got: {requests:?}"
    );
}

#[test]
fn navigation_bar_volume_trigger_keeps_click_capture_during_press_animation() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
    let (_left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = right - 0.5;
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
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
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
            CoreRequest::ActivatePopover { surface_id, .. }
                if surface_id == "@mesh/audio-popover"
        )),
        "release at the original press point should still click while the active animation changes visual bounds: {requests:?}"
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
    component.paint(&theme, 320, 220, &mut buffer, 1.0).unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered audio popover");
    let slider = first_node_by_tag(tree, "slider").expect("slider node");
    let slider_key = slider.mesh_key().expect("slider key").to_owned();
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
fn phase44_navigation_behavior_survives_focused_proof_path() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(960, 80);
    component.paint(&theme, 960, 80, &mut buffer, 1.0).unwrap();
    assert!(
        component.last_focused_proof_snapshot().is_some(),
        "initial navigation paint should store focused proof evidence"
    );

    component
        .handle_input(
            &theme,
            960,
            80,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    component.paint(&theme, 960, 80, &mut buffer, 1.0).unwrap();

    assert!(
        component.last_focused_proof_snapshot().is_some(),
        "keyboard navigation repaint should keep focused proof evidence"
    );
    assert!(
        component.focused_key.is_some(),
        "Tab navigation should focus a shipped navigation control"
    );
    assert_eq!(
        component.focused_key, component.focus_visible_key,
        "keyboard focus should remain visibly tracked after focused proof paint"
    );
}

fn assert_keybind_diagnostic(
    component: &FrontendSurfaceComponent,
    action_id: &str,
    reason_fragment: &str,
) {
    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    let mesh_core_diagnostics::HealthStatus::Degraded(message) = diagnostics.health() else {
        panic!("expected degraded keybind diagnostic");
    };
    assert!(
        message.contains("keybind diagnostic:"),
        "diagnostic should be keyed as keybind diagnostic: {message}"
    );
    assert!(
        message.contains("module_id='@test/reactive-surface'"),
        "diagnostic should include module id: {message}"
    );
    assert!(
        message.contains("surface_id='@test/reactive-surface'"),
        "diagnostic should include surface id: {message}"
    );
    assert!(
        message.contains(&format!("action_id='{action_id}'")),
        "diagnostic should include action id {action_id}: {message}"
    );
    assert!(
        message.contains(reason_fragment),
        "diagnostic should include reason fragment '{reason_fragment}': {message}"
    );
}

mod keyboard_settings_cache {
    use super::*;

    struct EnvGuard {
        key: &'static str,
        old: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let old = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, old }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.old {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    // Serializes access to the process-global MESH_SETTINGS_PATH /
    // MESH_SETTINGS_DEFAULTS_PATH env vars these tests mutate.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn keyboard_settings_cache_reflects_file_changes() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let defaults_path = dir.path().join("defaults.json");
        let user_path = dir.path().join("settings.json");
        std::fs::write(&defaults_path, "{}").unwrap();

        let _defaults_env = EnvGuard::set("MESH_SETTINGS_DEFAULTS_PATH", &defaults_path);
        let _user_env = EnvGuard::set("MESH_SETTINGS_PATH", &user_path);

        let component = test_frontend_component("<template><box/></template>");

        let initial = component.current_keyboard_settings();
        assert!(
            initial
                .button_activation_keys
                .contains(&"Enter".to_string())
        );

        // A brief pause guards against filesystems with coarse mtime
        // resolution reporting an unchanged timestamp for the rewrite below.
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(
            &user_path,
            r#"{"keyboard": {"button_activation_keys": ["KeyQ"]}}"#,
        )
        .unwrap();

        let updated = component.current_keyboard_settings();
        assert_eq!(updated.button_activation_keys, vec!["KeyQ".to_string()]);
    }

    // cargo test -p mesh-core-shell --release -- keyboard_settings_cache_beats_uncached_reload --ignored --nocapture
    #[test]
    #[ignore = "release-only keyboard settings cache microbenchmark"]
    fn keyboard_settings_cache_beats_uncached_reload() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let defaults_path = dir.path().join("defaults.json");
        let user_path = dir.path().join("settings.json");
        std::fs::write(&defaults_path, "{}").unwrap();
        std::fs::write(
            &user_path,
            r#"{"keyboard": {"button_activation_keys": ["Enter", "Space"]}}"#,
        )
        .unwrap();

        let _defaults_env = EnvGuard::set("MESH_SETTINGS_DEFAULTS_PATH", &defaults_path);
        let _user_env = EnvGuard::set("MESH_SETTINGS_PATH", &user_path);

        let component = test_frontend_component("<template><box/></template>");
        let iterations = 20_000usize;

        let uncached_started = Instant::now();
        let mut uncached_total = 0usize;
        for _ in 0..iterations {
            let settings = mesh_core_config::load_shell_settings()
                .map(|settings| settings.keyboard)
                .unwrap_or_default();
            uncached_total = uncached_total
                .saturating_add(std::hint::black_box(settings.button_activation_keys.len()));
        }
        let uncached_time = uncached_started.elapsed();

        let cached_started = Instant::now();
        let mut cached_total = 0usize;
        for _ in 0..iterations {
            let settings = component.current_keyboard_settings();
            cached_total = cached_total
                .saturating_add(std::hint::black_box(settings.button_activation_keys.len()));
        }
        let cached_time = cached_started.elapsed();

        assert_eq!(uncached_total, cached_total);
        eprintln!(
            "keyboard settings over {iterations} calls: reload every call {uncached_time:?}; mtime-cached {cached_time:?}"
        );
        assert!(
            cached_time < uncached_time,
            "mtime-cached settings lookup should beat reloading from disk every call"
        );
    }

    // cargo test -p mesh-core-shell --release -- resolved_surface_shortcuts_cache_beats_rebuild --ignored --nocapture
    #[test]
    #[ignore = "release-only resolved shortcut cache microbenchmark"]
    fn resolved_surface_shortcuts_cache_beats_rebuild() {
        let mut component = test_frontend_component("<template><box/></template>");
        for index in 0..24 {
            component.compiled.manifest.keybinds.actions.insert(
                format!("action-{index}"),
                mesh_core_module::KeybindAction {
                    trigger: mesh_core_module::KeybindTrigger {
                        kind: mesh_core_module::KeybindTriggerKind::Shortcut,
                        key: Some(format!("Key{index}")),
                        modifiers: vec!["Ctrl".into()],
                    },
                    localized_triggers: HashMap::new(),
                    ..mesh_core_module::KeybindAction::default()
                },
            );
        }
        let keyboard_settings = mesh_core_config::KeyboardSettings::default();
        let iterations = 50_000usize;

        let uncached_started = Instant::now();
        let mut uncached_total = 0usize;
        for _ in 0..iterations {
            *component.resolved_surface_shortcuts_cache.borrow_mut() = None;
            uncached_total = uncached_total.saturating_add(std::hint::black_box(
                component
                    .resolved_surface_shortcuts(&keyboard_settings)
                    .len(),
            ));
        }
        let uncached_time = uncached_started.elapsed();

        *component.resolved_surface_shortcuts_cache.borrow_mut() = None;
        let cached_started = Instant::now();
        let mut cached_total = 0usize;
        for _ in 0..iterations {
            cached_total = cached_total.saturating_add(std::hint::black_box(
                component
                    .resolved_surface_shortcuts(&keyboard_settings)
                    .len(),
            ));
        }
        let cached_time = cached_started.elapsed();

        assert_eq!(uncached_total, cached_total);
        eprintln!(
            "resolved surface shortcuts over {iterations} calls: rebuild {uncached_time:?}; cached {cached_time:?}; ratio {:.1}x",
            uncached_time.as_secs_f64() / cached_time.as_secs_f64()
        );
        assert!(
            cached_time < uncached_time,
            "resolved shortcut cache should beat rebuilding declarations and localized triggers"
        );
    }
}
