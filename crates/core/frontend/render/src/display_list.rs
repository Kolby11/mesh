use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use mesh_core_elements::{NodeId, WidgetNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayPrimitiveSlot {
    Background,
    Border,
    Text,
    Icon,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayListKey {
    pub node_id: NodeId,
    pub slot: DisplayPrimitiveSlot,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DamageRect {
    pub fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    fn union(self, other: Self) -> Self {
        if self.width == 0 || self.height == 0 {
            return other;
        }
        if other.width == 0 || other.height == 0 {
            return self;
        }
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .max(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .max(other.y.saturating_add(other.height));
        Self {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayListMetrics {
    pub retained_generation: u64,
    pub entries_total: u64,
    pub entries_reused: u64,
    pub entries_rebuilt: u64,
    pub entries_removed: u64,
    pub damage_rect_count: u64,
    pub damage_area: u64,
    pub surface_area: u64,
    pub full_surface_damage: bool,
    pub partial_present_supported: bool,
    pub skipped_paint_pixels: u64,
    pub batch_count: u64,
    pub batched_primitives: u64,
    pub barrier_count: u64,
    pub barriers: DisplayBatchBarrierCounts,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayBatchBarrierCounts {
    pub text: u64,
    pub icon: u64,
    pub opacity: u64,
    pub clip: u64,
    pub translucency: u64,
    pub material_change: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayBatchBarrier {
    Text,
    Icon,
    Opacity,
    Clip,
    Translucency,
    MaterialChange,
}

impl DisplayBatchBarrier {
    fn record(self, counts: &mut DisplayBatchBarrierCounts) {
        match self {
            Self::Text => counts.text = counts.text.saturating_add(1),
            Self::Icon => counts.icon = counts.icon.saturating_add(1),
            Self::Opacity => counts.opacity = counts.opacity.saturating_add(1),
            Self::Clip => counts.clip = counts.clip.saturating_add(1),
            Self::Translucency => counts.translucency = counts.translucency.saturating_add(1),
            Self::MaterialChange => {
                counts.material_change = counts.material_change.saturating_add(1);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct RetainedDisplayList {
    generation: u64,
    entries: HashMap<DisplayListKey, DisplayListEntry>,
    last_metrics: DisplayListMetrics,
}

impl RetainedDisplayList {
    pub fn update(
        &mut self,
        root: &WidgetNode,
        surface_width: u32,
        surface_height: u32,
        force_full_damage: bool,
        partial_present_supported: bool,
    ) -> DisplayListMetrics {
        let surface = DamageRect {
            x: 0,
            y: 0,
            width: surface_width.max(1),
            height: surface_height.max(1),
        };
        let mut ordered_entries = Vec::new();
        collect_display_entries(root, &mut ordered_entries);
        let next: HashMap<_, _> = ordered_entries.iter().copied().collect();

        let mut damage: Option<DamageRect> = None;
        let mut reused = 0u64;
        let mut rebuilt = 0u64;
        for (key, next_entry) in &next {
            match self.entries.get(key) {
                Some(previous) if previous == next_entry => reused = reused.saturating_add(1),
                Some(previous) => {
                    rebuilt = rebuilt.saturating_add(1);
                    damage = union_damage(damage, previous.bounds);
                    damage = union_damage(damage, next_entry.bounds);
                }
                None => {
                    rebuilt = rebuilt.saturating_add(1);
                    damage = union_damage(damage, next_entry.bounds);
                }
            }
        }

        let next_keys: HashSet<_> = next.keys().copied().collect();
        let mut removed = 0u64;
        for (key, previous) in &self.entries {
            if !next_keys.contains(key) {
                removed = removed.saturating_add(1);
                damage = union_damage(damage, previous.bounds);
            }
        }

        let full_surface_damage = force_full_damage || damage.is_none() && self.entries.is_empty();
        let damage_rect = if full_surface_damage {
            surface
        } else {
            damage.unwrap_or_default()
        };
        let damage_rect = clip_rect(damage_rect, surface).unwrap_or_default();
        let damage_area = damage_rect.area();
        let surface_area = surface.area();
        let skipped_paint_pixels = if partial_present_supported {
            surface_area.saturating_sub(damage_area)
        } else {
            0
        };
        let batch_metrics = compute_batch_metrics(&ordered_entries);

        if rebuilt > 0 || removed > 0 || force_full_damage {
            self.generation = self.generation.saturating_add(1);
        }
        self.entries = next;
        self.last_metrics = DisplayListMetrics {
            retained_generation: self.generation,
            entries_total: self.entries.len() as u64,
            entries_reused: reused,
            entries_rebuilt: rebuilt,
            entries_removed: removed,
            damage_rect_count: u64::from(damage_area > 0),
            damage_area,
            surface_area,
            full_surface_damage,
            partial_present_supported,
            skipped_paint_pixels,
            batch_count: batch_metrics.batch_count,
            batched_primitives: batch_metrics.batched_primitives,
            barrier_count: batch_metrics.barrier_count,
            barriers: batch_metrics.barriers,
        };
        self.last_metrics
    }

    pub fn last_metrics(&self) -> DisplayListMetrics {
        self.last_metrics
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayListEntry {
    bounds: DamageRect,
    signature: u64,
    batch_signature: u64,
    barrier: Option<DisplayBatchBarrier>,
}

fn collect_display_entries(node: &WidgetNode, out: &mut Vec<(DisplayListKey, DisplayListEntry)>) {
    if let Some(bounds) = damage_rect_for_node(node) {
        for slot in primitive_slots_for_node(node) {
            out.push((
                DisplayListKey {
                    node_id: node.id,
                    slot,
                },
                DisplayListEntry {
                    bounds,
                    signature: primitive_signature(node, slot),
                    batch_signature: batch_signature(node, slot),
                    barrier: batch_barrier(node, slot),
                },
            ));
        }
    }
    for child in &node.children {
        collect_display_entries(child, out);
    }
}

fn compute_batch_metrics(entries: &[(DisplayListKey, DisplayListEntry)]) -> DisplayListMetrics {
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

fn primitive_slots_for_node(node: &WidgetNode) -> Vec<DisplayPrimitiveSlot> {
    let mut slots = Vec::new();
    if node.computed_style.background_color.a > 0 {
        slots.push(DisplayPrimitiveSlot::Background);
    }
    if node.computed_style.border_color.a > 0
        && (node.computed_style.border_width.top > 0.0
            || node.computed_style.border_width.right > 0.0
            || node.computed_style.border_width.bottom > 0.0
            || node.computed_style.border_width.left > 0.0)
    {
        slots.push(DisplayPrimitiveSlot::Border);
    }
    match node.tag.as_str() {
        "text" => slots.push(DisplayPrimitiveSlot::Text),
        "icon" => slots.push(DisplayPrimitiveSlot::Icon),
        _ => {}
    }
    if slots.is_empty() {
        slots.push(DisplayPrimitiveSlot::Generic);
    }
    slots
}

fn damage_rect_for_node(node: &WidgetNode) -> Option<DamageRect> {
    if node.layout.width <= 0.0 || node.layout.height <= 0.0 {
        return None;
    }
    let x = node.layout.x.floor().max(0.0) as u32;
    let y = node.layout.y.floor().max(0.0) as u32;
    let right = (node.layout.x + node.layout.width).ceil().max(0.0) as u32;
    let bottom = (node.layout.y + node.layout.height).ceil().max(0.0) as u32;
    Some(DamageRect {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    })
}

fn primitive_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    slot.hash(&mut hasher);
    node.tag.hash(&mut hasher);
    node.attributes.get("content").hash(&mut hasher);
    node.attributes.get("name").hash(&mut hasher);
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
    node.computed_style.opacity.to_bits().hash(&mut hasher);
    node.computed_style.font_family.hash(&mut hasher);
    node.computed_style.font_size.to_bits().hash(&mut hasher);
    hasher.finish()
}

fn batch_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    slot.hash(&mut hasher);
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
    node.computed_style.font_family.hash(&mut hasher);
    node.computed_style.font_size.to_bits().hash(&mut hasher);
    hasher.finish()
}

fn batch_barrier(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> Option<DisplayBatchBarrier> {
    match slot {
        DisplayPrimitiveSlot::Text => return Some(DisplayBatchBarrier::Text),
        DisplayPrimitiveSlot::Icon => return Some(DisplayBatchBarrier::Icon),
        DisplayPrimitiveSlot::Background
        | DisplayPrimitiveSlot::Border
        | DisplayPrimitiveSlot::Generic => {}
    }
    if node.computed_style.opacity < 1.0 {
        return Some(DisplayBatchBarrier::Opacity);
    }
    if node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
    {
        return Some(DisplayBatchBarrier::Clip);
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

fn union_damage(current: Option<DamageRect>, next: DamageRect) -> Option<DamageRect> {
    Some(match current {
        Some(current) => current.union(next),
        None => next,
    })
}

fn clip_rect(rect: DamageRect, surface: DamageRect) -> Option<DamageRect> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::style::{Color, Overflow};

    fn node(id: NodeId, tag: &str, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.id = id;
        node.layout.x = x;
        node.layout.y = y;
        node.layout.width = width;
        node.layout.height = height;
        node.computed_style.background_color = Color {
            r: 10,
            g: 20,
            b: 30,
            a: 255,
        };
        node
    }

    #[test]
    fn display_list_reuses_unchanged_entries() {
        let root = node(1, "box", 0.0, 0.0, 100.0, 40.0);
        let mut list = RetainedDisplayList::default();

        let first = list.update(&root, 100, 40, false, false);
        assert_eq!(first.entries_rebuilt, 1);
        assert_eq!(first.entries_reused, 0);
        assert_eq!(first.damage_area, 4_000);

        let second = list.update(&root, 100, 40, false, false);
        assert_eq!(second.entries_rebuilt, 0);
        assert_eq!(second.entries_reused, 1);
        assert_eq!(second.damage_area, 0);
        assert_eq!(second.skipped_paint_pixels, 0);
    }

    #[test]
    fn display_list_damages_old_and_new_bounds() {
        let mut root = node(1, "box", 0.0, 0.0, 20.0, 20.0);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, false, true);

        root.layout.x = 30.0;
        root.layout.y = 0.0;
        let metrics = list.update(&root, 100, 100, false, true);

        assert_eq!(metrics.entries_rebuilt, 1);
        assert_eq!(metrics.damage_area, 1_000);
        assert_eq!(metrics.skipped_paint_pixels, 9_000);
    }

    #[test]
    fn display_list_records_removed_entry_damage() {
        let mut root = node(1, "box", 0.0, 0.0, 80.0, 20.0);
        root.children.push(node(2, "text", 10.0, 0.0, 20.0, 10.0));
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, false, false);

        root.children.clear();
        let metrics = list.update(&root, 100, 100, false, false);

        assert_eq!(metrics.entries_removed, 2);
        assert_eq!(metrics.damage_area, 200);
    }

    #[test]
    fn display_list_clips_damage_to_surface() {
        let mut root = node(1, "box", 80.0, 80.0, 40.0, 40.0);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, false, false);

        root.layout.x = 90.0;
        let metrics = list.update(&root, 100, 100, false, false);

        assert_eq!(metrics.damage_area, 400);
    }

    #[test]
    fn display_list_can_force_full_surface_damage() {
        let root = node(1, "box", 10.0, 10.0, 10.0, 10.0);
        let mut list = RetainedDisplayList::default();
        let metrics = list.update(&root, 100, 50, true, false);

        assert!(metrics.full_surface_damage);
        assert_eq!(metrics.damage_area, 5_000);
    }

    #[test]
    fn display_list_batches_adjacent_compatible_primitives() {
        let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
        root.children.push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
        root.children.push(node(3, "box", 20.0, 0.0, 20.0, 20.0));
        let mut list = RetainedDisplayList::default();

        let metrics = list.update(&root, 100, 20, false, false);

        assert_eq!(metrics.batch_count, 1);
        assert_eq!(metrics.batched_primitives, 3);
        assert_eq!(metrics.barrier_count, 0);
    }

    #[test]
    fn display_list_records_batch_barriers() {
        let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
        root.children.push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
        let mut text = node(3, "text", 20.0, 0.0, 20.0, 20.0);
        text.attributes.insert("content".into(), "hello".into());
        root.children.push(text);
        let mut clipped = node(4, "box", 40.0, 0.0, 20.0, 20.0);
        clipped.computed_style.overflow_x = Overflow::Hidden;
        root.children.push(clipped);
        let mut list = RetainedDisplayList::default();

        let metrics = list.update(&root, 100, 20, false, false);

        assert_eq!(metrics.barriers.text, 1);
        assert_eq!(metrics.barriers.clip, 1);
        assert_eq!(metrics.barrier_count, 2);
    }
}
