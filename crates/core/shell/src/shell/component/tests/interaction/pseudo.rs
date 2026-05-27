use super::*;

#[test]
fn pseudo_state_annotation_uses_stable_keys_after_rebuild() {
    let focused_key = Some("root/0".to_string());
    let hovered_path = vec!["root".to_string(), "root/0".to_string()];
    let active_key = Some("root/0".to_string());
    let checked_values = HashMap::from([("root/1".to_string(), true)]);

    let mut first_tree = root_with(vec![
        child_with_attrs("button", &[]),
        child_with_attrs("checkbox", &[]),
    ]);
    let first_button_id = first_tree.children[0].id;
    annotate_runtime_tree(
        &mut first_tree,
        "root".to_string(),
        &focused_key,
        &focused_key,
        &hovered_path,
        &active_key,
        &None,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &checked_values,
        &HashMap::new(),
    );

    let mut rebuilt_tree = root_with(vec![
        child_with_attrs("button", &[]),
        child_with_attrs("checkbox", &[]),
    ]);
    assert_ne!(
        first_button_id, rebuilt_tree.children[0].id,
        "rebuilt nodes should have transient ids"
    );
    annotate_runtime_tree(
        &mut rebuilt_tree,
        "root".to_string(),
        &focused_key,
        &focused_key,
        &hovered_path,
        &active_key,
        &None,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &checked_values,
        &HashMap::new(),
    );

    let button = node_by_mesh_key(&rebuilt_tree, "root/0");
    assert!(button.state.hovered);
    assert!(button.state.focused);
    assert!(button.state.active);

    let checkbox = node_by_mesh_key(&rebuilt_tree, "root/1");
    assert!(checkbox.state.checked);
}

#[test]
fn pseudo_state_annotation_sets_disabled_and_checked_deterministically() {
    let checked_values = HashMap::from([("root/2".to_string(), false)]);
    let mut tree = root_with(vec![
        child_with_attrs("button", &[("disabled", "true")]),
        child_with_attrs("button", &[("aria-disabled", "true")]),
        child_with_attrs("checkbox", &[("checked", "true")]),
        child_with_attrs("checkbox", &[("checked", "checked")]),
    ]);

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
        &checked_values,
        &HashMap::new(),
    );

    assert!(node_by_mesh_key(&tree, "root/0").state.disabled);
    assert!(node_by_mesh_key(&tree, "root/1").state.disabled);
    assert!(
        !node_by_mesh_key(&tree, "root/2").state.checked,
        "runtime checked state should override static checked attributes"
    );
    assert!(node_by_mesh_key(&tree, "root/3").state.checked);
}

#[test]
fn pseudo_state_restyle_applies_runtime_state_after_rebuild() {
    let mut component = test_frontend_component(
        r#"
<style>
button {
  background-color: #101010;
  border-color: #111111;
  opacity: 1;
}
button:hover {
  background-color: #202020;
}
button:active {
  border-color: #303030;
}
button:disabled {
  opacity: 0.4;
}
input {
  background-color: #121212;
  color: #131313;
}
input:focus {
  background-color: #404040;
}
input:focus-visible {
  color: #505050;
}
input:checked {
  background-color: #606060;
}
</style>
<template>
  <column>
    <button disabled="true" />
    <input />
    <button />
    <checkbox checked="true" />
  </column>
</template>
"#,
    );
    component.render_hooks_pending = false;
    component.focused_key = Some("root/0/1".into());
    component.hovered_path = vec!["root".into(), "root/0".into(), "root/0/2".into()];
    component.hovered_key = Some("root/0/2".into());
    component.pointer_down_key = Some("root/0/2".into());
    component.checked_values.insert("root/0/3".into(), true);

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 120);
    component.paint(&theme, 240, 120, &mut buffer).unwrap();
    let tree = component.last_tree.as_ref().unwrap();

    let disabled_button = node_by_mesh_key(tree, "root/0/0");
    assert!(disabled_button.state.disabled);
    assert!((disabled_button.computed_style.opacity - 0.4).abs() < f32::EPSILON);

    let focused_input = node_by_mesh_key(tree, "root/0/1");
    assert!(focused_input.state.focused);
    assert_eq!(
        focused_input.computed_style.background_color,
        Color::from_hex("#404040").unwrap()
    );
    assert_eq!(
        focused_input.computed_style.color,
        Color::from_hex("#505050").unwrap()
    );
    assert!(focused_input.state.focus_visible);

    let active_button = node_by_mesh_key(tree, "root/0/2");
    assert!(active_button.state.hovered);
    assert!(active_button.state.active);
    assert_eq!(
        active_button.computed_style.background_color,
        Color::from_hex("#202020").unwrap()
    );
    assert_eq!(
        active_button.computed_style.border_color,
        Color::from_hex("#303030").unwrap()
    );

    let checked_box = node_by_mesh_key(tree, "root/0/3");
    assert!(checked_box.state.checked);
    assert_eq!(
        checked_box.computed_style.background_color,
        Color::from_hex("#606060").unwrap()
    );
}

