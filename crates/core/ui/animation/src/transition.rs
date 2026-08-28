//! Transition controller and shared transition-safe style snapshots.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mesh_core_elements::{
    BoxShadow, Corners, Dimension, Edges, NodeId, TransitionProperties, TransitionStyle,
    VisualFilter, WidgetNode,
    style::{Color, Visibility},
};

use super::easing::{Easing, apply_easing};
use super::instance::{AnimationLifecycle, AnimationStep};
use super::interpolate::Interpolate;
use super::policy::MotionPolicy;

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
    pub min_width: Dimension,
    pub max_width: Dimension,
    pub min_height: Dimension,
    pub max_height: Dimension,
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
            // Visibility is discrete, so the transition must start from the
            // value that was actually displayed. `lerp_visibility` keeps this
            // snapshot visible during an exit until the transition endpoint.
            visibility: previous.visibility,
        }
    }

    /// Layer one entry's interpolated value onto a composed style: take the
    /// properties `props` opts into from `overlay` and everything else,
    /// including the discrete `visibility`, from `base`.
    ///
    /// Entries own disjoint property sets, so composing them in any order
    /// yields the same style. `visibility` belongs to no entry's set and is
    /// resolved separately by the caller.
    pub fn overlay(base: Self, overlay: Self, props: TransitionProperties) -> Self {
        let mut merged = Self::selective_from(overlay, base, props);
        merged.visibility = base.visibility;
        merged
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
            min_width: lerp_dimension(self.min_width, other.min_width, progress),
            max_width: lerp_dimension(self.max_width, other.max_width, progress),
            min_height: lerp_dimension(self.min_height, other.min_height, progress),
            max_height: lerp_dimension(self.max_height, other.max_height, progress),
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
            visibility: lerp_visibility(self.visibility, other.visibility, progress),
        }
    }
}

/// Interpolate CSS's discrete visibility value at its transition edges.
///
/// Visibility becomes visible at the beginning of an entrance, but remains
/// visible until the end of an exit. Other discrete visibility values use the
/// ordinary midpoint switch.
fn lerp_visibility(from: Visibility, to: Visibility, progress: f32) -> Visibility {
    if from == to {
        return from;
    }

    let progress = progress.clamp(0.0, 1.0);
    match (from, to) {
        (_, Visibility::Visible) => Visibility::Visible,
        (Visibility::Visible, _) if progress < 1.0 => Visibility::Visible,
        _ if progress < 0.5 => from,
        _ => to,
    }
}

/// One in-flight transition instance: a single entry of one node's
/// comma-separated `transition` list.
///
/// Entries carry independent timing, so each gets its own instance rather than
/// the node sharing one.
#[derive(Debug, Clone)]
pub struct ActiveTransition {
    pub from: AnimatableStyle,
    pub to: AnimatableStyle,
    pub started_at: Instant,
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
    /// The authored entry, narrowed to the properties no later entry claims.
    pub source: TransitionStyle,
    /// Position of this entry in the node's authored transition list.
    pub entry: usize,
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

    /// The instant this instance reaches its endpoint.
    pub fn ends_at(&self) -> Instant {
        self.started_at + self.delay + self.duration
    }

    /// How reconciling this instance against a removed or idle declaration is
    /// classified: reaching the endpoint completes, anything earlier cancels.
    fn ended_lifecycle(&self, now: Instant) -> AnimationLifecycle {
        if self.finished(now) {
            AnimationLifecycle::Completed
        } else {
            AnimationLifecycle::Cancelled
        }
    }
}

/// Rank used to summarise several per-entry lifecycles as one node-level
/// decision. A declaration that started or changed target outranks one that
/// merely continued, which outranks one that ended.
fn lifecycle_rank(lifecycle: AnimationLifecycle) -> u8 {
    match lifecycle {
        AnimationLifecycle::Idle => 0,
        AnimationLifecycle::Cancelled => 1,
        AnimationLifecycle::Completed => 2,
        AnimationLifecycle::Continued => 3,
        AnimationLifecycle::Replaced => 4,
        AnimationLifecycle::Reversed => 5,
        AnimationLifecycle::Started => 6,
    }
}

fn merge_lifecycle(current: AnimationLifecycle, next: AnimationLifecycle) -> AnimationLifecycle {
    if lifecycle_rank(next) > lifecycle_rank(current) {
        next
    } else {
        current
    }
}

