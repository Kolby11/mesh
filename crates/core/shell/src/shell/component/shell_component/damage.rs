#![allow(dead_code)] // Alternate damage-bound helpers remain for paint fixtures.

use super::*;
use mesh_core_elements::{
    AffineTransform, InteractionTarget, NodeEligibility, child_transform, node_eligibility,
    node_transform, root_transform,
};

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
    let policy = select_damage_policy(
        metrics,
        requires_tree_rebuild,
        has_extra_damage_sources,
        damage_area,
    );
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
    damage_rects_for_node_ids_into(node, node_ids, last_visual_damage, surface, &mut damage);
    damage
}

pub(super) fn damage_rects_for_node_ids_into(
    node: &WidgetNode,
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
    collect_damage_rects_for_node_ids(node, node_ids, surface, damage);
}

pub(super) fn collect_damage_rects_for_node_ids(
    node: &WidgetNode,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    surface: DamageRect,
    damage: &mut Vec<DamageRect>,
) {
    collect_damage_rects_for_node_ids_with_policy(
        node,
        node_ids,
        surface,
        damage,
        NodeEligibility::ROOT,
    );
}

fn collect_damage_rects_for_node_ids_with_policy(
    node: &WidgetNode,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    surface: DamageRect,
    damage: &mut Vec<DamageRect>,
    parent_policy: NodeEligibility,
) {
    if node_ids.is_empty() {
        return;
    }
    let policy = parent_policy.child(node);
    if node_ids.contains(&node.id)
        && policy.allows(InteractionTarget::Paint)
        && let Some(bounds) = damage_rect_for_widget_node(node, surface)
    {
        push_damage_rect(damage, bounds, surface);
    }

    for child in &node.children {
        collect_damage_rects_for_node_ids_with_policy(child, node_ids, surface, damage, policy);
    }
}

pub(super) fn damage_rect_for_widget_node(
    node: &WidgetNode,
    surface: DamageRect,
) -> Option<DamageRect> {
    visual_damage_rect_for_widget_node(node, surface)
}

/// Extend a node's plain `(left, top, right, bottom)` box to also cover its
/// box-shadow and blur-filter overflow, in the same coordinate space as the
/// input box. Shared by present-damage computation (clipped to the surface)
/// and popup buffer padding.
pub(super) fn shadow_filter_extended_bounds(
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    style: &mesh_core_elements::style::ComputedStyle,
) -> (f32, f32, f32, f32) {
    let mut ext_left = left;
    let mut ext_top = top;
    let mut ext_right = left + width;
    let mut ext_bottom = top + height;

    let shadow = style.box_shadow;
    if !shadow.is_none() && !shadow.inset {
        let spread = shadow.spread_radius;
        let blur_pad = shadow.blur_radius * 3.0;
        ext_left = ext_left.min(left + shadow.offset_x - spread - blur_pad);
        ext_top = ext_top.min(top + shadow.offset_y - spread - blur_pad);
        ext_right = ext_right.max(left + width + shadow.offset_x + spread + blur_pad);
        ext_bottom = ext_bottom.max(top + height + shadow.offset_y + spread + blur_pad);
    }

    let filter_pad = style
        .filter
        .blur_radius
        .max(style.backdrop_filter.blur_radius)
        * 3.0;
    if filter_pad > 0.0 {
        ext_left -= filter_pad;
        ext_top -= filter_pad;
        ext_right += filter_pad;
        ext_bottom += filter_pad;
    }

    (ext_left, ext_top, ext_right, ext_bottom)
}

/// A node's own box plus its visual overshoot from `box-shadow` (outer) and
/// `filter`/`backdrop-filter` blur, in the tree's absolute layout space.
/// Shared by damage-rect computation (clipped to the surface) and popover
/// buffer padding (which needs the raw, unclipped extent).
pub(super) fn node_visual_bounds(node: &WidgetNode) -> Option<(f32, f32, f32, f32)> {
    node_visual_bounds_with_transform(node, root_transform(0.0, 0.0)).map(layout_bounds_tuple)
}

