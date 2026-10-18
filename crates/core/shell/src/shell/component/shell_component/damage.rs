use super::*;
#[cfg(test)]
use mesh_core_elements::{InteractionTarget, node_eligibility};

pub(super) fn scale_damage_rect_to_buffer(
    rect: DamageRect,
    scale: f32,
    buffer_width: u32,
    buffer_height: u32,
) -> DamageRect {
    mesh_core_render::FractionalScale::new(scale)
        .clip_damage_rect(rect, buffer_width, buffer_height)
        .unwrap_or_default()
}

pub(super) fn resolve_tooltip_colors(theme: &Theme) -> mesh_core_render::TooltipPaintColors {
    let fallback = mesh_core_render::TooltipPaintColors::DEFAULT_DARK;
    mesh_core_render::TooltipPaintColors {
        background: token_color(theme, "color.surface-container", fallback.background),
        border: token_color(theme, "color.surface-container-high", fallback.border),
        foreground: token_color(theme, "color.on-surface", fallback.foreground),
    }
}

pub(super) fn token_color(
    theme: &Theme,
    key: &str,
    fallback: mesh_core_elements::style::Color,
) -> mesh_core_elements::style::Color {
    theme
        .token(key)
        .and_then(|value| match value {
            mesh_core_theme::TokenValue::String(s) => mesh_core_elements::style::Color::from_hex(s),
            _ => None,
        })
        .unwrap_or(fallback)
}

#[cfg(test)]
pub(super) fn select_effective_damage(
    metrics: DisplayListMetrics,
    surface: DamageRect,
    requires_tree_rebuild: bool,
    reorder_damage: Option<DamageRect>,
    tooltip_damage: Option<DamageRect>,
) -> EffectiveDamage {
    let extra_damage = reorder_damage.into_iter().collect::<Vec<_>>();
    let tooltip_damage = tooltip_damage.into_iter().collect::<Vec<_>>();
    select_effective_damage_rects(
        metrics,
        &[],
        surface,
        requires_tree_rebuild,
        &extra_damage,
        &tooltip_damage,
        Vec::new(),
    )
}

pub(super) fn select_effective_damage_rects(
    metrics: DisplayListMetrics,
    base_damage: &[DamageRect],
    surface: DamageRect,
    requires_tree_rebuild: bool,
    extra_damage: &[DamageRect],
    tooltip_damage: &[DamageRect],
    mut rects: Vec<DamageRect>,
) -> EffectiveDamage {
    rects.clear();
    if metrics.full_surface_damage {
        rects.push(surface);
        return EffectiveDamage {
            rect: Some(surface),
            rects,
            full_surface: true,
            policy: DisplayListRepaintPolicy::FullSurface,
        };
    }

    let has_extra_damage_sources = !extra_damage.is_empty() || !tooltip_damage.is_empty();
    if base_damage.is_empty() {
        if metrics.damage_area > 0 {
            push_damage_rect(&mut rects, metrics.damage_rect, surface);
        }
    } else {
        merge_damage_rects(&mut rects, base_damage.iter().copied(), surface);
    }
    merge_damage_rects(&mut rects, extra_damage.iter().copied(), surface);
    merge_damage_rects(&mut rects, tooltip_damage.iter().copied(), surface);

    let Some(damage) = bounding_damage_rect(&rects, surface) else {
        return EffectiveDamage::none();
    };

    let damage_area = damage_rects_area(&rects);
    let policy = if !tooltip_damage.is_empty() {
        DisplayListRepaintPolicy::FullSurface
    } else {
        select_damage_policy(
            metrics,
            requires_tree_rebuild,
            has_extra_damage_sources,
            damage_area,
        )
    };
    match policy {
        DisplayListRepaintPolicy::MinimalDamage | DisplayListRepaintPolicy::BoundingRect => {
            EffectiveDamage {
                rect: Some(damage),
                rects,
                full_surface: false,
                policy,
            }
        }
        DisplayListRepaintPolicy::FullSurface => {
            rects.clear();
            rects.push(surface);
            EffectiveDamage {
                rect: Some(surface),
                rects,
                full_surface: true,
                policy,
            }
        }
    }
}

