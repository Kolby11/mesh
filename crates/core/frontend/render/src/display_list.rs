use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use mesh_core_elements::style::{
    Color, Display, Edges, Overflow, TextAlign, TextDirection, TextOverflow,
};
use mesh_core_elements::{LayoutRect, NodeId, WidgetNode};

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
    pub damage_rect: DamageRect,
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
    retained_tree_generation: Option<u64>,
    surface_size: Option<(u32, u32)>,
    entries: HashMap<DisplayListKey, DisplayListEntry>,
    paint_commands: Vec<DisplayPaintCommand>,
    last_metrics: DisplayListMetrics,
}

#[derive(Debug, Clone)]
pub struct DisplayPaintCommand {
    pub node: DisplayPaintNode,
    pub clip: DisplayListClip,
    pub kind: DisplayPaintCommandKind,
}

#[derive(Debug, Clone)]
pub struct DisplayPaintNode {
    pub id: NodeId,
    pub layout: LayoutRect,
    pub style: DisplayPaintStyle,
    pub content: DisplayPaintContent,
    pub scrollbars: DisplayScrollbars,
}

#[derive(Debug, Clone)]
pub struct DisplayPaintStyle {
    pub background_color: Color,
    pub border_color: Color,
    pub border_width: Edges,
    pub border_radius: f32,
    pub color: Color,
    pub padding: Edges,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub text_align: TextAlign,
    pub text_overflow: TextOverflow,
    pub text_direction: TextDirection,
    pub icon_fill: Option<f32>,
    pub icon_weight: Option<f32>,
    pub icon_grade: Option<f32>,
    pub icon_optical_size: Option<f32>,
}

#[derive(Debug, Clone)]
pub enum DisplayPaintContent {
    None,
    Text(DisplayTextPaint),
    Input(DisplayInputPaint),
    Slider(DisplaySliderPaint),
    Icon(DisplayIconPaint),
}

#[derive(Debug, Clone)]
pub struct DisplayTextPaint {
    pub text: String,
    pub selection: Option<DisplayTextSelectionPaint>,
}

#[derive(Debug, Clone, Copy)]
pub struct DisplayTextSelectionPaint {
    pub background: Color,
    pub foreground: Color,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub focus_x: f32,
    pub focus_y: f32,
    pub text_x: f32,
    pub text_y: f32,
}

