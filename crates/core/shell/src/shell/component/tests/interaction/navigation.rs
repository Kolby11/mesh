use super::*;
use mesh_core_frontend_host::ShellComponent;

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
    component.settings_json = serde_json::json!({
        "keyboard": {
            "shortcuts": {
                "mute": {
                    "key": "m"
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
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
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
    assert!(matches!(
        activation_requests.as_slice(),
        [CoreRequest::SetTheme { theme_id }] if theme_id == "mesh-default-dark"
    ));
}

#[test]
fn navigation_language_button_publishes_locale_request_on_real_surface() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
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
    let language_button =
        first_node_with_attr(tree, "title", "Choose language").expect("language menu button");
    assert_eq!(
        language_button.attributes.get("title").map(String::as_str),
        Some("Choose language")
    );
    let language_key = language_button
        .attributes
        .get("_mesh_key")
        .expect("language menu button mesh key")
        .clone();
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
    assert!(open_requests.is_empty());
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree with language menu");
    let slovak_option =
        first_node_with_attr(tree, "title", "Slovak").expect("Slovak language option");
    let slovak_key = slovak_option
        .attributes
        .get("_mesh_key")
        .expect("Slovak option mesh key")
        .clone();
    component.focused_key = Some(slovak_key.clone());
    component.focus_visible_key = Some(slovak_key);

    let activation_requests = component
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
    assert!(matches!(
        activation_requests.as_slice(),
        [CoreRequest::SetLocale { locale }] if locale == "sk"
    ));
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
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
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
fn navigation_buttons_animate_shape_from_squircle_to_circle_with_transform() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
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
    assert_eq!(button.computed_style.border_radius.top_left, 8.0);
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
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    let hovered_tree = component
        .last_tree
        .as_ref()
        .expect("hovered navigation tree");
    let hovered_button = node_by_mesh_key(hovered_tree, &button_key);

    assert!(hovered_button.state.hovered);
    assert!(
        component.transitions.contains_key(&button_key),
        "hover should start the visible navigation transition"
    );
    assert_eq!(hovered_button.computed_style.border_radius.top_left, 8.0);
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

    std::thread::sleep(Duration::from_millis(220));
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    let settled_hover_tree = component
        .last_tree
        .as_ref()
        .expect("settled hovered navigation tree");
    let settled_hover_button = node_by_mesh_key(settled_hover_tree, &button_key);

    assert_eq!(
        settled_hover_button.computed_style.transform.translate_y,
        -1.0
    );
    assert!((settled_hover_button.computed_style.transform.scale_x - 1.04).abs() < 0.001);
    assert!((settled_hover_button.computed_style.transform.scale_y - 1.04).abs() < 0.001);

    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x: hover_x,
                y: hover_y,
                pressed: true,
            },
        )
        .unwrap();
    assert_eq!(
        component.pointer_down_key.as_deref(),
        Some(button_key.as_str())
    );
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    let active_tree = component
        .last_tree
        .as_ref()
        .expect("active navigation tree");
    let active_button = node_by_mesh_key(active_tree, &button_key);

    assert!(active_button.state.active);
    assert!(
        component.transitions.contains_key(&button_key),
        "active press should start the visible squircle-to-circle transition"
    );
    std::thread::sleep(Duration::from_millis(220));
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    let settled_tree = component
        .last_tree
        .as_ref()
        .expect("settled active navigation tree");
    let settled_button = node_by_mesh_key(settled_tree, &button_key);

    let max_visible_radius = settled_button
        .layout
        .width
        .min(settled_button.layout.height)
        * 0.5;
    assert_eq!(
        settled_button.computed_style.border_radius.top_left,
        max_visible_radius
    );
    assert!((settled_button.computed_style.transform.scale_x - 0.94).abs() < 0.001);
    assert!((settled_button.computed_style.transform.scale_y - 0.94).abs() < 0.001);
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
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

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
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

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
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

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
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

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
fn navigation_bar_volume_trigger_reopens_after_rapid_toggle_cycle() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
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
                    CoreRequest::HideSurface { surface_id } if surface_id == "@mesh/audio-popover"
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

#[test]
fn navigation_bar_volume_trigger_keeps_click_capture_during_press_animation() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
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
        "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .attributes
        .get("_mesh_key")
        .expect("volume button mesh key")
        .clone();
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
fn audio_popover_volume_button_repeats_on_key_press_without_waiting_for_release() {
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
    let volume_up_key = first_node_with_click_handler(tree, "onVolumeUp")
        .and_then(|button| button.attributes.get("_mesh_key"))
        .expect("volume up button key")
        .clone();
    component.focused_key = Some(volume_up_key);

    let first_requests = component
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::KeyPressed {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_volume_command(&first_requests, 0.55);

    let second_requests = component
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::KeyPressed {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_volume_command(&second_requests, 0.60);

    let release_requests = component
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        !release_requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { command, .. } if command == "set_volume"
        )),
        "release should not send an extra volume command after keypress activation"
    );
}

fn assert_volume_command(requests: &[CoreRequest], expected_volume: f64) {
    match requests {
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
                (volume - expected_volume).abs() < 0.001,
                "expected volume command near {expected_volume}, got {volume}"
            );
        }
        other => panic!("expected one audio set_volume request, got {other:?}"),
    }
}