pub(super) fn select_damage_policy(
    metrics: DisplayListMetrics,
    requires_tree_rebuild: bool,
    has_extra_damage_sources: bool,
    candidate_area: u64,
) -> DisplayListRepaintPolicy {
    const FULL_SURFACE_DAMAGE_NUMERATOR: u64 = 2;
    const FULL_SURFACE_DAMAGE_DENOMINATOR: u64 = 3;
    const MOSTLY_CHANGED_ENTRIES_NUMERATOR: u64 = 3;
    const MOSTLY_CHANGED_ENTRIES_DENOMINATOR: u64 = 4;

    if candidate_area == 0 {
        return DisplayListRepaintPolicy::MinimalDamage;
    }

    let changed_entries = metrics
        .entries_rebuilt
        .saturating_add(metrics.entries_removed);
    let mostly_changed_entries = metrics.entries_total > 0
        && changed_entries * MOSTLY_CHANGED_ENTRIES_DENOMINATOR
            >= metrics.entries_total * MOSTLY_CHANGED_ENTRIES_NUMERATOR;
    // Acceptance guard: candidate_area * FULL_SURFACE_DAMAGE_DENOMINATOR >= metrics.surface_area * FULL_SURFACE_DAMAGE_NUMERATOR.
    let large_damage = metrics.surface_area > 0
        && candidate_area * FULL_SURFACE_DAMAGE_DENOMINATOR
            >= metrics.surface_area * FULL_SURFACE_DAMAGE_NUMERATOR;

    if large_damage || (requires_tree_rebuild && mostly_changed_entries) {
        DisplayListRepaintPolicy::FullSurface
    } else if has_extra_damage_sources {
        DisplayListRepaintPolicy::BoundingRect
    } else {
        DisplayListRepaintPolicy::MinimalDamage
    }
}

pub(super) fn tooltip_damage_rect(
    tooltip: Option<&(Arc<str>, f32, f32)>,
    surface_width: u32,
    surface_height: u32,
) -> Option<DamageRect> {
    let (_, paint_x, paint_y) = tooltip?;
    let width = TOOLTIP_OVERLAY_WIDTH.min(surface_width.max(1));
    let height = TOOLTIP_OVERLAY_HEIGHT.min(surface_height.max(1));
    let max_x = surface_width.saturating_sub(width).saturating_sub(6);
    let max_y = surface_height.saturating_sub(height).saturating_sub(6);
    let device = mesh_core_render::FractionalScale::identity().device_layout_rect(
        mesh_core_elements::LayoutRect {
            x: *paint_x,
            y: *paint_y,
            width: width as f32,
            height: height as f32,
        },
    );
    let x = (device.x.max(0) as u32).min(max_x).max(4);
    let y = (device.y.max(0) as u32).min(max_y).max(4);
    Some(DamageRect {
        x,
        y,
        width,
        height,
    })
}

#[cfg(test)]
pub(super) fn damage_rect_for_node_ids(
    node: &WidgetNode,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    last_visual_damage: &HashMap<mesh_core_elements::NodeId, DamageRect>,
    surface: DamageRect,
) -> Option<DamageRect> {
    bounding_damage_rect(
        &damage_rects_for_node_ids(node, node_ids, last_visual_damage, surface),
        surface,
    )
}

#[cfg(test)]
pub(super) fn damage_rects_for_node_ids(
    node: &WidgetNode,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    last_visual_damage: &HashMap<mesh_core_elements::NodeId, DamageRect>,
    surface: DamageRect,
) -> Vec<DamageRect> {
    let mut damage = Vec::with_capacity(node_ids.len().min(MAX_DAMAGE_RECTS));
    let current = collect_visual_damage_rects(node, surface);
    damage_rects_for_node_ids_into(&current, node_ids, last_visual_damage, surface, &mut damage);
    damage
}

