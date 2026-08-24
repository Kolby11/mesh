//! Cross-consumer visibility, target eligibility, and node geometry.
//!
//! The widget tree is consumed by several independent crates. Keeping these
//! decisions beside [`WidgetNode`] prevents input, paint, and semantics from
//! quietly growing different interpretations of the same node.

use crate::layout::LayoutRect;
use crate::style::{Display, Transform2D, Visibility};
use crate::tree::WidgetNode;

/// A consumer-specific target policy derived from one node state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionTarget {
    /// A node that contributes visual output.
    Paint,
    Pointer,
    Focus,
    Gesture,
    Scroll,
    Tooltip,
    /// A node exposed to accessibility and automation consumers. Disabled
    /// controls remain semantically exposed so their disabled state is useful
    /// to assistive technology; inert and aria-hidden content does not.
    Semantics,
}

/// The shared eligibility snapshot for one node during a tree walk.
///
/// `child` carries ancestor state into descendants. Geometry is propagated for
/// interactive targets because a zero-sized ancestor cannot provide a painted
/// hit region, while semantic exposure intentionally does not depend on box
/// size so structural wrappers remain available to accessibility consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeEligibility {
    visual_visible: bool,
    semantic_visible: bool,
    has_geometry: bool,
    disabled: bool,
    inert: bool,
}

impl NodeEligibility {
    /// Eligibility before visiting a root node.
    pub const ROOT: Self = Self {
        visual_visible: true,
        semantic_visible: true,
        has_geometry: true,
        disabled: false,
        inert: false,
    };

    /// Resolve the local state of a node without ancestor state.
    pub fn local(node: &WidgetNode) -> Self {
        Self {
            visual_visible: locally_visual_visible(node),
            semantic_visible: locally_visual_visible(node)
                && !locally_aria_hidden(node)
                && !locally_inert(node),
            has_geometry: node_has_geometry(node),
            disabled: locally_disabled(node),
            inert: locally_inert(node),
        }
    }

    /// Resolve this node after applying the state inherited from its parent.
    pub fn for_node(node: &WidgetNode, parent: Self) -> Self {
        let local = Self::local(node);
        Self {
            visual_visible: parent.visual_visible && local.visual_visible,
            semantic_visible: parent.semantic_visible && local.semantic_visible,
            has_geometry: parent.has_geometry && local.has_geometry,
            disabled: parent.disabled || local.disabled,
            inert: parent.inert || local.inert,
        }
    }

    /// Resolve the root node.
    pub fn for_root(node: &WidgetNode) -> Self {
        Self::for_node(node, Self::ROOT)
    }

    pub const fn is_visible(self) -> bool {
        self.visual_visible
    }

    pub const fn is_semantically_visible(self) -> bool {
        self.semantic_visible
    }

    pub const fn has_geometry(self) -> bool {
        self.has_geometry
    }

    pub const fn is_disabled(self) -> bool {
        self.disabled
    }

    pub const fn is_inert(self) -> bool {
        self.inert
    }

    pub const fn allows(self, target: InteractionTarget) -> bool {
        match target {
            InteractionTarget::Paint => self.visual_visible && self.has_geometry,
            InteractionTarget::Semantics => self.semantic_visible,
            InteractionTarget::Pointer
            | InteractionTarget::Focus
            | InteractionTarget::Gesture
            | InteractionTarget::Scroll => {
                self.visual_visible && self.has_geometry && !self.disabled && !self.inert
            }
            // Key-based tooltip metadata can be queried before layout has
            // produced a box. Pointer-derived tooltip targets are already
            // reached through the geometry-gated pointer walk.
            InteractionTarget::Tooltip => self.visual_visible && !self.disabled && !self.inert,
        }
    }

    /// Whether this node can be used as a geometry-backed interactive target.
    /// Tooltip metadata walks use their own policy because they may run before
    /// layout has produced a box.
    pub const fn can_target(self) -> bool {
        self.allows(InteractionTarget::Pointer)
    }

    /// Carry ancestor state into a child without requiring callers to inspect
    /// the internal fields of the snapshot.
    pub fn child(self, node: &WidgetNode) -> Self {
        Self::for_node(node, self)
    }
}

/// Resolve the shared policy for a root node.
pub fn node_eligibility(node: &WidgetNode) -> NodeEligibility {
    NodeEligibility::for_root(node)
}

/// Resolve the shared policy for a node below a known parent policy.
pub fn child_eligibility(parent: NodeEligibility, node: &WidgetNode) -> NodeEligibility {
    parent.child(node)
}

/// Apply the transform used by the retained painter to an incoming screen
/// offset. The offset already includes ancestor transforms; this adds only
/// this node's translation so descendants and interaction walks agree.
pub fn transformed_offset(node: &WidgetNode, offset_x: f32, offset_y: f32) -> (f32, f32) {
    let transform = node.computed_style.transform;
    (
        offset_x + transform.translate_x,
        offset_y + transform.translate_y,
    )
}

/// Return the node's painted, transformed layout box in surface coordinates.
///
/// MESH's current retained paint path represents scale as an axis-aligned box
/// around the layout center. Rotation and clipping remain separate renderer
/// contracts; keeping this operation shared still ensures the currently
/// supported translation/scale behavior is identical for paint and queries.
pub fn transformed_layout_at(node: &WidgetNode, offset_x: f32, offset_y: f32) -> LayoutRect {
    let transform = node.computed_style.transform;
    transformed_layout_for(node.layout, transform, offset_x, offset_y)
}

