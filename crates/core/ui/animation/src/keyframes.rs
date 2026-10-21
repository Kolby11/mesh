//! `@keyframes` playback over validated percentage stops.
//!
//! Parsing and validation live in `mesh-core-component`. The renderer consumes
//! already-lowered keyframe rules and produces the in-flight animatable style
//! for a given timestamp.

use std::collections::HashMap;
use std::iter;
use std::time::{Duration, Instant};

use mesh_core_elements::TransitionProperties;
use mesh_core_elements::style::{
    AnimationDirection, AnimationFillMode, AnimationIterationCount, AnimationPlayState,
};

use super::easing::{Easing, apply_easing, step_count};
use super::instance::NextChange;
use super::transition::AnimatableStyle;
use crate::Interpolate;

/// A single `<percent>% { ... }` block from a validated `@keyframes` rule.
#[derive(Debug, Clone)]
pub struct KeyframeStop {
    /// Position in the timeline, normalized to `[0.0, 1.0]`.
    pub offset: f32,
    /// Style snapshot at this stop. Only `properties` are read from it.
    pub style: AnimatableStyle,
    /// The properties this stop declares.
    pub properties: TransitionProperties,
    /// Segment-local easing override starting at this stop.
    pub easing: Option<Easing>,
}

/// One named `@keyframes` rule, sorted by `offset`.
#[derive(Debug, Clone)]
pub struct KeyframeRule {
    pub name: String,
    pub stops: Vec<KeyframeStop>,
}

impl KeyframeRule {
    /// Every property some stop declares.
    pub fn animated_properties(&self) -> TransitionProperties {
        self.stops
            .iter()
            .fold(TransitionProperties::none(), |properties, stop| {
                properties.union(stop.properties)
            })
    }
}

/// Lookup table shared across nodes that reference an animation name.
#[derive(Debug, Default)]
pub struct KeyframeRegistry {
    rules: HashMap<String, KeyframeRule>,
}

impl KeyframeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, rule: KeyframeRule) {
        self.rules.insert(rule.name.clone(), rule);
    }

    pub fn get(&self, name: &str) -> Option<&KeyframeRule> {
        self.rules.get(name)
    }
}

/// One in-flight keyframe animation attached to a single widget node.
#[derive(Debug, Clone)]
pub struct ActiveKeyframeAnimation {
    pub rule_name: String,
    pub started_at: Instant,
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub play_state: AnimationPlayState,
    pub paused_at: Option<Instant>,
}

impl ActiveKeyframeAnimation {
    /// Change playback state without changing the animation's active progress.
    ///
    /// A paused animation keeps its original timeline origin so that iteration
    /// and direction calculations remain stable. Resuming moves that origin
    /// forward by the time spent paused instead of counting the pause as active
    /// playback time.
    pub fn set_play_state(&mut self, play_state: AnimationPlayState, now: Instant) {
        match (self.play_state, play_state) {
            (AnimationPlayState::Running, AnimationPlayState::Paused) => {
                self.paused_at = Some(now);
            }
            (AnimationPlayState::Paused, AnimationPlayState::Running) => {
                if let Some(paused_at) = self.paused_at {
                    let paused_duration = now.saturating_duration_since(paused_at);
                    self.started_at = self
                        .started_at
                        .checked_add(paused_duration)
                        .unwrap_or(self.started_at);
                }
                self.paused_at = None;
            }
            (AnimationPlayState::Running, AnimationPlayState::Running) => {
                self.paused_at = None;
            }
            (AnimationPlayState::Paused, AnimationPlayState::Paused) => {}
        }
        self.play_state = play_state;
    }

    /// Resolve the displayed style at `now`.
    pub fn current(
        &self,
        registry: &KeyframeRegistry,
        base: AnimatableStyle,
        now: Instant,
    ) -> Option<AnimatableStyle> {
        self.current_with_rule(registry.get(&self.rule_name)?, base, now)
    }

