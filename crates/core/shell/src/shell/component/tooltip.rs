use mesh_core_animation::{Easing, apply_easing};
use mesh_core_config::TooltipSettings;
use mesh_core_elements::style::{TooltipAnchor, parse_animation_shorthand, parse_transform};
use mesh_core_theme::Theme;
use std::time::Duration;

/// A tooltip enter animation lowered from theme CSS.
///
/// Authored entirely in the theme: the `tooltip { animation: <name>
/// <duration> <easing>; }` shorthand names a theme-level `@keyframes` rule
/// whose stops may declare `opacity` and `transform` (`scale()`,
/// `translate()`, `translateX()`, `translateY()`). No animation in the theme
/// means the tooltip appears instantly.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TooltipAnimation {
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
    /// Stops sorted by offset.
    stops: Vec<TooltipAnimationStop>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TooltipAnimationStop {
    offset: f32,
    opacity: Option<f32>,
    scale: Option<f32>,
    translate_x: Option<f32>,
    translate_y: Option<f32>,
}

/// Animated tooltip paint values at one point in time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TooltipAnimationSample {
    pub opacity: f32,
    pub scale: f32,
    pub translate_x: f32,
    pub translate_y: f32,
}

impl TooltipAnimationSample {
    /// The resting state once the animation has finished (or when the theme
    /// defines none).
    pub const FINISHED: Self = Self {
        opacity: 1.0,
        scale: 1.0,
        translate_x: 0.0,
        translate_y: 0.0,
    };
}

/// Lower the theme's tooltip animation (`tooltip { animation: ... }` plus its
/// `@keyframes` rule) into a sampled form. Returns `None` when the theme does
/// not declare one — the tooltip then shows instantly.
pub(super) fn tooltip_animation_from_theme(theme: &Theme) -> Option<TooltipAnimation> {
    let raw = theme.component_defaults("tooltip")?.get("animation")?;
    let resolved = resolve_theme_value_tokens(theme, raw);
    let shorthand = parse_animation_shorthand(&resolved);
    let style = shorthand.first()?;
    let name = style.name.as_deref()?;

    let stops = theme
        .keyframe_stops(name)?
        .iter()
        .map(|stop| {
            let opacity = stop
                .declarations
                .get("opacity")
                .map(|v| resolve_theme_value_tokens(theme, v))
                .and_then(|v| v.trim().parse::<f32>().ok());
            let transform = stop
                .declarations
                .get("transform")
                .map(|v| resolve_theme_value_tokens(theme, v))
                .map(|v| parse_transform(&v));
            TooltipAnimationStop {
                offset: stop.offset.clamp(0.0, 1.0),
                opacity,
                scale: transform.map(|t| t.scale_x),
                translate_x: transform.map(|t| t.translate_x),
                translate_y: transform.map(|t| t.translate_y),
            }
        })
        .collect::<Vec<_>>();
    if stops.is_empty() {
        return None;
    }

    Some(TooltipAnimation {
        duration: Duration::from_millis(u64::from(style.duration_ms)),
        delay: Duration::from_millis(u64::from(style.delay_ms)),
        easing: Easing::from(style.easing),
        stops,
    })
}

impl TooltipAnimation {
    /// Total wall-clock time until the animation reaches its resting state.
    pub fn total_duration(&self) -> Duration {
        self.delay + self.duration
    }

