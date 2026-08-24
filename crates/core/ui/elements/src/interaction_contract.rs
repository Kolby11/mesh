//! Cross-consumer visibility, target eligibility, and node geometry.
//!
//! The widget tree is consumed by several independent crates. Keeping these
//! decisions beside [`WidgetNode`] prevents input, paint, and semantics from
//! quietly growing different interpretations of the same node.

use crate::layout::LayoutRect;
use crate::style::{Display, Transform2D, Visibility};
use crate::tree::WidgetNode;

/// A two-dimensional affine transform in surface coordinates.
///
/// Points are represented as column vectors: `x' = m11*x + m21*y + tx` and
/// `y' = m12*x + m22*y + ty`. Keeping the matrix here lets every consumer use
/// the same composition and inverse instead of approximating transforms with
/// independent offsets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AffineTransform {
    pub m11: f32,
    pub m12: f32,
    pub m21: f32,
    pub m22: f32,
    pub tx: f32,
    pub ty: f32,
}

impl AffineTransform {
    pub const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub const fn translation(tx: f32, ty: f32) -> Self {
        Self {
            tx,
            ty,
            ..Self::IDENTITY
        }
    }

    pub const fn scale(sx: f32, sy: f32) -> Self {
        Self {
            m11: sx,
            m22: sy,
            ..Self::IDENTITY
        }
    }

    pub fn rotation(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            m11: cos,
            m12: sin,
            m21: -sin,
            m22: cos,
            ..Self::IDENTITY
        }
    }

    /// Compose `local` after this transform.
    pub fn then(self, local: Self) -> Self {
        Self {
            m11: self.m11 * local.m11 + self.m21 * local.m12,
            m12: self.m12 * local.m11 + self.m22 * local.m12,
            m21: self.m11 * local.m21 + self.m21 * local.m22,
            m22: self.m12 * local.m21 + self.m22 * local.m22,
            tx: self.m11 * local.tx + self.m21 * local.ty + self.tx,
            ty: self.m12 * local.tx + self.m22 * local.ty + self.ty,
        }
    }

    pub fn transform_point(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.m11 * x + self.m21 * y + self.tx,
            self.m12 * x + self.m22 * y + self.ty,
        )
    }

    pub fn inverse(self) -> Option<Self> {
        let determinant = self.m11 * self.m22 - self.m21 * self.m12;
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return None;
        }
        let inverse = 1.0 / determinant;
        Some(Self {
            m11: self.m22 * inverse,
            m12: -self.m12 * inverse,
            m21: -self.m21 * inverse,
            m22: self.m11 * inverse,
            tx: (self.m21 * self.ty - self.m22 * self.tx) * inverse,
            ty: (self.m12 * self.tx - self.m11 * self.ty) * inverse,
        })
    }

    /// Transform an axis-aligned local rectangle and return its surface AABB.
    pub fn transform_rect(self, rect: LayoutRect) -> LayoutRect {
        let points = [
            self.transform_point(rect.x, rect.y),
            self.transform_point(rect.x + rect.width, rect.y),
            self.transform_point(rect.x, rect.y + rect.height),
            self.transform_point(rect.x + rect.width, rect.y + rect.height),
        ];
        let (mut min_x, mut max_x) = (points[0].0, points[0].0);
        let (mut min_y, mut max_y) = (points[0].1, points[0].1);
        for (x, y) in points.into_iter().skip(1) {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        LayoutRect {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        }
    }
}

impl Default for AffineTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// One transformed overflow clip. Bounds are used by paint culling while
/// point membership uses the inverse matrix, so rotated clips do not become
/// larger hit regions merely because their AABB is larger.
#[derive(Debug, Clone, Copy)]
pub struct AffineClip {
    pub transform: AffineTransform,
    pub rect: LayoutRect,
}

impl AffineClip {
    pub fn bounds(self) -> LayoutRect {
        self.transform.transform_rect(self.rect)
    }

