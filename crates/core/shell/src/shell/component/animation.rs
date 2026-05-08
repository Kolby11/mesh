use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mesh_core_elements::{
    Corners, Dimension, Edges, Transform2D, TransitionEasing, TransitionStyle, WidgetNode,
    style::Color,
};
use mesh_core_render::animation::Interpolate;

use super::FrontendSurfaceComponent;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct AnimatedVisualStyle {
    border_radius: Corners,
    border_width: Edges,
    opacity: f32,
    background_color: Color,
    border_color: Color,
    color: Color,
    width: Dimension,
    height: Dimension,
    min_width: Option<f32>,
    max_width: Option<f32>,
    min_height: Option<f32>,
    max_height: Option<f32>,
    padding: Edges,
    margin: Edges,
    transform: Transform2D,
    font_size: f32,
    letter_spacing: f32,
    line_height: f32,
    gap: f32,
    inset_top: Option<f32>,
    inset_right: Option<f32>,
    inset_bottom: Option<f32>,
    inset_left: Option<f32>,
}

impl AnimatedVisualStyle {
    fn from_node(node: &WidgetNode) -> Self {
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

    fn apply_to_node(self, node: &mut WidgetNode) {
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

    /// Per-field lerp. Edges/Corners/Color/f32 use the renderer's `Interpolate`.
    /// `Dimension` only interpolates between matching variants (`Px`->`Px`,
    /// `Percent`->`Percent`); cross-variant changes snap to the target.
    fn interpolate(self, target: Self, progress: f32) -> Self {
        Self {
            border_radius: self.border_radius.lerp(target.border_radius, progress),
            border_width: self.border_width.lerp(target.border_width, progress),
            opacity: self.opacity.lerp(target.opacity, progress),
            background_color: self
                .background_color
                .lerp(target.background_color, progress),
            border_color: self.border_color.lerp(target.border_color, progress),
            color: self.color.lerp(target.color, progress),
            width: lerp_dimension(self.width, target.width, progress),
            height: lerp_dimension(self.height, target.height, progress),
            min_width: lerp_option_f32(self.min_width, target.min_width, progress),
            max_width: lerp_option_f32(self.max_width, target.max_width, progress),
            min_height: lerp_option_f32(self.min_height, target.min_height, progress),
            max_height: lerp_option_f32(self.max_height, target.max_height, progress),
            padding: self.padding.lerp(target.padding, progress),
            margin: self.margin.lerp(target.margin, progress),
            transform: self.transform.lerp(target.transform, progress),
            font_size: self.font_size.lerp(target.font_size, progress),
            letter_spacing: self.letter_spacing.lerp(target.letter_spacing, progress),
            line_height: self.line_height.lerp(target.line_height, progress),
            gap: self.gap.lerp(target.gap, progress),
            inset_top: lerp_option_f32(self.inset_top, target.inset_top, progress),
            inset_right: lerp_option_f32(self.inset_right, target.inset_right, progress),
            inset_bottom: lerp_option_f32(self.inset_bottom, target.inset_bottom, progress),
            inset_left: lerp_option_f32(self.inset_left, target.inset_left, progress),
        }
    }

    /// Build the start-of-animation snapshot: take the previous displayed
    /// value for any property the transition opts into, and the desired (new)
    /// value for everything else. The animator only lerps the opted-in
    /// properties because everything else has matching from/to.
    fn selective_from(
        previous: Self,
        desired: Self,
        props: mesh_core_elements::TransitionProperties,
    ) -> Self {
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
        }
    }
}

fn pick<T>(use_previous: bool, previous: T, desired: T) -> T {
    if use_previous { previous } else { desired }
}

#[derive(Debug, Clone)]
pub(super) struct StyleAnimation {
    from: AnimatedVisualStyle,
    to: AnimatedVisualStyle,
    started_at: Instant,
    duration: Duration,
    delay: Duration,
    transition: TransitionStyle,
}

impl StyleAnimation {
    fn current_style(&self, now: Instant) -> AnimatedVisualStyle {
        if self.duration.is_zero() {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed < self.delay {
            return self.from;
        }
        let active_elapsed = elapsed - self.delay;
        let raw = (active_elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.from
            .interpolate(self.to, apply_easing(self.transition.easing, raw))
    }

    pub(super) fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started_at) >= self.delay + self.duration
    }
}

impl FrontendSurfaceComponent {
    pub(super) fn apply_style_animations(&mut self, tree: &mut WidgetNode) {
        let previous_styles = self
            .last_tree
            .as_ref()
            .map(collect_visual_styles)
            .unwrap_or_default();
        let now = Instant::now();
        let mut live_keys = HashSet::new();
        let mut has_active_animation = false;

        self.apply_style_animations_to_node(
            tree,
            &previous_styles,
            now,
            &mut live_keys,
            &mut has_active_animation,
        );

        self.style_animations
            .retain(|key, animation| live_keys.contains(key) && !animation.finished(now));

        if has_active_animation {
            self.dirty = true;
        }
    }

    fn apply_style_animations_to_node(
        &mut self,
        node: &mut WidgetNode,
        previous_styles: &HashMap<String, AnimatedVisualStyle>,
        now: Instant,
        live_keys: &mut HashSet<String>,
        has_active_animation: &mut bool,
    ) {
        if let Some(key) = node.attributes.get("_mesh_key").cloned() {
            live_keys.insert(key.clone());
            self.apply_node_style_animation(&key, node, previous_styles, now, has_active_animation);
        }

        for child in &mut node.children {
            self.apply_style_animations_to_node(
                child,
                previous_styles,
                now,
                live_keys,
                has_active_animation,
            );
        }
    }