#[test]
fn keyboard_navigation_pointer_focus_visible_tracks_input_modality() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.last_tree = Some(root_with(vec![
        event_node("input", "root/0", 0.0, 0.0, 80.0, 24.0, &[]),
        event_node("button", "root/1", 0.0, 32.0, 80.0, 24.0, &[]),
    ]));

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
    assert_eq!(component.focused_key.as_deref(), Some("root/0"));
    assert_eq!(component.focus_visible_key.as_deref(), Some("root/0"));

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 8.0,
                y: 40.0,
                pressed: true,
            },
        )
        .unwrap();
    assert_eq!(component.focused_key.as_deref(), Some("root/1"));
    assert!(
        component.focus_visible_key.is_none(),
        "pointer-focused non-text controls should keep logical focus but clear visible focus"
    );
}

#[test]
fn keyboard_navigation_tab_orders_by_visual_position_and_wraps() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.last_tree = Some(root_with(vec![
        event_node("button", "root/0", 120.0, 0.0, 80.0, 24.0, &[]),
        event_node("button", "root/1", 0.0, 0.0, 80.0, 24.0, &[]),
        event_node("button", "root/2", 0.0, 32.0, 80.0, 24.0, &[]),
    ]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(component.focused_key.as_deref(), Some("root/1"));
    assert_eq!(component.focus_visible_key.as_deref(), Some("root/1"));

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(component.focused_key.as_deref(), Some("root/0"));

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(component.focused_key.as_deref(), Some("root/2"));

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/1"),
        "Tab should wrap back to the first tabbable target"
    );

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers {
                    shift: true,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/2"),
        "Shift+Tab should wrap backward from the first target"
    );
}

#[test]
fn keyboard_navigation_skips_disabled_hidden_and_tabindex_negative_targets() {
    let mut hidden = event_node("button", "root/1", 48.0, 0.0, 40.0, 24.0, &[]);
    hidden.computed_style.display = mesh_core_elements::style::Display::None;

    let mut component = test_frontend_component("<template><box /></template>");
    component.last_tree = Some(root_with(vec![
        event_node("button", "root/0", 0.0, 0.0, 40.0, 24.0, &[]),
        hidden,
        event_node_with_attrs(
            "button",
            "root/2",
            96.0,
            0.0,
            40.0,
            24.0,
            &[("disabled", "true")],
            &[],
        ),
        event_node_with_attrs(
            "button",
            "root/3",
            144.0,
            0.0,
            40.0,
            24.0,
            &[("tabindex", "-1")],
            &[],
        ),
        event_node("button", "root/4", 192.0, 0.0, 40.0, 24.0, &[]),
    ]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(component.focused_key.as_deref(), Some("root/0"));

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/4"),
        "normal traversal should skip hidden, disabled, and tabindex=-1 targets"
    );

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerButton {
                x: 150.0,
                y: 8.0,
                pressed: true,
            },
        )
        .unwrap();
    assert_eq!(
        component.focused_key.as_deref(),
        Some("root/3"),
        "tabindex=-1 should remain pointer-focusable"
    );
}