pub fn transformed_layout_for(
    layout: LayoutRect,
    transform: Transform2D,
    offset_x: f32,
    offset_y: f32,
) -> LayoutRect {
    let scale_x = transform.scale_x.max(0.0);
    let scale_y = transform.scale_y.max(0.0);
    let base_x = layout.x + offset_x;
    let base_y = layout.y + offset_y;
    let width = layout.width * scale_x;
    let height = layout.height * scale_y;
    LayoutRect {
        x: base_x - (width - layout.width) * 0.5,
        y: base_y - (height - layout.height) * 0.5,
        width,
        height,
    }
}

fn locally_visual_visible(node: &WidgetNode) -> bool {
    node.computed_style.display != Display::None
        && matches!(node.computed_style.visibility, Visibility::Visible)
        && !boolean_attribute(node, "hidden")
}

fn locally_aria_hidden(node: &WidgetNode) -> bool {
    boolean_attribute(node, "aria-hidden")
}

fn locally_disabled(node: &WidgetNode) -> bool {
    node.state.disabled
        || boolean_attribute(node, "disabled")
        || boolean_attribute(node, "aria-disabled")
}

fn locally_inert(node: &WidgetNode) -> bool {
    boolean_attribute(node, "inert")
}

fn node_has_geometry(node: &WidgetNode) -> bool {
    node.layout.width.is_finite()
        && node.layout.height.is_finite()
        && node.layout.width > 0.0
        && node.layout.height > 0.0
}

fn boolean_attribute(node: &WidgetNode, name: &str) -> bool {
    node.attributes
        .get_value(name)
        .is_some_and(|value| value.legacy_bool())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Display, Visibility};

    fn attr(node: &mut WidgetNode, name: &str, value: &str) {
        node.attributes.insert(name.into(), value.into());
    }

    #[test]
    fn eligibility_propagates_visibility_disabled_and_inert_state() {
        let mut root = WidgetNode::new("box");
        root.layout.width = 100.0;
        root.layout.height = 100.0;
        let mut parent = WidgetNode::new("box");
        parent.layout.width = 50.0;
        parent.layout.height = 50.0;
        attr(&mut parent, "inert", "true");
        let mut child = WidgetNode::new("button");
        child.layout.width = 10.0;
        child.layout.height = 10.0;
        child.state.disabled = true;
        parent.children.push(child);
        root.children.push(parent);

        let root_policy = node_eligibility(&root);
        let parent_policy = root_policy.child(&root.children[0]);
        let child_policy = parent_policy.child(&root.children[0].children[0]);
        assert!(root_policy.allows(InteractionTarget::Paint));
        assert!(!child_policy.can_target());
        assert!(child_policy.is_inert());
        assert!(child_policy.is_disabled());
        assert!(!child_policy.allows(InteractionTarget::Semantics));
    }

    #[test]
    fn aria_hidden_is_semantic_only_and_disabled_stays_exposed() {
        let mut root = WidgetNode::new("button");
        root.layout.width = 10.0;
        root.layout.height = 10.0;
        attr(&mut root, "aria-hidden", "true");
        let hidden = node_eligibility(&root);
        assert!(hidden.can_target());
        assert!(!hidden.allows(InteractionTarget::Semantics));

        attr(&mut root, "aria-hidden", "false");
        attr(&mut root, "aria-disabled", "true");
        let disabled = node_eligibility(&root);
        assert!(!disabled.can_target());
        assert!(disabled.allows(InteractionTarget::Semantics));
    }

    #[test]
    fn transformed_layout_is_the_painter_geometry() {
        let mut node = WidgetNode::new("box");
        node.layout = LayoutRect {
            x: 10.0,
            y: 20.0,
            width: 20.0,
            height: 10.0,
        };
        node.computed_style.transform.translate_x = 5.0;
        node.computed_style.transform.scale_x = 2.0;
        node.computed_style.transform.scale_y = 3.0;
        let translated = transformed_offset(&node, 7.0, 11.0);
        assert_eq!(translated, (12.0, 11.0));
        let rect = transformed_layout_at(&node, translated.0, translated.1);
        assert!((rect.x - 12.0).abs() < f32::EPSILON);
        assert!((rect.y - 21.0).abs() < f32::EPSILON);
        assert!((rect.width - 40.0).abs() < f32::EPSILON);
        assert!((rect.height - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn hidden_style_and_zero_geometry_do_not_paint_or_target() {
        let mut node = WidgetNode::new("button");
        node.layout.width = 10.0;
        node.layout.height = 10.0;
        node.computed_style.visibility = Visibility::Hidden;
        assert!(!node_eligibility(&node).can_target());
        assert!(!node_eligibility(&node).allows(InteractionTarget::Paint));

        node.computed_style.visibility = Visibility::Visible;
        node.computed_style.display = Display::Flex;
        node.layout.width = 0.0;
        assert!(!node_eligibility(&node).can_target());
        assert!(!node_eligibility(&node).allows(InteractionTarget::Paint));
    }
}
