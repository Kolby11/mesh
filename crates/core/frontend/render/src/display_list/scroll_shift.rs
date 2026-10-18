//! In-place scroll of a retained scope's composed viewport pixels.
//!
//! When nothing beneath a viewport varies across it (only full-cover solid
//! fills), the composed pixels inside the viewport are the content's pixels
//! over a constant colour, and an integral device-pixel scroll moves them
//! exactly. Only the exposed strips and the parts anchored to the viewport
//! (owner border and corner bands, scrollbars, overlays painted above the
//! scope) need replay. Anything else keeps the ordinary viewport repaint.

use smallvec::SmallVec;

use mesh_core_elements::NodeId;
use mesh_core_elements::style::BackgroundPaint;

use super::build::command_bounds;
use super::signature::clip_rect;
use super::sequence::PaintSequence;
use super::types::*;

/// A scroll scope whose composed viewport pixels can move in place.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollShift {
    pub owner: NodeId,
    /// The viewport in logical surface coordinates.
    pub viewport: DamageRect,
    /// New minus previous scroll offset, in logical pixels. Content moves by
    /// the negated amount.
    pub scroll_delta: (f32, f32),
    /// Logical rects inside the viewport that stay put while content moves
    /// and must be replayed after the shift, in addition to exposed strips.
    pub anchored: SmallVec<[DamageRect; 8]>,
    /// Logical surface rows (`[start, end)`) holding rounded-corner arcs of
    /// content at the new offset. Skia's anti-aliasing of an arc depends on
    /// whether the clip cuts it, so a replayed band should not end inside
    /// one: its pixels would then differ from the moved ones.
    pub arc_rows: Vec<(f32, f32)>,
    /// Logical surface columns holding rounded-corner arcs; see `arc_rows`.
    pub arc_columns: Vec<(f32, f32)>,
}

pub(super) fn plan_scroll_shift(
    commands: &PaintSequence<DisplayPaintCommand>,
    owner: NodeId,
    viewport: DamageRect,
    scroll_delta: (f32, f32),
) -> Option<ScrollShift> {
    if viewport.width == 0 || viewport.height == 0 {
        return None;
    }
    // A vertical move leaves everything that is constant down a column in
    // place (side borders, side edges), and a horizontal move everything
    // constant along a row, so only bands across the direction of motion
    // and corners need replay.
    let axes = MotionAxes {
        x: scroll_delta.0 != 0.0,
        y: scroll_delta.1 != 0.0,
    };
    let mut layer_depth = 0u32;
    let mut push_index = None;
    let mut anchored = SmallVec::<[DamageRect; 8]>::new();
    for (index, command) in commands.iter().enumerate() {
        match command.kind {
            DisplayPaintCommandKind::PushScrollContent if command.node.id == owner => {
                if layer_depth != 0 {
                    return None;
                }
                push_index = Some(index);
                break;
            }
            DisplayPaintCommandKind::PushCompositingLayer
            | DisplayPaintCommandKind::PushFilterLayer => layer_depth += 1,
            DisplayPaintCommandKind::PopCompositingLayer
            | DisplayPaintCommandKind::PopFilterLayer => {
                layer_depth = layer_depth.saturating_sub(1)
            }
            _ => {}
        }
        if command_bounds(command).intersects(viewport) {
            anchored.extend(beneath_varying_parts(command, owner, viewport, axes)?);
        }
    }
    let push_index = push_index?;

    let mut depth = 0u32;
    let mut pop_index = None;
    for (index, command) in commands.iter().enumerate().skip(push_index) {
        match command.kind {
            DisplayPaintCommandKind::PushScrollContent => depth += 1,
            DisplayPaintCommandKind::PopScrollContent => {
                depth -= 1;
                if depth == 0 {
                    pop_index = Some(index);
                    break;
                }
            }
            DisplayPaintCommandKind::Node | DisplayPaintCommandKind::Scrollbars => {}
            // Layers and backdrops inside the content are clipped or sampled
            // at the viewport edge, so their pixels are not shift-invariant.
            _ => return None,
        }
    }
    let pop_index = pop_index?;
    let (scroll_x, scroll_y) = commands
        .iter()
        .nth(push_index)
        .map(|push| (push.node.scrollbars.scroll_x, push.node.scrollbars.scroll_y))
        .unwrap_or_default();
    let mut arc_rows = Vec::new();
    let mut arc_columns = Vec::new();
    for command in commands.iter().take(pop_index).skip(push_index + 1) {
        let style = &command.node.style;
        let paints_edge = style.background_color.a > 0
            || !matches!(style.background_paint, BackgroundPaint::None)
            || style.border_color.a > 0;
        let corners = style.border_radius;
        if command.kind != DisplayPaintCommandKind::Node || !paints_edge {
            continue;
        }
        let layout = command.node.layout;
        let (x, y) = (layout.x - scroll_x, layout.y - scroll_y);
        let top = corners.top_left.max(corners.top_right);
        let bottom = corners.bottom_left.max(corners.bottom_right);
        let left = corners.top_left.max(corners.bottom_left);
        let right = corners.top_right.max(corners.bottom_right);
        let visible_rows = |a: f32, b: f32| b > viewport.y as f32 && a < viewport.bottom() as f32;
        let visible_columns =
            |a: f32, b: f32| b > viewport.x as f32 && a < viewport.right() as f32;
        if top > 0.0 && visible_rows(y, y + top) {
            arc_rows.push((y, y + top));
        }
        if bottom > 0.0 && visible_rows(y + layout.height - bottom, y + layout.height) {
            arc_rows.push((y + layout.height - bottom, y + layout.height));
        }
        if left > 0.0 && visible_columns(x, x + left) {
            arc_columns.push((x, x + left));
        }
        if right > 0.0 && visible_columns(x + layout.width - right, x + layout.width) {
            arc_columns.push((x + layout.width - right, x + layout.width));
        }
    }

    anchored.extend(owner_edge_bands(commands, push_index, owner, viewport, axes));
    for command in commands.iter().skip(pop_index + 1) {
        let bounds = command_bounds(command);
        if !bounds.intersects(viewport) {
            continue;
        }
        // A backdrop above the scope samples the moving content beyond its
        // own bounds; that case keeps the full repaint.
        if matches!(
            command.kind,
            DisplayPaintCommandKind::ApplyBackdropFilterCompositor
                | DisplayPaintCommandKind::ApplyBackdropFilterInSurface
                | DisplayPaintCommandKind::ApplyBackdropRefraction
        ) {
            return None;
        }
        if command.kind == DisplayPaintCommandKind::Scrollbars {
            anchored.extend(scrollbar_tracks(command, bounds, viewport));
        } else {
            anchored.extend(clip_rect(bounds, viewport));
        }
    }
    Some(ScrollShift {
        owner,
        viewport,
        scroll_delta,
        anchored,
        arc_rows,
        arc_columns,
    })
}

