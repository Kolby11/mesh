use mesh_core_config::TooltipSettings;
use mesh_core_elements::style::TooltipAnchor;

/// Result of tooltip placement computation.
pub(super) struct TooltipPlacement {
    /// Final paint X coordinate (logical pixels).
    pub paint_x: f32,
    /// Final paint Y coordinate (logical pixels).
    pub paint_y: f32,
    /// Opacity for fade-in animation (0.0–1.0).
    pub opacity: f32,
}

/// Compute the effective tooltip anchor for an element, falling back to the
/// shell-wide default from settings.
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
            _ => ResolvedAnchor::Bottom, // "auto" default
        },
        TooltipAnchor::Bottom => ResolvedAnchor::Bottom,
        TooltipAnchor::Top => ResolvedAnchor::Top,
        TooltipAnchor::Left => ResolvedAnchor::Left,
        TooltipAnchor::Right => ResolvedAnchor::Right,
        TooltipAnchor::Cursor => ResolvedAnchor::Cursor,
    }
}

/// Resolved tooltip placement strategy after config and element preferences
/// have been combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResolvedAnchor {
    Bottom,
    Top,
    Left,
    Right,
    Cursor,
}

/// Compute tooltip position with screen-edge avoidance.
///
/// The tooltip size is a *maximum estimate* (the overlay reserve), not the
/// actual rendered box size. The renderer will measure the real text and do
/// its own final clamping, so we only need to flip the primary axis when the
/// preferred side would overflow. We intentionally avoid centering on the
/// horizontal axis — the renderer uses the X position as the left edge and
/// left-aligns text, matching the original behavior.
pub(super) fn compute_tooltip_placement(
    anchor: ResolvedAnchor,
    element_bounds: Option<(f32, f32, f32, f32)>,
    cursor: (f32, f32),
    tooltip_max_size: (f32, f32),
    surface_size: (f32, f32),
    opacity: f32,
    config: &TooltipSettings,
) -> TooltipPlacement {
    let gap = config.gap;
    let (_tw, th) = tooltip_max_size;
    let (_sw, sh) = surface_size;

    match anchor {
        ResolvedAnchor::Cursor => TooltipPlacement {
            paint_x: cursor.0 + config.cursor_offset_x,
            paint_y: cursor.1 + config.cursor_offset_y,
            opacity,
        },
        ResolvedAnchor::Bottom
        | ResolvedAnchor::Top
        | ResolvedAnchor::Left
        | ResolvedAnchor::Right => {
            let Some((el_l, el_t, el_r, el_b)) = element_bounds else {
                // No element bounds → fall back to cursor positioning.
                return TooltipPlacement {
                    paint_x: cursor.0 + config.cursor_offset_x,
                    paint_y: cursor.1 + config.cursor_offset_y,
                    opacity,
                };
            };

            // Element at origin likely means unmeasured → cursor fallback.
            if el_l == 0.0 && el_r == 0.0 {
                return TooltipPlacement {
                    paint_x: cursor.0 + config.cursor_offset_x,
                    paint_y: cursor.1 + config.cursor_offset_y,
                    opacity,
                };
            }

            let (px, py) = match anchor {
                ResolvedAnchor::Bottom => {
                    // Horizontally centered on the element, always below it.
                    // No flip: the tooltip lives in the overlay area below the
                    // content regardless of surface boundaries — render_tooltip
                    // clamps within the pixel buffer.
                    let x = (el_l + el_r) / 2.0;
                    let y = el_b + gap;
                    (x, y.max(0.0))
                }
                ResolvedAnchor::Top => {
                    // Horizontally centered on the element, above it.
                    let x = (el_l + el_r) / 2.0;
                    let y = el_t - gap - th;
                    // Flip to below if it would overflow the top.
                    if y < 0.0 { (x, el_b + gap) } else { (x, y) }
                }
                ResolvedAnchor::Left => {
                    // Vertically centered, to the left of element.
                    let x = el_l - gap - tooltip_max_size.0;
                    let y = (el_t + el_b) / 2.0 - th / 2.0;
                    // Flip to right if it would overflow the left.
                    if x < 0.0 { (el_r + gap, y) } else { (x, y) }
                }
                ResolvedAnchor::Right => {
                    // Vertically centered, to the right of element.
                    let x = el_r + gap;
                    let y = (el_t + el_b) / 2.0 - th / 2.0;
                    // Flip to left if it would overflow the right.
                    if x + tooltip_max_size.0 > surface_size.0 {
                        (el_l - gap - tooltip_max_size.0, y)
                    } else {
                        (x, y)
                    }
                }
                ResolvedAnchor::Cursor => unreachable!(),
            };

            TooltipPlacement {
                paint_x: px,
                paint_y: py,
                opacity,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_place_tooltip_below_element() {
        let settings = TooltipSettings::default();
        let anchor = effective_anchor(TooltipAnchor::Auto, &settings);

        let placement = compute_tooltip_placement(
            anchor,
            Some((10.0, 20.0, 50.0, 44.0)),
            (18.0, 28.0),
            (120.0, 32.0),
            (240.0, 160.0),
            1.0,
            &settings,
        );

        assert_eq!(anchor, ResolvedAnchor::Bottom);
        // paint_x is the element center: (10 + 50) / 2 = 30. The renderer
        // centers the tooltip box around this X coordinate.
        assert_eq!(placement.paint_x, 30.0);
        assert_eq!(placement.paint_y, 50.0);
    }

    #[test]
    fn bottom_placement_uses_expanded_paint_surface_before_flipping() {
        let settings = TooltipSettings::default();
        let anchor = effective_anchor(TooltipAnchor::Auto, &settings);

        let placement = compute_tooltip_placement(
            anchor,
            Some((10.0, 56.0, 50.0, 80.0)),
            (18.0, 68.0),
            (120.0, 32.0),
            (240.0, 160.0),
            1.0,
            &settings,
        );

        assert_eq!(anchor, ResolvedAnchor::Bottom);
        assert_eq!(placement.paint_x, 30.0);
        assert_eq!(placement.paint_y, 86.0);
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
            (18.0, 28.0),
            (120.0, 32.0),
            (240.0, 160.0),
            0.5,
            &settings,
        );

        assert_eq!(anchor, ResolvedAnchor::Cursor);
        assert_eq!(placement.paint_x, 22.0);
        assert_eq!(placement.paint_y, 37.0);
        assert_eq!(placement.opacity, 0.5);
    }
}
