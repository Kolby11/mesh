use mesh_core_animation::{Easing, apply_easing};
use mesh_core_config::TooltipSettings;
use mesh_core_elements::style::{Color, TooltipAnchor, parse_animation_shorthand, parse_transform};
use mesh_core_render::TooltipPaintStyle;
use mesh_core_theme::Theme;
use std::sync::Arc;
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
    easing: Option<Easing>,
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
    let resolved = resolve_theme_animation_tokens(theme, raw);
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
                easing: stop
                    .easing
                    .as_deref()
                    .map(|value| resolve_theme_value_tokens(theme, value))
                    .and_then(|value| mesh_core_component::style::parse_easing(&value))
                    .map(Easing::from),
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

/// Upper bounds for themed tooltip metrics. The overlay reserve a surface
/// adds for tooltips (`TOOLTIP_OVERLAY_*`) is sized for a 320px wrap width and
/// modest padding, so larger values would paint outside the buffer.
const TOOLTIP_MAX_WRAP_WIDTH: f32 = 320.0;
const TOOLTIP_MAX_PADDING: f32 = 12.0;
const TOOLTIP_MAX_BORDER_WIDTH: f32 = 4.0;

/// Lower the theme's `tooltip {}` rule into tooltip chrome. Colours missing
/// from the rule follow the surface-container roles, so themes that only set
/// timing still get tooltips in their palette.
pub(super) fn tooltip_paint_style_from_theme(theme: &Theme) -> TooltipPaintStyle {
    let fallback = TooltipPaintStyle::DEFAULT_DARK;
    let token_color = |name: &str, default: Color| {
        theme
            .resolve_token_value(name)
            .ok()
            .flatten()
            .and_then(|value| match value {
                mesh_core_theme::TokenValue::String(value) => Color::from_css(&value),
                _ => None,
            })
            .unwrap_or(default)
    };
    let mut style = TooltipPaintStyle {
        background: token_color("color.surface-container", fallback.background),
        border: token_color("color.surface-container-high", fallback.border),
        foreground: token_color("color.on-surface", fallback.foreground),
        ..fallback
    };
    let Some(rule) = theme.component_defaults("tooltip") else {
        return style;
    };
    let value = |property: &str| {
        rule.get(property)
            .map(|raw| resolve_theme_value_tokens(theme, raw))
    };
    let color = |property: &str| value(property).and_then(|value| Color::from_css(&value));
    let length = |property: &str| value(property).and_then(|value| parse_length(&value));

    if let Some(border) = value("border") {
        for token in border.split_whitespace() {
            if let Some(width) = parse_length(token) {
                style.border_width = width;
            } else if let Some(color) = Color::from_css(token) {
                style.border = color;
            }
        }
    }
    if let Some(background) = color("background").or_else(|| color("background-color")) {
        style.background = background;
    }
    if let Some(border) = color("border-color") {
        style.border = border;
    }
    if let Some(foreground) = color("color") {
        style.foreground = foreground;
    }
    if let Some(width) = length("border-width") {
        style.border_width = width;
    }
    if let Some(radius) = length("border-radius") {
        style.radius = radius;
    }
    if let Some(padding) = value("padding") {
        let values = padding
            .split_whitespace()
            .filter_map(parse_length)
            .collect::<Vec<_>>();
        match values.as_slice() {
            [all] => (style.padding_y, style.padding_x) = (*all, *all),
            [vertical, horizontal, ..] => {
                (style.padding_y, style.padding_x) = (*vertical, *horizontal)
            }
            [] => {}
        }
    }
    if let Some(size) = length("font-size") {
        style.font_size = size;
    }
    if let Some(weight) = value("font-weight") {
        style.font_weight = match weight.trim() {
            "normal" => 400,
            "bold" => 700,
            other => other.parse::<f32>().map_or(style.font_weight, |w| w as u16),
        };
    }
    if let Some(width) = length("max-width") {
        style.max_width = width;
    }

    style.border_width = style.border_width.clamp(0.0, TOOLTIP_MAX_BORDER_WIDTH);
    style.radius = style.radius.max(0.0);
    style.padding_x = style.padding_x.clamp(0.0, TOOLTIP_MAX_PADDING);
    style.padding_y = style.padding_y.clamp(0.0, TOOLTIP_MAX_PADDING);
    style.font_size = style.font_size.clamp(8.0, 32.0);
    style.font_weight = style.font_weight.clamp(100, 900);
    style.max_width = style.max_width.clamp(40.0, TOOLTIP_MAX_WRAP_WIDTH);
    style
}

