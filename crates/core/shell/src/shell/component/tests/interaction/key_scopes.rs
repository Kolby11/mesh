use super::*;

fn declare(component: &mut FrontendSurfaceComponent, id: &str, kind: mesh_core_module::KeybindTriggerKind, key: Option<&str>, scope: mesh_core_module::KeybindScope) {
    component.compiled.manifest.keybinds.actions.insert(
        id.into(),
        mesh_core_module::KeybindAction {
            scope,
            trigger: mesh_core_module::KeybindTrigger {
                kind,
                key: key.map(str::to_string),
                modifiers: Vec::new(),
            },
            ..mesh_core_module::KeybindAction::default()
        },
    );
}

fn auto(component: &mut FrontendSurfaceComponent, id: &str) {
    declare(
        component,
        id,
        mesh_core_module::KeybindTriggerKind::Auto,
        None,
        mesh_core_module::KeybindScope::Surface,
    );
}

fn press(component: &mut FrontendSurfaceComponent, key: &str, ctrl: bool) {
    component
        .handle_input(
            &default_theme(),
            240,
            160,
            ComponentInput::KeyPressed {
                key: key.into(),
                modifiers: KeyModifiers {
                    ctrl,
                    ..KeyModifiers::default()
                },
            },
        )
        .unwrap();
}

fn release(component: &mut FrontendSurfaceComponent, key: &str) {
    component
        .handle_input(
            &default_theme(),
            240,
            160,
            ComponentInput::KeyReleased {
                key: key.into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
}

const SCRIPT: &str = r#"
<template><box /></template>
<script lang="luau">
count = 0
function onAction() count = count + 1 end
</script>
"#;

fn two_auto_actions() -> FrontendSurfaceComponent {
    let mut component = test_frontend_component(SCRIPT);
    auto(&mut component, "find");
    auto(&mut component, "open");
    component.last_tree = Some(root_with(vec![
        event_node_with_attrs("button", "root/0", 0.0, 0.0, 40.0, 24.0, &[("keybind", "find")], &[("keybind", "onAction")]),
        event_node_with_attrs("button", "root/1", 50.0, 0.0, 40.0, 24.0, &[("keybind", "open")], &[("keybind", "onAction")]),
    ]));
    component
}

fn resolved_key(component: &FrontendSurfaceComponent, id: &str) -> Option<String> {
    component
        .resolved_surface_shortcuts(&component.current_keyboard_settings())
        .into_iter()
        .find(|shortcut| shortcut.keybind_id == id)
        .map(|shortcut| shortcut.key)
}

#[test]
fn auto_actions_take_priority_keys_in_declaration_order() {
    let mut component = two_auto_actions();
    component.settings = test_settings_store_with(
        "shell",
        serde_json::json!({ "keyboard": { "key_priority": "qwertasd" } }),
    );
    assert_eq!(resolved_key(&component, "find").as_deref(), Some("q"));
    assert_eq!(resolved_key(&component, "open").as_deref(), Some("w"));
}

#[test]
fn pressing_an_auto_key_runs_the_subscriber() {
    let mut component = two_auto_actions();
    // Default priority starts with `a`, then `s`.
    press(&mut component, "s", false);
    assert_eq!(runtime_number(&component, "count"), 1.0);
    press(&mut component, "q", false);
    assert_eq!(runtime_number(&component, "count"), 1.0, "q is not assigned");
}

#[test]
fn explicit_bindings_are_skipped_by_auto_assignment() {
    let mut component = two_auto_actions();
    declare(
        &mut component,
        "close",
        mesh_core_module::KeybindTriggerKind::Shortcut,
        Some("a"),
        mesh_core_module::KeybindScope::Surface,
    );
    assert_eq!(resolved_key(&component, "close").as_deref(), Some("a"));
    assert_eq!(resolved_key(&component, "find").as_deref(), Some("s"));
    assert_eq!(resolved_key(&component, "open").as_deref(), Some("d"));
}

#[test]
fn saved_assignments_stay_put_and_new_keys_are_queued_for_saving() {
    let mut component = two_auto_actions();
    let surface = component.surface_id().to_string();
    component.settings = test_settings_store_with(
        "shell",
        serde_json::json!({ "keyboard": { "assigned": { surface.clone(): { "open": "a" } } } }),
    );
    assert_eq!(resolved_key(&component, "open").as_deref(), Some("a"));
    assert_eq!(resolved_key(&component, "find").as_deref(), Some("s"));
    let Some(CoreRequest::SetKeyboardSetting { field, value }) = component.take_assigned_write()
    else {
        panic!("a new assignment should be queued");
    };
    assert_eq!(field, "assigned");
    assert_eq!(value[&surface]["find"], "s");
    assert_eq!(value[&surface]["open"], "a");
    assert!(component.take_assigned_write().is_none(), "queued once");
}

#[test]
fn overflow_leaves_an_action_unbound() {
    let mut component = two_auto_actions();
    component.settings = test_settings_store_with(
        "shell",
        serde_json::json!({ "keyboard": { "key_priority": "q" } }),
    );
    assert_eq!(resolved_key(&component, "find").as_deref(), Some("q"));
    assert_eq!(resolved_key(&component, "open"), None);
}

#[test]
fn focus_scoped_binding_fires_only_while_focus_is_inside_its_component() {
    let mut component = test_frontend_component(SCRIPT);
    declare(
        &mut component,
        "step",
        mesh_core_module::KeybindTriggerKind::Shortcut,
        Some("j"),
        mesh_core_module::KeybindScope::Focus,
    );
    let mut panel = event_node_with_attrs("column", "root/0", 0.0, 0.0, 100.0, 60.0, &[("keybind", "step")], &[("keybind", "onAction")]);
    panel.children = vec![event_node("button", "root/0/0", 0.0, 0.0, 40.0, 24.0, &[])].into();
    component.last_tree = Some(root_with(vec![
        panel,
        event_node("button", "root/1", 120.0, 0.0, 40.0, 24.0, &[]),
    ]));

    component.focused_key = Some("root/1".into());
    press(&mut component, "j", false);
    assert_eq!(runtime_number(&component, "count"), 0.0, "focus is outside the panel");

    component.focused_key = Some("root/0/0".into());
    press(&mut component, "j", false);
    assert_eq!(runtime_number(&component, "count"), 1.0, "focus is inside the panel");
}

#[test]
fn focus_scoped_binding_wins_over_a_surface_binding_on_the_same_key() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
surface_count = 0
focus_count = 0
function onSurface() surface_count = surface_count + 1 end
function onFocus() focus_count = focus_count + 1 end
</script>
"#,
    );
    declare(&mut component, "wide", mesh_core_module::KeybindTriggerKind::Shortcut, Some("j"), mesh_core_module::KeybindScope::Surface);
    declare(&mut component, "near", mesh_core_module::KeybindTriggerKind::Shortcut, Some("j"), mesh_core_module::KeybindScope::Focus);
    let mut panel = event_node_with_attrs("column", "root/0", 0.0, 0.0, 100.0, 60.0, &[("keybind", "near")], &[("keybind", "onFocus")]);
    panel.children = vec![event_node("button", "root/0/0", 0.0, 0.0, 40.0, 24.0, &[])].into();
    component.last_tree = Some(root_with(vec![
        panel,
        event_node_with_attrs("button", "root/1", 120.0, 0.0, 40.0, 24.0, &[("keybind", "wide")], &[("keybind", "onSurface")]),
    ]));

    component.focused_key = Some("root/0/0".into());
    press(&mut component, "j", false);
    assert_eq!(runtime_number(&component, "focus_count"), 1.0);
    assert_eq!(runtime_number(&component, "surface_count"), 0.0);

    component.focused_key = Some("root/1".into());
    press(&mut component, "j", false);
    assert_eq!(runtime_number(&component, "surface_count"), 1.0, "outside the panel the surface binding answers");
}

