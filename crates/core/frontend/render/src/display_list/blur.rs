use mesh_core_elements::style::{BackgroundPaint, Corners};
use mesh_core_elements::{
    AffineTransform, LayoutRect, WidgetNode, child_transform, node_layout_bounds, node_transform,
    root_transform,
};

use super::build::*;
use super::signature::*;
use super::types::*;
use crate::RenderObjectDirtySummary;

/// Compositor blur regions read off the widget tree, not `paint_commands`: a
/// scoped update rebuilds only the dirty subtree, so deriving from commands
/// intermittently yields an empty set, which `org_kde_kwin_blur` reads as "blur
/// the whole surface". `offset_x`/`offset_y` must be the paint origin the
/// display list was built with, or the rects miss the painted content.
pub fn backdrop_blur_regions_from_tree(
    root: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    surface: DamageRect,
) -> Vec<DamageRect> {
    let mut regions = Vec::new();
    collect_backdrop_blur_regions(root, offset_x, offset_y, surface, &mut regions);
    regions
}

pub(super) fn collect_backdrop_blur_regions(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    surface: DamageRect,
    regions: &mut Vec<DamageRect>,
) {
    collect_backdrop_blur_regions_with_transform(
        node,
        root_transform(offset_x, offset_y),
        surface,
        regions,
    );
}

fn collect_backdrop_blur_regions_with_transform(
    node: &WidgetNode,
    parent_transform: AffineTransform,
    surface: DamageRect,
    regions: &mut Vec<DamageRect>,
) {
    if node_is_explicitly_hidden(node) {
        return;
    }
    let world_transform = node_transform(parent_transform, node);

    if node.computed_style.backdrop_filter.blur_radius > 0.0
        && node.layout.width > 0.0
        && node.layout.height > 0.0
    {
        let layout = node_layout_bounds(node, world_transform);
        if let Some(bounds) = blur_bounds_from_layout(layout) {
            for rect in rounded_region_bands(bounds, node.computed_style.border_radius, surface) {
                if !regions.contains(&rect) {
                    regions.push(rect);
                }
            }
        }
    }

    let scroll = node.resolved_scroll_metrics();
    let child_transform = child_transform(world_transform, node, scroll.x, scroll.y);
    for child in &node.children {
        collect_backdrop_blur_regions_with_transform(child, child_transform, surface, regions);
    }
}

/// Regions for nodes with an active `backdrop-filter`. Disjoint nodes stay
/// separate so the compositor is not asked to blur transparent gaps between
/// popup items. Negative origins clamp to 0 with the clipped leading edge
/// subtracted, so partially off-screen nodes do not snap to the corner.
pub fn backdrop_blur_regions(commands: &[DisplayPaintCommand]) -> Vec<DamageRect> {
    let mut regions = Vec::new();
    for cmd in commands {
        if cmd.node.style.backdrop_filter.blur_radius <= 0.0 {
            continue;
        }
        for rect in rounded_blur_regions(cmd) {
            if !regions.contains(&rect) {
                regions.push(rect);
            }
        }
    }
    regions
}

