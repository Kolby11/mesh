use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::shell::component::{ComponentDirtyFlags, SurfaceCssProps};
use mesh_core_animation::{
    AnimationInstanceId, AnimationLifecycle, NextChange,
    keyframes::{
        ActiveKeyframeAnimation, KeyframeRule as RenderKeyframeRule,
        KeyframeStop as RenderKeyframeStop,
    },
    transition::AnimatableStyle,
};
use mesh_core_component::style as component_style;
use mesh_core_elements::{
    NodeId, StyleResolver, TransitionProperties, TransitionStyle, WidgetNode,
    style::{AnimationPlayState, AnimationPropertyBucket, AnimationStyle, ComputedStyle},
};
use mesh_core_theme::{Theme, ThemeKeyframeStop};

use super::FrontendSurfaceComponent;

/// Inputs of one paint's style animation pass. See
/// [`FrontendSurfaceComponent::run_style_animation_pass`].
pub(super) struct PendingAnimationPass {
    /// The previous frame, when this paint restyles the retained tree.
    pub(super) previous_snapshot: Option<super::FrameSnapshot>,
    /// Displayed styles of the previous tree, when there is no snapshot.
    pub(super) previous_visual_styles: HashMap<NodeId, AnimatableStyle>,
}

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

/// The strongest bucket among the properties `rule` moves away from `base`.
/// A property every stop declares at its `base` value never changes, so it
/// cannot force a relayout.
pub(super) fn keyframe_rule_animation_bucket(
    rule: &RenderKeyframeRule,
    base: &AnimatableStyle,
) -> AnimationPropertyBucket {
    rule.stops
        .iter()
        .fold(TransitionProperties::none(), |changed, stop| {
            changed.union(stop.style.differing(base, stop.properties))
        })
        .animation_bucket()
}

/// The authored values of a node's keyframe-animated properties, and the
/// values the last animation pass wrote over them.
#[derive(Debug, Clone, Copy)]
pub(super) struct KeyframeBaseStyle {
    authored: AnimatableStyle,
    applied: AnimatableStyle,
}

/// One named animation declared on a node, with its resolved stops.
struct NodeKeyframeAnimation {
    list_index: u32,
    style: AnimationStyle,
    name: String,
    source: KeyframeSource,
}

/// The stops an animation name resolves to, shared by every node using it
/// until the theme or the compiled component changes.
#[derive(Clone)]
pub(super) struct KeyframeSource {
    stops: Arc<[component_style::KeyframeStop]>,
    fingerprint: u64,
    properties: TransitionProperties,
}

impl KeyframeSource {
    fn new(stops: Vec<component_style::KeyframeStop>) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut properties = TransitionProperties::none();
        for stop in &stops {
            stop.offset.to_bits().hash(&mut hasher);
            stop.easing.hash(&mut hasher);
            for declaration in &stop.declarations {
                declaration.property.hash(&mut hasher);
                declaration.value.hash(&mut hasher);
            }
            properties = properties.union(declared_properties(&stop.declarations));
        }
        Self {
            stops: stops.into(),
            fingerprint: hasher.finish(),
            properties,
        }
    }
}

/// One instance's stops resolved against its node, reused while every input
/// of that resolution is unchanged.
pub(super) struct CompiledKeyframeRule {
    pub(super) rule: RenderKeyframeRule,
    theme_revision: u64,
    props_fingerprint: u64,
    tag: String,
    input_style: ComputedStyle,
}

impl CompiledKeyframeRule {
    fn matches(&self, node: &WidgetNode, theme_revision: u64, props_fingerprint: u64) -> bool {
        self.theme_revision == theme_revision
            && self.props_fingerprint == props_fingerprint
            && self.tag == node.tag
            && self.input_style == node.computed_style
    }
}

fn declared_properties(declarations: &[component_style::Declaration]) -> TransitionProperties {
    declarations
        .iter()
        .fold(TransitionProperties::none(), |properties, declaration| {
            properties.union(TransitionProperties::of_property(&declaration.property))
        })
}