/// The viewport parts where `command`, painted before the scope, does not
/// contribute one colour: its border bands and rounded corners. `None` when
/// it varies anywhere else (partial cover, gradients, images, shadows,
/// transforms, content). The owner's own border and corners are covered by
/// [`owner_edge_bands`].
fn beneath_varying_parts(
    command: &DisplayPaintCommand,
    owner: NodeId,
    viewport: DamageRect,
    axes: MotionAxes,
) -> Option<SmallVec<[DamageRect; 8]>> {
    if command.kind != DisplayPaintCommandKind::Node
        || command.node.requires_affine_paint()
        || command.node.in_scroll_content
        || !matches!(command.node.content, DisplayPaintContent::None)
        || !clip_contains(command.clip, viewport)
    {
        return None;
    }
    let style = &command.node.style;
    let layout = command.node.layout;
    if !matches!(style.background_paint, BackgroundPaint::None)
        || layout.x > viewport.x as f32
        || layout.y > viewport.y as f32
        || layout.x + layout.width < viewport.right() as f32
        || layout.y + layout.height < viewport.bottom() as f32
    {
        return None;
    }
    let mut parts = SmallVec::new();
    if command.node.id == owner {
        return (style.box_shadow.is_none() || !style.box_shadow.inset).then_some(parts);
    }
    if !style.box_shadow.is_none() {
        return None;
    }
    let left = layout.x.floor().max(0.0) as u32;
    let top = layout.y.floor().max(0.0) as u32;
    let right = (layout.x + layout.width).ceil().max(0.0) as u32;
    let bottom = (layout.y + layout.height).ceil().max(0.0) as u32;
    let band = |side: f32| (side.max(0.0).ceil() as u32).saturating_add(1);
    let border = style.border_width;
    if style.border_color.a > 0 {
        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        let mut push_band = |x: u32, y: u32, width: u32, height: u32| {
            parts.extend(clip_rect(
                DamageRect {
                    x,
                    y,
                    width,
                    height,
                },
                viewport,
            ));
        };
        if axes.y && border.top > 0.0 {
            push_band(left, top, width, band(border.top));
        }
        if axes.y && border.bottom > 0.0 {
            let depth = band(border.bottom);
            push_band(left, bottom.saturating_sub(depth), width, depth);
        }
        if axes.x && border.left > 0.0 {
            push_band(left, top, band(border.left), height);
        }
        if axes.x && border.right > 0.0 {
            let depth = band(border.right);
            push_band(right.saturating_sub(depth), top, depth, height);
        }
    }
    let radius = max_radius(style.border_radius);
    if radius > 0.0 {
        let size = band(radius);
        for (x, y) in [
            (left, top),
            (right.saturating_sub(size), top),
            (left, bottom.saturating_sub(size)),
            (right.saturating_sub(size), bottom.saturating_sub(size)),
        ] {
            parts.extend(clip_rect(
                DamageRect {
                    x,
                    y,
                    width: size,
                    height: size,
                },
                viewport,
            ));
        }
    }
    Some(parts)
}

