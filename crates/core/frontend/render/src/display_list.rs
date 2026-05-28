use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

use mesh_core_elements::style::{
    BackgroundPaint, Color, Display, Edges, Overflow, TextAlign, TextDirection, TextOverflow,
    Visibility,
};
use mesh_core_elements::{BoxShadow, VisualFilter};
use mesh_core_elements::{LayoutRect, NodeId, WidgetNode};

use crate::RenderObjectDirtySummary;

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

    pub fn intersects(self, other: Self) -> bool {
        if self.width == 0 || self.height == 0 || other.width == 0 || other.height == 0 {
            return false;
        }
        let self_right = self.x.saturating_add(self.width);
        let self_bottom = self.y.saturating_add(self.height);
        let other_right = other.x.saturating_add(other.width);
        let other_bottom = other.y.saturating_add(other.height);
        self.x < other_right
            && self_right > other.x
            && self.y < other_bottom
            && self_bottom > other.y
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
    pub subtree_segments_reused: u64,
    pub subtree_segments_rebuilt: u64,
    pub subtree_commands_rebuilt: u64,
    pub changed_layout_count: u64,
    pub changed_paint_count: u64,
    pub effect_overflow_count: u64,
    pub fallback_promotion_count: u64,
    pub full_fallback_count: u64,
    pub broad_dirty_fallback_count: u64,
    pub damage_rect: DamageRect,
    pub damage_rect_count: u64,
    pub damage_area: u64,
    pub surface_area: u64,
    pub full_surface_damage: bool,
    pub partial_present_supported: bool,
    pub skipped_paint_pixels: u64,
    pub omitted_subtrees: u64,
    pub omitted_nodes: u64,
    pub omitted_commands: u64,
    pub preclipped_descendants: u64,
    pub repaint_policy: DisplayListRepaintPolicy,
    pub filtered_span_count: u64,
    pub filtered_command_count: u64,
    pub filtered_commands_skipped: u64,
    pub filtered_fallback_count: u64,
    pub batch_count: u64,
    pub batched_primitives: u64,
    pub barrier_count: u64,
    pub barriers: DisplayBatchBarrierCounts,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayListRepaintPolicy {
    MinimalDamage,
    BoundingRect,
    #[default]
    FullSurface,
}

impl DisplayListRepaintPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MinimalDamage => "minimal_damage",
            Self::BoundingRect => "bounding_rect",
            Self::FullSurface => "full_surface",
        }
    }
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

#[derive(Debug)]
pub struct RetainedDisplayList {
    generation: u64,
    retained_tree_generation: Option<u64>,
    root_id: Option<NodeId>,
    surface_size: Option<(u32, u32)>,
    entries: HashMap<DisplayListKey, DisplayListEntry>,
    subtrees: HashMap<NodeId, RetainedPaintSubtree>,
    command_spans: Vec<RetainedCommandSpan>,
    paint_commands: Arc<[DisplayPaintCommand]>,
    command_kinds: Arc<[DisplayPaintCommandKind]>,
    last_metrics: DisplayListMetrics,
}

impl Default for RetainedDisplayList {
    fn default() -> Self {
        Self {
            generation: 0,
            retained_tree_generation: None,
            root_id: None,
            surface_size: None,
            entries: HashMap::new(),
            subtrees: HashMap::new(),
            command_spans: Vec::new(),
            paint_commands: Vec::new().into(),
            command_kinds: Vec::new().into(),
            last_metrics: DisplayListMetrics::default(),
        }
    }
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
    pub background_paint: BackgroundPaint,
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
    pub opacity: f32,
    pub box_shadow: BoxShadow,
    pub filter: VisualFilter,
    pub backdrop_filter: VisualFilter,
    pub icon_fill: Option<f32>,
    pub icon_weight: Option<f32>,
    pub icon_grade: Option<f32>,
    pub icon_optical_size: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayPaintContent {
    None,
    Text(DisplayTextPaint),
    Input(DisplayInputPaint),
    Slider(DisplaySliderPaint),
    Icon(DisplayIconPaint),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayTextPaint {
    pub text: String,
    pub selection: Option<DisplayTextSelectionPaint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInputPaint {
    pub value: String,
    pub placeholder: String,
    pub mask_text: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplaySliderPaint {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub vertical: bool,
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone)]
pub struct SelectedDisplayListPaint<'a> {
    commands: &'a [DisplayPaintCommand],
    kinds: &'a [DisplayPaintCommandKind],
    selection: SelectedDisplayListSelection,
    metrics: DisplayListMetrics,
}

#[derive(Debug, Clone)]
enum SelectedDisplayListSelection {
    All,
    None,
    Spans {
        spans: Vec<SelectedCommandSpan>,
        command_count: usize,
    },
}

pub struct SelectedDisplayListPaintIter<'a> {
    commands: &'a [DisplayPaintCommand],
    state: SelectedDisplayListPaintIterState<'a>,
}

pub struct SelectedDisplayListPaintKindIter<'a> {
    commands: &'a [DisplayPaintCommand],
    kinds: &'a [DisplayPaintCommandKind],
    state: SelectedDisplayListPaintKindIterState<'a>,
}

enum SelectedDisplayListPaintIterState<'a> {
    All(std::slice::Iter<'a, DisplayPaintCommand>),
    None,
    Spans {
        spans: &'a [SelectedCommandSpan],
        span_index: usize,
        command_index: usize,
    },
}

enum SelectedDisplayListPaintKindIterState<'a> {
    All {
        index: usize,
    },
    None,
    Spans {
        spans: &'a [SelectedCommandSpan],
        span_index: usize,
        command_index: usize,
    },
}

impl<'a> Iterator for SelectedDisplayListPaintIter<'a> {
    type Item = &'a DisplayPaintCommand;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            SelectedDisplayListPaintIterState::All(iter) => iter.next(),
            SelectedDisplayListPaintIterState::None => None,
            SelectedDisplayListPaintIterState::Spans {
                spans,
                span_index,
                command_index,
            } => loop {
                let span = spans.get(*span_index)?;
                if *command_index >= span.end {
                    *span_index = span_index.saturating_add(1);
                    continue;
                }
                if *command_index < span.start {
                    *command_index = span.start;
                }
                let index = *command_index;
                *command_index = (*command_index).saturating_add(1);
                if let Some(command) = self.commands.get(index) {
                    return Some(command);
                }
            },
        }
    }
}

impl<'a> Iterator for SelectedDisplayListPaintKindIter<'a> {
    type Item = (&'a DisplayPaintCommand, DisplayPaintCommandKind);

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            SelectedDisplayListPaintKindIterState::All { index } => {
                let command_index = *index;
                *index = index.saturating_add(1);
                Some((
                    self.commands.get(command_index)?,
                    *self.kinds.get(command_index)?,
                ))
            }
            SelectedDisplayListPaintKindIterState::None => None,
            SelectedDisplayListPaintKindIterState::Spans {
                spans,
                span_index,
                command_index,
            } => loop {
                let span = spans.get(*span_index)?;
                if *command_index >= span.end {
                    *span_index = span_index.saturating_add(1);
                    continue;
                }
                if *command_index < span.start {
                    *command_index = span.start;
                }
                let index = *command_index;
                *command_index = (*command_index).saturating_add(1);
                if let (Some(command), Some(kind)) =
                    (self.commands.get(index), self.kinds.get(index))
                {
                    return Some((command, *kind));
                }
            },
        }
    }
}

