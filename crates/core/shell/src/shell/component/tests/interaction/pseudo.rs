use super::*;

#[test]
fn pseudo_state_annotation_uses_stable_keys_after_rebuild() {
    let focused_key = Some("root/0".to_string());
    let hovered_path = ["root", "root/0"].map(runtime_node_id_for_key);
    let active_id = Some(runtime_node_id_for_key("root/0"));
    let checked_values = HashMap::from([(runtime_node_id_for_key("root/1"), true)]);

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
        active_id,
        None,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &checked_values,
        &mut HashMap::new(),
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
        active_id,
        None,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &checked_values,
        &mut HashMap::new(),
    );

    let button = node_by_mesh_key(&rebuilt_tree, "root/0");
    assert!(button.state.contains(NodeState::HOVERED));
    assert!(button.state.contains(NodeState::FOCUSED));
    assert!(button.state.contains(NodeState::ACTIVE));

    let checkbox = node_by_mesh_key(&rebuilt_tree, "root/1");
    assert!(checkbox.state.contains(NodeState::CHECKED));
}

#[test]
fn pseudo_state_annotation_sets_disabled_and_checked_deterministically() {
    let checked_values = HashMap::from([(runtime_node_id_for_key("root/2"), false)]);
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
        None,
        None,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &checked_values,
        &mut HashMap::new(),
    );

    assert!(
        node_by_mesh_key(&tree, "root/0")
            .state
            .contains(NodeState::DISABLED)
    );
    assert!(
        node_by_mesh_key(&tree, "root/1")
            .state
            .contains(NodeState::DISABLED)
    );
    assert!(
        !node_by_mesh_key(&tree, "root/2")
            .state
            .contains(NodeState::CHECKED),
        "runtime checked state should override static checked attributes"
    );
    assert!(
        node_by_mesh_key(&tree, "root/3")
            .state
            .contains(NodeState::CHECKED)
    );
}

#[test]
fn pseudo_state_annotation_projects_authored_states_through_the_typed_table() {
    let mut tree = root_with(vec![
        child_with_attrs(
            "input",
            &[
                ("readonly", "true"),
                ("required", "true"),
                ("invalid", "true"),
                ("value", "ready"),
            ],
        ),
        child_with_attrs("tab", &[("selected", "true")]),
        child_with_attrs("details", &[("expanded", "true")]),
        child_with_attrs("button", &[("pressed", "true")]),
    ]);

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

    let input = node_by_mesh_key(&tree, "root/0");
    assert!(input.state.contains(NodeState::READ_ONLY));
    assert!(input.state.contains(NodeState::REQUIRED));
    assert!(input.state.contains(NodeState::INVALID));
    assert!(input.state.contains(NodeState::VALUE));
    assert!(
        node_by_mesh_key(&tree, "root/1")
            .state
            .contains(NodeState::SELECTED)
    );
    assert!(
        node_by_mesh_key(&tree, "root/2")
            .state
            .contains(NodeState::EXPANDED)
    );
    assert!(
        node_by_mesh_key(&tree, "root/3")
            .state
            .contains(NodeState::PRESSED)
    );
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
checkbox:checked {
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
    component.reactive_effects_pending = false;
    component.focused_key = Some("root/0/1".into());
    component.hovered_path = ["root", "root/0", "root/0/2"]
        .map(runtime_node_id_for_key)
        .to_vec();
    component.hovered_key = Some("root/0/2".into());
    component.pointer_down_id = Some(runtime_node_id_for_key("root/0/2"));
    component
        .checked_values
        .insert(runtime_node_id_for_key("root/0/3"), true);

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 120);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(240, 120), &mut buffer, 1.0)
        .unwrap();
    let tree = component.last_tree.as_ref().unwrap();

    let disabled_button = node_by_mesh_key(tree, "root/0/0");
    assert!(disabled_button.state.contains(NodeState::DISABLED));
    assert!((disabled_button.computed_style.opacity - 0.4).abs() < f32::EPSILON);

    let focused_input = node_by_mesh_key(tree, "root/0/1");
    assert!(focused_input.state.contains(NodeState::FOCUSED));
    assert_eq!(
        focused_input.computed_style.background_color,
        Color::from_hex("#404040").unwrap()
    );
    assert_eq!(
        focused_input.computed_style.color,
        Color::from_hex("#505050").unwrap()
    );
    assert!(focused_input.state.contains(NodeState::FOCUS_VISIBLE));

    let active_button = node_by_mesh_key(tree, "root/0/2");
    assert!(active_button.state.contains(NodeState::HOVERED));
    assert!(active_button.state.contains(NodeState::ACTIVE));
    assert_eq!(
        active_button.computed_style.background_color,
        Color::from_hex("#202020").unwrap()
    );
    assert_eq!(
        active_button.computed_style.border_color,
        Color::from_hex("#303030").unwrap()
    );

    let checked_box = node_by_mesh_key(tree, "root/0/3");
    assert!(checked_box.state.contains(NodeState::CHECKED));
    assert_eq!(
        checked_box.computed_style.background_color,
        Color::from_hex("#606060").unwrap()
    );
}