#[derive(Debug, Clone)]
pub struct DisplayInputPaint {
    pub value: String,
    pub placeholder: String,
    pub mask_text: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct DisplaySliderPaint {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub vertical: bool,
}

#[derive(Debug, Clone)]
pub struct DisplayIconPaint {
    pub src: Option<String>,
    pub name: Option<String>,
    pub size: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DisplayScrollbars {
    pub max_x: f32,
    pub max_y: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub content_width: f32,
    pub content_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPaintCommandKind {
    Node,
    Scrollbars,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayListClip {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
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
        self.update_inner(
            root,
            None,
            surface_width,
            surface_height,
            force_full_damage,
            partial_present_supported,
        )
    }

    pub fn update_for_retained_generation(
        &mut self,
        root: &WidgetNode,
        retained_tree_generation: u64,
        surface_width: u32,
        surface_height: u32,
        force_full_damage: bool,
        partial_present_supported: bool,
    ) -> DisplayListMetrics {
        self.update_inner(
            root,
            Some(retained_tree_generation),
            surface_width,
            surface_height,
            force_full_damage,
            partial_present_supported,
        )
    }

    fn update_inner(
        &mut self,
        root: &WidgetNode,
        retained_tree_generation: Option<u64>,
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
        if retained_tree_generation.is_some()
            && self.retained_tree_generation == retained_tree_generation
            && self.surface_size == Some((surface.width, surface.height))
        {
            return self.update_metrics_without_rebuild(
                surface,
                force_full_damage,
                partial_present_supported,
            );
        }

        let mut ordered_entries = Vec::new();
        let mut next = HashMap::new();
        collect_display_entries(root, &mut ordered_entries, &mut next);
        let mut paint_commands = Vec::new();
        collect_paint_commands(root, 0.0, 0.0, surface_clip(surface), &mut paint_commands);

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

        let mut removed = 0u64;
        for (key, previous) in &self.entries {
            if !next.contains_key(key) {
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
        self.paint_commands = paint_commands;
        self.retained_tree_generation = retained_tree_generation;
        self.surface_size = Some((surface.width, surface.height));
        self.last_metrics = DisplayListMetrics {
            retained_generation: self.generation,
            entries_total: self.entries.len() as u64,
            entries_reused: reused,
            entries_rebuilt: rebuilt,
            entries_removed: removed,
            damage_rect,
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

    fn update_metrics_without_rebuild(
        &mut self,
        surface: DamageRect,
        force_full_damage: bool,
        partial_present_supported: bool,
    ) -> DisplayListMetrics {
        let damage_rect = if force_full_damage {
            surface
        } else {
            DamageRect::default()
        };
        let damage_rect = clip_rect(damage_rect, surface).unwrap_or_default();
        let damage_area = damage_rect.area();
        let surface_area = surface.area();
        let skipped_paint_pixels = if partial_present_supported {
            surface_area.saturating_sub(damage_area)
        } else {
            0
        };
        self.last_metrics = DisplayListMetrics {
            retained_generation: self.generation,
            entries_total: self.entries.len() as u64,
            entries_reused: self.entries.len() as u64,
            entries_rebuilt: 0,
            entries_removed: 0,
            damage_rect,
            damage_rect_count: u64::from(damage_area > 0),
            damage_area,
            surface_area,
            full_surface_damage: force_full_damage,
            partial_present_supported,
            skipped_paint_pixels,
            batch_count: self.last_metrics.batch_count,
            batched_primitives: self.last_metrics.batched_primitives,
            barrier_count: self.last_metrics.barrier_count,
            barriers: self.last_metrics.barriers,
        };
        self.last_metrics
    }

    pub fn last_metrics(&self) -> DisplayListMetrics {
        self.last_metrics
    }

    pub fn paint_commands(&self) -> &[DisplayPaintCommand] {
        &self.paint_commands
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayListEntry {
    bounds: DamageRect,
    signature: u64,
    batch_signature: u64,
    barrier: Option<DisplayBatchBarrier>,
}

fn collect_display_entries(
    node: &WidgetNode,
    out: &mut Vec<(DisplayListKey, DisplayListEntry)>,
    next: &mut HashMap<DisplayListKey, DisplayListEntry>,
) {
    if let Some(bounds) = damage_rect_for_node(node) {
        for slot in primitive_slots_for_node(node) {
            let key = DisplayListKey {
                node_id: node.id,
                slot,
            };
            let entry = DisplayListEntry {
                bounds,
                signature: primitive_signature(node, slot),
                batch_signature: batch_signature(node, slot),
                barrier: batch_barrier(node, slot),
            };
            out.push((key, entry));
            next.insert(key, entry);
        }
    }
    for child in &node.children {
        collect_display_entries(child, out, next);
    }
}

fn collect_paint_commands(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: DisplayListClip,
    out: &mut Vec<DisplayPaintCommand>,
) {
    let style = &node.computed_style;
    if style.display == Display::None {
        return;
    }

    let transform = style.transform;
    let offset_x = offset_x + transform.translate_x;
    let offset_y = offset_y + transform.translate_y;
    let paint_node = build_paint_node(node, offset_x, offset_y);

    let bounds = node_clip_for(&paint_node);
    let node_clip = intersect_display_clip(clip, bounds);
    if node_clip.width <= 0 || node_clip.height <= 0 {
        return;
    }

    out.push(DisplayPaintCommand {
        node: paint_node.clone(),
        clip: node_clip,
        kind: DisplayPaintCommandKind::Node,
    });

    let scroll_x = node
        .attributes
        .get("_mesh_scroll_x")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0);
    let scroll_y = node
        .attributes
        .get("_mesh_scroll_y")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0);
    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    let child_clip = if node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents()
    {
        node_clip
    } else {
        clip
    };

    let needs_sort = node
        .children
        .windows(2)
        .any(|pair| pair[0].computed_style.z_index != pair[1].computed_style.z_index);
    if needs_sort {
        let mut child_order: Vec<usize> = (0..node.children.len()).collect();
        child_order.sort_by_key(|&index| node.children[index].computed_style.z_index);
        for index in child_order {
            collect_paint_commands(
                &node.children[index],
                child_offset_x,
                child_offset_y,
                child_clip,
                out,
            );
        }
    } else {
        for child in &node.children {
            collect_paint_commands(child, child_offset_x, child_offset_y, child_clip, out);
        }
    }

    out.push(DisplayPaintCommand {
        node: paint_node,
        clip: node_clip,
        kind: DisplayPaintCommandKind::Scrollbars,
    });
}

fn surface_clip(surface: DamageRect) -> DisplayListClip {
    DisplayListClip {
        x: surface.x as i32,
        y: surface.y as i32,
        width: surface.width as i32,
        height: surface.height as i32,
    }
}

fn node_clip_for(node: &DisplayPaintNode) -> DisplayListClip {
    DisplayListClip {
        x: node.layout.x.round() as i32,
        y: node.layout.y.round() as i32,
        width: node.layout.width.round().max(0.0) as i32,
        height: node.layout.height.round().max(0.0) as i32,
    }
}

fn build_paint_node(node: &WidgetNode, offset_x: f32, offset_y: f32) -> DisplayPaintNode {
    DisplayPaintNode {
        id: node.id,
        layout: LayoutRect {
            x: node.layout.x + offset_x,
            y: node.layout.y + offset_y,
            width: node.layout.width,
            height: node.layout.height,
        },
        style: DisplayPaintStyle {
            background_color: node.computed_style.background_color,
            border_color: node.computed_style.border_color,
            border_width: node.computed_style.border_width,
            border_radius: node.computed_style.border_radius.top_left,
            color: node.computed_style.color,
            padding: node.computed_style.padding,
            overflow_x: node.computed_style.overflow_x,
            overflow_y: node.computed_style.overflow_y,
            font_family: node.computed_style.font_family.clone(),
            font_size: node.computed_style.font_size,
            font_weight: node.computed_style.font_weight,
            line_height: node.computed_style.line_height,
            text_align: node.computed_style.text_align,
            text_overflow: node.computed_style.text_overflow,
            text_direction: node.computed_style.text_direction,
            icon_fill: node.computed_style.icon_fill,
            icon_weight: node.computed_style.icon_weight,
            icon_grade: node.computed_style.icon_grade,
            icon_optical_size: node.computed_style.icon_optical_size,
        },
        content: build_paint_content(node),
        scrollbars: DisplayScrollbars {
            max_x: attr_f32(node, "_mesh_scroll_max_x"),
            max_y: attr_f32(node, "_mesh_scroll_max_y"),
            scroll_x: attr_f32(node, "_mesh_scroll_x"),
            scroll_y: attr_f32(node, "_mesh_scroll_y"),
            content_width: attr_f32(node, "_mesh_content_width"),
            content_height: attr_f32(node, "_mesh_content_height"),
        },
    }
}

fn build_paint_content(node: &WidgetNode) -> DisplayPaintContent {
    match node.tag.as_str() {
        "text" => DisplayPaintContent::Text(DisplayTextPaint {
            text: node
                .attributes
                .get("text")
                .cloned()
                .or_else(|| node.attributes.get("content").cloned())
                .unwrap_or_default(),
            selection: build_text_selection(node),
        }),
        "input" => DisplayPaintContent::Input(DisplayInputPaint {
            value: node.attributes.get("value").cloned().unwrap_or_default(),
            placeholder: node
                .attributes
                .get("placeholder")
                .cloned()
                .unwrap_or_default(),
            mask_text: node
                .attributes
                .get("type")
                .is_some_and(|value| value == "password"),
            focused: node
                .attributes
                .get("_mesh_focused")
                .is_some_and(|value| value == "true"),
        }),
        "slider" => DisplayPaintContent::Slider(DisplaySliderPaint {
            min: attr_f32_with_default(node, "min", 0.0),
            max: attr_f32_with_default(node, "max", 100.0),
            value: attr_f32_with_default(node, "value", 50.0),
            vertical: node
                .attributes
                .get("orient")
                .is_some_and(|value| value == "vertical"),
        }),
        "icon" => DisplayPaintContent::Icon(DisplayIconPaint {
            src: node.attributes.get("src").cloned(),
            name: node.attributes.get("name").cloned(),
            size: node
                .attributes
                .get("size")
                .and_then(|value| value.parse::<u32>().ok()),
        }),
        _ => DisplayPaintContent::None,
    }
}

fn build_text_selection(node: &WidgetNode) -> Option<DisplayTextSelectionPaint> {
    Some(DisplayTextSelectionPaint {
        background: Color::from_hex(node.attributes.get("_mesh_selection_background")?)?,
        foreground: Color::from_hex(node.attributes.get("_mesh_selection_foreground")?)?,
        anchor_x: node
            .attributes
            .get("_mesh_selection_anchor_x")?
            .parse::<f32>()
            .ok()?,
        anchor_y: node
            .attributes
            .get("_mesh_selection_anchor_y")?
            .parse::<f32>()
            .ok()?,
        focus_x: node
            .attributes
            .get("_mesh_selection_focus_x")?
            .parse::<f32>()
            .ok()?,
        focus_y: node
            .attributes
            .get("_mesh_selection_focus_y")?
            .parse::<f32>()
            .ok()?,
        text_x: attr_f32(node, "_mesh_selection_text_x"),
        text_y: attr_f32(node, "_mesh_selection_text_y"),
    })
}

fn attr_f32(node: &WidgetNode, key: &str) -> f32 {
    attr_f32_with_default(node, key, 0.0)
}

fn attr_f32_with_default(node: &WidgetNode, key: &str, default: f32) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(default)
}

fn intersect_display_clip(a: DisplayListClip, b: DisplayListClip) -> DisplayListClip {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);

    DisplayListClip {
        x: x1,
        y: y1,
        width: (x2 - x1).max(0),
        height: (y2 - y1).max(0),
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
    hash_attribute(node, "content", &mut hasher);
    hash_attribute(node, "text", &mut hasher);
    hash_attribute(node, "name", &mut hasher);
    hash_attribute(node, "value", &mut hasher);
    hash_attribute(node, "placeholder", &mut hasher);
    hash_attribute(node, "type", &mut hasher);
    hash_attribute(node, "min", &mut hasher);
    hash_attribute(node, "max", &mut hasher);
    hash_attribute(node, "orient", &mut hasher);
    hash_attribute(node, "src", &mut hasher);
    hash_attribute(node, "size", &mut hasher);
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

fn hash_attribute(
    node: &WidgetNode,
    key: &str,
    hasher: &mut std::collections::hash_map::DefaultHasher,
) {
    key.hash(hasher);
    node.attributes.get(key).hash(hasher);
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
    fn display_list_skips_rebuild_when_retained_generation_is_unchanged() {
        let mut root = node(1, "box", 0.0, 0.0, 100.0, 40.0);
        let mut list = RetainedDisplayList::default();

        let first = list.update_for_retained_generation(&root, 1, 100, 40, false, true);
        assert_eq!(first.entries_rebuilt, 1);
        assert_eq!(list.paint_commands().len(), 2);

        root.children.push(node(2, "text", 10.0, 0.0, 20.0, 10.0));
        let skipped = list.update_for_retained_generation(&root, 1, 100, 40, true, true);
        assert_eq!(skipped.entries_rebuilt, 0);
        assert_eq!(skipped.entries_reused, 1);
        assert_eq!(skipped.damage_area, 4_000);
        assert!(skipped.full_surface_damage);
        assert_eq!(
            list.paint_commands().len(),
            2,
            "paint command cache should be reused while retained generation is unchanged"
        );

        let rebuilt = list.update_for_retained_generation(&root, 2, 100, 40, false, true);
        assert_eq!(rebuilt.entries_rebuilt, 2);
        assert_eq!(list.paint_commands().len(), 4);
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

    #[test]
    fn display_list_rebuilds_when_slider_value_changes() {
        let mut root = node(1, "slider", 0.0, 0.0, 100.0, 20.0);
        root.attributes.insert("min".into(), "0".into());
        root.attributes.insert("max".into(), "100".into());
        root.attributes.insert("value".into(), "25".into());
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 20, false, true);

        root.attributes.insert("value".into(), "75".into());
        let metrics = list.update(&root, 100, 20, false, true);

        assert_eq!(metrics.entries_rebuilt, 1);
        assert_eq!(metrics.damage_area, 2_000);
    }

    #[test]
    fn display_list_rebuilds_when_border_width_changes() {
        let mut root = node(1, "box", 0.0, 0.0, 100.0, 20.0);
        root.computed_style.border_color = Color::WHITE;
        root.computed_style.border_width = mesh_core_elements::style::Edges::all(1.0);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 20, false, true);

        root.computed_style.border_width = mesh_core_elements::style::Edges::all(4.0);
        let metrics = list.update(&root, 100, 20, false, true);

        assert_eq!(metrics.entries_rebuilt, 2);
        assert_eq!(metrics.damage_area, 2_000);
    }

    #[test]
    fn display_list_stores_compact_paint_payloads() {
        let mut root = node(1, "box", 10.0, 20.0, 80.0, 30.0);
        root.computed_style.transform.translate_x = 5.0;
        root.computed_style.transform.translate_y = 7.0;
        root.computed_style.overflow_x = Overflow::Scroll;
        root.attributes
            .insert("_mesh_scroll_max_x".into(), "40".into());
        root.attributes
            .insert("_mesh_content_width".into(), "120".into());

        let mut text = node(2, "text", 20.0, 30.0, 20.0, 10.0);
        text.attributes.insert("content".into(), "hello".into());
        text.attributes
            .insert("_mesh_selection_background".into(), "#112233".into());
        text.attributes
            .insert("_mesh_selection_foreground".into(), "#ddeeff".into());
        text.attributes
            .insert("_mesh_selection_anchor_x".into(), "2".into());
        text.attributes
            .insert("_mesh_selection_anchor_y".into(), "3".into());
        text.attributes
            .insert("_mesh_selection_focus_x".into(), "8".into());
        text.attributes
            .insert("_mesh_selection_focus_y".into(), "9".into());
        text.attributes
            .insert("_mesh_selection_text_x".into(), "1".into());
        text.attributes
            .insert("_mesh_selection_text_y".into(), "1".into());
        root.children.push(text);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, false, false);

        let root_command = list
            .paint_commands()
            .iter()
            .find(|command| command.node.id == 1 && command.kind == DisplayPaintCommandKind::Node)
            .expect("root command");
        assert_eq!(root_command.node.layout.x, 15.0);
        assert_eq!(root_command.node.layout.y, 27.0);
        assert_eq!(root_command.node.scrollbars.max_x, 40.0);
        assert_eq!(root_command.node.scrollbars.content_width, 120.0);

        let text_command = list
            .paint_commands()
            .iter()
            .find(|command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node)
            .expect("text command");
        match &text_command.node.content {
            DisplayPaintContent::Text(text) => {
                assert_eq!(text.text, "hello");
                let selection = text.selection.expect("selection payload");
                assert_eq!(selection.anchor_x, 2.0);
                assert_eq!(selection.focus_y, 9.0);
            }
            other => panic!("expected text paint payload, got {other:?}"),
        }
    }
}
