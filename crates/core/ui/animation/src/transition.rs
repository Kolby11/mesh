//! Transition controller and shared transition-safe style snapshots.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mesh_core_elements::{Corners, Dimension, Edges, TransitionStyle, WidgetNode, style::Color};

use super::easing::{Easing, apply_easing};
use super::interpolate::Interpolate;

/// Bundle of every property that can be transitioned or keyframed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimatableStyle {
    pub border_radius: Corners,
    pub border_width: Edges,
    pub opacity: f32,
    pub background_color: Color,
    pub border_color: Color,
    pub color: Color,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    pub padding: Edges,
    pub margin: Edges,
    pub transform: mesh_core_elements::Transform2D,
    pub font_size: f32,
    pub letter_spacing: f32,
    pub line_height: f32,
    pub gap: f32,
    pub inset_top: Option<f32>,
    pub inset_right: Option<f32>,
    pub inset_bottom: Option<f32>,
    pub inset_left: Option<f32>,
}

impl AnimatableStyle {
    pub fn from_node(node: &WidgetNode) -> Self {
        let s = &node.computed_style;
        Self {
            border_radius: s.border_radius,
            border_width: s.border_width,
            opacity: s.opacity,
            background_color: s.background_color,
            border_color: s.border_color,
            color: s.color,
            width: s.width,
            height: s.height,
            min_width: s.min_width,
            max_width: s.max_width,
            min_height: s.min_height,
            max_height: s.max_height,
            padding: s.padding,
            margin: s.margin,
            transform: s.transform,
            font_size: s.font_size,
            letter_spacing: s.letter_spacing,
            line_height: s.line_height,
            gap: s.gap,
            inset_top: s.inset_top,
            inset_right: s.inset_right,
            inset_bottom: s.inset_bottom,
            inset_left: s.inset_left,
        }
    }

    pub fn apply_to_node(self, node: &mut WidgetNode) {
        let s = &mut node.computed_style;
        s.border_radius = self.border_radius;
        s.border_width = self.border_width;
        s.opacity = self.opacity;
        s.background_color = self.background_color;
        s.border_color = self.border_color;
        s.color = self.color;
        s.width = self.width;
        s.height = self.height;
        s.min_width = self.min_width;
        s.max_width = self.max_width;
        s.min_height = self.min_height;
        s.max_height = self.max_height;
        s.padding = self.padding;
        s.margin = self.margin;
        s.transform = self.transform;
        s.font_size = self.font_size;
        s.letter_spacing = self.letter_spacing;
        s.line_height = self.line_height;
        s.gap = self.gap;
        s.inset_top = self.inset_top;
        s.inset_right = self.inset_right;
        s.inset_bottom = self.inset_bottom;
        s.inset_left = self.inset_left;
    }
}

impl Interpolate for AnimatableStyle {
    fn lerp(self, other: Self, progress: f32) -> Self {
        Self {
            border_radius: self.border_radius.lerp(other.border_radius, progress),
            border_width: self.border_width.lerp(other.border_width, progress),
            opacity: self.opacity.lerp(other.opacity, progress),
            background_color: self.background_color.lerp(other.background_color, progress),
            border_color: self.border_color.lerp(other.border_color, progress),
            color: self.color.lerp(other.color, progress),
            width: lerp_dimension(self.width, other.width, progress),
            height: lerp_dimension(self.height, other.height, progress),
            min_width: lerp_option_f32(self.min_width, other.min_width, progress),
            max_width: lerp_option_f32(self.max_width, other.max_width, progress),
            min_height: lerp_option_f32(self.min_height, other.min_height, progress),
            max_height: lerp_option_f32(self.max_height, other.max_height, progress),
            padding: self.padding.lerp(other.padding, progress),
            margin: self.margin.lerp(other.margin, progress),
            transform: self.transform.lerp(other.transform, progress),
            font_size: self.font_size.lerp(other.font_size, progress),
            letter_spacing: self.letter_spacing.lerp(other.letter_spacing, progress),
            line_height: self.line_height.lerp(other.line_height, progress),
            gap: self.gap.lerp(other.gap, progress),
            inset_top: lerp_option_f32(self.inset_top, other.inset_top, progress),
            inset_right: lerp_option_f32(self.inset_right, other.inset_right, progress),
            inset_bottom: lerp_option_f32(self.inset_bottom, other.inset_bottom, progress),
            inset_left: lerp_option_f32(self.inset_left, other.inset_left, progress),
        }
    }
}