#[test]
fn targeted_restyle_tracks_active_checked_and_focus_visible_changes() {
    let mut component = test_frontend_component(
        r#"
<template>
  <row>
    <button class="pressable" />
    <checkbox class="checkable" />
    <button class="focusable" />
  </row>
</template>
<style>
.pressable { font-weight: 400; }
.pressable:active { font-weight: 700; }
.checkable { z-index: 1; }
.checkable:checked { z-index: 7; }
.focusable { text-align: left; }
.focusable:focus-visible { text-align: right; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();
    if component.wants_render() {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
            .unwrap();
    }

    component.pointer_down_id = Some(runtime_node_id_for_key("root/0/0"));
    component
        .checked_values
        .insert(runtime_node_id_for_key("root/0/1"), true);
    component.focused_key = Some("root/0/2".into());
    component.focus_visible_key = Some("root/0/2".into());
    assert!(
        component
            .cached_restyle_state_dependencies
            .contains(mesh_core_elements::PseudoState::Active)
    );
    assert!(
        component
            .cached_restyle_state_dependencies
            .contains(mesh_core_elements::PseudoState::Checked)
    );
    assert!(
        component
            .cached_restyle_state_dependencies
            .contains(mesh_core_elements::PseudoState::FocusVisible)
    );
    let changed = component.collect_interaction_changed_node_ids(component.last_tree.as_ref());
    assert!(
        changed
            .affected
            .contains(&runtime_node_id_for_key("root/0/0"))
    );
    assert!(
        changed
            .affected
            .contains(&runtime_node_id_for_key("root/0/1"))
    );
    assert!(
        changed
            .affected
            .contains(&runtime_node_id_for_key("root/0/2"))
    );
    component.invalidate_interaction_restyle();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();

    let tree = component.last_tree.as_ref().unwrap();
    let active_node = node_by_mesh_key(tree, "root/0/0");
    assert_eq!(active_node.id, runtime_node_id_for_key("root/0/0"));
    assert!(active_node.state.contains(NodeState::ACTIVE));
    assert!(
        node_by_mesh_key(tree, "root/0/1")
            .state
            .contains(NodeState::CHECKED)
    );
    assert!(
        node_by_mesh_key(tree, "root/0/2")
            .state
            .contains(NodeState::FOCUS_VISIBLE)
    );
    assert_eq!(
        node_by_mesh_key(tree, "root/0/0")
            .computed_style
            .font_weight,
        700
    );
    assert_eq!(node_by_mesh_key(tree, "root/0/1").computed_style.z_index, 7);
    assert_eq!(
        node_by_mesh_key(tree, "root/0/2").computed_style.text_align,
        mesh_core_elements::style::TextAlign::Right
    );

    component.pointer_down_id = None;
    component
        .checked_values
        .insert(runtime_node_id_for_key("root/0/1"), false);
    component.focus_visible_key = None;
    component.invalidate_interaction_restyle();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
        .unwrap();

    let tree = component.last_tree.as_ref().unwrap();
    assert_eq!(
        node_by_mesh_key(tree, "root/0/0")
            .computed_style
            .font_weight,
        400
    );
    assert_eq!(node_by_mesh_key(tree, "root/0/1").computed_style.z_index, 1);
    assert_eq!(
        node_by_mesh_key(tree, "root/0/2").computed_style.text_align,
        mesh_core_elements::style::TextAlign::Left
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
                button: 0x110,
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
                button: 0x110,
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
fn keyboard_focus_visible_ignores_shortcut_modifiers_and_key_releases() {
    // Click a button, then run a compositor shortcut (Super+Shift+S): neither
    // the lone modifier presses nor any release may paint the button's ring.
    let mut component = test_frontend_component("<template><box /></template>");
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        80.0,
        24.0,
        &[],
    )]));

    let theme = default_theme();
    let send = |component: &mut FrontendSurfaceComponent, input| {
        component.handle_input(&theme, 240, 160, input).unwrap();
    };
    send(
        &mut component,
        ComponentInput::PointerButton {
            button: 0x110,
            x: 8.0,
            y: 8.0,
            pressed: true,
        },
    );
    for key in ["Super_L", "Shift_L"] {
        send(
            &mut component,
            ComponentInput::KeyPressed {
                key: key.into(),
                modifiers: KeyModifiers::default(),
            },
        );
    }
    for key in ["s", "Shift_L", "Super_L"] {
        send(
            &mut component,
            ComponentInput::KeyReleased {
                key: key.into(),
                modifiers: KeyModifiers::default(),
            },
        );
    }
    assert_eq!(component.focused_key.as_deref(), Some("root/0"));
    assert!(component.focus_visible_key.is_none());

    send(
        &mut component,
        ComponentInput::KeyPressed {
            key: "Down".into(),
            modifiers: KeyModifiers::default(),
        },
    );
    assert_eq!(component.focus_visible_key.as_deref(), Some("root/0"));
}

