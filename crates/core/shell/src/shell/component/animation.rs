use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::shell::component::ComponentDirtyFlags;
use mesh_core_animation::{
    Interpolate,
    keyframes::{
        ActiveKeyframeAnimation, KeyframeRegistry, KeyframeRule as RenderKeyframeRule,
        KeyframeStop as RenderKeyframeStop,
    },
    transition::AnimatableStyle,
};
use mesh_core_component::style as component_style;
use mesh_core_elements::{
    BoxShadow, Corners, Dimension, Edges, StyleResolver, Transform2D, TransitionEasing,
    TransitionStyle, VisualFilter, WidgetNode,
    style::{AnimationPlayState, AnimationPropertyBucket, Color},
};

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
    box_shadow: BoxShadow,
    filter: VisualFilter,
    backdrop_filter: VisualFilter,
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
            border_radius: visual_border_radius(
                s.border_radius,
                node.layout.width,
                node.layout.height,
            ),
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
            box_shadow: lerp_box_shadow(self.box_shadow, target.box_shadow, progress),
            filter: lerp_visual_filter(self.filter, target.filter, progress),
            backdrop_filter: lerp_visual_filter(
                self.backdrop_filter,
                target.backdrop_filter,
                progress,
            ),
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
        }
    }
}

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

pub(super) fn active_transition_bucket(transition: TransitionStyle) -> AnimationPropertyBucket {
    transition.properties.animation_bucket()
}

impl FrontendSurfaceComponent {
    #[cfg(test)]
    pub(super) fn apply_style_animations(&mut self, tree: &mut WidgetNode) {
        let previous_styles = self.previous_visual_styles();
        self.apply_style_animations_with_previous(tree, &previous_styles);
    }

    pub(super) fn previous_visual_styles(&self) -> HashMap<String, AnimatedVisualStyle> {
        self.last_tree
            .as_ref()
            .map(collect_visual_styles)
            .unwrap_or_default()
    }

    pub(super) fn apply_style_animations_with_previous(
        &mut self,
        tree: &mut WidgetNode,
        previous_styles: &HashMap<String, AnimatedVisualStyle>,
    ) {
        let now = Instant::now();
        let mut live_keys = HashSet::new();
        let mut live_keyframe_keys = HashSet::new();
        let mut has_active_animation = false;
        let mut has_layout_affecting_animation = false;
        let mut has_active_keyframe_animation = false;
        let theme = self.active_theme.borrow().clone();
        let resolver = StyleResolver::new(&theme);

        self.apply_style_animations_to_node(
            tree,
            previous_styles,
            &resolver,
            now,
            &mut live_keys,
            &mut live_keyframe_keys,
            &mut has_active_animation,
            &mut has_layout_affecting_animation,
            &mut has_active_keyframe_animation,
        );

        self.style_animations
            .retain(|key, animation| live_keys.contains(key) && !animation.finished(now));
        self.keyframe_animations
            .retain(|key, _| live_keyframe_keys.contains(key));
        self.keyframe_rules
            .retain(|key, _| live_keyframe_keys.contains(key));
        self.has_active_keyframe_animation = has_active_keyframe_animation;

        if has_active_animation || has_active_keyframe_animation {
            // Animations only mutate style/layout, never script state — keep
            // the cheap restyle-only path engaged so we don't drag the Luau
            // tree-build into every animation tick.
            let flags = if has_layout_affecting_animation || has_active_keyframe_animation {
                ComponentDirtyFlags::STYLE_RELAYOUT
            } else {
                ComponentDirtyFlags::VISUAL_REPAINT
            };
            self.invalidate_style_path(flags);
        }
    }