    fn apply_node_style_animation(
        &mut self,
        key: &str,
        node: &mut WidgetNode,
        previous_styles: &HashMap<String, AnimatedVisualStyle>,
        now: Instant,
        has_active_animation: &mut bool,
    ) {
        let desired = AnimatedVisualStyle::from_node(node);
        let previous_displayed = self
            .style_animations
            .get(key)
            .map(|animation| animation.current_style(now))
            .or_else(|| previous_styles.get(key).copied())
            .unwrap_or(desired);

        let transition = node.computed_style.transition;
        let props = transition.properties;
        let should_animate = transition.duration_ms > 0
            && ((props.animates_border_radius()
                && previous_displayed.border_radius != desired.border_radius)
                || (props.animates_border_width()
                    && previous_displayed.border_width != desired.border_width)
                || (props.animates_opacity() && previous_displayed.opacity != desired.opacity)
                || (props.animates_background_color()
                    && previous_displayed.background_color != desired.background_color)
                || (props.animates_border_color()
                    && previous_displayed.border_color != desired.border_color)
                || (props.animates_color() && previous_displayed.color != desired.color)
                || (props.animates_width() && previous_displayed.width != desired.width)
                || (props.animates_height() && previous_displayed.height != desired.height)
                || (props.animates_padding() && previous_displayed.padding != desired.padding)
                || (props.animates_margin() && previous_displayed.margin != desired.margin)
                || (props.animates_transform()
                    && previous_displayed.transform != desired.transform)
                || (props.animates_min_width()
                    && previous_displayed.min_width != desired.min_width)
                || (props.animates_max_width()
                    && previous_displayed.max_width != desired.max_width)
                || (props.animates_min_height()
                    && previous_displayed.min_height != desired.min_height)
                || (props.animates_max_height()
                    && previous_displayed.max_height != desired.max_height)
                || (props.animates_font_size()
                    && previous_displayed.font_size != desired.font_size)
                || (props.animates_letter_spacing()
                    && previous_displayed.letter_spacing != desired.letter_spacing)
                || (props.animates_line_height()
                    && previous_displayed.line_height != desired.line_height)
                || (props.animates_gap() && previous_displayed.gap != desired.gap)
                || (props.animates_inset_top()
                    && previous_displayed.inset_top != desired.inset_top)
                || (props.animates_inset_right()
                    && previous_displayed.inset_right != desired.inset_right)
                || (props.animates_inset_bottom()
                    && previous_displayed.inset_bottom != desired.inset_bottom)
                || (props.animates_inset_left()
                    && previous_displayed.inset_left != desired.inset_left));

        if should_animate {
            let restart = self.style_animations.get(key).is_none_or(|animation| {
                animation.to != desired
                    || animation.transition != transition
                    || animation.finished(now)
            });

            if restart {
                let from = AnimatedVisualStyle::selective_from(previous_displayed, desired, props);
                self.style_animations.insert(
                    key.to_string(),
                    StyleAnimation {
                        from,
                        to: desired,
                        started_at: now,
                        duration: Duration::from_millis(u64::from(transition.duration_ms)),
                        delay: Duration::from_millis(u64::from(transition.delay_ms)),
                        transition,
                    },
                );
            }
        } else {
            self.style_animations.remove(key);
        }

        if let Some(animation) = self.style_animations.get(key) {
            let current = animation.current_style(now);
            current.apply_to_node(node);
            if !animation.finished(now) {
                *has_active_animation = true;
            }
        }
    }
}

pub(super) fn collect_visual_styles(root: &WidgetNode) -> HashMap<String, AnimatedVisualStyle> {
    let mut styles = HashMap::new();
    collect_visual_styles_into(root, &mut styles);
    styles
}

fn collect_visual_styles_into(
    node: &WidgetNode,
    styles: &mut HashMap<String, AnimatedVisualStyle>,
) {
    if let Some(key) = node.attributes.get("_mesh_key") {
        styles.insert(key.clone(), AnimatedVisualStyle::from_node(node));
    }

    for child in &node.children {
        collect_visual_styles_into(child, styles);
    }
}

fn apply_easing(easing: TransitionEasing, t: f32) -> f32 {
    let renderer_easing: mesh_core_render::animation::Easing = easing.into();
    mesh_core_render::animation::apply_easing(renderer_easing, t)
}

/// Interpolate between two `Dimension` values when the variants match.
/// Cross-variant changes (e.g. `auto` -> `100px`) cannot be lerped without
/// resolving against a parent size, so they snap straight to the target.
fn lerp_dimension(from: Dimension, to: Dimension, progress: f32) -> Dimension {
    match (from, to) {
        (Dimension::Px(a), Dimension::Px(b)) => Dimension::Px(a.lerp(b, progress)),
        (Dimension::Percent(a), Dimension::Percent(b)) => Dimension::Percent(a.lerp(b, progress)),
        _ => to,
    }
}

/// Interpolate between two `Option<f32>` values. When both sides are `Some`,
/// lerp the inner values. Cross-variant transitions (`None` <-> `Some`) cannot
/// be smoothed because there is no numeric value to lerp from, so they snap
/// straight to the target — matching CSS behavior for unset constraints.
fn lerp_option_f32(from: Option<f32>, to: Option<f32>, progress: f32) -> Option<f32> {
    match (from, to) {
        (Some(a), Some(b)) => Some(a.lerp(b, progress)),
        _ => to,
    }
}