fn node_visual_bounds_with_transform(
    node: &WidgetNode,
    parent_transform: AffineTransform,
) -> Option<mesh_core_elements::LayoutRect> {
    if node.layout.width <= 0.0 || node.layout.height <= 0.0 {
        return None;
    }
    let world = node_transform(parent_transform, node);
    if world.inverse().is_none() {
        return None;
    }
    let local = mesh_core_elements::LayoutRect {
        x: 0.0,
        y: 0.0,
        width: node.layout.width.max(0.0),
        height: node.layout.height.max(0.0),
    };
    let mut bounds = world.transform_rect(local);
    let shadow = node.computed_style.box_shadow;
    if !shadow.is_none() && !shadow.inset {
        let pad = shadow.spread_radius + shadow.blur_radius * 3.0;
        bounds = union_layout_rect(
            bounds,
            world.transform_rect(mesh_core_elements::LayoutRect {
                x: shadow.offset_x - pad,
                y: shadow.offset_y - pad,
                width: local.width + pad * 2.0,
                height: local.height + pad * 2.0,
            }),
        );
    }
    let filter_pad = node
        .computed_style
        .filter
        .blur_radius
        .max(node.computed_style.backdrop_filter.blur_radius)
        * 3.0;
    if filter_pad > 0.0 {
        bounds.x -= filter_pad;
        bounds.y -= filter_pad;
        bounds.width += filter_pad * 2.0;
        bounds.height += filter_pad * 2.0;
    }
    Some(bounds)
}

fn layout_bounds_tuple(rect: mesh_core_elements::LayoutRect) -> (f32, f32, f32, f32) {
    (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height)
}

fn union_layout_rect(
    left: mesh_core_elements::LayoutRect,
    right: mesh_core_elements::LayoutRect,
) -> mesh_core_elements::LayoutRect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = (left.x + left.width).max(right.x + right.width);
    let bottom_edge = (left.y + left.height).max(right.y + right.height);
    mesh_core_elements::LayoutRect {
        x,
        y,
        width: (right_edge - x).max(0.0),
        height: (bottom_edge - y).max(0.0),
    }
}

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
    let mut bounds = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    accumulate_subtree_visual_bounds(node, &mut bounds);
    bounds
}

pub(super) fn accumulate_subtree_visual_bounds(
    node: &WidgetNode,
    bounds: &mut (f32, f32, f32, f32),
) {
    accumulate_subtree_visual_bounds_with_transform(
        node,
        root_transform(0.0, 0.0),
        NodeEligibility::ROOT,
        bounds,
    );
}

fn accumulate_subtree_visual_bounds_with_transform(
    node: &WidgetNode,
    parent_transform: AffineTransform,
    parent_policy: NodeEligibility,
    bounds: &mut (f32, f32, f32, f32),
) {
    let policy = parent_policy.child(node);
    if !policy.allows(InteractionTarget::Paint) {
        return;
    }
    if let Some(rect) = node_visual_bounds_with_transform(node, parent_transform) {
        let (left, top, right, bottom) = layout_bounds_tuple(rect);
        bounds.0 = bounds.0.min(left);
        bounds.1 = bounds.1.min(top);
        bounds.2 = bounds.2.max(right);
        bounds.3 = bounds.3.max(bottom);
    }
    let world = node_transform(parent_transform, node);
    let scroll = node.resolved_scroll_metrics();
    let child_transform = child_transform(world, node, scroll.x, scroll.y);
    for child in &node.children {
        accumulate_subtree_visual_bounds_with_transform(child, child_transform, policy, bounds);
    }
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

pub(super) fn collect_visual_damage_rects(
    node: &WidgetNode,
    surface: DamageRect,
) -> HashMap<mesh_core_elements::NodeId, DamageRect> {
    let mut damage = HashMap::new();
    collect_visual_damage_rects_with_policy(node, surface, &mut damage, NodeEligibility::ROOT);
    damage
}

pub(super) fn collect_visual_damage_rects_into(
    node: &WidgetNode,
    surface: DamageRect,
    damage: &mut HashMap<mesh_core_elements::NodeId, DamageRect>,
) {
    collect_visual_damage_rects_with_policy(node, surface, damage, NodeEligibility::ROOT);
}

fn collect_visual_damage_rects_with_policy(
    node: &WidgetNode,
    surface: DamageRect,
    damage: &mut HashMap<mesh_core_elements::NodeId, DamageRect>,
    parent_policy: NodeEligibility,
) {
    let policy = parent_policy.child(node);
    if !policy.allows(InteractionTarget::Paint) {
        return;
    }
    if let Some(bounds) = visual_damage_rect_for_widget_node(node, surface) {
        damage.insert(node.id, bounds);
    }
    for child in &node.children {
        collect_visual_damage_rects_with_policy(child, surface, damage, policy);
    }
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