/// Every property claimed by an entry after `index`.
///
/// CSS gives the last declaration of a property the win, so an earlier entry
/// only runs what nothing after it names. Transition lists hold a handful of
/// entries, so the quadratic scan stays cheaper than building a mask table.
fn claimed_after(entries: &[TransitionStyle], index: usize) -> TransitionProperties {
    entries[index + 1..]
        .iter()
        .fold(TransitionProperties::none(), |claimed, later| {
            claimed.union(later.properties)
        })
}

/// Per-component transition controller.
///
/// Owns the in-flight transitions keyed by retained widget identity
/// (`NodeId`). Each node holds one instance per animating entry of its
/// `transition` list, because entries carry independent duration, delay, and
/// easing. Callers that drive transitions alongside other concerns
/// (keyframes, theme restyle, dirty tracking) step nodes individually with
/// [`TransitionAnimator::step_node`]; callers that only need transitions can
/// walk a whole tree with [`TransitionAnimator::apply`].
///
/// A node is present in the map only while at least one of its entries is in
/// flight, so `is_empty` stays an honest "nothing is animating".
#[derive(Debug, Default)]
pub struct TransitionAnimator {
    active: HashMap<NodeId, Vec<ActiveTransition>>,
}

impl TransitionAnimator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub fn contains_key(&self, key: NodeId) -> bool {
        self.active.contains_key(&key)
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }

    pub fn remove(&mut self, key: NodeId) {
        self.active.remove(&key);
    }

    /// Style currently displayed for `key`: every in-flight entry's own
    /// properties layered over `base`, which supplies the node's desired value
    /// for everything no entry animates.
    pub fn displayed_style(
        &self,
        key: NodeId,
        now: Instant,
        base: AnimatableStyle,
    ) -> Option<AnimatableStyle> {
        let instances = self.active.get(&key)?;
        Some(compose(instances, base, now))
    }

    /// The unfinished transition instances for `key` — used to classify the
    /// active animation property bucket across every entry.
    pub fn unfinished(
        &self,
        key: NodeId,
        now: Instant,
    ) -> impl Iterator<Item = &ActiveTransition> + '_ {
        self.active
            .get(&key)
            .into_iter()
            .flatten()
            .filter(move |instance| !instance.finished(now))
    }

    /// Drop transitions whose key left the live set or that have finished.
    pub fn retain_live(&mut self, live: &HashSet<NodeId>, now: Instant) {
        self.active.retain(|key, instances| {
            if !live.contains(key) {
                return false;
            }
            instances.retain(|instance| !instance.finished(now));
            !instances.is_empty()
        });
    }

    pub fn has_active(&self, now: Instant) -> bool {
        self.active
            .values()
            .flatten()
            .any(|instance| !instance.finished(now))
    }

    /// Step a single keyed node toward the target described by its own
    /// `computed_style.transition`. `previous_displayed` is the value shown for
    /// this node last frame. Mutates `node`'s computed style to the current
    /// interpolated value and returns `true` if a transition is still active.
    pub fn step_node(
        &mut self,
        key: NodeId,
        node: &mut WidgetNode,
        previous_displayed: AnimatableStyle,
        now: Instant,
    ) -> bool {
        self.step_node_with_policy_state(
            key,
            node,
            previous_displayed,
            now,
            MotionPolicy::default(),
        )
        .active
    }

    /// Step a node with the caller's immutable motion snapshot. Non-essential
    /// transitions are completed immediately when reduced motion is enabled.
    pub fn step_node_with_policy(
        &mut self,
        key: NodeId,
        node: &mut WidgetNode,
        previous_displayed: AnimatableStyle,
        now: Instant,
        policy: MotionPolicy,
    ) -> bool {
        self.step_node_with_policy_state(key, node, previous_displayed, now, policy)
            .active
    }

    /// Step every entry of a node's `transition` list and report the lifecycle
    /// decision summarising them.
    ///
    /// Each entry is reconciled against its own instance from the previous
    /// frame, so a short colour fade and a long transform ease run on
    /// independent timelines instead of the node sharing one. A target that
    /// changes while an entry is running starts from the currently displayed
    /// value. If the new target is that entry's previous source, it is
    /// classified as a reversal; unrelated targets are replacements. Removing
    /// an entry before completion is a cancellation, while reaching its
    /// endpoint is completion.
    pub fn step_node_with_policy_state(
        &mut self,
        key: NodeId,
        node: &mut WidgetNode,
        previous_displayed: AnimatableStyle,
        now: Instant,
        policy: MotionPolicy,
    ) -> AnimationStep {
        let desired = AnimatableStyle::from_node(node);
        let entry_count = node.computed_style.transitions.len();

        // The clamped visual radius is authoritative whether or not the radius
        // itself animates, so push it onto the node before any interpolation.
        if node
            .computed_style
            .transitions
            .iter()
            .any(|entry| entry.properties.animates_border_radius())
        {
            node.computed_style.border_radius = desired.border_radius;
        }

        // Reusing last frame's vector keeps continued entries allocation-free.
        let mut running = self.active.remove(&key).unwrap_or_default();
        let mut lifecycle = AnimationLifecycle::Idle;

        for index in 0..entry_count {
            let mut entry = node.computed_style.transitions[index];
            entry.properties = entry
                .properties
                .difference(claimed_after(&node.computed_style.transitions, index));

            let slot = running.iter().position(|instance| instance.entry == index);
            let duration =
                policy.duration(Duration::from_millis(u64::from(entry.duration_ms)), false);
            let should_animate = !entry.properties.is_empty()
                && !duration.is_zero()
                && previous_displayed.differs(&desired, entry.properties);

            if !should_animate {
                if let Some(slot) = slot {
                    lifecycle = merge_lifecycle(lifecycle, running[slot].ended_lifecycle(now));
                    running.swap_remove(slot);
                }
                continue;
            }

            let entry_lifecycle = match slot.map(|slot| &running[slot]) {
                Some(active)
                    if !active.finished(now)
                        && target_matches(active.to, desired, entry.properties)
                        && active.source == entry =>
                {
                    AnimationLifecycle::Continued
                }
                Some(active) if target_matches(active.from, desired, entry.properties) => {
                    AnimationLifecycle::Reversed
                }
                Some(_) => AnimationLifecycle::Replaced,
                None => AnimationLifecycle::Started,
            };
            lifecycle = merge_lifecycle(lifecycle, entry_lifecycle);

            if entry_lifecycle == AnimationLifecycle::Continued {
                continue;
            }

            let instance = ActiveTransition {
                from: AnimatableStyle::selective_from(
                    previous_displayed,
                    desired,
                    entry.properties,
                ),
                to: desired,
                started_at: now,
                duration,
                delay: Duration::from_millis(u64::from(entry.delay_ms)),
                easing: entry.easing.into(),
                source: entry,
                entry: index,
            };
            match slot {
                Some(slot) => running[slot] = instance,
                None => running.push(instance),
            }
        }

        // Instances whose entry left the declaration entirely.
        running.retain(|instance| {
            if instance.entry < entry_count {
                return true;
            }
            lifecycle = merge_lifecycle(lifecycle, instance.ended_lifecycle(now));
            false
        });

        if running.is_empty() {
            return AnimationStep {
                lifecycle,
                active: false,
            };
        }

        compose(&running, desired, now).apply_to_node(node);

        let before = running.len();
        running.retain(|instance| !instance.finished(now));
        if running.is_empty() {
            return AnimationStep {
                lifecycle: AnimationLifecycle::Completed,
                active: false,
            };
        }
        if running.len() != before {
            lifecycle = merge_lifecycle(lifecycle, AnimationLifecycle::Completed);
        }

        self.active.insert(key, running);
        AnimationStep {
            lifecycle,
            active: true,
        }
    }

    /// Walk a widget tree and step the transition for every runtime-keyed node
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
        live: &mut HashSet<NodeId>,
    ) -> bool {
        let mut active = false;
        if node.mesh_key().is_some() {
            let key = node.id;
            live.insert(key);
            let desired = AnimatableStyle::from_node(node);
            let previous = self.displayed_style(key, now, desired).unwrap_or(desired);
            active |= self.step_node(key, node, previous, now);
        }
        for child in &mut node.children {
            active |= self.apply_node(child, now, live);
        }
        active
    }
}