fn animation_child_container(node: &WidgetNode) -> (f32, f32) {
    (
        (node.layout.width
            - node.computed_style.padding.horizontal()
            - node.computed_style.border_width.horizontal())
        .max(0.0),
        (node.layout.height
            - node.computed_style.padding.vertical()
            - node.computed_style.border_width.vertical())
        .max(0.0),
    )
}

impl FrontendSurfaceComponent {
    /// Whether this frame must sample style animations. A live transition or
    /// keyframe animation always needs sampling. Otherwise the pass exists to
    /// start transitions from restyled values, so a frame whose styles were
    /// not re-resolved (`styles_unchanged`, a paint-only retained frame)
    /// cannot start one and skips it — though any rule declaring a transition,
    /// as the default theme's `node {}` does, sets `has_animatable_style_rules`.
    pub(super) fn should_run_style_animation_pass(&self, styles_unchanged: bool) -> bool {
        !self.transitions.is_empty()
            || !self.keyframe_animations.is_empty()
            || self.has_active_keyframe_animation
            || (self.has_animatable_style_rules && !styles_unchanged)
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

    pub(super) fn lifecycle_style_nodes(&mut self, tree: &WidgetNode) -> HashSet<NodeId> {
        fn descendants(node: &WidgetNode, ids: &mut HashSet<NodeId>) {
            mesh_core_elements::frame_work::visit(
                mesh_core_elements::frame_work::Pass::AnimationLookup,
            );
            ids.insert(node.id);
            for child in &node.children {
                descendants(child, ids);
            }
        }
        let mut current = HashSet::new();
        if self.surface_entering || self.surface_exiting {
            descendants(tree, &mut current);
        } else if let Some(snapshot) = &self.last_frame_snapshot {
            for key in self
                .entering_child_keys
                .iter()
                .chain(&self.closing_child_keys)
            {
                let id = super::runtime_tree::runtime_node_id_for_key(key);
                let Some(path) = snapshot.node_path(id) else {
                    continue;
                };
                let mut node = tree;
                for &index in path {
                    mesh_core_elements::frame_work::visit(
                        mesh_core_elements::frame_work::Pass::AnimationLookup,
                    );
                    let Some(child) = node.children.get(index) else {
                        break;
                    };
                    node = child;
                }
                if node.id == id {
                    descendants(node, &mut current);
                }
            }
        }
        let mut changed = std::mem::replace(&mut self.previous_lifecycle_nodes, current.clone());
        changed.extend(current);
        changed
    }

    pub(super) fn animation_sample_nodes(&self) -> HashSet<NodeId> {
        self.transitions
            .node_ids()
            .chain(self.keyframe_animation_slots.keys().map(|(id, _)| *id))
            .collect()
    }

    pub(super) fn take_previous_visual_styles(&mut self) -> HashMap<NodeId, AnimatableStyle> {
        let mut styles = std::mem::take(&mut self.previous_visual_styles_scratch);
        styles.clear();
        if let Some(last_tree) = self.last_tree.as_ref() {
            collect_visual_styles_with_container(
                last_tree,
                (last_tree.layout.width, last_tree.layout.height),
                &mut styles,
                &|node| self.is_dormant_popover_wrapper(node),
            );
        }
        styles
    }

    pub(super) fn restore_previous_visual_styles(
        &mut self,
        styles: HashMap<NodeId, AnimatableStyle>,
    ) {
        self.previous_visual_styles_scratch = styles;
    }

    /// Sample transitions and keyframe animations into `tree` after restyle
    /// and before layout, so a layout-affecting sample is laid out in the
    /// frame that takes it. `restyled` is this frame's exact restyled set,
    /// when the restyle was scoped. Returns the nodes whose samples changed.
    pub(super) fn run_style_animation_pass(
        &mut self,
        tree: &mut WidgetNode,
        rebuilt: bool,
        restyled: Option<&HashSet<NodeId>>,
        pass: PendingAnimationPass,
        surface_css_props: &SurfaceCssProps,
    ) -> HashSet<NodeId> {
        let scope = if !rebuilt && pass.previous_snapshot.is_some() {
            restyled.map(|restyled| {
                let mut scope = self.animation_sample_nodes();
                scope.extend(restyled);
                scope
            })
        } else {
            None
        };
        let previous_visual_styles = if let Some(previous) = &pass.previous_snapshot {
            if let Some(scope) = &scope {
                let active = self.animation_sample_nodes();
                let previous_ids: HashSet<_> =
                    scope
                        .iter()
                        .copied()
                        .filter(|id| {
                            if active.contains(id) {
                                return true;
                            }
                            let Some(path) = previous.node_path(*id) else {
                                return false;
                            };
                            let mut node = &*tree;
                            for &index in path {
                                mesh_core_elements::frame_work::visit(
                                    mesh_core_elements::frame_work::Pass::AnimationLookup,
                                );
                                node = &node.children[index];
                            }
                            node.computed_style.transitions.iter().any(|transition| {
                                transition.duration_ms > 0 || transition.delay_ms > 0
                            }) || node
                                .computed_style
                                .animations
                                .iter()
                                .any(|animation| animation.name.is_some())
                        })
                        .collect();
                snapshot_visual_styles(previous, &previous_ids)
            } else {
                snapshot_visual_styles(
                    previous,
                    &previous.nodes().iter().map(|node| node.id()).collect(),
                )
            }
        } else {
            pass.previous_visual_styles
        };
        let dirty = self.apply_style_animations_scoped(
            tree,
            &previous_visual_styles,
            surface_css_props,
            scope.as_ref(),
        );
        self.restore_previous_visual_styles(previous_visual_styles);
        dirty
    }

    #[cfg(test)]
    pub(super) fn apply_style_animations_with_previous(
        &mut self,
        tree: &mut WidgetNode,
        previous_styles: &HashMap<NodeId, AnimatableStyle>,
        surface_css_props: &SurfaceCssProps,
    ) -> HashSet<NodeId> {
        self.apply_style_animations_scoped(tree, previous_styles, surface_css_props, None)
    }

    pub(super) fn apply_style_animations_scoped(
        &mut self,
        tree: &mut WidgetNode,
        previous_styles: &HashMap<NodeId, AnimatableStyle>,
        surface_css_props: &SurfaceCssProps,
        scope: Option<&HashSet<NodeId>>,
    ) -> HashSet<NodeId> {
        #[cfg(test)]
        {
            self.animation_nodes_visited = 0;
        }
        let now = Instant::now();
        let mut live_keys = std::mem::take(&mut self.animation_live_keys_scratch);
        live_keys.clear();
        let mut live_keyframe_keys = std::mem::take(&mut self.animation_live_keyframe_keys_scratch);
        live_keyframe_keys.clear();
        let mut dirty_node_ids = std::mem::take(&mut self.animation_dirty_node_ids_scratch);
        dirty_node_ids.clear();
        self.keyframe_animation_lifecycles.clear();
        let mut has_active_animation = false;
        let mut active_animation_bucket = AnimationPropertyBucket::None;
        let mut has_active_keyframe_animation = false;
        let mut active_keyframe_bucket = AnimationPropertyBucket::None;
        let theme = self.active_theme.borrow().clone();
        let resolver = StyleResolver::new(&theme).with_borrowed_props(surface_css_props);

        if let Some(scope) = scope {
            let snapshot = self
                .last_frame_snapshot
                .clone()
                .expect("scoped animation snapshot");
            'sample: for &id in scope {
                let path = snapshot.node_path(id).expect("scoped animation path");
                let mut container = (tree.layout.width, tree.layout.height);
                let mut entering = false;
                let mut node = &mut *tree;
                for &index in path {
                    mesh_core_elements::frame_work::visit(
                        mesh_core_elements::frame_work::Pass::AnimationLookup,
                    );
                    if self.is_dormant_popover_wrapper(node) {
                        continue 'sample;
                    }
                    entering |= node
                        .state
                        .contains(mesh_core_elements::NodeState::SURFACE_ENTERING);
                    container = animation_child_container(node);
                    node = &mut node.children[index];
                }
                assert_eq!(node.id, id, "animation scope requires unchanged structure");
                self.apply_style_animations_to_node(
                    node,
                    container,
                    previous_styles,
                    &resolver,
                    &theme,
                    now,
                    entering,
                    &mut live_keys,
                    &mut live_keyframe_keys,
                    &mut has_active_animation,
                    &mut active_animation_bucket,
                    &mut has_active_keyframe_animation,
                    &mut active_keyframe_bucket,
                    &mut dirty_node_ids,
                    false,
                );
            }
        } else {
            let containing_block = (tree.layout.width, tree.layout.height);
            self.apply_style_animations_to_node(
                tree,
                containing_block,
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
                true,
            );
        }

        self.transitions.retain_live(&live_keys, now);
        // Any instance absent from this frame's declarations is cancelled.
        // Replacement removes the prior slot occupant while the new instance
        // is installed below; retaining by typed identity prevents a stale
        // name-only entry from surviving a style update.
        for (slot, instance_id) in &self.keyframe_animation_slots {
            if !live_keyframe_keys.contains(instance_id) {
                self.keyframe_animation_lifecycles
                    .insert(*slot, AnimationLifecycle::Cancelled);
            }
        }
        self.keyframe_animations
            .retain(|key, _| live_keyframe_keys.contains(key));
        self.keyframe_rules
            .retain(|key, _| live_keyframe_keys.contains(key));
        self.keyframe_animation_slots
            .retain(|_, key| live_keyframe_keys.contains(key));
        if !self.keyframe_base_styles.is_empty() {
            let animated_nodes = self
                .keyframe_animation_slots
                .keys()
                .map(|(node_id, _)| *node_id)
                .collect::<HashSet<_>>();
            self.keyframe_base_styles
                .retain(|node_id, _| animated_nodes.contains(node_id));
        }
        if self.motion_policy.reduced_motion {
            self.keyframe_animations.clear();
            self.keyframe_rules.clear();
            self.keyframe_animation_slots.clear();
        }
        self.has_active_keyframe_animation = has_active_keyframe_animation;
        self.animation_live_keys_scratch = live_keys;
        self.animation_live_keyframe_keys_scratch = live_keyframe_keys;

        self.animation_hold = None;
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
            // A delay or `steps()` plateau repaints identical pixels every
            // vsync; sleep until the earliest sample that can differ.
            match self.animation_next_change(now) {
                NextChange::At(until) if until > now => {
                    self.animation_hold = Some((until, flags));
                }
                _ => self.invalidate_animation_style_path(flags),
            }
        }
        dirty_node_ids
    }

    fn animation_next_change(&self, now: Instant) -> NextChange {
        self.keyframe_animations.iter().fold(
            self.transitions.next_change(now),
            |next, (instance_id, animation)| {
                next.earliest(match self.keyframe_rules.get(instance_id) {
                    Some(compiled) => animation.next_change(&compiled.rule, now),
                    None => NextChange::EveryFrame,
                })
            },
        )
    }

    fn apply_style_animations_to_node(
        &mut self,
        node: &mut WidgetNode,
        containing_block: (f32, f32),
        previous_styles: &HashMap<NodeId, AnimatableStyle>,
        resolver: &StyleResolver,
        theme: &Theme,
        now: Instant,
        ancestor_entering: bool,
        live_keys: &mut HashSet<NodeId>,
        live_keyframe_keys: &mut HashSet<AnimationInstanceId>,
        has_active_animation: &mut bool,
        active_animation_bucket: &mut AnimationPropertyBucket,
        has_active_keyframe_animation: &mut bool,
        active_keyframe_bucket: &mut AnimationPropertyBucket,
        dirty_node_ids: &mut HashSet<NodeId>,
        recurse: bool,
    ) {
        mesh_core_elements::frame_work::visit(
            mesh_core_elements::frame_work::Pass::AnimationSample,
        );
        if self.is_dormant_popover_wrapper(node) {
            return;
        }
        #[cfg(test)]
        {
            self.animation_nodes_visited += 1;
        }
        let entering = ancestor_entering
            || node
                .state
                .contains(mesh_core_elements::NodeState::SURFACE_ENTERING);
        if node.mesh_key().is_some() {
            let node_id = node.id;
            let before_border_radius = node.computed_style.border_radius;
            let before_sampling =
                AnimatableStyle::from_node_in_containing_block(node, containing_block);
            live_keys.insert(node_id);
            let keyframes = self.node_keyframe_animations(node, theme);
            let keyframe_properties = keyframes
                .iter()
                .fold(TransitionProperties::none(), |properties, animation| {
                    properties.union(animation.source.properties)
                });
            if !keyframe_properties.is_empty() {
                self.restore_keyframe_base(node, keyframe_properties);
            }
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
                    containing_block,
                    previous_styles,
                    now,
                    has_active_animation,
                    keyframe_properties,
                )
            };
            let mut animation_is_live = false;
            // Every entry runs on its own timeline, so the node's bucket is the
            // strongest one any unfinished entry needs.
            for transition in self.transitions.unfinished(node_id, now) {
                animation_is_live = true;
                *active_animation_bucket = merge_animation_bucket(
                    *active_animation_bucket,
                    active_transition_bucket(transition.source),
                );
            }
            animation_is_live |= self.apply_node_keyframe_animation(
                node,
                keyframes,
                resolver,
                theme.revision(),
                now,
                live_keyframe_keys,
                has_active_keyframe_animation,
                active_keyframe_bucket,
            );
            if !keyframe_properties.is_empty()
                && let Some(base) = self.keyframe_base_styles.get_mut(&node_id)
            {
                base.applied =
                    AnimatableStyle::from_node_in_containing_block(node, containing_block);
            }
            // Live animations stay dirty even when a delay or a repeated pass
            // holds their sample constant. Completion can write an endpoint
            // after `unfinished` becomes false, so also compare with the input
            // sample and the last painted style. A value sampled at this same
            // `now` alone cannot detect that final change.
            let sampled = AnimatableStyle::from_node_in_containing_block(node, containing_block);
            if animation_is_live || before_sampling != sampled {
                node.mark_style_sampled();
            }
            if animation_is_live
                || before_sampling != sampled
                || before_border_radius != node.computed_style.border_radius
                || previous_styles
                    .get(&node_id)
                    .copied()
                    .or(previous_style)
                    .is_some_and(|previous| previous != sampled)
            {
                dirty_node_ids.insert(node.id);
                let previous = previous_styles
                    .get(&node_id)
                    .copied()
                    .unwrap_or(before_sampling);
                let bucket = previous
                    .differing(&sampled, TransitionProperties::all())
                    .animation_bucket();
                let mut change = mesh_core_elements::ChangeClass::PAINT;
                if bucket == AnimationPropertyBucket::LayoutAffecting {
                    change |= mesh_core_elements::ChangeClass::LAYOUT
                        | mesh_core_elements::ChangeClass::A11Y
                        | mesh_core_elements::ChangeClass::HIT;
                } else if bucket == AnimationPropertyBucket::LayerEffect {
                    change |= mesh_core_elements::ChangeClass::HIT;
                }
                self.frame_dirty.mark(node.id, change);
            }
        }

        if !recurse {
            return;
        }
        let child_container = animation_child_container(node);
        for child in &mut node.children {
            self.apply_style_animations_to_node(
                child,
                child_container,
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
                true,
            );
        }
    }

    fn apply_node_style_animation(
        &mut self,
        key: NodeId,
        node: &mut WidgetNode,
        containing_block: (f32, f32),
        previous_styles: &HashMap<NodeId, AnimatableStyle>,
        now: Instant,
        has_active_animation: &mut bool,
        keyframe_properties: TransitionProperties,
    ) -> Option<AnimatableStyle> {
        // The value shown for this node last frame: the in-flight transition's
        // current value if one exists, otherwise the previous tree snapshot,
        // otherwise the node's own desired style (nothing to animate from).
        let desired = AnimatableStyle::from_node_in_containing_block(node, containing_block);
        let previous_displayed = self
            .transitions
            .displayed_style(key, now, desired)
            .or_else(|| previous_styles.get(&key).copied())
            .unwrap_or(desired);
        // Keyframe animations own the properties they animate: their values
        // never start a transition, and the keyframe pass overwrites them.
        let previous_displayed =
            AnimatableStyle::overlay(previous_displayed, desired, keyframe_properties);

        if self
            .transitions
            .step_node_with_layout(
                key,
                node,
                previous_displayed,
                now,
                self.motion_policy,
                Some(containing_block),
            )
            .active
        {
            *has_active_animation = true;
        }
        Some(previous_displayed)
    }

    /// The named animations declared on `node`, with their stops. An
    /// unresolved name is reported and skipped but keeps its list index.
    fn node_keyframe_animations(
        &mut self,
        node: &WidgetNode,
        theme: &Theme,
    ) -> Vec<NodeKeyframeAnimation> {
        if node.mesh_key().is_none()
            || node
                .computed_style
                .animations
                .iter()
                .all(|animation| animation.name.is_none())
        {
            return Vec::new();
        }
        if self.keyframe_sources_theme_revision != Some(theme.revision()) {
            self.keyframe_sources.clear();
            self.keyframe_sources_theme_revision = Some(theme.revision());
        }
        let mut animations = Vec::new();
        for (list_index, animation) in node
            .computed_style
            .animations
            .iter()
            .filter(|animation| animation.name.is_some())
            .enumerate()
        {
            let Some(name) = animation.name.as_ref() else {
                continue;
            };
            let source_key = format!(
                "{}\0{}\0{name}",
                node.module_id()
                    .unwrap_or(self.compiled.manifest.package.id.as_str()),
                node.source_file().unwrap_or("")
            );
            let source = match self.keyframe_sources.get(&source_key) {
                Some(source) => source.clone(),
                None => {
                    let source = self
                        .find_component_keyframe_rule(node, name)
                        .map(|rule| rule.stops.clone())
                        .or_else(|| theme_keyframe_stops(theme, name))
                        .map(KeyframeSource::new);
                    self.keyframe_sources.insert(source_key, source.clone());
                    source
                }
            };
            let Some(source) = source else {
                self.record_runtime_animation_diagnostic(format!("unresolved animation '{name}'"));
                continue;
            };
            animations.push(NodeKeyframeAnimation {
                list_index: u32::try_from(list_index).unwrap_or(u32::MAX),
                style: animation.clone(),
                name: name.clone(),
                source,
            });
        }
        animations
    }

    /// Put the authored values of `properties` back on `node` before
    /// keyframes sample it. Animation-only frames reuse the previous frame's
    /// computed style, which holds the values the last pass wrote; sampling
    /// from those would drift, and `fill-mode: none` would never restore.
    fn restore_keyframe_base(&mut self, node: &mut WidgetNode, properties: TransitionProperties) {
        let current = AnimatableStyle::from_node(node);
        // A node still showing what the last pass wrote was not restyled, so
        // its authored value is the stored one. Any other value is freshly
        // resolved and becomes the new authored value.
        let authored = match self.keyframe_base_styles.get(&node.id) {
            Some(base) if !current.differs(&base.applied, properties) => {
                AnimatableStyle::overlay(current, base.authored, properties)
            }
            _ => current,
        };
        if authored != current {
            authored.apply_to_node(node);
        }
        self.keyframe_base_styles.insert(
            node.id,
            KeyframeBaseStyle {
                authored,
                applied: authored,
            },
        );
    }

    /// Returns whether this node still has a running, unfinished keyframe
    /// animation, so the caller can keep it dirty for as long as it produces
    /// new values.
    fn apply_node_keyframe_animation(
        &mut self,
        node: &mut WidgetNode,
        animations: Vec<NodeKeyframeAnimation>,
        resolver: &StyleResolver,
        theme_revision: u64,
        now: Instant,
        live_keyframe_keys: &mut HashSet<AnimationInstanceId>,
        has_active_keyframe_animation: &mut bool,
        active_keyframe_bucket: &mut AnimationPropertyBucket,
    ) -> bool {
        let mut node_is_live = false;
        let props_fingerprint = resolver.props_fingerprint();

        for NodeKeyframeAnimation {
            list_index,
            style: animation_style,
            name: animation_name,
            source,
        } in animations
        {
            let declaration_generation =
                Self::animation_declaration_generation(&animation_style, source.fingerprint);
            let instance_id = AnimationInstanceId::new(node.id, list_index, declaration_generation);
            live_keyframe_keys.insert(instance_id);

            let slot = (node.id, list_index);
            let previous_id = self.keyframe_animation_slots.insert(slot, instance_id);
            let lifecycle = match previous_id {
                Some(previous_id) if previous_id == instance_id => AnimationLifecycle::Continued,
                Some(previous_id) => {
                    // A changed timing declaration or keyframe definition is a
                    // replacement in the same slot. Never inherit the old
                    // timeline under a new fingerprint.
                    self.keyframe_animations.remove(&previous_id);
                    self.keyframe_rules.remove(&previous_id);
                    AnimationLifecycle::Replaced
                }
                None => AnimationLifecycle::Started,
            };
            self.keyframe_animation_lifecycles.insert(slot, lifecycle);

            // Resolving stops clones the node's computed style per stop and
            // parses every declaration, so it runs only when an input of that
            // resolution changed, not on every animation frame.
            if !self
                .keyframe_rules
                .get(&instance_id)
                .is_some_and(|compiled| compiled.matches(node, theme_revision, props_fingerprint))
            {
                self.keyframe_rule_compilations = self.keyframe_rule_compilations.saturating_add(1);
                let animation_key = instance_id.registry_key(&animation_name);
                let rule =
                    self.build_render_keyframe_rule(&animation_key, &source.stops, node, resolver);
                self.keyframe_rules.insert(
                    instance_id,
                    CompiledKeyframeRule {
                        rule,
                        theme_revision,
                        props_fingerprint,
                        tag: node.tag.clone(),
                        input_style: node.computed_style.clone(),
                    },
                );
            }
            let render_rule = &self.keyframe_rules[&instance_id].rule;
            // Earlier animations in the list are the underlying value of
            // later ones, so each samples from the node as it stands.
            let base = AnimatableStyle::from_node(node);
            let keyframe_bucket = keyframe_rule_animation_bucket(render_rule, &base);

            if lifecycle != AnimationLifecycle::Continued {
                self.keyframe_animations.remove(&instance_id);
            }
            let active = self
                .keyframe_animations
                .entry(instance_id)
                .or_insert_with(|| ActiveKeyframeAnimation {
                    rule_name: render_rule.name.clone(),
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
            active.duration = self.motion_policy.duration(
                Duration::from_millis(u64::from(animation_style.duration_ms)),
                false,
            );
            active.delay = self.motion_policy.duration(
                Duration::from_millis(u64::from(animation_style.delay_ms)),
                false,
            );
            active.easing = animation_style.easing.into();
            active.iteration_count = animation_style.iteration_count;
            active.direction = animation_style.direction;
            active.fill_mode = animation_style.fill_mode;
            active.set_play_state(animation_style.play_state, now);

            if let Some(current) = active.current_with_rule(render_rule, base, now) {
                current.apply_to_node(node);
            }

            let finished = active.finished(now);
            let running = active.play_state == AnimationPlayState::Running;
            if finished {
                self.keyframe_animation_lifecycles
                    .insert(slot, AnimationLifecycle::Completed);
            }

            if !self.motion_policy.reduced_motion && running && !finished {
                *has_active_keyframe_animation = true;
                node_is_live = true;
                *active_keyframe_bucket =
                    merge_animation_bucket(*active_keyframe_bucket, keyframe_bucket);
            }
        }

        node_is_live
    }

    fn animation_declaration_generation(
        animation_style: &mesh_core_elements::style::AnimationStyle,
        stops_fingerprint: u64,
    ) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // Play state is a control operation on the same instance, not a new
        // declaration. Keep it out of the fingerprint so pause/resume can
        // preserve the timeline while timing, direction, and fill changes
        // still replace the instance explicitly.
        animation_style.name.hash(&mut hasher);
        animation_style.duration_ms.hash(&mut hasher);
        animation_style.delay_ms.hash(&mut hasher);
        animation_style.easing.hash(&mut hasher);
        animation_style.iteration_count.hash(&mut hasher);
        animation_style.direction.hash(&mut hasher);
        animation_style.fill_mode.hash(&mut hasher);
        stops_fingerprint.hash(&mut hasher);
        hasher.finish()
    }

    fn find_component_keyframe_rule(
        &self,
        node: &WidgetNode,
        name: &str,
    ) -> Option<&component_style::KeyframeRule> {
        let module_id = node
            .module_id()
            .unwrap_or(self.compiled.manifest.package.id.as_str());
        let compiled = if module_id == self.compiled.manifest.package.id.as_str() {
            &*self.compiled
        } else {
            &*self.frontend_catalog.modules.get(module_id)?.compiled
        };
        let source = node
            .source_file()
            .map(std::path::Path::new)
            .unwrap_or(&compiled.source_path);
        let component = compiled.component_at_source(source).or_else(|| {
            self.frontend_catalog
                .contribution_entries_for_host(module_id)
                .into_iter()
                .find_map(|entry| entry.component_at_source(source))
        })?;
        component
            .style
            .as_ref()
            .and_then(|style| style.keyframes.iter().find(|rule| rule.name == name))
            .or_else(|| {
                compiled
                    .component
                    .style
                    .as_ref()?
                    .keyframes
                    .iter()
                    .find(|rule| rule.name == name)
            })
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
                easing: stop.easing.map(Into::into),
                properties: declared_properties(&stop.declarations),
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
            .map(|stop| theme_keyframe_stop(theme, stop))
            .collect(),
    )
}