#[test]
fn surface_auto_focus_does_not_paint_a_focus_ring() {
    let mut component = test_frontend_component("<template><box /></template>");
    let tree = root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        80.0,
        24.0,
        &[],
    )]);
    component.pending_auto_focus = true;
    component.apply_pending_auto_focus(&tree);

    assert_eq!(component.focused_key.as_deref(), Some("root/0"));
    assert!(component.focus_visible_key.is_none());
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
                button: 0x110,
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

#[test]
fn pointer_enter_and_leave_dispatch_hover_handlers() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
entered = false
left = false
function onHoverEnter() entered = true end
function onHoverLeave() left = true end
</script>
"#,
    );
    let button = event_node(
        "button",
        "root/0",
        10.0,
        10.0,
        40.0,
        40.0,
        &[
            ("pointerenter", "onHoverEnter"),
            ("pointerleave", "onHoverLeave"),
        ],
    );
    component.last_tree = Some(root_with(vec![button]));

    let theme = default_theme();
    // Moving onto the button fires pointerenter, not pointerleave.
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 20.0, y: 20.0 },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "entered"),
        Some(serde_json::json!(true))
    );
    assert_eq!(
        runtime_value(&component, "left"),
        Some(serde_json::json!(false))
    );

    // The pointer leaving the whole surface fires pointerleave.
    component
        .handle_input(&theme, 240, 160, ComponentInput::PointerLeave)
        .unwrap();
    assert_eq!(
        runtime_value(&component, "left"),
        Some(serde_json::json!(true))
    );
}

#[test]
fn mouse_enter_leave_aliases_dispatch_hover_handlers() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
entered = false
left = false
function onHoverEnter() entered = true end
function onHoverLeave() left = true end
</script>
"#,
    );
    let button = event_node(
        "button",
        "root/0",
        10.0,
        10.0,
        40.0,
        40.0,
        &[
            ("mouseenter", "onHoverEnter"),
            ("mouseleave", "onHoverLeave"),
        ],
    );
    component.last_tree = Some(root_with(vec![button]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 20.0, y: 20.0 },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "entered"),
        Some(serde_json::json!(true))
    );

    // Moving off the button (still inside the surface) fires mouseleave.
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 100.0, y: 100.0 },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "left"),
        Some(serde_json::json!(true))
    );
}

