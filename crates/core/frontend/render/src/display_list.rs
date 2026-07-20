use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

use mesh_core_elements::style::{
    BackgroundPaint, BlendMode, Color, Display, Edges, Overflow, Position, TextAlign,
    TextDirection, TextOverflow, Visibility,
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

const DISPLAY_PRIMITIVE_SLOTS: [DisplayPrimitiveSlot; 5] = [
    DisplayPrimitiveSlot::Background,
    DisplayPrimitiveSlot::Border,
    DisplayPrimitiveSlot::Text,
    DisplayPrimitiveSlot::Icon,
    DisplayPrimitiveSlot::Generic,
];

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
    paint_origin: (u32, u32),
    entries: HashMap<DisplayListKey, DisplayListEntry>,
    subtrees: HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    #[cfg(debug_assertions)]
    ordered_entries_scratch: Vec<(DisplayListKey, DisplayListEntry)>,
    next_entries_scratch: HashMap<DisplayListKey, DisplayListEntry>,
    next_subtrees_scratch: HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    dirty_ancestors_scratch: HashSet<NodeId>,
    ancestor_path_scratch: Vec<NodeId>,
    command_spans: Arc<[RetainedCommandSpan]>,
    paint_commands: Arc<[DisplayPaintCommand]>,
    command_kinds: Arc<[DisplayPaintCommandKind]>,
    backdrop_regions: Vec<DamageRect>,
    last_metrics: DisplayListMetrics,
    last_damage_rects: Vec<DamageRect>,
}