/// One in-flight transition for a single widget node.
#[derive(Debug, Clone)]
pub struct ActiveTransition {
    pub from: AnimatableStyle,
    pub to: AnimatableStyle,
    pub started_at: Instant,
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
    pub source: TransitionStyle,
}

impl ActiveTransition {
    pub fn current(&self, now: Instant) -> AnimatableStyle {
        if self.duration.is_zero() {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed < self.delay {
            return self.from;
        }
        let active = elapsed - self.delay;
        let raw = (active.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.from.lerp(self.to, apply_easing(self.easing, raw))
    }

    pub fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started_at) >= self.delay + self.duration
    }
}

/// Per-component transition state.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct TransitionAnimator {
    active: HashMap<String, ActiveTransition>,
    has_active: bool,
}

impl TransitionAnimator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, _tree: &mut WidgetNode, _now: Instant) {
        unimplemented!("TransitionAnimator::apply")
    }

    pub fn has_active(&self) -> bool {
        self.has_active
    }
}

fn lerp_dimension(from: Dimension, to: Dimension, progress: f32) -> Dimension {
    match (from, to) {
        (Dimension::Px(a), Dimension::Px(b)) => Dimension::Px(a.lerp(b, progress)),
        (Dimension::Percent(a), Dimension::Percent(b)) => Dimension::Percent(a.lerp(b, progress)),
        _ => to,
    }
}

fn lerp_option_f32(from: Option<f32>, to: Option<f32>, progress: f32) -> Option<f32> {
    match (from, to) {
        (Some(a), Some(b)) => Some(a.lerp(b, progress)),
        _ => to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{ComputedStyle, Transform2D};

    fn node_with_style(style: ComputedStyle) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.computed_style = style;
        node
    }

    #[test]
    fn animatable_style_round_trips_node_fields() {
        let style = ComputedStyle {
            opacity: 0.5,
            font_size: 18.0,
            gap: 12.0,
            ..ComputedStyle::default()
        };
        let node = node_with_style(style.clone());
        let snapshot = AnimatableStyle::from_node(&node);

        let mut target = WidgetNode::new("box");
        snapshot.apply_to_node(&mut target);
        assert_eq!(target.computed_style.opacity, style.opacity);
        assert_eq!(target.computed_style.font_size, style.font_size);
        assert_eq!(target.computed_style.gap, style.gap);
    }

    #[test]
    fn animatable_style_interpolates_transition_safe_fields() {
        let from = AnimatableStyle {
            border_radius: Corners::zero(),
            border_width: Edges::zero(),
            opacity: 0.0,
            background_color: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            color: Color::TRANSPARENT,
            width: Dimension::Px(10.0),
            height: Dimension::Px(10.0),
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            padding: Edges::zero(),
            margin: Edges::zero(),
            transform: Transform2D::IDENTITY,
            font_size: 10.0,
            letter_spacing: 0.0,
            line_height: 1.0,
            gap: 0.0,
            inset_top: Some(0.0),
            inset_right: None,
            inset_bottom: None,
            inset_left: None,
        };
        let to = AnimatableStyle {
            opacity: 1.0,
            background_color: Color::WHITE,
            color: Color::WHITE,
            padding: Edges::all(20.0),
            transform: Transform2D {
                translate_x: 40.0,
                ..Transform2D::IDENTITY
            },
            font_size: 20.0,
            gap: 16.0,
            inset_top: Some(20.0),
            ..from
        };

        let mid = from.lerp(to, 0.5);
        assert!((mid.opacity - 0.5).abs() < 0.001);
        assert_eq!(mid.background_color.r, 128);
        assert_eq!(mid.padding.top, 10.0);
        assert_eq!(mid.transform.translate_x, 20.0);
        assert_eq!(mid.font_size, 15.0);
        assert_eq!(mid.gap, 8.0);
        assert_eq!(mid.inset_top, Some(10.0));
    }
}