/// Bands along each viewport edge as deep as the owner's border or corner
/// radius on that side, plus one pixel for edge anti-aliasing.
fn owner_edge_bands(
    commands: &PaintSequence<DisplayPaintCommand>,
    push_index: usize,
    owner: NodeId,
    viewport: DamageRect,
    axes: MotionAxes,
) -> SmallVec<[DamageRect; 8]> {
    let (border, radius) = commands
        .iter()
        .take(push_index)
        .filter(|command| command.node.id == owner)
        .last()
        .map(|command| {
            let border = command.node.style.border_width;
            (
                [border.top, border.right, border.bottom, border.left],
                max_radius(command.node.style.border_radius),
            )
        })
        .unwrap_or_default();
    let depth = |side: f32| (side.max(radius).max(0.0).ceil() as u32).saturating_add(1);
    let (top, right, bottom, left) = (
        depth(border[0]).min(viewport.height),
        depth(border[1]).min(viewport.width),
        depth(border[2]).min(viewport.height),
        depth(border[3]).min(viewport.width),
    );
    // Rounded corners lie inside the bands across the motion: each band is
    // at least as deep as the largest radius.
    let mut bands = SmallVec::new();
    if axes.y {
        bands.push(DamageRect {
            height: top,
            ..viewport
        });
        bands.push(DamageRect {
            y: viewport.bottom() - bottom,
            height: bottom,
            ..viewport
        });
    }
    if axes.x {
        bands.push(DamageRect {
            width: left,
            ..viewport
        });
        bands.push(DamageRect {
            x: viewport.right() - right,
            width: right,
            ..viewport
        });
    }
    bands
}

#[derive(Debug, Clone, Copy)]
struct MotionAxes {
    x: bool,
    y: bool,
}

/// Logical depth of the strip holding a scrollbar along its viewport edge:
/// the painter's 4px inset plus 6px track, with a pixel either side for
/// device rounding.
const SCROLLBAR_STRIP: u32 = 12;

/// The edge strips a scroll owner's scrollbars paint into, instead of the
/// owner's whole box.
fn scrollbar_tracks(
    command: &DisplayPaintCommand,
    bounds: DamageRect,
    viewport: DamageRect,
) -> SmallVec<[DamageRect; 2]> {
    let node = &command.node;
    let vertical = node.style.overflow_y.always_shows_scrollbar()
        || (node.style.overflow_y.shows_scrollbar_when_overflowing()
            && node.scrollbars.max_y > f32::EPSILON);
    let horizontal = node.style.overflow_x.always_shows_scrollbar()
        || (node.style.overflow_x.shows_scrollbar_when_overflowing()
            && node.scrollbars.max_x > f32::EPSILON);
    let mut tracks = SmallVec::new();
    if vertical {
        let width = SCROLLBAR_STRIP.min(bounds.width);
        tracks.extend(clip_rect(
            DamageRect {
                x: bounds.right() - width,
                width,
                ..bounds
            },
            viewport,
        ));
    }
    if horizontal {
        let height = SCROLLBAR_STRIP.min(bounds.height);
        tracks.extend(clip_rect(
            DamageRect {
                y: bounds.bottom() - height,
                height,
                ..bounds
            },
            viewport,
        ));
    }
    tracks
}

fn max_radius(corners: mesh_core_elements::style::Corners) -> f32 {
    corners
        .top_left
        .max(corners.top_right)
        .max(corners.bottom_right)
        .max(corners.bottom_left)
}

fn clip_contains(clip: DisplayListClip, rect: DamageRect) -> bool {
    clip.x as i64 <= rect.x as i64
        && clip.y as i64 <= rect.y as i64
        && clip.x as i64 + clip.width as i64 >= rect.right() as i64
        && clip.y as i64 + clip.height as i64 >= rect.bottom() as i64
}