/// Layer every in-flight entry's own properties onto `base`.
///
/// Entries own disjoint property sets, so this is order-independent for
/// everything except `visibility`, which belongs to no set. The entry that
/// finishes last carries it, so a node stays visible until its slowest exit
/// transition is done.
fn compose(instances: &[ActiveTransition], base: AnimatableStyle, now: Instant) -> AnimatableStyle {
    let mut composed = base;
    let mut visibility = None;
    let mut latest_end: Option<Instant> = None;

    for instance in instances {
        let current = instance.current(now);
        composed = AnimatableStyle::overlay(composed, current, instance.source.properties);
        let ends_at = instance.ends_at();
        if latest_end.is_none_or(|end| ends_at > end) {
            latest_end = Some(ends_at);
            visibility = Some(current.visibility);
        }
    }

    if let Some(visibility) = visibility {
        composed.visibility = visibility;
    }
    composed
}

/// True if `candidate` already holds `desired` for the properties one entry
/// animates. `visibility` is compared too: it rides along with whichever
/// entries are in flight instead of belonging to a property set, so a change
/// to it re-targets every entry rather than being silently dropped.
fn target_matches(
    candidate: AnimatableStyle,
    desired: AnimatableStyle,
    props: TransitionProperties,
) -> bool {
    !candidate.differs(&desired, props) && candidate.visibility == desired.visibility
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
        node.computed_style.transitions = vec![transition];
        node.computed_style.opacity = 1.0;

        // Previously displayed at 0.0; target is 1.0 -> transition starts.
        let start = Instant::now();
        let previous = AnimatableStyle {
            opacity: 0.0,
            ..AnimatableStyle::from_node(&node)
        };
        let key = node.id;
        let active = animator.step_node(key, &mut node, previous, start);
        assert!(active);
        assert!(node.computed_style.opacity < 1.0);
        assert!(animator.contains_key(key));

        // A fresh tree rebuild re-asserts the desired target (1.0) each frame.
        node.computed_style.opacity = 1.0;
        let done = start + Duration::from_millis(150);
        let displayed = animator
            .displayed_style(key, done, AnimatableStyle::from_node(&node))
            .expect("in flight");
        let still_active = animator.step_node(key, &mut node, displayed, done);
        assert!(!still_active);
        assert!((node.computed_style.opacity - 1.0).abs() < 1e-4);
    }

    #[test]
    fn transition_reversal_starts_from_the_current_displayed_value() {
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
        node.computed_style.transitions = vec![transition];
        node.computed_style.opacity = 1.0;
        let previous = AnimatableStyle {
            opacity: 0.0,
            ..AnimatableStyle::from_node(&node)
        };
        let start = Instant::now();
        let key = node.id;

        assert_eq!(
            animator
                .step_node_with_policy_state(
                    key,
                    &mut node,
                    previous,
                    start,
                    MotionPolicy::default(),
                )
                .lifecycle,
            AnimationLifecycle::Started
        );

        let halfway = start + Duration::from_millis(50);
        let displayed = animator
            .displayed_style(key, halfway, AnimatableStyle::from_node(&node))
            .expect("active transition");
        node.computed_style.opacity = 0.0;
        let step = animator.step_node_with_policy_state(
            key,
            &mut node,
            displayed,
            halfway,
            MotionPolicy::default(),
        );

        assert_eq!(step.lifecycle, AnimationLifecycle::Reversed);
        assert!(step.active);
        assert!((node.computed_style.opacity - displayed.opacity).abs() < 0.01);
    }

    #[test]
    fn transition_replacement_and_cancellation_are_explicit() {
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
        node.computed_style.transitions = vec![transition];
        node.computed_style.opacity = 1.0;
        let previous = AnimatableStyle {
            opacity: 0.0,
            ..AnimatableStyle::from_node(&node)
        };
        let start = Instant::now();
        let key = node.id;
        assert_eq!(
            animator
                .step_node_with_policy_state(
                    key,
                    &mut node,
                    previous,
                    start,
                    MotionPolicy::default(),
                )
                .lifecycle,
            AnimationLifecycle::Started
        );

        let replacement_time = start + Duration::from_millis(20);
        let displayed = animator
            .displayed_style(key, replacement_time, AnimatableStyle::from_node(&node))
            .expect("active transition");
        node.computed_style.opacity = 0.5;
        let replaced = animator.step_node_with_policy_state(
            key,
            &mut node,
            displayed,
            replacement_time,
            MotionPolicy::default(),
        );
        assert_eq!(replaced.lifecycle, AnimationLifecycle::Replaced);
        assert!(replaced.active);

        node.computed_style.transitions.clear();
        let cancelled = animator.step_node_with_policy_state(
            key,
            &mut node,
            displayed,
            replacement_time + Duration::from_millis(10),
            MotionPolicy::default(),
        );
        assert_eq!(cancelled.lifecycle, AnimationLifecycle::Cancelled);
        assert!(!cancelled.active);
        assert!(!animator.contains_key(key));
    }

    fn entry(
        duration_ms: u32,
        properties: mesh_core_elements::TransitionProperties,
    ) -> TransitionStyle {
        TransitionStyle {
            duration_ms,
            properties,
            ..TransitionStyle::default()
        }
    }

    fn opacity_and_transform_node() -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.computed_style.transitions = vec![
            entry(
                100,
                mesh_core_elements::TransitionProperties {
                    opacity: true,
                    ..mesh_core_elements::TransitionProperties::none()
                },
            ),
            entry(
                400,
                mesh_core_elements::TransitionProperties {
                    transform: true,
                    ..mesh_core_elements::TransitionProperties::none()
                },
            ),
        ];
        node.computed_style.opacity = 1.0;
        node.computed_style.transform = Transform2D {
            translate_x: 100.0,
            ..Transform2D::IDENTITY
        };
        node
    }

    #[test]
    fn every_transition_entry_runs_on_its_own_timeline() {
        let mut animator = TransitionAnimator::new();
        let mut node = opacity_and_transform_node();
        let key = node.id;
        let desired = AnimatableStyle::from_node(&node);
        let previous = AnimatableStyle {
            opacity: 0.0,
            transform: Transform2D::IDENTITY,
            ..desired
        };

        let start = Instant::now();
        assert!(animator.step_node(key, &mut node, previous, start));

        // Halfway through the short entry and an eighth into the long one, both
        // properties must be moving. Reading only the first entry left the
        // transform pinned at its target from the very first frame.
        node.computed_style = opacity_and_transform_node().computed_style;
        let halfway = start + Duration::from_millis(50);
        let displayed = animator
            .displayed_style(key, halfway, AnimatableStyle::from_node(&node))
            .expect("in flight");
        assert!(displayed.opacity > 0.0 && displayed.opacity < 1.0);
        assert!(displayed.transform.translate_x > 0.0);
        assert!(displayed.transform.translate_x < 100.0);
        assert!(displayed.transform.translate_x < displayed.opacity * 100.0);
        assert!(animator.step_node(key, &mut node, displayed, halfway));

        // The short entry completes on its own schedule while the long one keeps
        // running.
        node.computed_style = opacity_and_transform_node().computed_style;
        let short_done = start + Duration::from_millis(100);
        let displayed = animator
            .displayed_style(key, short_done, AnimatableStyle::from_node(&node))
            .expect("in flight");
        assert!((displayed.opacity - 1.0).abs() < 1e-4);
        assert!(displayed.transform.translate_x < 100.0);
        assert!(animator.step_node(key, &mut node, displayed, short_done));
        assert_eq!(animator.unfinished(key, short_done).count(), 1);

        // The long entry reaches its own endpoint 300ms later.
        node.computed_style = opacity_and_transform_node().computed_style;
        let long_done = start + Duration::from_millis(400);
        let displayed = animator
            .displayed_style(key, long_done, AnimatableStyle::from_node(&node))
            .expect("in flight");
        assert!((displayed.transform.translate_x - 100.0).abs() < 1e-3);
        assert!(!animator.step_node(key, &mut node, displayed, long_done));
        assert!(!animator.contains_key(key));
    }

    #[test]
    fn a_later_entry_wins_a_property_an_earlier_one_also_names() {
        let colour = mesh_core_elements::TransitionProperties {
            color: true,
            ..mesh_core_elements::TransitionProperties::none()
        };
        let mut animator = TransitionAnimator::new();
        let mut node = WidgetNode::new("box");
        node.computed_style.transitions = vec![entry(400, colour), entry(100, colour)];
        node.computed_style.color = Color::WHITE;

        let desired = AnimatableStyle::from_node(&node);
        let previous = AnimatableStyle {
            color: Color::TRANSPARENT,
            ..desired
        };
        let key = node.id;
        let start = Instant::now();
        assert!(animator.step_node(key, &mut node, previous, start));
        // Only the winning entry is in flight; the shadowed one runs nothing.
        assert_eq!(animator.unfinished(key, start).count(), 1);

        node.computed_style.color = Color::WHITE;
        let done = start + Duration::from_millis(100);
        let displayed = animator
            .displayed_style(key, done, AnimatableStyle::from_node(&node))
            .expect("in flight");
        assert_eq!(displayed.color, Color::WHITE);
        assert!(!animator.step_node(key, &mut node, displayed, done));
    }

    #[test]
    fn retargeting_one_entry_leaves_another_entrys_timeline_alone() {
        let mut animator = TransitionAnimator::new();
        let mut node = opacity_and_transform_node();
        let key = node.id;
        let desired = AnimatableStyle::from_node(&node);
        let previous = AnimatableStyle {
            opacity: 0.0,
            transform: Transform2D::IDENTITY,
            ..desired
        };

        let start = Instant::now();
        assert!(animator.step_node(key, &mut node, previous, start));

        // A new opacity target mid-flight retargets only the opacity entry.
        node.computed_style = opacity_and_transform_node().computed_style;
        node.computed_style.opacity = 0.25;
        let halfway = start + Duration::from_millis(50);
        let displayed = animator
            .displayed_style(key, halfway, AnimatableStyle::from_node(&node))
            .expect("in flight");
        let step = animator.step_node_with_policy_state(
            key,
            &mut node,
            displayed,
            halfway,
            MotionPolicy::default(),
        );
        assert_eq!(step.lifecycle, AnimationLifecycle::Replaced);

        let transform_entry = animator
            .unfinished(key, halfway)
            .find(|instance| instance.entry == 1)
            .expect("transform entry still running");
        assert_eq!(transform_entry.started_at, start);
        assert_eq!(transform_entry.to.transform.translate_x, 100.0);
    }

    #[test]
    fn visibility_transition_preserves_previous_value_until_completion() {
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
        node.computed_style.transitions = vec![transition];
        node.computed_style.opacity = 1.0;
        node.computed_style.visibility = Visibility::Hidden;

        let previous = AnimatableStyle {
            opacity: 0.0,
            visibility: Visibility::Visible,
            ..AnimatableStyle::from_node(&node)
        };
        let start = Instant::now();
        let key = node.id;

        assert!(animator.step_node(key, &mut node, previous, start));
        assert_eq!(node.computed_style.visibility, Visibility::Visible);

        node.computed_style.opacity = 1.0;
        node.computed_style.visibility = Visibility::Hidden;
        let halfway = start + Duration::from_millis(50);
        let displayed = animator
            .displayed_style(key, halfway, AnimatableStyle::from_node(&node))
            .expect("active transition");
        assert!(animator.step_node(key, &mut node, displayed, halfway));
        assert_eq!(node.computed_style.visibility, Visibility::Visible);

        node.computed_style.opacity = 1.0;
        node.computed_style.visibility = Visibility::Hidden;
        let done = start + Duration::from_millis(100);
        let displayed = animator
            .displayed_style(key, done, AnimatableStyle::from_node(&node))
            .expect("active transition");
        assert!(!animator.step_node(key, &mut node, displayed, done));
        assert_eq!(node.computed_style.visibility, Visibility::Hidden);
    }

    #[test]
    fn visibility_interpolation_enters_at_start_and_exits_at_end() {
        let mut visible = AnimatableStyle::from_node(&WidgetNode::new("box"));
        visible.visibility = Visibility::Visible;
        let mut hidden = visible;
        hidden.visibility = Visibility::Hidden;

        assert_eq!(hidden.lerp(visible, 0.0).visibility, Visibility::Visible);
        assert_eq!(visible.lerp(hidden, 0.5).visibility, Visibility::Visible);
        assert_eq!(visible.lerp(hidden, 1.0).visibility, Visibility::Hidden);
    }

    #[test]
    fn reduced_motion_completes_nonessential_transition_immediately() {
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
        node.computed_style.transitions = vec![transition];
        node.computed_style.opacity = 1.0;
        let previous = AnimatableStyle {
            opacity: 0.0,
            ..AnimatableStyle::from_node(&node)
        };

        let active = animator.step_node_with_policy(
            node.id,
            &mut node,
            previous,
            Instant::now(),
            MotionPolicy::new(true),
        );

        assert!(!active);
        assert_eq!(node.computed_style.opacity, 1.0);
        assert!(!animator.contains_key(node.id));
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
            min_width: Dimension::Auto,
            max_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_height: Dimension::Auto,
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