impl<'a> SelectedDisplayListPaint<'a> {
    pub fn iter(&self) -> SelectedDisplayListPaintIter<'_> {
        SelectedDisplayListPaintIter {
            commands: self.commands,
            state: match &self.selection {
                SelectedDisplayListSelection::All => {
                    SelectedDisplayListPaintIterState::All(self.commands.iter())
                }
                SelectedDisplayListSelection::None => SelectedDisplayListPaintIterState::None,
                SelectedDisplayListSelection::Spans { spans, .. } => {
                    SelectedDisplayListPaintIterState::Spans {
                        spans,
                        span_index: 0,
                        command_index: 0,
                    }
                }
            },
        }
    }

    pub fn iter_with_kinds(&self) -> SelectedDisplayListPaintKindIter<'_> {
        SelectedDisplayListPaintKindIter {
            commands: self.commands,
            kinds: self.kinds,
            state: match &self.selection {
                SelectedDisplayListSelection::All => {
                    SelectedDisplayListPaintKindIterState::All { index: 0 }
                }
                SelectedDisplayListSelection::None => SelectedDisplayListPaintKindIterState::None,
                SelectedDisplayListSelection::Spans { spans, .. } => {
                    SelectedDisplayListPaintKindIterState::Spans {
                        spans,
                        span_index: 0,
                        command_index: 0,
                    }
                }
            },
        }
    }

    pub fn len(&self) -> usize {
        match &self.selection {
            SelectedDisplayListSelection::All => self.commands.len(),
            SelectedDisplayListSelection::None => 0,
            SelectedDisplayListSelection::Spans { command_count, .. } => *command_count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn metrics(&self) -> DisplayListMetrics {
        self.metrics
    }
}

#[derive(Debug, Clone)]
struct RetainedPaintSubtree {
    commands: Arc<[DisplayPaintCommand]>,
    kinds: Arc<[DisplayPaintCommandKind]>,
    pruning: PruningMetrics,
    command_span: Option<RetainedSubtreeSpan>,
    child_order: Option<Arc<[usize]>>,
}

impl Default for RetainedPaintSubtree {
    fn default() -> Self {
        Self {
            commands: Vec::new().into(),
            kinds: Vec::new().into(),
            pruning: PruningMetrics::default(),
            command_span: None,
            child_order: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RetainedSubtreeSpan {
    bounds: DamageRect,
    local_bounds: DamageRect,
    command_count: usize,
    includes_scrollbars: bool,
}

#[derive(Debug, Default)]
struct PaintSubtreeBuilder {
    commands: Vec<DisplayPaintCommand>,
    kinds: Vec<DisplayPaintCommandKind>,
    pruning: PruningMetrics,
    bounds: DamageRect,
    local_bounds: DamageRect,
    includes_scrollbars: bool,
    local_command_count: usize,
    child_order: Option<Arc<[usize]>>,
}

impl PaintSubtreeBuilder {
    fn push_command(&mut self, command: DisplayPaintCommand) {
        let kind = command.kind;
        let bounds = command_bounds(&command);
        self.bounds = if self.bounds.width == 0 || self.bounds.height == 0 {
            bounds
        } else {
            self.bounds.union(bounds)
        };
        self.local_bounds = if self.local_bounds.width == 0 || self.local_bounds.height == 0 {
            bounds
        } else {
            self.local_bounds.union(bounds)
        };
        self.includes_scrollbars |= matches!(command.kind, DisplayPaintCommandKind::Scrollbars);
        self.local_command_count = self.local_command_count.saturating_add(1);
        self.commands.push(command);
        self.kinds.push(kind);
    }

    fn append_child(&mut self, child_subtree: &RetainedPaintSubtree) {
        if let Some(span) = child_subtree.command_span {
            self.bounds = if self.bounds.width == 0 || self.bounds.height == 0 {
                span.bounds
            } else {
                self.bounds.union(span.bounds)
            };
            self.includes_scrollbars |= span.includes_scrollbars;
        }

        self.commands.reserve(child_subtree.commands.len());
        self.commands.extend_from_slice(&child_subtree.commands);
        self.kinds.reserve(child_subtree.kinds.len());
        self.kinds.extend_from_slice(&child_subtree.kinds);
    }

    fn append_pruning(&mut self, child_subtree: &RetainedPaintSubtree) {
        self.pruning.omitted_subtrees = self
            .pruning
            .omitted_subtrees
            .saturating_add(child_subtree.pruning.omitted_subtrees);
        self.pruning.omitted_nodes = self
            .pruning
            .omitted_nodes
            .saturating_add(child_subtree.pruning.omitted_nodes);
        self.pruning.omitted_commands = self
            .pruning
            .omitted_commands
            .saturating_add(child_subtree.pruning.omitted_commands);
        self.pruning.preclipped_descendants = self
            .pruning
            .preclipped_descendants
            .saturating_add(child_subtree.pruning.preclipped_descendants);
    }

    fn into_retained(self) -> RetainedPaintSubtree {
        let command_count = self.local_command_count;
        let command_span = if command_count == 0 {
            None
        } else {
            Some(RetainedSubtreeSpan {
                bounds: self.bounds,
                local_bounds: self.local_bounds,
                command_count,
                includes_scrollbars: self.includes_scrollbars,
            })
        };
        RetainedPaintSubtree {
            commands: self.commands.into(),
            kinds: self.kinds.into(),
            pruning: self.pruning,
            command_span,
            child_order: self.child_order,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RetainedCommandSpan {
    owner: NodeId,
    start: usize,
    end: usize,
    bounds: DamageRect,
    command_count: usize,
    includes_scrollbars: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedCommandSpan {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LocalReuseMetrics {
    reused_segments: u64,
    rebuilt_segments: u64,
    rebuilt_commands: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalReuseDecision {
    RebuildDirtySubtrees,
    FallbackFull { broad_dirty: bool },
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
            None,
            None,
            surface_width,
            surface_height,
            force_full_damage,
            partial_present_supported,
        )
    }

    pub fn update_with_dirty_nodes(
        &mut self,
        root: &WidgetNode,
        dirty_summary: RenderObjectDirtySummary,
        dirty_node_ids: &HashSet<NodeId>,
        surface_width: u32,
        surface_height: u32,
        force_full_damage: bool,
        partial_present_supported: bool,
    ) -> DisplayListMetrics {
        self.update_inner(
            root,
            None,
            Some(dirty_summary),
            Some(dirty_node_ids),
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
        dirty_summary: RenderObjectDirtySummary,
        dirty_node_ids: &HashSet<NodeId>,
        surface_width: u32,
        surface_height: u32,
        force_full_damage: bool,
        partial_present_supported: bool,
    ) -> DisplayListMetrics {
        self.update_inner(
            root,
            Some(retained_tree_generation),
            Some(dirty_summary),
            Some(dirty_node_ids),
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
        dirty_summary: Option<RenderObjectDirtySummary>,
        dirty_node_ids: Option<&HashSet<NodeId>>,
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
        collect_display_entries(root, 0.0, 0.0, &mut ordered_entries, &mut next);
        let dirty_summary = dirty_summary.unwrap_or_default();
        let empty_dirty_nodes = HashSet::new();
        let dirty_node_ids = dirty_node_ids.unwrap_or(&empty_dirty_nodes);
        let decision = self.local_reuse_decision(
            root,
            dirty_summary,
            dirty_node_ids,
            surface.width,
            surface.height,
        );
        let (paint_commands, command_kinds, pruning, subtrees, local_metrics) = match decision {
            LocalReuseDecision::RebuildDirtySubtrees => {
                let rebuild_ancestors = collect_dirty_ancestor_ids(root, dirty_node_ids);
                let mut next_subtrees = HashMap::new();
                let mut local_metrics = LocalReuseMetrics::default();
                let subtree = build_paint_subtree(
                    root,
                    0.0,
                    0.0,
                    surface_clip(surface),
                    false,
                    dirty_node_ids,
                    &rebuild_ancestors,
                    &self.subtrees,
                    &mut next_subtrees,
                    &mut local_metrics,
                );
                (
                    subtree.commands,
                    subtree.kinds,
                    subtree.pruning,
                    next_subtrees,
                    local_metrics,
                )
            }
            LocalReuseDecision::FallbackFull { .. } => {
                let mut next_subtrees = HashMap::new();
                let mut local_metrics = LocalReuseMetrics::default();
                let subtree = build_paint_subtree(
                    root,
                    0.0,
                    0.0,
                    surface_clip(surface),
                    true,
                    dirty_node_ids,
                    &HashSet::new(),
                    &HashMap::new(),
                    &mut next_subtrees,
                    &mut local_metrics,
                );
                (
                    subtree.commands,
                    subtree.kinds,
                    subtree.pruning,
                    next_subtrees,
                    local_metrics,
                )
            }
        };

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
        let command_spans = build_command_spans(root, &subtrees);
        let effect_overflow_count = count_effect_overflow_commands(paint_commands.as_ref());

        if rebuilt > 0 || removed > 0 || force_full_damage {
            self.generation = self.generation.saturating_add(1);
        }
        self.entries = next;
        self.subtrees = subtrees;
        self.command_spans = command_spans;
        self.paint_commands = paint_commands;
        self.command_kinds = command_kinds;
        self.root_id = Some(root.id);
        self.retained_tree_generation = retained_tree_generation;
        self.surface_size = Some((surface.width, surface.height));
        let (full_fallback_count, broad_dirty_fallback_count) = match decision {
            LocalReuseDecision::FallbackFull { broad_dirty } if !self.entries.is_empty() => {
                (1, u64::from(broad_dirty))
            }
            LocalReuseDecision::FallbackFull { broad_dirty } => (1, u64::from(broad_dirty)),
            _ => (0, 0),
        };
        self.last_metrics = DisplayListMetrics {
            retained_generation: self.generation,
            entries_total: self.entries.len() as u64,
            entries_reused: reused,
            entries_rebuilt: rebuilt,
            entries_removed: removed,
            subtree_segments_reused: local_metrics.reused_segments,
            subtree_segments_rebuilt: local_metrics.rebuilt_segments,
            subtree_commands_rebuilt: local_metrics.rebuilt_commands,
            changed_layout_count: changed_layout_count(dirty_summary),
            changed_paint_count: changed_paint_count(dirty_summary),
            effect_overflow_count,
            fallback_promotion_count: u64::from(full_surface_damage)
                + full_fallback_count
                + broad_dirty_fallback_count,
            full_fallback_count,
            broad_dirty_fallback_count,
            damage_rect,
            damage_rect_count: u64::from(damage_area > 0),
            damage_area,
            surface_area,
            full_surface_damage,
            partial_present_supported,
            skipped_paint_pixels,
            omitted_subtrees: pruning.omitted_subtrees,
            omitted_nodes: pruning.omitted_nodes,
            omitted_commands: pruning.omitted_commands,
            preclipped_descendants: pruning.preclipped_descendants,
            repaint_policy: DisplayListRepaintPolicy::FullSurface,
            filtered_span_count: 0,
            filtered_command_count: self.paint_commands.len() as u64,
            filtered_commands_skipped: 0,
            filtered_fallback_count: 0,
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
        let effect_overflow_count = count_effect_overflow_commands(self.paint_commands.as_ref());
        self.last_metrics = DisplayListMetrics {
            retained_generation: self.generation,
            entries_total: self.entries.len() as u64,
            entries_reused: self.entries.len() as u64,
            entries_rebuilt: 0,
            entries_removed: 0,
            subtree_segments_reused: self.subtrees.len() as u64,
            subtree_segments_rebuilt: 0,
            subtree_commands_rebuilt: 0,
            changed_layout_count: 0,
            changed_paint_count: 0,
            effect_overflow_count,
            fallback_promotion_count: u64::from(force_full_damage),
            full_fallback_count: 0,
            broad_dirty_fallback_count: 0,
            damage_rect,
            damage_rect_count: u64::from(damage_area > 0),
            damage_area,
            surface_area,
            full_surface_damage: force_full_damage,
            partial_present_supported,
            skipped_paint_pixels,
            omitted_subtrees: self.last_metrics.omitted_subtrees,
            omitted_nodes: self.last_metrics.omitted_nodes,
            omitted_commands: self.last_metrics.omitted_commands,
            preclipped_descendants: self.last_metrics.preclipped_descendants,
            repaint_policy: DisplayListRepaintPolicy::FullSurface,
            filtered_span_count: 0,
            filtered_command_count: self.paint_commands.len() as u64,
            filtered_commands_skipped: 0,
            filtered_fallback_count: 0,
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
        self.paint_commands.as_ref()
    }

    pub fn paint_command_kinds(&self) -> &[DisplayPaintCommandKind] {
        self.command_kinds.as_ref()
    }

    pub fn select_paint_commands(
        &self,
        damage: Option<DamageRect>,
        policy: DisplayListRepaintPolicy,
    ) -> SelectedDisplayListPaint<'_> {
        let mut metrics = self.last_metrics;
        metrics.repaint_policy = policy;
        metrics.filtered_span_count = 0;
        metrics.filtered_command_count = 0;
        metrics.filtered_commands_skipped = 0;
        metrics.filtered_fallback_count = 0;

        let full_commands = self.paint_commands.len() as u64;
        if self.paint_commands.is_empty() {
            metrics.filtered_commands_skipped = 0;
            return SelectedDisplayListPaint {
                commands: self.paint_commands.as_ref(),
                kinds: self.command_kinds.as_ref(),
                selection: SelectedDisplayListSelection::None,
                metrics,
            };
        }

        let Some(damage) = damage else {
            metrics.repaint_policy = DisplayListRepaintPolicy::MinimalDamage;
            metrics.filtered_commands_skipped = full_commands;
            return SelectedDisplayListPaint {
                commands: self.paint_commands.as_ref(),
                kinds: self.command_kinds.as_ref(),
                selection: SelectedDisplayListSelection::None,
                metrics,
            };
        };

        if matches!(policy, DisplayListRepaintPolicy::FullSurface) {
            metrics.filtered_span_count = self.command_spans.len() as u64;
            metrics.filtered_command_count = full_commands;
            metrics.filtered_fallback_count = u64::from(!self.paint_commands.is_empty());
            return SelectedDisplayListPaint {
                commands: self.paint_commands.as_ref(),
                kinds: self.command_kinds.as_ref(),
                selection: SelectedDisplayListSelection::All,
                metrics,
            };
        }

        if self
            .surface_size
            .is_some_and(|surface_size| damage_covers_surface(damage, surface_size))
        {
            metrics.repaint_policy = DisplayListRepaintPolicy::FullSurface;
            metrics.filtered_span_count = self.command_spans.len() as u64;
            metrics.filtered_command_count = full_commands;
            metrics.filtered_fallback_count = u64::from(!self.paint_commands.is_empty());
            return SelectedDisplayListPaint {
                commands: self.paint_commands.as_ref(),
                kinds: self.command_kinds.as_ref(),
                selection: SelectedDisplayListSelection::All,
                metrics,
            };
        }

        let mut selected_spans = Vec::with_capacity(self.command_spans.len().min(32));
        let mut matched_spans = 0u64;
        for span in &self.command_spans {
            if span.bounds.intersects(damage) {
                matched_spans = matched_spans.saturating_add(1);
                insert_selected_command_span(
                    &mut selected_spans,
                    SelectedCommandSpan {
                        start: span.start,
                        end: span.end,
                    },
                );
            }
        }
        if selected_spans.is_empty() {
            for (index, command) in self.paint_commands.iter().enumerate() {
                if command_bounds(command).intersects(damage) {
                    insert_selected_command_span(
                        &mut selected_spans,
                        SelectedCommandSpan {
                            start: index,
                            end: index.saturating_add(1),
                        },
                    );
                }
            }
        }
        let selected_command_count = selected_spans
            .iter()
            .map(|span| span.end.saturating_sub(span.start))
            .sum::<usize>();

        metrics.filtered_span_count = matched_spans;
        metrics.filtered_command_count = selected_command_count as u64;
        metrics.filtered_commands_skipped =
            full_commands.saturating_sub(selected_command_count as u64);

        SelectedDisplayListPaint {
            commands: self.paint_commands.as_ref(),
            kinds: self.command_kinds.as_ref(),
            selection: SelectedDisplayListSelection::Spans {
                spans: selected_spans,
                command_count: selected_command_count,
            },
            metrics,
        }
    }

    pub fn select_paint_commands_for_rects(
        &self,
        damages: &[DamageRect],
        policy: DisplayListRepaintPolicy,
    ) -> SelectedDisplayListPaint<'_> {
        const MAX_SPARSE_DAMAGE_RECTS: usize = 8;

        let mut metrics = self.last_metrics;
        metrics.repaint_policy = policy;
        metrics.filtered_span_count = 0;
        metrics.filtered_command_count = 0;
        metrics.filtered_commands_skipped = 0;
        metrics.filtered_fallback_count = 0;

        if damages.len() == 1 {
            return self.select_paint_commands(damages.first().copied(), policy);
        }
        if damages.len() > MAX_SPARSE_DAMAGE_RECTS {
            return self.select_paint_commands(union_damage_rects(damages), policy);
        }

        let full_commands = self.paint_commands.len() as u64;
        if self.paint_commands.is_empty() {
            metrics.filtered_commands_skipped = 0;
            return SelectedDisplayListPaint {
                commands: self.paint_commands.as_ref(),
                kinds: self.command_kinds.as_ref(),
                selection: SelectedDisplayListSelection::None,
                metrics,
            };
        }

        let Some(_) = damages.first() else {
            metrics.repaint_policy = DisplayListRepaintPolicy::MinimalDamage;
            metrics.filtered_commands_skipped = full_commands;
            return SelectedDisplayListPaint {
                commands: self.paint_commands.as_ref(),
                kinds: self.command_kinds.as_ref(),
                selection: SelectedDisplayListSelection::None,
                metrics,
            };
        };

        if matches!(policy, DisplayListRepaintPolicy::FullSurface) {
            metrics.filtered_span_count = self.command_spans.len() as u64;
            metrics.filtered_command_count = full_commands;
            metrics.filtered_fallback_count = u64::from(!self.paint_commands.is_empty());
            return SelectedDisplayListPaint {
                commands: self.paint_commands.as_ref(),
                kinds: self.command_kinds.as_ref(),
                selection: SelectedDisplayListSelection::All,
                metrics,
            };
        }

        let mut selected_spans = Vec::with_capacity(self.command_spans.len().min(32));
        let mut matched_spans = 0u64;
        for span in &self.command_spans {
            if rects_intersect_any(span.bounds, damages) {
                matched_spans = matched_spans.saturating_add(1);
                insert_selected_command_span(
                    &mut selected_spans,
                    SelectedCommandSpan {
                        start: span.start,
                        end: span.end,
                    },
                );
            }
        }

        if selected_spans.is_empty() {
            for (index, command) in self.paint_commands.iter().enumerate() {
                let command_bounds = command_bounds(command);
                if rects_intersect_any(command_bounds, damages) {
                    insert_selected_command_span(
                        &mut selected_spans,
                        SelectedCommandSpan {
                            start: index,
                            end: index.saturating_add(1),
                        },
                    );
                }
            }
        }

        let selected_command_count = selected_spans
            .iter()
            .map(|span| span.end.saturating_sub(span.start))
            .sum::<usize>();

        metrics.filtered_span_count = matched_spans;
        metrics.filtered_command_count = selected_command_count as u64;
        metrics.filtered_commands_skipped =
            full_commands.saturating_sub(selected_command_count as u64);

        SelectedDisplayListPaint {
            commands: self.paint_commands.as_ref(),
            kinds: self.command_kinds.as_ref(),
            selection: SelectedDisplayListSelection::Spans {
                spans: selected_spans,
                command_count: selected_command_count,
            },
            metrics,
        }
    }

    fn local_reuse_decision(
        &self,
        root: &WidgetNode,
        dirty_summary: RenderObjectDirtySummary,
        dirty_node_ids: &HashSet<NodeId>,
        surface_width: u32,
        surface_height: u32,
    ) -> LocalReuseDecision {
        if self.surface_size == Some((surface_width, surface_height))
            && self.root_id == Some(root.id)
            && !self.subtrees.is_empty()
            && dirty_summary.any()
        {
            if dirty_node_ids.is_empty() {
                return LocalReuseDecision::FallbackFull { broad_dirty: false };
            }
            let broad_limit = (self.subtrees.len() / 2).max(8);
            if dirty_node_ids.len() > broad_limit {
                return LocalReuseDecision::FallbackFull { broad_dirty: true };
            }
            return LocalReuseDecision::RebuildDirtySubtrees;
        }

        if self.surface_size == Some((surface_width, surface_height))
            && self.root_id == Some(root.id)
            && !self.subtrees.is_empty()
        {
            return LocalReuseDecision::FallbackFull {
                broad_dirty: dirty_node_ids.is_empty(),
            };
        }

        LocalReuseDecision::FallbackFull { broad_dirty: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayListEntry {
    bounds: DamageRect,
    signature: u64,
    batch_signature: u64,
    barrier: Option<DisplayBatchBarrier>,
}

struct DisplaySignatureHasher(u64);

impl Default for DisplaySignatureHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for DisplaySignatureHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PruningMetrics {
    omitted_subtrees: u64,
    omitted_nodes: u64,
    omitted_commands: u64,
    preclipped_descendants: u64,
}

impl PruningMetrics {
    fn record_omitted_subtree(&mut self, counts: PrunedSubtreeCounts, preclipped: bool) {
        if counts.nodes == 0 && counts.commands == 0 {
            return;
        }
        self.omitted_subtrees = self.omitted_subtrees.saturating_add(1);
        self.omitted_nodes = self.omitted_nodes.saturating_add(counts.nodes);
        self.omitted_commands = self.omitted_commands.saturating_add(counts.commands);
        if preclipped {
            self.preclipped_descendants = self.preclipped_descendants.saturating_add(counts.nodes);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PrunedSubtreeCounts {
    nodes: u64,
    commands: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FloatBounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl FloatBounds {
    fn intersects_clip(self, clip: DisplayListClip) -> bool {
        let clip_left = clip.x as f32;
        let clip_top = clip.y as f32;
        let clip_right = (clip.x + clip.width) as f32;
        let clip_bottom = (clip.y + clip.height) as f32;
        self.right > clip_left
            && self.bottom > clip_top
            && self.left < clip_right
            && self.top < clip_bottom
    }

    fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

fn collect_display_entries(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    out: &mut Vec<(DisplayListKey, DisplayListEntry)>,
    next: &mut HashMap<DisplayListKey, DisplayListEntry>,
) {
    if node_is_explicitly_hidden(node) {
        return;
    }

    let style = &node.computed_style;
    let transform = style.transform;
    let offset_x = offset_x + transform.translate_x;
    let offset_y = offset_y + transform.translate_y;

    if let Some(bounds) = damage_rect_for_node_at(node, offset_x, offset_y) {
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

    for child in &node.children {
        collect_display_entries(child, child_offset_x, child_offset_y, out, next);
    }
}

fn collect_dirty_ancestor_ids(
    root: &WidgetNode,
    dirty_node_ids: &HashSet<NodeId>,
) -> HashSet<NodeId> {
    let mut ancestors = HashSet::new();
    let mut path = Vec::new();
    collect_dirty_ancestor_ids_inner(root, dirty_node_ids, &mut path, &mut ancestors);
    ancestors
}

fn collect_dirty_ancestor_ids_inner(
    node: &WidgetNode,
    dirty_node_ids: &HashSet<NodeId>,
    path: &mut Vec<NodeId>,
    ancestors: &mut HashSet<NodeId>,
) {
    let is_dirty = dirty_node_ids.contains(&node.id);
    if is_dirty {
        for ancestor in path.iter().copied() {
            ancestors.insert(ancestor);
        }
    }
    path.push(node.id);
    for child in &node.children {
        collect_dirty_ancestor_ids_inner(child, dirty_node_ids, path, ancestors);
    }
    path.pop();
}

#[allow(clippy::too_many_arguments)]
fn build_paint_subtree(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: DisplayListClip,
    force_rebuild: bool,
    dirty_node_ids: &HashSet<NodeId>,
    dirty_ancestors: &HashSet<NodeId>,
    previous_subtrees: &HashMap<NodeId, RetainedPaintSubtree>,
    next_subtrees: &mut HashMap<NodeId, RetainedPaintSubtree>,
    metrics: &mut LocalReuseMetrics,
) -> RetainedPaintSubtree {
    let node_is_dirty = dirty_node_ids.contains(&node.id);
    let node_is_ancestor = dirty_ancestors.contains(&node.id);
    if !force_rebuild
        && !node_is_dirty
        && !node_is_ancestor
        && let Some(previous) = previous_subtrees.get(&node.id)
    {
        metrics.reused_segments = metrics.reused_segments.saturating_add(1);
        let reused = previous.clone();
        next_subtrees.insert(node.id, reused.clone());
        return reused;
    }

    metrics.rebuilt_segments = metrics.rebuilt_segments.saturating_add(1);

    if node_is_explicitly_hidden(node) {
        let mut subtree = RetainedPaintSubtree::default();
        subtree
            .pruning
            .record_omitted_subtree(count_pruned_subtree(node, offset_x, offset_y, true), false);
        next_subtrees.insert(node.id, subtree.clone());
        return subtree;
    }

    let style = &node.computed_style;
    let transform = style.transform;
    let offset_x = offset_x + transform.translate_x;
    let offset_y = offset_y + transform.translate_y;
    let paint_node = build_paint_node(node, offset_x, offset_y);
    let bounds = node_clip_for(&paint_node);
    let visual_bounds = visual_clip_for(&paint_node);
    let node_clip = intersect_display_clip(clip, visual_bounds);
    if node_clip.width <= 0 || node_clip.height <= 0 {
        let mut subtree = RetainedPaintSubtree::default();
        subtree
            .pruning
            .record_omitted_subtree(count_pruned_subtree(node, offset_x, offset_y, false), true);
        next_subtrees.insert(node.id, subtree.clone());
        return subtree;
    }

    let mut subtree = PaintSubtreeBuilder::default();
    subtree.push_command(DisplayPaintCommand {
        node: paint_node.clone(),
        clip: node_clip,
        kind: DisplayPaintCommandKind::Node,
    });
    metrics.rebuilt_commands = metrics.rebuilt_commands.saturating_add(1);

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
        intersect_display_clip(clip, bounds)
    } else {
        clip
    };
    let child_order = compute_child_order(node);
    for_children_in_order(node, child_order.as_deref(), |child| {
        append_child_paint_subtree(
            &mut subtree,
            child,
            child_offset_x,
            child_offset_y,
            child_clip,
            force_rebuild || node_is_dirty,
            dirty_node_ids,
            dirty_ancestors,
            previous_subtrees,
            next_subtrees,
            metrics,
        );
    });
    subtree.child_order = child_order;

    if display_node_may_show_scrollbars(&paint_node) {
        subtree.push_command(DisplayPaintCommand {
            node: paint_node,
            clip: node_clip,
            kind: DisplayPaintCommandKind::Scrollbars,
        });
        metrics.rebuilt_commands = metrics.rebuilt_commands.saturating_add(1);
    }
    let subtree = subtree.into_retained();
    next_subtrees.insert(node.id, subtree.clone());
    subtree
}

fn display_node_may_show_scrollbars(node: &DisplayPaintNode) -> bool {
    node.style.overflow_y.always_shows_scrollbar()
        || node.style.overflow_x.always_shows_scrollbar()
        || (node.style.overflow_y.shows_scrollbar_when_overflowing()
            && node.scrollbars.max_y > f32::EPSILON)
        || (node.style.overflow_x.shows_scrollbar_when_overflowing()
            && node.scrollbars.max_x > f32::EPSILON)
}

fn damage_covers_surface(damage: DamageRect, surface_size: (u32, u32)) -> bool {
    damage.x == 0
        && damage.y == 0
        && damage.width >= surface_size.0
        && damage.height >= surface_size.1
}

fn union_damage_rects(damages: &[DamageRect]) -> Option<DamageRect> {
    let first = damages.first().copied()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x.saturating_add(first.width);
    let mut max_y = first.y.saturating_add(first.height);
    for damage in &damages[1..] {
        min_x = min_x.min(damage.x);
        min_y = min_y.min(damage.y);
        max_x = max_x.max(damage.x.saturating_add(damage.width));
        max_y = max_y.max(damage.y.saturating_add(damage.height));
    }
    Some(DamageRect {
        x: min_x,
        y: min_y,
        width: max_x.saturating_sub(min_x),
        height: max_y.saturating_sub(min_y),
    })
}

#[allow(clippy::too_many_arguments)]
fn append_child_paint_subtree(
    subtree: &mut PaintSubtreeBuilder,
    child: &WidgetNode,
    child_offset_x: f32,
    child_offset_y: f32,
    child_clip: DisplayListClip,
    force_rebuild: bool,
    dirty_node_ids: &HashSet<NodeId>,
    dirty_ancestors: &HashSet<NodeId>,
    previous_subtrees: &HashMap<NodeId, RetainedPaintSubtree>,
    next_subtrees: &mut HashMap<NodeId, RetainedPaintSubtree>,
    metrics: &mut LocalReuseMetrics,
) {
    if should_preclip_child_subtree(child, child_offset_x, child_offset_y, child_clip) {
        subtree.pruning.record_omitted_subtree(
            count_pruned_subtree(child, child_offset_x, child_offset_y, false),
            true,
        );
        return;
    }
    let child_subtree = build_paint_subtree(
        child,
        child_offset_x,
        child_offset_y,
        child_clip,
        force_rebuild,
        dirty_node_ids,
        dirty_ancestors,
        previous_subtrees,
        next_subtrees,
        metrics,
    );
    subtree.append_child(&child_subtree);
    subtree.append_pruning(&child_subtree);
}

fn for_children_in_order(
    node: &WidgetNode,
    child_order: Option<&[usize]>,
    mut visit: impl FnMut(&WidgetNode),
) {
    let Some(child_order) = child_order else {
        for child in &node.children {
            visit(child);
        }
        return;
    };

    for child_index in child_order {
        visit(&node.children[*child_index]);
    }
}

fn compute_child_order(node: &WidgetNode) -> Option<Arc<[usize]>> {
    let child_count = node.children.len();
    if child_count <= 1 {
        return None;
    }

    let mut has_inversion = false;
    let mut previous_z_index = node.children[0].computed_style.z_index;
    for child in node.children.iter().skip(1) {
        if previous_z_index > child.computed_style.z_index {
            has_inversion = true;
            break;
        }
        previous_z_index = child.computed_style.z_index;
    }
    if !has_inversion {
        return None;
    }

    let mut child_order: Vec<usize> = (0..child_count).collect();
    child_order.sort_unstable_by_key(|&index| node.children[index].computed_style.z_index);
    Some(child_order.into_boxed_slice().into())
}

fn node_is_explicitly_hidden(node: &WidgetNode) -> bool {
    node.computed_style.display == Display::None
        || matches!(
            node.computed_style.visibility,
            Visibility::Hidden | Visibility::Collapse
        )
        || node
            .attributes
            .get("hidden")
            .is_some_and(|value| truthy_attribute(value))
}

fn truthy_attribute(value: &str) -> bool {
    matches!(value, "" | "true" | "1" | "hidden" | "disabled" | "checked")
}

fn should_preclip_child_subtree(
    child: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: DisplayListClip,
) -> bool {
    subtree_bounds_at(child, offset_x, offset_y).is_some_and(|bounds| !bounds.intersects_clip(clip))
}

fn subtree_bounds_at(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Option<FloatBounds> {
    if node_is_explicitly_hidden(node) {
        return None;
    }

    let transform = node.computed_style.transform;
    let offset_x = offset_x + transform.translate_x;
    let offset_y = offset_y + transform.translate_y;
    let mut bounds = node_visual_bounds_at(node, offset_x, offset_y);
    let scroll_x = attr_f32(node, "_mesh_scroll_x");
    let scroll_y = attr_f32(node, "_mesh_scroll_y");
    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    for child in &node.children {
        if let Some(child_bounds) = subtree_bounds_at(child, child_offset_x, child_offset_y) {
            bounds = Some(match bounds {
                Some(existing) => existing.union(child_bounds),
                None => child_bounds,
            });
        }
    }
    bounds
}

fn node_visual_bounds_at(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Option<FloatBounds> {
    (node.layout.width > 0.0 && node.layout.height > 0.0).then(|| {
        let paint_node = build_paint_node(node, offset_x, offset_y);
        let visual = visual_clip_for(&paint_node);
        FloatBounds {
            left: visual.x as f32,
            top: visual.y as f32,
            right: (visual.x + visual.width) as f32,
            bottom: (visual.y + visual.height) as f32,
        }
    })
}

fn count_pruned_subtree(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    include_hidden_root: bool,
) -> PrunedSubtreeCounts {
    if node_is_explicitly_hidden(node) && !include_hidden_root {
        return PrunedSubtreeCounts::default();
    }

    let transform = node.computed_style.transform;
    let offset_x = offset_x + transform.translate_x;
    let offset_y = offset_y + transform.translate_y;
    let mut counts = PrunedSubtreeCounts::default();
    if node.layout.width > 0.0 && node.layout.height > 0.0 {
        counts.nodes = 1;
        counts.commands = 2;
    }
    let scroll_x = attr_f32(node, "_mesh_scroll_x");
    let scroll_y = attr_f32(node, "_mesh_scroll_y");
    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    for child in &node.children {
        let child_counts = count_pruned_subtree(child, child_offset_x, child_offset_y, false);
        counts.nodes = counts.nodes.saturating_add(child_counts.nodes);
        counts.commands = counts.commands.saturating_add(child_counts.commands);
    }
    counts
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

fn visual_clip_for(node: &DisplayPaintNode) -> DisplayListClip {
    let mut left = node.layout.x;
    let mut top = node.layout.y;
    let mut right = node.layout.x + node.layout.width;
    let mut bottom = node.layout.y + node.layout.height;
    let shadow = node.style.box_shadow;
    if !shadow.is_none() && !shadow.inset {
        let spread = shadow.spread_radius;
        let blur_pad = shadow.blur_radius * 3.0;
        left = left.min(node.layout.x + shadow.offset_x - spread - blur_pad);
        top = top.min(node.layout.y + shadow.offset_y - spread - blur_pad);
        right = right.max(node.layout.x + node.layout.width + shadow.offset_x + spread + blur_pad);
        bottom =
            bottom.max(node.layout.y + node.layout.height + shadow.offset_y + spread + blur_pad);
    }
    let filter_pad = node
        .style
        .filter
        .blur_radius
        .max(node.style.backdrop_filter.blur_radius)
        * 3.0;
    if filter_pad > 0.0 {
        left -= filter_pad;
        top -= filter_pad;
        right += filter_pad;
        bottom += filter_pad;
    }
    DisplayListClip {
        x: left.floor() as i32,
        y: top.floor() as i32,
        width: (right - left).ceil().max(0.0) as i32,
        height: (bottom - top).ceil().max(0.0) as i32,
    }
}

fn command_bounds(command: &DisplayPaintCommand) -> DamageRect {
    let bounds = visual_clip_for(&command.node);
    let clip = intersect_display_clip(bounds, command.clip);
    DamageRect {
        x: clip.x.max(0) as u32,
        y: clip.y.max(0) as u32,
        width: clip.width.max(0) as u32,
        height: clip.height.max(0) as u32,
    }
}

fn count_effect_overflow_commands(commands: &[DisplayPaintCommand]) -> u64 {
    commands
        .iter()
        .filter(|command| command.kind == DisplayPaintCommandKind::Node)
        .filter(|command| visual_clip_for(&command.node) != node_clip_for(&command.node))
        .count() as u64
}

fn changed_layout_count(dirty_summary: RenderObjectDirtySummary) -> u64 {
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

fn changed_paint_count(dirty_summary: RenderObjectDirtySummary) -> u64 {
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

fn build_command_spans(
    root: &WidgetNode,
    subtrees: &HashMap<NodeId, RetainedPaintSubtree>,
) -> Vec<RetainedCommandSpan> {
    let mut spans = Vec::new();
    if subtrees.is_empty() || !subtrees.contains_key(&root.id) {
        return spans;
    }

    collect_command_spans(root, subtrees, 0, &mut spans);
    spans
}

fn collect_command_spans(
    node: &WidgetNode,
    subtrees: &HashMap<NodeId, RetainedPaintSubtree>,
    command_start: usize,
    spans: &mut Vec<RetainedCommandSpan>,
) -> usize {
    let Some(subtree) = subtrees.get(&node.id) else {
        return command_start;
    };
    let subtree_end = command_start.saturating_add(subtree.commands.len());

    if let Some(span) = subtree.command_span {
        let owned = span.command_count;
        let has_children = subtree_end > command_start.saturating_add(owned);
        let bounds = span.local_bounds;
        if !has_children || owned <= 1 {
            spans.push(RetainedCommandSpan {
                owner: node.id,
                start: command_start,
                end: command_start.saturating_add(owned.min(2)),
                bounds,
                command_count: owned.min(2),
                includes_scrollbars: owned > 1,
            });
        } else {
            spans.push(RetainedCommandSpan {
                owner: node.id,
                start: command_start,
                end: command_start.saturating_add(1),
                bounds,
                command_count: 1,
                includes_scrollbars: false,
            });
            let scrollbar_index = subtree_end.saturating_sub(1);
            if span.includes_scrollbars {
                spans.push(RetainedCommandSpan {
                    owner: node.id,
                    start: scrollbar_index,
                    end: scrollbar_index.saturating_add(1),
                    bounds,
                    command_count: 1,
                    includes_scrollbars: true,
                });
            }
        }
    }

    if subtree.commands.is_empty() {
        return subtree_end;
    }

    let child_start = command_start.saturating_add(1);
    let mut next_child_start = child_start;
    let child_order = subtree.child_order.clone();
    for_children_in_order(node, child_order.as_deref(), |child| {
        next_child_start = collect_command_spans(child, subtrees, next_child_start, spans);
    });
    subtree_end
}

fn insert_selected_command_span(
    spans: &mut Vec<SelectedCommandSpan>,
    mut next: SelectedCommandSpan,
) {
    if next.start >= next.end {
        return;
    }
    let last_end_start = spans.last().map(|span| (span.start, span.end));
    let Some((last_start, last_end)) = last_end_start else {
        spans.push(next);
        return;
    };
    if next.start >= last_start {
        if next.start <= last_end {
            if let Some(last) = spans.last_mut() {
                last.end = last.end.max(next.end);
            }
            return;
        }
        spans.push(next);
        return;
    }

    let insert_index = spans.partition_point(|span| span.end < next.start);
    let mut next_index = insert_index;
    while next_index < spans.len() && spans[next_index].start <= next.end {
        let span = spans[next_index];
        next.start = next.start.min(span.start);
        next.end = next.end.max(span.end);
        next_index = next_index.saturating_add(1);
    }
    spans.drain(insert_index..next_index);
    spans.insert(insert_index, next);
}

fn rects_intersect_any(bounds: DamageRect, rects: &[DamageRect]) -> bool {
    rects.iter().copied().any(|rect| bounds.intersects(rect))
}

fn build_paint_node(node: &WidgetNode, offset_x: f32, offset_y: f32) -> DisplayPaintNode {
    let opacity = node.computed_style.opacity;
    DisplayPaintNode {
        id: node.id,
        layout: transformed_layout_at(node, offset_x, offset_y),
        style: DisplayPaintStyle {
            background_color: opacity_color(node.computed_style.background_color, opacity),
            background_paint: node.computed_style.background_paint.clone(),
            border_color: opacity_color(node.computed_style.border_color, opacity),
            border_width: node.computed_style.border_width,
            border_radius: node.computed_style.border_radius.top_left,
            color: opacity_color(node.computed_style.color, opacity),
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
            opacity,
            box_shadow: node.computed_style.box_shadow,
            filter: node.computed_style.filter,
            backdrop_filter: node.computed_style.backdrop_filter,
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

fn transformed_layout_at(node: &WidgetNode, offset_x: f32, offset_y: f32) -> LayoutRect {
    let scale_x = node.computed_style.transform.scale_x.max(0.0);
    let scale_y = node.computed_style.transform.scale_y.max(0.0);
    let base_x = node.layout.x + offset_x;
    let base_y = node.layout.y + offset_y;
    let width = node.layout.width * scale_x;
    let height = node.layout.height * scale_y;
    LayoutRect {
        x: base_x - (width - node.layout.width) * 0.5,
        y: base_y - (height - node.layout.height) * 0.5,
        width,
        height,
    }
}

fn opacity_color(color: Color, opacity: f32) -> Color {
    Color {
        a: ((color.a as f32) * opacity.clamp(0.0, 1.0))
            .round()
            .clamp(0.0, 255.0) as u8,
        ..color
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

fn damage_rect_for_node_at(node: &WidgetNode, offset_x: f32, offset_y: f32) -> Option<DamageRect> {
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

fn primitive_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
    let mut hasher = DisplaySignatureHasher::default();
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
    let mut hasher = DisplaySignatureHasher::default();
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
    hasher.finish()
}

fn hash_box_shadow(shadow: BoxShadow, hasher: &mut DisplaySignatureHasher) {
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

fn hash_background_paint(paint: &BackgroundPaint, hasher: &mut DisplaySignatureHasher) {
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

fn hash_attribute(node: &WidgetNode, key: &str, hasher: &mut DisplaySignatureHasher) {
    key.hash(hasher);
    node.attributes.get(key).hash(hasher);
}

fn batch_barrier(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> Option<DisplayBatchBarrier> {
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

fn cached_icon_resource_opacity(node: &WidgetNode) -> crate::surface::icon::CachedResourceOpacity {
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
    use mesh_core_elements::style::{
        BackgroundPaint, Color, Overflow, StyleImageSource, StyleLinearGradient, Visibility,
    };

    fn command_debugs(commands: &[DisplayPaintCommand], ids: &[NodeId]) -> Vec<String> {
        commands
            .iter()
            .filter(|command| ids.contains(&command.node.id))
            .map(|command| format!("{command:?}"))
            .collect()
    }

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
    fn display_list_effect_rebuilds_when_background_paint_changes() {
        let mut root = node(1, "box", 0.0, 0.0, 100.0, 40.0);
        let mut list = RetainedDisplayList::default();

        list.update(&root, 100, 40, false, false);
        root.computed_style.background_paint = BackgroundPaint::Image(StyleImageSource {
            path: "assets/first.png".to_string(),
        });
        let image_metrics = list.update(&root, 100, 40, false, false);
        assert_eq!(image_metrics.entries_rebuilt, 1);
        assert_eq!(image_metrics.entries_reused, 0);

        root.computed_style.background_paint =
            BackgroundPaint::LinearGradient(StyleLinearGradient {
                from: Color::from_hex("#112233").unwrap(),
                to: Color::from_hex("#445566").unwrap(),
            });
        let gradient_metrics = list.update(&root, 100, 40, false, false);
        assert_eq!(gradient_metrics.entries_rebuilt, 1);
        assert_eq!(gradient_metrics.entries_reused, 0);
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
    fn display_list_selects_blurred_background_outside_layout_bounds() {
        let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
        root.computed_style.filter = VisualFilter { blur_radius: 4.0 };

        let mut list = RetainedDisplayList::default();
        list.update(&root, 80, 80, false, true);
        let selected = list.select_paint_commands(
            Some(DamageRect {
                x: 10,
                y: 24,
                width: 2,
                height: 2,
            }),
            DisplayListRepaintPolicy::MinimalDamage,
        );

        assert!(
            selected.iter().any(|command| command.node.id == 1),
            "blurred visual bounds should participate in sparse repaint selection"
        );
    }

    #[test]
    fn display_list_effect_visual_clip_includes_shadow_and_filter_overflow() {
        let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
        root.computed_style.filter = VisualFilter { blur_radius: 4.0 };
        root.computed_style.box_shadow = BoxShadow {
            offset_x: 10.0,
            offset_y: 0.0,
            blur_radius: 4.0,
            spread_radius: 0.0,
            color: Color::from_hex("#00000080").unwrap(),
            inset: false,
        };

        let mut list = RetainedDisplayList::default();
        list.update(&root, 90, 90, false, true);
        let selected = list.select_paint_commands(
            Some(DamageRect {
                x: 50,
                y: 24,
                width: 2,
                height: 2,
            }),
            DisplayListRepaintPolicy::MinimalDamage,
        );

        assert!(selected.iter().any(|command| command.node.id == 1));
    }

    #[test]
    fn display_list_effect_visual_clip_includes_image_bounds() {
        let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
        root.computed_style.background_paint = BackgroundPaint::Image(StyleImageSource {
            path: "assets/panel.png".to_string(),
        });
        let paint_node = build_paint_node(&root, 0.0, 0.0);
        let visual = visual_clip_for(&paint_node);

        assert_eq!(visual.x, 20);
        assert_eq!(visual.y, 20);
        assert_eq!(visual.width, 20);
        assert_eq!(visual.height, 20);
    }

    #[test]
    fn display_list_effect_visual_clip_includes_gradient_bounds() {
        let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
        root.computed_style.background_paint =
            BackgroundPaint::LinearGradient(StyleLinearGradient {
                from: Color::from_hex("#112233").unwrap(),
                to: Color::from_hex("#445566").unwrap(),
            });
        let paint_node = build_paint_node(&root, 0.0, 0.0);
        let visual = visual_clip_for(&paint_node);

        assert_eq!(visual.x, 20);
        assert_eq!(visual.y, 20);
        assert_eq!(visual.width, 20);
        assert_eq!(visual.height, 20);
    }

    #[test]
    fn display_list_selects_box_shadow_outside_layout_bounds() {
        let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
        root.computed_style.box_shadow = BoxShadow {
            offset_x: 10.0,
            offset_y: 0.0,
            blur_radius: 0.0,
            spread_radius: 0.0,
            color: Color::from_hex("#00000080").unwrap(),
            inset: false,
        };

        let mut list = RetainedDisplayList::default();
        list.update(&root, 80, 80, false, true);
        let selected = list.select_paint_commands(
            Some(DamageRect {
                x: 44,
                y: 24,
                width: 2,
                height: 2,
            }),
            DisplayListRepaintPolicy::MinimalDamage,
        );

        assert!(
            selected.iter().any(|command| command.node.id == 1),
            "box-shadow visual bounds should participate in sparse repaint selection"
        );
    }

    #[test]
    fn display_list_orders_commands_by_z_index_before_replay() {
        let mut root = node(1, "stack", 0.0, 0.0, 100.0, 100.0);
        let mut top = node(2, "box", 0.0, 0.0, 40.0, 40.0);
        top.computed_style.z_index = 10;
        let mut bottom = node(3, "box", 0.0, 0.0, 40.0, 40.0);
        bottom.computed_style.z_index = -1;
        root.children.push(top);
        root.children.push(bottom);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, false, false);
        let node_order: Vec<_> = list
            .paint_commands()
            .iter()
            .filter(|command| command.kind == DisplayPaintCommandKind::Node)
            .map(|command| command.node.id)
            .collect();

        assert_eq!(node_order, vec![1, 3, 2]);
    }

    #[test]
    fn display_list_preclip_uses_visual_bounds_for_effect_overflow() {
        let mut root = node(1, "box", 0.0, 0.0, 40.0, 40.0);
        root.computed_style.overflow_x = Overflow::Hidden;
        root.computed_style.overflow_y = Overflow::Hidden;
        let mut child = node(2, "box", 48.0, 0.0, 10.0, 10.0);
        child.computed_style.box_shadow = BoxShadow {
            offset_x: -15.0,
            offset_y: 0.0,
            blur_radius: 0.0,
            spread_radius: 0.0,
            color: Color::from_hex("#00000080").unwrap(),
            inset: false,
        };
        root.children.push(child);

        let mut list = RetainedDisplayList::default();
        let metrics = list.update(&root, 100, 100, false, false);

        assert!(
            list.paint_commands().iter().any(
                |command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node
            ),
            "effect overflow intersecting a parent clip must not be preclipped by layout bounds"
        );
        assert_eq!(metrics.preclipped_descendants, 0);
        assert_eq!(metrics.effect_overflow_count, 1);
    }

    #[test]
    fn display_list_profiles_changed_paint_layout_effect_overflow_and_fallbacks() {
        let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, false, true);

        root.computed_style.box_shadow = BoxShadow {
            offset_x: 10.0,
            offset_y: 0.0,
            blur_radius: 2.0,
            spread_radius: 0.0,
            color: Color::from_hex("#00000080").unwrap(),
            inset: false,
        };
        let metrics = list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                material: 1,
                geometry: 1,
                ..Default::default()
            },
            &HashSet::from([1]),
            100,
            100,
            false,
            true,
        );

        assert_eq!(metrics.changed_paint_count, 1);
        assert_eq!(metrics.changed_layout_count, 1);
        assert_eq!(metrics.effect_overflow_count, 1);

        let fallback_metrics = list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                geometry: 1,
                ..Default::default()
            },
            &HashSet::new(),
            100,
            100,
            false,
            true,
        );
        assert_eq!(fallback_metrics.full_fallback_count, 1);
        assert_eq!(fallback_metrics.fallback_promotion_count, 1);
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
        root.computed_style.overflow_y = Overflow::Scroll;
        let mut list = RetainedDisplayList::default();

        let first = list.update_for_retained_generation(
            &root,
            1,
            RenderObjectDirtySummary {
                inserted: 1,
                ..Default::default()
            },
            &HashSet::from([1]),
            100,
            40,
            false,
            true,
        );
        assert_eq!(first.entries_rebuilt, 1);
        assert_eq!(list.paint_commands().len(), 2);

        let mut child = node(2, "text", 10.0, 0.0, 20.0, 10.0);
        child.computed_style.overflow_y = Overflow::Scroll;
        root.children.push(child);
        let skipped = list.update_for_retained_generation(
            &root,
            1,
            RenderObjectDirtySummary {
                inserted: 1,
                ..Default::default()
            },
            &HashSet::from([1]),
            100,
            40,
            true,
            true,
        );
        assert_eq!(skipped.entries_rebuilt, 0);
        assert_eq!(skipped.entries_reused, 1);
        assert_eq!(skipped.damage_area, 4_000);
        assert!(skipped.full_surface_damage);
        assert_eq!(
            list.paint_commands().len(),
            2,
            "paint command cache should be reused while retained generation is unchanged"
        );

        let rebuilt = list.update_for_retained_generation(
            &root,
            2,
            RenderObjectDirtySummary {
                inserted: 1,
                ..Default::default()
            },
            &HashSet::from([1]),
            100,
            40,
            false,
            true,
        );
        assert_eq!(rebuilt.entries_rebuilt, 2);
        assert_eq!(list.paint_commands().len(), 4);
    }

    #[test]
    fn display_list_reuses_unrelated_subtrees_for_transform_updates() {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
        left.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
        let mut right = node(4, "box", 60.0, 0.0, 40.0, 40.0);
        right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
        root.children.push(left);
        root.children.push(right);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 120, 40, false, true);
        let before = list.paint_commands().to_vec();

        root.children[0].computed_style.transform.translate_x = 12.0;
        let metrics = list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                transform: 1,
                ..Default::default()
            },
            &HashSet::from([2]),
            120,
            40,
            false,
            true,
        );
        let after = list.paint_commands().to_vec();

        let right_before = command_debugs(&before, &[4, 5]);
        let right_after = command_debugs(&after, &[4, 5]);
        assert_eq!(right_before, right_after);
        assert!(metrics.subtree_segments_reused > 0);
        assert!(metrics.subtree_segments_rebuilt > 0);
        assert_eq!(metrics.full_fallback_count, 0);
    }

    #[test]
    fn display_list_reuses_unrelated_subtrees_for_scroll_updates() {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
        left.computed_style.overflow_x = Overflow::Hidden;
        left.attributes.insert("_mesh_scroll_x".into(), "0".into());
        left.children.push(node(3, "text", 30.0, 4.0, 20.0, 12.0));
        let mut right = node(4, "box", 60.0, 0.0, 40.0, 40.0);
        right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
        root.children.push(left);
        root.children.push(right);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 120, 40, false, true);
        let before = list.paint_commands().to_vec();

        root.children[0]
            .attributes
            .insert("_mesh_scroll_x".into(), "18".into());
        let metrics = list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                geometry: 1,
                ..Default::default()
            },
            &HashSet::from([2]),
            120,
            40,
            false,
            true,
        );
        let after = list.paint_commands().to_vec();

        let right_before = command_debugs(&before, &[4, 5]);
        let right_after = command_debugs(&after, &[4, 5]);
        assert_eq!(right_before, right_after);
        assert!(metrics.subtree_segments_reused > 0);
        assert!(metrics.subtree_commands_rebuilt > 0);
    }

    #[test]
    fn display_list_reuses_unrelated_subtrees_for_local_reorder_updates() {
        let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
        let mut left = node(2, "row", 0.0, 0.0, 80.0, 40.0);
        let mut left_first = node(3, "box", 0.0, 0.0, 20.0, 20.0);
        left_first.computed_style.z_index = 0;
        let mut left_second = node(4, "box", 20.0, 0.0, 20.0, 20.0);
        left_second.computed_style.z_index = 1;
        left.children.push(left_first);
        left.children.push(left_second);
        let mut right = node(5, "box", 100.0, 0.0, 40.0, 40.0);
        right.children.push(node(6, "text", 4.0, 4.0, 20.0, 12.0));
        root.children.push(left);
        root.children.push(right);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 160, 40, false, true);
        let before = list.paint_commands().to_vec();

        root.children[0].children.swap(0, 1);
        let metrics = list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                reordered: 1,
                ..Default::default()
            },
            &HashSet::from([2]),
            160,
            40,
            false,
            true,
        );
        let after = list.paint_commands().to_vec();

        let right_before = command_debugs(&before, &[5, 6]);
        let right_after = command_debugs(&after, &[5, 6]);
        assert_eq!(right_before, right_after);
        assert!(metrics.subtree_segments_reused > 0);
        assert_eq!(metrics.full_fallback_count, 0);
    }

    #[test]
    fn display_list_records_span_metadata_and_policy_labels() {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
        left.computed_style.overflow_y = Overflow::Scroll;
        left.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
        let mut right = node(4, "box", 70.0, 0.0, 40.0, 40.0);
        right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
        root.children.push(left);
        root.children.push(right);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 120, 40, false, true);

        let left_spans: Vec<_> = list
            .command_spans
            .iter()
            .filter(|span| span.owner == 2)
            .collect();
        let left_span = left_spans
            .first()
            .expect("left retained subtree should have command span metadata");
        assert!(left_span.start < left_span.end);
        assert!(
            left_spans
                .iter()
                .all(|span| span.end.saturating_sub(span.start) == span.command_count)
        );
        assert!(left_spans.iter().any(|span| span.includes_scrollbars));
        let left_total_commands: usize = left_spans.iter().map(|span| span.command_count).sum();
        assert_eq!(left_total_commands, 2);
        assert_eq!(
            DisplayListRepaintPolicy::MinimalDamage.as_str(),
            "minimal_damage"
        );
        assert_eq!(
            DisplayListRepaintPolicy::BoundingRect.as_str(),
            "bounding_rect"
        );
        assert_eq!(
            DisplayListRepaintPolicy::FullSurface.as_str(),
            "full_surface"
        );
    }

    #[test]
    fn display_list_filters_sparse_damage_without_reordering_commands() {
        let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
        let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
        left.computed_style.overflow_y = Overflow::Scroll;
        root.children.push(left);
        root.children.push(node(3, "box", 80.0, 0.0, 40.0, 40.0));

        let mut list = RetainedDisplayList::default();
        list.update(&root, 160, 40, false, true);
        let full_order: Vec<_> = list
            .paint_commands()
            .iter()
            .map(|command| (command.node.id, command.kind))
            .collect();

        let selected = list.select_paint_commands(
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 45,
                height: 40,
            }),
            DisplayListRepaintPolicy::MinimalDamage,
        );
        let filtered_order: Vec<_> = selected
            .iter()
            .map(|command| (command.node.id, command.kind))
            .collect();

        assert!(selected.metrics().filtered_command_count < full_order.len() as u64);
        assert!(selected.metrics().filtered_commands_skipped > 0);
        assert!(selected.metrics().filtered_span_count > 0);
        assert!(
            filtered_order
                .iter()
                .any(|item| *item == (2, DisplayPaintCommandKind::Scrollbars))
        );
        let projected_full: Vec<_> = full_order
            .into_iter()
            .filter(|item| filtered_order.contains(item))
            .collect();
        assert_eq!(filtered_order, projected_full);
    }

    #[test]
    fn display_list_partial_damage_replays_intersecting_backgrounds() {
        let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
        root.children.push(node(2, "box", 0.0, 0.0, 40.0, 40.0));
        root.children.push(node(3, "box", 80.0, 0.0, 40.0, 40.0));

        let mut list = RetainedDisplayList::default();
        list.update(&root, 160, 40, false, true);
        let selected = list.select_paint_commands(
            Some(DamageRect {
                x: 88,
                y: 8,
                width: 12,
                height: 12,
            }),
            DisplayListRepaintPolicy::MinimalDamage,
        );
        let ids: Vec<_> = selected.iter().map(|command| command.node.id).collect();

        assert!(
            ids.contains(&1),
            "partial repaint must replay root background under damaged child pixels"
        );
        assert!(ids.contains(&3), "damaged child command should be selected");
    }

    #[test]
    fn display_list_full_surface_policy_keeps_all_commands_and_records_fallback() {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        root.children.push(node(2, "box", 0.0, 0.0, 40.0, 40.0));
        root.children.push(node(3, "box", 70.0, 0.0, 40.0, 40.0));

        let mut list = RetainedDisplayList::default();
        list.update(&root, 120, 40, false, true);
        let selected = list.select_paint_commands(
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 120,
                height: 40,
            }),
            DisplayListRepaintPolicy::FullSurface,
        );

        assert_eq!(selected.len(), list.paint_commands().len());
        assert_eq!(
            selected.metrics().repaint_policy,
            DisplayListRepaintPolicy::FullSurface
        );
        assert_eq!(selected.metrics().filtered_commands_skipped, 0);
        assert_eq!(selected.metrics().filtered_fallback_count, 1);
    }

    #[test]
    fn display_list_select_paint_commands_for_rects_matches_expected_commands() {
        let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
        root.children.push(node(2, "box", 0.0, 0.0, 40.0, 40.0));
        root.children.push(node(3, "box", 80.0, 0.0, 40.0, 40.0));

        let mut list = RetainedDisplayList::default();
        list.update(&root, 160, 40, false, true);
        let selected_left = list.select_paint_commands(
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 45,
                height: 40,
            }),
            DisplayListRepaintPolicy::MinimalDamage,
        );
        let selected_right = list.select_paint_commands(
            Some(DamageRect {
                x: 80,
                y: 0,
                width: 40,
                height: 40,
            }),
            DisplayListRepaintPolicy::MinimalDamage,
        );
        let selected_multi = list.select_paint_commands_for_rects(
            &[
                DamageRect {
                    x: 0,
                    y: 0,
                    width: 45,
                    height: 40,
                },
                DamageRect {
                    x: 80,
                    y: 0,
                    width: 40,
                    height: 40,
                },
            ],
            DisplayListRepaintPolicy::MinimalDamage,
        );

        let multi_ids: Vec<_> = selected_multi
            .iter()
            .map(|command| command.node.id)
            .collect();

        assert!(multi_ids.contains(&1));
        assert!(multi_ids.contains(&2));
        assert!(multi_ids.contains(&3));
        assert!(selected_multi.len() >= selected_left.len());
        assert!(selected_multi.len() >= selected_right.len());
    }

    #[test]
    fn display_list_select_paint_commands_for_rects_single_rect_delegates() {
        let mut root = node(1, "row", 0.0, 0.0, 100.0, 40.0);
        root.children.push(node(2, "box", 0.0, 0.0, 40.0, 40.0));

        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 40, false, true);
        let damage = DamageRect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        };
        let selected_single =
            list.select_paint_commands(Some(damage), DisplayListRepaintPolicy::MinimalDamage);
        let selected_multi = list
            .select_paint_commands_for_rects(&[damage], DisplayListRepaintPolicy::MinimalDamage);

        assert_eq!(selected_single.len(), selected_multi.len());
        assert_eq!(
            selected_single.metrics().filtered_command_count,
            selected_multi.metrics().filtered_command_count
        );
        assert_eq!(
            selected_single.metrics().filtered_span_count,
            selected_multi.metrics().filtered_span_count
        );
        assert_eq!(
            selected_single.metrics().filtered_commands_skipped,
            selected_multi.metrics().filtered_commands_skipped
        );
    }

    #[test]
    fn display_list_falls_back_for_ambiguous_dirty_summaries() {
        let root = node(1, "box", 0.0, 0.0, 100.0, 40.0);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 40, false, true);

        let metrics = list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                geometry: 1,
                ..Default::default()
            },
            &HashSet::new(),
            100,
            40,
            false,
            true,
        );

        assert_eq!(metrics.full_fallback_count, 1);
        assert_eq!(metrics.broad_dirty_fallback_count, 0);
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
    fn display_list_keeps_opaque_backgrounds_batchable_and_translucent_backgrounds_conservative() {
        let mut opaque_root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
        opaque_root
            .children
            .push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
        let mut opaque_list = RetainedDisplayList::default();

        let opaque_metrics = opaque_list.update(&opaque_root, 100, 20, false, false);

        assert_eq!(opaque_metrics.barriers.translucency, 0);
        assert_eq!(opaque_metrics.barrier_count, 0);
        assert_eq!(opaque_metrics.batch_count, 1);

        let mut translucent_root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
        let mut translucent = node(2, "box", 0.0, 0.0, 20.0, 20.0);
        translucent.computed_style.background_color.a = 128;
        translucent_root.children.push(translucent);
        let mut translucent_list = RetainedDisplayList::default();

        let translucent_metrics = translucent_list.update(&translucent_root, 100, 20, false, false);

        assert_eq!(translucent_metrics.barriers.translucency, 1);
        assert_eq!(translucent_metrics.barrier_count, 1);
        assert_eq!(translucent_metrics.batch_count, 0);
    }

    #[test]
    fn display_list_uses_cached_icon_opacity_for_conservative_barriers() {
        let td = tempfile::tempdir().unwrap();
        let opaque_path = td.path().join("opaque.png");
        let translucent_path = td.path().join("translucent.png");
        image::ImageBuffer::from_fn(2, 2, |_, _| image::Rgba([255u8, 0, 0, 255]))
            .save(&opaque_path)
            .unwrap();
        image::ImageBuffer::from_fn(2, 2, |x, _| {
            if x == 0 {
                image::Rgba([255u8, 0, 0, 255])
            } else {
                image::Rgba([255u8, 0, 0, 96])
            }
        })
        .save(&translucent_path)
        .unwrap();

        let mut buffer = crate::surface::PixelBuffer::new(16, 16);
        let tint = Color::WHITE;
        crate::surface::icon::draw_icon_from_path(&mut buffer, &opaque_path, 0, 0, 10, 10, tint);
        crate::surface::icon::draw_icon_from_path(
            &mut buffer,
            &translucent_path,
            0,
            0,
            10,
            10,
            tint,
        );
        assert_eq!(
            crate::surface::icon::cached_file_resource_opacity(&opaque_path, 10, 10, tint, false),
            crate::surface::icon::CachedResourceOpacity::Opaque
        );
        assert_eq!(
            crate::surface::icon::cached_file_resource_opacity(
                &translucent_path,
                10,
                10,
                tint,
                false
            ),
            crate::surface::icon::CachedResourceOpacity::Translucent
        );

        let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
        root.computed_style.background_color = Color::TRANSPARENT;
        let mut opaque = node(2, "icon", 0.0, 0.0, 10.0, 10.0);
        opaque.computed_style.background_color = Color::TRANSPARENT;
        opaque.computed_style.color = tint;
        opaque
            .attributes
            .insert("src".into(), opaque_path.to_string_lossy().into_owned());
        let mut translucent = node(3, "icon", 12.0, 0.0, 10.0, 10.0);
        translucent.computed_style.background_color = Color::TRANSPARENT;
        translucent.computed_style.color = tint;
        translucent.attributes.insert(
            "src".into(),
            translucent_path.to_string_lossy().into_owned(),
        );
        let mut unknown = node(4, "icon", 24.0, 0.0, 10.0, 10.0);
        unknown.computed_style.background_color = Color::TRANSPARENT;
        unknown.computed_style.color = tint;
        unknown.attributes.insert(
            "src".into(),
            td.path().join("missing.png").to_string_lossy().into_owned(),
        );
        root.children.push(opaque);
        root.children.push(translucent);
        root.children.push(unknown);

        let mut list = RetainedDisplayList::default();
        let metrics = list.update(&root, 100, 20, false, false);

        assert_eq!(metrics.barriers.icon, 1);
        assert_eq!(metrics.barriers.translucency, 1);
        assert_eq!(metrics.barriers.opacity, 0);

        let mut transparent_root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
        transparent_root.computed_style.background_color = Color::TRANSPARENT;
        let mut transparent_icon = node(2, "icon", 0.0, 0.0, 10.0, 10.0);
        transparent_icon.computed_style.background_color = Color::TRANSPARENT;
        transparent_icon.computed_style.color = tint;
        transparent_icon.computed_style.opacity = 0.5;
        transparent_icon
            .attributes
            .insert("src".into(), opaque_path.to_string_lossy().into_owned());
        transparent_root.children.push(transparent_icon);

        let mut transparent_list = RetainedDisplayList::default();
        let transparent_metrics = transparent_list.update(&transparent_root, 100, 20, false, false);

        assert_eq!(transparent_metrics.barriers.opacity, 1);
        assert_eq!(transparent_metrics.barriers.icon, 0);
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

    #[test]
    fn display_list_omits_explicitly_hidden_descendants() {
        let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
        let mut hidden = node(2, "box", 10.0, 10.0, 20.0, 20.0);
        hidden.computed_style.visibility = Visibility::Hidden;
        hidden.children.push(node(3, "text", 0.0, 0.0, 10.0, 10.0));
        root.children.push(hidden);

        let mut list = RetainedDisplayList::default();
        let metrics = list.update(&root, 100, 100, false, false);

        assert!(
            list.paint_commands()
                .iter()
                .all(|command| command.node.id != 2 && command.node.id != 3)
        );
        assert_eq!(metrics.omitted_subtrees, 1);
        assert_eq!(metrics.omitted_nodes, 2);
        assert_eq!(metrics.omitted_commands, 4);
    }

    #[test]
    fn display_list_keeps_plain_opacity_zero_nodes_paintable() {
        let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
        let mut transparent = node(2, "box", 10.0, 10.0, 20.0, 20.0);
        transparent.computed_style.opacity = 0.0;
        root.children.push(transparent);

        let mut list = RetainedDisplayList::default();
        let metrics = list.update(&root, 100, 100, false, false);

        assert!(
            list.paint_commands().iter().any(
                |command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node
            )
        );
        assert_eq!(metrics.omitted_subtrees, 0);
        assert_eq!(metrics.omitted_nodes, 0);
    }

    #[test]
    fn display_list_preclips_fully_out_of_viewport_descendants() {
        let mut root = node(1, "box", 0.0, 0.0, 40.0, 40.0);
        root.computed_style.overflow_x = Overflow::Hidden;
        root.computed_style.overflow_y = Overflow::Hidden;
        let child = node(2, "box", 60.0, 0.0, 20.0, 20.0);
        root.children.push(child);

        let mut list = RetainedDisplayList::default();
        let metrics = list.update(&root, 100, 100, false, false);

        assert!(
            list.paint_commands()
                .iter()
                .all(|command| command.node.id != 2),
            "fully out-of-viewport descendants should be omitted before paint traversal"
        );
        assert_eq!(metrics.omitted_subtrees, 1);
        assert_eq!(metrics.omitted_nodes, 1);
        assert_eq!(metrics.preclipped_descendants, 1);
    }

    #[test]
    fn display_list_keeps_partially_intersecting_descendants_paintable() {
        let mut root = node(1, "box", 0.0, 0.0, 40.0, 40.0);
        root.computed_style.overflow_x = Overflow::Hidden;
        root.computed_style.overflow_y = Overflow::Hidden;
        let child = node(2, "box", 30.0, 0.0, 20.0, 20.0);
        root.children.push(child);

        let mut list = RetainedDisplayList::default();
        let metrics = list.update(&root, 100, 100, false, false);

        assert!(
            list.paint_commands().iter().any(
                |command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node
            )
        );
        assert_eq!(metrics.omitted_subtrees, 0);
        assert_eq!(metrics.preclipped_descendants, 0);
    }
}
