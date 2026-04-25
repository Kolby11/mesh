/// UI event types and dispatch.
use crate::tree::{ElementState, NodeId, WidgetNode};

/// A UI event targeted at a specific node.
#[derive(Debug, Clone)]
pub enum UiEvent {
    PointerDown {
        node_id: NodeId,
        x: f32,
        y: f32,
    },
    PointerUp {
        node_id: NodeId,
        x: f32,
        y: f32,
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
            Self::PointerUp { .. } => "click",
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
#[derive(Debug, Clone, Copy, Default)]
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

/// Tracks pointer and keyboard interaction state across frames.
///
/// Call `process` with each raw input event to update node state flags
/// (hover, active, focus) on the widget tree and produce the resulting UI events.
/// After `process` returns, any node whose `state` changed should have its
/// computed style re-resolved via `StyleResolver::restyle_subtree`.
#[derive(Debug, Default)]
pub struct InputState {
    hovered_node: Option<NodeId>,
    active_node: Option<NodeId>,
    focused_node: Option<NodeId>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a raw input event to the tree, updating node state flags and
    /// returning the resulting UI events.
    pub fn process(&mut self, root: &mut WidgetNode, raw: &RawInputEvent) -> Vec<UiEvent> {
        let mut events = Vec::new();

        match raw {
            RawInputEvent::PointerMotion { x, y } => {
                let new_hovered = EventDispatcher::hit_test(root, *x, *y);
                self.update_hover(root, new_hovered, &mut events);

                if let Some(node_id) = new_hovered {
                    events.push(UiEvent::PointerMove { node_id, x: *x, y: *y });
                }
            }

            RawInputEvent::PointerButton { x, y, pressed, .. } => {
                if *pressed {
                    // Ensure hover state is current (pointer may not have moved first).
                    let target = EventDispatcher::hit_test(root, *x, *y);
                    self.update_hover(root, target, &mut events);

                    // Transfer focus to the clicked node.
                    if self.focused_node != target {
                        if let Some(old_id) = self.focused_node {
                            set_state_flag(root, old_id, |s| s.focused = false);
                            events.push(UiEvent::Blur { node_id: old_id });
                        }
                        if let Some(new_id) = target {
                            set_state_flag(root, new_id, |s| s.focused = true);
                            events.push(UiEvent::Focus { node_id: new_id });
                        }
                        self.focused_node = target;
                    }

                    // Mark the pressed node as active.
                    if let Some(node_id) = target {
                        set_state_flag(root, node_id, |s| s.active = true);
                        self.active_node = Some(node_id);
                        events.push(UiEvent::PointerDown { node_id, x: *x, y: *y });
                    }
                } else {
                    // Release active state.
                    if let Some(node_id) = self.active_node.take() {
                        set_state_flag(root, node_id, |s| s.active = false);
                    }

                    if let Some(node_id) = EventDispatcher::hit_test(root, *x, *y) {
                        events.push(UiEvent::PointerUp { node_id, x: *x, y: *y });
                    }
                }
            }

            RawInputEvent::Key { keycode, pressed, modifiers } => {
                let node_id = self.focused_node.unwrap_or(root.id);
                let key = format!("{keycode}");
                if *pressed {
                    events.push(UiEvent::KeyDown { node_id, key, modifiers: *modifiers });
                } else {
                    events.push(UiEvent::KeyUp { node_id, key, modifiers: *modifiers });
                }
            }

            RawInputEvent::Scroll { x, y, dx, dy } => {
                if let Some(node_id) = EventDispatcher::hit_test(root, *x, *y) {
                    events.push(UiEvent::Scroll { node_id, dx: *dx, dy: *dy });
                }
            }
        }

        events
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

    /// Explicitly move keyboard focus to a node (e.g. for Tab navigation).
    pub fn set_focus(&mut self, root: &mut WidgetNode, target: Option<NodeId>) -> Vec<UiEvent> {
        let mut events = Vec::new();
        if self.focused_node == target {
            return events;
        }
        if let Some(old_id) = self.focused_node {
            set_state_flag(root, old_id, |s| s.focused = false);
            events.push(UiEvent::Blur { node_id: old_id });
        }
        if let Some(new_id) = target {
            set_state_flag(root, new_id, |s| s.focused = true);
            events.push(UiEvent::Focus { node_id: new_id });
        }
        self.focused_node = target;
        events
    }

    /// Reset all tracked state (e.g. when the surface loses pointer/keyboard focus).
    pub fn reset(&mut self, root: &mut WidgetNode) {
        if let Some(id) = self.hovered_node.take() {
            set_state_flag(root, id, |s| s.hovered = false);
        }
        if let Some(id) = self.active_node.take() {
            set_state_flag(root, id, |s| s.active = false);
        }
        if let Some(id) = self.focused_node.take() {
            set_state_flag(root, id, |s| s.focused = false);
        }
    }

    fn update_hover(
        &mut self,
        root: &mut WidgetNode,
        new_hovered: Option<NodeId>,
        events: &mut Vec<UiEvent>,
    ) {
        if new_hovered == self.hovered_node {
            return;
        }
        if let Some(old_id) = self.hovered_node {
            set_state_flag(root, old_id, |s| s.hovered = false);
            events.push(UiEvent::PointerLeave { node_id: old_id });
        }
        if let Some(new_id) = new_hovered {
            set_state_flag(root, new_id, |s| s.hovered = true);
            events.push(UiEvent::PointerEnter { node_id: new_id });
        }
        self.hovered_node = new_hovered;
    }
}

fn set_state_flag(root: &mut WidgetNode, id: NodeId, f: impl FnOnce(&mut ElementState)) {
    if let Some(node) = root.find_mut(id) {
        f(&mut node.state);
    }
}

/// Performs hit-testing and event routing on a widget tree.
pub struct EventDispatcher;

impl EventDispatcher {
    /// Find the deepest node at the given coordinates.
    pub fn hit_test(root: &WidgetNode, x: f32, y: f32) -> Option<NodeId> {
        hit_test_node(root, x, y)
    }

    /// Convert a raw input event into targeted UI events.
    pub fn dispatch(root: &WidgetNode, raw: &RawInputEvent) -> Vec<UiEvent> {
        match raw {
            RawInputEvent::PointerButton { x, y, pressed, .. } => {
                if let Some(node_id) = Self::hit_test(root, *x, *y) {
                    if *pressed {
                        vec![UiEvent::PointerDown {
                            node_id,
                            x: *x,
                            y: *y,
                        }]
                    } else {
                        vec![UiEvent::PointerUp {
                            node_id,
                            x: *x,
                            y: *y,
                        }]
                    }
                } else {
                    vec![]
                }
            }
            RawInputEvent::PointerMotion { x, y } => {
                if let Some(node_id) = Self::hit_test(root, *x, *y) {
                    vec![UiEvent::PointerMove {
                        node_id,
                        x: *x,
                        y: *y,
                    }]
                } else {
                    vec![]
                }
            }
            RawInputEvent::Key {
                keycode,
                pressed,
                modifiers,
            } => {
                // Keys go to the focused node. For now, target root.
                let node_id = root.id;
                let key = format!("{keycode}");
                if *pressed {
                    vec![UiEvent::KeyDown {
                        node_id,
                        key,
                        modifiers: *modifiers,
                    }]
                } else {
                    vec![UiEvent::KeyUp {
                        node_id,
                        key,
                        modifiers: *modifiers,
                    }]
                }
            }
            RawInputEvent::Scroll { x, y, dx, dy } => {
                if let Some(node_id) = Self::hit_test(root, *x, *y) {
                    vec![UiEvent::Scroll {
                        node_id,
                        dx: *dx,
                        dy: *dy,
                    }]
                } else {
                    vec![]
                }
            }
        }
    }
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
    (
        offset_x - node_attr_f32(node, "_mesh_scroll_x"),
        offset_y - node_attr_f32(node, "_mesh_scroll_y"),
    )
}

fn node_attr_f32(node: &WidgetNode, key: &str) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
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

    #[test]
    fn hit_test_finds_deepest_node() {
        let mut root = WidgetNode::new("root");
        root.computed_style.width = Dimension::Px(200.0);
        root.computed_style.height = Dimension::Px(100.0);

        let mut child = WidgetNode::new("button");
        child.computed_style.width = Dimension::Px(100.0);
        child.computed_style.height = Dimension::Px(50.0);
        let child_id = child.id;

        root.children = vec![child];
        LayoutEngine::compute(&mut root, 200.0, 100.0);

        // Inside the child.
        assert_eq!(EventDispatcher::hit_test(&root, 50.0, 25.0), Some(child_id));
        // Outside the child but inside root.
        assert_eq!(EventDispatcher::hit_test(&root, 150.0, 75.0), Some(root.id));
        // Outside everything.
        assert_eq!(EventDispatcher::hit_test(&root, 300.0, 300.0), None);
    }
}