    fn apply_style_animations_to_node(
        &mut self,
        node: &mut WidgetNode,
        previous_styles: &HashMap<String, AnimatedVisualStyle>,
        resolver: &StyleResolver,
        now: Instant,
        live_keys: &mut HashSet<String>,
        live_keyframe_keys: &mut HashSet<String>,
        has_active_animation: &mut bool,
        has_layout_affecting_animation: &mut bool,
        has_active_keyframe_animation: &mut bool,
    ) {
        if let Some(key) = node.attributes.get("_mesh_key").cloned() {
            live_keys.insert(key.clone());
            self.apply_node_style_animation(&key, node, previous_styles, now, has_active_animation);
            if self.style_animations.get(&key).is_some_and(|animation| {
                !animation.finished(now) && animation.transition.properties.affects_layout()
            }) {
                *has_layout_affecting_animation = true;
            }
            self.apply_node_keyframe_animation(
                &key,
                node,
                resolver,
                now,
                live_keyframe_keys,
                has_active_keyframe_animation,
            );
        }

        for child in &mut node.children {
            self.apply_style_animations_to_node(
                child,
                previous_styles,
                resolver,
                now,
                live_keys,
                live_keyframe_keys,
                has_active_animation,
                has_layout_affecting_animation,
                has_active_keyframe_animation,
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
        if node.computed_style.animation.name.is_some() {
            // CSS animations own their animated properties; do not layer
            // transition playback on top of the same node.
            self.style_animations.remove(key);
            return;
        }

        let desired = AnimatedVisualStyle::from_node(node);
        let previous_displayed = self
            .style_animations
            .get(key)
            .map(|animation| animation.current_style(now))
            .or_else(|| previous_styles.get(key).copied())
            .unwrap_or(desired);

        let transition = node.computed_style.transition;
        let props = transition.properties;
        if props.animates_border_radius() {
            node.computed_style.border_radius = desired.border_radius;
        }
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

    fn apply_node_keyframe_animation(
        &mut self,
        key: &str,
        node: &mut WidgetNode,
        resolver: &StyleResolver,
        now: Instant,
        live_keyframe_keys: &mut HashSet<String>,
        has_active_keyframe_animation: &mut bool,
    ) {
        let Some(animation_name) = node.computed_style.animation.name.clone() else {
            return;
        };

        let animation_key = format!("{key}::{animation_name}");
        live_keyframe_keys.insert(animation_key.clone());

        let Some(parsed_rule) = self.find_component_keyframe_rule(&animation_name).cloned() else {
            self.record_runtime_animation_diagnostic(format!(
                "unresolved animation '{animation_name}'"
            ));
            return;
        };

        let render_rule =
            self.build_render_keyframe_rule(&animation_key, &parsed_rule, node, resolver);
        self.keyframe_rules
            .insert(animation_key.clone(), render_rule.clone());

        let existing = self.keyframe_animations.get(&animation_key).cloned();
        let animation_style = node.computed_style.animation.clone();
        let paused_at = match (&existing, animation_style.play_state) {
            (Some(active), AnimationPlayState::Paused)
                if active.play_state == AnimationPlayState::Paused =>
            {
                active.paused_at
            }
            (Some(_), AnimationPlayState::Paused) => Some(now),
            (None, AnimationPlayState::Paused) => Some(now),
            _ => None,
        };

        let active = ActiveKeyframeAnimation {
            rule_name: animation_key.clone(),
            started_at: existing
                .map(|animation| animation.started_at)
                .unwrap_or(now),
            duration: Duration::from_millis(u64::from(animation_style.duration_ms)),
            delay: Duration::from_millis(u64::from(animation_style.delay_ms)),
            easing: animation_style.easing.into(),
            iteration_count: animation_style.iteration_count,
            direction: animation_style.direction,
            fill_mode: animation_style.fill_mode,
            play_state: animation_style.play_state,
            paused_at,
        };
        self.keyframe_animations
            .insert(animation_key.clone(), active.clone());

        let mut registry = KeyframeRegistry::new();
        registry.insert(render_rule);
        if let Some(current) = active.current(&registry, AnimatableStyle::from_node(node), now) {
            current.apply_to_node(node);
        }

        if active.play_state == AnimationPlayState::Running && !active.finished(now) {
            *has_active_keyframe_animation = true;
        }
    }

    fn find_component_keyframe_rule(&self, name: &str) -> Option<&component_style::KeyframeRule> {
        self.compiled
            .component
            .style
            .as_ref()?
            .keyframes
            .iter()
            .find(|rule| rule.name == name)
    }

    fn build_render_keyframe_rule(
        &self,
        animation_key: &str,
        parsed_rule: &component_style::KeyframeRule,
        node: &WidgetNode,
        resolver: &StyleResolver,
    ) -> RenderKeyframeRule {
        let selector = node
            .attributes
            .get("_mesh_key")
            .map(|key| format!("#{key}"))
            .unwrap_or_else(|| node.tag.clone());
        let mut stops = Vec::new();

        for stop in &parsed_rule.stops {
            let mut computed_style = node.computed_style.clone();
            for diagnostic in resolver.apply_declarations_with_diagnostics(
                &mut computed_style,
                &stop.declarations,
                Some(&selector),
            ) {
                self.record_runtime_animation_diagnostic(diagnostic.message);
            }

            let mut styled_node = WidgetNode::new(&node.tag);
            styled_node.computed_style = computed_style;
            stops.push(RenderKeyframeStop {
                offset: stop.offset,
                style: AnimatableStyle::from_node(&styled_node),
                easing: None,
            });
        }

        RenderKeyframeRule {
            name: animation_key.to_string(),
            stops,
        }
    }

    pub(super) fn record_runtime_animation_diagnostic(&self, message: String) {
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.error(message);
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
    let renderer_easing: mesh_core_animation::Easing = easing.into();
    mesh_core_animation::apply_easing(renderer_easing, t)
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

    #[test]
    fn visual_border_radius_clamps_full_radius_to_visible_node_radius() {
        let radius = visual_border_radius(Corners::all(9999.0), 32.0, 28.0);

        assert_eq!(radius, Corners::all(14.0));
    }

    #[test]
    fn animated_visual_style_uses_visible_border_radius_for_full_radius() {
        let mut node = WidgetNode::new("button");
        node.layout.width = 32.0;
        node.layout.height = 28.0;
        node.computed_style.border_radius = Corners::all(9999.0);

        let style = AnimatedVisualStyle::from_node(&node);

        assert_eq!(style.border_radius, Corners::all(14.0));
    }

    #[test]
    fn animation_property_bucket_shell_helper_preserves_transition_classification() {
        let opacity = TransitionStyle {
            properties: mesh_core_elements::TransitionProperties {
                opacity: true,
                ..mesh_core_elements::TransitionProperties::none()
            },
            ..TransitionStyle::default()
        };
        let box_shadow = TransitionStyle {
            properties: mesh_core_elements::TransitionProperties {
                box_shadow: true,
                ..mesh_core_elements::TransitionProperties::none()
            },
            ..TransitionStyle::default()
        };
        let width = TransitionStyle {
            properties: mesh_core_elements::TransitionProperties {
                width: true,
                ..mesh_core_elements::TransitionProperties::none()
            },
            ..TransitionStyle::default()
        };

        assert_eq!(
            active_transition_bucket(opacity),
            AnimationPropertyBucket::PaintOnly
        );
        assert_eq!(
            active_transition_bucket(box_shadow),
            AnimationPropertyBucket::LayerEffect
        );
        assert_eq!(
            active_transition_bucket(width),
            AnimationPropertyBucket::LayoutAffecting
        );
    }
}