/// Clamp a layout rect to integer `wl_region` bounds, matching the painter's
/// ceil/floor rounding. `None` for degenerate rects.
pub(super) fn blur_bounds_from_layout(layout: LayoutRect) -> Option<DamageRect> {
    let left = layout.x.max(0.0).ceil() as u32;
    let top = layout.y.max(0.0).ceil() as u32;
    let right = (layout.x + layout.width).max(0.0).floor() as u32;
    let bottom = (layout.y + layout.height).max(0.0).floor() as u32;
    if right <= left || bottom <= top {
        return None;
    }
    Some(DamageRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// Approximate a rounded painted shape with `wl_region` rectangles, so a fully
/// rounded 36×36 node masks as a circle. Rectangular nodes stay one rect.
pub(super) fn rounded_blur_regions(command: &DisplayPaintCommand) -> Vec<DamageRect> {
    let Some(bounds) = blur_bounds_from_layout(
        command
            .node
            .transform
            .transform_rect(command.node.local_layout),
    ) else {
        return Vec::new();
    };
    let clip = DamageRect {
        x: command.clip.x.max(0) as u32,
        y: command.clip.y.max(0) as u32,
        width: command.clip.width.max(0) as u32,
        height: command.clip.height.max(0) as u32,
    };
    rounded_region_bands(bounds, command.node.style.border_radius, clip)
}

/// Decompose a rounded rectangle into `clip`-bounded horizontal bands.
pub(super) fn rounded_region_bands(
    bounds: DamageRect,
    corners: Corners,
    clip: DamageRect,
) -> Vec<DamageRect> {
    let corners = normalize_corners(bounds.width as f32, bounds.height as f32, corners);
    if corners == Corners::zero() {
        return intersect_damage_rect(bounds, clip).into_iter().collect();
    }

    let mut bands: Vec<DamageRect> = Vec::new();
    for row in 0..bounds.height {
        let center_y = row as f32 + 0.5;
        let top_distance = center_y;
        let bottom_distance = bounds.height as f32 - center_y;
        let left_radius = if top_distance < corners.top_left {
            corner_inset(corners.top_left, top_distance)
        } else if bottom_distance < corners.bottom_left {
            corner_inset(corners.bottom_left, bottom_distance)
        } else {
            0.0
        };
        let right_radius = if top_distance < corners.top_right {
            corner_inset(corners.top_right, top_distance)
        } else if bottom_distance < corners.bottom_right {
            corner_inset(corners.bottom_right, bottom_distance)
        } else {
            0.0
        };
        let left_inset = left_radius.ceil() as u32;
        let right_inset = right_radius.ceil() as u32;
        if left_inset.saturating_add(right_inset) >= bounds.width {
            continue;
        }
        let row_rect = DamageRect {
            x: bounds.x + left_inset,
            y: bounds.y + row,
            width: bounds.width - left_inset - right_inset,
            height: 1,
        };
        let Some(row_rect) = intersect_damage_rect(row_rect, clip) else {
            continue;
        };
        if let Some(previous) = bands.last_mut()
            && previous.x == row_rect.x
            && previous.width == row_rect.width
            && previous.y + previous.height == row_rect.y
        {
            previous.height += 1;
        } else {
            bands.push(row_rect);
        }
    }
    bands
}

fn corner_inset(radius: f32, distance: f32) -> f32 {
    let radius = radius.max(0.0);
    if radius <= 0.0 || distance >= radius {
        return 0.0;
    }
    let offset = radius - distance;
    radius - (radius * radius - offset * offset).max(0.0).sqrt()
}

fn normalize_corners(width: f32, height: f32, corners: Corners) -> Corners {
    let mut corners = Corners {
        top_left: corners.top_left.max(0.0),
        top_right: corners.top_right.max(0.0),
        bottom_right: corners.bottom_right.max(0.0),
        bottom_left: corners.bottom_left.max(0.0),
    };
    let scale = [
        width / (corners.top_left + corners.top_right).max(1.0),
        width / (corners.bottom_left + corners.bottom_right).max(1.0),
        height / (corners.top_left + corners.bottom_left).max(1.0),
        height / (corners.top_right + corners.bottom_right).max(1.0),
    ]
    .into_iter()
    .fold(1.0, f32::min)
    .min(1.0);
    corners.top_left *= scale;
    corners.top_right *= scale;
    corners.bottom_right *= scale;
    corners.bottom_left *= scale;
    corners
}

pub(super) fn intersect_damage_rect(left: DamageRect, right: DamageRect) -> Option<DamageRect> {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = left
        .x
        .saturating_add(left.width)
        .min(right.x.saturating_add(right.width));
    let bottom_edge = left
        .y
        .saturating_add(left.height)
        .min(right.y.saturating_add(right.height));
    if right_edge <= x || bottom_edge <= y {
        return None;
    }
    Some(DamageRect {
        x,
        y,
        width: right_edge - x,
        height: bottom_edge - y,
    })
}

/// The layout rect inflated by the blur kernel reach (3x radius, matching the
/// painter's `apply_backdrop_filter_impl` pad) and clipped to the surface.
pub(super) fn backdrop_read_region(
    node: &DisplayPaintNode,
    surface: DamageRect,
) -> Option<DamageRect> {
    let radius = node.style.backdrop_filter.blur_radius;
    if radius <= 0.0 {
        return None;
    }
    let pad = radius * 3.0;
    let layout = node.transform.transform_rect(node.local_layout);
    let left = layout.x - pad;
    let top = layout.y - pad;
    let right = layout.x + layout.width + pad;
    let bottom = layout.y + layout.height + pad;
    let x = left.max(0.0).floor() as u32;
    let y = top.max(0.0).floor() as u32;
    let right = right.max(0.0).ceil() as u32;
    let bottom = bottom.max(0.0).ceil() as u32;
    clip_rect(
        DamageRect {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        },
        surface,
    )
}

/// Whether replaying this command writes pixels. Conservative: over-reporting
/// only costs an identity blur pass.
pub(super) fn display_command_paints_pixels(command: &DisplayPaintCommand) -> bool {
    if !command.kind.draws_content() {
        return false;
    }
    if command.kind == DisplayPaintCommandKind::Scrollbars {
        return true;
    }
    let style = &command.node.style;
    !matches!(command.node.content, DisplayPaintContent::None)
        || style.background_color.a > 0
        || !matches!(style.background_paint, BackgroundPaint::None)
        || (style.border_color.a > 0
            && [
                style.border_width.top,
                style.border_width.right,
                style.border_width.bottom,
                style.border_width.left,
            ]
            .into_iter()
            .any(|width| width > 0.0))
        || (!style.box_shadow.is_none() && !style.box_shadow.inset)
        || style.backdrop_filter.blur_radius > 0.0
}

/// Read regions of backdrop-filter nodes with content beneath them in paint
/// order. A node with nothing beneath contributes no region, so an empty
/// in-surface backdrop never widens sparse damage.
pub(super) fn compute_backdrop_regions(
    commands: &[DisplayPaintCommand],
    surface: DamageRect,
) -> Vec<DamageRect> {
    let mut regions = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        if command.kind != DisplayPaintCommandKind::Node {
            continue;
        }
        let Some(region) = backdrop_read_region(&command.node, surface) else {
            continue;
        };
        let has_backdrop_content = commands[..index].iter().any(|earlier| {
            display_command_paints_pixels(earlier) && command_bounds(earlier).intersects(region)
        });
        if has_backdrop_content {
            regions.push(region);
        }
    }
    regions
}

/// Half-open command range per layer. Ranges nest but never interleave, since
/// they come from a tree walk.
pub(super) fn collect_layer_scopes(commands: &[DisplayPaintCommand]) -> Vec<(usize, usize)> {
    let mut scopes = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        match command.kind {
            DisplayPaintCommandKind::PushCompositingLayer
            | DisplayPaintCommandKind::PushFilterLayer => open.push(index),
            DisplayPaintCommandKind::PopCompositingLayer
            | DisplayPaintCommandKind::PopFilterLayer => {
                if let Some(start) = open.pop() {
                    scopes.push((start, index.saturating_add(1)));
                }
            }
            _ => {}
        }
    }
    scopes.sort_unstable();
    scopes
}

