use super::*;
use mesh_core_frontend_shell_adapter::ShellComponent;

fn host(contents: &str, script: &str) -> FrontendSurfaceComponent {
    let mut component = test_frontend_component_with_required_icons(
        &format!(
            r#"
<template><column class="root">{contents}</column></template>
<script lang="luau">{script}</script>
<style>
.root {{ width: 400px; height: 400px; }}
button {{ width: 80px; height: 24px; }}
.panel {{ width: 200px; height: fit; min-height: 30px; }}
.item {{ width: 100px; height: 24px; }}
</style>"#
        ),
        &[],
    );
    paint(&mut component);
    component
}

fn paint(component: &mut FrontendSurfaceComponent) {
    component
        .paint_raster(
            &default_theme(),
            SurfaceExtent::unpadded(400, 400),
            &mut PixelBuffer::new(400, 400),
            1.0,
        )
        .unwrap();
}

fn key_for(component: &FrontendSurfaceComponent, reference: &str) -> String {
    first_node_with_ref(component.last_tree.as_ref().unwrap(), reference)
        .unwrap()
        .mesh_key()
        .unwrap()
        .to_owned()
}

fn focus(component: &mut FrontendSurfaceComponent, reference: &str) {
    component.focused_key = Some(key_for(component, reference));
}

fn input(component: &mut FrontendSurfaceComponent, input: ComponentInput) {
    component
        .handle_input(&default_theme(), 400, 400, input)
        .unwrap();
    paint(component);
}

fn key(component: &mut FrontendSurfaceComponent, key: &str, shift: bool) {
    input(
        component,
        ComponentInput::KeyPressed {
            key: key.into(),
            modifiers: KeyModifiers {
                shift,
                ..Default::default()
            },
        },
    );
}

fn assert_focus(component: &FrontendSurfaceComponent, reference: &str) {
    assert_eq!(component.focused_key, Some(key_for(component, reference)));
}

const DIALOG: &str = r#"
<button ref="trigger" onclick={show}>Open</button>
<dialog class="panel" ref="dialog" open={opened} aria-label="Test" onopenchange={changed}>
  <button ref="first" onclick={nest}>First</button>
  <button ref="last">Last</button>
  <sheet class="panel" ref="sheet" open={nested} aria-label="Nested">
    <button ref="nested">Nested</button>
  </sheet>
</dialog>
<button ref="away" onclick={away}>Away</button>
"#;
const SCRIPT: &str = r#"
opened = false
nested = false
clicks = 0
reason = ""
function show() opened = true end
function nest() nested = true end
function away() clicks += 1 end
function changed(event) reason = event.reason end
"#;

fn open_dialog(component: &mut FrontendSurfaceComponent) {
    focus(component, "trigger");
    key(component, "Enter", false);
    assert_focus(component, "first");
}

#[test]
fn modal_tab_wraps_both_directions_and_escape_restores_trigger() {
    let mut component = host(DIALOG, SCRIPT);
    open_dialog(&mut component);
    key(&mut component, "Tab", true);
    assert_focus(&component, "last");
    key(&mut component, "Tab", false);
    assert_focus(&component, "first");
    key(&mut component, "Escape", false);
    assert!(!runtime_bool(&component, "opened"));
    assert_focus(&component, "trigger");
    let dialog = first_node_with_ref(component.last_tree.as_ref().unwrap(), "dialog").unwrap();
    assert_eq!(dialog.layout.height, 0.0);
    assert!(
        !mesh_core_elements::AccessibilityTree::publish(component.last_tree.as_ref().unwrap())
            .nodes
            .iter()
            .any(|node| node.id == dialog.id)
    );
}

#[test]
fn nested_modal_escape_closes_only_sheet_and_returns_to_its_opener() {
    let mut component = host(DIALOG, SCRIPT);
    open_dialog(&mut component);
    key(&mut component, "Enter", false);
    assert_focus(&component, "nested");
    key(&mut component, "Escape", false);
    assert!(!runtime_bool(&component, "nested"));
    assert!(runtime_bool(&component, "opened"));
    assert_focus(&component, "first");
}

#[test]
fn outside_press_dismisses_without_activating_background_on_release() {
    let mut component = host(DIALOG, SCRIPT);
    open_dialog(&mut component);
    let away = key_for(&component, "away");
    let (left, top, right, bottom) =
        find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &away, 0.0, 0.0).unwrap();
    let (x, y) = ((left + right) / 2.0, (top + bottom) / 2.0);
    input(
        &mut component,
        ComponentInput::PointerButton {
            x,
            y,
            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
            pressed: true,
        },
    );
    assert!(!runtime_bool(&component, "opened"));
    input(
        &mut component,
        ComponentInput::PointerButton {
            x,
            y,
            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
            pressed: false,
        },
    );
    assert_eq!(runtime_number(&component, "clicks"), 0.0);
    assert_focus(&component, "trigger");
}