#[test]
fn moving_directly_between_sibling_buttons_fires_leave_and_enter_with_correct_bounds() {
    // A single pointer move can transition off one node and onto another in
    // the same frame; both nodes must be resolved and their handlers fired
    // with bounds/current_target matching their own layout, not each other's.
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
left_bounds = nil
entered_bounds = nil
function onLeaveA(event) left_bounds = event.current.bounds end
function onEnterB(event) entered_bounds = event.current.bounds end
</script>
"#,
    );
    let button_a = event_node(
        "button",
        "root/0",
        10.0,
        10.0,
        40.0,
        40.0,
        &[("pointerleave", "onLeaveA")],
    );
    let button_b = event_node(
        "button",
        "root/1",
        100.0,
        10.0,
        40.0,
        40.0,
        &[("pointerenter", "onEnterB")],
    );
    component.last_tree = Some(root_with(vec![button_a, button_b]));

    let theme = default_theme();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 20.0, y: 20.0 },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::PointerMove { x: 110.0, y: 20.0 },
        )
        .unwrap();

    assert_eq!(
        runtime_value(&component, "left_bounds")
            .and_then(|value| value.get("left").and_then(|v| v.as_f64())),
        Some(10.0)
    );
    assert_eq!(
        runtime_value(&component, "entered_bounds")
            .and_then(|value| value.get("left").and_then(|v| v.as_f64())),
        Some(100.0)
    );
}

#[test]
fn window_states_project_onto_the_tree_as_css_state() {
    // The compositor's toplevel states are ambient: they belong to the surface,
    // not to any node, but this CSS engine has no descendant combinators, so
    // they are carried on every node. A module differentiates its filling size
    // from its floating one by writing `:fullscreen` on the element that
    // sizes.
    let mut component = test_frontend_component(
        r#"
<style>
.window {
  width: 200px;
  height: 100px;
  background-color: #222222;
}
.window:fullscreen {
  width: 100%;
  height: 100%;
  background-color: #eeeeee;
}
</style>
<template>
  <column class="window">
    <text>Settings</text>
  </column>
</template>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(400, 300);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(400, 300), &mut buffer, 1.0)
        .unwrap();

    let floating = node_by_mesh_key(component.last_tree.as_ref().unwrap(), "root/0");
    assert!(!floating.state.window().fullscreen);
    assert_eq!(
        floating.computed_style.background_color,
        Color::from_hex("#222222").unwrap()
    );
    assert_eq!(floating.layout.width, 200.0);

    assert!(
        component.surface_window_states_changed(WindowStates {
            fullscreen: true,
            activated: true,
            ..WindowStates::default()
        }),
        "a new set of window states owes the surface a restyle"
    );
    assert!(component.wants_render());
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(400, 300), &mut buffer, 1.0)
        .unwrap();

    let filling = node_by_mesh_key(component.last_tree.as_ref().unwrap(), "root/0");
    assert!(filling.state.window().fullscreen);
    assert!(filling.state.window().activated);
    assert_eq!(
        filling.computed_style.background_color,
        Color::from_hex("#eeeeee").unwrap()
    );
    assert_eq!(
        filling.layout.width, 400.0,
        "a fullscreen root must take the compositor's size, not its floating one"
    );

    assert!(
        !component.surface_window_states_changed(WindowStates {
            fullscreen: true,
            activated: true,
            ..WindowStates::default()
        }),
        "repeating the same states must not dirty the surface"
    );
}

#[test]
fn theme_only_hover_rules_invalidate_and_restyle_their_nodes() {
    let mut component = test_frontend_component(
        r#"
<template><button>Theme hover</button></template>
<style>button { transition: none; }</style>
"#,
    );
    let mut theme = default_theme();
    theme.modules_mut().insert(
        component.id().into(),
        mesh_core_theme::ThemeModule {
            rules: vec![mesh_core_theme::ThemeStyleRule {
                selector: mesh_core_component::style::Selector::State(
                    "button".into(),
                    "hover".into(),
                ),
                declarations: [("background".into(), "#ff0000".into())]
                    .into_iter()
                    .collect(),
            }],
            ..Default::default()
        },
    );
    component.theme_changed().unwrap();
    let extent = SurfaceExtent::unpadded(240, 80);
    let mut buffer = PixelBuffer::new(240, 80);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert!(component.module_styles_have_hover_rules());
    let button = first_node_by_tag(component.last_tree.as_ref().unwrap(), "button").unwrap();
    let key = button.mesh_key().unwrap().to_string();
    let id = button.id;
    let resting = button.computed_style.background_color;
    component.hovered_key = Some(key);
    component.hovered_path = vec![id];
    component.invalidate_interaction_restyle();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        first_node_by_tag(component.last_tree.as_ref().unwrap(), "button")
            .unwrap()
            .computed_style
            .background_color,
        mesh_core_elements::Color::from_hex("#ff0000").unwrap()
    );
    component.hovered_key = None;
    component.hovered_path.clear();
    component.invalidate_interaction_restyle();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        first_node_by_tag(component.last_tree.as_ref().unwrap(), "button")
            .unwrap()
            .computed_style
            .background_color,
        resting
    );
}

