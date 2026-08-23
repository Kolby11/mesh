/// UI event types and dispatch.
use crate::element::element_contract_for_tag;
use crate::style::{Display, Visibility};
use crate::tree::{ElementState, NodeId, WidgetNode};

/// A UI event targeted at a specific node.
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    PointerDown {
        node_id: NodeId,
        x: f32,
        y: f32,
        button: u32,
    },
    PointerUp {
        node_id: NodeId,
        x: f32,
        y: f32,
        button: u32,
    },
    /// A successful primary-button activation. This is separate from the raw
    /// `PointerUp`, which is delivered to the press origin after capture.
    Click {
        node_id: NodeId,
        x: f32,
        y: f32,
        button: u32,
    },
    PointerMove {
        node_id: NodeId,
        x: f32,
        y: f32,
    },
    PointerEnter {
        node_id: NodeId,
    },
    PointerLeave {
        node_id: NodeId,
    },
    KeyDown {
        node_id: NodeId,
        key: String,
        modifiers: Modifiers,
    },
    KeyUp {
        node_id: NodeId,
        key: String,
        modifiers: Modifiers,
    },
    Focus {
        node_id: NodeId,
    },
    Blur {
        node_id: NodeId,
    },
    Scroll {
        node_id: NodeId,
        dx: f32,
        dy: f32,
    },
}

impl UiEvent {
    /// The node this event targets.
    pub fn node_id(&self) -> NodeId {
        match self {
            Self::PointerDown { node_id, .. }
            | Self::PointerUp { node_id, .. }
            | Self::Click { node_id, .. }
            | Self::PointerMove { node_id, .. }
            | Self::PointerEnter { node_id }
            | Self::PointerLeave { node_id }
            | Self::KeyDown { node_id, .. }
            | Self::KeyUp { node_id, .. }
            | Self::Focus { node_id }
            | Self::Blur { node_id }
            | Self::Scroll { node_id, .. } => *node_id,
        }
    }

    /// The event name used to look up script handlers (e.g. "click", "change").
    pub fn handler_name(&self) -> &str {
        match self {
            Self::PointerDown { .. } => "pointerdown",
            Self::PointerUp { .. } => "pointerup",
            Self::Click { .. } => "click",
            Self::PointerMove { .. } => "pointermove",
            Self::PointerEnter { .. } => "pointerenter",
            Self::PointerLeave { .. } => "pointerleave",
            Self::KeyDown { .. } => "keydown",
            Self::KeyUp { .. } => "keyup",
            Self::Focus { .. } => "focus",
            Self::Blur { .. } => "blur",
            Self::Scroll { .. } => "scroll",
        }
    }
}

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
}

/// A raw input event from the Wayland backend.
#[derive(Debug, Clone)]
pub enum RawInputEvent {
    PointerMotion {
        x: f32,
        y: f32,
    },
    PointerButton {
        x: f32,
        y: f32,
        button: u32,
        pressed: bool,
    },
    Key {
        keycode: u32,
        pressed: bool,
        modifiers: Modifiers,
    },
    Scroll {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
}

/// The output of one stateful input dispatch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputDispatchResult {
    pub events: Vec<UiEvent>,
    /// Nodes whose interaction pseudo-state changed during this dispatch.
    pub changed_nodes: Vec<NodeId>,
}

