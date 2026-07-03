use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::shell::component::{ComponentDirtyFlags, SurfaceCssProps};
use mesh_core_animation::{
    keyframes::{
        ActiveKeyframeAnimation, KeyframeRegistry, KeyframeRule as RenderKeyframeRule,
        KeyframeStop as RenderKeyframeStop,
    },
    transition::AnimatableStyle,
};
use mesh_core_component::style as component_style;
use mesh_core_elements::{
    StyleResolver, TransitionStyle, WidgetNode,
    style::{AnimationPlayState, AnimationPropertyBucket},
};

use super::FrontendSurfaceComponent;

pub(super) fn active_transition_bucket(transition: TransitionStyle) -> AnimationPropertyBucket {
    transition.properties.animation_bucket()
}

fn merge_animation_bucket(
    current: AnimationPropertyBucket,
    next: AnimationPropertyBucket,
) -> AnimationPropertyBucket {
    match (current, next) {
        (AnimationPropertyBucket::LayoutAffecting, _)
        | (_, AnimationPropertyBucket::LayoutAffecting) => AnimationPropertyBucket::LayoutAffecting,
        (AnimationPropertyBucket::LayerEffect, _) | (_, AnimationPropertyBucket::LayerEffect) => {
            AnimationPropertyBucket::LayerEffect
        }
        (AnimationPropertyBucket::PaintOnly, _) | (_, AnimationPropertyBucket::PaintOnly) => {
            AnimationPropertyBucket::PaintOnly
        }
        _ => AnimationPropertyBucket::None,
    }
}

pub(super) fn keyframe_rule_animation_bucket(rule: &RenderKeyframeRule) -> AnimationPropertyBucket {
    let mut bucket = AnimationPropertyBucket::None;
    for pair in rule.stops.windows(2) {
        let previous = pair[0].style;
        let next = pair[1].style;
        let changed = mesh_core_elements::TransitionProperties {
            border_radius: previous.border_radius != next.border_radius,
            border_width: previous.border_width != next.border_width,
            opacity: previous.opacity != next.opacity,
            background_color: previous.background_color != next.background_color,
            border_color: previous.border_color != next.border_color,
            color: previous.color != next.color,
            width: previous.width != next.width,
            height: previous.height != next.height,
            min_width: previous.min_width != next.min_width,
            max_width: previous.max_width != next.max_width,
            min_height: previous.min_height != next.min_height,
            max_height: previous.max_height != next.max_height,
            padding: previous.padding != next.padding,
            margin: previous.margin != next.margin,
            transform: previous.transform != next.transform,
            box_shadow: previous.box_shadow != next.box_shadow,
            filter: previous.filter != next.filter,
            backdrop_filter: previous.backdrop_filter != next.backdrop_filter,
            font_size: previous.font_size != next.font_size,
            letter_spacing: previous.letter_spacing != next.letter_spacing,
            line_height: previous.line_height != next.line_height,
            gap: previous.gap != next.gap,
            inset_top: previous.inset_top != next.inset_top,
            inset_right: previous.inset_right != next.inset_right,
            inset_bottom: previous.inset_bottom != next.inset_bottom,
            inset_left: previous.inset_left != next.inset_left,
            ..mesh_core_elements::TransitionProperties::none()
        }
        .animation_bucket();
        bucket = merge_animation_bucket(bucket, changed);
    }
    bucket
}

impl FrontendSurfaceComponent {
    pub(super) fn should_run_style_animation_pass(&self) -> bool {
        self.has_animatable_style_rules
            || !self.transitions.is_empty()
            || !self.keyframe_animations.is_empty()
            || self.has_active_keyframe_animation
    }

    #[cfg(test)]
    pub(super) fn apply_style_animations(&mut self, tree: &mut WidgetNode) {
        let previous_styles = self.previous_visual_styles();
        let surface_css_props = self.surface_css_props();
        self.apply_style_animations_with_previous(tree, &previous_styles, &surface_css_props);
    }

    pub(super) fn previous_visual_styles(&self) -> HashMap<String, AnimatableStyle> {
        self.last_tree
            .as_ref()
            .map(collect_visual_styles)
            .unwrap_or_default()
    }

