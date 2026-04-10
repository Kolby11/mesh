/// UI event types and dispatch.
use crate::tree::{NodeId, WidgetNode};

/// A UI event targeted at a specific node.
#[derive(Debug, Clone)]
pub enum UiEvent {
    PointerDown { node_id: NodeId, x: f32, y: f32 },
    PointerUp { node_id: NodeId, x: f32, y: f32 },
    PointerMove { node_id: NodeId, x: f32, y: f32 },
    PointerEnter { node_id: NodeId },
    PointerLeave { node_id: NodeId },
    KeyDown { node_id: NodeId, key: String, modifiers: Modifiers },
    KeyUp { node_id: NodeId, key: String, modifiers: Modifiers },
    Focus { node_id: NodeId },
    Blur { node_id: NodeId },
    Scroll { node_id: NodeId, dx: f32, dy: f32 },
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
    PointerMotion { x: f32, y: f32 },
    PointerButton { x: f32, y: f32, button: u32, pressed: bool },
    Key { keycode: u32, pressed: bool, modifiers: Modifiers },
    Scroll { x: f32, y: f32, dx: f32, dy: f32 },
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
                        vec![UiEvent::PointerDown { node_id, x: *x, y: *y }]
                    } else {
                        vec![UiEvent::PointerUp { node_id, x: *x, y: *y }]
                    }
                } else {
                    vec![]
                }
            }
            RawInputEvent::PointerMotion { x, y } => {
                if let Some(node_id) = Self::hit_test(root, *x, *y) {
                    vec![UiEvent::PointerMove { node_id, x: *x, y: *y }]
                } else {
                    vec![]
                }
            }
            RawInputEvent::Key { keycode, pressed, modifiers } => {
                // Keys go to the focused node. For now, target root.
                let node_id = root.id;
                let key = format!("{keycode}");
                if *pressed {
                    vec![UiEvent::KeyDown { node_id, key, modifiers: *modifiers }]
                } else {
                    vec![UiEvent::KeyUp { node_id, key, modifiers: *modifiers }]
                }
            }
            RawInputEvent::Scroll { x, y, dx, dy } => {
                if let Some(node_id) = Self::hit_test(root, *x, *y) {
                    vec![UiEvent::Scroll { node_id, dx: *dx, dy: *dy }]
                } else {
                    vec![]
                }
            }
        }
    }
}

fn hit_test_node(node: &WidgetNode, x: f32, y: f32) -> Option<NodeId> {
    if !node.layout.contains(x, y) {
        return None;
    }
    // Check children in reverse order (last painted = topmost).
    for child in node.children.iter().rev() {
        if let Some(id) = hit_test_node(child, x, y) {
            return Some(id);
        }
    }
    Some(node.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Dimension;
    use crate::layout::LayoutEngine;

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