impl InputDispatchResult {
    fn push_changed(&mut self, node_id: NodeId) {
        if !self.changed_nodes.contains(&node_id) {
            self.changed_nodes.push(node_id);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PressOrigin {
    node_id: NodeId,
    button: u32,
}

/// Tracks pointer and keyboard interaction state across frames and is the
/// canonical stateful event dispatcher.
///
/// Call `process` with each raw input event to update node state flags
/// (hover, active, focus) on the widget tree and produce the resulting UI events.
/// Use [`Self::dispatch`] when the changed-node invalidation set is needed by
/// a style resolver; `process` remains a convenience wrapper returning only
/// events.
#[derive(Debug, Default)]
pub struct EventDispatcher {
    hovered_node: Option<NodeId>,
    active_node: Option<NodeId>,
    focused_node: Option<NodeId>,
    pointer_capture: Option<PressOrigin>,
}

/// Compatibility name for the canonical stateful dispatcher.
pub type InputState = EventDispatcher;

impl EventDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Find the deepest node at the given coordinates.
    pub fn hit_test(root: &WidgetNode, x: f32, y: f32) -> Option<NodeId> {
        hit_test_node(root, x, y)
    }

    /// Apply one raw input event through the canonical stateful route and
    /// return both targeted events and the nodes needing interaction restyle.
    pub fn dispatch(&mut self, root: &mut WidgetNode, raw: &RawInputEvent) -> InputDispatchResult {
        let mut result = InputDispatchResult::default();
        self.reconcile_invalid_state(root, &mut result);

        match raw {
            RawInputEvent::PointerMotion { x, y } => {
                let new_hovered = EventDispatcher::hit_test(root, *x, *y);
                self.update_hover(root, new_hovered, &mut result);

                if let Some(node_id) = self
                    .pointer_capture
                    .map(|capture| capture.node_id)
                    .or(new_hovered)
                    .filter(|node_id| root.find(*node_id).is_some())
                {
                    result.events.push(UiEvent::PointerMove {
                        node_id,
                        x: *x,
                        y: *y,
                    });
                }
            }

            RawInputEvent::PointerButton {
                x,
                y,
                button,
                pressed,
            } => {
                if *pressed {
                    let target = EventDispatcher::hit_test(root, *x, *y)
                        .filter(|node_id| node_can_receive_pointer(root, *node_id));
                    self.update_hover(root, target, &mut result);
                    self.clear_pointer_capture(root, &mut result);

                    // Pointer focus goes to the nearest eligible control, not
                    // an arbitrary painted descendant.
                    if let Some(focus_target) = focusable_at_point(root, *x, *y) {
                        self.set_focus_inner(root, Some(focus_target), &mut result);
                    } else {
                        self.set_focus_inner(root, None, &mut result);
                    }

                    if let Some(node_id) = target {
                        set_state_flag(root, node_id, |s| s.active = true, &mut result);
                        self.active_node = Some(node_id);
                        self.pointer_capture = Some(PressOrigin {
                            node_id,
                            button: *button,
                        });
                        result.events.push(UiEvent::PointerDown {
                            node_id,
                            x: *x,
                            y: *y,
                            button: *button,
                        });
                    }
                } else {
                    let capture = self.pointer_capture.take();
                    let release_target = capture
                        .map(|capture| capture.node_id)
                        .or(self.active_node.take());
                    if let Some(node_id) = release_target {
                        set_state_flag(root, node_id, |s| s.active = false, &mut result);
                        if root.find(node_id).is_some() {
                            // Raw release remains owned by the press origin;
                            // hit testing cannot retarget it to another node.
                            result.events.push(UiEvent::PointerUp {
                                node_id,
                                x: *x,
                                y: *y,
                                button: *button,
                            });
                            if capture.is_some_and(|origin| {
                                origin.button == *button && is_primary_button(*button)
                            }) && node_contains_point(root, node_id, *x, *y)
                            {
                                result.events.push(UiEvent::Click {
                                    node_id,
                                    x: *x,
                                    y: *y,
                                    button: *button,
                                });
                            }
                        }
                    }
                }
            }

            RawInputEvent::Key {
                keycode,
                pressed,
                modifiers,
            } => {
                let node_id = self
                    .focused_node
                    .filter(|id| node_is_focusable_target(root, *id))
                    .unwrap_or(root.id);
                let key = format!("{keycode}");
                if *pressed {
                    result.events.push(UiEvent::KeyDown {
                        node_id,
                        key,
                        modifiers: *modifiers,
                    });
                } else {
                    result.events.push(UiEvent::KeyUp {
                        node_id,
                        key,
                        modifiers: *modifiers,
                    });
                }
            }

            RawInputEvent::Scroll { x, y, dx, dy } => {
                if let Some(node_id) = EventDispatcher::hit_test(root, *x, *y) {
                    result.events.push(UiEvent::Scroll {
                        node_id,
                        dx: *dx,
                        dy: *dy,
                    });
                }
            }
        }

        result
    }

    /// Apply a raw input event and return only its UI events.
    pub fn process(&mut self, root: &mut WidgetNode, raw: &RawInputEvent) -> Vec<UiEvent> {
        self.dispatch(root, raw).events
    }

    /// Returns the currently hovered node, if any.
    pub fn hovered_node(&self) -> Option<NodeId> {
        self.hovered_node
    }

    /// Returns the currently focused node, if any.
    pub fn focused_node(&self) -> Option<NodeId> {
        self.focused_node
    }

    /// Returns the currently active (pressed) node, if any.
    pub fn active_node(&self) -> Option<NodeId> {
        self.active_node
    }

    /// Returns the node currently holding pointer capture.
    pub fn pointer_capture_node(&self) -> Option<NodeId> {
        self.pointer_capture.map(|capture| capture.node_id)
    }

    /// Explicitly move keyboard focus to a node (e.g. for Tab navigation).
    pub fn set_focus(&mut self, root: &mut WidgetNode, target: Option<NodeId>) -> Vec<UiEvent> {
        self.set_focus_with_invalidation(root, target).events
    }

    /// Explicitly move keyboard focus and report the nodes requiring restyle.
    pub fn set_focus_with_invalidation(
        &mut self,
        root: &mut WidgetNode,
        target: Option<NodeId>,
    ) -> InputDispatchResult {
        let mut result = InputDispatchResult::default();
        let target = target.filter(|id| node_is_focusable_target(root, *id));
        self.set_focus_inner(root, target, &mut result);
        result
    }

    /// Reset all tracked state (e.g. when the surface loses pointer/keyboard focus).
    pub fn reset(&mut self, root: &mut WidgetNode) {
        self.reset_with_invalidation(root);
    }

    /// Reset all tracked state and report the nodes requiring restyle.
    pub fn reset_with_invalidation(&mut self, root: &mut WidgetNode) -> InputDispatchResult {
        let mut result = InputDispatchResult::default();
        if let Some(id) = self.hovered_node.take() {
            set_state_flag(root, id, |s| s.hovered = false, &mut result);
        }
        self.clear_pointer_capture(root, &mut result);
        if let Some(id) = self.focused_node.take() {
            set_state_flag(root, id, |s| s.focused = false, &mut result);
        }
        result
    }

    fn update_hover(
        &mut self,
        root: &mut WidgetNode,
        new_hovered: Option<NodeId>,
        result: &mut InputDispatchResult,
    ) {
        if new_hovered == self.hovered_node {
            return;
        }
        if let Some(old_id) = self.hovered_node {
            set_state_flag(root, old_id, |s| s.hovered = false, result);
            result
                .events
                .push(UiEvent::PointerLeave { node_id: old_id });
        }
        if let Some(new_id) = new_hovered {
            set_state_flag(root, new_id, |s| s.hovered = true, result);
            result
                .events
                .push(UiEvent::PointerEnter { node_id: new_id });
        }
        self.hovered_node = new_hovered;
    }

    fn set_focus_inner(
        &mut self,
        root: &mut WidgetNode,
        target: Option<NodeId>,
        result: &mut InputDispatchResult,
    ) {
        if self.focused_node == target {
            return;
        }
        if let Some(old_id) = self.focused_node {
            set_state_flag(root, old_id, |s| s.focused = false, result);
            result.events.push(UiEvent::Blur { node_id: old_id });
        }
        if let Some(new_id) = target {
            set_state_flag(root, new_id, |s| s.focused = true, result);
            result.events.push(UiEvent::Focus { node_id: new_id });
        }
        self.focused_node = target;
    }

    fn clear_pointer_capture(&mut self, root: &mut WidgetNode, result: &mut InputDispatchResult) {
        self.pointer_capture = None;
        if let Some(id) = self.active_node.take() {
            set_state_flag(root, id, |s| s.active = false, result);
        }
    }

    fn reconcile_invalid_state(&mut self, root: &mut WidgetNode, result: &mut InputDispatchResult) {
        if self.hovered_node.is_some_and(|id| root.find(id).is_none()) {
            self.hovered_node = None;
        }
        if let Some(id) = self.focused_node
            && !node_is_focusable_target(root, id)
        {
            self.focused_node = None;
            if root.find(id).is_some() {
                set_state_flag(root, id, |s| s.focused = false, result);
                result.events.push(UiEvent::Blur { node_id: id });
            }
        }
        if self
            .active_node
            .is_some_and(|id| !node_is_pointer_target(root, id))
            || self
                .pointer_capture
                .is_some_and(|capture| !node_is_pointer_target(root, capture.node_id))
        {
            self.clear_pointer_capture(root, result);
        }
    }
}

fn set_state_flag(
    root: &mut WidgetNode,
    id: NodeId,
    f: impl FnOnce(&mut ElementState),
    result: &mut InputDispatchResult,
) {
    if let Some(node) = root.find_mut(id) {
        let before = node.state;
        f(&mut node.state);
        if before != node.state {
            result.push_changed(id);
        }
    }
}

fn is_primary_button(button: u32) -> bool {
    matches!(button, 0x110)
}

fn node_is_hidden(node: &WidgetNode) -> bool {
    matches!(node.computed_style.display, Display::None)
        || !matches!(node.computed_style.visibility, Visibility::Visible)
}

fn boolean_attribute(node: &WidgetNode, name: &str) -> bool {
    node.attributes.get(name).is_some_and(|value| {
        value.is_empty() || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case(name)
    })
}

fn node_is_disabled(node: &WidgetNode) -> bool {
    node.state.disabled
        || boolean_attribute(node, "disabled")
        || boolean_attribute(node, "aria-disabled")
        || boolean_attribute(node, "inert")
}

fn node_is_focusable(node: &WidgetNode) -> bool {
    if node_is_hidden(node) || node_is_disabled(node) {
        return false;
    }
    if node.accessibility.focusable || node.attributes.contains_key("tabindex") {
        return true;
    }
    let source_tag = node
        .attributes
        .get("data-mesh-element")
        .map(String::as_str)
        .unwrap_or(node.tag.as_str());
    element_contract_for_tag(source_tag).is_some_and(|contract| contract.accessibility.focusable)
}

fn node_can_receive_pointer(root: &WidgetNode, node_id: NodeId) -> bool {
    node_is_pointer_target(root, node_id)
}

fn node_is_pointer_target(root: &WidgetNode, target: NodeId) -> bool {
    node_is_pointer_target_with_ancestors(root, target, false)
}

fn node_is_pointer_target_with_ancestors(
    node: &WidgetNode,
    target: NodeId,
    blocked_ancestor: bool,
) -> bool {
    let blocked = blocked_ancestor || node_is_disabled(node) || node_is_hidden(node);
    if node.id == target {
        return !blocked;
    }
    node.children
        .iter()
        .any(|child| node_is_pointer_target_with_ancestors(child, target, blocked))
}

fn node_is_focusable_target(root: &WidgetNode, target: NodeId) -> bool {
    node_is_focusable_target_with_ancestors(root, target, false)
}

fn node_is_focusable_target_with_ancestors(
    node: &WidgetNode,
    target: NodeId,
    blocked_ancestor: bool,
) -> bool {
    let blocked = blocked_ancestor || node_is_disabled(node) || node_is_hidden(node);
    if node.id == target {
        return !blocked && node_is_focusable(node);
    }
    node.children
        .iter()
        .any(|child| node_is_focusable_target_with_ancestors(child, target, blocked))
}

fn focusable_at_point(root: &WidgetNode, x: f32, y: f32) -> Option<NodeId> {
    focusable_at_point_with_offset(root, x, y, 0.0, 0.0, false)
}

fn focusable_at_point_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
    disabled_ancestor: bool,
) -> Option<NodeId> {
    if node_is_hidden(node) {
        return None;
    }
    let disabled = disabled_ancestor || node_is_disabled(node);
    let inside_self = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside_self && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(id) =
            focusable_at_point_with_offset(child, x, y, child_offset_x, child_offset_y, disabled)
        {
            return Some(id);
        }
    }
    (inside_self && !disabled && node_is_focusable(node)).then_some(node.id)
}