#[test]
fn slider_parts_restyle_with_their_host_and_never_take_the_press() {
    let mut component = test_frontend_component(
        r#"
<template>
  <row>
    <slider class="level" value="40" />
  </row>
</template>
<style>
.level { width: 200px; height: 40px; padding: 0px; }
.level::track { height: 6px; margin-left: 0px; margin-right: 0px; }
slider::thumb { width: 36px; height: 22px; }
slider:active::thumb { width: 48px; height: 30px; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    let mut paint = |component: &mut FrontendSurfaceComponent| {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
            .unwrap();
        if component.wants_render() {
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
                .unwrap();
        }
    };
    paint(&mut component);
    let thumb = |component: &FrontendSurfaceComponent| {
        fn find(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.control_part() == Some(mesh_core_elements::ControlPart::SliderThumb) {
                return Some(node);
            }
            node.children.iter().find_map(find)
        }
        find(component.last_tree.as_ref().unwrap()).unwrap().clone()
    };
    let resting = thumb(&component);
    assert_eq!((resting.layout.width, resting.layout.height), (36.0, 22.0));
    let centre_x = resting.layout.x + resting.layout.width / 2.0;
    let centre_y = resting.layout.y + resting.layout.height / 2.0;

    // A press on the thumb lands on the slider, which owns input.
    let tree = component.last_tree.as_ref().unwrap();
    let hit = mesh_core_interaction::pointer_press_hit(tree, centre_x, centre_y);
    assert_eq!(hit.target.map(|target| target.key), Some("root/0/0"));
    // Parts expose no semantics; the slider does.
    assert!(!mesh_core_elements::NodeEligibility::local(&resting).is_semantically_visible());

    let slider_id = runtime_node_id_for_key("root/0/0");
    component.pointer_down_id = Some(slider_id);
    component.invalidate_interaction_restyle();
    paint(&mut component);
    let pressed = thumb(&component);
    assert_eq!((pressed.layout.width, pressed.layout.height), (48.0, 30.0));
    let pressed_centre = pressed.layout.x + pressed.layout.width / 2.0;
    assert!((pressed_centre - centre_x).abs() < 0.5, "the thumb grows around the value");

    component.pointer_down_id = None;
    component.invalidate_interaction_restyle();
    paint(&mut component);
    assert_eq!(thumb(&component).layout.width, 36.0);
}

#[test]
fn slider_thumb_transitions_animate_the_press() {
    let mut component = test_frontend_component(
        r#"
<template>
  <row>
    <slider value="40" />
  </row>
</template>
<style>
slider { width: 200px; height: 40px; }
slider::thumb { width: 36px; height: 22px; transition: width 10s linear, height 10s linear; }
slider:active::thumb { width: 48px; height: 30px; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    let mut paint = |component: &mut FrontendSurfaceComponent| {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
            .unwrap();
    };
    paint(&mut component);
    if component.wants_render() {
        paint(&mut component);
    }
    let thumb = |component: &FrontendSurfaceComponent| {
        fn find(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.control_part() == Some(mesh_core_elements::ControlPart::SliderThumb) {
                return Some(node);
            }
            node.children.iter().find_map(find)
        }
        find(component.last_tree.as_ref().unwrap()).unwrap().clone()
    };
    let thumb_id = thumb(&component).id;

    component.pointer_down_id = Some(runtime_node_id_for_key("root/0/0"));
    component.invalidate_interaction_restyle();
    paint(&mut component);

    let width = thumb(&component).layout.width;
    assert!(
        (36.0..48.0).contains(&width),
        "a ten-second transition has barely started, width {width}"
    );
    assert!(component.transitions.contains_key(thumb_id));
    assert!(component.wants_render(), "the running transition keeps frames coming");
}

#[test]
fn slider_value_changes_move_the_thumb() {
    let mut component = test_frontend_component(
        r#"
<template>
  <row>
    <slider min="0" max="100" step="10" value="40" />
  </row>
</template>
<style>
slider { width: 200px; height: 40px; padding: 0px; }
slider::track { margin-left: 0px; margin-right: 0px; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 80);
    let mut paint = |component: &mut FrontendSurfaceComponent| {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
            .unwrap();
        if component.wants_render() {
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
                .unwrap();
        }
    };
    paint(&mut component);
    let anchor_x = |component: &FrontendSurfaceComponent| {
        fn find(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.control_part() == Some(mesh_core_elements::ControlPart::SliderAnchor) {
                return Some(node);
            }
            node.children.iter().find_map(find)
        }
        let tree = component.last_tree.as_ref().unwrap();
        let slider = &tree.children[0].children[0];
        let anchor = find(tree).unwrap();
        anchor.layout.x + anchor.layout.width / 2.0 - slider.layout.x
    };
    assert!((anchor_x(&component) - 80.0).abs() < 0.5, "{}", anchor_x(&component));

    component.focused_key = Some("root/0/0".into());
    component
        .handle_input(
            &theme,
            240,
            80,
            ComponentInput::KeyPressed {
                key: "ArrowRight".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    paint(&mut component);
    assert!((anchor_x(&component) - 100.0).abs() < 0.5, "{}", anchor_x(&component));
}

#[test]
fn theme_declared_part_transitions_animate_without_component_motion_rules() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("theme.css"),
        "slider::thumb { width: 36px; height: 22px; transition: width 10s linear; }\n\
         slider:active::thumb { width: 48px; }\n",
    )
    .unwrap();
    let pack = mesh_core_theme::load_theme_from_source(
        &mesh_core_theme::ThemeSourceHandle::new(dir.path(), "theme.css").unwrap(),
        "parts",
        "Parts",
    )
    .unwrap();
    let theme = mesh_core_theme::Theme::compose_layers(
        &default_theme(),
        &pack,
        "@test/parts:parts",
        "dark",
        [],
        &std::collections::HashMap::new(),
    )
    .unwrap();
    let mut component = test_frontend_component(
        r#"
<template>
  <row>
    <slider value="40" />
  </row>
</template>
<style>
slider { width: 200px; height: 40px; }
</style>
"#,
    );
    let mut buffer = PixelBuffer::new(240, 80);
    let mut paint = |component: &mut FrontendSurfaceComponent| {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 80), &mut buffer, 1.0)
            .unwrap();
    };
    paint(&mut component);
    if component.wants_render() {
        paint(&mut component);
    }
    component.pointer_down_id = Some(runtime_node_id_for_key("root/0/0"));
    component.invalidate_interaction_restyle();
    paint(&mut component);

    fn thumb(node: &WidgetNode) -> Option<&WidgetNode> {
        if node.control_part() == Some(mesh_core_elements::ControlPart::SliderThumb) {
            return Some(node);
        }
        node.children.iter().find_map(thumb)
    }
    let thumb = thumb(component.last_tree.as_ref().unwrap()).unwrap();
    assert!(
        thumb.layout.width < 48.0,
        "the theme's transition animates the press, width {}",
        thumb.layout.width
    );
    assert!(component.transitions.contains_key(thumb.id));
}

#[test]
fn progress_fill_follows_script_value_writes() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    <progress value="{done}" max="8" />
  </column>
</template>
<script lang="luau">
done = 2
</script>
<style>
progress { width: 200px; height: 10px; padding: 0px; }
progress::track { height: 6px; }
progress::fill { background-color: #ff0000; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    let mut paint = |component: &mut FrontendSurfaceComponent| {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(240, 40), &mut buffer, 1.0)
            .unwrap();
        if component.wants_render() {
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(240, 40), &mut buffer, 1.0)
                .unwrap();
        }
    };
    paint(&mut component);
    let fill = |component: &FrontendSurfaceComponent| {
        fn find(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.control_part() == Some(mesh_core_elements::ControlPart::ProgressFill) {
                return Some(node);
            }
            node.children.iter().find_map(find)
        }
        find(component.last_tree.as_ref().unwrap()).unwrap().clone()
    };
    let before = fill(&component);
    assert_eq!((before.layout.width, before.layout.height), (50.0, 6.0));
    assert_eq!(
        before.computed_style.background_color,
        mesh_core_elements::style::Color::from_hex("#ff0000").unwrap()
    );

    component
        .run_script_call(None, "test", |script_ctx| {
            script_ctx
                .assign_global("done", &serde_json::json!(6))
                .map(|()| Vec::new())
        })
        .unwrap();
    paint(&mut component);
    assert_eq!(fill(&component).layout.width, 150.0);
}

#[test]
fn clicking_checkables_checks_their_parts() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    <checkbox class="agree" />
    <switch class="wifi" />
  </column>
</template>
<style>
column { padding: 0px; gap: 10px; align-items: start; }
switch::track { width: 40px; padding: 2px; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 80);
    let mut paint = |component: &mut FrontendSurfaceComponent| {
        for _ in 0..3 {
            component
                .paint_raster(&theme, SurfaceExtent::unpadded(120, 80), &mut buffer, 1.0)
                .unwrap();
            if !component.wants_render() {
                break;
            }
        }
    };
    paint(&mut component);
    fn part(node: &WidgetNode, wanted: mesh_core_elements::ControlPart) -> Option<&WidgetNode> {
        if node.control_part() == Some(wanted) {
            return Some(node);
        }
        node.children.iter().find_map(|child| part(child, wanted))
    }
    let find = |component: &FrontendSurfaceComponent, wanted| {
        part(component.last_tree.as_ref().unwrap(), wanted)
            .unwrap()
            .clone()
    };
    use mesh_core_elements::ControlPart::{CheckboxIndicator, SwitchThumb, SwitchTrack};
    let thumb_offset = |component: &FrontendSurfaceComponent| {
        find(component, SwitchThumb).layout.x - find(component, SwitchTrack).layout.x
    };
    assert!(!find(&component, CheckboxIndicator).state.contains(NodeState::CHECKED));
    assert_eq!(thumb_offset(&component), 2.0);

    let click = |component: &mut FrontendSurfaceComponent, node: &WidgetNode| {
        let (x, y) = (
            node.layout.x + node.layout.width / 2.0,
            node.layout.y + node.layout.height / 2.0,
        );
        for pressed in [true, false] {
            component
                .handle_input(
                    &theme,
                    120,
                    80,
                    ComponentInput::PointerButton {
                        x,
                        y,
                        button: 0x110,
                        pressed,
                    },
                )
                .unwrap();
        }
    };
    let indicator = find(&component, CheckboxIndicator);
    click(&mut component, &indicator);
    let thumb = find(&component, SwitchThumb);
    click(&mut component, &thumb);
    paint(&mut component);

    assert!(find(&component, CheckboxIndicator).state.contains(NodeState::CHECKED));
    assert_eq!(thumb_offset(&component), 40.0 - 2.0 - 16.0);
}

