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
    NodeId, StyleResolver, TransitionStyle, WidgetNode,
    style::{AnimationPlayState, AnimationPropertyBucket},
};
use mesh_core_theme::{Theme, ThemeKeyframeStop};

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

    #[cfg(test)]
    pub(super) fn previous_visual_styles(&self) -> HashMap<NodeId, AnimatableStyle> {
        self.last_tree
            .as_ref()
            .map(collect_visual_styles)
            .unwrap_or_default()
    }

    pub(super) fn take_previous_visual_styles(&mut self) -> HashMap<NodeId, AnimatableStyle> {
        let mut styles = std::mem::take(&mut self.previous_visual_styles_scratch);
        styles.clear();
        if let Some(last_tree) = self.last_tree.as_ref() {
            collect_visual_styles_into(last_tree, &mut styles);
        }
        styles
    }

    pub(super) fn restore_previous_visual_styles(
        &mut self,
        styles: HashMap<NodeId, AnimatableStyle>,
    ) {
        self.previous_visual_styles_scratch = styles;
    }

    pub(super) fn apply_style_animations_with_previous(
        &mut self,
        tree: &mut WidgetNode,
        previous_styles: &HashMap<NodeId, AnimatableStyle>,
        surface_css_props: &SurfaceCssProps,
    ) -> HashSet<NodeId> {
        let now = Instant::now();
        let mut live_keys = std::mem::take(&mut self.animation_live_keys_scratch);
        live_keys.clear();
        let mut live_keyframe_keys = std::mem::take(&mut self.animation_live_keyframe_keys_scratch);
        live_keyframe_keys.clear();
        let mut dirty_node_ids = std::mem::take(&mut self.animation_dirty_node_ids_scratch);
        dirty_node_ids.clear();
        let mut has_active_animation = false;
        let mut active_animation_bucket = AnimationPropertyBucket::None;
        let mut has_active_keyframe_animation = false;
        let mut active_keyframe_bucket = AnimationPropertyBucket::None;
        let theme = self.active_theme.borrow().clone();
        let resolver = StyleResolver::new(&theme).with_borrowed_props(surface_css_props);

        self.apply_style_animations_to_node(
            tree,
            previous_styles,
            &resolver,
            &theme,
            now,
            false,
            &mut live_keys,
            &mut live_keyframe_keys,
            &mut has_active_animation,
            &mut active_animation_bucket,
            &mut has_active_keyframe_animation,
            &mut active_keyframe_bucket,
            &mut dirty_node_ids,
        );

        self.transitions.retain_live(&live_keys, now);
        self.keyframe_animations
            .retain(|key, _| live_keyframe_keys.contains(key));
        self.keyframe_rules
            .retain(|key, _| live_keyframe_keys.contains(key));
        self.has_active_keyframe_animation = has_active_keyframe_animation;
        self.animation_live_keys_scratch = live_keys;
        self.animation_live_keyframe_keys_scratch = live_keyframe_keys;

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
            self.invalidate_animation_style_path(flags);
        }
        dirty_node_ids
    }

    fn apply_style_animations_to_node(
        &mut self,
        node: &mut WidgetNode,
        previous_styles: &HashMap<NodeId, AnimatableStyle>,
        resolver: &StyleResolver,
        theme: &Theme,
        now: Instant,
        ancestor_entering: bool,
        live_keys: &mut HashSet<NodeId>,
        live_keyframe_keys: &mut HashSet<String>,
        has_active_animation: &mut bool,
        active_animation_bucket: &mut AnimationPropertyBucket,
        has_active_keyframe_animation: &mut bool,
        active_keyframe_bucket: &mut AnimationPropertyBucket,
        dirty_node_ids: &mut HashSet<NodeId>,
    ) {
        let entering = ancestor_entering
            || node
                .attributes
                .get("_mesh_surface_entering")
                .is_some_and(|value| value == "true");
        if node.mesh_key().is_some() {
            let node_id = node.id;
            live_keys.insert(node_id);
            let previous_style = if entering {
                // A promoted child is mapped from this exact paint. Snap its
                // first buffer to the authored entrance state; on the next
                // paint the marker disappears and the normal transition pass
                // animates from this snapshot to the resting style.
                self.transitions.remove(node_id);
                previous_styles.get(&node_id).copied()
            } else {
                self.apply_node_style_animation(
                    node_id,
                    node,
                    previous_styles,
                    now,
                    has_active_animation,
                )
            };
            if let Some(transition) = self.transitions.active_unfinished(node_id, now) {
                *active_animation_bucket = merge_animation_bucket(
                    *active_animation_bucket,
                    active_transition_bucket(transition.source),
                );
            }
            self.apply_node_keyframe_animation(
                node,
                resolver,
                theme,
                now,
                live_keyframe_keys,
                has_active_keyframe_animation,
                active_keyframe_bucket,
            );
            if previous_style.is_some_and(|previous| previous != AnimatableStyle::from_node(node)) {
                dirty_node_ids.insert(node.id);
            }
        }

        for child in &mut node.children {
            self.apply_style_animations_to_node(
                child,
                previous_styles,
                resolver,
                theme,
                now,
                entering,
                live_keys,
                live_keyframe_keys,
                has_active_animation,
                active_animation_bucket,
                has_active_keyframe_animation,
                active_keyframe_bucket,
                dirty_node_ids,
            );
        }
    }

    fn apply_node_style_animation(
        &mut self,
        key: NodeId,
        node: &mut WidgetNode,
        previous_styles: &HashMap<NodeId, AnimatableStyle>,
        now: Instant,
        has_active_animation: &mut bool,
    ) -> Option<AnimatableStyle> {
        if node
            .computed_style
            .animations
            .iter()
            .any(|a| a.name.is_some())
        {
            // CSS animations own their animated properties; do not layer
            // transition playback on top of the same node.
            self.transitions.remove(key);
            return previous_styles.get(&key).copied();
        }

        // The value shown for this node last frame: the in-flight transition's
        // current value if one exists, otherwise the previous tree snapshot,
        // otherwise the node's own desired style (nothing to animate from).
        let desired = AnimatableStyle::from_node(node);
        let previous_displayed = self
            .transitions
            .displayed_style(key, now)
            .or_else(|| previous_styles.get(&key).copied())
            .unwrap_or(desired);

        if self
            .transitions
            .step_node(key, node, previous_displayed, now)
        {
            *has_active_animation = true;
        }
        Some(previous_displayed)
    }

    fn apply_node_keyframe_animation(
        &mut self,
        node: &mut WidgetNode,
        resolver: &StyleResolver,
        theme: &Theme,
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

        let Some(key) = node.mesh_key().map(str::to_owned) else {
            return;
        };

        for animation_style in animations {
            let animation_name = animation_style.name.clone().unwrap();

            let animation_key = format!("{key}::{animation_name}");
            live_keyframe_keys.insert(animation_key.clone());

            let stops = self
                .find_component_keyframe_rule(&animation_name)
                .map(|rule| rule.stops.clone())
                .or_else(|| theme_keyframe_stops(theme, &animation_name));
            let Some(stops) = stops else {
                self.record_runtime_animation_diagnostic(format!(
                    "unresolved animation '{animation_name}'"
                ));
                continue;
            };

            let render_rule =
                self.build_render_keyframe_rule(&animation_key, &stops, node, resolver);
            let keyframe_bucket = keyframe_rule_animation_bucket(&render_rule);
            self.keyframe_rules
                .insert(animation_key.clone(), render_rule.clone());

            let existing = self.keyframe_animations.get(&animation_key).cloned();
            let mut active = existing.unwrap_or(ActiveKeyframeAnimation {
                rule_name: animation_key.clone(),
                started_at: now,
                duration: Duration::ZERO,
                delay: Duration::ZERO,
                easing: animation_style.easing.into(),
                iteration_count: animation_style.iteration_count,
                direction: animation_style.direction,
                fill_mode: animation_style.fill_mode,
                play_state: AnimationPlayState::Running,
                paused_at: None,
            });
            active.rule_name = animation_key.clone();
            active.duration = Duration::from_millis(u64::from(animation_style.duration_ms));
            active.delay = Duration::from_millis(u64::from(animation_style.delay_ms));
            active.easing = animation_style.easing.into();
            active.iteration_count = animation_style.iteration_count;
            active.direction = animation_style.direction;
            active.fill_mode = animation_style.fill_mode;
            active.set_play_state(animation_style.play_state, now);
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
        parsed_stops: &[component_style::KeyframeStop],
        node: &WidgetNode,
        resolver: &StyleResolver,
    ) -> RenderKeyframeRule {
        let selector = node
            .mesh_key()
            .map(|key| format!("#{key}"))
            .unwrap_or_else(|| node.tag.clone());
        let mut stops = Vec::new();

        for stop in parsed_stops {
            let mut computed_style = node.computed_style.clone();
            for diagnostic in resolver.apply_declarations_with_diagnostics_and_variables(
                &mut computed_style,
                &stop.declarations,
                Some(&selector),
                &node.computed_style.custom_properties,
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

fn theme_keyframe_stops(theme: &Theme, name: &str) -> Option<Vec<component_style::KeyframeStop>> {
    Some(
        theme
            .keyframe_stops(name)?
            .iter()
            .map(theme_keyframe_stop)
            .collect(),
    )
}

fn theme_keyframe_stop(stop: &ThemeKeyframeStop) -> component_style::KeyframeStop {
    component_style::KeyframeStop {
        offset: stop.offset,
        declarations: stop
            .declarations
            .iter()
            .map(|(property, value)| component_style::Declaration {
                property: property.clone(),
                value: theme_style_value(value),
            })
            .collect(),
    }
}

fn theme_style_value(value: &str) -> component_style::StyleValue {
    let value = value.trim();
    if value.starts_with("var(") && value.ends_with(')') {
        component_style::StyleValue::Var(value[4..value.len() - 1].trim().to_string())
    } else {
        component_style::StyleValue::Literal(value.to_string())
    }
}

#[cfg(test)]
pub(super) fn collect_visual_styles(root: &WidgetNode) -> HashMap<NodeId, AnimatableStyle> {
    let mut styles = HashMap::new();
    collect_visual_styles_into(root, &mut styles);
    styles
}

fn collect_visual_styles_into(node: &WidgetNode, styles: &mut HashMap<NodeId, AnimatableStyle>) {
    if node.mesh_key().is_some() {
        styles.insert(node.id, AnimatableStyle::from_node(node));
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

    // cargo test -p mesh-core-shell --release -- animation_live_key_scratch_reuse_beats_fresh_sets --ignored --nocapture
    #[test]
    #[ignore = "release-only animation live-key scratch microbenchmark"]
    fn animation_live_key_scratch_reuse_beats_fresh_sets() {
        let keys: Vec<String> = (0..512).map(|index| format!("root/{index}")).collect();
        let iterations = 20_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let mut live = HashSet::new();
            for key in &keys {
                live.insert(key.clone());
            }
            old_total += std::hint::black_box(live.len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        let mut live = HashSet::new();
        for _ in 0..iterations {
            live.clear();
            for key in &keys {
                live.insert(key.clone());
            }
            new_total += std::hint::black_box(live.len());
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_total, new_total);
        eprintln!(
            "animation live-key sets: fresh {old_time:?}; scratch reuse {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release --lib animation_node_id_keys_beats_string_keys -- --ignored --nocapture
    #[test]
    #[ignore = "release-only animation NodeId key microbenchmark"]
    fn animation_node_id_keys_beats_string_keys() {
        let keys: Vec<String> = (0..1_024).map(|index| format!("root/{index}")).collect();
        let node_ids: Vec<NodeId> = (1..=keys.len() as NodeId).collect();
        let previous_strings: HashMap<String, u64> = keys
            .iter()
            .enumerate()
            .map(|(index, key)| (key.clone(), index as u64))
            .collect();
        let previous_node_ids: HashMap<NodeId, u64> = node_ids
            .iter()
            .copied()
            .enumerate()
            .map(|(index, node_id)| (node_id, index as u64))
            .collect();
        let iterations = 5_000usize;

        let old_started = Instant::now();
        let mut old_live = HashSet::new();
        let mut old_total = 0u64;
        for _ in 0..iterations {
            old_live.clear();
            for key in &keys {
                // This is the former hot-path shape: own the mesh key for the
                // transition lookup, then clone it into the live set.
                let owned_key = key.clone();
                old_live.insert(owned_key.clone());
                old_total = old_total.wrapping_add(
                    previous_strings
                        .get(&owned_key)
                        .copied()
                        .unwrap_or_default(),
                );
            }
            old_total = old_total.wrapping_add(old_live.len() as u64);
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_live = HashSet::new();
        let mut new_total = 0u64;
        for _ in 0..iterations {
            new_live.clear();
            for node_id in &node_ids {
                new_live.insert(*node_id);
                new_total = new_total
                    .wrapping_add(previous_node_ids.get(node_id).copied().unwrap_or_default());
            }
            new_total = new_total.wrapping_add(new_live.len() as u64);
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_total, new_total);
        eprintln!(
            "animation identity keys: owned strings {old_time:?}; NodeId {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- animation_previous_style_scratch_reuse_beats_fresh_map --ignored --nocapture
    #[test]
    #[ignore = "release-only animation previous-style scratch microbenchmark"]
    fn animation_previous_style_scratch_reuse_beats_fresh_map() {
        fn build_tree(next_id: &mut usize, width: usize, depth: usize) -> WidgetNode {
            let id = *next_id;
            *next_id += 1;
            let mut node = WidgetNode::new("box");
            node.set_mesh_key(format!("root/{id}"));
            if depth > 0 {
                node.children = (0..width)
                    .map(|_| build_tree(next_id, width, depth - 1))
                    .collect();
            }
            node
        }

        let mut next_id = 0;
        let root = build_tree(&mut next_id, 4, 5);
        let iterations = 20_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total +=
                std::hint::black_box(collect_visual_styles(std::hint::black_box(&root)).len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        let mut styles = HashMap::new();
        for _ in 0..iterations {
            styles.clear();
            collect_visual_styles_into(std::hint::black_box(&root), &mut styles);
            new_total += std::hint::black_box(styles.len());
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_total, new_total);
        eprintln!(
            "animation previous styles: fresh map {old_time:?}; scratch reuse {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }
}
