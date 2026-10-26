use super::*;

/// The compositor-facing size of a parent surface whose content measures
/// `width` x `height`, together with the [`SurfacePadding`] that size implies.
///
/// **Both halves come from here, and nothing else may inflate a surface.** A
/// parent surface reserves extra logical pixels so tooltips can paint outside
/// its content box; those pixels are transparent, and a compositor routes
/// clicks over them to MESH unless the surface's input region says otherwise.
/// Returning the inflated size and the reserve as one value is what makes the
/// two impossible to change independently — the reserve is not an extra step
/// somebody can forget, it is the second half of the return type. `SurfaceEntry`
/// then derives the input region from the padding on every commit.
///
/// Window surfaces get no reserve. A toplevel's size *is* its content size — it
/// is what the compositor pins, decorates, tiles, and reports — so padding the
/// buffer would make the window measurably larger than the UI inside it. A
/// tooltip that needs to escape a window's bounds is an `xdg_popup`, the same
/// primitive `<popover>` already promotes to.
///
/// The reserve is trailing (content at the buffer origin), so it is dropped on
/// the axis of a bottom or right anchor: there it would sit between the content
/// and the screen edge and push a dock off that edge by the reserve. It is also
/// dropped on an axis the surface `spans` (its size there is whatever the
/// compositor configures): the configure already covers the output, so the
/// reserve could only come out of the content, leaving a full-screen overlay
/// short of the screen edge by the reserve.
///
/// When the compositor supports `xdg_popup` (`popup_supported`), there is no
/// reserve at all: a tooltip that leaves the content box is promoted to a
/// popup, so the reserve would only be transparent pixels every frame.
pub(super) fn surface_geometry_with_overlay_reserve(
    surface_id: &str,
    role: SurfaceRole,
    edge: Option<mesh_core_wayland::Edge>,
    width: u32,
    height: u32,
    spans: (bool, bool),
    popup_supported: bool,
) -> (u32, u32, SurfacePadding) {
    let (extra_w, extra_h) = if popup_supported
        || surface_id == DEBUG_INSPECTOR_SURFACE_ID
        || role == SurfaceRole::Window
    {
        (0, 0)
    } else {
        let (extra_w, extra_h) = component::tooltip_overlay_extra_for_content(width, height);
        let (extra_w, extra_h) = (
            if spans.0 { 0 } else { extra_w },
            if spans.1 { 0 } else { extra_h },
        );
        match edge {
            Some(mesh_core_wayland::Edge::Bottom) => (extra_w, 0),
            Some(mesh_core_wayland::Edge::Right) => (0, extra_h),
            _ => (extra_w, extra_h),
        }
    };
    (
        width.saturating_add(extra_w),
        height.saturating_add(extra_h),
        SurfacePadding::trailing(extra_w, extra_h),
    )
}

pub(super) fn map_popover_anchor(anchor: PopoverAnchor) -> PopupAnchor {
    match anchor {
        PopoverAnchor::Center => PopupAnchor::Center,
        PopoverAnchor::Top => PopupAnchor::Top,
        PopoverAnchor::Bottom => PopupAnchor::Bottom,
        PopoverAnchor::Left => PopupAnchor::Left,
        PopoverAnchor::Right => PopupAnchor::Right,
        PopoverAnchor::TopLeft => PopupAnchor::TopLeft,
        PopoverAnchor::TopRight => PopupAnchor::TopRight,
        PopoverAnchor::BottomLeft => PopupAnchor::BottomLeft,
        PopoverAnchor::BottomRight => PopupAnchor::BottomRight,
    }
}

pub(super) fn map_popover_gravity(gravity: PopoverGravity) -> PopupGravity {
    match gravity {
        PopoverGravity::Center => PopupGravity::Center,
        PopoverGravity::Top => PopupGravity::Top,
        PopoverGravity::Bottom => PopupGravity::Bottom,
        PopoverGravity::Left => PopupGravity::Left,
        PopoverGravity::Right => PopupGravity::Right,
        PopoverGravity::TopLeft => PopupGravity::TopLeft,
        PopoverGravity::TopRight => PopupGravity::TopRight,
        PopoverGravity::BottomLeft => PopupGravity::BottomLeft,
        PopoverGravity::BottomRight => PopupGravity::BottomRight,
    }
}