#[test]
fn switch_thumb_glides_with_its_margin_transition() {
    let mut component = test_frontend_component(
        r#"
<template>
  <column>
    <switch />
  </column>
</template>
<style>
column { padding: 0px; align-items: start; }
switch::track { width: 40px; padding: 2px; }
switch::thumb { transition: margin 1s linear; }
</style>
"#,
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 60);
    let mut paint = |component: &mut FrontendSurfaceComponent| {
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(120, 60), &mut buffer, 1.0)
            .unwrap();
    };
    paint(&mut component);
    if component.wants_render() {
        paint(&mut component);
    }
    fn part(node: &WidgetNode, wanted: mesh_core_elements::ControlPart) -> Option<&WidgetNode> {
        if node.control_part() == Some(wanted) {
            return Some(node);
        }
        node.children.iter().find_map(|child| part(child, wanted))
    }
    use mesh_core_elements::ControlPart::{SwitchThumb, SwitchTrack};
    let tree = || component.last_tree.as_ref().unwrap();
    let thumb = part(tree(), SwitchThumb).unwrap().clone();
    let track_x = part(tree(), SwitchTrack).unwrap().layout.x;
    let (x, y) = (
        thumb.layout.x + thumb.layout.width / 2.0,
        thumb.layout.y + thumb.layout.height / 2.0,
    );
    for pressed in [true, false] {
        component
            .handle_input(
                &theme,
                120,
                60,
                ComponentInput::PointerButton {
                    x,
                    y,
                    button: 0x110,
                    pressed,
                },
            )
            .unwrap();
    }
    paint(&mut component);
    std::thread::sleep(std::time::Duration::from_millis(200));
    paint(&mut component);
    let offset = part(component.last_tree.as_ref().unwrap(), SwitchThumb)
        .unwrap()
        .layout
        .x
        - track_x;
    assert!(
        offset > 2.0 && offset < 22.0,
        "a one-second glide is under way, offset {offset}"
    );
    assert!(component.wants_render(), "the glide keeps frames coming");
}
