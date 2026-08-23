use std::hash::{Hash, Hasher};
use std::path::Path;

use mesh_core_elements::BoxShadow;
use mesh_core_elements::WidgetNode;
use mesh_core_elements::style::{BackgroundPaint, Color};

use super::build::*;
use super::paint_node::*;
use super::types::*;

pub(super) fn compute_batch_metrics(
    entries: &[(DisplayListKey, DisplayListEntry)],
) -> DisplayListMetrics {
    let mut batch_count = 0u64;
    let mut batched_primitives = 0u64;
    let mut barrier_count = 0u64;
    let mut barriers = DisplayBatchBarrierCounts::default();
    let mut current_batch_signature: Option<u64> = None;
    let mut current_batch_len = 0u64;

    for (_, entry) in entries {
        if let Some(reason) = entry.barrier {
            if current_batch_len > 1 {
                batch_count = batch_count.saturating_add(1);
                batched_primitives = batched_primitives.saturating_add(current_batch_len);
            }
            current_batch_signature = None;
            current_batch_len = 0;
            barrier_count = barrier_count.saturating_add(1);
            reason.record(&mut barriers);
            continue;
        }

        match current_batch_signature {
            Some(signature) if signature == entry.batch_signature => {
                current_batch_len = current_batch_len.saturating_add(1);
            }
            Some(_) => {
                if current_batch_len > 1 {
                    batch_count = batch_count.saturating_add(1);
                    batched_primitives = batched_primitives.saturating_add(current_batch_len);
                }
                barrier_count = barrier_count.saturating_add(1);
                DisplayBatchBarrier::MaterialChange.record(&mut barriers);
                current_batch_signature = Some(entry.batch_signature);
                current_batch_len = 1;
            }
            None => {
                current_batch_signature = Some(entry.batch_signature);
                current_batch_len = 1;
            }
        }
    }

    if current_batch_len > 1 {
        batch_count = batch_count.saturating_add(1);
        batched_primitives = batched_primitives.saturating_add(current_batch_len);
    }

    DisplayListMetrics {
        batch_count,
        batched_primitives,
        barrier_count,
        barriers,
        ..Default::default()
    }
}

pub(super) fn for_each_primitive_slot(
    node: &WidgetNode,
    mut visit: impl FnMut(DisplayPrimitiveSlot),
) {
    let mut emitted = false;
    if node.computed_style.background_color.a > 0 {
        emitted = true;
        visit(DisplayPrimitiveSlot::Background);
    }
    if node.computed_style.border_color.a > 0
        && (node.computed_style.border_width.top > 0.0
            || node.computed_style.border_width.right > 0.0
            || node.computed_style.border_width.bottom > 0.0
            || node.computed_style.border_width.left > 0.0)
    {
        emitted = true;
        visit(DisplayPrimitiveSlot::Border);
    }
    match node.tag.as_str() {
        "text" => {
            emitted = true;
            visit(DisplayPrimitiveSlot::Text);
        }
        "icon" => {
            emitted = true;
            visit(DisplayPrimitiveSlot::Icon);
        }
        _ => {}
    }
    if !emitted {
        visit(DisplayPrimitiveSlot::Generic);
    }
}

pub(super) fn damage_rect_for_node_at(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
) -> Option<DamageRect> {
    if node.layout.width <= 0.0 || node.layout.height <= 0.0 {
        return None;
    }
    let layout = transformed_layout_at(node, offset_x, offset_y);
    let left = layout.x;
    let top = layout.y;
    let mut left = left;
    let mut top = top;
    let mut right = left + layout.width;
    let mut bottom = top + layout.height;
    let shadow = node.computed_style.box_shadow;
    if !shadow.is_none() && !shadow.inset {
        let spread = shadow.spread_radius;
        let blur_pad = shadow.blur_radius * 3.0;
        left = left.min(layout.x + shadow.offset_x - spread - blur_pad);
        top = top.min(layout.y + shadow.offset_y - spread - blur_pad);
        right = right.max(layout.x + layout.width + shadow.offset_x + spread + blur_pad);
        bottom = bottom.max(layout.y + layout.height + shadow.offset_y + spread + blur_pad);
    }
    let filter_pad = node
        .computed_style
        .filter
        .blur_radius
        .max(node.computed_style.backdrop_filter.blur_radius)
        * 3.0;
    if filter_pad > 0.0 {
        left -= filter_pad;
        top -= filter_pad;
        right += filter_pad;
        bottom += filter_pad;
    }
    let x = left.floor().max(0.0) as u32;
    let y = top.floor().max(0.0) as u32;
    let right = right.ceil().max(0.0) as u32;
    let bottom = bottom.ceil().max(0.0) as u32;
    Some(DamageRect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    })
}

