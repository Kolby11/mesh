//! `@keyframes` playback over validated percentage stops.
//!
//! Parsing and validation live in `mesh-core-component`. The renderer consumes
//! already-lowered keyframe rules and produces the in-flight animatable style
//! for a given timestamp.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mesh_core_elements::style::{
    AnimationDirection, AnimationFillMode, AnimationIterationCount, AnimationPlayState,
};

use super::easing::{Easing, apply_easing};
use super::transition::AnimatableStyle;
use crate::Interpolate;

/// A single `<percent>% { ... }` block from a validated `@keyframes` rule.
#[derive(Debug, Clone)]
pub struct KeyframeStop {
    /// Position in the timeline, normalized to `[0.0, 1.0]`.
    pub offset: f32,
    /// Style snapshot at this stop.
    pub style: AnimatableStyle,
    /// Segment-local easing override starting at this stop.
    pub easing: Option<Easing>,
}

/// One named `@keyframes` rule, sorted by `offset`.
#[derive(Debug, Clone)]
pub struct KeyframeRule {
    pub name: String,
    pub stops: Vec<KeyframeStop>,
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
        let rule = registry.get(&self.rule_name)?;
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

    fn sample_rule(
        &self,
        rule: &KeyframeRule,
        base: AnimatableStyle,
        progress: f32,
    ) -> AnimatableStyle {
        if rule.stops.len() == 1 {
            return base.lerp(rule.stops[0].style, 1.0);
        }

        let progress = progress.clamp(0.0, 1.0);
        let first = &rule.stops[0];
        if progress <= first.offset {
            return base.lerp(first.style, 1.0);
        }

        let last = rule.stops.last().expect("checked len above");
        if progress >= last.offset {
            return base.lerp(last.style, 1.0);
        }

        for window in rule.stops.windows(2) {
            let from = &window[0];
            let to = &window[1];
            if progress <= to.offset {
                let span = (to.offset - from.offset).max(f32::EPSILON);
                let local = ((progress - from.offset) / span).clamp(0.0, 1.0);
                let easing = from.easing.unwrap_or(self.easing);
                return from.style.lerp(to.style, apply_easing(easing, local));
            }
        }

        last.style
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{BoxShadow, Corners, Edges, Transform2D, VisualFilter, style::Color};

    fn style(opacity: f32, translate_x: f32) -> AnimatableStyle {
        AnimatableStyle {
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

    fn registry() -> KeyframeRegistry {
        let mut registry = KeyframeRegistry::new();
        registry.insert(KeyframeRule {
            name: "pulse".into(),
            stops: vec![
                KeyframeStop {
                    offset: 0.0,
                    style: style(0.0, 0.0),
                    easing: None,
                },
                KeyframeStop {
                    offset: 0.5,
                    style: style(0.5, 50.0),
                    easing: None,
                },
                KeyframeStop {
                    offset: 1.0,
                    style: style(1.0, 100.0),
                    easing: None,
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
}