impl Default for RetainedDisplayList {
    fn default() -> Self {
        Self {
            generation: 0,
            retained_tree_generation: None,
            root_id: None,
            surface_size: None,
            paint_origin: (0.0_f32.to_bits(), 0.0_f32.to_bits()),
            entries: HashMap::new(),
            subtrees: HashMap::new(),
            #[cfg(debug_assertions)]
            ordered_entries_scratch: Vec::new(),
            next_entries_scratch: HashMap::new(),
            next_subtrees_scratch: HashMap::new(),
            dirty_ancestors_scratch: HashSet::new(),
            ancestor_path_scratch: Vec::new(),
            command_spans: Vec::new().into(),
            paint_commands: Vec::new().into(),
            command_kinds: Vec::new().into(),
            backdrop_regions: Vec::new(),
            last_metrics: DisplayListMetrics::default(),
            last_damage_rects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DisplayPaintCommand {
    pub node: Arc<DisplayPaintNode>,
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
    pub font_family: Arc<str>,
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
    pub mix_blend_mode: BlendMode,
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
    Checkmark(DisplayCheckmarkPaint),
}

/// The selected-state glyph for a `checkbox`/`radio` element, painted as a
/// vector path. Only emitted when the control is checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayCheckmarkPaint {
    pub kind: CheckmarkKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckmarkKind {
    /// A check (tick) glyph — used by `checkbox`.
    Check,
    /// A filled dot — used by `radio`.
    Dot,
}

#[derive(Debug, Clone)]
pub struct DisplayTextPaint {
    pub text: Arc<str>,
    pub selection: Option<DisplayTextSelectionPaint>,
}

impl PartialEq for DisplayTextPaint {
    fn eq(&self, other: &Self) -> bool {
        shared_str_eq(&self.text, &other.text) && self.selection == other.selection
    }
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

#[derive(Debug, Clone)]
pub struct DisplayInputPaint {
    pub value: Arc<str>,
    pub placeholder: Arc<str>,
    pub mask_text: bool,
    pub focused: bool,
}

impl PartialEq for DisplayInputPaint {
    fn eq(&self, other: &Self) -> bool {
        shared_str_eq(&self.value, &other.value)
            && shared_str_eq(&self.placeholder, &other.placeholder)
            && self.mask_text == other.mask_text
            && self.focused == other.focused
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplaySliderPaint {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub vertical: bool,
}

#[derive(Debug, Clone)]
pub struct DisplayIconPaint {
    pub src: Option<Arc<str>>,
    pub name: Option<Arc<str>>,
    pub size: Option<u32>,
}

impl PartialEq for DisplayIconPaint {
    fn eq(&self, other: &Self) -> bool {
        optional_shared_str_eq(&self.src, &other.src)
            && optional_shared_str_eq(&self.name, &other.name)
            && self.size == other.size
    }
}

fn shared_str_eq(left: &Arc<str>, right: &Arc<str>) -> bool {
    Arc::ptr_eq(left, right) || left.as_ref() == right.as_ref()
}

fn optional_shared_str_eq(left: &Option<Arc<str>>, right: &Option<Arc<str>>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => shared_str_eq(left, right),
        (None, None) => true,
        _ => false,
    }
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
    generation: u64,
    commands: Arc<[DisplayPaintCommand]>,
    kinds: Arc<[DisplayPaintCommandKind]>,
    effect_overflow_count: u64,
    pruning: PruningMetrics,
    command_span: Option<RetainedSubtreeSpan>,
    child_order: Option<Arc<[usize]>>,
}

impl Default for RetainedPaintSubtree {
    fn default() -> Self {
        Self {
            generation: 0,
            commands: Vec::new().into(),
            kinds: Vec::new().into(),
            effect_overflow_count: 0,
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
    effect_overflow_count: u64,
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
        if command_has_effect_overflow(&command) {
            self.effect_overflow_count = self.effect_overflow_count.saturating_add(1);
        }
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
        self.effect_overflow_count = self
            .effect_overflow_count
            .saturating_add(child_subtree.effect_overflow_count);
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

    fn into_retained(self, generation: u64) -> RetainedPaintSubtree {
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
            generation,
            commands: self.commands.into(),
            kinds: self.kinds.into(),
            effect_overflow_count: self.effect_overflow_count,
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
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Generation of one retained paint subtree.
    ///
    /// Unlike the display-list generation, this stays stable when paint work
    /// elsewhere in the surface changes. Promoted child surfaces use it to
    /// avoid repainting an unchanged popup for unrelated parent updates.
    pub fn subtree_generation(&self, node_id: NodeId) -> Option<u64> {
        self.subtrees
            .get(&node_id)
            .map(|subtree| subtree.generation)
    }

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
            0.0,
            0.0,
            surface_width,
            surface_height,
            force_full_damage,
            partial_present_supported,
        )
    }

    /// Build a display list for `root` translated into a target-local viewport.
    ///
    /// Promoted child surfaces use this to retain their own command stream even
    /// when the authored subtree lies outside (or is clipped by) its parent
    /// surface. The origin participates in the cache key, so moving the subtree
    /// cannot accidentally replay commands produced for an older position.
    pub fn update_at(
        &mut self,
        root: &WidgetNode,
        offset_x: f32,
        offset_y: f32,
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
            offset_x,
            offset_y,
            surface_width,
            surface_height,
            force_full_damage,
            partial_present_supported,
        )
    }

    /// Generation-gated variant of [`Self::update_at`] for independently
    /// retained surface targets. An unchanged generation, viewport, and origin
    /// bypasses both entry collection and subtree command reconstruction.
    pub fn update_at_for_retained_generation(
        &mut self,
        root: &WidgetNode,
        retained_tree_generation: u64,
        offset_x: f32,
        offset_y: f32,
        surface_width: u32,
        surface_height: u32,
        force_full_damage: bool,
        partial_present_supported: bool,
    ) -> DisplayListMetrics {
        self.update_inner(
            root,
            Some(retained_tree_generation),
            None,
            None,
            offset_x,
            offset_y,
            surface_width,
            surface_height,
            force_full_damage,
            partial_present_supported,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_at_for_retained_generation_with_dirty_nodes(
        &mut self,
        root: &WidgetNode,
        retained_tree_generation: u64,
        dirty_summary: RenderObjectDirtySummary,
        dirty_node_ids: &HashSet<NodeId>,
        offset_x: f32,
        offset_y: f32,
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
            offset_x,
            offset_y,
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
            0.0,
            0.0,
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
            0.0,
            0.0,
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
        offset_x: f32,
        offset_y: f32,
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
        let paint_origin = (offset_x.to_bits(), offset_y.to_bits());
        if retained_tree_generation.is_some()
            && self.retained_tree_generation == retained_tree_generation
            && self.surface_size == Some((surface.width, surface.height))
            && self.paint_origin == paint_origin
        {
            return self.update_metrics_without_rebuild(
                surface,
                force_full_damage,
                partial_present_supported,
            );
        }

        let dirty_summary = dirty_summary.unwrap_or_default();
        let empty_dirty_nodes = HashSet::new();
        let dirty_node_ids = dirty_node_ids.unwrap_or(&empty_dirty_nodes);
        let patch_sparse_entries = (!cfg!(debug_assertions) || cfg!(test))
            && self.can_patch_sparse_entries(
                root,
                dirty_summary,
                dirty_node_ids,
                surface.width,
                surface.height,
            );

        #[cfg(debug_assertions)]
        let mut ordered_entries = std::mem::take(&mut self.ordered_entries_scratch);
        #[cfg(debug_assertions)]
        ordered_entries.clear();
        let mut next = std::mem::take(&mut self.next_entries_scratch);
        next.clear();
        collect_display_entries(
            root,
            offset_x,
            offset_y,
            #[cfg(debug_assertions)]
            Some(&mut ordered_entries),
            #[cfg(not(debug_assertions))]
            None,
            patch_sparse_entries.then_some(dirty_node_ids),
            &mut next,
        );
        if self.root_id == Some(root.id)
            && self.surface_size == Some((surface.width, surface.height))
            && self.paint_origin == paint_origin
            && self.entries == next
            && !dirty_summary.any()
        {
            next.clear();
            self.next_entries_scratch = next;
            #[cfg(debug_assertions)]
            {
                ordered_entries.clear();
                self.ordered_entries_scratch = ordered_entries;
            }
            return self.update_metrics_without_rebuild(
                surface,
                force_full_damage,
                partial_present_supported,
            );
        }
        let origin_changed = self.paint_origin != paint_origin;
        let decision = if origin_changed {
            LocalReuseDecision::FallbackFull { broad_dirty: false }
        } else {
            self.local_reuse_decision(
                root,
                dirty_summary,
                dirty_node_ids,
                surface.width,
                surface.height,
            )
        };
        let (
            paint_commands,
            command_kinds,
            command_spans,
            effect_overflow_count,
            pruning,
            subtrees,
            local_metrics,
        ) = match decision {
            LocalReuseDecision::RebuildDirtySubtrees => {
                let mut rebuild_ancestors = std::mem::take(&mut self.dirty_ancestors_scratch);
                rebuild_ancestors.clear();
                let mut ancestor_path = std::mem::take(&mut self.ancestor_path_scratch);
                ancestor_path.clear();
                collect_dirty_ancestor_ids_into(
                    root,
                    dirty_node_ids,
                    &mut ancestor_path,
                    &mut rebuild_ancestors,
                );
                let mut next_subtrees = std::mem::take(&mut self.next_subtrees_scratch);
                next_subtrees.clear();
                let mut local_metrics = LocalReuseMetrics::default();
                let vclip = surface_clip(surface);
                let allow_clean_descendant_reuse = changed_layout_count(dirty_summary) == 0;
                let subtree = build_paint_subtree(
                    root,
                    offset_x,
                    offset_y,
                    vclip,
                    vclip,
                    false,
                    allow_clean_descendant_reuse,
                    dirty_node_ids,
                    &rebuild_ancestors,
                    &self.subtrees,
                    &mut next_subtrees,
                    &mut local_metrics,
                );
                self.dirty_ancestors_scratch = rebuild_ancestors;
                self.ancestor_path_scratch = ancestor_path;
                let command_spans = build_command_spans(root, &next_subtrees).into();
                (
                    Arc::clone(&subtree.commands),
                    Arc::clone(&subtree.kinds),
                    command_spans,
                    subtree.effect_overflow_count,
                    subtree.pruning,
                    next_subtrees,
                    local_metrics,
                )
            }
            LocalReuseDecision::FallbackFull { .. } => {
                let mut next_subtrees = std::mem::take(&mut self.next_subtrees_scratch);
                next_subtrees.clear();
                let mut local_metrics = LocalReuseMetrics::default();
                let vclip = surface_clip(surface);
                let subtree = build_paint_subtree(
                    root,
                    offset_x,
                    offset_y,
                    vclip,
                    vclip,
                    true,
                    false,
                    dirty_node_ids,
                    &HashSet::new(),
                    &HashMap::new(),
                    &mut next_subtrees,
                    &mut local_metrics,
                );
                let command_spans = build_command_spans(root, &next_subtrees).into();
                (
                    Arc::clone(&subtree.commands),
                    Arc::clone(&subtree.kinds),
                    command_spans,
                    subtree.effect_overflow_count,
                    subtree.pruning,
                    next_subtrees,
                    local_metrics,
                )
            }
        };

        let backdrop_regions = compute_backdrop_regions(paint_commands.as_ref(), surface);

        let mut damage: Option<DamageRect> = None;
        let mut damage_rects = std::mem::take(&mut self.last_damage_rects);
        damage_rects.clear();
        let (reused, rebuilt, removed) = if patch_sparse_entries {
            let mut rebuilt = 0u64;
            let mut removed = 0u64;
            for node_id in dirty_node_ids {
                for slot in DISPLAY_PRIMITIVE_SLOTS {
                    let key = DisplayListKey {
                        node_id: *node_id,
                        slot,
                    };
                    let previous = self.entries.get(&key).copied();
                    let next_entry = next.remove(&key);
                    match (previous, next_entry) {
                        (Some(previous), Some(next_entry)) if previous == next_entry => {}
                        (Some(previous), Some(next_entry)) => {
                            rebuilt = rebuilt.saturating_add(1);
                            damage = union_damage(damage, previous.bounds);
                            damage = union_damage(damage, next_entry.bounds);
                            push_sparse_damage_rect(&mut damage_rects, previous.bounds, surface);
                            push_sparse_damage_rect(&mut damage_rects, next_entry.bounds, surface);
                            self.entries.insert(key, next_entry);
                        }
                        (None, Some(next_entry)) => {
                            rebuilt = rebuilt.saturating_add(1);
                            damage = union_damage(damage, next_entry.bounds);
                            push_sparse_damage_rect(&mut damage_rects, next_entry.bounds, surface);
                            self.entries.insert(key, next_entry);
                        }
                        (Some(previous), None) => {
                            removed = removed.saturating_add(1);
                            damage = union_damage(damage, previous.bounds);
                            push_sparse_damage_rect(&mut damage_rects, previous.bounds, surface);
                            self.entries.remove(&key);
                        }
                        (None, None) => {}
                    }
                }
            }
            debug_assert!(
                next.is_empty(),
                "sparse display-entry collection emitted an unknown primitive slot"
            );
            let reused = (self.entries.len() as u64).saturating_sub(rebuilt);
            (reused, rebuilt, removed)
        } else {
            let mut reused = 0u64;
            let mut rebuilt = 0u64;
            let mut inserted = 0u64;
            for (key, next_entry) in &next {
                match self.entries.get(key) {
                    Some(previous) if previous == next_entry => reused = reused.saturating_add(1),
                    Some(previous) => {
                        rebuilt = rebuilt.saturating_add(1);
                        damage = union_damage(damage, previous.bounds);
                        damage = union_damage(damage, next_entry.bounds);
                        push_sparse_damage_rect(&mut damage_rects, previous.bounds, surface);
                        push_sparse_damage_rect(&mut damage_rects, next_entry.bounds, surface);
                    }
                    None => {
                        inserted = inserted.saturating_add(1);
                        rebuilt = rebuilt.saturating_add(1);
                        damage = union_damage(damage, next_entry.bounds);
                        push_sparse_damage_rect(&mut damage_rects, next_entry.bounds, surface);
                    }
                }
            }

            let mut removed = 0u64;
            if inserted > 0 || next.len() != self.entries.len() {
                for (key, previous) in &self.entries {
                    if !next.contains_key(key) {
                        removed = removed.saturating_add(1);
                        damage = union_damage(damage, previous.bounds);
                        push_sparse_damage_rect(&mut damage_rects, previous.bounds, surface);
                    }
                }
            }
            (reused, rebuilt, removed)
        };

        let full_surface_damage = force_full_damage || damage.is_none() && self.entries.is_empty();
        let damage_rect = if full_surface_damage {
            surface
        } else {
            damage.unwrap_or_default()
        };
        let damage_rect = clip_rect(damage_rect, surface).unwrap_or_default();
        if full_surface_damage {
            damage_rects.clear();
            damage_rects.push(surface);
        }
        let damage_area = damage_rect.area();
        let surface_area = surface.area();
        let skipped_paint_pixels = if partial_present_supported {
            surface_area.saturating_sub(damage_area)
        } else {
            0
        };
        #[cfg(debug_assertions)]
        let batch_metrics = compute_batch_metrics(&ordered_entries);
        #[cfg(not(debug_assertions))]
        let batch_metrics = DisplayListMetrics::default();

        if rebuilt > 0 || removed > 0 || force_full_damage {
            self.generation = self.generation.saturating_add(1);
        }
        if patch_sparse_entries {
            next.clear();
            self.next_entries_scratch = next;
        } else {
            let mut previous_entries = std::mem::replace(&mut self.entries, next);
            previous_entries.clear();
            self.next_entries_scratch = previous_entries;
        }
        let mut previous_subtrees = std::mem::replace(&mut self.subtrees, subtrees);
        previous_subtrees.clear();
        self.next_subtrees_scratch = previous_subtrees;
        #[cfg(debug_assertions)]
        {
            self.ordered_entries_scratch = ordered_entries;
        }
        self.command_spans = command_spans;
        self.paint_commands = paint_commands;
        self.command_kinds = command_kinds;
        self.backdrop_regions = backdrop_regions;
        self.root_id = Some(root.id);
        self.retained_tree_generation = retained_tree_generation;
        self.surface_size = Some((surface.width, surface.height));
        self.paint_origin = paint_origin;
        let (full_fallback_count, broad_dirty_fallback_count) = match decision {
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
            // DisplayListMetrics tracks the display-list-level merged damage rect (0 or 1).
            // RetainedPaintSnapshot overrides this with effective_damage.damage_rect_count()
            // which returns the actual per-frame Vec<DamageRect> count (DMGE-03).
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
        self.last_damage_rects = damage_rects;
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
        self.last_damage_rects.clear();
        if force_full_damage {
            self.last_damage_rects.push(surface);
        }
        let damage_area = damage_rect.area();
        let surface_area = surface.area();
        let skipped_paint_pixels = if partial_present_supported {
            surface_area.saturating_sub(damage_area)
        } else {
            0
        };
        let effect_overflow_count = self.last_metrics.effect_overflow_count;
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
            // DisplayListMetrics tracks the display-list-level merged damage rect (0 or 1).
            // RetainedPaintSnapshot overrides this with effective_damage.damage_rect_count()
            // which returns the actual per-frame Vec<DamageRect> count (DMGE-03).
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

    pub fn damage_rects(&self) -> &[DamageRect] {
        &self.last_damage_rects
    }

    /// Regions where an in-surface `backdrop-filter` node has painted content
    /// beneath it in paint order (node rect inflated by the blur kernel reach).
    pub fn backdrop_filter_regions(&self) -> &[DamageRect] {
        &self.backdrop_regions
    }

    /// Expands every damage rect that intersects an active backdrop-filter
    /// region to cover that whole region, so the blur re-reads freshly painted
    /// backdrop pixels instead of mixing pixels from different frames. Runs to
    /// a fixpoint so chained/overlapping blur regions cascade. Returns whether
    /// any rect grew.
    pub fn expand_damage_for_backdrop_filters(&self, rects: &mut [DamageRect]) -> bool {
        if self.backdrop_regions.is_empty() || rects.is_empty() {
            return false;
        }
        let mut expanded = false;
        loop {
            let mut changed = false;
            for region in &self.backdrop_regions {
                for rect in rects.iter_mut() {
                    if !rect.intersects(*region) {
                        continue;
                    }
                    let union = rect.union(*region);
                    if union != *rect {
                        *rect = union;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
            expanded = true;
        }
        expanded
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
        for span in self.command_spans.iter() {
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
        for span in self.command_spans.iter() {
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

    fn can_patch_sparse_entries(
        &self,
        root: &WidgetNode,
        dirty_summary: RenderObjectDirtySummary,
        dirty_node_ids: &HashSet<NodeId>,
        surface_width: u32,
        surface_height: u32,
    ) -> bool {
        self.root_id == Some(root.id)
            && self.surface_size == Some((surface_width, surface_height))
            && !self.entries.is_empty()
            && !dirty_node_ids.is_empty()
            && dirty_node_ids.len() <= (self.subtrees.len() / 4).max(8)
            && dirty_summary.any()
            && dirty_summary.inserted == 0
            && dirty_summary.removed == 0
            && dirty_summary.reordered == 0
            && dirty_summary.transform == 0
            && dirty_summary.clip == 0
            && dirty_summary.opacity == 0
            && dirty_summary.geometry == 0
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

    fn write_u8(&mut self, value: u8) {
        self.write_mix(u64::from(value));
    }
    fn write_u16(&mut self, value: u16) {
        self.write_mix(u64::from(value));
    }
    fn write_u32(&mut self, value: u32) {
        self.write_mix(u64::from(value));
    }
    fn write_u64(&mut self, value: u64) {
        self.write_mix(value);
    }
    fn write_u128(&mut self, value: u128) {
        self.write_mix(value as u64);
        self.write_mix((value >> 64) as u64);
    }
    fn write_usize(&mut self, value: usize) {
        self.write_mix(value as u64);
    }
    fn write_i8(&mut self, value: i8) {
        self.write_mix(value as u8 as u64);
    }
    fn write_i16(&mut self, value: i16) {
        self.write_mix(value as u16 as u64);
    }
    fn write_i32(&mut self, value: i32) {
        self.write_mix(value as u32 as u64);
    }
    fn write_i64(&mut self, value: i64) {
        self.write_mix(value as u64);
    }
    fn write_i128(&mut self, value: i128) {
        self.write_u128(value as u128);
    }
    fn write_isize(&mut self, value: isize) {
        self.write_mix(value as usize as u64);
    }
}

impl DisplaySignatureHasher {
    #[inline]
    fn write_mix(&mut self, value: u64) {
        self.0 ^= value;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        self.0 ^= self.0 >> 32;
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
    mut ordered_entries: Option<&mut Vec<(DisplayListKey, DisplayListEntry)>>,
    selected_node_ids: Option<&HashSet<NodeId>>,
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
        let selected = selected_node_ids.is_none_or(|node_ids| node_ids.contains(&node.id));
        for_each_primitive_slot(node, |slot| {
            // Debug batch metrics still need the full ordered stream. Release
            // builds can avoid constructing signatures for unselected nodes.
            if !selected && ordered_entries.is_none() {
                return;
            }
            let key = DisplayListKey {
                node_id: node.id,
                slot,
            };
            let barrier = batch_barrier(node, slot);
            let entry = DisplayListEntry {
                bounds,
                signature: primitive_signature(node, slot),
                batch_signature: barrier.map_or_else(|| batch_signature(node, slot), |_| 0),
                barrier,
            };
            if let Some(entries) = ordered_entries.as_deref_mut() {
                entries.push((key, entry));
            }
            if selected {
                next.insert(key, entry);
            }
        });
    }

    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;

    for child in &node.children {
        collect_display_entries(
            child,
            child_offset_x,
            child_offset_y,
            ordered_entries.as_deref_mut(),
            selected_node_ids,
            next,
        );
    }
}

#[cfg(test)]
fn collect_dirty_ancestor_ids(
    root: &WidgetNode,
    dirty_node_ids: &HashSet<NodeId>,
) -> HashSet<NodeId> {
    let mut ancestors = HashSet::new();
    let mut path = Vec::new();
    collect_dirty_ancestor_ids_into(root, dirty_node_ids, &mut path, &mut ancestors);
    ancestors
}

fn collect_dirty_ancestor_ids_into(
    root: &WidgetNode,
    dirty_node_ids: &HashSet<NodeId>,
    path: &mut Vec<NodeId>,
    ancestors: &mut HashSet<NodeId>,
) {
    if dirty_node_ids.is_empty() {
        return;
    }
    collect_dirty_ancestor_ids_inner(root, dirty_node_ids, dirty_node_ids.len(), path, ancestors);
}

fn collect_dirty_ancestor_ids_inner(
    node: &WidgetNode,
    dirty_node_ids: &HashSet<NodeId>,
    remaining_dirty: usize,
    path: &mut Vec<NodeId>,
    ancestors: &mut HashSet<NodeId>,
) -> usize {
    if remaining_dirty == 0 {
        return 0;
    }
    let mut remaining_dirty = remaining_dirty;
    let is_dirty = dirty_node_ids.contains(&node.id);
    if is_dirty {
        for ancestor in path.iter().copied() {
            ancestors.insert(ancestor);
        }
        remaining_dirty -= 1;
        if remaining_dirty == 0 {
            return 0;
        }
    }
    path.push(node.id);
    for child in &node.children {
        remaining_dirty = collect_dirty_ancestor_ids_inner(
            child,
            dirty_node_ids,
            remaining_dirty,
            path,
            ancestors,
        );
        if remaining_dirty == 0 {
            break;
        }
    }
    path.pop();
    remaining_dirty
}

#[allow(clippy::too_many_arguments)]
fn build_paint_subtree(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    clip: DisplayListClip,
    viewport_clip: DisplayListClip,
    force_rebuild: bool,
    allow_clean_descendant_reuse: bool,
    dirty_node_ids: &HashSet<NodeId>,
    dirty_ancestors: &HashSet<NodeId>,
    previous_subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    next_subtrees: &mut HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    metrics: &mut LocalReuseMetrics,
) -> Arc<RetainedPaintSubtree> {
    let node_is_dirty = dirty_node_ids.contains(&node.id);
    let node_is_ancestor = dirty_ancestors.contains(&node.id);
    if !force_rebuild
        && !node_is_dirty
        && !node_is_ancestor
        && let Some(previous) = previous_subtrees.get(&node.id)
    {
        metrics.reused_segments = metrics.reused_segments.saturating_add(1);
        let reused = Arc::clone(previous);
        next_subtrees.insert(node.id, Arc::clone(&reused));
        return reused;
    }

    metrics.rebuilt_segments = metrics.rebuilt_segments.saturating_add(1);
    let generation = previous_subtrees
        .get(&node.id)
        .map_or(1, |previous| previous.generation.saturating_add(1));

    if node_is_explicitly_hidden(node) {
        let mut subtree = RetainedPaintSubtree::default();
        subtree.generation = generation;
        subtree
            .pruning
            .record_omitted_subtree(count_pruned_subtree(node, offset_x, offset_y, true), false);
        let subtree = Arc::new(subtree);
        next_subtrees.insert(node.id, Arc::clone(&subtree));
        return subtree;
    }

    let style = &node.computed_style;
    let transform = style.transform;
    let offset_x = offset_x + transform.translate_x;
    let offset_y = offset_y + transform.translate_y;
    let previous_paint_node = previous_subtrees
        .get(&node.id)
        .and_then(|subtree| subtree.commands.first())
        .filter(|command| command.node.id == node.id)
        .map(|command| command.node.as_ref());
    let paint_node = Arc::new(build_paint_node_with_previous(
        node,
        offset_x,
        offset_y,
        previous_paint_node,
    ));
    let bounds = node_clip_for(&paint_node);
    let visual_bounds = visual_clip_for(&paint_node);
    let node_clip = intersect_display_clip(clip, visual_bounds);
    if node_clip.width <= 0 || node_clip.height <= 0 {
        let mut subtree = RetainedPaintSubtree::default();
        subtree.generation = generation;
        subtree
            .pruning
            .record_omitted_subtree(count_pruned_subtree(node, offset_x, offset_y, false), true);
        let subtree = Arc::new(subtree);
        next_subtrees.insert(node.id, Arc::clone(&subtree));
        return subtree;
    }

    let mut subtree = PaintSubtreeBuilder::default();
    subtree.push_command(DisplayPaintCommand {
        node: Arc::clone(&paint_node),
        clip: node_clip,
        kind: DisplayPaintCommandKind::Node,
    });
    metrics.rebuilt_commands = metrics.rebuilt_commands.saturating_add(1);

    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
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
        let (cox, coy, cc) = if child.computed_style.position == Position::Fixed {
            (0.0, 0.0, viewport_clip)
        } else {
            (child_offset_x, child_offset_y, child_clip)
        };
        append_child_paint_subtree(
            &mut subtree,
            child,
            cox,
            coy,
            cc,
            viewport_clip,
            force_rebuild || (node_is_dirty && !allow_clean_descendant_reuse),
            allow_clean_descendant_reuse,
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
    let subtree = Arc::new(subtree.into_retained(generation));
    next_subtrees.insert(node.id, Arc::clone(&subtree));
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

fn push_sparse_damage_rect(rects: &mut Vec<DamageRect>, rect: DamageRect, surface: DamageRect) {
    const MAX_RETAINED_DAMAGE_RECTS: usize = 16;

    let Some(mut merged) = clip_rect(rect, surface) else {
        return;
    };
    let mut index = 0;
    while index < rects.len() {
        if rects[index].intersects(merged) {
            merged = rects.swap_remove(index).union(merged);
            index = 0;
        } else {
            index += 1;
        }
    }
    rects.push(merged);
    if rects.len() > MAX_RETAINED_DAMAGE_RECTS {
        let union = union_damage_rects(rects).expect("non-empty retained damage list");
        rects.clear();
        rects.push(union);
    }
}

#[allow(clippy::too_many_arguments)]
fn append_child_paint_subtree(
    subtree: &mut PaintSubtreeBuilder,
    child: &WidgetNode,
    child_offset_x: f32,
    child_offset_y: f32,
    child_clip: DisplayListClip,
    viewport_clip: DisplayListClip,
    force_rebuild: bool,
    allow_clean_descendant_reuse: bool,
    dirty_node_ids: &HashSet<NodeId>,
    dirty_ancestors: &HashSet<NodeId>,
    previous_subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    next_subtrees: &mut HashMap<NodeId, Arc<RetainedPaintSubtree>>,
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
        viewport_clip,
        force_rebuild,
        allow_clean_descendant_reuse,
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
    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
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
    let scroll = node.resolved_scroll_metrics();
    let scroll_x = scroll.x;
    let scroll_y = scroll.y;
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

/// Logical-coordinate regions for display commands whose node has an active
/// `backdrop-filter: blur(...)`. Keeping disjoint nodes separate avoids
/// asking the compositor to blur transparent gaps between popup items.
/// Negative origins are clamped to 0 with the clipped leading edge subtracted
/// from the dimensions, so partially off-screen nodes don't snap to (0,0).
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

/// Approximate the node's actual rounded painted shape with horizontal
/// rectangles accepted by `wl_region`. A fully rounded 36×36 option therefore
/// produces a circular mask instead of a 36×36 square. Rectangular surfaces
/// remain a single protocol rectangle.
fn rounded_blur_regions(command: &DisplayPaintCommand) -> Vec<DamageRect> {
    let layout = command.node.layout;
    let left = layout.x.max(0.0).ceil() as u32;
    let top = layout.y.max(0.0).ceil() as u32;
    let right = (layout.x + layout.width).max(0.0).floor() as u32;
    let bottom = (layout.y + layout.height).max(0.0).floor() as u32;
    if right <= left || bottom <= top {
        return Vec::new();
    }
    let bounds = DamageRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    };
    let clip = DamageRect {
        x: command.clip.x.max(0) as u32,
        y: command.clip.y.max(0) as u32,
        width: command.clip.width.max(0) as u32,
        height: command.clip.height.max(0) as u32,
    };
    let radius = command
        .node
        .style
        .border_radius
        .max(0.0)
        .min(bounds.width.min(bounds.height) as f32 * 0.5);
    if radius < 0.5 {
        return intersect_damage_rect(bounds, clip).into_iter().collect();
    }

    let mut bands: Vec<DamageRect> = Vec::new();
    for row in 0..bounds.height {
        let center_y = row as f32 + 0.5;
        let edge_y = center_y.min(bounds.height as f32 - center_y);
        let inset = if edge_y >= radius {
            0
        } else {
            let circle_y = radius - edge_y;
            (radius - (radius * radius - circle_y * circle_y).max(0.0).sqrt()).ceil() as u32
        };
        if inset.saturating_mul(2) >= bounds.width {
            continue;
        }
        let row_rect = DamageRect {
            x: bounds.x + inset,
            y: bounds.y + row,
            width: bounds.width - inset * 2,
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

fn intersect_damage_rect(left: DamageRect, right: DamageRect) -> Option<DamageRect> {
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

/// Screen-space region an in-surface `backdrop-filter` node reads and
/// rewrites: its layout rect inflated by the blur kernel reach (3× radius,
/// matching the painter's `apply_backdrop_filter_impl` pad), clipped to the
/// surface.
fn backdrop_read_region(node: &DisplayPaintNode, surface: DamageRect) -> Option<DamageRect> {
    let radius = node.style.backdrop_filter.blur_radius;
    if radius <= 0.0 {
        return None;
    }
    let pad = radius * 3.0;
    let left = node.layout.x - pad;
    let top = node.layout.y - pad;
    let right = node.layout.x + node.layout.width + pad;
    let bottom = node.layout.y + node.layout.height + pad;
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

/// Whether replaying this command writes any pixels; used to decide if a
/// backdrop-filter node actually has in-surface content beneath it.
/// Conservative: over-reporting only costs an identity blur pass.
fn display_command_paints_pixels(command: &DisplayPaintCommand) -> bool {
    if command.kind == DisplayPaintCommandKind::Scrollbars {
        return true;
    }
    let style = &command.node.style;
    !matches!(command.node.content, DisplayPaintContent::None)
        || style.background_color.a > 0
        || !matches!(style.background_paint, BackgroundPaint::None)
        || (style.border_width.top > 0.0 && style.border_color.a > 0)
        || (!style.box_shadow.is_none() && !style.box_shadow.inset)
        || style.backdrop_filter.blur_radius > 0.0
}

/// Collects the read regions of backdrop-filter nodes that have painted
/// content beneath them in paint order. A backdrop node with nothing beneath
/// (e.g. a frosted surface root whose in-surface backdrop is empty — the
/// compositor blurs what's behind the surface) contributes no region, so it
/// never widens sparse damage.
fn compute_backdrop_regions(
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

fn command_has_effect_overflow(command: &DisplayPaintCommand) -> bool {
    command.kind == DisplayPaintCommandKind::Node
        && visual_clip_for(&command.node) != node_clip_for(&command.node)
}

#[cfg(test)]
fn count_effect_overflow_commands(commands: &[DisplayPaintCommand]) -> u64 {
    commands
        .iter()
        .filter(|command| command_has_effect_overflow(command))
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
    subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
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
    subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
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
    for_children_in_order(node, subtree.child_order.as_deref(), |child| {
        next_child_start = collect_command_spans(child, subtrees, next_child_start, spans);
    });
    subtree_end
}

#[cfg(test)]
fn build_command_spans_with_ancestor_copying(
    root: &WidgetNode,
    subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
) -> Vec<RetainedCommandSpan> {
    fn collect(
        node: &WidgetNode,
        subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
        command_start: usize,
    ) -> (Vec<RetainedCommandSpan>, usize) {
        let Some(subtree) = subtrees.get(&node.id) else {
            return (Vec::new(), command_start);
        };
        let subtree_end = command_start.saturating_add(subtree.commands.len());
        let mut spans = Vec::new();

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
                if span.includes_scrollbars {
                    let scrollbar_index = subtree_end.saturating_sub(1);
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
            return (spans, subtree_end);
        }

        let mut next_child_start = command_start.saturating_add(1);
        for_children_in_order(node, subtree.child_order.as_deref(), |child| {
            let (child_spans, child_end) = collect(child, subtrees, next_child_start);
            spans.extend(child_spans);
            next_child_start = child_end;
        });
        (spans, subtree_end)
    }

    if subtrees.is_empty() || !subtrees.contains_key(&root.id) {
        return Vec::new();
    }
    collect(root, subtrees, 0).0
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
    build_paint_node_with_previous(node, offset_x, offset_y, None)
}

fn build_paint_node_with_previous(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    previous: Option<&DisplayPaintNode>,
) -> DisplayPaintNode {
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
            mix_blend_mode: node.computed_style.mix_blend_mode,
            icon_fill: node.computed_style.icon_fill,
            icon_weight: node.computed_style.icon_weight,
            icon_grade: node.computed_style.icon_grade,
            icon_optical_size: node.computed_style.icon_optical_size,
        },
        content: build_paint_content_with_previous(node, previous.map(|node| &node.content)),
        scrollbars: {
            let scroll = node.resolved_scroll_metrics();
            DisplayScrollbars {
                max_x: scroll.max_x,
                max_y: scroll.max_y,
                scroll_x: scroll.x,
                scroll_y: scroll.y,
                content_width: scroll.content_width,
                content_height: scroll.content_height,
            }
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

#[cfg(test)]
fn build_paint_content(node: &WidgetNode) -> DisplayPaintContent {
    build_paint_content_with_previous(node, None)
}

fn build_paint_content_with_previous(
    node: &WidgetNode,
    previous: Option<&DisplayPaintContent>,
) -> DisplayPaintContent {
    match node.tag.as_str() {
        "text" => DisplayPaintContent::Text(DisplayTextPaint {
            text: retained_display_str(
                node.attributes
                    .get("text")
                    .or_else(|| node.attributes.get("content"))
                    .map_or("", String::as_str),
                match previous {
                    Some(DisplayPaintContent::Text(text)) => Some(&text.text),
                    _ => None,
                },
            ),
            selection: build_text_selection(node),
        }),
        "input" => DisplayPaintContent::Input(DisplayInputPaint {
            value: retained_display_str(
                node.attributes.get("value").map_or("", String::as_str),
                match previous {
                    Some(DisplayPaintContent::Input(input)) => Some(&input.value),
                    _ => None,
                },
            ),
            placeholder: retained_display_str(
                node.attributes
                    .get("placeholder")
                    .map_or("", String::as_str),
                match previous {
                    Some(DisplayPaintContent::Input(input)) => Some(&input.placeholder),
                    _ => None,
                },
            ),
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
            src: retained_optional_display_str(
                node.attributes.get("src").map(String::as_str),
                match previous {
                    Some(DisplayPaintContent::Icon(icon)) => icon.src.as_ref(),
                    _ => None,
                },
            ),
            name: retained_optional_display_str(
                node.attributes.get("name").map(String::as_str),
                match previous {
                    Some(DisplayPaintContent::Icon(icon)) => icon.name.as_ref(),
                    _ => None,
                },
            ),
            size: node
                .attributes
                .get("size")
                .and_then(|value| value.parse::<u32>().ok()),
        }),
        "checkbox" if node_is_checked(node) => {
            DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
                kind: CheckmarkKind::Check,
            })
        }
        "radio" if node_is_checked(node) => DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
            kind: CheckmarkKind::Dot,
        }),
        _ => DisplayPaintContent::None,
    }
}

fn retained_display_str(value: &str, previous: Option<&Arc<str>>) -> Arc<str> {
    match previous {
        Some(previous) if previous.as_ref() == value => Arc::clone(previous),
        _ => Arc::from(value),
    }
}

fn retained_optional_display_str(
    value: Option<&str>,
    previous: Option<&Arc<str>>,
) -> Option<Arc<str>> {
    value.map(|value| retained_display_str(value, previous))
}

/// A `checkbox`/`radio` is checked when its `checked` attribute is present and
/// not an explicit false value (`checked`, `checked="true"`, `checked="1"`).
pub(crate) fn node_is_checked(node: &WidgetNode) -> bool {
    node.attributes
        .get("checked")
        .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1" | "checked"))
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

fn for_each_primitive_slot(node: &WidgetNode, mut visit: impl FnMut(DisplayPrimitiveSlot)) {
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

fn hash_paint_content_attributes(node: &WidgetNode, hasher: &mut DisplaySignatureHasher) {
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

fn batch_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
    let mut hasher = DisplaySignatureHasher::default();
    slot.hash(&mut hasher);
    hash_batch_material(node, slot, &mut hasher);
    hasher.finish()
}

fn hash_batch_material(
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

fn hash_generic_batch_material(node: &WidgetNode, hasher: &mut DisplaySignatureHasher) {
    match node.tag.as_str() {
        "input" => hash_text_batch_material(node, hasher),
        "slider" | "checkbox" | "radio" => hash_color(node.computed_style.color, hasher),
        _ => {}
    }
}

fn hash_text_batch_material(node: &WidgetNode, hasher: &mut DisplaySignatureHasher) {
    hash_color(node.computed_style.color, hasher);
    node.computed_style.font_family.hash(hasher);
    node.computed_style.font_size.to_bits().hash(hasher);
    node.computed_style.font_weight.hash(hasher);
    node.computed_style.line_height.to_bits().hash(hasher);
    std::mem::discriminant(&node.computed_style.text_align).hash(hasher);
}

fn hash_color(color: Color, hasher: &mut DisplaySignatureHasher) {
    color.r.hash(hasher);
    color.g.hash(hasher);
    color.b.hash(hasher);
    color.a.hash(hasher);
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
    fn display_text_payload_clone_shares_the_text_allocation() {
        let mut text_node = node(1, "text", 0.0, 0.0, 100.0, 20.0);
        text_node
            .attributes
            .insert("content".into(), "shared display text".into());

        let DisplayPaintContent::Text(first) = build_paint_content(&text_node) else {
            panic!("text node must produce text paint content");
        };
        let cloned = first.clone();

        assert!(Arc::ptr_eq(&first.text, &cloned.text));
        assert_eq!(first, cloned);
        assert_eq!(first.text.as_ref(), "shared display text");
    }

    #[test]
    fn rebuilt_display_node_retains_unchanged_text_allocation() {
        let mut text_node = node(1, "text", 0.0, 0.0, 100.0, 20.0);
        text_node
            .attributes
            .insert("content".into(), "retained display text".into());
        let first = build_paint_node(&text_node, 0.0, 0.0);

        text_node.computed_style.font_size += 1.0;
        let rebuilt = build_paint_node_with_previous(&text_node, 0.0, 0.0, Some(&first));
        let DisplayPaintContent::Text(first_text) = &first.content else {
            panic!("text node must produce text paint content");
        };
        let DisplayPaintContent::Text(rebuilt_text) = &rebuilt.content else {
            panic!("text node must produce text paint content");
        };

        assert!(Arc::ptr_eq(&first_text.text, &rebuilt_text.text));
    }

    #[test]
    fn display_payload_equality_falls_back_to_text_content() {
        let first = DisplayIconPaint {
            src: Some(Arc::from("icons/search.svg")),
            name: Some(Arc::from("search")),
            size: Some(16),
        };
        let second = DisplayIconPaint {
            src: Some(Arc::from("icons/search.svg")),
            name: Some(Arc::from("search")),
            size: Some(16),
        };

        assert!(!Arc::ptr_eq(
            first.src.as_ref().expect("first src"),
            second.src.as_ref().expect("second src")
        ));
        assert_eq!(first, second);
    }

    fn display_entry_benchmark_tree(rows: usize, cols: usize) -> WidgetNode {
        let mut root = node(
            1,
            "column",
            0.0,
            0.0,
            (cols as f32) * 12.0,
            (rows as f32) * 8.0,
        );
        let mut id = 2;
        for row_index in 0..rows {
            let mut row = node(
                id,
                "row",
                0.0,
                (row_index as f32) * 8.0,
                (cols as f32) * 12.0,
                8.0,
            );
            id += 1;
            for col_index in 0..cols {
                let mut cell = node(id, "text", (col_index as f32) * 12.0, 0.0, 10.0, 8.0);
                cell.attributes
                    .insert("content".into(), format!("{row_index}:{col_index}"));
                row.children.push(cell);
                id += 1;
            }
            root.children.push(row);
        }
        root
    }

    fn child_popup_benchmark_tree(rows: usize, cols: usize) -> WidgetNode {
        let mut root = node(1, "popover", 300.0, 180.0, 200.0, 120.0);
        let mut id = 2;
        for row in 0..rows {
            for col in 0..cols {
                let mut child = node(
                    id,
                    "box",
                    4.0 + col as f32 * 19.0,
                    4.0 + row as f32 * 18.0,
                    16.0,
                    14.0,
                );
                child.computed_style.background_color = Color {
                    r: (20 + row * 9) as u8,
                    g: (30 + col * 7) as u8,
                    b: 120,
                    a: 255,
                };
                root.children.push(child);
                id += 1;
            }
        }
        root
    }

    #[test]
    fn checkbox_and_radio_emit_checkmark_content_only_when_checked() {
        let mut checkbox = node(1, "checkbox", 0.0, 0.0, 18.0, 18.0);
        checkbox.attributes.insert("checked".into(), "true".into());
        assert_eq!(
            build_paint_content(&checkbox),
            DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
                kind: CheckmarkKind::Check,
            })
        );

        let mut radio = node(2, "radio", 0.0, 0.0, 18.0, 18.0);
        radio.attributes.insert("checked".into(), "checked".into());
        assert_eq!(
            build_paint_content(&radio),
            DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
                kind: CheckmarkKind::Dot,
            })
        );

        // Unchecked controls paint no mark.
        let unchecked = node(3, "checkbox", 0.0, 0.0, 18.0, 18.0);
        assert_eq!(build_paint_content(&unchecked), DisplayPaintContent::None);

        let mut falsey = node(4, "checkbox", 0.0, 0.0, 18.0, 18.0);
        falsey.attributes.insert("checked".into(), "false".into());
        assert_eq!(build_paint_content(&falsey), DisplayPaintContent::None);
    }

    #[test]
    fn primitive_signature_ignores_irrelevant_payload_attrs_for_generic_nodes() {
        let mut base = node(1, "box", 0.0, 0.0, 20.0, 20.0);
        let original = primitive_signature(&base, DisplayPrimitiveSlot::Generic);

        base.attributes.insert("content".into(), "ignored".into());
        base.attributes.insert("value".into(), "ignored".into());
        base.attributes.insert("src".into(), "ignored.png".into());

        assert_eq!(
            primitive_signature(&base, DisplayPrimitiveSlot::Generic),
            original
        );
    }

    #[test]
    fn primitive_signature_tracks_relevant_paint_payload_attrs() {
        let mut text = node(1, "text", 0.0, 0.0, 20.0, 20.0);
        let original_text = primitive_signature(&text, DisplayPrimitiveSlot::Generic);
        text.attributes.insert("content".into(), "changed".into());
        assert_ne!(
            primitive_signature(&text, DisplayPrimitiveSlot::Generic),
            original_text
        );

        let mut checkbox = node(2, "checkbox", 0.0, 0.0, 20.0, 20.0);
        let original_checkbox = primitive_signature(&checkbox, DisplayPrimitiveSlot::Generic);
        checkbox.attributes.insert("checked".into(), "true".into());
        assert_ne!(
            primitive_signature(&checkbox, DisplayPrimitiveSlot::Generic),
            original_checkbox
        );
    }

    #[test]
    fn batch_signature_uses_only_slot_material() {
        let mut background = node(1, "box", 0.0, 0.0, 20.0, 20.0);
        let original_background = batch_signature(&background, DisplayPrimitiveSlot::Background);

        background.computed_style.color = Color::from_hex("#ff00ff").unwrap();
        background.computed_style.font_size = 48.0;
        background.computed_style.border_color = Color::from_hex("#00ffff").unwrap();
        assert_eq!(
            batch_signature(&background, DisplayPrimitiveSlot::Background),
            original_background
        );

        background.computed_style.background_color = Color::from_hex("#123456").unwrap();
        assert_ne!(
            batch_signature(&background, DisplayPrimitiveSlot::Background),
            original_background
        );
    }

    #[test]
    fn batch_signature_tracks_generic_content_material() {
        let mut slider = node(1, "slider", 0.0, 0.0, 20.0, 20.0);
        slider.computed_style.background_color.a = 0;
        let original = batch_signature(&slider, DisplayPrimitiveSlot::Generic);

        slider.computed_style.font_size = 42.0;
        assert_eq!(
            batch_signature(&slider, DisplayPrimitiveSlot::Generic),
            original
        );

        slider.computed_style.color = Color::from_hex("#336699").unwrap();
        assert_ne!(
            batch_signature(&slider, DisplayPrimitiveSlot::Generic),
            original
        );
    }

    #[test]
    fn display_entries_skip_batch_signature_for_barriers() {
        let mut text = node(1, "text", 0.0, 0.0, 20.0, 20.0);
        text.computed_style.background_color.a = 0;
        text.attributes.insert("content".into(), "barrier".into());
        let mut out = Vec::new();
        let mut next = HashMap::new();

        collect_display_entries(&text, 0.0, 0.0, Some(&mut out), None, &mut next);

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0.slot, DisplayPrimitiveSlot::Text);
        assert_eq!(out[0].1.barrier, Some(DisplayBatchBarrier::Text));
        assert_eq!(out[0].1.batch_signature, 0);
    }

    #[test]
    fn display_entry_collection_can_patch_only_selected_nodes() {
        let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
        let mut first = node(2, "text", 0.0, 0.0, 40.0, 20.0);
        first.attributes.insert("content".into(), "first".into());
        let mut second = node(3, "text", 40.0, 0.0, 40.0, 20.0);
        second.attributes.insert("content".into(), "second".into());
        root.children.extend([first, second]);

        let mut full = HashMap::new();
        collect_display_entries(&root, 0.0, 0.0, None, None, &mut full);
        let mut selected = HashMap::new();
        collect_display_entries(
            &root,
            0.0,
            0.0,
            None,
            Some(&HashSet::from([3])),
            &mut selected,
        );

        assert!(!selected.is_empty());
        assert!(selected.keys().all(|key| key.node_id == 3));
        assert_eq!(
            selected.get(&DisplayListKey {
                node_id: 3,
                slot: DisplayPrimitiveSlot::Text,
            }),
            full.get(&DisplayListKey {
                node_id: 3,
                slot: DisplayPrimitiveSlot::Text,
            })
        );
    }

    // cargo test -p mesh-core-render --release -- display_primitive_hashing_beats_byte_fallback --ignored --nocapture
    #[test]
    #[ignore = "release-only display signature primitive hashing microbenchmark"]
    fn display_primitive_hashing_beats_byte_fallback() {
        #[derive(Default)]
        struct ByteOnlyHasher(u64);

        impl Hasher for ByteOnlyHasher {
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

        fn hash_fields(hasher: &mut impl Hasher) {
            4_u8.hash(hasher);
            0x1234_u16.hash(hasher);
            0x1234_5678_u32.hash(hasher);
            0x1234_5678_9abc_def0_u64.hash(hasher);
            0x1234_5678_9abc_def0_1234_5678_9abc_def0_u128.hash(hasher);
            1920_usize.hash(hasher);
            (-42_i32).hash(hasher);
            (-9001_i64).hash(hasher);
        }

        let iterations = 5_000_000;
        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0_u64;
        for _ in 0..iterations {
            let mut hasher = ByteOnlyHasher(0xcbf2_9ce4_8422_2325);
            hash_fields(&mut hasher);
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0_u64;
        for _ in 0..iterations {
            let mut hasher = DisplaySignatureHasher::default();
            hash_fields(&mut hasher);
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "display primitive hashing: byte fallback {old_time:?}; word-at-a-time {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_ne!(old_accumulator, 0);
        assert_ne!(new_accumulator, 0);
        assert!(new_time * 5 < old_time * 4);
    }

    // cargo test -p mesh-core-render --release -- retained_subtree_handle_beats_fieldwise_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only retained paint-subtree clone microbenchmark"]
    fn retained_subtree_handle_beats_fieldwise_clone() {
        let subtree = RetainedPaintSubtree {
            generation: 1,
            commands: vec![DisplayPaintCommand {
                node: Arc::new(build_paint_node(
                    &node(1, "box", 0.0, 0.0, 20.0, 20.0),
                    0.0,
                    0.0,
                )),
                clip: DisplayListClip {
                    x: 0,
                    y: 0,
                    width: 20,
                    height: 20,
                },
                kind: DisplayPaintCommandKind::Node,
            }]
            .into(),
            kinds: vec![DisplayPaintCommandKind::Node].into(),
            effect_overflow_count: 0,
            pruning: PruningMetrics::default(),
            command_span: Some(RetainedSubtreeSpan {
                bounds: DamageRect {
                    x: 0,
                    y: 0,
                    width: 20,
                    height: 20,
                },
                local_bounds: DamageRect {
                    x: 0,
                    y: 0,
                    width: 20,
                    height: 20,
                },
                command_count: 1,
                includes_scrollbars: false,
            }),
            child_order: Some(vec![0, 1, 2, 3].into()),
        };
        let retained = Arc::new(subtree.clone());
        let iterations = 10_000_000;

        let old_started = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(std::hint::black_box(&subtree).clone());
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(Arc::clone(std::hint::black_box(&retained)));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "retained subtree reuse clone: fieldwise {old_time:?}; whole-subtree handle {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time * 5 < old_time * 4);
    }

    // cargo test -p mesh-core-render --release -- arc_paint_command_node_beats_owned_command_clones --ignored --nocapture
    #[test]
    #[ignore = "release-only display paint command node clone microbenchmark"]
    fn arc_paint_command_node_beats_owned_command_clones() {
        #[derive(Clone)]
        struct OldDisplayPaintCommand {
            node: DisplayPaintNode,
            clip: DisplayListClip,
            kind: DisplayPaintCommandKind,
        }

        let mut widget = node(1, "text", 0.0, 0.0, 180.0, 24.0);
        widget.attributes.insert(
            "content".into(),
            "The same paint node is copied through retained command buffers".into(),
        );
        let paint_node = build_paint_node(&widget, 0.0, 0.0);
        let clip = DisplayListClip {
            x: 0,
            y: 0,
            width: 180,
            height: 24,
        };
        let old_commands = vec![
            OldDisplayPaintCommand {
                node: paint_node.clone(),
                clip,
                kind: DisplayPaintCommandKind::Node,
            },
            OldDisplayPaintCommand {
                node: paint_node.clone(),
                clip,
                kind: DisplayPaintCommandKind::Scrollbars,
            },
        ];
        let shared_node = Arc::new(paint_node);
        let new_commands = vec![
            DisplayPaintCommand {
                node: Arc::clone(&shared_node),
                clip,
                kind: DisplayPaintCommandKind::Node,
            },
            DisplayPaintCommand {
                node: shared_node,
                clip,
                kind: DisplayPaintCommandKind::Scrollbars,
            },
        ];
        let iterations = 2_000_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let cloned = old_commands.clone();
            old_total = old_total.wrapping_add(
                cloned
                    .iter()
                    .map(|command| {
                        command.node.id as usize
                            + command.clip.width as usize
                            + usize::from(command.kind == DisplayPaintCommandKind::Scrollbars)
                    })
                    .sum::<usize>(),
            );
            std::hint::black_box(cloned);
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let cloned = new_commands.clone();
            new_total = new_total.wrapping_add(
                cloned
                    .iter()
                    .map(|command| {
                        command.node.id as usize
                            + command.clip.width as usize
                            + usize::from(command.kind == DisplayPaintCommandKind::Scrollbars)
                    })
                    .sum::<usize>(),
            );
            std::hint::black_box(cloned);
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "display paint command node clone: owned node {old_time:?}; arc node {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-render --release -- unchanged_display_list_update_beats_flat_rebuild --ignored --nocapture
    #[test]
    #[ignore = "release-only unchanged display-list update microbenchmark"]
    fn unchanged_display_list_update_beats_flat_rebuild() {
        let root = display_entry_benchmark_tree(24, 24);
        let iterations = 1_000;

        let mut retained = RetainedDisplayList::default();
        retained.update(&root, 1200, 800, false, true);

        let no_op_started = std::time::Instant::now();
        let mut no_op_accumulator = 0u64;
        for _ in 0..iterations {
            let metrics = retained.update(&root, 1200, 800, false, true);
            no_op_accumulator =
                no_op_accumulator.wrapping_add(std::hint::black_box(metrics.entries_reused));
        }
        let no_op_time = no_op_started.elapsed();

        let rebuild_started = std::time::Instant::now();
        let mut rebuild_accumulator = 0u64;
        for _ in 0..iterations {
            let mut rebuilt = RetainedDisplayList::default();
            let metrics = rebuilt.update(&root, 1200, 800, false, true);
            rebuild_accumulator =
                rebuild_accumulator.wrapping_add(std::hint::black_box(metrics.entries_rebuilt));
        }
        let rebuild_time = rebuild_started.elapsed();

        eprintln!(
            "unchanged display-list update: no-op {no_op_time:?}; fresh flat rebuild {rebuild_time:?}; ratio {:.1}x; accumulators={no_op_accumulator}/{rebuild_accumulator}",
            rebuild_time.as_secs_f64() / no_op_time.as_secs_f64()
        );
        assert_ne!(no_op_accumulator, 0);
        assert_ne!(rebuild_accumulator, 0);
        assert!(no_op_time < rebuild_time);
    }

    // cargo test -p mesh-core-render --release -- retained_generation_shortcut_beats_non_clean_entry_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only retained-generation display-list microbenchmark"]
    fn retained_generation_shortcut_beats_non_clean_entry_scan() {
        let root = display_entry_benchmark_tree(120, 20);
        let iterations = 2_000;
        let empty_dirty = HashSet::new();

        let mut scanned = RetainedDisplayList::default();
        scanned.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary::default(),
            &empty_dirty,
            1200,
            800,
            false,
            true,
        );
        let scan_started = std::time::Instant::now();
        let mut scan_total = 0u64;
        for _ in 0..iterations {
            let metrics = scanned.update_with_dirty_nodes(
                &root,
                RenderObjectDirtySummary::default(),
                &empty_dirty,
                1200,
                800,
                false,
                true,
            );
            scan_total = scan_total.wrapping_add(std::hint::black_box(metrics.entries_reused));
        }
        let scan_time = scan_started.elapsed();

        let mut generation_gated = RetainedDisplayList::default();
        generation_gated.update_for_retained_generation(
            &root,
            1,
            RenderObjectDirtySummary::default(),
            &empty_dirty,
            1200,
            800,
            false,
            true,
        );
        let gated_started = std::time::Instant::now();
        let mut gated_total = 0u64;
        for _ in 0..iterations {
            let metrics = generation_gated.update_for_retained_generation(
                &root,
                1,
                RenderObjectDirtySummary::default(),
                &empty_dirty,
                1200,
                800,
                false,
                true,
            );
            gated_total = gated_total.wrapping_add(std::hint::black_box(metrics.entries_reused));
        }
        let gated_time = gated_started.elapsed();

        assert_eq!(scan_total, gated_total);
        eprintln!(
            "unchanged non-clean display-list sync: entry scan {scan_time:?}; retained-generation gate {gated_time:?}; ratio {:.1}x",
            scan_time.as_secs_f64() / gated_time.as_secs_f64()
        );
        assert!(gated_time * 2 < scan_time);
    }

    // cargo test -p mesh-core-render --release -- sparse_display_entry_patch_beats_full_signature_collection --ignored --nocapture
    #[test]
    #[ignore = "release-only sparse display-entry collection microbenchmark"]
    fn sparse_display_entry_patch_beats_full_signature_collection() {
        let root = display_entry_benchmark_tree(120, 20);
        let iterations = 2_000;
        let selected_ids = HashSet::from([1_200_u64]);
        let mut retained = HashMap::new();
        collect_display_entries(&root, 0.0, 0.0, None, None, &mut retained);

        let full_started = std::time::Instant::now();
        let mut full = HashMap::new();
        let mut full_total = 0usize;
        for _ in 0..iterations {
            full.clear();
            collect_display_entries(&root, 0.0, 0.0, None, None, &mut full);
            full_total = full_total.wrapping_add(std::hint::black_box(full.len()));
        }
        let full_time = full_started.elapsed();

        let copied_started = std::time::Instant::now();
        let mut copied = HashMap::new();
        let mut copied_total = 0usize;
        for _ in 0..iterations {
            copied.clear();
            copied.extend(retained.iter().map(|(key, entry)| (*key, *entry)));
            for node_id in &selected_ids {
                for slot in DISPLAY_PRIMITIVE_SLOTS {
                    copied.remove(&DisplayListKey {
                        node_id: *node_id,
                        slot,
                    });
                }
            }
            collect_display_entries(&root, 0.0, 0.0, None, Some(&selected_ids), &mut copied);
            copied_total = copied_total.wrapping_add(std::hint::black_box(copied.len()));
        }
        let copied_time = copied_started.elapsed();

        let in_place_started = std::time::Instant::now();
        let mut in_place = retained.clone();
        let mut replacements = HashMap::new();
        let mut in_place_total = 0usize;
        for _ in 0..iterations {
            replacements.clear();
            collect_display_entries(
                &root,
                0.0,
                0.0,
                None,
                Some(&selected_ids),
                &mut replacements,
            );
            for node_id in &selected_ids {
                for slot in DISPLAY_PRIMITIVE_SLOTS {
                    let key = DisplayListKey {
                        node_id: *node_id,
                        slot,
                    };
                    if let Some(entry) = replacements.remove(&key) {
                        in_place.insert(key, entry);
                    } else {
                        in_place.remove(&key);
                    }
                }
            }
            in_place_total = in_place_total.wrapping_add(std::hint::black_box(in_place.len()));
        }
        let in_place_time = in_place_started.elapsed();

        assert_eq!(full_total, copied_total);
        assert_eq!(copied_total, in_place_total);
        assert_eq!(full, copied);
        assert_eq!(copied, in_place);
        eprintln!(
            "sparse display entries: full signatures {full_time:?}; copied-map patch {copied_time:?}; in-place patch {in_place_time:?}; copy elimination ratio {:.1}x",
            copied_time.as_secs_f64() / in_place_time.as_secs_f64()
        );
        assert!(
            in_place_time * 5 < copied_time * 4,
            "in-place sparse patching should beat copied-map patching by at least 20%"
        );
    }

    // cargo test -p mesh-core-render --release -- tag_aware_payload_signature_skips_irrelevant_attr_hashes --ignored --nocapture
    #[test]
    #[ignore = "release-only display signature payload hashing microbenchmark"]
    fn tag_aware_payload_signature_skips_irrelevant_attr_hashes() {
        fn old_hash_all_payload_attrs(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
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
            hasher.finish()
        }

        let mut nodes = Vec::new();
        for index in 0..512_u64 {
            let tag = if index % 8 == 0 { "text" } else { "box" };
            let mut item = node(index + 1, tag, 0.0, 0.0, 20.0, 20.0);
            item.attributes
                .insert("content".into(), format!("row {index}"));
            item.attributes.insert("value".into(), index.to_string());
            item.attributes.insert("src".into(), "icon.png".into());
            nodes.push(item);
        }
        let iterations = 20_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u64;
        for _ in 0..iterations {
            for item in &nodes {
                old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
                    old_hash_all_payload_attrs(item, DisplayPrimitiveSlot::Generic),
                ));
            }
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u64;
        for _ in 0..iterations {
            for item in &nodes {
                let mut hasher = DisplaySignatureHasher::default();
                DisplayPrimitiveSlot::Generic.hash(&mut hasher);
                item.tag.hash(&mut hasher);
                hash_paint_content_attributes(item, &mut hasher);
                new_accumulator =
                    new_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
            }
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "display payload signature attrs: all attrs {old_time:?}; tag-aware {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_ne!(old_accumulator, 0);
        assert_ne!(new_accumulator, 0);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-render --release -- slot_aware_batch_signature_skips_irrelevant_material_hashes --ignored --nocapture
    #[test]
    #[ignore = "release-only display batch signature material hashing microbenchmark"]
    fn slot_aware_batch_signature_skips_irrelevant_material_hashes() {
        fn old_batch_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
            let mut hasher = DisplaySignatureHasher::default();
            slot.hash(&mut hasher);
            hash_color(node.computed_style.background_color, &mut hasher);
            hash_color(node.computed_style.border_color, &mut hasher);
            hash_color(node.computed_style.color, &mut hasher);
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

        let mut nodes = Vec::new();
        for index in 0..512_u64 {
            let mut item = node(index + 1, "box", 0.0, 0.0, 20.0, 20.0);
            item.computed_style.background_color = Color {
                r: (index % 251) as u8,
                g: ((index * 3) % 251) as u8,
                b: ((index * 7) % 251) as u8,
                a: 255,
            };
            item.computed_style.border_color = Color {
                r: ((index * 11) % 251) as u8,
                g: ((index * 13) % 251) as u8,
                b: ((index * 17) % 251) as u8,
                a: 255,
            };
            item.computed_style.color = Color {
                r: ((index * 19) % 251) as u8,
                g: ((index * 23) % 251) as u8,
                b: ((index * 29) % 251) as u8,
                a: 255,
            };
            if index % 4 == 0 {
                item.computed_style.background_paint =
                    BackgroundPaint::LinearGradient(StyleLinearGradient {
                        from: Color::from_hex("#112233").unwrap(),
                        to: Color::from_hex("#445566").unwrap(),
                    });
            }
            nodes.push(item);
        }
        let iterations = 50_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u64;
        for _ in 0..iterations {
            for item in &nodes {
                old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
                    old_batch_signature(item, DisplayPrimitiveSlot::Background),
                ));
            }
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u64;
        for _ in 0..iterations {
            for item in &nodes {
                new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(
                    batch_signature(item, DisplayPrimitiveSlot::Background),
                ));
            }
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "display batch signature material: broad {old_time:?}; slot-aware {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_ne!(old_accumulator, 0);
        assert_ne!(new_accumulator, 0);
        assert!(new_time < old_time);
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
    fn display_list_preserves_disjoint_changed_entry_damage_rects() {
        let mut root = node(1, "row", 0.0, 0.0, 200.0, 40.0);
        root.children.push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
        root.children.push(node(3, "box", 160.0, 0.0, 20.0, 20.0));
        let mut list = RetainedDisplayList::default();
        list.update(&root, 200, 40, false, true);

        root.children[0].computed_style.background_color.r = 40;
        root.children[1].computed_style.background_color.r = 50;
        let metrics = list.update(&root, 200, 40, false, true);

        assert_eq!(metrics.entries_rebuilt, 2);
        assert_eq!(list.damage_rects().len(), 2);
        assert!(list.damage_rects().contains(&DamageRect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        }));
        assert!(list.damage_rects().contains(&DamageRect {
            x: 160,
            y: 0,
            width: 20,
            height: 20,
        }));
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

    // cargo test -p mesh-core-render --release -- removal_scan_guard_skips_equal_key_sets --ignored --nocapture
    #[test]
    #[ignore = "release-only display-list removal scan microbenchmark"]
    fn removal_scan_guard_skips_equal_key_sets() {
        fn old_removed_count(previous: &HashMap<u64, u64>, next: &HashMap<u64, u64>) -> usize {
            let mut removed = 0usize;
            for key in previous.keys() {
                if !next.contains_key(key) {
                    removed += 1;
                }
            }
            removed
        }

        fn guarded_removed_count(
            previous: &HashMap<u64, u64>,
            next: &HashMap<u64, u64>,
            inserted: usize,
        ) -> usize {
            if inserted == 0 && previous.len() == next.len() {
                return 0;
            }
            old_removed_count(previous, next)
        }

        let previous = (0..1024_u64)
            .map(|index| (index, index.wrapping_mul(3)))
            .collect::<HashMap<_, _>>();
        let next = (0..1024_u64)
            .map(|index| (index, index.wrapping_mul(5)))
            .collect::<HashMap<_, _>>();
        let iterations = 200_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total +=
                old_removed_count(std::hint::black_box(&previous), std::hint::black_box(&next));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_total += guarded_removed_count(
                std::hint::black_box(&previous),
                std::hint::black_box(&next),
                std::hint::black_box(0),
            );
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "display-list removal scan: full previous-entry scan {old_time:?}; guarded skip {new_time:?}; ratio {:.1}x; counts={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time * 10 < old_time);
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
    fn sparse_entry_patch_matches_full_collection_for_text_updates() {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        let mut unchanged = node(2, "text", 0.0, 0.0, 50.0, 20.0);
        unchanged
            .attributes
            .insert("content".into(), "unchanged".into());
        let mut changed = node(3, "text", 50.0, 0.0, 50.0, 20.0);
        changed.attributes.insert("content".into(), "before".into());
        root.children.extend([unchanged, changed]);

        let mut full = RetainedDisplayList::default();
        let mut sparse = RetainedDisplayList::default();
        full.update(&root, 120, 40, false, true);
        sparse.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                inserted: 3,
                ..Default::default()
            },
            &HashSet::from([1, 2, 3]),
            120,
            40,
            false,
            true,
        );

        root.children[1]
            .attributes
            .insert("content".into(), "after".into());
        let full_metrics = full.update(&root, 120, 40, false, true);
        let sparse_metrics = sparse.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                text: 1,
                ..Default::default()
            },
            &HashSet::from([3]),
            120,
            40,
            false,
            true,
        );

        assert_eq!(sparse.entries, full.entries);
        assert_eq!(sparse.damage_rects(), full.damage_rects());
        assert_eq!(sparse_metrics.entries_rebuilt, full_metrics.entries_rebuilt);
        assert_eq!(sparse_metrics.damage_area, full_metrics.damage_area);
    }

    #[test]
    fn sparse_entry_patch_matches_full_collection_for_material_updates() {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        let unchanged = node(2, "box", 0.0, 0.0, 50.0, 20.0);
        let changed = node(3, "box", 50.0, 0.0, 50.0, 20.0);
        root.children.extend([unchanged, changed]);

        let mut full = RetainedDisplayList::default();
        let mut sparse = RetainedDisplayList::default();
        full.update(&root, 120, 40, false, true);
        sparse.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                inserted: 3,
                ..Default::default()
            },
            &HashSet::from([1, 2, 3]),
            120,
            40,
            false,
            true,
        );

        root.children[1].computed_style.background_color = Color {
            r: 220,
            g: 40,
            b: 30,
            a: 255,
        };
        let full_metrics = full.update(&root, 120, 40, false, true);
        let sparse_metrics = sparse.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                material: 1,
                ..Default::default()
            },
            &HashSet::from([3]),
            120,
            40,
            false,
            true,
        );

        assert_eq!(sparse.entries, full.entries);
        assert_eq!(sparse.damage_rects(), full.damage_rects());
        assert_eq!(sparse_metrics.entries_rebuilt, full_metrics.entries_rebuilt);
        assert_eq!(sparse_metrics.damage_area, full_metrics.damage_area);
        assert_eq!(
            command_debugs(sparse.paint_commands(), &[1, 2, 3]),
            command_debugs(full.paint_commands(), &[1, 2, 3])
        );
    }

    // cargo test -p mesh-core-render --release -- sparse_material_update_beats_full_display_rebuild --ignored --nocapture
    #[test]
    #[ignore = "release-only sparse material-update display-list benchmark"]
    fn sparse_material_update_beats_full_display_rebuild() {
        let iterations = 1_000_u64;
        let changed_row = 60;
        let changed_col = 10;

        let mut full_root = display_entry_benchmark_tree(120, 20);
        let changed_id = full_root.children[changed_row].children[changed_col].id;
        let mut full = RetainedDisplayList::default();
        full.update(&full_root, 1200, 800, false, true);
        let full_started = std::time::Instant::now();
        let mut full_rebuilt = 0u64;
        for generation in 0..iterations {
            full_root.children[changed_row].children[changed_col]
                .computed_style
                .background_color
                .r = (generation % 251) as u8;
            full_rebuilt = full_rebuilt.wrapping_add(std::hint::black_box(
                full.update(&full_root, 1200, 800, false, true)
                    .entries_rebuilt,
            ));
        }
        let full_time = full_started.elapsed();

        let mut sparse_root = display_entry_benchmark_tree(120, 20);
        let mut sparse = RetainedDisplayList::default();
        sparse.update(&sparse_root, 1200, 800, false, true);
        let dirty_ids = HashSet::from([changed_id]);
        let sparse_started = std::time::Instant::now();
        let mut sparse_rebuilt = 0u64;
        for generation in 0..iterations {
            sparse_root.children[changed_row].children[changed_col]
                .computed_style
                .background_color
                .r = (generation % 251) as u8;
            sparse_rebuilt = sparse_rebuilt.wrapping_add(std::hint::black_box(
                sparse
                    .update_with_dirty_nodes(
                        &sparse_root,
                        RenderObjectDirtySummary {
                            material: 1,
                            ..Default::default()
                        },
                        &dirty_ids,
                        1200,
                        800,
                        false,
                        true,
                    )
                    .entries_rebuilt,
            ));
        }
        let sparse_time = sparse_started.elapsed();

        assert_eq!(sparse_rebuilt, full_rebuilt);
        assert_eq!(sparse.entries, full.entries);
        assert_eq!(sparse.damage_rects(), full.damage_rects());
        eprintln!(
            "one-node material display-list update: full {full_time:?}; sparse {sparse_time:?}; ratio {:.1}x",
            full_time.as_secs_f64() / sparse_time.as_secs_f64()
        );
        assert!(
            sparse_time * 2 < full_time,
            "sparse material updates should be at least 2x faster than full display-list rebuilds"
        );
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
    fn subtree_generation_ignores_unrelated_surface_paint_changes() {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        let mut sibling = node(2, "box", 0.0, 0.0, 40.0, 40.0);
        sibling.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
        let mut popup = node(4, "popover", 60.0, 0.0, 40.0, 40.0);
        popup.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
        root.children.push(sibling);
        root.children.push(popup);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 120, 40, false, true);
        let initial_root = list.generation();
        let initial_popup = list.subtree_generation(4).expect("popup subtree");

        root.children[0].computed_style.background_color.r = 99;
        list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                material: 1,
                ..Default::default()
            },
            &HashSet::from([2]),
            120,
            40,
            false,
            true,
        );

        assert!(list.generation() > initial_root);
        assert_eq!(list.subtree_generation(4), Some(initial_popup));

        root.children[1].children[0]
            .computed_style
            .background_color
            .g = 77;
        list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                material: 1,
                ..Default::default()
            },
            &HashSet::from([5]),
            120,
            40,
            false,
            true,
        );

        assert!(list.subtree_generation(4).expect("popup subtree") > initial_popup);
        assert_eq!(list.subtree_generation(999), None);
    }

    #[test]
    fn target_local_display_list_matches_immediate_child_popup_pixels() {
        let popup = child_popup_benchmark_tree(4, 6);
        let offset_x = -popup.layout.x + 7.0;
        let offset_y = -popup.layout.y + 5.0;
        let mut immediate = crate::PixelBuffer::new(214, 132);
        let mut retained = crate::PixelBuffer::new(214, 132);

        crate::paint_frontend_tree_at_for_module(
            &popup,
            &mut immediate,
            1.0,
            offset_x,
            offset_y,
            None,
            None,
        );
        let mut display_list = RetainedDisplayList::default();
        display_list.update_at(&popup, offset_x, offset_y, 214, 132, false, false);
        crate::paint_display_list_for_module_with_profiling_metrics(
            display_list.paint_commands(),
            &mut retained,
            1.0,
            None,
            None,
            None,
            None,
        );

        assert_eq!(retained.data, immediate.data);
    }

    // cargo test -p mesh-core-render --release -- retained_child_popup_replay_beats_immediate_tree_paint --ignored --nocapture
    #[test]
    #[ignore = "release-only child popup retained-replay benchmark"]
    fn retained_child_popup_replay_beats_immediate_tree_paint() {
        const ITERATIONS: u64 = 400;
        let offset_x = -300.0 + 6.0;
        let offset_y = -180.0 + 6.0;
        let changed_id = 1;

        let mut immediate_tree = child_popup_benchmark_tree(6, 10);
        let mut immediate_buffer = crate::PixelBuffer::new(212, 132);
        let immediate_started = std::time::Instant::now();
        for generation in 1..=ITERATIONS {
            immediate_tree.computed_style.opacity = 0.25 + (generation % 70) as f32 / 100.0;
            immediate_buffer.clear(Color::TRANSPARENT);
            crate::paint_frontend_tree_at_for_module(
                std::hint::black_box(&immediate_tree),
                &mut immediate_buffer,
                1.0,
                offset_x,
                offset_y,
                None,
                None,
            );
        }
        let immediate_time = immediate_started.elapsed();

        let mut retained_tree = child_popup_benchmark_tree(6, 10);
        let mut retained_buffer = crate::PixelBuffer::new(212, 132);
        let mut display_list = RetainedDisplayList::default();
        display_list.update_at_for_retained_generation(
            &retained_tree,
            0,
            offset_x,
            offset_y,
            212,
            132,
            false,
            false,
        );
        let dirty_ids = HashSet::from([changed_id]);
        let retained_started = std::time::Instant::now();
        for generation in 1..=ITERATIONS {
            retained_tree.computed_style.opacity = 0.25 + (generation % 70) as f32 / 100.0;
            display_list.update_at_for_retained_generation_with_dirty_nodes(
                std::hint::black_box(&retained_tree),
                generation,
                RenderObjectDirtySummary {
                    opacity: 1,
                    ..Default::default()
                },
                &dirty_ids,
                offset_x,
                offset_y,
                212,
                132,
                false,
                false,
            );
            retained_buffer.clear(Color::TRANSPARENT);
            crate::paint_display_list_for_module_with_profiling_metrics(
                display_list.paint_commands(),
                &mut retained_buffer,
                1.0,
                None,
                None,
                None,
                None,
            );
        }
        let retained_time = retained_started.elapsed();

        assert_eq!(retained_buffer.data, immediate_buffer.data);
        eprintln!(
            "animated child popup raster: immediate tree {immediate_time:?}; retained display-list {retained_time:?}; ratio {:.2}x",
            immediate_time.as_secs_f64() / retained_time.as_secs_f64()
        );
        assert!(
            retained_time * 10 < immediate_time * 9,
            "retained child replay should improve the production animation path by at least 10%"
        );
    }

    // cargo test -p mesh-core-render --release -- popup_subtree_generation_beats_broad_surface_repaint --ignored --nocapture
    #[test]
    #[ignore = "release-only child-popup invalidation microbenchmark"]
    fn popup_subtree_generation_beats_broad_surface_repaint() {
        const FRAMES: u64 = 10_000;
        const BUFFER_BYTES: usize = 160 * 90 * 4;

        let mut eager_buffer = vec![255_u8; BUFFER_BYTES];
        let eager_started = std::time::Instant::now();
        let mut cached_parent_generation = 0_u64;
        for parent_generation in 1..=FRAMES {
            if std::hint::black_box(parent_generation) != cached_parent_generation {
                std::hint::black_box(&mut eager_buffer).fill(0);
                cached_parent_generation = parent_generation;
            }
        }
        let eager_time = eager_started.elapsed();

        let mut retained_buffer = vec![255_u8; BUFFER_BYTES];
        let retained_started = std::time::Instant::now();
        let popup_generation = 1_u64;
        let mut cached_popup_generation = 0_u64;
        let mut repaints = 0_u64;
        for _ in 0..FRAMES {
            if std::hint::black_box(popup_generation) != cached_popup_generation {
                std::hint::black_box(&mut retained_buffer).fill(0);
                cached_popup_generation = popup_generation;
                repaints += 1;
            }
        }
        let retained_time = retained_started.elapsed();

        assert_eq!(repaints, 1);
        assert_eq!(eager_buffer, retained_buffer);
        eprintln!(
            "unrelated parent updates: broad generation {eager_time:?}; popup subtree generation {retained_time:?}; ratio {:.1}x; repaints={FRAMES}/{repaints}",
            eager_time.as_secs_f64() / retained_time.as_secs_f64()
        );
        assert!(retained_time * 10 < eager_time);
    }

    #[test]
    fn dirty_ancestor_collection_preserves_ancestors_for_sparse_dirty_nodes() {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
        left.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
        let mut right = node(4, "box", 60.0, 0.0, 40.0, 40.0);
        right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
        root.children.push(left);
        root.children.push(right);

        let ancestors = collect_dirty_ancestor_ids(&root, &HashSet::from([3]));

        assert_eq!(ancestors, HashSet::from([1, 2]));
    }

    // cargo test -p mesh-core-render --release -- dirty_ancestor_collection_stops_after_sparse_dirty_nodes --ignored --nocapture
    #[test]
    #[ignore = "release-only dirty ancestor microbenchmark"]
    fn dirty_ancestor_collection_stops_after_sparse_dirty_nodes() {
        fn build_subtree(next_id: &mut NodeId, width: usize, depth: usize) -> WidgetNode {
            let id = *next_id;
            *next_id += 1;
            let mut root = node(id, "box", 0.0, 0.0, 20.0, 20.0);
            if depth > 0 {
                root.children = (0..width)
                    .map(|_| build_subtree(next_id, width, depth - 1))
                    .collect();
            }
            root
        }

        fn old_collect_dirty_ancestor_ids(
            root: &WidgetNode,
            dirty_node_ids: &HashSet<NodeId>,
        ) -> HashSet<NodeId> {
            fn walk(
                node: &WidgetNode,
                dirty_node_ids: &HashSet<NodeId>,
                path: &mut Vec<NodeId>,
                ancestors: &mut HashSet<NodeId>,
            ) {
                if dirty_node_ids.contains(&node.id) {
                    for ancestor in path.iter().copied() {
                        ancestors.insert(ancestor);
                    }
                }
                path.push(node.id);
                for child in &node.children {
                    walk(child, dirty_node_ids, path, ancestors);
                }
                path.pop();
            }

            let mut ancestors = HashSet::new();
            let mut path = Vec::new();
            walk(root, dirty_node_ids, &mut path, &mut ancestors);
            ancestors
        }

        let mut next_id = 1;
        let root = build_subtree(&mut next_id, 5, 5);
        let dirty = HashSet::from([root.children[0].children[0].children[0].id]);
        let iterations = 50_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total += old_collect_dirty_ancestor_ids(
                std::hint::black_box(&root),
                std::hint::black_box(&dirty),
            )
            .len();
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_total += collect_dirty_ancestor_ids(
                std::hint::black_box(&root),
                std::hint::black_box(&dirty),
            )
            .len();
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "dirty ancestor collection: full walk {old_time:?}; early exit {new_time:?}; ratio {:.1}x; counts={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time * 2 < old_time);
    }

    // cargo test -p mesh-core-render --release -- dirty_ancestor_scratch_reuse_beats_fresh_allocations --ignored --nocapture
    #[test]
    #[ignore = "release-only dirty ancestor scratch microbenchmark"]
    fn dirty_ancestor_scratch_reuse_beats_fresh_allocations() {
        fn build_subtree(next_id: &mut NodeId, width: usize, depth: usize) -> WidgetNode {
            let id = *next_id;
            *next_id += 1;
            let mut root = node(id, "box", 0.0, 0.0, 20.0, 20.0);
            if depth > 0 {
                root.children = (0..width)
                    .map(|_| build_subtree(next_id, width, depth - 1))
                    .collect();
            }
            root
        }

        let mut next_id = 1;
        let root = build_subtree(&mut next_id, 5, 5);
        let dirty = HashSet::from([root.children[0].children[0].children[0].id]);
        let iterations = 50_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total += std::hint::black_box(collect_dirty_ancestor_ids(&root, &dirty).len());
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        let mut ancestors = HashSet::new();
        let mut path = Vec::new();
        for _ in 0..iterations {
            ancestors.clear();
            path.clear();
            collect_dirty_ancestor_ids_into(&root, &dirty, &mut path, &mut ancestors);
            new_total += std::hint::black_box(ancestors.len());
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_total, new_total);
        eprintln!(
            "dirty ancestor scratch: fresh {old_time:?}; reused {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    #[test]
    fn display_list_unchanged_tree_skips_flat_command_rebuild() {
        let root = display_entry_benchmark_tree(8, 8);
        let mut list = RetainedDisplayList::default();
        let first = list.update(&root, 800, 400, false, true);
        let initial_commands = format!("{:?}", list.paint_commands());

        let second = list.update(&root, 800, 400, false, true);

        assert!(first.subtree_commands_rebuilt > 0);
        assert_eq!(second.entries_rebuilt, 0);
        assert_eq!(second.entries_reused, first.entries_total);
        assert_eq!(second.subtree_segments_rebuilt, 0);
        assert_eq!(second.subtree_commands_rebuilt, 0);
        assert_eq!(second.damage_area, 0);
        assert_eq!(format!("{:?}", list.paint_commands()), initial_commands);
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
    fn display_list_reuses_clean_descendants_for_paint_only_dirty_parent() {
        let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
        let mut panel = node(2, "box", 0.0, 0.0, 120.0, 40.0);
        panel.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
        panel.children.push(node(4, "text", 30.0, 4.0, 20.0, 12.0));
        root.children.push(panel);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 160, 40, false, true);
        let before = list.paint_commands().to_vec();

        root.children[0].computed_style.background_color = Color::from_hex("#336699").unwrap();
        let metrics = list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                material: 1,
                ..Default::default()
            },
            &HashSet::from([2]),
            160,
            40,
            false,
            true,
        );
        let after = list.paint_commands().to_vec();

        assert_eq!(
            command_debugs(&before, &[3, 4]),
            command_debugs(&after, &[3, 4])
        );
        assert!(
            metrics.subtree_segments_reused >= 2,
            "paint-only dirty parent should reuse clean child subtrees: {metrics:?}"
        );
        assert_eq!(
            metrics.subtree_commands_rebuilt, 2,
            "only root and the dirty parent should rebuild their local commands"
        );
    }

    #[test]
    fn display_list_rebuilds_descendants_for_layout_dirty_parent() {
        let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
        let mut panel = node(2, "box", 0.0, 0.0, 120.0, 40.0);
        panel.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
        panel.children.push(node(4, "text", 30.0, 4.0, 20.0, 12.0));
        root.children.push(panel);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 160, 40, false, true);

        root.children[0].layout.x = 8.0;
        let metrics = list.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary {
                geometry: 1,
                ..Default::default()
            },
            &HashSet::from([2]),
            160,
            40,
            false,
            true,
        );

        assert_eq!(
            metrics.subtree_segments_reused, 0,
            "layout dirty parent must rebuild descendants because offsets changed"
        );
        assert_eq!(metrics.subtree_commands_rebuilt, 4);
    }

    // cargo test -p mesh-core-render --release -- paint_only_dirty_parent_reuses_clean_descendants --ignored --nocapture
    #[test]
    #[ignore = "release-only display-list paint-only subtree reuse microbenchmark"]
    fn paint_only_dirty_parent_reuses_clean_descendants() {
        fn make_tree(children: usize) -> WidgetNode {
            let mut root = node(1, "row", 0.0, 0.0, children as f32 * 12.0, 24.0);
            let mut panel = node(2, "box", 0.0, 0.0, children as f32 * 12.0, 24.0);
            for index in 0..children {
                let id = 3 + index as NodeId;
                let mut child = node(id, "text", index as f32 * 12.0, 0.0, 10.0, 12.0);
                child
                    .attributes
                    .insert("content".into(), format!("Item {index}"));
                panel.children.push(child);
            }
            root.children.push(panel);
            root
        }

        let mut root = make_tree(512);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 6200, 40, false, true);
        root.children[0].computed_style.background_color = Color::from_hex("#335577").unwrap();

        let dirty_node_ids = HashSet::from([2]);
        let dirty_ancestors = collect_dirty_ancestor_ids(&root, &dirty_node_ids);
        let vclip = surface_clip(DamageRect {
            x: 0,
            y: 0,
            width: 6200,
            height: 40,
        });
        let iterations = 1_000usize;

        let old_started = std::time::Instant::now();
        let mut old_rebuilt_commands = 0u64;
        for _ in 0..iterations {
            let mut next_subtrees = HashMap::new();
            let mut metrics = LocalReuseMetrics::default();
            build_paint_subtree(
                std::hint::black_box(&root),
                0.0,
                0.0,
                vclip,
                vclip,
                false,
                false,
                &dirty_node_ids,
                &dirty_ancestors,
                &list.subtrees,
                &mut next_subtrees,
                &mut metrics,
            );
            old_rebuilt_commands =
                old_rebuilt_commands.saturating_add(std::hint::black_box(metrics.rebuilt_commands));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_rebuilt_commands = 0u64;
        for _ in 0..iterations {
            let mut next_subtrees = HashMap::new();
            let mut metrics = LocalReuseMetrics::default();
            build_paint_subtree(
                std::hint::black_box(&root),
                0.0,
                0.0,
                vclip,
                vclip,
                false,
                true,
                &dirty_node_ids,
                &dirty_ancestors,
                &list.subtrees,
                &mut next_subtrees,
                &mut metrics,
            );
            new_rebuilt_commands =
                new_rebuilt_commands.saturating_add(std::hint::black_box(metrics.rebuilt_commands));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "paint-only dirty parent subtree reuse: forced {old_time:?}; descendant-reuse {new_time:?}; ratio {:.1}x; rebuilt_commands={old_rebuilt_commands}/{new_rebuilt_commands}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
        assert!(new_rebuilt_commands < old_rebuilt_commands);
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

        assert_eq!(
            list.command_spans.as_ref(),
            build_command_spans(&root, &list.subtrees)
        );

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
                assert_eq!(text.text.as_ref(), "hello");
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

    // cargo test -p mesh-core-render --release -- display_entry_scratch_reuse_beats_fresh_allocations_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only display-list scratch allocation microbenchmark"]
    fn display_entry_scratch_reuse_beats_fresh_allocations_benchmark() {
        let tree = display_entry_benchmark_tree(120, 20);
        let iterations = 2_000;

        let old_start = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let mut ordered_entries = Vec::new();
            let mut next = HashMap::new();
            collect_display_entries(&tree, 0.0, 0.0, Some(&mut ordered_entries), None, &mut next);
            old_total = old_total
                .saturating_add(std::hint::black_box(ordered_entries.len()))
                .saturating_add(std::hint::black_box(next.len()));
        }
        let old_elapsed = old_start.elapsed();

        let new_start = std::time::Instant::now();
        let mut new_total = 0usize;
        let mut ordered_entries = Vec::new();
        let mut next = HashMap::new();
        for _ in 0..iterations {
            ordered_entries.clear();
            next.clear();
            collect_display_entries(&tree, 0.0, 0.0, Some(&mut ordered_entries), None, &mut next);
            new_total = new_total
                .saturating_add(std::hint::black_box(ordered_entries.len()))
                .saturating_add(std::hint::black_box(next.len()));
        }
        let new_elapsed = new_start.elapsed();

        assert_eq!(old_total, new_total);
        println!(
            "display entry collection over {iterations} iterations: fresh allocations {:?}, scratch reuse {:?}",
            old_elapsed, new_elapsed
        );
        assert!(
            new_elapsed < old_elapsed,
            "scratch reuse should be faster than fresh allocations"
        );
    }

    // cargo test -p mesh-core-render --release -- release_entry_collection_skips_debug_ordered_sink --ignored --nocapture
    #[test]
    #[ignore = "release-only display-list debug sink microbenchmark"]
    fn release_entry_collection_skips_debug_ordered_sink() {
        let tree = display_entry_benchmark_tree(120, 20);
        let iterations = 2_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0_usize;
        let mut ordered_entries = Vec::new();
        let mut old_next = HashMap::new();
        for _ in 0..iterations {
            ordered_entries.clear();
            old_next.clear();
            collect_display_entries(
                &tree,
                0.0,
                0.0,
                Some(&mut ordered_entries),
                None,
                &mut old_next,
            );
            old_total = old_total.saturating_add(std::hint::black_box(old_next.len()));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0_usize;
        let mut new_next = HashMap::new();
        for _ in 0..iterations {
            new_next.clear();
            collect_display_entries(&tree, 0.0, 0.0, None, None, &mut new_next);
            new_total = new_total.saturating_add(std::hint::black_box(new_next.len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "display entry debug sink: ordered {old_time:?}; release sink omitted {new_time:?}; ratio {:.2}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-render --release -- retained_effect_count_beats_command_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only retained effect-count microbenchmark"]
    fn retained_effect_count_beats_command_scan() {
        let mut tree = display_entry_benchmark_tree(120, 20);
        for (index, child) in tree.children.iter_mut().enumerate() {
            if index % 8 == 0 {
                child.computed_style.box_shadow.blur_radius = 4.0;
                child.computed_style.box_shadow.color = Color::BLACK;
            }
        }
        let mut list = RetainedDisplayList::default();
        list.update(&tree, 4096, 4096, false, false);
        let retained_count = list
            .subtrees
            .get(&tree.id)
            .expect("retained root subtree")
            .effect_overflow_count;
        assert!(retained_count > 0);
        assert_eq!(
            retained_count,
            count_effect_overflow_commands(list.paint_commands.as_ref())
        );
        let iterations = 20_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0_u64;
        for _ in 0..iterations {
            old_total = old_total.wrapping_add(std::hint::black_box(
                count_effect_overflow_commands(std::hint::black_box(list.paint_commands.as_ref())),
            ));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0_u64;
        for _ in 0..iterations {
            new_total = new_total.wrapping_add(std::hint::black_box(retained_count));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "effect overflow metric: command scan {old_time:?}; retained count {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-render --release -- retained_command_spans_beat_tree_walk --ignored --nocapture
    #[test]
    #[ignore = "release-only retained command-span microbenchmark"]
    fn retained_command_spans_beat_tree_walk() {
        let tree = display_entry_benchmark_tree(120, 20);
        let mut list = RetainedDisplayList::default();
        list.update(&tree, 4096, 4096, false, false);
        let traversed = build_command_spans(&tree, &list.subtrees);
        assert_eq!(list.command_spans.as_ref(), traversed);
        let iterations = 10_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0_usize;
        for _ in 0..iterations {
            old_total = old_total.saturating_add(std::hint::black_box(
                build_command_spans(std::hint::black_box(&tree), &list.subtrees).len(),
            ));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0_usize;
        for _ in 0..iterations {
            let spans = std::hint::black_box(Arc::clone(&list.command_spans));
            new_total = new_total.saturating_add(std::hint::black_box(spans.len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "command span assembly: tree walk {old_time:?}; retained root handle {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-render --release -- single_root_command_span_assembly_beats_ancestor_copying --ignored --nocapture
    #[test]
    #[ignore = "release-only retained command-span assembly microbenchmark"]
    fn single_root_command_span_assembly_beats_ancestor_copying() {
        let tree = display_entry_benchmark_tree(120, 20);
        let mut list = RetainedDisplayList::default();
        list.update(&tree, 4096, 4096, false, false);

        let copied = build_command_spans_with_ancestor_copying(&tree, &list.subtrees);
        let assembled = build_command_spans(&tree, &list.subtrees);
        assert_eq!(copied, assembled);

        let iterations = 1_000;
        let old_started = std::time::Instant::now();
        let mut old_total = 0_usize;
        for _ in 0..iterations {
            old_total = old_total.saturating_add(std::hint::black_box(
                build_command_spans_with_ancestor_copying(
                    std::hint::black_box(&tree),
                    std::hint::black_box(&list.subtrees),
                )
                .len(),
            ));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0_usize;
        for _ in 0..iterations {
            new_total = new_total.saturating_add(std::hint::black_box(
                build_command_spans(
                    std::hint::black_box(&tree),
                    std::hint::black_box(&list.subtrees),
                )
                .len(),
            ));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "command span construction: ancestor-copying {old_time:?}; single-root {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time * 10 < old_time * 9);
    }

    fn frosted_node(id: NodeId, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
        let mut frosted = node(id, "box", x, y, width, height);
        frosted.computed_style.background_color = Color::TRANSPARENT;
        frosted.computed_style.backdrop_filter = VisualFilter { blur_radius: 4.0 };
        frosted
    }

    #[test]
    fn backdrop_regions_require_painted_content_beneath() {
        // Frosted node with nothing painted beneath it (transparent root):
        // its in-surface backdrop is empty, so it must not widen damage.
        let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
        root.computed_style.background_color = Color::TRANSPARENT;
        root.children.push(frosted_node(2, 20.0, 20.0, 40.0, 40.0));
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, true, true);
        assert!(
            list.backdrop_filter_regions().is_empty(),
            "backdrop with empty in-surface backdrop must contribute no region"
        );

        // Opaque content painted beneath the frosted node activates it. The
        // region is the node rect inflated by the 3x blur-kernel pad.
        let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
        root.computed_style.background_color = Color::TRANSPARENT;
        root.children.push(node(2, "box", 0.0, 0.0, 50.0, 100.0));
        root.children.push(frosted_node(3, 20.0, 20.0, 40.0, 40.0));
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, true, true);
        assert_eq!(
            list.backdrop_filter_regions(),
            &[DamageRect {
                x: 8,
                y: 8,
                width: 64,
                height: 64,
            }],
            "active backdrop region should be the node rect plus 12px pad"
        );
    }

    #[test]
    fn expand_damage_for_backdrop_filters_grows_intersecting_rects() {
        let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
        root.computed_style.background_color = Color::TRANSPARENT;
        root.children.push(node(2, "box", 0.0, 0.0, 50.0, 100.0));
        root.children.push(frosted_node(3, 20.0, 20.0, 40.0, 40.0));
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 100, true, true);

        // Damage far from the frosted region stays untouched.
        let mut disjoint = [DamageRect {
            x: 80,
            y: 80,
            width: 10,
            height: 10,
        }];
        assert!(!list.expand_damage_for_backdrop_filters(&mut disjoint));
        assert_eq!(
            disjoint[0],
            DamageRect {
                x: 80,
                y: 80,
                width: 10,
                height: 10,
            }
        );

        // Damage touching the read region grows to cover the whole region so
        // the blur re-reads a consistently repainted backdrop.
        let mut touching = [DamageRect {
            x: 0,
            y: 30,
            width: 10,
            height: 10,
        }];
        assert!(list.expand_damage_for_backdrop_filters(&mut touching));
        assert_eq!(
            touching[0],
            DamageRect {
                x: 0,
                y: 8,
                width: 72,
                height: 64,
            },
            "expanded damage must union the backdrop read region"
        );
    }
}