pub(super) fn primitive_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
    let mut hasher = DisplaySignatureHasher::default();
    slot.hash(&mut hasher);
    node.tag.hash(&mut hasher);
    hash_paint_content_attributes(node, &mut hasher);
    node.computed_style.background_color.r.hash(&mut hasher);
    node.computed_style.background_color.g.hash(&mut hasher);
    node.computed_style.background_color.b.hash(&mut hasher);
    node.computed_style.background_color.a.hash(&mut hasher);
    node.computed_style.border_color.r.hash(&mut hasher);
    node.computed_style.border_color.g.hash(&mut hasher);
    node.computed_style.border_color.b.hash(&mut hasher);
    node.computed_style.border_color.a.hash(&mut hasher);
    node.computed_style.color.r.hash(&mut hasher);
    node.computed_style.color.g.hash(&mut hasher);
    node.computed_style.color.b.hash(&mut hasher);
    node.computed_style.color.a.hash(&mut hasher);
    node.computed_style
        .border_width
        .top
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .border_width
        .right
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .border_width
        .bottom
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .border_width
        .left
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .border_radius
        .top_left
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .border_radius
        .top_right
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .border_radius
        .bottom_right
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .border_radius
        .bottom_left
        .to_bits()
        .hash(&mut hasher);
    node.computed_style.padding.top.to_bits().hash(&mut hasher);
    node.computed_style
        .padding
        .right
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .padding
        .bottom
        .to_bits()
        .hash(&mut hasher);
    node.computed_style.padding.left.to_bits().hash(&mut hasher);
    node.computed_style.opacity.to_bits().hash(&mut hasher);
    hash_box_shadow(node.computed_style.box_shadow, &mut hasher);
    hash_background_paint(&node.computed_style.background_paint, &mut hasher);
    node.computed_style
        .filter
        .blur_radius
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .backdrop_filter
        .blur_radius
        .to_bits()
        .hash(&mut hasher);
    node.computed_style.mix_blend_mode.hash(&mut hasher);
    node.computed_style.font_family.hash(&mut hasher);
    node.computed_style.font_size.to_bits().hash(&mut hasher);
    node.computed_style.font_weight.hash(&mut hasher);
    node.computed_style.line_height.to_bits().hash(&mut hasher);
    std::mem::discriminant(&node.computed_style.text_align).hash(&mut hasher);
    std::mem::discriminant(&node.computed_style.text_overflow).hash(&mut hasher);
    std::mem::discriminant(&node.computed_style.text_direction).hash(&mut hasher);
    std::mem::discriminant(&node.computed_style.font_style).hash(&mut hasher);
    std::mem::discriminant(&node.computed_style.overflow_x).hash(&mut hasher);
    std::mem::discriminant(&node.computed_style.overflow_y).hash(&mut hasher);
    node.computed_style
        .letter_spacing
        .to_bits()
        .hash(&mut hasher);
    node.computed_style
        .icon_fill
        .map(f32::to_bits)
        .hash(&mut hasher);
    node.computed_style
        .icon_weight
        .map(f32::to_bits)
        .hash(&mut hasher);
    node.computed_style
        .icon_grade
        .map(f32::to_bits)
        .hash(&mut hasher);
    node.computed_style
        .icon_optical_size
        .map(f32::to_bits)
        .hash(&mut hasher);
    hasher.finish()
}

