use super::*;
use mesh_core_frontend_host::ShellComponent;

#[test]
fn pointer_button_identity_reaches_click_event_without_secondary_activation() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
clicks = 0
observed_button = 0
function onClick(event)
    clicks = clicks + 1
    observed_button = event.pointer.button
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        10.0,
        10.0,
        80.0,
        28.0,
        &[("click", "onClick")],
    )]));
    let theme = default_theme();

    for pressed in [true, false] {
        component
            .handle_input(
                &theme,
                160,
                80,
                ComponentInput::PointerButton {
                    x: 24.0,
                    y: 18.0,
                    button: 0x111,
                    pressed,
                },
            )
            .unwrap();
    }
    assert_eq!(
        runtime_value(&component, "clicks"),
        Some(serde_json::json!(0))
    );

    for pressed in [true, false] {
        component
            .handle_input(
                &theme,
                160,
                80,
                ComponentInput::PointerButton {
                    x: 24.0,
                    y: 18.0,
                    button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
                    pressed,
                },
            )
            .unwrap();
    }

    assert_eq!(
        runtime_value(&component, "clicks"),
        Some(serde_json::json!(1))
    );
    assert_eq!(
        runtime_value(&component, "observed_button"),
        Some(serde_json::json!(
            mesh_core_presentation::PRIMARY_POINTER_BUTTON
        ))
    );
}

#[test]
fn disabled_and_inert_state_changes_cancel_captured_and_focused_activation() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
clicks = 0
function onClick()
    clicks = clicks + 1
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        10.0,
        10.0,
        80.0,
        28.0,
        &[("click", "onClick")],
    )]));
    let theme = default_theme();

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 24.0,
                y: 18.0,
                button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
                pressed: true,
            },
        )
        .unwrap();
    component
        .last_tree
        .as_mut()
        .expect("retained tree after pointer press")
        .attributes
        .insert("inert".into(), "true".into());
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 24.0,
                y: 18.0,
                button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
                pressed: false,
            },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "clicks"),
        Some(serde_json::json!(0))
    );

    let tree = component
        .last_tree
        .as_mut()
        .expect("retained tree after captured release");
    tree.attributes.remove("inert");
    tree.children[0]
        .attributes
        .insert("disabled".into(), "true".into());
    component.focused_key = Some("root/0".into());
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
        runtime_value(&component, "clicks"),
        Some(serde_json::json!(0))
    );
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
            ComponentInput::KeyPressed {
                key: "Backspace".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

    assert_eq!(
        component.input_values.get(&input_id).map(String::as_str),
        Some("a")
    );
    assert_eq!(
        runtime_value(&component, "input_seen"),
        Some(serde_json::Value::String("a".into()))
    );
}

#[test]
fn text_delete_preserves_utf8_boundaries_and_dispatches_once() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
input_seen = ""
change_count = 0
function onInputChange(value)
    input_seen = value
    change_count = change_count + 1
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
    let input_id = find_node_by_key(component.last_tree.as_ref().unwrap(), "root/0")
        .unwrap()
        .id;
    component.input_values.insert(input_id, "A🙂B🙂C".into());
    // Cursor after the first composed scalar: delete one scalar on each side.
    component.input_cursors.insert(input_id, "A🙂".len());

    component
        .handle_input(
            &default_theme(),
            240,
            160,
            ComponentInput::TextDelete {
                before_bytes: "🙂".len(),
                after_bytes: "B".len(),
            },
        )
        .unwrap();

    assert_eq!(
        component.input_values.get(&input_id).map(String::as_str),
        Some("A🙂C")
    );
    assert_eq!(component.input_cursors.get(&input_id), Some(&1));
    assert_eq!(
        runtime_value(&component, "input_seen"),
        Some(serde_json::Value::String("A🙂C".into()))
    );
    assert_eq!(
        runtime_value(&component, "change_count"),
        Some(serde_json::json!(1))
    );

    // One byte is not a valid prefix of the following emoji; the request is
    // ignored rather than splitting its UTF-8 sequence.
    component
        .handle_input(
            &default_theme(),
            240,
            160,
            ComponentInput::TextDelete {
                before_bytes: 0,
                after_bytes: 1,
            },
        )
        .unwrap();
    assert_eq!(
        component.input_values.get(&input_id).map(String::as_str),
        Some("A🙂C")
    );
    assert_eq!(
        runtime_value(&component, "change_count"),
        Some(serde_json::json!(1))
    );
}

#[test]
fn keyboard_activation_focused_input_delete_edits_next_scalar() {
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
    let input_id = find_node_by_key(component.last_tree.as_ref().unwrap(), "root/0")
        .unwrap()
        .id;
    component.input_values.insert(input_id, "A🙂B".into());
    component.input_cursors.insert(input_id, "A".len());

    component
        .handle_input(
            &default_theme(),
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Delete".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

    assert_eq!(
        component.input_values.get(&input_id).map(String::as_str),
        Some("AB")
    );
    assert_eq!(
        runtime_value(&component, "input_seen"),
        Some(serde_json::Value::String("AB".into()))
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
    let input_id = find_node_by_key(component.last_tree.as_ref().unwrap(), "root/0")
        .unwrap()
        .id;
    component.input_values.insert(input_id, "me".into());

    let theme = default_theme();
    component
        .handle_input(&theme, 240, 160, ComponentInput::Char { ch: 's' })
        .unwrap();

    assert_eq!(
        component.input_values.get(&input_id).map(String::as_str),
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
fn option_and_radio_activation_store_group_values_by_live_node_id() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    let mut select = event_node_with_attrs(
        "input",
        "root/0",
        0.0,
        0.0,
        120.0,
        44.0,
        &[("data-mesh-element", "select")],
        &[],
    );
    select.children.push(event_node_with_attrs(
        "input",
        "root/0/0",
        0.0,
        24.0,
        120.0,
        20.0,
        &[("data-mesh-element", "option"), ("value", "sk")],
        &[],
    ));
    let mut radio_group = event_node_with_attrs(
        "column",
        "root/1",
        0.0,
        52.0,
        120.0,
        44.0,
        &[("data-mesh-element", "radio-group")],
        &[],
    );
    radio_group.children.push(event_node_with_attrs(
        "input",
        "root/1/0",
        0.0,
        52.0,
        120.0,
        20.0,
        &[("data-mesh-element", "radio"), ("value", "compact")],
        &[],
    ));
    let tree = root_with(vec![select, radio_group]);
    let select_id = find_node_by_key(&tree, "root/0").unwrap().id;
    let radio_group_id = find_node_by_key(&tree, "root/1").unwrap().id;

    component.activate_option_choice(&tree, "root/0/0").unwrap();
    component.activate_radio_choice(&tree, "root/1/0").unwrap();

    assert_eq!(
        component.input_values.get(&select_id).map(String::as_str),
        Some("sk")
    );
    assert_eq!(
        component
            .input_values
            .get(&radio_group_id)
            .map(String::as_str),
        Some("compact")
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
    let select_id = find_node_by_key(component.last_tree.as_ref().unwrap(), "root/0")
        .unwrap()
        .id;

    let theme = default_theme();
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
    // Choice activation lands on release, so the press alone must not select.
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
        component.input_values.get(&select_id).map(String::as_str),
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
