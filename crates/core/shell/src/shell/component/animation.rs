use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mesh_core_elements::{Corners, TransitionEasing, TransitionStyle, WidgetNode, style::Color};

use super::FrontendSurfaceComponent;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct AnimatedVisualStyle {
    border_radius: Corners,
    opacity: f32,
    background_color: Color,
    color: Color,
}

impl AnimatedVisualStyle {
    fn from_node(node: &WidgetNode) -> Self {
        Self {
            border_radius: node.computed_style.border_radius,
            opacity: node.computed_style.opacity,
            background_color: node.computed_style.background_color,
            color: node.computed_style.color,
        }
    }

    fn apply_to_node(self, node: &mut WidgetNode) {
        node.computed_style.border_radius = self.border_radius;
        node.computed_style.opacity = self.opacity;
        node.computed_style.background_color = self.background_color;
        node.computed_style.color = self.color;
    }

    fn interpolate(self, target: Self, progress: f32) -> Self {
        Self {
            border_radius: lerp_corners(self.border_radius, target.border_radius, progress),
            opacity: lerp_f32(self.opacity, target.opacity, progress),
            background_color: lerp_color(self.background_color, target.background_color, progress),
            color: lerp_color(self.color, target.color, progress),
        }
    }

    fn selective_from(
        previous: Self,
        desired: Self,
        props: mesh_core_elements::TransitionProperties,
    ) -> Self {
        Self {
            border_radius: if props.animates_border_radius() {
                previous.border_radius
            } else {
                desired.border_radius
            },
            opacity: if props.animates_opacity() {
                previous.opacity
            } else {
                desired.opacity
            },
            background_color: if props.animates_background_color() {
                previous.background_color
            } else {
                desired.background_color
            },
            color: if props.animates_color() {
                previous.color
            } else {
                desired.color
            },
        }
    }
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
                || (props.animates_opacity() && previous_displayed.opacity != desired.opacity)
                || (props.animates_background_color()
                    && previous_displayed.background_color != desired.background_color)
                || (props.animates_color() && previous_displayed.color != desired.color));

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
    match easing {
        TransitionEasing::Linear => t,
        TransitionEasing::Ease => ease_in_out_cubic(t),
        TransitionEasing::EaseIn => ease_in_cubic(t),
        TransitionEasing::EaseOut => ease_out_cubic(t),
        TransitionEasing::EaseInOut => ease_in_out_cubic(t),
    }
}

fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn lerp_corners(from: Corners, to: Corners, progress: f32) -> Corners {
    Corners {
        top_left: lerp_f32(from.top_left, to.top_left, progress),
        top_right: lerp_f32(from.top_right, to.top_right, progress),
        bottom_right: lerp_f32(from.bottom_right, to.bottom_right, progress),
        bottom_left: lerp_f32(from.bottom_left, to.bottom_left, progress),
    }
}

fn lerp_color(from: Color, to: Color, progress: f32) -> Color {
    Color {
        r: lerp_f32(from.r as f32, to.r as f32, progress).round() as u8,
        g: lerp_f32(from.g as f32, to.g as f32, progress).round() as u8,
        b: lerp_f32(from.b as f32, to.b as f32, progress).round() as u8,
        a: lerp_f32(from.a as f32, to.a as f32, progress).round() as u8,
    }
}

fn lerp_f32(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}