pub(super) fn hash_paint_content_attributes(
    node: &WidgetNode,
    hasher: &mut DisplaySignatureHasher,
) {
    match node.tag.as_str() {
        "text" => {
            hash_attribute(node, "content", hasher);
            hash_attribute(node, "text", hasher);
            hash_attribute(node, "_mesh_selection_anchor_x", hasher);
            hash_attribute(node, "_mesh_selection_anchor_y", hasher);
            hash_attribute(node, "_mesh_selection_focus_x", hasher);
            hash_attribute(node, "_mesh_selection_focus_y", hasher);
            hash_attribute(node, "_mesh_selection_text_x", hasher);
            hash_attribute(node, "_mesh_selection_text_y", hasher);
        }
        "input" => {
            hash_attribute(node, "value", hasher);
            hash_attribute(node, "placeholder", hasher);
            hash_attribute(node, "type", hasher);
            hash_attribute(node, "_mesh_focused", hasher);
            hash_attribute(node, "_mesh_preedit_start", hasher);
            hash_attribute(node, "_mesh_preedit_end", hasher);
            hash_attribute(node, "_mesh_preedit_cursor_begin", hasher);
            hash_attribute(node, "_mesh_preedit_cursor_end", hasher);
        }
        "slider" => {
            hash_attribute(node, "min", hasher);
            hash_attribute(node, "max", hasher);
            hash_attribute(node, "value", hasher);
            hash_attribute(node, "orient", hasher);
        }
        "icon" => {
            hash_attribute(node, "src", hasher);
            hash_attribute(node, "name", hasher);
            hash_attribute(node, "size", hasher);
        }
        "checkbox" | "radio" => {
            hash_attribute(node, "checked", hasher);
        }
        _ => {}
    }
}

pub(super) fn batch_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
    let mut hasher = DisplaySignatureHasher::default();
    slot.hash(&mut hasher);
    hash_batch_material(node, slot, &mut hasher);
    hasher.finish()
}

pub(super) fn hash_batch_material(
    node: &WidgetNode,
    slot: DisplayPrimitiveSlot,
    hasher: &mut DisplaySignatureHasher,
) {
    match slot {
        DisplayPrimitiveSlot::Background => {
            hash_color(node.computed_style.background_color, hasher);
            hash_background_paint(&node.computed_style.background_paint, hasher);
        }
        DisplayPrimitiveSlot::Border => {
            hash_color(node.computed_style.border_color, hasher);
        }
        DisplayPrimitiveSlot::Icon => {
            hash_color(node.computed_style.color, hasher);
            node.computed_style.icon_fill.map(f32::to_bits).hash(hasher);
            node.computed_style
                .icon_weight
                .map(f32::to_bits)
                .hash(hasher);
            node.computed_style
                .icon_grade
                .map(f32::to_bits)
                .hash(hasher);
            node.computed_style
                .icon_optical_size
                .map(f32::to_bits)
                .hash(hasher);
        }
        DisplayPrimitiveSlot::Generic => {
            hash_generic_batch_material(node, hasher);
        }
        DisplayPrimitiveSlot::Text => {
            hash_text_batch_material(node, hasher);
        }
    }
}

pub(super) fn hash_generic_batch_material(node: &WidgetNode, hasher: &mut DisplaySignatureHasher) {
    match node.tag.as_str() {
        "input" => hash_text_batch_material(node, hasher),
        "slider" | "checkbox" | "radio" => hash_color(node.computed_style.color, hasher),
        _ => {}
    }
}

pub(super) fn hash_text_batch_material(node: &WidgetNode, hasher: &mut DisplaySignatureHasher) {
    hash_color(node.computed_style.color, hasher);
    node.computed_style.font_family.hash(hasher);
    node.computed_style.font_size.to_bits().hash(hasher);
    node.computed_style.font_weight.hash(hasher);
    std::mem::discriminant(&node.computed_style.font_style).hash(hasher);
    node.computed_style.line_height.to_bits().hash(hasher);
    std::mem::discriminant(&node.computed_style.text_align).hash(hasher);
}

pub(super) fn hash_color(color: Color, hasher: &mut DisplaySignatureHasher) {
    color.r.hash(hasher);
    color.g.hash(hasher);
    color.b.hash(hasher);
    color.a.hash(hasher);
}