/// A bare number or a `px` length; theme size tokens are unitless pixels.
fn parse_length(value: &str) -> Option<f32> {
    let value = value.trim();
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
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
        let mut prev: Option<(f32, f32, Option<Easing>)> = None;
        let mut next: Option<(f32, f32, Option<Easing>)> = None;
        for stop in &self.stops {
            let Some(value) = get(stop) else { continue };
            if stop.offset <= progress {
                prev = Some((stop.offset, value, stop.easing));
            } else {
                next = Some((stop.offset, value, stop.easing));
                break;
            }
        }
        match (prev, next) {
            (Some((from_offset, from, easing)), Some((to_offset, to, _))) => {
                let span = (to_offset - from_offset).max(f32::EPSILON);
                let local = ((progress - from_offset) / span).clamp(0.0, 1.0);
                from + (to - from) * apply_easing(easing.unwrap_or(self.easing), local)
            }
            (Some((_, value, _)), None) | (None, Some((_, value, _))) => value,
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
    let resolved = theme
        .resolve_token_references(raw)
        .unwrap_or_else(|_| raw.to_string());
    if raw.trim().starts_with("var(")
        && raw.trim().ends_with(')')
        && resolved.trim().parse::<f64>().is_ok()
    {
        format!("{}ms", resolved.trim())
    } else {
        resolved
    }
}

/// Resolve an animation shorthand while preserving the project's convention
/// that numeric theme duration tokens represent milliseconds. The generic
/// theme resolver intentionally preserves scalar types and therefore returns
/// `150` for `var(--animation-duration-short)`; in this CSS time position it
/// must become `150ms` before the shorthand parser sees it.
fn resolve_theme_animation_tokens(theme: &Theme, raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(start) = rest.find("var(") {
        output.push_str(&rest[..start]);
        let reference_start = start + "var(".len();
        let Some(end) = find_matching_parenthesis(&rest[reference_start..]) else {
            output.push_str(&rest[start..]);
            return output;
        };
        let reference_end = reference_start + end + 1;
        let reference = &rest[start..reference_end];
        let resolved = theme
            .resolve_token_references(reference)
            .unwrap_or_else(|_| reference.to_string());
        if resolved.trim().parse::<f64>().is_ok() {
            output.push_str(resolved.trim());
            output.push_str("ms");
        } else {
            output.push_str(&resolved);
        }
        rest = &rest[reference_end..];
    }

    output.push_str(rest);
    output
}

fn find_matching_parenthesis(value: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in value.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' if depth == 0 => return Some(offset),
            b')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Key of the child surface a tooltip is promoted to. It cannot collide with
/// an authored `_mesh_key`, which never starts with `::`.
pub(super) const TOOLTIP_CHILD_KEY: &str = "::mesh-tooltip";

/// Transparent ring around a promoted tooltip box: the painter keeps the box
/// 4px from the buffer's leading edges and 6px from its trailing ones, and
/// draws the themed border (at most `TOOLTIP_MAX_BORDER_WIDTH`) outside it.
const TOOLTIP_POPUP_PADDING: u32 = 8;

/// A tooltip that does not fit inside its surface, presented as an
/// `xdg_popup` anchored to the element instead of painted in place. Bottom-
/// and right-anchored layer surfaces and windows have no tooltip reserve on
/// those sides, so their tooltips need this to show at all.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TooltipPopup {
    pub text: Arc<str>,
    /// The element, `(left, top, right, bottom)` in surface coordinates.
    pub element: (f32, f32, f32, f32),
    pub side: PlacedSide,
    /// The tooltip box, excluding its outside border.
    pub size: (f32, f32),
    pub gap: f32,
    pub style: TooltipPaintStyle,
    pub font_family: String,
    pub opacity: f32,
    pub scale: f32,
}

impl TooltipPopup {
    /// Whether a box of `size` at the painter's position for `placement`
    /// leaves the `surface`, including the painter's 4px/6px edge clamps.
    pub fn overflows(placement: &TooltipPlacement, size: (f32, f32), surface: (f32, f32)) -> bool {
        let left = match placement.side {
            PlacedSide::Top | PlacedSide::Bottom => placement.paint_x - size.0 / 2.0,
            _ => placement.paint_x,
        };
        left < 4.0
            || placement.paint_y < 4.0
            || left + size.0 > surface.0 - 6.0
            || placement.paint_y + size.1 > surface.1 - 6.0
    }

    /// The popup request: anchored to the element's side, grown away from it,
    /// flipped across it when the output edge is in the way. The gap is part
    /// of the anchor rectangle because `xdg_positioner` does not flip offsets.
    pub fn request(&self) -> mesh_core_frontend_shell_adapter::ChildSurfaceRequest {
        use mesh_core_elements::{
            PopoverAnchor, PopoverConstraintAdjustment, PopoverGravity, PopoverPlacement,
        };
        let (left, top, right, bottom) = self.element;
        let vertical = !matches!(self.side, PlacedSide::Left | PlacedSide::Right);
        let (left, top, right, bottom) = if vertical {
            (left, top - self.gap, right, bottom + self.gap)
        } else {
            (left - self.gap, top, right + self.gap, bottom)
        };
        let (anchor, gravity) = match self.side {
            PlacedSide::Top => (PopoverAnchor::Top, PopoverGravity::Top),
            PlacedSide::Left => (PopoverAnchor::Left, PopoverGravity::Left),
            PlacedSide::Right => (PopoverAnchor::Right, PopoverGravity::Right),
            PlacedSide::Bottom | PlacedSide::Cursor => (PopoverAnchor::Bottom, PopoverGravity::Bottom),
        };
        let x = left.floor() as i32;
        let y = top.floor() as i32;
        mesh_core_frontend_shell_adapter::ChildSurfaceRequest {
            node_key: TOOLTIP_CHILD_KEY.to_string(),
            kind: mesh_core_frontend_shell_adapter::ChildSurfaceKind::Tooltip,
            anchor_rect: (
                x,
                y,
                (right.ceil() as i32 - x).max(1),
                (bottom.ceil() as i32 - y).max(1),
            ),
            content_size: (
                self.size.0.ceil().max(1.0) as u32,
                self.size.1.ceil().max(1.0) as u32,
            ),
            content_padding: (TOOLTIP_POPUP_PADDING, TOOLTIP_POPUP_PADDING, TOOLTIP_POPUP_PADDING, TOOLTIP_POPUP_PADDING),
            placement: PopoverPlacement {
                anchor,
                gravity,
                offset_x: 0,
                offset_y: 0,
                constraint_adjustment: PopoverConstraintAdjustment {
                    flip_x: !vertical,
                    flip_y: vertical,
                    slide_x: vertical,
                    slide_y: !vertical,
                    resize_x: false,
                    resize_y: false,
                },
                ..PopoverPlacement::default()
            },
            popover_trigger: None,
        }
    }

    /// A value that changes whenever the popup's pixels would.
    pub fn paint_generation(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.text.hash(&mut hasher);
        self.font_family.hash(&mut hasher);
        for value in [self.size.0, self.size.1, self.opacity, self.scale] {
            value.to_bits().hash(&mut hasher);
        }
        format!("{:?}", self.style).hash(&mut hasher);
        hasher.finish()
    }
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

    fn dock_popup(side: PlacedSide) -> TooltipPopup {
        TooltipPopup {
            text: Arc::from("Firefox"),
            element: (100.0, 30.0, 164.0, 94.0),
            side,
            size: TOOLTIP,
            gap: 8.0,
            style: TooltipPaintStyle::DEFAULT_DARK,
            font_family: "Inter".into(),
            opacity: 1.0,
            scale: 1.0,
        }
    }

    #[test]
    fn a_tooltip_without_room_in_its_surface_is_promoted() {
        // A dock-like strip: 120px tall, element near the top, no reserve.
        let strip = (400.0, 120.0);
        let settings = TooltipSettings::default();
        let placement = compute_tooltip_placement(
            ResolvedAnchor::Top,
            Some((100.0, 30.0, 164.0, 94.0)),
            None,
            (0.0, 0.0),
            TOOLTIP,
            strip,
            1.0,
            &settings,
        );
        assert!(TooltipPopup::overflows(&placement, TOOLTIP, strip));

        // A bar with the overlay reserve below it keeps painting in place.
        let bar = (400.0, 248.0);
        let placement = compute_tooltip_placement(
            ResolvedAnchor::Bottom,
            Some((100.0, 4.0, 164.0, 40.0)),
            None,
            (0.0, 0.0),
            TOOLTIP,
            bar,
            1.0,
            &settings,
        );
        assert!(!TooltipPopup::overflows(&placement, TOOLTIP, bar));
    }

    #[test]
    fn promoted_tooltip_anchors_past_the_gap_and_flips_across_the_element() {
        let request = dock_popup(PlacedSide::Top).request();
        assert_eq!(request.kind, mesh_core_frontend_shell_adapter::ChildSurfaceKind::Tooltip);
        assert_eq!(request.node_key, TOOLTIP_CHILD_KEY);
        // The gap widens the anchor on the flip axis, since xdg_positioner
        // keeps the offset when it flips.
        assert_eq!(request.anchor_rect, (100, 22, 64, 80));
        assert_eq!(request.placement.offset_y, 0);
        assert_eq!(request.content_size, (120, 26));
        let adjust = request.placement.constraint_adjustment;
        assert!(adjust.flip_y && adjust.slide_x && !adjust.flip_x && !adjust.slide_y);
        assert_eq!(request.placement.anchor, mesh_core_elements::PopoverAnchor::Top);
        assert_eq!(request.placement.gravity, mesh_core_elements::PopoverGravity::Top);

        let side = dock_popup(PlacedSide::Right).request();
        assert_eq!(side.anchor_rect, (92, 30, 80, 64));
        assert!(side.placement.constraint_adjustment.flip_x);
    }

    #[test]
    fn promoted_tooltip_repaints_when_its_pixels_change() {
        let resting = dock_popup(PlacedSide::Top);
        let fading = TooltipPopup {
            opacity: 0.5,
            ..resting.clone()
        };
        assert_ne!(resting.paint_generation(), fading.paint_generation());
        assert_eq!(resting.paint_generation(), resting.clone().paint_generation());
    }

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
            .defaults_mut()
            .components
            .entry("tooltip".into())
            .or_default()
            .insert("animation".into(), shorthand.into());
        theme.keyframes.insert(
            "tooltip-enter".into(),
            vec![
                ThemeKeyframeStop {
                    offset: 0.0,
                    easing: None,
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
                    easing: None,
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
        theme.tokens_mut().insert(
            "animation.duration.short".into(),
            mesh_core_theme::TokenValue::Number(150.0),
        );
        let animation = tooltip_animation_from_theme(&theme).expect("animation lowered");
        assert_eq!(animation.duration, Duration::from_millis(150));
        assert_eq!(animation.easing, Easing::EaseOut);
    }

    #[test]
    fn theme_animation_honors_per_keyframe_easing() {
        let mut theme = theme_with_tooltip_animation("tooltip-enter 200ms linear");
        theme
            .keyframes
            .get_mut("tooltip-enter")
            .expect("tooltip keyframes")
            .first_mut()
            .expect("first tooltip stop")
            .easing = Some("ease-in".into());

        let animation = tooltip_animation_from_theme(&theme).expect("animation lowered");
        let mid = animation.sample(Duration::from_millis(100));
        // CSS `ease-in` maps half progress to ~.3153.
        assert!((mid.opacity - 0.3153).abs() < 5e-3);
        assert!((mid.scale - 0.6577).abs() < 5e-3);
        assert!((mid.translate_y - 6.847).abs() < 5e-2);
    }

    #[test]
    fn missing_keyframes_or_animation_means_no_animation() {
        let mut theme = theme_with_tooltip_animation("does-not-exist 100ms");
        assert!(tooltip_animation_from_theme(&theme).is_none());

        let tooltip_defaults = theme.defaults_mut().components.get_mut("tooltip").unwrap();
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
    fn every_accepted_tooltip_position_resolves_to_a_real_anchor() {
        // `mesh_core_config::TOOLTIP_POSITIONS` is what settings validation
        // accepts and what a diagnostic tells the user to type; this match is
        // what actually honors it. A value in the list that fell through to
        // `Auto` here would be advice that silently does nothing — the exact
        // failure the validation exists to remove.
        for position in mesh_core_config::TOOLTIP_POSITIONS {
            let settings = TooltipSettings {
                position: (*position).into(),
                ..TooltipSettings::default()
            };
            let resolved = effective_anchor(TooltipAnchor::Auto, &settings);
            if *position == "auto" {
                assert_eq!(resolved, ResolvedAnchor::Auto);
            } else {
                assert_ne!(
                    resolved,
                    ResolvedAnchor::Auto,
                    "accepted tooltip position '{position}' is not honored"
                );
            }
        }
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

    fn theme_with_tooltip_rule(declarations: &[(&str, &str)]) -> Theme {
        let mut theme = mesh_core_theme::default_theme();
        let rule = theme
            .defaults_mut()
            .components
            .entry("tooltip".into())
            .or_default();
        for (property, value) in declarations {
            rule.insert((*property).into(), (*value).into());
        }
        theme
    }

    #[test]
    fn tooltip_rule_styles_the_tooltip_chrome() {
        let mut theme = theme_with_tooltip_rule(&[
            ("background", "var(--tip-bg)"),
            ("color", "#112233"),
            ("border", "2px solid #445566"),
            ("border-radius", "10px"),
            ("padding", "6px 11px"),
            ("font-size", "14px"),
            ("font-weight", "bold"),
            ("max-width", "200px"),
        ]);
        theme.set_token(
            "tip.bg",
            mesh_core_theme::TokenValue::String("#ff000080".into()),
            mesh_core_theme::ThemeProvenance::BaseRecovery,
        );

        let style = tooltip_paint_style_from_theme(&theme);

        assert_eq!(style.background, Color::from_hex("#ff000080").unwrap());
        assert_eq!(style.foreground, Color::from_hex("#112233").unwrap());
        assert_eq!(style.border, Color::from_hex("#445566").unwrap());
        assert_eq!(style.border_width, 2.0);
        assert_eq!(style.radius, 10.0);
        assert_eq!((style.padding_y, style.padding_x), (6.0, 11.0));
        assert_eq!(style.font_size, 14.0);
        assert_eq!(style.font_weight, 700);
        assert_eq!(style.max_width, 200.0);
        assert_eq!(style.box_size(150.0, 18.0), (150.0 + 22.0, 18.0 + 12.0));
    }

    #[test]
    fn tooltip_metrics_stay_inside_the_overlay_reserve() {
        let theme = theme_with_tooltip_rule(&[
            ("padding", "40px"),
            ("border-width", "20px"),
            ("max-width", "900px"),
        ]);

        let style = tooltip_paint_style_from_theme(&theme);
        let widest = style.max_width + 2.0 * (style.padding_x + style.border_width);

        assert!(widest <= super::super::TOOLTIP_OVERLAY_WIDTH as f32);
    }

    #[test]
    fn tooltip_without_a_rule_uses_the_surface_container_roles() {
        let mut theme = mesh_core_theme::default_theme();
        theme.set_token(
            "color.surface-container",
            mesh_core_theme::TokenValue::String("#010203".into()),
            mesh_core_theme::ThemeProvenance::BaseRecovery,
        );

        let style = tooltip_paint_style_from_theme(&theme);

        assert_eq!(style.background, Color::from_hex("#010203").unwrap());
        assert_eq!(style.radius, TooltipPaintStyle::DEFAULT_DARK.radius);
    }
}
