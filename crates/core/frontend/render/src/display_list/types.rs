use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mesh_core_elements::style::{
    BackgroundPaint, BlendMode, Color, Corners, Edges, FontStyle, Overflow, TextAlign,
    TextDirection, TextOverflow, WhiteSpace,
};
use mesh_core_elements::{AffineClip, AffineTransform, BoxShadow, VisualFilter};
use mesh_core_elements::{LayoutRect, NodeId};

use crate::fractional_scale::{DeviceRect, FractionalScale};

use super::build::*;
use super::subtree::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayPrimitiveSlot {
    Background,
    Border,
    Text,
    Icon,
    Generic,
}

pub(super) const DISPLAY_PRIMITIVE_SLOTS: [DisplayPrimitiveSlot; 5] = [
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

    pub(super) fn union(self, other: Self) -> Self {
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

/// The immutable inputs, topology, geometry, effects, replay coverage, and
/// damage decisions that describe one retained frame.
///
/// A frame plan is the hand-off between retained-tree construction and the
/// painter/presentation seams. Consumers must use the plan snapshot rather
/// than reconstructing any of these decisions from a partially filtered
/// command stream.
#[derive(Debug, Clone)]
pub struct FramePaintPlan {
    pub inputs: FramePaintInputs,
    pub topology: FramePaintTopology,
    pub transforms: Arc<[FramePaintTransform]>,
    pub effects: FramePaintEffects,
    pub replay: FramePaintReplay,
    pub damage: FramePaintDamage,
}

impl FramePaintPlan {
    pub(crate) fn new(
        inputs: FramePaintInputs,
        topology: FramePaintTopology,
        transforms: Arc<[FramePaintTransform]>,
        effects: FramePaintEffects,
        replay: FramePaintReplay,
        damage: FramePaintDamage,
    ) -> Self {
        Self {
            inputs,
            topology,
            transforms,
            effects,
            replay,
            damage,
        }
    }

    pub(crate) fn empty() -> Self {
        Self::new(
            FramePaintInputs {
                resource_revision: 0,
                retained_tree_generation: None,
                root_id: None,
                surface_size: None,
                paint_origin: (0.0, 0.0),
                backdrop_blur_policy: BackdropBlurPolicy::CompositorRegion,
                nodes: Vec::new().into(),
            },
            FramePaintTopology {
                generation: 0,
                commands: Vec::new().into(),
                kinds: Vec::new().into(),
            },
            Vec::new().into(),
            FramePaintEffects {
                backdrop_regions: Vec::new().into(),
                blur_regions: Vec::new().into(),
                filter_layer_regions: Vec::new().into(),
            },
            FramePaintReplay {
                spans: Vec::new().into(),
                layer_scopes: Vec::new().into(),
            },
            FramePaintDamage {
                logical: Vec::new().into(),
                logical_bounds: DamageRect::default(),
                surface: DamageRect::default(),
                full_surface: false,
                partial_present_supported: false,
            },
        )
    }
}

#[derive(Debug, Clone)]
pub struct FramePaintInputs {
    pub resource_revision: u64,
    pub retained_tree_generation: Option<u64>,
    pub root_id: Option<NodeId>,
    pub surface_size: Option<(u32, u32)>,
    pub paint_origin: (f32, f32),
    pub backdrop_blur_policy: BackdropBlurPolicy,
    /// Immutable node payloads referenced by the command topology.
    pub nodes: Arc<[Arc<DisplayPaintNode>]>,
}

#[derive(Debug, Clone)]
pub struct FramePaintTopology {
    pub generation: u64,
    pub commands: Arc<[DisplayPaintCommand]>,
    pub kinds: Arc<[DisplayPaintCommandKind]>,
}

#[derive(Debug, Clone)]
pub struct FramePaintTransform {
    pub node_id: NodeId,
    pub transform: AffineTransform,
    pub local_layout: LayoutRect,
    pub visual_bounds: LayoutRect,
    pub ancestor_clips: Arc<[AffineClip]>,
}

#[derive(Debug, Clone, Default)]
pub struct FramePaintEffects {
    pub backdrop_regions: Arc<[DamageRect]>,
    pub blur_regions: Arc<[DamageRect]>,
    pub filter_layer_regions: Arc<[DamageRect]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePaintReplaySpan {
    pub owner: NodeId,
    pub start: usize,
    pub end: usize,
    pub bounds: DamageRect,
    pub command_count: usize,
    pub includes_scrollbars: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FramePaintReplay {
    pub spans: Arc<[FramePaintReplaySpan]>,
    pub layer_scopes: Arc<[(usize, usize)]>,
}

#[derive(Debug, Clone)]
pub struct FramePaintDamage {
    pub logical: Arc<[DamageRect]>,
    pub logical_bounds: DamageRect,
    pub surface: DamageRect,
    pub full_surface: bool,
    pub partial_present_supported: bool,
}

impl FramePaintDamage {
    /// Convert every logical damage edge with the shared floor/ceil coverage
    /// contract. This is the authoritative device-space damage conversion.
    pub fn device_rects(&self, scale: FractionalScale) -> Arc<[DeviceRect]> {
        self.logical
            .iter()
            .copied()
            .map(|rect| scale.device_rect(rect))
            .collect::<Vec<_>>()
            .into()
    }

    /// Convert and clip the logical damage to the physical buffer. The
    /// returned rectangles are suitable for SHM clears and Wayland damage.
    pub fn device_damage_for_buffer(
        &self,
        scale: FractionalScale,
        buffer_width: u32,
        buffer_height: u32,
    ) -> Arc<[DamageRect]> {
        self.logical
            .iter()
            .copied()
            .filter_map(|rect| scale.clip_damage_rect(rect, buffer_width, buffer_height))
            .collect::<Vec<_>>()
            .into()
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
pub(super) enum DisplayBatchBarrier {
    Text,
    Icon,
    Opacity,
    Clip,
    Translucency,
    MaterialChange,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DisplayBatchMaterial {
    pub(super) batch_signature: u64,
    pub(super) barrier: Option<DisplayBatchBarrier>,
}

impl DisplayBatchBarrier {
    pub(super) fn record(self, counts: &mut DisplayBatchBarrierCounts) {
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
    pub(super) generation: u64,
    /// Resource/catalog revision used to build the retained commands and
    /// entries. A resource change can alter pixels without changing the tree.
    pub(super) resource_revision: u64,
    /// Policy requested for the next command-stream build.
    pub(super) backdrop_blur_policy: BackdropBlurPolicy,
    /// Policy used by the currently retained command stream.
    pub(super) built_backdrop_blur_policy: BackdropBlurPolicy,
    pub(super) retained_tree_generation: Option<u64>,
    #[cfg(debug_assertions)]
    pub(super) retained_caller_lineage: Option<u64>,
    pub(super) root_id: Option<NodeId>,
    pub(super) surface_size: Option<(u32, u32)>,
    pub(super) paint_origin: (u32, u32),
    pub(super) entries: HashMap<DisplayListKey, DisplayListEntry>,
    pub(super) subtrees: HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    /// Ordered material metadata retained so release metrics describe the
    /// same batch/barrier stream as debug diagnostics without retaining full
    /// display entries or keys.
    pub(super) batch_entries_scratch: Vec<DisplayBatchMaterial>,
    pub(super) next_entries_scratch: HashMap<DisplayListKey, DisplayListEntry>,
    pub(super) next_subtrees_scratch: HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    pub(super) dirty_ancestors_scratch: HashSet<NodeId>,
    pub(super) ancestor_path_scratch: Vec<NodeId>,
    pub(super) command_spans: Arc<[RetainedCommandSpan]>,
    pub(super) paint_commands: Arc<[DisplayPaintCommand]>,
    pub(super) command_kinds: Arc<[DisplayPaintCommandKind]>,
    /// In-surface read regions available to the renderer fallback.
    pub(super) backdrop_regions: Vec<DamageRect>,
    /// Compositor blur regions for `org_kde_kwin_blur`, computed from the full
    /// widget tree (not the scoped `paint_commands` selection). Deriving them
    /// from `paint_commands` would drop the blur nodes on partial retained
    /// updates, yielding an empty region set that flips the compositor to
    /// whole-surface blur.
    pub(super) blur_regions: Vec<DamageRect>,
    /// Extent of every element `filter: blur()` layer in the current list,
    /// inflated by the blur kernel reach. Damage inside one has to grow to
    /// cover it, and the layer's command range is replayed as a whole.
    pub(super) filter_layer_regions: Vec<DamageRect>,
    /// Command ranges `[start, end)` opened by an effect/compositing push and
    /// closed by its pop, in paint order. A selection that touches part of a
    /// range is widened to all of it.
    pub(super) layer_scopes: Vec<(usize, usize)>,
    pub(super) last_metrics: DisplayListMetrics,
    pub(super) last_damage_rects: Vec<DamageRect>,
    pub(super) frame_plan: Arc<FramePaintPlan>,
}

impl Default for RetainedDisplayList {
    fn default() -> Self {
        Self {
            generation: 0,
            resource_revision: mesh_core_resources::resource_revision(),
            backdrop_blur_policy: BackdropBlurPolicy::CompositorRegion,
            built_backdrop_blur_policy: BackdropBlurPolicy::CompositorRegion,
            retained_tree_generation: None,
            #[cfg(debug_assertions)]
            retained_caller_lineage: None,
            root_id: None,
            surface_size: None,
            paint_origin: (0.0_f32.to_bits(), 0.0_f32.to_bits()),
            entries: HashMap::new(),
            subtrees: HashMap::new(),
            batch_entries_scratch: Vec::new(),
            next_entries_scratch: HashMap::new(),
            next_subtrees_scratch: HashMap::new(),
            dirty_ancestors_scratch: HashSet::new(),
            ancestor_path_scratch: Vec::new(),
            command_spans: Vec::new().into(),
            paint_commands: Vec::new().into(),
            command_kinds: Vec::new().into(),
            backdrop_regions: Vec::new(),
            blur_regions: Vec::new(),
            filter_layer_regions: Vec::new(),
            layer_scopes: Vec::new(),
            last_metrics: DisplayListMetrics::default(),
            last_damage_rects: Vec::new(),
            frame_plan: Arc::new(FramePaintPlan::empty()),
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
    /// The authored module that owns this node. A mounted component can be
    /// painted inside a different root surface, so resource-backed content
    /// such as icons must resolve against the node owner rather than the
    /// surface root.
    pub module_id: Option<Arc<str>>,
    /// The cumulative transform from this node's local border-box into the
    /// surface. `layout` remains its transformed AABB for damage/culling.
    pub transform: AffineTransform,
    /// The node's untransformed local layout. Paint content uses this size
    /// while the backend applies `transform` to the whole node.
    pub local_layout: LayoutRect,
    pub layout: LayoutRect,
    /// Ancestor overflow clips in the same surface coordinate system as
    /// `transform`. Their exact affine shapes are retained alongside the
    /// conservative AABB clip used for command selection.
    pub ancestor_clips: Arc<[AffineClip]>,
    pub style: DisplayPaintStyle,
    pub content: DisplayPaintContent,
    pub scrollbars: DisplayScrollbars,
}

impl DisplayPaintNode {
    /// Dimensions used by content layout. Axis-aligned transforms already
    /// have their scaled dimensions in `layout`; rotated nodes must measure
    /// text and controls in their untransformed local box before the backend
    /// applies the affine matrix.
    pub fn paint_width(&self) -> f32 {
        if self.transform.m12.abs() > 0.0001
            || self.transform.m21.abs() > 0.0001
            || self.transform.m11 < -0.0001
            || self.transform.m22 < -0.0001
        {
            self.local_layout.width
        } else {
            self.layout.width
        }
    }

    pub fn paint_height(&self) -> f32 {
        if self.transform.m12.abs() > 0.0001
            || self.transform.m21.abs() > 0.0001
            || self.transform.m11 < -0.0001
            || self.transform.m22 < -0.0001
        {
            self.local_layout.height
        } else {
            self.layout.height
        }
    }
}

#[derive(Debug, Clone)]
pub struct DisplayPaintStyle {
    pub resource_revision: u64,
    pub background_color: Color,
    pub background_paint: BackgroundPaint,
    pub border_color: Color,
    pub border_width: Edges,
    pub border_radius: Corners,
    pub color: Color,
    pub padding: Edges,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub font_family: Arc<str>,
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub letter_spacing: f32,
    pub line_height: f32,
    pub text_align: TextAlign,
    pub text_overflow: TextOverflow,
    pub text_direction: TextDirection,
    pub white_space: WhiteSpace,
    pub language: Arc<str>,
    pub shaping_features: Arc<str>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayInputPreedit {
    pub start: usize,
    pub end: usize,
    pub cursor_begin: usize,
    pub cursor_end: usize,
}

#[derive(Debug, Clone)]
pub struct DisplayInputPaint {
    pub value: Arc<str>,
    pub placeholder: Arc<str>,
    pub mask_text: bool,
    pub focused: bool,
    pub preedit: Option<DisplayInputPreedit>,
}

impl PartialEq for DisplayInputPaint {
    fn eq(&self, other: &Self) -> bool {
        shared_str_eq(&self.value, &other.value)
            && shared_str_eq(&self.placeholder, &other.placeholder)
            && self.mask_text == other.mask_text
            && self.focused == other.focused
            && self.preedit == other.preedit
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

pub(super) fn shared_str_eq(left: &Arc<str>, right: &Arc<str>) -> bool {
    Arc::ptr_eq(left, right) || left.as_ref() == right.as_ref()
}

pub(super) fn optional_shared_str_eq(left: &Option<Arc<str>>, right: &Option<Arc<str>>) -> bool {
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
    /// Opens an isolated compositing group for a node and its descendants.
    /// The node's opacity and blend mode are applied when the group is
    /// composited, rather than being threaded through individual primitives.
    PushCompositingLayer,
    /// Composites the open node group onto its parent.
    PopCompositingLayer,
    /// Opens an offscreen layer that every following command paints into,
    /// until the matching [`DisplayPaintCommandKind::PopFilterLayer`]. Carries
    /// the blurred node, whose `style.filter` is the filter to apply, and a
    /// `clip` already inflated to the subtree's blurred extent.
    PushFilterLayer,
    /// Composites the open filter layer onto its parent.
    PopFilterLayer,
    /// Records that backdrop blur is delegated to the presentation
    /// compositor. This command changes topology without writing SHM pixels.
    ApplyBackdropFilterCompositor,
    /// Applies backdrop blur by reading pixels already painted into the
    /// renderer's surface buffer.
    ApplyBackdropFilterInSurface,
    /// Records an explicit rejected backdrop request. The node is painted
    /// without blur and the renderer emits a diagnostic at the lowering seam.
    ApplyBackdropFilterRejected,
}

impl DisplayPaintCommandKind {
    /// Whether this command draws content rather than managing layer scope.
    pub fn draws_content(self) -> bool {
        matches!(self, Self::Node | Self::Scrollbars)
    }

    /// Whether this command opens or closes a retained layer scope.
    pub fn is_layer_scope(self) -> bool {
        matches!(
            self,
            Self::PushCompositingLayer
                | Self::PopCompositingLayer
                | Self::PushFilterLayer
                | Self::PopFilterLayer
        )
    }

    /// Whether this command represents a selected backdrop-blur policy.
    pub fn is_backdrop_filter(self) -> bool {
        matches!(
            self,
            Self::ApplyBackdropFilterCompositor
                | Self::ApplyBackdropFilterInSurface
                | Self::ApplyBackdropFilterRejected
        )
    }
}

/// Where a `backdrop-filter` request is realized for one retained display
/// list. The policy is part of command topology so changing presentation or
/// renderer support cannot reuse a command stream lowered for another mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackdropBlurPolicy {
    /// Export the region to the compositor and leave client pixels unchanged.
    CompositorRegion,
    /// Read and filter pixels already painted into this surface buffer.
    InSurfaceFilter,
    /// Paint the node normally and retain an observable diagnostic.
    Rejected,
}

impl BackdropBlurPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompositorRegion => "compositor_region",
            Self::InSurfaceFilter => "in_surface_filter",
            Self::Rejected => "rejected",
        }
    }
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
    pub(super) commands: &'a [DisplayPaintCommand],
    pub(super) kinds: &'a [DisplayPaintCommandKind],
    pub(super) selection: SelectedDisplayListSelection,
    pub(super) metrics: DisplayListMetrics,
}

#[derive(Debug, Clone)]
pub(super) enum SelectedDisplayListSelection {
    All,
    None,
    Spans {
        spans: Vec<SelectedCommandSpan>,
        command_count: usize,
    },
}

pub struct SelectedDisplayListPaintIter<'a> {
    pub(super) commands: &'a [DisplayPaintCommand],
    pub(super) state: SelectedDisplayListPaintIterState<'a>,
}

pub struct SelectedDisplayListPaintKindIter<'a> {
    pub(super) commands: &'a [DisplayPaintCommand],
    pub(super) kinds: &'a [DisplayPaintCommandKind],
    pub(super) state: SelectedDisplayListPaintKindIterState<'a>,
}

pub(super) enum SelectedDisplayListPaintIterState<'a> {
    All(std::slice::Iter<'a, DisplayPaintCommand>),
    None,
    Spans {
        spans: &'a [SelectedCommandSpan],
        span_index: usize,
        command_index: usize,
    },
}

pub(super) enum SelectedDisplayListPaintKindIterState<'a> {
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