pub(super) fn map_popover_constraint(adjustment: PopoverConstraintAdjustment) -> PopupConstraint {
    PopupConstraint {
        flip_x: adjustment.flip_x,
        flip_y: adjustment.flip_y,
        slide_x: adjustment.slide_x,
        slide_y: adjustment.slide_y,
        resize_x: adjustment.resize_x,
        resize_y: adjustment.resize_y,
    }
}

/// Compute the logical-coordinate regions of all display list nodes
/// that have an active `backdrop-filter: blur(...)`.
///
/// Returns an empty vector when no nodes have `backdrop_filter.blur_radius > 0.0`,
/// which means no `kde_blur` protocol calls are emitted (BLUR-04).
#[cfg(test)]
pub(super) fn compute_blur_regions(commands: &[DisplayPaintCommand]) -> Vec<DamageRect> {
    mesh_core_render::display_list::backdrop_blur_regions(commands)
}

pub(super) fn compute_opaque_rect_for_root(
    commands: &[DisplayPaintCommand],
    surface_width: u32,
    surface_height: u32,
) -> Option<DamageRect> {
    let root = commands.first()?;
    let style = &root.node.style;

    if style.background_color.a != 255 {
        return None;
    }
    if style.background_paint != BackgroundPaint::None {
        return None;
    }
    if style.border_radius != mesh_core_elements::style::Corners::zero() {
        return None;
    }
    if !style.overflow_x.clips_contents() || !style.overflow_y.clips_contents() {
        return None;
    }

    Some(DamageRect {
        x: 0,
        y: 0,
        width: surface_width.max(1),
        height: surface_height.max(1),
    })
}

#[cfg(test)]
mod overlay_reserve_tests {
    use super::*;
    use mesh_core_wayland::Edge;

    #[test]
    fn reserve_is_dropped_on_the_axis_of_a_bottom_or_right_anchor() {
        let reserve = |edge| {
            let (w, h, padding) = surface_geometry_with_overlay_reserve(
                "@test/dock",
                SurfaceRole::Layer,
                edge,
                120,
                80,
                (false, false),
                false,
            );
            (w, h, padding.right, padding.bottom)
        };
        let (_, top_h, _, top_bottom) = reserve(Some(Edge::Top));
        assert!(
            top_h > 80 && top_bottom > 0,
            "a top bar keeps room for tooltips below it"
        );

        let (_, bottom_h, bottom_right, bottom_bottom) = reserve(Some(Edge::Bottom));
        assert_eq!(
            (bottom_h, bottom_bottom),
            (80, 0),
            "a dock must sit on the bottom edge"
        );
        assert!(
            bottom_right > 0,
            "the width reserve is unaffected by a bottom anchor"
        );

        let (right_w, _, right_right, right_bottom) = reserve(Some(Edge::Right));
        assert_eq!(
            (right_w, right_right),
            (120, 0),
            "a right panel must sit on the right edge"
        );
        assert!(right_bottom > 0);
    }

    #[test]
    fn reserve_is_dropped_on_an_axis_the_surface_spans() {
        let (_, height, padding) = surface_geometry_with_overlay_reserve(
            "@test/dashboard",
            SurfaceRole::Layer,
            Some(Edge::Top),
            2880,
            1800,
            (true, true),
            false,
        );
        assert_eq!(
            (height, padding.bottom),
            (1800, 0),
            "a full-output overlay keeps the whole configure for its content"
        );
        let (width, _, padding) = surface_geometry_with_overlay_reserve(
            "@test/narrow",
            SurfaceRole::Layer,
            Some(Edge::Top),
            100,
            40,
            (true, false),
            false,
        );
        assert_eq!((width, padding.right), (100, 0));
        assert!(padding.bottom > 0, "a fixed height keeps its reserve");
    }

    #[test]
    fn no_reserve_when_tooltips_can_be_popups() {
        let (width, height, padding) = surface_geometry_with_overlay_reserve(
            "@test/bar",
            SurfaceRole::Layer,
            Some(Edge::Top),
            120,
            48,
            (false, false),
            true,
        );
        assert_eq!((width, height), (120, 48));
        assert_eq!((padding.right, padding.bottom), (0, 0));
    }
}