pub(super) fn damage_rects_for_node_ids_into(
    current: &HashMap<mesh_core_elements::NodeId, DamageRect>,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    last_visual_damage: &HashMap<mesh_core_elements::NodeId, DamageRect>,
    surface: DamageRect,
    damage: &mut Vec<DamageRect>,
) {
    damage.clear();
    if node_ids.is_empty() {
        return;
    }

    damage.reserve(node_ids.len().min(MAX_DAMAGE_RECTS));
    for node_id in node_ids {
        if let Some(previous) = last_visual_damage.get(node_id).copied() {
            push_damage_rect(damage, previous, surface);
        }
    }
    for node_id in node_ids {
        if let Some(rect) = current.get(node_id) {
            push_damage_rect(damage, *rect, surface);
        }
    }
}

/// A node's own box plus its visual overshoot from `box-shadow` (outer) and
/// `filter`/`backdrop-filter` blur, in the tree's absolute layout space.
/// Shared by damage-rect computation (clipped to the surface) and popover
/// buffer padding (which needs the raw, unclipped extent).
pub(super) fn node_visual_bounds(node: &WidgetNode) -> Option<(f32, f32, f32, f32)> {
    mesh_core_render::display_list::widget_visual_bounds(node).map(layout_bounds_tuple)
}

fn layout_bounds_tuple(rect: mesh_core_elements::LayoutRect) -> (f32, f32, f32, f32) {
    (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height)
}

