use mesh_core_elements::Prop;
use std::hash::{Hash, Hasher};
use std::path::Path;

#[cfg(test)]
use mesh_core_elements::BoxShadow;
use mesh_core_elements::style::{BackgroundPaint, Color, GradientGeometry};
use mesh_core_elements::{AffineTransform, LayoutRect, WidgetNode};

use crate::FractionalScale;
use crate::paint_input::{PaintInput, PaintPrimitiveSlot};

use super::build::*;
use super::types::*;

#[cfg(test)]
pub(super) fn compute_batch_metrics(entries: &[DisplayBatchMaterial]) -> DisplayListMetrics {
    let mut batch_count = 0u64;
    let mut batched_primitives = 0u64;
    let mut barrier_count = 0u64;
    let mut barriers = DisplayBatchBarrierCounts::default();
    let mut current_batch_signature: Option<u64> = None;
    let mut current_batch_len = 0u64;

    for entry in entries {
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
        "icon" | "image" => {
            emitted = true;
            visit(DisplayPrimitiveSlot::Icon);
        }
        _ => {}
    }
    if !emitted {
        visit(DisplayPrimitiveSlot::Generic);
    }
}

pub(super) fn damage_rect_for_node_with_transform(
    node: &WidgetNode,
    world_transform: AffineTransform,
) -> Option<DamageRect> {
    if node.layout.width <= 0.0 || node.layout.height <= 0.0 || world_transform.inverse().is_none()
    {
        return None;
    }
    let layout = visual_bounds_for_node_with_transform(node, world_transform);
    FractionalScale::identity()
        .device_layout_rect(layout)
        .to_nonnegative_damage_rect()
}

/// Return the conservative surface AABB for a node's transformed visual
/// output. Effect overflow is expanded in local coordinates before applying
/// the same cumulative affine transform as paint and descendant traversal;
/// this keeps rotated shadows and filters from being expanded along the wrong
/// surface axes.
pub(super) fn visual_bounds_for_node_with_transform(
    node: &WidgetNode,
    world_transform: AffineTransform,
) -> LayoutRect {
    visual_layout_from_parts(
        world_transform,
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: node.layout.width.max(0.0),
            height: node.layout.height.max(0.0),
        },
        node.computed_style.box_shadow,
        &node.computed_style.additional_box_shadows,
        node.computed_style.outline.extent(),
        node.computed_style.filter.blur_radius,
        node.computed_style.backdrop_filter.blur_radius,
    )
}

#[cfg(test)]
pub(super) fn primitive_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
    let paint = PaintInput::for_node(node, None);
    primitive_signature_for_paint(&paint, slot)
}

pub(super) fn primitive_signature_for_paint(paint: &PaintInput, slot: DisplayPrimitiveSlot) -> u64 {
    let slot = match slot {
        DisplayPrimitiveSlot::Background => PaintPrimitiveSlot::Background,
        DisplayPrimitiveSlot::Border => PaintPrimitiveSlot::Border,
        DisplayPrimitiveSlot::Text => PaintPrimitiveSlot::Text,
        DisplayPrimitiveSlot::Icon => PaintPrimitiveSlot::Icon,
        DisplayPrimitiveSlot::Generic => PaintPrimitiveSlot::Generic,
    };
    paint.signature_for_slot(slot)
}