pub(super) fn hash_box_shadow(shadow: BoxShadow, hasher: &mut DisplaySignatureHasher) {
    shadow.offset_x.to_bits().hash(hasher);
    shadow.offset_y.to_bits().hash(hasher);
    shadow.blur_radius.to_bits().hash(hasher);
    shadow.spread_radius.to_bits().hash(hasher);
    shadow.color.r.hash(hasher);
    shadow.color.g.hash(hasher);
    shadow.color.b.hash(hasher);
    shadow.color.a.hash(hasher);
    shadow.inset.hash(hasher);
}

pub(super) fn hash_background_paint(paint: &BackgroundPaint, hasher: &mut DisplaySignatureHasher) {
    match paint {
        BackgroundPaint::None => 0_u8.hash(hasher),
        BackgroundPaint::Image(source) => {
            1_u8.hash(hasher);
            source.path.hash(hasher);
        }
        BackgroundPaint::LinearGradient(gradient) => {
            2_u8.hash(hasher);
            gradient.from.r.hash(hasher);
            gradient.from.g.hash(hasher);
            gradient.from.b.hash(hasher);
            gradient.from.a.hash(hasher);
            gradient.to.r.hash(hasher);
            gradient.to.g.hash(hasher);
            gradient.to.b.hash(hasher);
            gradient.to.a.hash(hasher);
        }
    }
}

pub(super) fn hash_attribute(node: &WidgetNode, key: &str, hasher: &mut DisplaySignatureHasher) {
    key.hash(hasher);
    node.attributes.get(key).hash(hasher);
}

pub(super) fn batch_barrier(
    node: &WidgetNode,
    slot: DisplayPrimitiveSlot,
) -> Option<DisplayBatchBarrier> {
    match slot {
        DisplayPrimitiveSlot::Text => return Some(DisplayBatchBarrier::Text),
        DisplayPrimitiveSlot::Icon => {}
        DisplayPrimitiveSlot::Background
        | DisplayPrimitiveSlot::Border
        | DisplayPrimitiveSlot::Generic => {}
    }
    if node.computed_style.opacity < 1.0 {
        return Some(DisplayBatchBarrier::Opacity);
    }
    if !node.computed_style.box_shadow.is_none()
        || !node.computed_style.filter.is_none()
        || !node.computed_style.backdrop_filter.is_none()
    {
        return Some(DisplayBatchBarrier::Translucency);
    }
    if node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
    {
        return Some(DisplayBatchBarrier::Clip);
    }
    if matches!(slot, DisplayPrimitiveSlot::Icon) {
        return match cached_icon_resource_opacity(node) {
            crate::surface::icon::CachedResourceOpacity::Opaque => None,
            crate::surface::icon::CachedResourceOpacity::Translucent => {
                Some(DisplayBatchBarrier::Translucency)
            }
            crate::surface::icon::CachedResourceOpacity::Unknown => Some(DisplayBatchBarrier::Icon),
        };
    }
    let translucent = match slot {
        DisplayPrimitiveSlot::Background => node.computed_style.background_color.a < 255,
        DisplayPrimitiveSlot::Border => node.computed_style.border_color.a < 255,
        DisplayPrimitiveSlot::Generic => false,
        DisplayPrimitiveSlot::Text | DisplayPrimitiveSlot::Icon => false,
    };
    if translucent {
        return Some(DisplayBatchBarrier::Translucency);
    }
    None
}

pub(super) fn cached_icon_resource_opacity(
    node: &WidgetNode,
) -> crate::surface::icon::CachedResourceOpacity {
    let Some(src) = node.attributes.get("src") else {
        return crate::surface::icon::CachedResourceOpacity::Unknown;
    };
    let width = node.layout.width.round().max(1.0) as u32;
    let height = node.layout.height.round().max(1.0) as u32;
    crate::surface::icon::cached_file_resource_opacity(
        Path::new(src),
        width,
        height,
        node.computed_style.color,
        false,
    )
}

pub(super) fn union_damage(current: Option<DamageRect>, next: DamageRect) -> Option<DamageRect> {
    Some(match current {
        Some(current) => current.union(next),
        None => next,
    })
}

pub(super) fn clip_rect(rect: DamageRect, surface: DamageRect) -> Option<DamageRect> {
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
