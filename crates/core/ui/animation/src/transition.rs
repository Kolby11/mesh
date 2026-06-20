//! Transition controller and shared transition-safe style snapshots.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mesh_core_elements::{
    BoxShadow, Corners, Dimension, Edges, TransitionProperties, TransitionStyle, VisualFilter,
    WidgetNode, style::{Color, Visibility},
};

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
    pub box_shadow: BoxShadow,
    pub filter: VisualFilter,
    pub backdrop_filter: VisualFilter,
    pub font_size: f32,
    pub letter_spacing: f32,
    pub line_height: f32,
    pub gap: f32,
    pub inset_top: Option<f32>,
    pub inset_right: Option<f32>,
    pub inset_bottom: Option<f32>,
    pub inset_left: Option<f32>,
    pub visibility: Visibility,
}

impl AnimatableStyle {
    pub fn from_node(node: &WidgetNode) -> Self {
        let s = &node.computed_style;
        Self {
            // Clamp to the radius the painter can actually draw for this box so
            // transitions and keyframes interpolate toward the visible value
            // rather than an over-large authored radius.
            border_radius: visual_border_radius(s.border_radius, node.layout.width, node.layout.height),
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
            box_shadow: s.box_shadow,
            filter: s.filter,
            backdrop_filter: s.backdrop_filter,
            font_size: s.font_size,
            letter_spacing: s.letter_spacing,
            line_height: s.line_height,
            gap: s.gap,
            inset_top: s.inset_top,
            inset_right: s.inset_right,
            inset_bottom: s.inset_bottom,
            inset_left: s.inset_left,
            visibility: s.visibility,
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
        s.box_shadow = self.box_shadow;
        s.filter = self.filter;
        s.backdrop_filter = self.backdrop_filter;
        s.font_size = self.font_size;
        s.letter_spacing = self.letter_spacing;
        s.line_height = self.line_height;
        s.gap = self.gap;
        s.inset_top = self.inset_top;
        s.inset_right = self.inset_right;
        s.inset_bottom = self.inset_bottom;
        s.inset_left = self.inset_left;
        s.visibility = self.visibility;
    }

    /// Build the start-of-animation snapshot: take the previous displayed value
    /// for any property the transition opts into, and the desired (new) value
    /// for everything else. Only the opted-in properties differ between `from`
    /// and `to`, so the animator only ever interpolates those.
    pub fn selective_from(previous: Self, desired: Self, props: TransitionProperties) -> Self {
        Self {
            border_radius: pick(
                props.animates_border_radius(),
                previous.border_radius,
                desired.border_radius,
            ),
            border_width: pick(
                props.animates_border_width(),
                previous.border_width,
                desired.border_width,
            ),
            opacity: pick(props.animates_opacity(), previous.opacity, desired.opacity),
            background_color: pick(
                props.animates_background_color(),
                previous.background_color,
                desired.background_color,
            ),
            border_color: pick(
                props.animates_border_color(),
                previous.border_color,
                desired.border_color,
            ),
            color: pick(props.animates_color(), previous.color, desired.color),
            width: pick(props.animates_width(), previous.width, desired.width),
            height: pick(props.animates_height(), previous.height, desired.height),
            min_width: pick(
                props.animates_min_width(),
                previous.min_width,
                desired.min_width,
            ),
            max_width: pick(
                props.animates_max_width(),
                previous.max_width,
                desired.max_width,
            ),
            min_height: pick(
                props.animates_min_height(),
                previous.min_height,
                desired.min_height,
            ),
            max_height: pick(
                props.animates_max_height(),
                previous.max_height,
                desired.max_height,
            ),
            padding: pick(props.animates_padding(), previous.padding, desired.padding),
            margin: pick(props.animates_margin(), previous.margin, desired.margin),
            transform: pick(
                props.animates_transform(),
                previous.transform,
                desired.transform,
            ),
            box_shadow: pick(
                props.animates_box_shadow(),
                previous.box_shadow,
                desired.box_shadow,
            ),
            filter: pick(props.animates_filter(), previous.filter, desired.filter),
            backdrop_filter: pick(
                props.animates_backdrop_filter(),
                previous.backdrop_filter,
                desired.backdrop_filter,
            ),
            font_size: pick(
                props.animates_font_size(),
                previous.font_size,
                desired.font_size,
            ),
            letter_spacing: pick(
                props.animates_letter_spacing(),
                previous.letter_spacing,
                desired.letter_spacing,
            ),
            line_height: pick(
                props.animates_line_height(),
                previous.line_height,
                desired.line_height,
            ),
            gap: pick(props.animates_gap(), previous.gap, desired.gap),
            inset_top: pick(
                props.animates_inset_top(),
                previous.inset_top,
                desired.inset_top,
            ),
            inset_right: pick(
                props.animates_inset_right(),
                previous.inset_right,
                desired.inset_right,
            ),
            inset_bottom: pick(
                props.animates_inset_bottom(),
                previous.inset_bottom,
                desired.inset_bottom,
            ),
            inset_left: pick(
                props.animates_inset_left(),
                previous.inset_left,
                desired.inset_left,
            ),
            // visibility is always taken from desired — CSS discrete interpolation handles it in lerp
            visibility: desired.visibility,
        }
    }

    /// True if any property the transition opts into differs between `self` and
    /// `other`. Used to decide whether a transition needs to (re)start.
    pub fn differs(&self, other: &Self, props: TransitionProperties) -> bool {
        (props.animates_border_radius() && self.border_radius != other.border_radius)
            || (props.animates_border_width() && self.border_width != other.border_width)
            || (props.animates_opacity() && self.opacity != other.opacity)
            || (props.animates_background_color()
                && self.background_color != other.background_color)
            || (props.animates_border_color() && self.border_color != other.border_color)
            || (props.animates_color() && self.color != other.color)
            || (props.animates_width() && self.width != other.width)
            || (props.animates_height() && self.height != other.height)
            || (props.animates_min_width() && self.min_width != other.min_width)
            || (props.animates_max_width() && self.max_width != other.max_width)
            || (props.animates_min_height() && self.min_height != other.min_height)
            || (props.animates_max_height() && self.max_height != other.max_height)
            || (props.animates_padding() && self.padding != other.padding)
            || (props.animates_margin() && self.margin != other.margin)
            || (props.animates_transform() && self.transform != other.transform)
            || (props.animates_box_shadow() && self.box_shadow != other.box_shadow)
            || (props.animates_filter() && self.filter != other.filter)
            || (props.animates_backdrop_filter() && self.backdrop_filter != other.backdrop_filter)
            || (props.animates_font_size() && self.font_size != other.font_size)
            || (props.animates_letter_spacing() && self.letter_spacing != other.letter_spacing)
            || (props.animates_line_height() && self.line_height != other.line_height)
            || (props.animates_gap() && self.gap != other.gap)
            || (props.animates_inset_top() && self.inset_top != other.inset_top)
            || (props.animates_inset_right() && self.inset_right != other.inset_right)
            || (props.animates_inset_bottom() && self.inset_bottom != other.inset_bottom)
            || (props.animates_inset_left() && self.inset_left != other.inset_left)
    }
}

/// Clamp each corner radius to half the shorter box side — the largest radius
/// the painter can actually render for a box of this size.
fn visual_border_radius(radius: Corners, width: f32, height: f32) -> Corners {
    let cap = (width.min(height) * 0.5).max(0.0);
    if cap <= 0.0 {
        return radius;
    }
    Corners {
        top_left: radius.top_left.min(cap),
        top_right: radius.top_right.min(cap),
        bottom_right: radius.bottom_right.min(cap),
        bottom_left: radius.bottom_left.min(cap),
    }
}

fn pick<T>(use_previous: bool, previous: T, desired: T) -> T {
    if use_previous { previous } else { desired }
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
            box_shadow: lerp_box_shadow(self.box_shadow, other.box_shadow, progress),
            filter: lerp_visual_filter(self.filter, other.filter, progress),
            backdrop_filter: lerp_visual_filter(
                self.backdrop_filter,
                other.backdrop_filter,
                progress,
            ),
            font_size: self.font_size.lerp(other.font_size, progress),
            letter_spacing: self.letter_spacing.lerp(other.letter_spacing, progress),
            line_height: self.line_height.lerp(other.line_height, progress),
            gap: self.gap.lerp(other.gap, progress),
            inset_top: lerp_option_f32(self.inset_top, other.inset_top, progress),
            inset_right: lerp_option_f32(self.inset_right, other.inset_right, progress),
            inset_bottom: lerp_option_f32(self.inset_bottom, other.inset_bottom, progress),
            inset_left: lerp_option_f32(self.inset_left, other.inset_left, progress),
            // CSS: visibility is discrete — if either endpoint is Visible, stay Visible
            visibility: if self.visibility == Visibility::Visible
                || other.visibility == Visibility::Visible
            {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
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

/// Per-component transition controller.
///
/// Owns the in-flight transitions keyed by retained widget identity
/// (`_mesh_key`). Callers that drive transitions alongside other concerns
/// (keyframes, theme restyle, dirty tracking) step nodes individually with
/// [`TransitionAnimator::step_node`]; callers that only need transitions can
/// walk a whole tree with [`TransitionAnimator::apply`].
#[derive(Debug, Default)]
pub struct TransitionAnimator {
    active: HashMap<String, ActiveTransition>,
}

impl TransitionAnimator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.active.contains_key(key)
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }

    pub fn remove(&mut self, key: &str) {
        self.active.remove(key);
    }

    /// Style currently displayed by an in-flight transition for `key`.
    pub fn displayed_style(&self, key: &str, now: Instant) -> Option<AnimatableStyle> {
        self.active.get(key).map(|transition| transition.current(now))
    }

    /// The transition for `key` if it has not finished — used to classify the
    /// active animation property bucket.
    pub fn active_unfinished(&self, key: &str, now: Instant) -> Option<&ActiveTransition> {
        self.active
            .get(key)
            .filter(|transition| !transition.finished(now))
    }

    /// Drop transitions whose key left the live set or that have finished.
    pub fn retain_live(&mut self, live: &HashSet<String>, now: Instant) {
        self.active
            .retain(|key, transition| live.contains(key) && !transition.finished(now));
    }

    pub fn has_active(&self, now: Instant) -> bool {
        self.active
            .values()
            .any(|transition| !transition.finished(now))
    }

    /// Step a single keyed node toward the target described by its own
    /// `computed_style.transition`. `previous_displayed` is the value shown for
    /// this node last frame. Mutates `node`'s computed style to the current
    /// interpolated value and returns `true` if a transition is still active.
    pub fn step_node(
        &mut self,
        key: &str,
        node: &mut WidgetNode,
        previous_displayed: AnimatableStyle,
        now: Instant,
    ) -> bool {
        let desired = AnimatableStyle::from_node(node);
        let transition = node.computed_style.transitions.first().copied().unwrap_or_default();
        let props = transition.properties;

        // The clamped visual radius is authoritative whether or not the radius
        // itself animates, so push it onto the node before any interpolation.
        if props.animates_border_radius() {
            node.computed_style.border_radius = desired.border_radius;
        }

        let should_animate = transition.duration_ms > 0 && previous_displayed.differs(&desired, props);

        if should_animate {
            let restart = self.active.get(key).is_none_or(|transition_in_flight| {
                transition_in_flight.to != desired
                    || transition_in_flight.source != transition
                    || transition_in_flight.finished(now)
            });

            if restart {
                let from = AnimatableStyle::selective_from(previous_displayed, desired, props);
                self.active.insert(
                    key.to_string(),
                    ActiveTransition {
                        from,
                        to: desired,
                        started_at: now,
                        duration: Duration::from_millis(u64::from(transition.duration_ms)),
                        delay: Duration::from_millis(u64::from(transition.delay_ms)),
                        easing: transition.easing.into(),
                        source: transition,
                    },
                );
            }
        } else {
            self.active.remove(key);
        }

        if let Some(transition_in_flight) = self.active.get(key) {
            transition_in_flight.current(now).apply_to_node(node);
            if !transition_in_flight.finished(now) {
                return true;
            }
        }
        false
    }

    /// Walk a widget tree and step the transition for every `_mesh_key` node
    /// using that node's own `computed_style.transition`. Suitable for
    /// consumers that only need transitions (no keyframes or theme
    /// orchestration). Returns `true` if any transition is active.
    pub fn apply(&mut self, tree: &mut WidgetNode, now: Instant) -> bool {
        let mut live = HashSet::new();
        let active = self.apply_node(tree, now, &mut live);
        self.retain_live(&live, now);
        active
    }

    fn apply_node(
        &mut self,
        node: &mut WidgetNode,
        now: Instant,
        live: &mut HashSet<String>,
    ) -> bool {
        let mut active = false;
        if let Some(key) = node.attributes.get("_mesh_key").cloned() {
            live.insert(key.clone());
            let previous = self
                .displayed_style(&key, now)
                .unwrap_or_else(|| AnimatableStyle::from_node(node));
            active |= self.step_node(&key, node, previous, now);
        }
        for child in &mut node.children {
            active |= self.apply_node(child, now, live);
        }
        active
    }
}

fn lerp_dimension(from: Dimension, to: Dimension, progress: f32) -> Dimension {
    match (from, to) {
        (Dimension::Px(a), Dimension::Px(b)) => Dimension::Px(a.lerp(b, progress)),
        (Dimension::Percent(a), Dimension::Percent(b)) => Dimension::Percent(a.lerp(b, progress)),
        // Treat Auto as Px(0) when the other side is Px, so it interpolates through zero
        (Dimension::Auto, Dimension::Px(b)) => Dimension::Px((0.0f32).lerp(b, progress)),
        (Dimension::Px(a), Dimension::Auto) => Dimension::Px(a.lerp(0.0, progress)),
        _ => to,
    }
}

fn lerp_option_f32(from: Option<f32>, to: Option<f32>, progress: f32) -> Option<f32> {
    match (from, to) {
        (Some(a), Some(b)) => Some(a.lerp(b, progress)),
        // Treat None as Some(0) so None<->Some(v) transitions interpolate through zero
        (None, Some(b)) => Some((0.0f32).lerp(b, progress)),
        (Some(a), None) => Some(a.lerp(0.0, progress)),
        (None, None) => None,
    }
}

fn lerp_box_shadow(from: BoxShadow, to: BoxShadow, progress: f32) -> BoxShadow {
    BoxShadow {
        offset_x: from.offset_x.lerp(to.offset_x, progress),
        offset_y: from.offset_y.lerp(to.offset_y, progress),
        blur_radius: from.blur_radius.lerp(to.blur_radius, progress),
        spread_radius: from.spread_radius.lerp(to.spread_radius, progress),
        color: from.color.lerp(to.color, progress),
        inset: if progress < 0.5 { from.inset } else { to.inset },
    }
}

fn lerp_visual_filter(from: VisualFilter, to: VisualFilter, progress: f32) -> VisualFilter {
    VisualFilter {
        blur_radius: from.blur_radius.lerp(to.blur_radius, progress),
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
    fn from_node_clamps_border_radius_to_visible_cap() {
        let mut node = WidgetNode::new("button");
        node.layout.width = 32.0;
        node.layout.height = 28.0;
        node.computed_style.border_radius = Corners::all(9999.0);

        let style = AnimatableStyle::from_node(&node);

        // cap = min(32, 28) / 2 = 14.
        assert_eq!(style.border_radius, Corners::all(14.0));
    }

    #[test]
    fn step_node_drives_opacity_transition_to_completion() {
        let transition = TransitionStyle {
            duration_ms: 100,
            properties: mesh_core_elements::TransitionProperties {
                opacity: true,
                ..mesh_core_elements::TransitionProperties::none()
            },
            ..TransitionStyle::default()
        };

        let mut animator = TransitionAnimator::new();
        let mut node = WidgetNode::new("box");
        node.computed_style.transition = transition;
        node.computed_style.opacity = 1.0;

        // Previously displayed at 0.0; target is 1.0 -> transition starts.
        let start = Instant::now();
        let previous = AnimatableStyle {
            opacity: 0.0,
            ..AnimatableStyle::from_node(&node)
        };
        let active = animator.step_node("k", &mut node, previous, start);
        assert!(active);
        assert!(node.computed_style.opacity < 1.0);
        assert!(animator.contains_key("k"));

        // A fresh tree rebuild re-asserts the desired target (1.0) each frame.
        node.computed_style.opacity = 1.0;
        let done = start + Duration::from_millis(150);
        let displayed = animator.displayed_style("k", done).expect("in flight");
        let still_active = animator.step_node("k", &mut node, displayed, done);
        assert!(!still_active);
        assert!((node.computed_style.opacity - 1.0).abs() < 1e-4);
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
            box_shadow: BoxShadow::NONE,
            filter: VisualFilter::NONE,
            backdrop_filter: VisualFilter::NONE,
            font_size: 10.0,
            letter_spacing: 0.0,
            line_height: 1.0,
            gap: 0.0,
            inset_top: Some(0.0),
            inset_right: None,
            inset_bottom: None,
            inset_left: None,
            visibility: Visibility::Visible,
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
