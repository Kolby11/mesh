use super::reflow::component_with_embedded_popover;
use super::*;
use mesh_core_elements::HOVER_INTENT_CLOSE_DELAY;

const WIDTH: u32 = 200;
const HEIGHT: u32 = 30;

fn host(popover_src: &str) -> FrontendSurfaceComponent {
    component_with_embedded_popover(
        r#"
<template>
    <row class="bar">
        <button class="away" aria-label="Elsewhere" />
        <box class="spacer" />
        <button class="trigger" ref="trigger" title="Trigger tip" aria-label="Trigger">
            <EdgePopover />
        </button>
    </row>
</template>
<script lang="luau">
local EdgePopover = require("@mesh/test-popover")
</script>
<style>
.bar { width: 200px; height: 30px; }
.away { width: 24px; height: 24px; flex-shrink: 0; }
.spacer { flex-grow: 1; }
.trigger { width: 24px; height: 24px; flex-shrink: 0; }
.trigger:expanded { border-width: 3px; }
</style>
"#,
        popover_src,
    )
}

fn disclosure_popover(open_on: &str) -> String {
    format!(
        r#"
<template>
    <popover open={{open}} class="shell" anchor-ref="trigger" open-on="{open_on}"
        onopenchange={{onOpenChange}} aria-label="Panel">
        <column class="panel">
            <button class="inner" aria-label="Inner" />
            <slider class="level" min="0" max="100" value="{{level}}" />
        </column>
    </popover>
</template>
<script lang="luau">
open = false
level = 50
last_reason = ""
function onOpenChange(event)
    last_reason = event.reason
end
</script>
<style>
.shell {{ width: fit; height: fit; }}
.panel {{ width: 80px; height: 120px; }}
.inner {{ width: 40px; height: 20px; }}
.level {{ width: 20px; height: 80px; }}
</style>
"#
    )
}

fn repaint(component: &mut FrontendSurfaceComponent) {
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(WIDTH, HEIGHT);
    component
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(WIDTH, HEIGHT),
            &mut buffer,
            1.0,
        )
        .unwrap();
}

fn input(component: &mut FrontendSurfaceComponent, input: ComponentInput) -> Vec<CoreRequest> {
    let requests = component
        .handle_input(&default_theme(), WIDTH, HEIGHT, input)
        .unwrap();
    repaint(component);
    requests
}

fn popup_input(component: &mut FrontendSurfaceComponent, input: ComponentInput) {
    let key = popover_key(component);
    component
        .handle_child_surface_input(&key, &default_theme(), WIDTH, HEIGHT, (0.0, 0.0), input)
        .unwrap();
    repaint(component);
}

fn tree(component: &FrontendSurfaceComponent) -> &WidgetNode {
    component.last_tree.as_ref().expect("rendered tree")
}

fn popover_key(component: &FrontendSurfaceComponent) -> String {
    first_node_by_class(tree(component), "shell")
        .and_then(WidgetNode::mesh_key)
        .expect("popover key")
        .to_owned()
}

fn class_key(component: &FrontendSurfaceComponent, class: &str) -> String {
    first_node_by_class(tree(component), class)
        .and_then(WidgetNode::mesh_key)
        .unwrap_or_else(|| panic!("{class} key"))
        .to_owned()
}

fn center_of(component: &FrontendSurfaceComponent, class: &str) -> (f32, f32) {
    let key = class_key(component, class);
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree(component), &key, 0.0, 0.0).expect("bounds");
    ((left + right) / 2.0, (top + bottom) / 2.0)
}

/// Popup-local center of a node inside the promoted popover subtree.
fn popup_center_of(component: &FrontendSurfaceComponent, class: &str) -> (f32, f32) {
    let popover = first_node_by_class(tree(component), "shell").expect("popover");
    let node = first_node_by_class(popover, class).expect("popup node");
    (
        node.layout.x - popover.layout.x + node.layout.width / 2.0,
        node.layout.y - popover.layout.y + node.layout.height / 2.0,
    )
}