/// Taken from each push command's clip, already inflated by the kernel reach.
pub(super) fn filter_layer_regions(
    commands: &[DisplayPaintCommand],
    scopes: &[(usize, usize)],
    surface: DamageRect,
) -> Vec<DamageRect> {
    scopes
        .iter()
        .filter_map(|&(start, _)| {
            let push = commands.get(start)?;
            if push.kind != DisplayPaintCommandKind::PushFilterLayer {
                return None;
            }
            clip_rect(
                DamageRect {
                    x: push.clip.x.max(0) as u32,
                    y: push.clip.y.max(0) as u32,
                    width: push.clip.width.max(0) as u32,
                    height: push.clip.height.max(0) as u32,
                },
                surface,
            )
        })
        .collect()
}

pub(super) fn command_has_effect_overflow(command: &DisplayPaintCommand) -> bool {
    command.kind == DisplayPaintCommandKind::Node
        && visual_clip_for(&command.node) != node_clip_for(&command.node)
}

#[cfg(test)]
pub(super) fn count_effect_overflow_commands(commands: &[DisplayPaintCommand]) -> u64 {
    commands
        .iter()
        .filter(|command| command_has_effect_overflow(command))
        .count() as u64
}

pub(super) fn changed_layout_count(dirty_summary: RenderObjectDirtySummary) -> u64 {
    [
        dirty_summary.inserted,
        dirty_summary.removed,
        dirty_summary.reordered,
        dirty_summary.transform,
        dirty_summary.clip,
        dirty_summary.geometry,
    ]
    .into_iter()
    .map(|count| count as u64)
    .sum()
}

pub(super) fn dirty_summary_preserves_blur_metadata(dirty: RenderObjectDirtySummary) -> bool {
    dirty.inserted == 0
        && dirty.removed == 0
        && dirty.reordered == 0
        && dirty.transform == 0
        && dirty.clip == 0
        && dirty.opacity == 0
        && dirty.geometry == 0
        && dirty.material == 0
        && dirty.primitive == 0
}

pub(super) fn changed_paint_count(dirty_summary: RenderObjectDirtySummary) -> u64 {
    [
        dirty_summary.opacity,
        dirty_summary.material,
        dirty_summary.primitive,
        dirty_summary.text,
    ]
    .into_iter()
    .map(|count| count as u64)
    .sum()
}