    /// [`Self::current`] for a caller that already holds this animation's
    /// rule, without a registry lookup by `rule_name`.
    pub fn current_with_rule(
        &self,
        rule: &KeyframeRule,
        base: AnimatableStyle,
        now: Instant,
    ) -> Option<AnimatableStyle> {
        if rule.stops.is_empty() {
            return None;
        }

        let effective_now = self.effective_now(now);
        let elapsed = effective_now.saturating_duration_since(self.started_at);

        if elapsed < self.delay {
            return self.pre_delay_style(rule, base);
        }

        let active_elapsed = elapsed - self.delay;
        let Some(total_active_duration) = self.total_active_duration() else {
            return Some(self.sample_rule(rule, base, self.directed_progress(active_elapsed)));
        };

        if active_elapsed >= total_active_duration {
            return self.post_completion_style(rule, base, total_active_duration);
        }

        Some(self.sample_rule(rule, base, self.directed_progress(active_elapsed)))
    }

    pub fn finished(&self, now: Instant) -> bool {
        let Some(total_active_duration) = self.total_active_duration() else {
            return false;
        };

        let elapsed = self
            .effective_now(now)
            .saturating_duration_since(self.started_at);
        elapsed >= self.delay + total_active_duration
    }

    /// When [`Self::current_with_rule`] can next return a different value.
    /// Only animations whose every segment eases with `steps()` hold between
    /// frames; the result may be early, never late.
    pub fn next_change(&self, rule: &KeyframeRule, now: Instant) -> NextChange {
        /// Above this many intervals a step plateau is shorter than a frame
        /// at any practical duration, so the scan is not worth it.
        const MAX_STEPS: u32 = 120;

        if self.play_state == AnimationPlayState::Paused || self.finished(now) {
            return NextChange::Never;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed < self.delay {
            return NextChange::At(self.started_at + self.delay);
        }
        if self.duration.is_zero() || rule.stops.is_empty() {
            return NextChange::EveryFrame;
        }
        let mut counts = Vec::new();
        for easing in iter::once(self.easing).chain(rule.stops.iter().filter_map(|s| s.easing)) {
            match step_count(easing) {
                Some(count) if count <= MAX_STEPS => counts.push(count),
                _ => return NextChange::EveryFrame,
            }
        }
        counts.sort_unstable();
        counts.dedup();

        // Every step boundary of every segment a property could span, in
        // directed progress. Segments run between any two stop offsets.
        let mut offsets = vec![0.0f64, 1.0];
        offsets.extend(rule.stops.iter().map(|stop| f64::from(stop.offset)));
        offsets.sort_by(f64::total_cmp);
        offsets.dedup();
        let mut boundaries = Vec::new();
        for (index, &from) in offsets.iter().enumerate() {
            for &to in &offsets[index + 1..] {
                for &count in &counts {
                    for step in 0..=count {
                        let at = from + (to - from) * f64::from(step) / f64::from(count);
                        boundaries.push(at);
                        boundaries.push(1.0 - at);
                    }
                }
            }
        }

        let active = elapsed - self.delay;
        let duration = self.duration.as_secs_f64();
        let iteration = (active.as_secs_f64() / duration).floor();
        let raw_now = active.as_secs_f64() / duration - iteration;
        let next_raw = boundaries
            .into_iter()
            .filter(|&raw| raw > raw_now + 1e-9)
            .fold(1.0f64, f64::min);
        let next = self.started_at + self.delay + self.duration.mul_f64(iteration + next_raw);
        match self.total_active_duration() {
            Some(total) => NextChange::At(next.min(self.started_at + self.delay + total)),
            None => NextChange::At(next),
        }
    }

    fn effective_now(&self, now: Instant) -> Instant {
        if self.play_state == AnimationPlayState::Paused {
            self.paused_at.unwrap_or(now)
        } else {
            now
        }
    }

    fn pre_delay_style(
        &self,
        rule: &KeyframeRule,
        base: AnimatableStyle,
    ) -> Option<AnimatableStyle> {
        match self.fill_mode {
            AnimationFillMode::Backwards | AnimationFillMode::Both => {
                Some(self.sample_rule(rule, base, self.direct_progress(0.0, 0)))
            }
            AnimationFillMode::None | AnimationFillMode::Forwards => None,
        }
    }

    fn post_completion_style(
        &self,
        rule: &KeyframeRule,
        base: AnimatableStyle,
        _total_active_duration: Duration,
    ) -> Option<AnimatableStyle> {
        match self.fill_mode {
            AnimationFillMode::Forwards | AnimationFillMode::Both => {
                Some(self.sample_rule(rule, base, self.final_progress()))
            }
            AnimationFillMode::None | AnimationFillMode::Backwards => None,
        }
    }

    fn total_active_duration(&self) -> Option<Duration> {
        match self.iteration_count {
            AnimationIterationCount::Infinite => None,
            AnimationIterationCount::Number(count) => {
                Some(self.duration.checked_mul(count).unwrap_or(Duration::MAX))
            }
        }
    }

    fn directed_progress(&self, active_elapsed: Duration) -> f32 {
        if self.duration.is_zero() {
            return self.direct_progress(1.0, 0);
        }

        let duration_nanos = self.duration.as_nanos();
        let iteration_index =
            (active_elapsed.as_nanos() / duration_nanos).min(u128::from(u32::MAX)) as u32;
        let iteration_elapsed_nanos = active_elapsed.as_nanos() % duration_nanos;
        let progress = iteration_elapsed_nanos as f64 / duration_nanos as f64;
        self.direct_progress((progress.clamp(0.0, 1.0)) as f32, iteration_index)
    }

    fn final_progress(&self) -> f32 {
        let last_iteration = match self.iteration_count {
            AnimationIterationCount::Infinite => 0,
            AnimationIterationCount::Number(count) => count.saturating_sub(1),
        };
        self.direct_progress(1.0, last_iteration)
    }

    fn direct_progress(&self, progress: f32, iteration_index: u32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self.direction {
            AnimationDirection::Normal => progress,
            AnimationDirection::Reverse => 1.0 - progress,
            AnimationDirection::Alternate => {
                if iteration_index % 2 == 0 {
                    progress
                } else {
                    1.0 - progress
                }
            }
            AnimationDirection::AlternateReverse => {
                if iteration_index % 2 == 0 {
                    1.0 - progress
                } else {
                    progress
                }
            }
        }
    }

    /// Sample every animated property at `progress`. Each property
    /// interpolates between the nearest stops that declare it; with no
    /// declaring stop at or before `progress` it starts from `base` (the
    /// implicit 0% keyframe), and with none after it ends at `base` (the
    /// implicit 100% keyframe). Properties no stop declares keep `base`.
    fn sample_rule(
        &self,
        rule: &KeyframeRule,
        base: AnimatableStyle,
        progress: f32,
    ) -> AnimatableStyle {
        let progress = progress.clamp(0.0, 1.0);
        let mut sampled = base;
        let mut unresolved = rule.animated_properties();
        let stops_before = rule
            .stops
            .iter()
            .rev()
            .filter(|stop| stop.offset <= progress)
            .map(Some)
            .chain(iter::once(None));
        for from in stops_before {
            if unresolved.is_empty() {
                break;
            }
            let (from_offset, from_style, from_easing, mut pending) = match from {
                Some(stop) => (
                    stop.offset,
                    stop.style,
                    stop.easing,
                    unresolved.intersection(stop.properties),
                ),
                None => (0.0, base, None, unresolved),
            };
            if pending.is_empty() {
                continue;
            }
            unresolved = unresolved.difference(pending);
            let easing = from_easing.unwrap_or(self.easing);
            let stops_after = rule
                .stops
                .iter()
                .filter(|stop| stop.offset > progress)
                .map(Some)
                .chain(iter::once(None));
            for to in stops_after {
                if pending.is_empty() {
                    break;
                }
                let (to_offset, to_style, properties) = match to {
                    Some(stop) => (
                        stop.offset,
                        stop.style,
                        pending.intersection(stop.properties),
                    ),
                    None => (1.0, base, pending),
                };
                if properties.is_empty() {
                    continue;
                }
                pending = pending.difference(properties);
                let span = to_offset - from_offset;
                let value = if span <= f32::EPSILON {
                    from_style
                } else {
                    let local = ((progress - from_offset) / span).clamp(0.0, 1.0);
                    from_style.lerp(to_style, apply_easing(easing, local))
                };
                sampled = AnimatableStyle::overlay(sampled, value, properties);
            }
        }
        sampled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{BoxShadow, Corners, Edges, Transform2D, VisualFilter, style::Color};

    fn style(opacity: f32, translate_x: f32) -> AnimatableStyle {
        AnimatableStyle {
            used_width: 0.0,
            used_height: 0.0,
            containing_block: None,
            border_radius: Corners::zero(),
            border_width: Edges::zero(),
            opacity,
            background_color: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            color: Color::WHITE,
            width: mesh_core_elements::Dimension::Auto,
            height: mesh_core_elements::Dimension::Auto,
            min_width: mesh_core_elements::Dimension::Auto,
            max_width: mesh_core_elements::Dimension::Auto,
            min_height: mesh_core_elements::Dimension::Auto,
            max_height: mesh_core_elements::Dimension::Auto,
            padding: Edges::zero(),
            margin: Edges::zero(),
            transform: Transform2D {
                translate_x,
                ..Transform2D::IDENTITY
            },
            box_shadow: BoxShadow::NONE,
            filter: VisualFilter::NONE,
            backdrop_filter: VisualFilter::NONE,
            font_size: 14.0,
            letter_spacing: 0.0,
            line_height: 1.4,
            gap: 0.0,
            inset_top: None,
            inset_right: None,
            inset_bottom: None,
            inset_left: None,
            visibility: mesh_core_elements::style::Visibility::Visible,
        }
    }

    fn opacity_and_transform() -> TransitionProperties {
        TransitionProperties {
            opacity: true,
            transform: true,
            ..TransitionProperties::none()
        }
    }

    fn opacity() -> TransitionProperties {
        TransitionProperties {
            opacity: true,
            ..TransitionProperties::none()
        }
    }

    fn registry() -> KeyframeRegistry {
        let mut registry = KeyframeRegistry::new();
        registry.insert(KeyframeRule {
            name: "pulse".into(),
            stops: vec![
                KeyframeStop {
                    offset: 0.0,
                    style: style(0.0, 0.0),
                    easing: None,
                    properties: opacity_and_transform(),
                },
                KeyframeStop {
                    offset: 0.5,
                    style: style(0.5, 50.0),
                    easing: None,
                    properties: opacity_and_transform(),
                },
                KeyframeStop {
                    offset: 1.0,
                    style: style(1.0, 100.0),
                    easing: None,
                    properties: opacity_and_transform(),
                },
            ],
        });
        registry
    }

    fn animation() -> ActiveKeyframeAnimation {
        ActiveKeyframeAnimation {
            rule_name: "pulse".into(),
            started_at: Instant::now(),
            duration: Duration::from_millis(1000),
            delay: Duration::ZERO,
            easing: Easing::Linear,
            iteration_count: AnimationIterationCount::Number(1),
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
            play_state: AnimationPlayState::Running,
            paused_at: None,
        }
    }

    #[test]
    fn keyframe_interpolates_between_percentage_stops() {
        let registry = registry();
        let animation = animation();
        let now = animation.started_at + Duration::from_millis(250);
        let current = animation
            .current(&registry, style(0.0, 0.0), now)
            .expect("current style");
        assert!((current.opacity - 0.25).abs() < 0.001);
        assert!((current.transform.translate_x - 25.0).abs() < 0.001);
    }

    #[test]
    fn a_property_skips_stops_that_do_not_declare_it() {
        let mut registry = registry();
        registry
            .rules
            .get_mut("pulse")
            .expect("test keyframe rule")
            .stops[1]
            .properties = TransitionProperties {
            transform: true,
            ..TransitionProperties::none()
        };
        let animation = animation();
        let current = animation
            .current(
                &registry,
                style(0.7, 0.0),
                animation.started_at + Duration::from_millis(250),
            )
            .expect("current style");

        // Opacity runs 0% -> 100% directly; the 50% stop only moves transform.
        assert!((current.opacity - 0.25).abs() < 0.001);
        assert!((current.transform.translate_x - 25.0).abs() < 0.001);
    }

    #[test]
    fn missing_end_stops_interpolate_from_and_to_the_base() {
        let mut registry = KeyframeRegistry::new();
        registry.insert(KeyframeRule {
            name: "pulse".into(),
            stops: vec![KeyframeStop {
                offset: 0.5,
                style: style(0.0, 0.0),
                easing: None,
                properties: opacity(),
            }],
        });
        let animation = animation();
        let base = style(1.0, 40.0);
        let at = |millis| {
            animation
                .current(
                    &registry,
                    base,
                    animation.started_at + Duration::from_millis(millis),
                )
                .expect("current style")
        };

        assert!((at(250).opacity - 0.5).abs() < 0.001);
        assert!((at(750).opacity - 0.5).abs() < 0.001);
        assert_eq!(at(250).transform.translate_x, 40.0);
    }

    #[test]
    fn keyframe_uses_easing_declared_at_the_start_of_each_segment() {
        let mut registry = registry();
        registry
            .rules
            .get_mut("pulse")
            .expect("test keyframe rule")
            .stops[1]
            .easing = Some(Easing::EaseIn);
        let animation = animation();
        let current = animation
            .current(
                &registry,
                style(0.0, 0.0),
                animation.started_at + Duration::from_millis(750),
            )
            .expect("eased segment frame");

        // The second half is sampled at local progress .5. Ease-in maps that
        // to .125, so the midpoint stop (.5) advances only 1/16 of the full
        // style range instead of the linear 1/4.
        assert!((current.opacity - 0.5625).abs() < 0.001);
        assert!((current.transform.translate_x - 56.25).abs() < 0.001);
    }

    #[test]
    fn backwards_fill_applies_first_frame_before_delay() {
        let registry = registry();
        let mut animation = animation();
        animation.delay = Duration::from_millis(200);
        animation.fill_mode = AnimationFillMode::Backwards;
        let current = animation
            .current(
                &registry,
                style(1.0, 100.0),
                animation.started_at + Duration::from_millis(50),
            )
            .expect("filled pre-delay style");
        assert_eq!(current.opacity, 0.0);
    }

    #[test]
    fn forwards_fill_holds_final_frame_after_completion() {
        let registry = registry();
        let mut animation = animation();
        animation.fill_mode = AnimationFillMode::Forwards;
        let current = animation
            .current(
                &registry,
                style(0.0, 0.0),
                animation.started_at + Duration::from_millis(1200),
            )
            .expect("forwards fill");
        assert_eq!(current.opacity, 1.0);
        assert_eq!(current.transform.translate_x, 100.0);
    }

    #[test]
    fn both_fill_covers_pre_delay_and_post_completion() {
        let registry = registry();
        let mut animation = animation();
        animation.delay = Duration::from_millis(100);
        animation.fill_mode = AnimationFillMode::Both;
        let pre = animation
            .current(
                &registry,
                style(1.0, 100.0),
                animation.started_at + Duration::from_millis(10),
            )
            .expect("backwards fill");
        let post = animation
            .current(
                &registry,
                style(0.0, 0.0),
                animation.started_at + Duration::from_millis(1200),
            )
            .expect("forwards fill");
        assert_eq!(pre.opacity, 0.0);
        assert_eq!(post.opacity, 1.0);
    }

    #[test]
    fn reverse_direction_flips_progress() {
        let registry = registry();
        let mut animation = animation();
        animation.direction = AnimationDirection::Reverse;
        let current = animation
            .current(
                &registry,
                style(0.0, 0.0),
                animation.started_at + Duration::from_millis(250),
            )
            .expect("reverse frame");
        assert!((current.opacity - 0.75).abs() < 0.001);
    }

    #[test]
    fn alternate_direction_flips_every_other_iteration() {
        let registry = registry();
        let mut animation = animation();
        animation.iteration_count = AnimationIterationCount::Number(2);
        animation.direction = AnimationDirection::Alternate;
        let current = animation
            .current(
                &registry,
                style(0.0, 0.0),
                animation.started_at + Duration::from_millis(1250),
            )
            .expect("alternate frame");
        assert!((current.opacity - 0.75).abs() < 0.001);
    }

    #[test]
    fn infinite_animation_never_finishes() {
        let mut animation = animation();
        animation.iteration_count = AnimationIterationCount::Infinite;
        assert!(!animation.finished(animation.started_at + Duration::from_secs(60)));
    }

    #[test]
    fn paused_animation_holds_last_frame() {
        let registry = registry();
        let mut animation = animation();
        animation.play_state = AnimationPlayState::Paused;
        animation.paused_at = Some(animation.started_at + Duration::from_millis(250));
        let first = animation
            .current(
                &registry,
                style(0.0, 0.0),
                animation.started_at + Duration::from_millis(250),
            )
            .expect("paused frame");
        let later = animation
            .current(
                &registry,
                style(0.0, 0.0),
                animation.started_at + Duration::from_millis(900),
            )
            .expect("still paused frame");
        assert_eq!(first.opacity, later.opacity);
        assert_eq!(first.transform.translate_x, later.transform.translate_x);
    }

    #[test]
    fn resuming_preserves_progress_after_a_long_pause() {
        let registry = registry();
        let mut animation = animation();
        animation.iteration_count = AnimationIterationCount::Number(3);
        animation.direction = AnimationDirection::Alternate;
        let started_at = animation.started_at;
        let pause_at = started_at + Duration::from_millis(1250);

        animation.set_play_state(AnimationPlayState::Paused, pause_at);
        let paused = animation
            .current(&registry, style(0.0, 0.0), pause_at)
            .expect("paused frame");
        assert!(!animation.finished(pause_at + Duration::from_secs(60)));

        let resume_at = pause_at + Duration::from_secs(60);
        animation.set_play_state(AnimationPlayState::Running, resume_at);
        assert_eq!(animation.started_at, started_at + Duration::from_secs(60));
        assert_eq!(animation.paused_at, None);

        let resumed = animation
            .current(&registry, style(0.0, 0.0), resume_at)
            .expect("resumed frame");
        assert_eq!(resumed.opacity, paused.opacity);
        assert_eq!(resumed.transform.translate_x, paused.transform.translate_x);
        assert!(!animation.finished(resume_at));
    }

    #[test]
    fn resuming_preserves_progress_at_iteration_boundaries() {
        let registry = registry();
        for direction in [
            AnimationDirection::Normal,
            AnimationDirection::Reverse,
            AnimationDirection::Alternate,
            AnimationDirection::AlternateReverse,
        ] {
            for boundary in [Duration::from_secs(1), Duration::from_secs(2)] {
                let mut animation = animation();
                animation.iteration_count = AnimationIterationCount::Infinite;
                animation.direction = direction;
                let pause_at = animation.started_at + boundary;
                animation.set_play_state(AnimationPlayState::Paused, pause_at);
                let paused = animation
                    .current(&registry, style(0.0, 0.0), pause_at)
                    .expect("paused iteration-boundary frame");

                let resume_at = pause_at + Duration::from_secs(30);
                animation.set_play_state(AnimationPlayState::Running, resume_at);
                let resumed = animation
                    .current(&registry, style(0.0, 0.0), resume_at)
                    .expect("resumed iteration-boundary frame");

                assert_eq!(resumed.opacity, paused.opacity);
                assert_eq!(resumed.transform.translate_x, paused.transform.translate_x);
            }
        }
    }

    fn blink() -> KeyframeRule {
        KeyframeRule {
            name: "blink".into(),
            stops: vec![
                KeyframeStop {
                    offset: 0.5,
                    style: style(0.0, 0.0),
                    easing: None,
                    properties: opacity(),
                },
                KeyframeStop {
                    offset: 1.0,
                    style: style(0.0, 0.0),
                    easing: None,
                    properties: opacity(),
                },
            ],
        }
    }

    #[test]
    fn stepped_keyframes_hold_until_their_next_boundary() {
        let ms = Duration::from_millis;
        let mut animation = animation();
        let start = animation.started_at;
        animation.rule_name = "blink".into();
        animation.easing = Easing::Steps(1, mesh_core_elements::StepPosition::JumpEnd);
        animation.iteration_count = AnimationIterationCount::Infinite;
        let rule = blink();
        assert_eq!(
            animation.next_change(&rule, start + ms(100)),
            NextChange::At(start + ms(500))
        );
        assert_eq!(
            animation.next_change(&rule, start + ms(700)),
            NextChange::At(start + ms(1000))
        );
        assert_eq!(
            animation.next_change(&rule, start + ms(2100)),
            NextChange::At(start + ms(2500))
        );
        // Every value change lands on a reported boundary.
        let mut at = start;
        while at < start + ms(3000) {
            let NextChange::At(next) = animation.next_change(&rule, at) else {
                panic!("a stepped animation holds");
            };
            let held = animation.current_with_rule(&rule, style(1.0, 0.0), at);
            let mut probe = at;
            while probe < next {
                assert_eq!(
                    animation
                        .current_with_rule(&rule, style(1.0, 0.0), probe)
                        .map(|s| s.opacity),
                    held.map(|s| s.opacity)
                );
                probe += ms(7);
            }
            at = next;
        }

        animation.delay = ms(200);
        animation.easing = Easing::Linear;
        assert_eq!(
            animation.next_change(&rule, start + ms(50)),
            NextChange::At(start + ms(200))
        );
        assert_eq!(
            animation.next_change(&rule, start + ms(300)),
            NextChange::EveryFrame
        );
        animation.set_play_state(AnimationPlayState::Paused, start + ms(300));
        assert_eq!(
            animation.next_change(&rule, start + ms(400)),
            NextChange::Never
        );
    }
}