#[cfg(test)]
pub(super) fn visual_damage_rect_for_widget_node(
    node: &WidgetNode,
    surface: DamageRect,
) -> Option<DamageRect> {
    if !node_eligibility(node).allows(InteractionTarget::Paint) {
        return None;
    }
    let (left, top, right, bottom) = node_visual_bounds(node)?;
    let rect = mesh_core_render::FractionalScale::identity()
        .device_layout_rect(mesh_core_elements::LayoutRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
        .to_nonnegative_damage_rect()?;
    clip_damage(rect, surface)
}

/// Union of `node_visual_bounds` over `node` and its full subtree, in
/// absolute layout space. Used to size a popover's popup buffer so
/// descendant `box-shadow`/`filter` overshoot (e.g. a floating bubble
/// button's shadow) isn't clipped at the buffer edge.
pub(super) fn subtree_visual_bounds(node: &WidgetNode) -> (f32, f32, f32, f32) {
    mesh_core_render::display_list::widget_subtree_visual_bounds(node)
        .map(layout_bounds_tuple)
        .unwrap_or((f32::MAX, f32::MAX, f32::MIN, f32::MIN))
}

/// Extra buffer padding (left, top, right, bottom) a popover subtree needs
/// beyond its own laid-out box so descendant shadow/filter overshoot paints
/// instead of clipping at the popup buffer edge.
pub(super) fn popover_content_padding(node: &WidgetNode) -> (u32, u32, u32, u32) {
    let (left, top, right, bottom) = subtree_visual_bounds(node);
    if left > right || top > bottom {
        return (0, 0, 0, 0);
    }
    let Some((own_left, own_top, own_right, own_bottom)) = node_visual_bounds(node) else {
        return (0, 0, 0, 0);
    };
    (
        (own_left - left).max(0.0).ceil() as u32,
        (own_top - top).max(0.0).ceil() as u32,
        (right - own_right).max(0.0).ceil() as u32,
        (bottom - own_bottom).max(0.0).ceil() as u32,
    )
}

#[cfg(test)]
pub(super) fn collect_visual_damage_rects(
    node: &WidgetNode,
    surface: DamageRect,
) -> HashMap<mesh_core_elements::NodeId, DamageRect> {
    let mut list = mesh_core_render::RetainedDisplayList::default();
    list.update(node, surface.width, surface.height, true, true);
    list.visual_damage_rects(surface)
}

pub(super) fn damage_rects_from_options_into(
    rects: impl IntoIterator<Item = Option<DamageRect>>,
    surface: DamageRect,
    damage: &mut Vec<DamageRect>,
) {
    damage.clear();
    damage.reserve(2);
    for rect in rects.into_iter().flatten() {
        push_damage_rect(damage, rect, surface);
    }
}

pub(super) fn merge_damage_rects(
    current: &mut Vec<DamageRect>,
    next: impl IntoIterator<Item = DamageRect>,
    surface: DamageRect,
) {
    for rect in next {
        push_damage_rect(current, rect, surface);
    }
}

pub(super) fn push_damage_rect(rects: &mut Vec<DamageRect>, rect: DamageRect, surface: DamageRect) {
    let Some(rect) = clip_damage(rect, surface) else {
        return;
    };
    if let Some(index) = rects.iter().position(|existing| existing.intersects(rect)) {
        let merged = union_damage(rects[index], rect);
        rects.remove(index);
        push_damage_rect(rects, merged, surface);
        return;
    }
    if rects.len() < MAX_DAMAGE_RECTS {
        rects.push(rect);
        return;
    }

    let (merge_index, _) = rects
        .iter()
        .enumerate()
        .map(|(index, existing)| {
            let merged = union_damage(*existing, rect);
            let growth = merged.area().saturating_sub(existing.area());
            (index, growth)
        })
        .min_by_key(|(_, growth)| *growth)
        .unwrap_or((0, 0));
    let merged = union_damage(rects[merge_index], rect);
    rects.remove(merge_index);
    push_damage_rect(rects, merged, surface);
}

pub(super) fn bounding_damage_rect(
    rects: &[DamageRect],
    surface: DamageRect,
) -> Option<DamageRect> {
    let mut iter = rects.iter().copied();
    let first = iter.next()?;
    let bounds = iter.fold(first, union_damage);
    clip_damage(bounds, surface)
}

pub(super) fn damage_rects_area(rects: &[DamageRect]) -> u64 {
    rects.iter().map(|rect| rect.area()).sum()
}

pub(super) fn union_damage(current: DamageRect, next: DamageRect) -> DamageRect {
    let left = current.x.min(next.x);
    let top = current.y.min(next.y);
    let right = current
        .x
        .saturating_add(current.width)
        .max(next.x.saturating_add(next.width));
    let bottom = current
        .y
        .saturating_add(current.height)
        .max(next.y.saturating_add(next.height));
    DamageRect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
}

pub(super) fn clip_damage(rect: DamageRect, surface: DamageRect) -> Option<DamageRect> {
    let left = rect.x.max(surface.x);
    let top = rect.y.max(surface.y);
    let right = rect
        .x
        .saturating_add(rect.width)
        .min(surface.x.saturating_add(surface.width));
    let bottom = rect
        .y
        .saturating_add(rect.height)
        .min(surface.y.saturating_add(surface.height));
    if right > left && bottom > top {
        Some(DamageRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    } else {
        None
    }
}

impl FrontendSurfaceComponent {
    /// Move the retained pixels of viewports that only scrolled since the
    /// last painted frame, and narrow `effective` to the strips and anchored
    /// rects that still need replay. Returns the presentation damage — the
    /// whole moved viewports — when a shift was applied; `None` leaves
    /// `effective` untouched for an ordinary repaint.
    pub(super) fn shift_scrolled_viewports(
        &mut self,
        effective: &mut EffectiveDamage,
        buffer: &mut PixelBuffer,
        scale: f32,
        current_tooltip_damage: Option<DamageRect>,
    ) -> Option<Vec<DamageRect>> {
        let shifts = self.retained_display_list.scroll_shifts();
        if shifts.is_empty()
            || effective.full_surface
            || self.surface_pixels_invalid
            || self.buffer_overlaid
        {
            return None;
        }
        let mut displacements = smallvec::SmallVec::<[(i32, i32); 2]>::new();
        for shift in shifts {
            // The tooltip is painted into the buffer over the content and
            // would move with it.
            if [current_tooltip_damage, self.last_tooltip_damage]
                .into_iter()
                .flatten()
                .any(|tooltip| tooltip.intersects(shift.viewport))
            {
                return None;
            }
            // Other damage reaching into a moved viewport would be replayed
            // over pixels that are about to move.
            if effective.rects.iter().any(|rect| {
                rect.intersects(shift.viewport) && !damage_rect_contains(shift.viewport, *rect)
            }) {
                return None;
            }
            let dx = -shift.scroll_delta.0 * scale;
            let dy = -shift.scroll_delta.1 * scale;
            if (dx - dx.round()).abs() > 1e-3 || (dy - dy.round()).abs() > 1e-3 {
                return None;
            }
            displacements.push((dx.round() as i32, dy.round() as i32));
        }

        let present = effective.rects.clone();
        let mut paint = effective
            .rects
            .iter()
            .copied()
            .filter(|rect| !shifts.iter().any(|shift| rect.intersects(shift.viewport)))
            .collect::<Vec<_>>();
        for (shift, (dx, dy)) in shifts.iter().zip(displacements) {
            // The device pixels wholly inside the viewport; partially covered
            // edge pixels lie in the anchored edge bands, which are replayed.
            let left = (shift.viewport.x as f32 * scale).ceil() as u32;
            let top = (shift.viewport.y as f32 * scale).ceil() as u32;
            let right = ((shift.viewport.x + shift.viewport.width) as f32 * scale).floor() as u32;
            let bottom =
                ((shift.viewport.y + shift.viewport.height) as f32 * scale).floor() as u32;
            let device = DamageRect {
                x: left,
                y: top,
                width: right.saturating_sub(left),
                height: bottom.saturating_sub(top),
            };
            // A viewport edge between device pixels leaves a partially
            // covered column or row outside `device`; replay it like the
            // other anchored parts.
            let edge = |logical: u32| (logical as f32 * scale).fract() != 0.0;
            let v = shift.viewport;
            if edge(v.x) {
                paint.push(DamageRect { width: 1, ..v });
            }
            if edge(v.x + v.width) {
                paint.push(DamageRect { x: v.x + v.width - 1, width: 1, ..v });
            }
            if edge(v.y) {
                paint.push(DamageRect { height: 1, ..v });
            }
            if edge(v.y + v.height) {
                paint.push(DamageRect { y: v.y + v.height - 1, height: 1, ..v });
            }
            for strip in buffer.shift_region_pixels(device, dx, dy) {
                let x = (strip.x as f32 / scale).floor() as u32;
                let y = (strip.y as f32 / scale).floor() as u32;
                let right = ((strip.x + strip.width) as f32 / scale).ceil() as u32;
                let bottom = ((strip.y + strip.height) as f32 / scale).ceil() as u32;
                paint.extend(clip_damage(
                    DamageRect {
                        x,
                        y,
                        width: right.saturating_sub(x),
                        height: bottom.saturating_sub(y),
                    },
                    shift.viewport,
                ));
            }
            paint.extend(shift.anchored.iter().copied());
        }
        for shift in shifts {
            for rect in paint.iter_mut() {
                if damage_rect_contains(shift.viewport, *rect) {
                    *rect = avoid_arc_edges(*rect, shift);
                }
            }
        }
        effective.rect = paint.iter().copied().reduce(union_damage);
        effective.policy = DisplayListRepaintPolicy::MinimalDamage;
        effective.rects = paint;
        self.scroll_shifted_frames += 1;
        Some(present)
    }
}

fn damage_rect_contains(outer: DamageRect, inner: DamageRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// Grow `rect` inside its viewport until no edge that lies inside the
/// viewport ends on a row or column holding a rounded-corner arc, so every
/// arc is either replayed whole or moved whole. Edges on the viewport
/// boundary stay; the viewport's own edges are handled as anchored bands.
fn avoid_arc_edges(
    rect: DamageRect,
    shift: &mesh_core_render::display_list::ScrollShift,
) -> DamageRect {
    let viewport = shift.viewport;
    let grow = |mut start: u32, mut end: u32, low: u32, high: u32, arcs: &[(f32, f32)]| {
        loop {
            let mut moved = false;
            for &(a, b) in arcs {
                let (a, b) = (a.floor() - 1.0, b.ceil() + 1.0);
                if start > low && (start as f32) > a && (start as f32) < b {
                    start = (a.max(low as f32)) as u32;
                    moved = true;
                }
                if end < high && (end as f32) > a && (end as f32) < b {
                    end = (b.min(high as f32)) as u32;
                    moved = true;
                }
            }
            if !moved {
                return (start, end);
            }
        }
    };
    let (top, bottom) = grow(
        rect.y,
        rect.y + rect.height,
        viewport.y,
        viewport.y + viewport.height,
        &shift.arc_rows,
    );
    let (left, right) = grow(
        rect.x,
        rect.x + rect.width,
        viewport.x,
        viewport.x + viewport.width,
        &shift.arc_columns,
    );
    DamageRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