#[test]
fn empty_modal_keeps_focus_and_tab_inside_scope() {
    let mut component = host(
        r#"<button ref="away"/><dialog ref="empty" class="panel" open="true" aria-label="Empty"/>"#,
        "",
    );
    assert_focus(&component, "empty");
    key(&mut component, "Tab", false);
    assert_focus(&component, "empty");
}

const MENU: &str = r#"
<menu class="panel" ref="menu" aria-label="Commands">
  <menu-item class="item" ref="alpha" aria-label="Alpha"/>
  <column>
    <menu-item class="item" ref="beta" aria-label="Beta"/>
    <menu-item class="item" ref="disabled" aria-label="Bravo" disabled="true"/>
    <menu-item class="item" ref="bravo" aria-label="Bravo"/>
  </column>
  <menu-item class="panel" ref="more" aria-label="More">
    <text>More</text>
    <menu class="panel" ref="submenu" open={submenu} aria-label="More commands">
      <menu-item class="item" ref="child1"><text>Child one</text></menu-item>
      <menu-item class="item" ref="child2"><text>Child two</text></menu-item>
    </menu>
  </menu-item>
</menu>
"#;

#[test]
fn menu_typeahead_accumulates_cycles_skips_disabled_and_expires() {
    let mut component = host(MENU, "submenu = false");
    focus(&mut component, "alpha");
    input(&mut component, ComponentInput::Char { ch: 'b' });
    assert_focus(&component, "beta");
    input(&mut component, ComponentInput::Char { ch: 'b' });
    assert_focus(&component, "bravo");
    component.primitives.typed_at = Some(Instant::now() - Duration::from_secs(1));
    input(
        &mut component,
        ComponentInput::TextInput { text: "be".into() },
    );
    assert_focus(&component, "beta");
    input(&mut component, ComponentInput::Char { ch: 't' });
    assert_focus(&component, "beta");
}

#[test]
fn menu_arrows_cross_wrappers_and_submenus_keep_their_own_roving_scope() {
    let mut component = host(MENU, "submenu = false");
    focus(&mut component, "alpha");
    key(&mut component, "ArrowDown", false);
    assert_focus(&component, "beta");
    key(&mut component, "ArrowDown", false);
    assert_focus(&component, "bravo");
    key(&mut component, "End", false);
    assert_focus(&component, "more");
    key(&mut component, "ArrowRight", false);
    assert!(runtime_bool(&component, "submenu"));
    assert_focus(&component, "child1");
    key(&mut component, "ArrowUp", false);
    assert_focus(&component, "child2");
    key(&mut component, "Escape", false);
    assert!(!runtime_bool(&component, "submenu"));
    assert_focus(&component, "more");
    key(&mut component, "Home", false);
    assert_focus(&component, "alpha");
}

#[test]
fn submenu_typeahead_uses_rendered_text_and_left_returns_to_parent_item() {
    let mut component = host(MENU, "submenu = false");
    focus(&mut component, "more");
    key(&mut component, "ArrowRight", false);
    input(
        &mut component,
        ComponentInput::TextInput {
            text: "child t".into(),
        },
    );
    assert_focus(&component, "child2");
    key(&mut component, "ArrowLeft", false);
    assert_focus(&component, "more");
    assert!(!runtime_bool(&component, "submenu"));
}

#[test]
fn opening_modal_does_not_activate_its_first_button_on_trigger_key_release() {
    let mut component = host(DIALOG, SCRIPT);
    open_dialog(&mut component);
    input(
        &mut component,
        ComponentInput::KeyReleased {
            key: "Enter".into(),
            modifiers: KeyModifiers::default(),
        },
    );
    assert!(!runtime_bool(&component, "nested"));
    assert_focus(&component, "first");
}

#[test]
fn clicking_a_submenu_parent_opens_and_focuses_the_child_menu() {
    let mut component = host(MENU, "submenu = false");
    let target = key_for(&component, "more");
    let (left, top, right, _) =
        find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &target, 0.0, 0.0).unwrap();
    let (x, y) = ((left + right) / 2.0, top + 5.0);
    input(
        &mut component,
        ComponentInput::PointerButton {
            x,
            y,
            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
            pressed: true,
        },
    );
    assert!(runtime_bool(&component, "submenu"));
    assert_focus(&component, "child1");
}