fn hover(component: &mut FrontendSurfaceComponent, class: &str) -> Vec<CoreRequest> {
    let (x, y) = center_of(component, class);
    input(component, ComponentInput::PointerMove { x, y })
}

fn script_value(component: &FrontendSurfaceComponent, name: &str) -> Option<serde_json::Value> {
    component
        .runtimes
        .lock()
        .unwrap()
        .values()
        .find_map(|runtime| runtime.script_ctx.state().get(name))
}

fn is_open(component: &FrontendSurfaceComponent) -> bool {
    component.child_surface_requests().len() == 1
}

fn last_reason(component: &FrontendSurfaceComponent) -> String {
    script_value(component, "last_reason")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn expire_hover_intent(component: &mut FrontendSurfaceComponent) {
    let tree = component.last_tree.take().expect("tree");
    let due = Instant::now() + HOVER_INTENT_CLOSE_DELAY + Duration::from_millis(1);
    let result = component.run_due_disclosures(&tree, due);
    component.last_tree = Some(tree);
    result.unwrap();
    repaint(component);
}

#[test]
fn hover_trigger_writes_open_binding_and_reports_the_reason() {
    let mut component = host(&disclosure_popover("hover"));
    assert!(!is_open(&component));

    hover(&mut component, "trigger");

    assert!(
        is_open(&component),
        "entering the trigger opens the popover"
    );
    assert_eq!(
        script_value(&component, "open"),
        Some(serde_json::json!(true))
    );
    assert_eq!(last_reason(&component), "hover");
}

#[test]
fn leaving_trigger_closes_only_after_hover_intent_expires() {
    let mut component = host(&disclosure_popover("hover"));
    hover(&mut component, "trigger");

    hover(&mut component, "spacer");
    assert!(is_open(&component), "the close waits for hover intent");
    let deadline = component
        .disclosures
        .next_deadline()
        .expect("close scheduled");
    assert!(deadline > Instant::now() + HOVER_INTENT_CLOSE_DELAY / 2);

    expire_hover_intent(&mut component);
    assert!(!is_open(&component));
    assert_eq!(last_reason(&component), "leave");
}

#[test]
fn crossing_into_the_popup_cancels_the_pending_close() {
    let mut component = host(&disclosure_popover("hover"));
    hover(&mut component, "trigger");
    input(&mut component, ComponentInput::PointerLeave);
    assert!(component.disclosures.next_deadline().is_some());

    let (x, y) = popup_center_of(&component, "panel");
    popup_input(&mut component, ComponentInput::PointerMove { x, y });

    assert_eq!(component.disclosures.next_deadline(), None);
    expire_hover_intent(&mut component);
    assert!(is_open(&component), "the pointer is over the popup");
}

#[test]
fn a_drag_started_in_the_popup_holds_it_open_until_release() {
    let mut component = host(&disclosure_popover("hover"));
    hover(&mut component, "trigger");
    input(&mut component, ComponentInput::PointerLeave);
    let (x, y) = popup_center_of(&component, "level");
    popup_input(&mut component, ComponentInput::PointerMove { x, y });
    popup_input(
        &mut component,
        ComponentInput::PointerButton {
            x,
            y,
            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
            pressed: true,
        },
    );

    popup_input(&mut component, ComponentInput::PointerLeave);
    assert_eq!(
        component.disclosures.next_deadline(),
        None,
        "a captured drag keeps the popover open"
    );

    popup_input(
        &mut component,
        ComponentInput::PointerButton {
            x,
            y: y + 200.0,
            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
            pressed: false,
        },
    );
    assert!(component.disclosures.next_deadline().is_some());
    expire_hover_intent(&mut component);
    assert!(!is_open(&component));
}

#[test]
fn escape_inside_the_popup_closes_it_and_returns_focus_to_the_trigger() {
    let mut component = host(&disclosure_popover("hover"));
    hover(&mut component, "trigger");
    let inner = class_key(&component, "inner");
    component.focused_key = Some(inner);

    input(
        &mut component,
        ComponentInput::KeyPressed {
            key: "Escape".into(),
            modifiers: KeyModifiers::default(),
        },
    );
    repaint(&mut component);

    assert!(!is_open(&component));
    assert_eq!(last_reason(&component), "escape");
    assert_eq!(
        component.focused_key.as_deref(),
        Some(class_key(&component, "trigger").as_str())
    );
}

#[test]
fn trigger_keyboard_activation_opens_and_focuses_the_popup() {
    let mut component = host(&disclosure_popover("hover"));
    let trigger = class_key(&component, "trigger");
    component.focused_key = Some(trigger);

    input(
        &mut component,
        ComponentInput::KeyPressed {
            key: "Enter".into(),
            modifiers: KeyModifiers::default(),
        },
    );
    repaint(&mut component);

    assert!(is_open(&component));
    assert_eq!(last_reason(&component), "keyboard");
    assert_eq!(
        component.focused_key.as_deref(),
        Some(class_key(&component, "inner").as_str()),
        "focus moves to the first control in the popup"
    );
}

#[test]
fn click_mode_toggles_on_the_trigger_and_closes_on_an_outside_press() {
    let mut component = host(&disclosure_popover("click"));
    let press = |component: &mut FrontendSurfaceComponent, class: &str| {
        let (x, y) = center_of(component, class);
        input(component, ComponentInput::PointerMove { x, y });
        for pressed in [true, false] {
            input(
                component,
                ComponentInput::PointerButton {
                    x,
                    y,
                    button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
                    pressed,
                },
            );
        }
    };

    hover(&mut component, "trigger");
    assert!(!is_open(&component), "click mode ignores hover");

    press(&mut component, "trigger");
    assert!(is_open(&component));
    assert_eq!(last_reason(&component), "trigger");

    press(&mut component, "away");
    assert!(!is_open(&component));
    assert_eq!(last_reason(&component), "outside");
}

#[test]
fn an_open_popover_expands_its_trigger_and_hides_its_tooltip() {
    let mut component = host(&disclosure_popover("hover"));
    let closed_border = first_node_by_class(tree(&component), "trigger")
        .expect("trigger")
        .computed_style
        .border_width;

    hover(&mut component, "trigger");

    let trigger = first_node_by_class(tree(&component), "trigger").expect("trigger");
    assert!(trigger.state.expanded, "trigger matches :expanded");
    assert_ne!(trigger.computed_style.border_width, closed_border);
    assert_eq!(
        component.hovered_tooltip, None,
        "the open popover hides the tip"
    );
    let trigger_id = trigger.id;
    let semantics = mesh_core_elements::AccessibilityTree::publish(tree(&component));
    let trigger_semantics = semantics
        .nodes
        .iter()
        .find(|node| node.id == trigger_id)
        .expect("trigger semantics");
    assert_eq!(trigger_semantics.info.state.expanded, Some(true));

    hover(&mut component, "spacer");
    expire_hover_intent(&mut component);
    let trigger = first_node_by_class(tree(&component), "trigger").expect("trigger");
    assert!(!trigger.state.expanded);
    assert_eq!(trigger.computed_style.border_width, closed_border);
}

#[test]
fn a_script_write_to_open_is_observed_without_a_second_copy() {
    let mut component = host(&disclosure_popover("hover"));
    hover(&mut component, "trigger");
    assert!(is_open(&component));

    let popover_instance = component
        .runtimes
        .lock()
        .unwrap()
        .iter()
        .find(|(_, runtime)| runtime.script_ctx.state().get("last_reason").is_some())
        .map(|(key, _)| key.to_string())
        .expect("popover instance");
    component
        .run_script_call(Some(&popover_instance), "test", |script_ctx| {
            script_ctx
                .assign_global("open", &serde_json::json!(false))
                .map(|()| Vec::new())
        })
        .unwrap();
    repaint(&mut component);

    assert!(!is_open(&component));
    repaint(&mut component);
    assert!(
        !first_node_by_class(tree(&component), "trigger")
            .expect("trigger")
            .state
            .expanded,
        "the trigger follows a script close"
    );
}