fn node_contains_point(root: &WidgetNode, target: NodeId, x: f32, y: f32) -> bool {
    node_contains_point_with_offset(root, target, x, y, 0.0, 0.0)
}

fn node_contains_point_with_offset(
    node: &WidgetNode,
    target: NodeId,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if node.id == target {
        return inside;
    }
    if !inside && node_clips_children(node) {
        return false;
    }
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    node.children.iter().any(|child| {
        node_contains_point_with_offset(child, target, x, y, child_offset_x, child_offset_y)
    })
}

fn hit_test_node(node: &WidgetNode, x: f32, y: f32) -> Option<NodeId> {
    hit_test_node_with_offset(node, x, y, 0.0, 0.0)
}

fn hit_test_node_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<NodeId> {
    let inside_self = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside_self && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(id) = hit_test_node_with_offset(child, x, y, child_offset_x, child_offset_y) {
            return Some(id);
        }
    }

    if inside_self { Some(node.id) } else { None }
}

fn layout_contains_with_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> bool {
    let left = node.layout.x + offset_x;
    let top = node.layout.y + offset_y;
    x >= left && x < left + node.layout.width && y >= top && y < top + node.layout.height
}

fn child_offsets_with_scroll(node: &WidgetNode, offset_x: f32, offset_y: f32) -> (f32, f32) {
    let scroll = node.resolved_scroll_metrics();
    (offset_x - scroll.x, offset_y - scroll.y)
}

