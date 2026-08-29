use super::*;
use mesh_core_frontend_host::ShellComponent;

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
    // Surface shortcuts resolve from `mesh.contributes.keybinds` declarations; the legacy
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
        None,
        None,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
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
        None,
        None,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
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
        "legacy settings shortcut is ignored because mesh.contributes.keybinds declares this action",
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
        "legacy settings shortcut declarations are migration-only; declare this action in mesh.contributes.keybinds",
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
    component.locale.load_module_translations(
        "@test/keybind-descriptor",
        mesh_core_locale::TranslationSet {
            locale: "sk".into(),
            messages: HashMap::from([
                ("keybind.mute.label".into(), "Stlmit".into()),
                (
                    "keybind.mute.description".into(),
                    "Prepnúť zvukový výstup".into(),
                ),
                ("keybind.category.audio".into(), "Zvuk".into()),
            ]),
        },
    );
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
        message.contains("field_path='mesh.contributes.keybinds.mute.label'"),
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
    component.locale.load_module_translations(
        "@test/reactive-surface",
        mesh_core_locale::TranslationSet {
            locale: "sk".into(),
            messages: HashMap::from([
                ("keybind.mute.label".into(), "Stlmit".into()),
                (
                    "keybind.mute.description".into(),
                    "Prepnúť zvukový výstup".into(),
                ),
                ("keybind.category.audio".into(), "Zvuk".into()),
            ]),
        },
    );
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

mod keyboard_settings {
    use super::*;

    #[test]
    fn keyboard_settings_come_from_the_injected_store() {
        // No file is read on the input path any more: the component answers
        // from the store the shell handed it.
        let component = test_frontend_component("<template><box/></template>");

        let settings = component.current_keyboard_settings();

        assert!(
            settings
                .button_activation_keys
                .contains(&"Enter".to_string())
        );
        assert!(
            settings
                .button_activation_keys
                .contains(&"Space".to_string())
        );
    }

    #[test]
    fn applying_a_reloaded_store_updates_keyboard_settings() {
        let mut component = test_frontend_component("<template><box/></template>");

        let reloaded = test_settings_store_with(
            "shell",
            serde_json::json!({ "keyboard": { "button_activation_keys": ["KeyQ"] } }),
        );
        component.apply_settings(&reloaded).unwrap();

        assert_eq!(
            component.current_keyboard_settings().button_activation_keys,
            vec!["KeyQ".to_string()]
        );
    }

    #[test]
    fn an_unrelated_shell_override_leaves_activation_keys_at_their_defaults() {
        let mut component = test_frontend_component("<template><box/></template>");

        let reloaded = test_settings_store_with(
            "shell",
            serde_json::json!({ "tooltip": { "delay_ms": 10 } }),
        );
        component.apply_settings(&reloaded).unwrap();

        assert_eq!(
            component.current_keyboard_settings().button_activation_keys,
            vec!["Enter".to_string(), "Space".to_string()]
        );
    }
}

mod resolved_surface_shortcuts_cache {
    use super::*;

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