#[cfg(test)]
pub(super) fn hash_paint_content_attributes(
    node: &WidgetNode,
    hasher: &mut DisplaySignatureHasher,
) {
    match node.tag.as_str() {
        "text" => {
            hash_attribute(node, Prop::Content, hasher);
            hash_attribute(node, Prop::Text, hasher);
            node.text_selection.hash(hasher);
        }
        "input" => {
            hash_attribute(node, Prop::Value, hasher);
            hash_attribute(node, Prop::Placeholder, hasher);
            hash_attribute(node, Prop::Type, hasher);
            node.state
                .contains(mesh_core_elements::NodeState::FOCUSED)
                .hash(hasher);
            node.input_preedit.hash(hasher);
        }
        "slider" => {
            hash_attribute(node, Prop::Min, hasher);
            hash_attribute(node, Prop::Max, hasher);
            hash_attribute(node, Prop::Value, hasher);
            hash_attribute(node, Prop::Orient, hasher);
        }
        "icon" | "image" => {
            hash_attribute(node, Prop::Src, hasher);
            hash_attribute(node, Prop::Name, hasher);
            hash_attribute(node, Prop::Size, hasher);
        }
        "checkbox" | "radio" => {
            hash_attribute(node, Prop::Checked, hasher);
        }
        "shape" => {
            hash_attribute(node, Prop::Type, hasher);
            hash_attribute(node, Prop::Start, hasher);
            hash_attribute(node, Prop::End, hasher);
            hash_attribute(node, Prop::Inner, hasher);
            hash_attribute(node, Prop::Points, hasher);
            node.computed_style.border_width.top.to_bits().hash(hasher);
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
            node.computed_style.font_family.hash(hasher);
            node.computed_style.font_size.to_bits().hash(hasher);
            node.computed_style.font_weight.hash(hasher);
            std::mem::discriminant(&node.computed_style.font_style).hash(hasher);
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
            mesh_core_resources::resource_revision().hash(hasher);
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
    node.computed_style.letter_spacing.to_bits().hash(hasher);
    node.computed_style.line_height.to_bits().hash(hasher);
    std::mem::discriminant(&node.computed_style.text_align).hash(hasher);
    std::mem::discriminant(&node.computed_style.text_direction).hash(hasher);
    std::mem::discriminant(&node.computed_style.white_space).hash(hasher);
    node.attributes.get(Prop::Lang).hash(hasher);
    node.attributes.get(Prop::FontFeatures).hash(hasher);
    mesh_core_resources::resource_revision().hash(hasher);
}

pub(super) fn hash_color(color: Color, hasher: &mut DisplaySignatureHasher) {
    color.r.hash(hasher);
    color.g.hash(hasher);
    color.b.hash(hasher);
    color.a.hash(hasher);
}

#[cfg(test)]
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
        BackgroundPaint::MultiStopLinearGradient(colors) => {
            3_u8.hash(hasher);
            colors.len().hash(hasher);
            for color in colors {
                color.r.hash(hasher);
                color.g.hash(hasher);
                color.b.hash(hasher);
                color.a.hash(hasher);
            }
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
        BackgroundPaint::Gradient(gradient) => {
            4_u8.hash(hasher);
            match gradient.geometry {
                GradientGeometry::Linear { angle_degrees } => {
                    0_u8.hash(hasher);
                    angle_degrees.to_bits().hash(hasher);
                }
                GradientGeometry::Radial {
                    shape,
                    extent,
                    center,
                } => {
                    1_u8.hash(hasher);
                    std::mem::discriminant(&shape).hash(hasher);
                    std::mem::discriminant(&extent).hash(hasher);
                    center.0.to_bits().hash(hasher);
                    center.1.to_bits().hash(hasher);
                }
            }
            gradient.stops.len().hash(hasher);
            for stop in &gradient.stops {
                stop.color.r.hash(hasher);
                stop.color.g.hash(hasher);
                stop.color.b.hash(hasher);
                stop.color.a.hash(hasher);
                stop.position.to_bits().hash(hasher);
            }
        }
    }
}

#[cfg(test)]
pub(super) fn hash_attribute(node: &WidgetNode, key: Prop, hasher: &mut DisplaySignatureHasher) {
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
    if !node.computed_style.additional_box_shadows.is_empty()
        || !node.computed_style.box_shadow.is_none()
        || !node.computed_style.outline.is_none()
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
    let Some(src) = node.attributes.get(Prop::Src) else {
        return crate::surface::icon::CachedResourceOpacity::Unknown;
    };
    let scale = FractionalScale::identity();
    let width = scale.physical_extent_f32(node.layout.width);
    let height = scale.physical_extent_f32(node.layout.height);
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