fn node_clips_children(node: &WidgetNode) -> bool {
    node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutEngine;
    use crate::style::Dimension;

    fn two_button_fixture() -> (WidgetNode, NodeId, NodeId) {
        let mut root = WidgetNode::new("root");
        root.computed_style.width = Dimension::Px(200.0);
        root.computed_style.height = Dimension::Px(100.0);

        let mut left = WidgetNode::new("button");
        left.computed_style.width = Dimension::Px(100.0);
        left.computed_style.height = Dimension::Px(100.0);
        let left_id = left.id;

        let mut right = WidgetNode::new("button");
        right.computed_style.width = Dimension::Px(100.0);
        right.computed_style.height = Dimension::Px(100.0);
        let right_id = right.id;

        root.children = vec![left, right].into();
        LayoutEngine::compute(&mut root, 200.0, 100.0);
        (root, left_id, right_id)
    }

    #[test]
    fn hit_test_finds_deepest_node() {
        let mut root = WidgetNode::new("root");
        root.computed_style.width = Dimension::Px(200.0);
        root.computed_style.height = Dimension::Px(100.0);

        let mut child = WidgetNode::new("button");
        child.computed_style.width = Dimension::Px(100.0);
        child.computed_style.height = Dimension::Px(50.0);
        let child_id = child.id;

        root.children = vec![child].into();
        LayoutEngine::compute(&mut root, 200.0, 100.0);

        // Inside the child.
        assert_eq!(EventDispatcher::hit_test(&root, 50.0, 25.0), Some(child_id));
        // Outside the child but inside root.
        assert_eq!(EventDispatcher::hit_test(&root, 150.0, 75.0), Some(root.id));
        // Outside everything.
        assert_eq!(EventDispatcher::hit_test(&root, 300.0, 300.0), None);
    }

    #[test]
    fn dispatch_preserves_pointer_button_identity() {
        let mut root = WidgetNode::new("root");
        root.computed_style.width = Dimension::Px(200.0);
        root.computed_style.height = Dimension::Px(100.0);
        root.children = vec![WidgetNode::new("button")].into();
        LayoutEngine::compute(&mut root, 200.0, 100.0);

        let mut dispatcher = InputState::new();
        let result = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 10.0,
                y: 10.0,
                button: 0x111,
                pressed: true,
            },
        );
        assert!(matches!(
            result
                .events
                .iter()
                .find(|event| matches!(event, UiEvent::PointerDown { .. })),
            Some(UiEvent::PointerDown { button: 0x111, .. })
        ));
    }

    #[test]
    fn stateful_dispatcher_captures_press_origin_and_separates_activation() {
        let (mut root, left_id, right_id) = two_button_fixture();
        let mut dispatcher = InputState::new();

        let down = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 25.0,
                y: 25.0,
                button: 0x110,
                pressed: true,
            },
        );
        assert!(down.changed_nodes.contains(&left_id));
        assert_eq!(dispatcher.pointer_capture_node(), Some(left_id));
        assert_eq!(dispatcher.active_node(), Some(left_id));
        assert_eq!(dispatcher.focused_node(), Some(left_id));

        let moved = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerMotion { x: 175.0, y: 25.0 },
        );
        assert!(moved.changed_nodes.contains(&right_id));
        assert!(moved.events.iter().any(|event| {
            matches!(event, UiEvent::PointerMove { node_id, .. } if *node_id == left_id)
        }));

        let cancelled = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 175.0,
                y: 25.0,
                button: 0x110,
                pressed: false,
            },
        );
        assert_eq!(dispatcher.pointer_capture_node(), None);
        assert_eq!(dispatcher.active_node(), None);
        assert!(cancelled.events.iter().any(|event| {
            matches!(event, UiEvent::PointerUp { node_id, .. } if *node_id == left_id)
        }));
        assert!(
            !cancelled
                .events
                .iter()
                .any(|event| matches!(event, UiEvent::Click { .. }))
        );

        dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 25.0,
                y: 25.0,
                button: 0x111,
                pressed: true,
            },
        );
        let secondary_up = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 25.0,
                y: 25.0,
                button: 0x111,
                pressed: false,
            },
        );
        assert!(
            !secondary_up
                .events
                .iter()
                .any(|event| matches!(event, UiEvent::Click { .. }))
        );

        dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 25.0,
                y: 25.0,
                button: 0x110,
                pressed: true,
            },
        );
        dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerMotion { x: 175.0, y: 25.0 },
        );
        dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerMotion { x: 25.0, y: 25.0 },
        );
        let activated = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 25.0,
                y: 25.0,
                button: 0x110,
                pressed: false,
            },
        );
        assert!(activated.events.iter().any(|event| {
            matches!(event, UiEvent::Click { node_id, .. } if *node_id == left_id)
        }));
        assert_eq!(
            UiEvent::PointerUp {
                node_id: left_id,
                x: 0.0,
                y: 0.0,
                button: 0x110,
            }
            .handler_name(),
            "pointerup"
        );
        assert_eq!(
            UiEvent::Click {
                node_id: left_id,
                x: 0.0,
                y: 0.0,
                button: 0x110,
            }
            .handler_name(),
            "click"
        );
    }

    #[test]
    fn focus_and_activation_reject_disabled_targets_and_keyboard_uses_focus() {
        let (mut root, left_id, _) = two_button_fixture();
        root.children[0]
            .attributes
            .insert("disabled".into(), "true".into());
        let mut dispatcher = InputState::new();

        let down = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 25.0,
                y: 25.0,
                button: 0x110,
                pressed: true,
            },
        );
        assert!(down.events.is_empty());
        assert!(down.changed_nodes.is_empty());
        assert_eq!(dispatcher.focused_node(), None);

        root.children[0].attributes.remove("disabled");
        root.attributes.insert("inert".into(), "true".into());
        let blocked_by_ancestor = dispatcher.set_focus_with_invalidation(&mut root, Some(left_id));
        assert!(blocked_by_ancestor.events.is_empty());
        assert!(blocked_by_ancestor.changed_nodes.is_empty());
        root.attributes.remove("inert");

        let focus_events = dispatcher.set_focus_with_invalidation(&mut root, Some(left_id));
        assert!(focus_events.changed_nodes.contains(&left_id));
        let key = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::Key {
                keycode: 28,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        );
        assert!(key.events.iter().any(|event| {
            matches!(event, UiEvent::KeyDown { node_id, .. } if *node_id == left_id)
        }));
    }

    #[test]
    fn dispatch_reports_state_changes_and_drops_removed_press_origins() {
        let (mut root, left_id, _) = two_button_fixture();
        let mut dispatcher = InputState::new();
        dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 25.0,
                y: 25.0,
                button: 0x110,
                pressed: true,
            },
        );
        root.children.remove(0);

        let released = dispatcher.dispatch(
            &mut root,
            &RawInputEvent::PointerButton {
                x: 175.0,
                y: 25.0,
                button: 0x110,
                pressed: false,
            },
        );
        assert!(released.events.iter().all(|event| {
            !matches!(event, UiEvent::PointerUp { node_id, .. } | UiEvent::Click { node_id, .. } if *node_id == left_id)
        }));
        assert_eq!(dispatcher.pointer_capture_node(), None);
        assert_eq!(dispatcher.active_node(), None);
    }
}