#[test]
fn audio_popover_access_key_toggles_mute_on_real_surface() {
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
    let subscribers = component.keybind_subscribers(tree);
    assert!(
        subscribers
            .iter()
            .any(|subscriber| subscriber.keybind_id == "toggle_mute"
                && subscriber.handler.contains("onToggleMute")),
        "audio popover mute button should subscribe to its manifest access key"
    );
    let mute_button = first_node_with_attr(tree, "keybind", "toggle_mute")
        .expect("audio popover mute button should carry keybind metadata");
    assert_eq!(
        mute_button.accessibility.keyboard_shortcut.as_deref(),
        Some("m")
    );
    let keybinds = component.debug_surface_keybinds();
    assert!(
        keybinds.iter().any(|entry| {
            entry.surface_id == "@mesh/audio-popover"
                && entry.action_id == "toggle_mute"
                && entry.trigger_kind == "access_key"
                && entry.accessibility_shortcut == "m"
        }),
        "audio popover keybind should be visible in debug metadata"
    );

    let requests = component
        .handle_input(&theme, 320, 220, ComponentInput::Char { ch: 'm' })
        .unwrap();
    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::ServiceCommand { interface, command, payload, .. }]
            if interface == "mesh.audio"
                && command == "set_muted"
                && payload["device_id"] == serde_json::json!("default")
                && payload["muted"] == serde_json::json!(true)
    ));

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
    component.paint(&theme, 320, 220, &mut buffer, 1.0).unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("re-rendered audio popover");
    let mute_key = first_node_with_attr(tree, "keybind", "toggle_mute")
        .and_then(|node| node.attributes.get("_mesh_key"))
        .cloned()
        .expect("audio popover mute button key");
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &mute_key, 0.0, 0.0).expect("mute button bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;
    component
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: true,
            },
        )
        .unwrap();
    let click_requests = component
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: false,
            },
        )
        .unwrap();
    assert!(matches!(
        click_requests.as_slice(),
        [CoreRequest::ServiceCommand { interface, command, payload, .. }]
            if interface == "mesh.audio"
                && command == "set_muted"
                && payload["device_id"] == serde_json::json!("default")
                && payload["muted"] == serde_json::json!(true)
    ));
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
    component
        .paint(&theme, 920, 80, &mut wide_buffer, 1.0)
        .unwrap();
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
        .paint(&theme, 240, 80, &mut compact_buffer, 1.0)
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

#[test]
fn phase44_navigation_behavior_survives_focused_proof_path() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
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