    /// Sample the animated values at `elapsed` since the tooltip appeared.
    /// During the delay the first stop's values hold (an enter animation
    /// fills backwards); past the end the sample is the last stop.
    pub fn sample(&self, elapsed: Duration) -> TooltipAnimationSample {
        let progress = if elapsed <= self.delay {
            0.0
        } else if self.duration.is_zero() {
            1.0
        } else {
            ((elapsed - self.delay).as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
        };
        TooltipAnimationSample {
            opacity: self.sample_property(progress, |s| s.opacity, 1.0),
            scale: self.sample_property(progress, |s| s.scale, 1.0),
            translate_x: self.sample_property(progress, |s| s.translate_x, 0.0),
            translate_y: self.sample_property(progress, |s| s.translate_y, 0.0),
        }
    }

    /// Per-property interpolation between the stops that declare it, with the
    /// timing function applied per segment (CSS keyframe semantics).
    fn sample_property(
        &self,
        progress: f32,
        get: impl Fn(&TooltipAnimationStop) -> Option<f32>,
        default: f32,
    ) -> f32 {
        let mut prev: Option<(f32, f32)> = None;
        let mut next: Option<(f32, f32)> = None;
        for stop in &self.stops {
            let Some(value) = get(stop) else { continue };
            if stop.offset <= progress {
                prev = Some((stop.offset, value));
            } else {
                next = Some((stop.offset, value));
                break;
            }
        }
        match (prev, next) {
            (Some((from_offset, from)), Some((to_offset, to))) => {
                let span = (to_offset - from_offset).max(f32::EPSILON);
                let local = ((progress - from_offset) / span).clamp(0.0, 1.0);
                from + (to - from) * apply_easing(self.easing, local)
            }
            (Some((_, value)), None) | (None, Some((_, value))) => value,
            (None, None) => default,
        }
    }
}

/// Resolve `var(--a-b)` tokens inside a theme CSS value against the theme's
/// token map (`--a-b` → token `a.b`), token by token so shorthands like
/// `animation: name var(--animation-duration-short) ease-out` work. Theme
/// duration tokens are bare millisecond numbers, so a var substitution that
/// yields a bare number is suffixed with `ms` to stay a valid CSS time.
fn resolve_theme_value_tokens(theme: &Theme, raw: &str) -> String {
    raw.split_whitespace()
        .map(|token| {
            let Some(variable) = token
                .strip_prefix("var(")
                .and_then(|s| s.strip_suffix(')'))
                .map(str::trim)
            else {
                return token.to_string();
            };
            let resolved = variable
                .strip_prefix("--")
                .map(|name| name.replace('-', "."))
                .and_then(|token_name| theme.token(&token_name).map(|v| v.to_string()));
            match resolved {
                Some(value) if value.trim().parse::<f64>().is_ok() => {
                    format!("{}ms", value.trim())
                }
                Some(value) => value,
                None => token.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Result of tooltip placement computation.
pub(super) struct TooltipPlacement {
    /// Final paint X coordinate (logical pixels). For top/bottom sides this is
    /// the horizontal center of the tooltip box; for left/right/cursor it is
    /// the left edge.
    pub paint_x: f32,
    /// Final paint Y coordinate (logical pixels).
    pub paint_y: f32,
    /// Opacity for fade-in animation (0.0–1.0).
    pub opacity: f32,
    /// The side the tooltip actually ended up on after auto placement and
    /// overflow flipping. Drives the slide-in direction and X centering.
    pub side: PlacedSide,
}

/// The concrete side a tooltip was placed on. Unlike [`ResolvedAnchor`] this
/// is always a paintable outcome — `Auto` has been decided by then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlacedSide {
    Bottom,
    Top,
    Left,
    Right,
    Cursor,
}

/// Compute the effective tooltip anchor for an element.
///
/// Resolution cascade:
/// 1. shell-wide default from settings (`tooltip.position`, default `bottom`)
/// 2. element-specific preference (`tooltip-anchor` CSS) overrides the shell
///    default
/// 3. `auto` (either level) defers fully to container-aware placement in
///    [`compute_tooltip_placement`]; explicit sides also flip there when they
///    would overflow their container
pub(super) fn effective_anchor(
    element_anchor: TooltipAnchor,
    config: &TooltipSettings,
) -> ResolvedAnchor {
    match element_anchor {
        TooltipAnchor::Auto => match config.position.as_str() {
            "bottom" => ResolvedAnchor::Bottom,
            "top" => ResolvedAnchor::Top,
            "left" => ResolvedAnchor::Left,
            "right" => ResolvedAnchor::Right,
            "cursor" => ResolvedAnchor::Cursor,
            _ => ResolvedAnchor::Auto, // "auto" and unknown values
        },
        TooltipAnchor::Bottom => ResolvedAnchor::Bottom,
        TooltipAnchor::Top => ResolvedAnchor::Top,
        TooltipAnchor::Left => ResolvedAnchor::Left,
        TooltipAnchor::Right => ResolvedAnchor::Right,
        TooltipAnchor::Cursor => ResolvedAnchor::Cursor,
    }
}

/// Resolved tooltip placement strategy after config and element preferences
/// have been combined. `Auto` means "pick the side with room, preferring
/// bottom".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResolvedAnchor {
    Auto,
    Bottom,
    Top,
    Left,
    Right,
    Cursor,
}

/// Compute tooltip position with container- and surface-edge avoidance.
///
/// `tooltip_size` is the measured logical box of the tooltip (text plus
/// padding), so fit checks reflect what will actually paint. `container_bounds`
/// is the innermost clipping ancestor of the hovered element when one exists;
/// the tooltip prefers sides where it stays inside that box. Without a clipping
/// container the whole paint surface (which includes the overlay reserve below
/// bar content) is the constraint, so bar tooltips keep rendering in the
/// reserve below the element.
///
/// Flip rules: the preferred side is used when the tooltip fits inside the
/// constraint box there; otherwise the opposite side is used if it fits;
/// otherwise the preferred side wins and the renderer clamps into the buffer.
pub(super) fn compute_tooltip_placement(
    anchor: ResolvedAnchor,
    element_bounds: Option<(f32, f32, f32, f32)>,
    container_bounds: Option<(f32, f32, f32, f32)>,
    cursor: (f32, f32),
    tooltip_size: (f32, f32),
    surface_size: (f32, f32),
    opacity: f32,
    config: &TooltipSettings,
) -> TooltipPlacement {
    let gap = config.gap;
    let (tw, th) = tooltip_size;
    let (sw, sh) = surface_size;

    let cursor_placement = |opacity: f32| TooltipPlacement {
        paint_x: cursor.0 + config.cursor_offset_x,
        paint_y: cursor.1 + config.cursor_offset_y,
        opacity,
        side: PlacedSide::Cursor,
    };

    if anchor == ResolvedAnchor::Cursor {
        return cursor_placement(opacity);
    }

    let Some((el_l, el_t, el_r, el_b)) = element_bounds else {
        // No element bounds → fall back to cursor positioning.
        return cursor_placement(opacity);
    };

    // Element at origin likely means unmeasured → cursor fallback.
    if el_l == 0.0 && el_r == 0.0 {
        return cursor_placement(opacity);
    }

    // Constraint box the tooltip should stay inside: the element's clipping
    // container clamped to the surface, or the whole surface when the element
    // is not inside a clipping container.
    let (limit_l, limit_t, limit_r, limit_b) = match container_bounds {
        Some((cl, ct, cr, cb)) => (cl.max(0.0), ct.max(0.0), cr.min(sw), cb.min(sh)),
        None => (0.0, 0.0, sw, sh),
    };

    let center_x = (el_l + el_r) / 2.0;
    let center_y = (el_t + el_b) / 2.0 - th / 2.0;

    let below_y = el_b + gap;
    let above_y = el_t - gap - th;
    let fits_below = below_y + th <= limit_b;
    let fits_above = above_y >= limit_t;

    let left_x = el_l - gap - tw;
    let right_x = el_r + gap;
    let fits_left = left_x >= limit_l;
    let fits_right = right_x + tw <= limit_r;

    let below = || (center_x, below_y.max(0.0), PlacedSide::Bottom);
    let above = || (center_x, above_y, PlacedSide::Top);
    let left = || (left_x, center_y, PlacedSide::Left);
    let right = || (right_x, center_y, PlacedSide::Right);

    let (px, py, side) = match anchor {
        // Auto and Bottom both prefer below the element; Auto exists as a
        // distinct strategy so settings/elements can opt into "wherever fits"
        // without implying an authored preference.
        ResolvedAnchor::Auto | ResolvedAnchor::Bottom => {
            if fits_below || !fits_above {
                below()
            } else {
                above()
            }
        }
        ResolvedAnchor::Top => {
            if fits_above || !fits_below {
                above()
            } else {
                below()
            }
        }
        ResolvedAnchor::Left => {
            if fits_left || !fits_right {
                left()
            } else {
                right()
            }
        }
        ResolvedAnchor::Right => {
            if fits_right || !fits_left {
                right()
            } else {
                left()
            }
        }
        ResolvedAnchor::Cursor => unreachable!(),
    };

    TooltipPlacement {
        paint_x: px,
        paint_y: py,
        opacity,
        side,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOOLTIP: (f32, f32) = (120.0, 26.0);
    const SURFACE: (f32, f32) = (240.0, 160.0);

    #[test]
    fn default_settings_place_tooltip_below_element() {
        let settings = TooltipSettings::default();
        let anchor = effective_anchor(TooltipAnchor::Auto, &settings);

        let placement = compute_tooltip_placement(
            anchor,
            Some((10.0, 20.0, 50.0, 44.0)),
            None,
            (18.0, 28.0),
            TOOLTIP,
            SURFACE,
            1.0,
            &settings,
        );

        assert_eq!(anchor, ResolvedAnchor::Bottom);
        // paint_x is the element center: (10 + 50) / 2 = 30. The renderer
        // centers the tooltip box around this X coordinate.
        assert_eq!(placement.paint_x, 30.0);
        assert_eq!(placement.paint_y, 44.0 + settings.gap);
        assert_eq!(placement.side, PlacedSide::Bottom);
    }

    #[test]
    fn element_anchor_overrides_shell_default() {
        let settings = TooltipSettings::default(); // position = "bottom"
        let anchor = effective_anchor(TooltipAnchor::Top, &settings);
        assert_eq!(anchor, ResolvedAnchor::Top);

        let placement = compute_tooltip_placement(
            anchor,
            Some((10.0, 100.0, 50.0, 124.0)),
            None,
            (18.0, 108.0),
            TOOLTIP,
            SURFACE,
            1.0,
            &settings,
        );
        assert_eq!(placement.side, PlacedSide::Top);
        assert_eq!(placement.paint_y, 100.0 - settings.gap - TOOLTIP.1);
    }

    #[test]
    fn auto_position_setting_resolves_to_auto() {
        let settings = TooltipSettings {
            position: "auto".into(),
            ..TooltipSettings::default()
        };
        assert_eq!(
            effective_anchor(TooltipAnchor::Auto, &settings),
            ResolvedAnchor::Auto
        );
    }

    #[test]
    fn bottom_flips_above_when_container_bottom_would_overflow() {
        let settings = TooltipSettings::default();
        // Element sits at the bottom edge of a clipping container; below it
        // there is no room inside the container, above there is plenty.
        let container = Some((0.0, 0.0, 240.0, 130.0));
        let placement = compute_tooltip_placement(
            ResolvedAnchor::Bottom,
            Some((10.0, 100.0, 50.0, 124.0)),
            container,
            (18.0, 108.0),
            TOOLTIP,
            SURFACE,
            1.0,
            &settings,
        );
        assert_eq!(placement.side, PlacedSide::Top);
        assert_eq!(placement.paint_y, 100.0 - settings.gap - TOOLTIP.1);
    }

    #[test]
    fn top_flips_below_when_element_at_container_top() {
        let settings = TooltipSettings::default();
        let container = Some((0.0, 10.0, 240.0, 150.0));
        let placement = compute_tooltip_placement(
            ResolvedAnchor::Top,
            Some((10.0, 12.0, 50.0, 36.0)),
            container,
            (18.0, 20.0),
            TOOLTIP,
            SURFACE,
            1.0,
            &settings,
        );
        assert_eq!(placement.side, PlacedSide::Bottom);
        assert_eq!(placement.paint_y, 36.0 + settings.gap);
    }

    #[test]
    fn bottom_stays_below_when_neither_side_fits_container() {
        let settings = TooltipSettings::default();
        // Short bar-like container: the tooltip fits neither above nor below
        // inside it, so the authored preference (below) wins and paints into
        // the overlay reserve.
        let container = Some((0.0, 0.0, 240.0, 40.0));
        let placement = compute_tooltip_placement(
            ResolvedAnchor::Bottom,
            Some((10.0, 4.0, 50.0, 36.0)),
            container,
            (18.0, 20.0),
            TOOLTIP,
            SURFACE,
            1.0,
            &settings,
        );
        assert_eq!(placement.side, PlacedSide::Bottom);
        assert_eq!(placement.paint_y, 36.0 + settings.gap);
    }

    #[test]
    fn auto_prefers_bottom_then_flips_at_container_bottom() {
        let settings = TooltipSettings {
            position: "auto".into(),
            ..TooltipSettings::default()
        };
        let anchor = effective_anchor(TooltipAnchor::Auto, &settings);
        let container = Some((0.0, 0.0, 240.0, 150.0));

        // Element near container top → below.
        let near_top = compute_tooltip_placement(
            anchor,
            Some((10.0, 4.0, 50.0, 28.0)),
            container,
            (18.0, 12.0),
            TOOLTIP,
            SURFACE,
            1.0,
            &settings,
        );
        assert_eq!(near_top.side, PlacedSide::Bottom);

        // Element near container bottom → above.
        let near_bottom = compute_tooltip_placement(
            anchor,
            Some((10.0, 120.0, 50.0, 144.0)),
            container,
            (18.0, 130.0),
            TOOLTIP,
            SURFACE,
            1.0,
            &settings,
        );
        assert_eq!(near_bottom.side, PlacedSide::Top);
    }

    #[test]
    fn right_flips_left_when_container_right_would_overflow() {
        let settings = TooltipSettings::default();
        let container = Some((0.0, 0.0, 200.0, 160.0));
        let placement = compute_tooltip_placement(
            ResolvedAnchor::Right,
            Some((150.0, 20.0, 190.0, 44.0)),
            container,
            (160.0, 28.0),
            TOOLTIP,
            SURFACE,
            1.0,
            &settings,
        );
        assert_eq!(placement.side, PlacedSide::Left);
        assert_eq!(placement.paint_x, 150.0 - settings.gap - TOOLTIP.0);
    }

    fn theme_with_tooltip_animation(shorthand: &str) -> Theme {
        use mesh_core_theme::ThemeKeyframeStop;
        let mut theme = mesh_core_theme::default_theme();
        theme
            .defaults
            .components
            .entry("tooltip".into())
            .or_default()
            .insert("animation".into(), shorthand.into());
        theme.keyframes.insert(
            "tooltip-enter".into(),
            vec![
                ThemeKeyframeStop {
                    offset: 0.0,
                    declarations: [
                        ("opacity".to_string(), "0".to_string()),
                        (
                            "transform".to_string(),
                            "scale(0.5) translateY(10px)".to_string(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                },
                ThemeKeyframeStop {
                    offset: 1.0,
                    declarations: [
                        ("opacity".to_string(), "1".to_string()),
                        ("transform".to_string(), "scale(1)".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                },
            ],
        );
        theme
    }

    #[test]
    fn theme_animation_lowers_shorthand_and_keyframes() {
        let theme = theme_with_tooltip_animation("tooltip-enter 200ms linear");
        let animation = tooltip_animation_from_theme(&theme).expect("animation lowered");

        assert_eq!(animation.duration, Duration::from_millis(200));
        assert_eq!(animation.easing, Easing::Linear);
        assert_eq!(animation.total_duration(), Duration::from_millis(200));

        let start = animation.sample(Duration::ZERO);
        assert_eq!(start.opacity, 0.0);
        assert_eq!(start.scale, 0.5);
        assert_eq!(start.translate_y, 10.0);

        let mid = animation.sample(Duration::from_millis(100));
        assert!((mid.opacity - 0.5).abs() < 1e-3);
        assert!((mid.scale - 0.75).abs() < 1e-3);
        assert!((mid.translate_y - 5.0).abs() < 1e-3);

        let done = animation.sample(Duration::from_millis(300));
        assert_eq!(done, TooltipAnimationSample::FINISHED);
    }

    #[test]
    fn theme_animation_resolves_bare_number_duration_tokens_as_ms() {
        // Theme duration tokens are bare millisecond numbers (`--animation-
        // duration-short: 150`); a var() substitution must stay a CSS time.
        let mut theme =
            theme_with_tooltip_animation("tooltip-enter var(--animation-duration-short) ease-out");
        theme.tokens.insert(
            "animation.duration.short".into(),
            mesh_core_theme::TokenValue::Number(150.0),
        );
        let animation = tooltip_animation_from_theme(&theme).expect("animation lowered");
        assert_eq!(animation.duration, Duration::from_millis(150));
        assert_eq!(animation.easing, Easing::EaseOut);
    }

    #[test]
    fn missing_keyframes_or_animation_means_no_animation() {
        let mut theme = theme_with_tooltip_animation("does-not-exist 100ms");
        assert!(tooltip_animation_from_theme(&theme).is_none());

        let tooltip_defaults = theme.defaults.components.get_mut("tooltip").unwrap();
        *tooltip_defaults = tooltip_defaults
            .iter()
            .filter(|(property, _)| property.as_str() != "animation")
            .map(|(property, value)| (property.clone(), value.clone()))
            .collect();
        assert!(tooltip_animation_from_theme(&theme).is_none());
    }

    #[test]
    fn animation_delay_holds_first_stop() {
        let theme = theme_with_tooltip_animation("tooltip-enter 100ms 50ms linear");
        let animation = tooltip_animation_from_theme(&theme).expect("animation lowered");
        assert_eq!(animation.delay, Duration::from_millis(50));
        assert_eq!(animation.total_duration(), Duration::from_millis(150));
        let during_delay = animation.sample(Duration::from_millis(25));
        assert_eq!(during_delay.opacity, 0.0);
        assert_eq!(during_delay.scale, 0.5);
    }

    #[test]
    fn configured_cursor_position_uses_cursor_offsets() {
        let settings = TooltipSettings {
            position: "cursor".into(),
            cursor_offset_x: 4.0,
            cursor_offset_y: 9.0,
            ..TooltipSettings::default()
        };
        let anchor = effective_anchor(TooltipAnchor::Auto, &settings);

        let placement = compute_tooltip_placement(
            anchor,
            Some((10.0, 20.0, 50.0, 44.0)),
            None,
            (18.0, 28.0),
            TOOLTIP,
            SURFACE,
            0.5,
            &settings,
        );

        assert_eq!(anchor, ResolvedAnchor::Cursor);
        assert_eq!(placement.paint_x, 22.0);
        assert_eq!(placement.paint_y, 37.0);
        assert_eq!(placement.opacity, 0.5);
        assert_eq!(placement.side, PlacedSide::Cursor);
    }
}