    pub fn contains(self, x: f32, y: f32) -> bool {
        self.transform
            .inverse()
            .map(|inverse| {
                let (local_x, local_y) = inverse.transform_point(x, y);
                local_x >= self.rect.x
                    && local_x < self.rect.x + self.rect.width
                    && local_y >= self.rect.y
                    && local_y < self.rect.y + self.rect.height
            })
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AffineClipStack {
    clips: Vec<AffineClip>,
}

impl AffineClipStack {
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    pub fn push(&self, clip: AffineClip) -> Self {
        let mut next = self.clone();
        next.clips.push(clip);
        next
    }

    /// Return the ordered transformed clips accumulated from the surface root.
    /// Consumers that rasterize or serialize the shared geometry can use the
    /// same clip stack without reconstructing it from axis-aligned bounds.
    pub fn as_slice(&self) -> &[AffineClip] {
        &self.clips
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.clips.iter().all(|clip| clip.contains(x, y))
    }

    pub fn bounds(&self) -> Option<LayoutRect> {
        let mut bounds = None;
        for clip in &self.clips {
            bounds = intersect_layout_rect(bounds, clip.bounds());
            if bounds.is_none() {
                return None;
            }
        }
        bounds
    }
}

fn intersect_layout_rect(current: Option<LayoutRect>, next: LayoutRect) -> Option<LayoutRect> {
    let Some(current) = current else {
        return Some(next);
    };
    let left = current.x.max(next.x);
    let top = current.y.max(next.y);
    let right = (current.x + current.width).min(next.x + next.width);
    let bottom = (current.y + current.height).min(next.y + next.height);
    (right > left && bottom > top).then_some(LayoutRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

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

/// Return whether a node can receive a target while retaining all ancestor
/// eligibility.  Target lookups that already walk through the tree naturally
/// inherit this policy; captured and key-addressed targets need this explicit
/// check because they can outlive a state change on the node or one of its
/// ancestors.
pub fn node_can_receive_target(
    root: &WidgetNode,
    target: crate::tree::NodeId,
    interaction: InteractionTarget,
) -> bool {
    fn visit(
        node: &WidgetNode,
        target: crate::tree::NodeId,
        interaction: InteractionTarget,
        parent: NodeEligibility,
    ) -> bool {
        let policy = parent.child(node);
        if node.id == target {
            return policy.allows(interaction);
        }
        if !policy.allows(interaction) {
            return false;
        }
        node.children
            .iter()
            .any(|child| visit(child, target, interaction, policy))
    }

    visit(root, target, interaction, NodeEligibility::ROOT)
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

/// Build the transform from a node's local border-box coordinates into its
/// parent's coordinate space. Translation is applied in the parent space;
/// scale and rotation use the resolved CSS transform origin in local space.
pub fn local_transform(node: &WidgetNode) -> AffineTransform {
    AffineTransform::translation(node.layout.x, node.layout.y).then(node_transform_effect(node))
}

fn node_transform_effect(node: &WidgetNode) -> AffineTransform {
    let transform = node.computed_style.transform;
    let origin = node.computed_style.transform_origin;
    let origin_x = origin.x.resolve(node.layout.width);
    let origin_y = origin.y.resolve(node.layout.height);
    let geometric = AffineTransform::translation(origin_x, origin_y)
        .then(AffineTransform::rotation(transform.rotation))
        .then(AffineTransform::scale(
            transform.scale_x.max(0.0),
            transform.scale_y.max(0.0),
        ))
        .then(AffineTransform::translation(-origin_x, -origin_y));
    AffineTransform::translation(transform.translate_x, transform.translate_y).then(geometric)
}

/// Compose a node's local transform into a cumulative surface transform.
pub fn node_transform(parent: AffineTransform, node: &WidgetNode) -> AffineTransform {
    parent.then(local_transform(node))
}

/// Transform used for a tree root before visiting its local layout box.
pub const fn root_transform(offset_x: f32, offset_y: f32) -> AffineTransform {
    AffineTransform::translation(offset_x, offset_y)
}

/// Transform child layout coordinates through a scroll container's content
/// space. The scroll offset is expressed in the container's local space.
pub fn child_transform(
    node_world: AffineTransform,
    node: &WidgetNode,
    scroll_x: f32,
    scroll_y: f32,
) -> AffineTransform {
    let parent = local_transform(node)
        .inverse()
        .map(|inverse| node_world.then(inverse))
        .unwrap_or_default();
    parent
        .then(AffineTransform::translation(node.layout.x, node.layout.y))
        .then(node_transform_effect(node))
        .then(AffineTransform::translation(-node.layout.x, -node.layout.y))
        .then(AffineTransform::translation(-scroll_x, -scroll_y))
}

/// The transformed border-box AABB for a node in surface coordinates.
pub fn node_layout_bounds(node: &WidgetNode, transform: AffineTransform) -> LayoutRect {
    transform.transform_rect(LayoutRect {
        x: 0.0,
        y: 0.0,
        width: node.layout.width.max(0.0),
        height: node.layout.height.max(0.0),
    })
}

/// A clip matching a node's overflow content box in surface coordinates.
pub fn node_clip(node: &WidgetNode, transform: AffineTransform) -> AffineClip {
    AffineClip {
        transform,
        rect: LayoutRect {
            x: 0.0,
            y: 0.0,
            width: node.layout.width.max(0.0),
            height: node.layout.height.max(0.0),
        },
    }
}

/// Return the node's transformed border-box AABB in surface coordinates.
pub fn transformed_layout_at(node: &WidgetNode, offset_x: f32, offset_y: f32) -> LayoutRect {
    node_layout_bounds(
        node,
        node_transform(root_transform(offset_x, offset_y), node),
    )
}

pub fn transformed_layout_for(
    layout: LayoutRect,
    transform: Transform2D,
    offset_x: f32,
    offset_y: f32,
) -> LayoutRect {
    let style_transform = transform;
    let origin_x = layout.width * 0.5;
    let origin_y = layout.height * 0.5;
    let geometric = AffineTransform::translation(origin_x, origin_y)
        .then(AffineTransform::rotation(style_transform.rotation))
        .then(AffineTransform::scale(
            style_transform.scale_x.max(0.0),
            style_transform.scale_y.max(0.0),
        ))
        .then(AffineTransform::translation(-origin_x, -origin_y));
    let transform = AffineTransform::translation(offset_x + layout.x, offset_y + layout.y)
        .then(AffineTransform::translation(
            style_transform.translate_x,
            style_transform.translate_y,
        ))
        .then(geometric);
    transform.transform_rect(LayoutRect {
        x: 0.0,
        y: 0.0,
        width: layout.width.max(0.0),
        height: layout.height.max(0.0),
    })
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
        let rect = transformed_layout_at(&node, 7.0, 11.0);
        assert!((rect.x - 12.0).abs() < f32::EPSILON);
        assert!((rect.y - 21.0).abs() < f32::EPSILON);
        assert!((rect.width - 40.0).abs() < f32::EPSILON);
        assert!((rect.height - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn affine_transform_uses_origin_and_inverse_for_rotated_geometry() {
        let mut node = WidgetNode::new("box");
        node.layout = LayoutRect {
            x: 10.0,
            y: 20.0,
            width: 20.0,
            height: 10.0,
        };
        node.computed_style.transform.rotation = std::f32::consts::FRAC_PI_2;
        let world = node_transform(root_transform(0.0, 0.0), &node);
        let bounds = node_layout_bounds(&node, world);
        assert!((bounds.x - 15.0).abs() < 0.001);
        assert!((bounds.y - 15.0).abs() < 0.001);
        assert!((bounds.width - 10.0).abs() < 0.001);
        assert!((bounds.height - 20.0).abs() < 0.001);

        let (center_x, center_y) = world.transform_point(10.0, 5.0);
        let (local_x, local_y) = world.inverse().unwrap().transform_point(center_x, center_y);
        assert!((local_x - 10.0).abs() < 0.001);
        assert!((local_y - 5.0).abs() < 0.001);
    }

    #[test]
    fn affine_clip_contains_rotated_shape_without_using_its_aabb() {
        let transform = AffineTransform::translation(50.0, 50.0)
            .then(AffineTransform::rotation(std::f32::consts::FRAC_PI_4));
        let clip = AffineClip {
            transform,
            rect: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
        };
        assert!(clip.contains(50.0, 50.0));
        assert!(!clip.contains(50.0 - 14.0, 50.0 + 14.0));
        assert!(clip.bounds().width > 20.0);
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

    #[test]
    fn keyed_target_eligibility_rechecks_disabled_and_inert_ancestors() {
        let mut root = WidgetNode::new("box");
        root.layout.width = 100.0;
        root.layout.height = 40.0;
        let mut parent = WidgetNode::new("box");
        parent.layout.width = 80.0;
        parent.layout.height = 30.0;
        let mut child = WidgetNode::new("button");
        child.layout.width = 40.0;
        child.layout.height = 20.0;
        let child_id = child.id;
        parent.children.push(child);
        root.children.push(parent);

        assert!(node_can_receive_target(
            &root,
            child_id,
            InteractionTarget::Pointer
        ));

        attr(&mut root.children[0], "inert", "true");
        assert!(!node_can_receive_target(
            &root,
            child_id,
            InteractionTarget::Pointer
        ));

        attr(&mut root.children[0], "inert", "false");
        attr(&mut root.children[0].children[0], "disabled", "true");
        assert!(!node_can_receive_target(
            &root,
            child_id,
            InteractionTarget::Pointer
        ));
    }
}