    pub(super) fn apply_style_animations_with_previous(
        &mut self,
        tree: &mut WidgetNode,
        previous_styles: &HashMap<String, AnimatableStyle>,
        surface_css_props: &SurfaceCssProps,
    ) {
        let now = Instant::now();
        let mut live_keys = HashSet::new();
        let mut live_keyframe_keys = HashSet::new();
        let mut has_active_animation = false;
        let mut active_animation_bucket = AnimationPropertyBucket::None;
        let mut has_active_keyframe_animation = false;
        let mut active_keyframe_bucket = AnimationPropertyBucket::None;
        let theme = self.active_theme.borrow().clone();
        let resolver = StyleResolver::new(&theme).with_props(surface_css_props.clone());

        self.apply_style_animations_to_node(
            tree,
            previous_styles,
            &resolver,
            now,
            false,
            &mut live_keys,
            &mut live_keyframe_keys,
            &mut has_active_animation,
            &mut active_animation_bucket,
            &mut has_active_keyframe_animation,
            &mut active_keyframe_bucket,
        );

        self.transitions.retain_live(&live_keys, now);
        self.keyframe_animations
            .retain(|key, _| live_keyframe_keys.contains(key));
        self.keyframe_rules
            .retain(|key, _| live_keyframe_keys.contains(key));
        self.has_active_keyframe_animation = has_active_keyframe_animation;

        if has_active_animation || has_active_keyframe_animation {
            // Animations only mutate style/layout, never script state — keep
            // the cheap restyle-only path engaged so we don't drag the Luau
            // tree-build into every animation tick.
            let keyframes_need_layout = has_active_keyframe_animation
                && !matches!(
                    active_keyframe_bucket,
                    AnimationPropertyBucket::PaintOnly | AnimationPropertyBucket::LayerEffect
                );
            let flags = if active_animation_bucket == AnimationPropertyBucket::LayoutAffecting
                || keyframes_need_layout
            {
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
        previous_styles: &HashMap<String, AnimatableStyle>,
        resolver: &StyleResolver,
        now: Instant,
        ancestor_entering: bool,
        live_keys: &mut HashSet<String>,
        live_keyframe_keys: &mut HashSet<String>,
        has_active_animation: &mut bool,
        active_animation_bucket: &mut AnimationPropertyBucket,
        has_active_keyframe_animation: &mut bool,
        active_keyframe_bucket: &mut AnimationPropertyBucket,
    ) {
        let entering = ancestor_entering
            || node
                .attributes
                .get("_mesh_surface_entering")
                .is_some_and(|value| value == "true");
        if let Some(key) = node.attributes.get("_mesh_key").cloned() {
            live_keys.insert(key.clone());
            if entering {
                // A promoted child is mapped from this exact paint. Snap its
                // first buffer to the authored entrance state; on the next
                // paint the marker disappears and the normal transition pass
                // animates from this snapshot to the resting style.
                self.transitions.remove(&key);
            } else {
                self.apply_node_style_animation(
                    &key,
                    node,
                    previous_styles,
                    now,
                    has_active_animation,
                );
            }
            if let Some(transition) = self.transitions.active_unfinished(&key, now) {
                *active_animation_bucket = merge_animation_bucket(
                    *active_animation_bucket,
                    active_transition_bucket(transition.source),
                );
            }
            self.apply_node_keyframe_animation(
                &key,
                node,
                resolver,
                now,
                live_keyframe_keys,
                has_active_keyframe_animation,
                active_keyframe_bucket,
            );
        }

        for child in &mut node.children {
            self.apply_style_animations_to_node(
                child,
                previous_styles,
                resolver,
                now,
                entering,
                live_keys,
                live_keyframe_keys,
                has_active_animation,
                active_animation_bucket,
                has_active_keyframe_animation,
                active_keyframe_bucket,
            );
        }
    }

    fn apply_node_style_animation(
        &mut self,
        key: &str,
        node: &mut WidgetNode,
        previous_styles: &HashMap<String, AnimatableStyle>,
        now: Instant,
        has_active_animation: &mut bool,
    ) {
        if node
            .computed_style
            .animations
            .iter()
            .any(|a| a.name.is_some())
        {
            // CSS animations own their animated properties; do not layer
            // transition playback on top of the same node.
            self.transitions.remove(key);
            return;
        }

        // The value shown for this node last frame: the in-flight transition's
        // current value if one exists, otherwise the previous tree snapshot,
        // otherwise the node's own desired style (nothing to animate from).
        let desired = AnimatableStyle::from_node(node);
        let previous_displayed = self
            .transitions
            .displayed_style(key, now)
            .or_else(|| previous_styles.get(key).copied())
            .unwrap_or(desired);

        if self
            .transitions
            .step_node(key, node, previous_displayed, now)
        {
            *has_active_animation = true;
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
        active_keyframe_bucket: &mut AnimationPropertyBucket,
    ) {
        // Apply all named keyframe animations on this node
        let animations: Vec<_> = node
            .computed_style
            .animations
            .iter()
            .filter(|a| a.name.is_some())
            .cloned()
            .collect();

        if animations.is_empty() {
            return;
        }

        for animation_style in animations {
            let animation_name = animation_style.name.clone().unwrap();

            let animation_key = format!("{key}::{animation_name}");
            live_keyframe_keys.insert(animation_key.clone());

            let Some(parsed_rule) = self.find_component_keyframe_rule(&animation_name).cloned()
            else {
                self.record_runtime_animation_diagnostic(format!(
                    "unresolved animation '{animation_name}'"
                ));
                continue;
            };

            let render_rule =
                self.build_render_keyframe_rule(&animation_key, &parsed_rule, node, resolver);
            let keyframe_bucket = keyframe_rule_animation_bucket(&render_rule);
            self.keyframe_rules
                .insert(animation_key.clone(), render_rule.clone());

            let existing = self.keyframe_animations.get(&animation_key).cloned();
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
            if let Some(current) = active.current(&registry, AnimatableStyle::from_node(node), now)
            {
                current.apply_to_node(node);
            }

            if active.play_state == AnimationPlayState::Running && !active.finished(now) {
                *has_active_keyframe_animation = true;
                *active_keyframe_bucket =
                    merge_animation_bucket(*active_keyframe_bucket, keyframe_bucket);
            }
        } // end for animation_style in animations
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

pub(super) fn collect_visual_styles(root: &WidgetNode) -> HashMap<String, AnimatableStyle> {
    let mut styles = HashMap::new();
    collect_visual_styles_into(root, &mut styles);
    styles
}

fn collect_visual_styles_into(node: &WidgetNode, styles: &mut HashMap<String, AnimatableStyle>) {
    if let Some(key) = node.attributes.get("_mesh_key") {
        styles.insert(key.clone(), AnimatableStyle::from_node(node));
    }

    for child in &node.children {
        collect_visual_styles_into(child, styles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