fn theme_keyframe_stop(theme: &Theme, stop: &ThemeKeyframeStop) -> component_style::KeyframeStop {
    component_style::KeyframeStop {
        offset: stop.offset,
        easing: stop
            .easing
            .as_deref()
            .and_then(|value| theme.resolve_token_references(value).ok())
            .and_then(|value| component_style::parse_easing(&value)),
        declarations: stop
            .declarations
            .iter()
            .map(|(property, value)| component_style::Declaration {
                property: property.clone(),
                value: component_style::classify_style_value(value),
            })
            .collect(),
    }
}

#[cfg(test)]
pub(super) fn collect_visual_styles(root: &WidgetNode) -> HashMap<NodeId, AnimatableStyle> {
    visual_styles_for_tree(root)
}

#[cfg(test)]
pub(super) fn visual_styles_for_tree(root: &WidgetNode) -> HashMap<NodeId, AnimatableStyle> {
    let mut styles = HashMap::new();
    collect_visual_styles_with_container(
        root,
        (root.layout.width, root.layout.height),
        &mut styles,
        &|_| false,
    );
    styles
}

pub(super) fn snapshot_visual_styles(
    snapshot: &mesh_core_elements::FrameSnapshot,
    ids: &HashSet<NodeId>,
) -> HashMap<NodeId, AnimatableStyle> {
    ids.iter()
        .filter_map(|id| {
            let node = snapshot.node_by_id(*id)?;
            mesh_core_elements::frame_work::visit(
                mesh_core_elements::frame_work::Pass::AnimationPreviousStyle,
            );
            let container = node
                .parent()
                .and_then(|parent| snapshot.node(parent))
                .map(|parent| {
                    (
                        (parent.layout().width
                            - parent.style().padding.horizontal()
                            - parent.style().border_width.horizontal())
                        .max(0.0),
                        (parent.layout().height
                            - parent.style().padding.vertical()
                            - parent.style().border_width.vertical())
                        .max(0.0),
                    )
                })
                .unwrap_or((node.layout().width, node.layout().height));
            let mut style = AnimatableStyle::from_style_and_layout(node.style(), node.layout());
            style.containing_block = Some(container);
            Some((*id, style))
        })
        .collect()
}

fn collect_visual_styles_with_container(
    node: &WidgetNode,
    containing_block: (f32, f32),
    styles: &mut HashMap<NodeId, AnimatableStyle>,
    skip: &dyn Fn(&WidgetNode) -> bool,
) {
    mesh_core_elements::frame_work::visit(
        mesh_core_elements::frame_work::Pass::AnimationPreviousStyle,
    );
    if skip(node) {
        return;
    }
    if node.mesh_key().is_some() {
        styles.insert(
            node.id,
            AnimatableStyle::from_node_in_containing_block(node, containing_block),
        );
    }
    let child_container = (
        (node.layout.width
            - node.computed_style.padding.horizontal()
            - node.computed_style.border_width.horizontal())
        .max(0.0),
        (node.layout.height
            - node.computed_style.padding.vertical()
            - node.computed_style.border_width.vertical())
        .max(0.0),
    );
    for child in &node.children {
        collect_visual_styles_with_container(child, child_container, styles, skip);
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
}