#[test]
fn a_quick_tap_of_the_hint_chord_keeps_hints_on_and_escape_clears_them() {
    let mut component = two_auto_actions();
    assert!(!component.key_hints_visible());
    press(&mut component, "h", true);
    assert!(component.key_hints_visible(), "held while the chord is down");
    release(&mut component, "h");
    assert!(component.key_hints_visible(), "a tap leaves hints on");
    press(&mut component, "Escape", false);
    assert!(!component.key_hints_visible());
}

#[test]
fn holding_the_hint_chord_shows_hints_only_until_release_in_hold_mode() {
    let mut component = two_auto_actions();
    component.settings = test_settings_store_with(
        "shell",
        serde_json::json!({ "keyboard": { "hint_mode": "hold" } }),
    );
    press(&mut component, "h", true);
    assert!(component.key_hints_visible());
    release(&mut component, "h");
    assert!(!component.key_hints_visible());
}

#[test]
fn toggle_mode_flips_hints_on_each_press() {
    let mut component = two_auto_actions();
    component.settings = test_settings_store_with(
        "shell",
        serde_json::json!({ "keyboard": { "hint_mode": "toggle" } }),
    );
    press(&mut component, "h", true);
    release(&mut component, "h");
    assert!(component.key_hints_visible());
    press(&mut component, "h", true);
    release(&mut component, "h");
    assert!(!component.key_hints_visible());
}

#[test]
fn hints_label_each_bound_control_with_its_key() {
    let mut component = two_auto_actions();
    let mut tree = component.last_tree.clone().unwrap();
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
    let hints = component.collect_key_hints(&tree);
    let labels: Vec<_> = hints.iter().map(|hint| hint.label.as_str()).collect();
    assert_eq!(labels, ["A", "S"]);
    assert!(hints.iter().all(|hint| hint.width > 0.0));
}

#[test]
fn hint_overlay_paints_key_caps_over_the_surface_and_removes_them_again() {
    let mut component = test_frontend_component(
        r#"
<template><row class="bar">
    <button class="a" keybind="find" onkeybind={onAction}><text>Find</text></button>
    <button class="b" keybind="open" onkeybind={onAction}><text>Open</text></button>
</row></template>
<script lang="luau">
function onAction() end
</script>
<style>
.bar { width: 100%; height: 100%; gap: 12px; }
.a, .b { width: 90px; height: 40px; }
</style>
"#,
    );
    auto(&mut component, "find");
    auto(&mut component, "open");
    let theme = default_theme();
    let paint = |component: &mut FrontendSurfaceComponent| {
        let mut pixels = PixelBuffer::new(240, 160);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 160), &mut pixels, 1.0)
            .unwrap();
        pixels
    };
    let plain = paint(&mut component);

    press(&mut component, "h", true);
    let hinted = paint(&mut component);
    assert!(!component.collect_key_hints(component.last_tree.as_ref().unwrap()).is_empty());
    assert_ne!(plain.data(), hinted.data(), "key caps change pixels");

    release(&mut component, "h");
    press(&mut component, "Escape", false);
    let cleared = paint(&mut component);
    assert_eq!(plain.data(), cleared.data(), "hints leave no trace once off");
}