#[test]
fn keyboard_navigation_tabindex_positive_overrides_visual_order() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.last_tree = Some(root_with(vec![
        event_node_with_attrs(
            "button",
            "root/0",
            120.0,
            0.0,
            40.0,
            24.0,
            &[("tabindex", "2")],
            &[],
        ),
        event_node_with_attrs(
            "button",
            "root/1",
            0.0,
            32.0,
            40.0,
            24.0,
            &[("tabindex", "1")],
            &[],
        ),
        event_node("button", "root/2", 0.0, 0.0, 40.0, 24.0, &[]),
    ]));

    let theme = default_theme();
    for expected in ["root/1", "root/0", "root/2"] {
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::KeyPressed {
                    key: "Tab".into(),
                    modifiers: KeyModifiers::default(),
                },
            )
            .unwrap();
        assert_eq!(component.focused_key.as_deref(), Some(expected));
    }
}

#[test]
fn keyboard_navigation_tab_triggers_blur_and_focus_handlers() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
focus_a = 0
blur_a = 0
focus_b = 0

function onFocusA()
    focus_a = focus_a + 1
end

function onBlurA()
    blur_a = blur_a + 1
end

function onFocusB()
    focus_b = focus_b + 1
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![
        event_node(
            "button",
            "root/0",
            0.0,
            0.0,
            40.0,
            24.0,
            &[("focus", "onFocusA"), ("blur", "onBlurA")],
        ),
        event_node(
            "button",
            "root/1",
            48.0,
            0.0,
            40.0,
            24.0,
            &[("focus", "onFocusB")],
        ),
    ]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();

    assert_eq!(runtime_number(&component, "focus_a"), 1.0);
    assert_eq!(runtime_number(&component, "blur_a"), 1.0);
    assert_eq!(runtime_number(&component, "focus_b"), 1.0);
}

#[test]
fn keyboard_activation_button_fires_on_key_press_without_duplicate_release() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
button_count = 0
toggle_seen = false

function onButtonClick(event)
    if event.trigger and event.trigger.type == "keyboard" then
        button_count = button_count + 1
    end
end

function onToggleChange(value)
    toggle_seen = value
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![
        event_node(
            "button",
            "root/0",
            0.0,
            0.0,
            40.0,
            24.0,
            &[("click", "onButtonClick")],
        ),
        event_node(
            "checkbox",
            "root/1",
            48.0,
            0.0,
            40.0,
            24.0,
            &[("change", "onToggleChange")],
        ),
    ]));
    let theme = default_theme();

    component.focused_key = Some("root/0".into());
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
        runtime_number(&component, "button_count"),
        1.0,
        "button release should not duplicate the keypress activation"
    );

    component.focused_key = Some("root/1".into());
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "Space".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        !runtime_bool(&component, "toggle_seen"),
        "toggle default activation should wait for key release"
    );
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyReleased {
                key: "Space".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(runtime_bool(&component, "toggle_seen"));
}

#[test]
fn keyboard_activation_slider_arrow_keys_step_value() {
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
    component.last_tree = Some(root_with(vec![event_node_with_attrs(
        "slider",
        "root/0",
        0.0,
        0.0,
        120.0,
        24.0,
        &[
            ("min", "0"),
            ("max", "1"),
            ("step", "0.1"),
            ("value", "0.5"),
        ],
        &[("change", "onSliderChange")],
    )]));
    component.focused_key = Some("root/0".into());

    let theme = default_theme();
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

    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "ArrowLeft".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!((runtime_number(&component, "slider_seen") - 0.5).abs() < 0.001);
}
