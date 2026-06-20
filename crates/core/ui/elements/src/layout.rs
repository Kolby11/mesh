/// Flexbox-subset layout engine.
///
/// Computes `LayoutRect` for every node in a widget tree. Supports row/column
/// direction, flex-grow/shrink, gap, padding, and margin.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::lru::LruCache;
use crate::style::{
    AlignItems, AlignSelf, Dimension, Display, Edges, FlexDirection, JustifyContent, Overflow,
    Position, TextDirection,
};
use crate::tree::{NodeId, WidgetNode};
use taffy::TaffyTree;
use taffy::geometry::{Point as TaffyPoint, Rect as TaffyRect, Size as TaffySize};
use taffy::prelude::{AvailableSpace as TaffyAvailableSpace, NodeId as TaffyNodeId};
use taffy::style as taffy_style;

/// Trait for measuring text dimensions. Implemented outside `mesh-core-elements` (in the
/// shell render stack) and injected so the layout engine can shrink-wrap text
/// nodes without taking a direct dependency on the renderer.
pub trait TextMeasurer {
    /// Return `(width, height)` in logical pixels for the given text and style.
    /// `max_width: None` means unconstrained (natural single-line width).
    fn measure_text(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32);
}

/// Computed layout rectangle for a node.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaffyLayoutDiagnostic {
    pub node_id: NodeId,
    pub tag: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct TaffyLayoutReport {
    pub diagnostics: Vec<TaffyLayoutDiagnostic>,
}

const CONTENT_DIMENSION_TAFFY_DIAGNOSTIC: &str =
    "content dimension mapped through Taffy measurement";

impl TaffyLayoutReport {
    pub fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

fn record_taffy_diagnostic(
    report: &mut TaffyLayoutReport,
    node: &WidgetNode,
    reason: impl Into<String>,
) {
    report.diagnostics.push(TaffyLayoutDiagnostic {
        node_id: node.id,
        tag: node.tag.clone(),
        reason: reason.into(),
    });
}

const INTRINSIC_TEXT_CACHE_CAPACITY: usize = 512;

#[derive(Debug)]
pub struct IntrinsicLayoutCache {
    text_measurements: LruCache<TextMeasureKey, (f32, f32)>,
}

impl Default for IntrinsicLayoutCache {
    fn default() -> Self {
        Self {
            text_measurements: LruCache::new(INTRINSIC_TEXT_CACHE_CAPACITY),
        }
    }
}

impl IntrinsicLayoutCache {
    fn get_text_measurement(&mut self, key: &TextMeasureKey) -> Option<(f32, f32)> {
        self.text_measurements.get(key).copied()
    }

    fn insert_text_measurement(&mut self, key: TextMeasureKey, value: (f32, f32)) {
        self.text_measurements.insert(key, value);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextMeasureKey {
    content: String,
    font_family: Arc<str>,
    font_size: u32,
    font_weight: u16,
    line_height: u32,
    max_width: Option<u32>,
}

impl TextMeasureKey {
    fn new(text: &TextMeasureData, max_width: Option<f32>) -> Self {
        Self {
            content: text.content.clone(),
            font_family: text.font_family.clone(),
            font_size: text.font_size.to_bits(),
            font_weight: text.font_weight,
            line_height: text.line_height.to_bits(),
            max_width: max_width.map(f32::to_bits),
        }
    }
}

/// The layout engine. Stateless — call `compute` on a widget tree.
pub struct LayoutEngine;

/// Retained layout state for a single surface, holding a persistent
/// [`TaffyTree`] and the `_mesh_key → TaffyNodeId` identity map so that
/// layout geometry is mutated in place across frames instead of rebuilt
/// from scratch.
pub struct PerSurfaceLayoutState {
    /// The retained Taffy layout tree, mutated incrementally.
    pub tree: TaffyTree<NodeId>,
    /// Maps stable `_mesh_key` attribute values (e.g. `"root/0/button"`)
    /// to their corresponding `TaffyNodeId` in the retained tree.
    /// Keyed by `String` (NOT ephemeral `NodeId`) per LAYOUT-03.
    pub node_map: HashMap<String, TaffyNodeId>,
    /// `(width, height)` used in the last `compute_layout` call.
    pub last_available: (f32, f32),
    /// `false` after theme/locale/source-reload resets; forces a
    /// full fresh-build on the next pass, which then sets `valid = true`.
    pub valid: bool,
}

// SAFETY: `PerSurfaceLayoutState` is an owned per-surface cache. The shell may
// move a `FrontendSurfaceComponent` between threads because `ShellComponent`
// requires `Send`, but layout mutation happens only through `&mut self`; the
// retained `TaffyTree` is never shared concurrently.
unsafe impl Send for PerSurfaceLayoutState {}

impl PerSurfaceLayoutState {
    /// Construct a fresh, invalid state (equivalent to `Default`).
    pub fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
            node_map: HashMap::new(),
            last_available: (0.0, 0.0),
            valid: false,
        }
    }
}

impl Default for PerSurfaceLayoutState {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    /// Compute layout for the entire tree within the given bounds.
    pub fn compute(root: &mut WidgetNode, available_width: f32, available_height: f32) {
        let mut cache = IntrinsicLayoutCache::default();
        Self::compute_with_intrinsic_cache_and_measurer(
            root,
            available_width,
            available_height,
            &mut cache,
            None,
        );
    }

    /// Like `compute` but with an optional text measurer for accurate shrink-wrapping.
    pub fn compute_with_measurer(
        root: &mut WidgetNode,
        available_width: f32,
        available_height: f32,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        let mut cache = IntrinsicLayoutCache::default();
        Self::compute_with_intrinsic_cache_and_measurer(
            root,
            available_width,
            available_height,
            &mut cache,
            measurer,
        );
    }

    /// Reuses retained intrinsic probe results across layout passes.
    pub fn compute_with_intrinsic_cache_and_measurer(
        root: &mut WidgetNode,
        available_width: f32,
        available_height: f32,
        intrinsic_cache: &mut IntrinsicLayoutCache,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        Self::compute_taffy_layout_with_cache(
            root,
            available_width,
            available_height,
            intrinsic_cache,
            measurer,
        );
    }

    /// Compute layout through Taffy and write the resulting rectangles back onto
    /// the stable MESH `WidgetNode` tree.
    pub fn compute_taffy_layout(
        root: &mut WidgetNode,
        available_width: f32,
        available_height: f32,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        let mut intrinsic_cache = IntrinsicLayoutCache::default();
        Self::compute_taffy_layout_with_cache(
            root,
            available_width,
            available_height,
            &mut intrinsic_cache,
            measurer,
        );
    }

    fn compute_taffy_layout_with_cache(
        root: &mut WidgetNode,
        available_width: f32,
        available_height: f32,
        intrinsic_cache: &mut IntrinsicLayoutCache,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        let mut report = TaffyLayoutReport::default();
        let mut tree = TaffyTree::<NodeId>::new();
        let mut node_map = HashMap::new();
        let mut text_nodes = HashMap::new();

        match build_taffy_tree(root, &mut tree, &mut node_map, &mut text_nodes, &mut report) {
            Ok(root_id) => {
                let available_space = TaffySize {
                    width: TaffyAvailableSpace::Definite(available_width),
                    height: TaffyAvailableSpace::Definite(available_height),
                };

                if let Err(error) = tree.compute_layout_with_measure(
                    root_id,
                    available_space,
                    |known_dimensions, available_space, _node_id, context, _style| {
                        measure_taffy_node(
                            known_dimensions,
                            available_space,
                            context.map(|node_id| *node_id),
                            &text_nodes,
                            intrinsic_cache,
                            measurer,
                        )
                    },
                ) {
                    tracing::warn!(
                        target: "mesh::layout",
                        error = %error,
                        "taffy layout computation failed"
                    );
                    zero_layout_subtree(root);
                } else {
                    write_taffy_layout(root, &tree, &node_map, available_width, available_height);
                }
            }
            Err(error) => {
                tracing::warn!(
                    target: "mesh::layout",
                    error = %error,
                    "taffy layout tree construction failed"
                );
                zero_layout_subtree(root);
            }
        }

        for diagnostic in &report.diagnostics {
            if is_expected_taffy_measurement_diagnostic(&diagnostic.reason) {
                tracing::debug!(
                    target: "mesh::layout",
                    node_id = diagnostic.node_id,
                    tag = %diagnostic.tag,
                    reason = %diagnostic.reason,
                    "taffy layout diagnostic"
                );
            } else {
                tracing::warn!(
                    target: "mesh::layout",
                    node_id = diagnostic.node_id,
                    tag = %diagnostic.tag,
                    reason = %diagnostic.reason,
                    "taffy layout diagnostic"
                );
            }
        }
    }

    /// Compute layout by mutating a retained per-surface Taffy tree.
    pub fn compute_incremental(
        root: &mut WidgetNode,
        state: &mut PerSurfaceLayoutState,
        available_width: f32,
        available_height: f32,
        dirty_layout: bool,
        dirty_structural: bool,
        intrinsic_cache: &mut IntrinsicLayoutCache,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        if !state.valid {
            compute_fresh_retained_layout(
                root,
                state,
                available_width,
                available_height,
                intrinsic_cache,
                measurer,
            );
            return;
        }

        if dirty_structural {
            compute_structural_retained_layout(
                root,
                state,
                available_width,
                available_height,
                intrinsic_cache,
                measurer,
            );
            return;
        }

        let Some(root_id) = retained_taffy_id(root, state) else {
            state.valid = false;
            compute_fresh_retained_layout(
                root,
                state,
                available_width,
                available_height,
                intrinsic_cache,
                measurer,
            );
            return;
        };

        let mut report = TaffyLayoutReport::default();
        let mut node_map = HashMap::new();
        let mut text_nodes = HashMap::new();
        update_retained_node_styles(
            root,
            state,
            dirty_layout,
            &mut node_map,
            &mut text_nodes,
            &mut report,
        );

        let available_changed = state.last_available != (available_width, available_height);
        if available_changed || dirty_layout {
            let available_space = taffy_available_space(available_width, available_height);
            if let Err(error) = state.tree.compute_layout_with_measure(
                root_id,
                available_space,
                |known_dimensions, available_space, _node_id, context, _style| {
                    measure_taffy_node(
                        known_dimensions,
                        available_space,
                        context.map(|node_id| *node_id),
                        &text_nodes,
                        intrinsic_cache,
                        measurer,
                    )
                },
            ) {
                tracing::warn!(
                    target: "mesh::layout",
                    error = %error,
                    "retained taffy layout computation failed"
                );
                zero_layout_subtree(root);
            } else {
                write_taffy_layout(root, &state.tree, &node_map, available_width, available_height);
                state.last_available = (available_width, available_height);
            }
        }

        log_taffy_report(&report);
    }
}

#[derive(Clone)]
struct TextMeasureData {
    content: String,
    font_family: Arc<str>,
    font_size: f32,
    font_weight: u16,
    line_height: f32,
    nowrap: bool,
}

fn taffy_dimension(dimension: Dimension) -> taffy_style::Dimension {
    match dimension {
        Dimension::Auto => taffy_style::Dimension::auto(),
        Dimension::Px(value) => taffy_style::Dimension::length(value),
        Dimension::Percent(value) => taffy_style::Dimension::percent(value / 100.0),
        Dimension::Content => taffy_style::Dimension::auto(),
    }
}

fn taffy_length_percentage(value: Option<f32>) -> taffy_style::LengthPercentageAuto {
    value
        .map(taffy_style::LengthPercentageAuto::length)
        .unwrap_or_else(taffy_style::LengthPercentageAuto::auto)
}

fn taffy_length(value: f32) -> taffy_style::LengthPercentage {
    taffy_style::LengthPercentage::length(value)
}

fn taffy_style_for_node(node: &WidgetNode, report: &mut TaffyLayoutReport) -> taffy_style::Style {
    let style = &node.computed_style;

    if matches!(style.width, Dimension::Content) || matches!(style.height, Dimension::Content) {
        record_taffy_diagnostic(report, node, CONTENT_DIMENSION_TAFFY_DIAGNOSTIC);
    }

    let mut taffy = taffy_style::Style {
        display: match style.display {
            Display::Flex => taffy_style::Display::Flex,
            Display::None => taffy_style::Display::None,
        },
        direction: match style.text_direction {
            TextDirection::Ltr => taffy_style::Direction::Ltr,
            TextDirection::Rtl => taffy_style::Direction::Rtl,
        },
        overflow: TaffyPoint {
            x: match style.overflow_x {
                Overflow::Visible => taffy_style::Overflow::Visible,
                Overflow::Hidden | Overflow::Auto => taffy_style::Overflow::Hidden,
                Overflow::Scroll => taffy_style::Overflow::Scroll,
            },
            y: match style.overflow_y {
                Overflow::Visible => taffy_style::Overflow::Visible,
                Overflow::Hidden | Overflow::Auto => taffy_style::Overflow::Hidden,
                Overflow::Scroll => taffy_style::Overflow::Scroll,
            },
        },
        position: match style.position {
            Position::Static | Position::Relative => taffy_style::Position::Relative,
            Position::Absolute | Position::Fixed => taffy_style::Position::Absolute,
        },
        inset: TaffyRect {
            left: taffy_length_percentage(style.inset_left),
            right: taffy_length_percentage(style.inset_right),
            top: taffy_length_percentage(style.inset_top),
            bottom: taffy_length_percentage(style.inset_bottom),
        },
        size: TaffySize {
            width: taffy_dimension(style.width),
            height: taffy_dimension(style.height),
        },
        min_size: TaffySize {
            width: style
                .min_width
                .map(taffy_style::Dimension::length)
                .unwrap_or_else(taffy_style::Dimension::auto),
            height: style
                .min_height
                .map(taffy_style::Dimension::length)
                .unwrap_or_else(taffy_style::Dimension::auto),
        },
        max_size: TaffySize {
            width: style
                .max_width
                .map(taffy_style::Dimension::length)
                .unwrap_or_else(taffy_style::Dimension::auto),
            height: style
                .max_height
                .map(taffy_style::Dimension::length)
                .unwrap_or_else(taffy_style::Dimension::auto),
        },
        margin: TaffyRect {
            left: taffy_style::LengthPercentageAuto::length(style.margin.left),
            right: taffy_style::LengthPercentageAuto::length(style.margin.right),
            top: taffy_style::LengthPercentageAuto::length(style.margin.top),
            bottom: taffy_style::LengthPercentageAuto::length(style.margin.bottom),
        },
        padding: TaffyRect {
            left: taffy_length(style.padding.left),
            right: taffy_length(style.padding.right),
            top: taffy_length(style.padding.top),
            bottom: taffy_length(style.padding.bottom),
        },
        border: TaffyRect {
            left: taffy_length(style.border_width.left),
            right: taffy_length(style.border_width.right),
            top: taffy_length(style.border_width.top),
            bottom: taffy_length(style.border_width.bottom),
        },
        align_items: Some(match style.align_items {
            AlignItems::Start => taffy_style::AlignItems::FlexStart,
            AlignItems::End => taffy_style::AlignItems::FlexEnd,
            AlignItems::Center => taffy_style::AlignItems::Center,
            AlignItems::Stretch => taffy_style::AlignItems::Stretch,
        }),
        align_self: match style.align_self {
            AlignSelf::Auto => None,
            AlignSelf::Start => Some(taffy_style::AlignSelf::FlexStart),
            AlignSelf::End => Some(taffy_style::AlignSelf::FlexEnd),
            AlignSelf::Center => Some(taffy_style::AlignSelf::Center),
            AlignSelf::Stretch => Some(taffy_style::AlignSelf::Stretch),
            AlignSelf::Baseline => Some(taffy_style::AlignSelf::Baseline),
        },
        justify_content: Some(match style.justify_content {
            JustifyContent::Start => taffy_style::JustifyContent::FlexStart,
            JustifyContent::End => taffy_style::JustifyContent::FlexEnd,
            JustifyContent::Center => taffy_style::JustifyContent::Center,
            JustifyContent::SpaceBetween => taffy_style::JustifyContent::SpaceBetween,
            JustifyContent::SpaceAround => taffy_style::JustifyContent::SpaceAround,
        }),
        gap: TaffySize {
            width: taffy_length(style.gap),
            height: taffy_length(style.gap),
        },
        flex_direction: match style.direction {
            FlexDirection::Row => taffy_style::FlexDirection::Row,
            FlexDirection::Column => taffy_style::FlexDirection::Column,
        },
        flex_basis: taffy_dimension(style.flex_basis),
        flex_grow: style.flex_grow.max(0.0),
        flex_shrink: style.flex_shrink.max(0.0),
        ..Default::default()
    };

    taffy.flex_wrap = match style.flex_wrap {
        crate::style::FlexWrap::NoWrap => taffy_style::FlexWrap::NoWrap,
        crate::style::FlexWrap::Wrap => taffy_style::FlexWrap::Wrap,
        crate::style::FlexWrap::WrapReverse => taffy_style::FlexWrap::WrapReverse,
    };
    taffy
}

fn is_expected_taffy_measurement_diagnostic(reason: &str) -> bool {
    reason == CONTENT_DIMENSION_TAFFY_DIAGNOSTIC
}

fn build_taffy_tree(
    node: &WidgetNode,
    tree: &mut TaffyTree<NodeId>,
    node_map: &mut HashMap<NodeId, TaffyNodeId>,
    text_nodes: &mut HashMap<NodeId, TextMeasureData>,
    report: &mut TaffyLayoutReport,
) -> Result<TaffyNodeId, taffy::TaffyError> {
    let style = taffy_style_for_node(node, report);
    let taffy_node = if node.children.is_empty() {
        if node.tag == "text" {
            text_nodes.insert(
                node.id,
                TextMeasureData {
                    content: node
                        .attributes
                        .get("content")
                        .cloned()
                        .unwrap_or_else(String::new),
                    font_family: node.computed_style.font_family.clone(),
                    font_size: node.computed_style.font_size,
                    font_weight: node.computed_style.font_weight,
                    line_height: node.computed_style.line_height,
                    nowrap: node.computed_style.white_space == crate::WhiteSpace::Nowrap,
                },
            );
        }
        tree.new_leaf_with_context(style, node.id)?
    } else {
        let children = node
            .children
            .iter()
            .map(|child| build_taffy_tree(child, tree, node_map, text_nodes, report))
            .collect::<Result<Vec<_>, _>>()?;
        tree.new_with_children(style, &children)?
    };

    node_map.insert(node.id, taffy_node);
    Ok(taffy_node)
}

fn compute_fresh_retained_layout(
    root: &mut WidgetNode,
    state: &mut PerSurfaceLayoutState,
    available_width: f32,
    available_height: f32,
    intrinsic_cache: &mut IntrinsicLayoutCache,
    measurer: Option<&dyn TextMeasurer>,
) {
    let mut report = TaffyLayoutReport::default();
    let mut node_id_to_taffy = HashMap::new();
    let mut text_nodes = HashMap::new();

    state.tree = TaffyTree::<NodeId>::new();
    state.node_map.clear();

    match build_taffy_tree(
        root,
        &mut state.tree,
        &mut node_id_to_taffy,
        &mut text_nodes,
        &mut report,
    ) {
        Ok(root_id) => {
            collect_stable_taffy_map(root, &node_id_to_taffy, &mut state.node_map);
            let available_space = taffy_available_space(available_width, available_height);
            if let Err(error) = state.tree.compute_layout_with_measure(
                root_id,
                available_space,
                |known_dimensions, available_space, _node_id, context, _style| {
                    measure_taffy_node(
                        known_dimensions,
                        available_space,
                        context.map(|node_id| *node_id),
                        &text_nodes,
                        intrinsic_cache,
                        measurer,
                    )
                },
            ) {
                tracing::warn!(
                    target: "mesh::layout",
                    error = %error,
                    "retained taffy fresh layout computation failed"
                );
                zero_layout_subtree(root);
                state.valid = false;
            } else {
                write_taffy_layout(root, &state.tree, &node_id_to_taffy, available_width, available_height);
                state.last_available = (available_width, available_height);
                state.valid = true;
            }
        }
        Err(error) => {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "retained taffy tree construction failed"
            );
            zero_layout_subtree(root);
            state.valid = false;
        }
    }

    log_taffy_report(&report);
}

fn compute_structural_retained_layout(
    root: &mut WidgetNode,
    state: &mut PerSurfaceLayoutState,
    available_width: f32,
    available_height: f32,
    intrinsic_cache: &mut IntrinsicLayoutCache,
    measurer: Option<&dyn TextMeasurer>,
) {
    let mut report = TaffyLayoutReport::default();
    let mut node_id_to_taffy = HashMap::new();
    let mut text_nodes = HashMap::new();
    let mut present_keys = HashSet::new();
    collect_mesh_keys(root, &mut present_keys);

    match reconcile_retained_taffy_node(
        root,
        state,
        &mut node_id_to_taffy,
        &mut text_nodes,
        &mut report,
    ) {
        Ok(root_id) => {
            let mut stale_keys = state
                .node_map
                .keys()
                .filter(|key| !present_keys.contains(*key))
                .cloned()
                .collect::<Vec<_>>();
            stale_keys.sort_by_key(|key| std::cmp::Reverse(key.len()));
            for key in stale_keys {
                if let Some(taffy_id) = state.node_map.remove(&key) {
                    if let Err(error) = remove_taffy_subtree(&mut state.tree, taffy_id) {
                        tracing::warn!(
                            target: "mesh::layout",
                            key = %key,
                            error = %error,
                            "failed to remove stale retained layout subtree"
                        );
                    }
                }
            }

            let available_space = taffy_available_space(available_width, available_height);
            if let Err(error) = state.tree.compute_layout_with_measure(
                root_id,
                available_space,
                |known_dimensions, available_space, _node_id, context, _style| {
                    measure_taffy_node(
                        known_dimensions,
                        available_space,
                        context.map(|node_id| *node_id),
                        &text_nodes,
                        intrinsic_cache,
                        measurer,
                    )
                },
            ) {
                tracing::warn!(
                    target: "mesh::layout",
                    error = %error,
                    "retained taffy structural layout computation failed"
                );
                zero_layout_subtree(root);
                state.valid = false;
            } else {
                write_taffy_layout(root, &state.tree, &node_id_to_taffy, available_width, available_height);
                state.last_available = (available_width, available_height);
                state.valid = true;
            }
        }
        Err(error) => {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "retained taffy structural reconciliation failed"
            );
            zero_layout_subtree(root);
            state.valid = false;
        }
    }

    log_taffy_report(&report);
}

fn reconcile_retained_taffy_node(
    node: &WidgetNode,
    state: &mut PerSurfaceLayoutState,
    node_id_to_taffy: &mut HashMap<NodeId, TaffyNodeId>,
    text_nodes: &mut HashMap<NodeId, TextMeasureData>,
    report: &mut TaffyLayoutReport,
) -> Result<TaffyNodeId, taffy::TaffyError> {
    let style = taffy_style_for_node(node, report);
    let key = node.attributes.get("_mesh_key").cloned();
    let taffy_id = if let Some(key) = key {
        if let Some(existing) = state.node_map.get(&key).copied() {
            state.tree.set_style(existing, style)?;
            existing
        } else {
            let created = state.tree.new_leaf(style)?;
            state.node_map.insert(key, created);
            created
        }
    } else {
        // Unkeyed nodes cannot be retained safely across TREE_REBUILD passes:
        // there is no stable identity to reconcile against (RESEARCH.md Pitfall 3).
        state.tree.new_leaf(style)?
    };

    update_text_context(node, &mut state.tree, taffy_id, text_nodes)?;
    node_id_to_taffy.insert(node.id, taffy_id);

    let child_ids = node
        .children
        .iter()
        .map(|child| {
            reconcile_retained_taffy_node(child, state, node_id_to_taffy, text_nodes, report)
        })
        .collect::<Result<Vec<_>, _>>()?;
    state.tree.set_children(taffy_id, &child_ids)?;
    Ok(taffy_id)
}

fn update_retained_node_styles(
    node: &WidgetNode,
    state: &mut PerSurfaceLayoutState,
    mark_dirty: bool,
    node_id_to_taffy: &mut HashMap<NodeId, TaffyNodeId>,
    text_nodes: &mut HashMap<NodeId, TextMeasureData>,
    report: &mut TaffyLayoutReport,
) {
    if let Some(taffy_id) = retained_taffy_id(node, state) {
        let style = taffy_style_for_node(node, report);
        if let Err(error) = state.tree.set_style(taffy_id, style) {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "failed to update retained taffy style"
            );
        }
        if mark_dirty && let Err(error) = state.tree.mark_dirty(taffy_id) {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "failed to mark retained taffy node dirty"
            );
        }
        if let Err(error) = update_text_context(node, &mut state.tree, taffy_id, text_nodes) {
            tracing::warn!(
                target: "mesh::layout",
                error = %error,
                "failed to update retained taffy text context"
            );
        }
        node_id_to_taffy.insert(node.id, taffy_id);
    }

    for child in &node.children {
        update_retained_node_styles(
            child,
            state,
            mark_dirty,
            node_id_to_taffy,
            text_nodes,
            report,
        );
    }
}

fn update_text_context(
    node: &WidgetNode,
    tree: &mut TaffyTree<NodeId>,
    taffy_id: TaffyNodeId,
    text_nodes: &mut HashMap<NodeId, TextMeasureData>,
) -> Result<(), taffy::TaffyError> {
    if node.tag == "text" {
        text_nodes.insert(
            node.id,
            TextMeasureData {
                content: node
                    .attributes
                    .get("content")
                    .cloned()
                    .unwrap_or_else(String::new),
                font_family: node.computed_style.font_family.clone(),
                font_size: node.computed_style.font_size,
                font_weight: node.computed_style.font_weight,
                line_height: node.computed_style.line_height,
                nowrap: node.computed_style.white_space == crate::WhiteSpace::Nowrap,
            },
        );
        tree.set_node_context(taffy_id, Some(node.id))?;
    } else {
        tree.set_node_context(taffy_id, None)?;
    }
    Ok(())
}

fn collect_stable_taffy_map(
    node: &WidgetNode,
    node_id_to_taffy: &HashMap<NodeId, TaffyNodeId>,
    stable_map: &mut HashMap<String, TaffyNodeId>,
) {
    if let Some(key) = node.attributes.get("_mesh_key")
        && let Some(taffy_id) = node_id_to_taffy.get(&node.id)
    {
        stable_map.insert(key.clone(), *taffy_id);
    }
    for child in &node.children {
        collect_stable_taffy_map(child, node_id_to_taffy, stable_map);
    }
}

fn collect_mesh_keys(node: &WidgetNode, keys: &mut HashSet<String>) {
    if let Some(key) = node.attributes.get("_mesh_key") {
        keys.insert(key.clone());
    }
    for child in &node.children {
        collect_mesh_keys(child, keys);
    }
}

fn retained_taffy_id(node: &WidgetNode, state: &PerSurfaceLayoutState) -> Option<TaffyNodeId> {
    node.attributes
        .get("_mesh_key")
        .and_then(|key| state.node_map.get(key))
        .copied()
}

fn taffy_available_space(width: f32, height: f32) -> TaffySize<TaffyAvailableSpace> {
    TaffySize {
        width: TaffyAvailableSpace::Definite(width),
        height: TaffyAvailableSpace::Definite(height),
    }
}

fn log_taffy_report(report: &TaffyLayoutReport) {
    for diagnostic in &report.diagnostics {
        if is_expected_taffy_measurement_diagnostic(&diagnostic.reason) {
            tracing::debug!(
                target: "mesh::layout",
                node_id = diagnostic.node_id,
                tag = %diagnostic.tag,
                reason = %diagnostic.reason,
                "taffy layout diagnostic"
            );
        } else {
            tracing::warn!(
                target: "mesh::layout",
                node_id = diagnostic.node_id,
                tag = %diagnostic.tag,
                reason = %diagnostic.reason,
                "taffy layout diagnostic"
            );
        }
    }
}

fn measure_taffy_node(
    known_dimensions: TaffySize<Option<f32>>,
    available_space: TaffySize<TaffyAvailableSpace>,
    node_id: Option<NodeId>,
    text_nodes: &HashMap<NodeId, TextMeasureData>,
    intrinsic_cache: &mut IntrinsicLayoutCache,
    measurer: Option<&dyn TextMeasurer>,
) -> TaffySize<f32> {
    let Some(node_id) = node_id else {
        return TaffySize::ZERO;
    };
    let Some(text) = text_nodes.get(&node_id) else {
        return TaffySize::ZERO;
    };
    let Some(measurer) = measurer else {
        return TaffySize {
            width: known_dimensions.width.unwrap_or(0.0),
            height: known_dimensions.height.unwrap_or(0.0),
        };
    };

    let max_width = if text.nowrap {
        known_dimensions.width
    } else {
        known_dimensions.width
            .or_else(|| available_space_to_option(available_space.width))
    };
    let measure_key = TextMeasureKey::new(text, max_width);
    let (measured_width, measured_height) =
        if let Some(measured) = intrinsic_cache.get_text_measurement(&measure_key) {
            measured
        } else {
            let measured = measurer.measure_text(
                &text.content,
                &text.font_family,
                text.font_size,
                text.font_weight,
                text.line_height,
                max_width,
            );
            intrinsic_cache.insert_text_measurement(measure_key, measured);
            measured
        };

    TaffySize {
        width: known_dimensions.width.unwrap_or(measured_width),
        height: known_dimensions.height.unwrap_or(measured_height),
    }
}

fn available_space_to_option(value: TaffyAvailableSpace) -> Option<f32> {
    match value {
        TaffyAvailableSpace::Definite(value) => Some(value),
        TaffyAvailableSpace::MinContent | TaffyAvailableSpace::MaxContent => None,
    }
}

fn write_taffy_layout(
    node: &mut WidgetNode,
    tree: &TaffyTree<NodeId>,
    node_map: &HashMap<NodeId, TaffyNodeId>,
    viewport_w: f32,
    viewport_h: f32,
) {
    write_taffy_layout_with_parent(node, tree, node_map, None, 0.0, 0.0, viewport_w, viewport_h);
}

fn write_taffy_layout_with_parent(
    node: &mut WidgetNode,
    tree: &TaffyTree<NodeId>,
    node_map: &HashMap<NodeId, TaffyNodeId>,
    parent_padding: Option<Edges>,
    parent_x: f32,
    parent_y: f32,
    viewport_w: f32,
    viewport_h: f32,
) {
    if node.computed_style.display == Display::None {
        zero_layout_subtree(node);
        return;
    }

    if let Some(taffy_node) = node_map.get(&node.id)
        && let Ok(layout) = tree.layout(*taffy_node)
    {
        node.layout = LayoutRect {
            x: parent_x + layout.location.x,
            y: parent_y + layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        };

        if node.computed_style.position == Position::Absolute
            && let Some(padding) = parent_padding
        {
            if node.computed_style.inset_left.is_some() {
                node.layout.x += padding.left;
            }
            if node.computed_style.inset_top.is_some() {
                node.layout.y += padding.top;
            }
            if node.computed_style.inset_left.is_some() && node.computed_style.inset_right.is_some()
            {
                node.layout.width = (node.layout.width - padding.horizontal()).max(0.0);
            }
            if node.computed_style.inset_top.is_some() && node.computed_style.inset_bottom.is_some()
            {
                node.layout.height = (node.layout.height - padding.vertical()).max(0.0);
            }
        }

        // Fixed: positioned relative to the viewport, ignoring scroll and transforms.
        // Override x/y (and size when both edges constrain it) using viewport dimensions.
        if node.computed_style.position == Position::Fixed {
            let w = node.layout.width;
            let h = node.layout.height;
            let s = &node.computed_style;
            match (s.inset_left, s.inset_right) {
                (Some(l), Some(r)) => {
                    node.layout.x = l;
                    node.layout.width = (viewport_w - l - r).max(0.0);
                }
                (Some(l), None) => node.layout.x = l,
                (None, Some(r)) => node.layout.x = (viewport_w - w - r).max(0.0),
                (None, None) => node.layout.x = 0.0,
            }
            match (s.inset_top, s.inset_bottom) {
                (Some(t), Some(b)) => {
                    node.layout.y = t;
                    node.layout.height = (viewport_h - t - b).max(0.0);
                }
                (Some(t), None) => node.layout.y = t,
                (None, Some(b)) => node.layout.y = (viewport_h - h - b).max(0.0),
                (None, None) => node.layout.y = 0.0,
            }
        }
    }

    let padding = node.computed_style.padding;
    for child in &mut node.children {
        // Fixed children are positioned from the viewport origin, not the parent.
        let (child_parent_x, child_parent_y, child_parent_padding) =
            if child.computed_style.position == Position::Fixed {
                (0.0, 0.0, None)
            } else {
                (node.layout.x, node.layout.y, Some(padding))
            };
        write_taffy_layout_with_parent(
            child,
            tree,
            node_map,
            child_parent_padding,
            child_parent_x,
            child_parent_y,
            viewport_w,
            viewport_h,
        );
    }
}

fn zero_layout_subtree(node: &mut WidgetNode) {
    node.layout = LayoutRect::default();
    for child in &mut node.children {
        zero_layout_subtree(child);
    }
}

/// Remove a Taffy node and all its descendants, post-order.
///
/// [`TaffyTree::remove`] only detaches the parent and orphans its
/// children — it does NOT recurse.  This helper walks children first
/// (post-order) so no orphan TaffyNodeIds accumulate (LAYOUT-04).
pub fn remove_taffy_subtree(
    tree: &mut TaffyTree<NodeId>,
    node_id: TaffyNodeId,
) -> Result<(), taffy::TaffyError> {
    // Snapshot children before any mutation — once we remove the parent,
    // the children handles become invalid.
    let children = tree.children(node_id).unwrap_or_default();
    // Post-order: remove children first so no orphan TaffyNodeIds accumulate.
    for child in children {
        remove_taffy_subtree(tree, child)?;
    }
    tree.remove(node_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Display, Edges, FlexDirection, Position};
    use std::cell::Cell;

    fn make_node(tag: &str, width: Dimension, height: Dimension) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.computed_style.width = width;
        node.computed_style.height = height;
        node
    }

    fn keyed_node(key: &str, tag: &str, width: Dimension, height: Dimension) -> WidgetNode {
        let mut node = make_node(tag, width, height);
        node.attributes.insert("_mesh_key".into(), key.into());
        node
    }

    fn retained_fixture() -> WidgetNode {
        let mut root = keyed_node("root", "row", Dimension::Px(200.0), Dimension::Px(100.0));
        root.computed_style.direction = FlexDirection::Row;
        root.children = vec![
            keyed_node("root/0", "a", Dimension::Px(50.0), Dimension::Px(20.0)),
            keyed_node("root/1", "b", Dimension::Px(60.0), Dimension::Px(20.0)),
        ];
        root
    }

    fn collect_keyed_layouts(node: &WidgetNode, layouts: &mut HashMap<String, LayoutRect>) {
        if let Some(key) = node.attributes.get("_mesh_key") {
            layouts.insert(key.clone(), node.layout);
        }
        for child in &node.children {
            collect_keyed_layouts(child, layouts);
        }
    }

    fn keyed_layouts(node: &WidgetNode) -> HashMap<String, LayoutRect> {
        let mut layouts = HashMap::new();
        collect_keyed_layouts(node, &mut layouts);
        layouts
    }

    fn assert_layout_maps_eq(
        retained: &HashMap<String, LayoutRect>,
        fresh: &HashMap<String, LayoutRect>,
    ) {
        assert_eq!(retained.len(), fresh.len());
        for (key, retained_rect) in retained {
            let fresh_rect = fresh.get(key).expect("fresh layout has key");
            assert_eq!(
                (
                    retained_rect.x,
                    retained_rect.y,
                    retained_rect.width,
                    retained_rect.height
                ),
                (
                    fresh_rect.x,
                    fresh_rect.y,
                    fresh_rect.width,
                    fresh_rect.height
                ),
                "layout mismatch for {key}"
            );
        }
    }

    fn assert_retained_matches_fresh(mut retained: WidgetNode, mut fresh: WidgetNode) {
        let mut state = PerSurfaceLayoutState::default();
        let mut cache = IntrinsicLayoutCache::default();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            false,
            &mut cache,
            None,
        );
        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            &mut fresh,
            200.0,
            100.0,
            &mut IntrinsicLayoutCache::default(),
            None,
        );
        assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
    }

    #[derive(Default)]
    struct CountingMeasurer {
        calls: Cell<usize>,
    }

    impl TextMeasurer for CountingMeasurer {
        fn measure_text(
            &self,
            text: &str,
            _font_family: &str,
            _font_size: f32,
            _font_weight: u16,
            _line_height: f32,
            _max_width: Option<f32>,
        ) -> (f32, f32) {
            self.calls.set(self.calls.get() + 1);
            (text.len() as f32 * 8.0, 16.0)
        }
    }

    #[test]
    fn simple_row_layout() {
        let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
        root.computed_style.direction = FlexDirection::Row;

        let child1 = make_node("text", Dimension::Px(100.0), Dimension::Auto);
        let child2 = make_node("text", Dimension::Px(100.0), Dimension::Auto);
        root.children = vec![child1, child2];

        LayoutEngine::compute(&mut root, 300.0, 50.0);

        assert_eq!(root.layout.width, 300.0);
        assert_eq!(root.children[0].layout.x, 0.0);
        assert_eq!(root.children[0].layout.width, 100.0);
        assert_eq!(root.children[1].layout.x, 100.0);
        assert_eq!(root.children[1].layout.width, 100.0);
    }

    #[test]
    fn column_with_gap() {
        let mut root = make_node("column", Dimension::Px(200.0), Dimension::Px(300.0));
        root.computed_style.direction = FlexDirection::Column;
        root.computed_style.gap = 10.0;

        let child1 = make_node("text", Dimension::Auto, Dimension::Px(50.0));
        let child2 = make_node("text", Dimension::Auto, Dimension::Px(50.0));
        root.children = vec![child1, child2];

        LayoutEngine::compute(&mut root, 200.0, 300.0);

        assert_eq!(root.children[0].layout.y, 0.0);
        assert_eq!(root.children[0].layout.height, 50.0);
        assert_eq!(root.children[1].layout.y, 60.0); // 50 + 10 gap
        assert_eq!(root.children[1].layout.height, 50.0);
    }

    #[test]
    fn flex_grow_distributes_space() {
        let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
        root.computed_style.direction = FlexDirection::Row;

        let mut child1 = make_node("a", Dimension::Auto, Dimension::Auto);
        child1.computed_style.flex_grow = 1.0;
        let mut child2 = make_node("b", Dimension::Auto, Dimension::Auto);
        child2.computed_style.flex_grow = 2.0;
        root.children = vec![child1, child2];

        LayoutEngine::compute(&mut root, 300.0, 50.0);

        assert!((root.children[0].layout.width - 100.0).abs() < 0.1);
        assert!((root.children[1].layout.width - 200.0).abs() < 0.1);
    }

    #[test]
    fn padding_insets_children() {
        let mut root = make_node("row", Dimension::Px(200.0), Dimension::Px(100.0));
        root.computed_style.padding = Edges::all(10.0);

        let child = make_node("text", Dimension::Px(50.0), Dimension::Auto);
        root.children = vec![child];

        LayoutEngine::compute(&mut root, 200.0, 100.0);

        assert_eq!(root.children[0].layout.x, 10.0);
        assert_eq!(root.children[0].layout.y, 10.0);
    }

    #[test]
    fn absolute_child_positioned_from_insets() {
        use crate::style::Position;

        let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(200.0));

        // An absolutely-positioned overlay in the bottom-right corner.
        let mut overlay = make_node("overlay", Dimension::Px(80.0), Dimension::Px(40.0));
        overlay.computed_style.position = Position::Absolute;
        overlay.computed_style.inset_right = Some(10.0);
        overlay.computed_style.inset_bottom = Some(10.0);

        // A normal flow child that should not be displaced by the overlay.
        let flow = make_node("content", Dimension::Px(100.0), Dimension::Auto);

        root.children = vec![flow, overlay];
        LayoutEngine::compute(&mut root, 300.0, 200.0);

        // Flow child starts at origin.
        assert_eq!(root.children[0].layout.x, 0.0);
        assert_eq!(root.children[0].layout.y, 0.0);

        // Overlay: right=10 → x = 300 - 80 - 10 = 210; bottom=10 → y = 200 - 40 - 10 = 150.
        assert!(
            (root.children[1].layout.x - 210.0).abs() < 0.5,
            "overlay x = {}",
            root.children[1].layout.x
        );
        assert!(
            (root.children[1].layout.y - 150.0).abs() < 0.5,
            "overlay y = {}",
            root.children[1].layout.y
        );
        assert_eq!(root.children[1].layout.width, 80.0);
        assert_eq!(root.children[1].layout.height, 40.0);
    }

    #[test]
    fn absolute_child_with_top_left_insets() {
        let mut root = make_node("container", Dimension::Px(400.0), Dimension::Px(300.0));

        let mut tooltip = make_node("tooltip", Dimension::Px(120.0), Dimension::Px(30.0));
        tooltip.computed_style.position = Position::Absolute;
        tooltip.computed_style.inset_top = Some(20.0);
        tooltip.computed_style.inset_left = Some(50.0);

        root.children = vec![tooltip];
        LayoutEngine::compute(&mut root, 400.0, 300.0);

        assert!((root.children[0].layout.x - 50.0).abs() < 0.5);
        assert!((root.children[0].layout.y - 20.0).abs() < 0.5);
    }

    #[test]
    fn absolute_position_uses_inset_edges() {
        let mut root = make_node("container", Dimension::Px(300.0), Dimension::Px(200.0));
        root.computed_style.padding = Edges::all(10.0);

        let mut panel = make_node("panel", Dimension::Auto, Dimension::Auto);
        panel.computed_style.position = Position::Absolute;
        panel.computed_style.inset_top = Some(15.0);
        panel.computed_style.inset_right = Some(30.0);
        panel.computed_style.inset_bottom = Some(25.0);
        panel.computed_style.inset_left = Some(20.0);

        root.children = vec![panel];
        LayoutEngine::compute(&mut root, 300.0, 200.0);

        assert_eq!(root.children[0].layout.x, 30.0);
        assert_eq!(root.children[0].layout.y, 25.0);
        assert_eq!(root.children[0].layout.width, 230.0);
        assert_eq!(root.children[0].layout.height, 140.0);
    }

    #[test]
    fn taffy_layout_flex_basis_participates_in_growth() {
        let mut root = make_node("row", Dimension::Px(200.0), Dimension::Px(40.0));
        root.computed_style.direction = FlexDirection::Row;

        let mut basis_child = make_node("basis", Dimension::Auto, Dimension::Auto);
        basis_child.computed_style.flex_grow = 1.0;
        basis_child.computed_style.flex_shrink = 0.0;
        basis_child.computed_style.flex_basis = Dimension::Px(80.0);
        let fixed_child = make_node("fixed", Dimension::Px(40.0), Dimension::Auto);

        root.children = vec![basis_child, fixed_child];
        LayoutEngine::compute(&mut root, 200.0, 40.0);

        assert_eq!(root.children[0].layout.width, 160.0);
        assert_eq!(root.children[1].layout.x, 160.0);
        assert_eq!(root.children[1].layout.width, 40.0);
    }

    #[test]
    fn display_none_excludes_node_from_layout() {
        let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(40.0));
        root.computed_style.direction = FlexDirection::Row;

        let mut hidden = make_node("hidden", Dimension::Px(100.0), Dimension::Px(20.0));
        hidden.computed_style.display = Display::None;
        let visible = make_node("visible", Dimension::Px(50.0), Dimension::Px(20.0));

        root.children = vec![hidden, visible];
        LayoutEngine::compute(&mut root, 300.0, 40.0);

        assert_eq!(root.children[0].layout.x, 0.0);
        assert_eq!(root.children[0].layout.y, 0.0);
        assert_eq!(root.children[0].layout.width, 0.0);
        assert_eq!(root.children[0].layout.height, 0.0);
        assert_eq!(root.children[1].layout.x, 0.0);
        assert_eq!(root.children[1].layout.width, 50.0);
    }

    #[test]
    fn taffy_layout_text_leaf_uses_measurer() {
        let mut root = make_node("row", Dimension::Px(100.0), Dimension::Px(40.0));
        root.computed_style.direction = FlexDirection::Row;
        root.computed_style.align_items = AlignItems::Start;
        let mut child = make_node("text", Dimension::Content, Dimension::Content);
        child.attributes.insert("content".into(), "hello".into());
        root.children = vec![child];

        let measurer = CountingMeasurer::default();
        LayoutEngine::compute_with_measurer(&mut root, 100.0, 40.0, Some(&measurer));

        assert!(measurer.calls.get() > 0);
        assert_eq!(root.children[0].layout.width, 40.0);
        assert_eq!(root.children[0].layout.height, 16.0);
    }

    #[test]
    fn rtl_row_reverses_child_order() {
        use crate::style::TextDirection;

        // Container 300px wide, two children 100px each.
        let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
        root.computed_style.direction = FlexDirection::Row;
        root.computed_style.text_direction = TextDirection::Rtl;

        let a = make_node("a", Dimension::Px(100.0), Dimension::Auto);
        let b = make_node("b", Dimension::Px(100.0), Dimension::Auto);
        root.children = vec![a, b];
        LayoutEngine::compute(&mut root, 300.0, 50.0);

        // In RTL the first child should be at x=200 (right side) and the second at x=100.
        assert!(
            (root.children[0].layout.x - 200.0).abs() < 0.5,
            "a.x = {}",
            root.children[0].layout.x
        );
        assert!(
            (root.children[1].layout.x - 100.0).abs() < 0.5,
            "b.x = {}",
            root.children[1].layout.x
        );
    }

    #[test]
    fn rtl_column_is_unaffected() {
        use crate::style::TextDirection;

        let mut root = make_node("col", Dimension::Px(200.0), Dimension::Px(200.0));
        root.computed_style.direction = FlexDirection::Column;
        root.computed_style.text_direction = TextDirection::Rtl;

        let a = make_node("a", Dimension::Auto, Dimension::Px(40.0));
        let b = make_node("b", Dimension::Auto, Dimension::Px(40.0));
        root.children = vec![a, b];
        LayoutEngine::compute(&mut root, 200.0, 200.0);

        // Column direction is not affected by RTL — children still stack top-to-bottom.
        assert_eq!(root.children[0].layout.y, 0.0);
        assert_eq!(root.children[1].layout.y, 40.0);
    }

    #[test]
    fn taffy_layout_text_content_changes_measurement_without_node_id_churn() {
        let mut root = make_node("row", Dimension::Content, Dimension::Auto);
        root.computed_style.direction = FlexDirection::Row;
        let mut child = make_node("text", Dimension::Auto, Dimension::Auto);
        child.attributes.insert("content".into(), "hello".into());
        let child_id = child.id;
        root.children.push(child);

        let measurer = CountingMeasurer::default();
        let mut cache = IntrinsicLayoutCache::default();

        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            &mut root,
            300.0,
            40.0,
            &mut cache,
            Some(&measurer),
        );
        let first_width = root.children[0].layout.width;

        root.children[0]
            .attributes
            .insert("content".into(), "hello world".into());
        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            &mut root,
            300.0,
            40.0,
            &mut cache,
            Some(&measurer),
        );

        assert_eq!(root.children[0].id, child_id);
        assert!(measurer.calls.get() >= 2);
        assert!(root.children[0].layout.width > first_width);
    }

    #[test]
    fn phase47_taffy_required_layout_parity_cases() {
        let mut row = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
        row.computed_style.direction = FlexDirection::Row;
        row.children = vec![
            make_node("a", Dimension::Px(100.0), Dimension::Px(20.0)),
            make_node("b", Dimension::Px(100.0), Dimension::Px(20.0)),
        ];
        LayoutEngine::compute(&mut row, 300.0, 50.0);
        assert_eq!(row.children[0].layout.x, 0.0);
        assert_eq!(row.children[1].layout.x, 100.0);

        let mut nested = make_node("nested-root", Dimension::Px(300.0), Dimension::Px(80.0));
        nested.computed_style.direction = FlexDirection::Row;
        let mut nested_parent =
            make_node("nested-parent", Dimension::Px(120.0), Dimension::Px(40.0));
        nested_parent.computed_style.margin.left = 30.0;
        nested_parent.children = vec![make_node(
            "nested-child",
            Dimension::Px(20.0),
            Dimension::Px(20.0),
        )];
        nested.children = vec![nested_parent];
        LayoutEngine::compute(&mut nested, 300.0, 80.0);
        assert_eq!(nested.children[0].layout.x, 30.0);
        assert_eq!(nested.children[0].children[0].layout.x, 30.0);

        let mut column = make_node("column", Dimension::Px(200.0), Dimension::Px(300.0));
        column.computed_style.direction = FlexDirection::Column;
        column.computed_style.gap = 10.0;
        column.children = vec![
            make_node("first", Dimension::Px(100.0), Dimension::Px(50.0)),
            make_node("second", Dimension::Px(100.0), Dimension::Px(50.0)),
        ];
        LayoutEngine::compute(&mut column, 200.0, 300.0);
        assert_eq!(column.children[0].layout.y, 0.0);
        assert_eq!(column.children[1].layout.y, 60.0);

        let mut stack = make_node("stack", Dimension::Px(120.0), Dimension::Px(80.0));
        let mut first = make_node("first", Dimension::Px(40.0), Dimension::Px(30.0));
        first.computed_style.position = Position::Absolute;
        first.computed_style.inset_left = Some(0.0);
        first.computed_style.inset_top = Some(0.0);
        let mut second = make_node("second", Dimension::Px(40.0), Dimension::Px(30.0));
        second.computed_style.position = Position::Absolute;
        second.computed_style.inset_left = Some(0.0);
        second.computed_style.inset_top = Some(0.0);
        stack.children = vec![first, second];
        LayoutEngine::compute(&mut stack, 120.0, 80.0);
        assert_eq!(stack.children[0].layout.x, 0.0);
        assert_eq!(stack.children[1].layout.x, 0.0);
        assert_eq!(stack.children[0].layout.y, 0.0);
        assert_eq!(stack.children[1].layout.y, 0.0);

        let mut fixed = make_node("fixed-root", Dimension::Px(200.0), Dimension::Px(100.0));
        fixed.children = vec![make_node(
            "fixed-child",
            Dimension::Px(75.0),
            Dimension::Px(25.0),
        )];
        LayoutEngine::compute(&mut fixed, 200.0, 100.0);
        assert_eq!(fixed.children[0].layout.width, 75.0);
        assert_eq!(fixed.children[0].layout.height, 25.0);

        let mut padded = make_node("padded", Dimension::Px(200.0), Dimension::Px(100.0));
        padded.computed_style.padding = Edges::all(10.0);
        padded.children = vec![make_node(
            "padded-child",
            Dimension::Px(50.0),
            Dimension::Px(20.0),
        )];
        LayoutEngine::compute(&mut padded, 200.0, 100.0);
        assert_eq!(padded.children[0].layout.x, 10.0);
        assert_eq!(padded.children[0].layout.y, 10.0);

        let mut absolute = make_node("absolute-root", Dimension::Px(300.0), Dimension::Px(200.0));
        let mut overlay = make_node("overlay", Dimension::Px(80.0), Dimension::Px(40.0));
        overlay.computed_style.position = Position::Absolute;
        overlay.computed_style.inset_right = Some(10.0);
        overlay.computed_style.inset_bottom = Some(10.0);
        absolute.children = vec![overlay];
        LayoutEngine::compute(&mut absolute, 300.0, 200.0);
        assert!((absolute.children[0].layout.x - 210.0).abs() <= 0.5);
        assert!((absolute.children[0].layout.y - 150.0).abs() <= 0.5);

        let mut percent = make_node("percent-root", Dimension::Px(300.0), Dimension::Px(60.0));
        percent.children = vec![make_node(
            "percent-child",
            Dimension::Percent(50.0),
            Dimension::Px(20.0),
        )];
        LayoutEngine::compute(&mut percent, 300.0, 60.0);
        assert_eq!(percent.children[0].layout.width, 150.0);
    }

    #[test]
    fn phase87_layout_runtime_stack_spacer_divider_and_scroll_area_stay_compatible() {
        let mut stack = make_node("stack", Dimension::Px(160.0), Dimension::Px(90.0));
        let mut base = make_node("base", Dimension::Px(160.0), Dimension::Px(90.0));
        base.computed_style.position = Position::Absolute;
        base.computed_style.inset_left = Some(0.0);
        base.computed_style.inset_top = Some(0.0);
        let mut overlay = make_node("overlay", Dimension::Px(40.0), Dimension::Px(20.0));
        overlay.computed_style.position = Position::Absolute;
        overlay.computed_style.inset_left = Some(0.0);
        overlay.computed_style.inset_top = Some(0.0);
        overlay.computed_style.z_index = 1;
        stack.children = vec![base, overlay];
        LayoutEngine::compute(&mut stack, 160.0, 90.0);
        assert_eq!(stack.children[0].layout.x, 0.0);
        assert_eq!(stack.children[1].layout.x, 0.0);
        assert_eq!(stack.children[0].layout.y, 0.0);
        assert_eq!(stack.children[1].layout.y, 0.0);
        assert!(
            stack.children[1].computed_style.z_index > stack.children[0].computed_style.z_index
        );

        let mut row = make_node("row", Dimension::Px(240.0), Dimension::Px(24.0));
        row.computed_style.direction = FlexDirection::Row;
        let fixed = make_node("fixed", Dimension::Px(40.0), Dimension::Px(24.0));
        let mut spacer = make_node("spacer", Dimension::Auto, Dimension::Px(24.0));
        spacer.computed_style.flex_grow = 1.0;
        let divider = make_node("divider", Dimension::Px(1.0), Dimension::Px(24.0));
        row.children = vec![fixed, spacer, divider];
        LayoutEngine::compute(&mut row, 240.0, 24.0);
        assert_eq!(row.children[0].layout.width, 40.0);
        assert!((row.children[1].layout.width - 199.0).abs() < 0.5);
        assert_eq!(row.children[2].layout.width, 1.0);

        let mut scroll_area = make_node("scroll", Dimension::Px(120.0), Dimension::Px(60.0));
        scroll_area
            .attributes
            .insert("data-mesh-element".into(), "scroll-area".into());
        scroll_area
            .attributes
            .insert("_mesh_scroll_y".into(), "12.50".into());
        scroll_area.children = vec![make_node(
            "content",
            Dimension::Px(120.0),
            Dimension::Px(180.0),
        )];
        LayoutEngine::compute(&mut scroll_area, 120.0, 60.0);
        assert_eq!(
            scroll_area
                .attributes
                .get("data-mesh-element")
                .map(String::as_str),
            Some("scroll-area")
        );
        assert_eq!(scroll_area.layout.width, 120.0);
        assert_eq!(scroll_area.layout.height, 60.0);
        assert_eq!(scroll_area.children[0].layout.height, 180.0);
    }

    #[test]
    fn taffy_diagnostic_records_node_identity_and_reason() {
        let node = make_node("diagnostic-target", Dimension::Auto, Dimension::Auto);
        let mut report = TaffyLayoutReport::default();

        record_taffy_diagnostic(&mut report, &node, "unsupported layout mapping: test-only");

        assert!(!report.is_clean());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].node_id, node.id);
        assert_eq!(report.diagnostics[0].tag, "diagnostic-target");
        assert_eq!(
            report.diagnostics[0].reason,
            "unsupported layout mapping: test-only"
        );
    }

    #[test]
    fn content_dimension_taffy_diagnostic_is_expected_measurement_noise() {
        assert!(is_expected_taffy_measurement_diagnostic(
            CONTENT_DIMENSION_TAFFY_DIAGNOSTIC
        ));
        assert!(!is_expected_taffy_measurement_diagnostic(
            "unsupported layout mapping: test-only"
        ));
    }

    #[test]
    fn compute_incremental_fresh_build_matches_baseline() {
        assert_retained_matches_fresh(retained_fixture(), retained_fixture());
    }

    #[test]
    fn compute_incremental_visual_repaint_preserves_layout() {
        let mut root = retained_fixture();
        let mut state = PerSurfaceLayoutState::default();
        let mut cache = IntrinsicLayoutCache::default();
        LayoutEngine::compute_incremental(
            &mut root, &mut state, 200.0, 100.0, false, false, &mut cache, None,
        );
        let before = keyed_layouts(&root);

        root.children[0].computed_style.background_color = Color {
            r: 10,
            g: 20,
            b: 30,
            a: 255,
        };
        LayoutEngine::compute_incremental(
            &mut root, &mut state, 200.0, 100.0, false, false, &mut cache, None,
        );

        assert_layout_maps_eq(&keyed_layouts(&root), &before);
    }

    #[test]
    fn retained_layout_parity_style_only() {
        let mut retained = retained_fixture();
        let mut state = PerSurfaceLayoutState::default();
        let mut cache = IntrinsicLayoutCache::default();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            false,
            &mut cache,
            None,
        );

        retained.children[0].computed_style.opacity = 0.5;
        let mut fresh = retained.clone();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            false,
            &mut cache,
            None,
        );
        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            &mut fresh,
            200.0,
            100.0,
            &mut IntrinsicLayoutCache::default(),
            None,
        );

        assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
    }

    #[test]
    fn retained_layout_parity_layout_dirty() {
        let mut retained = retained_fixture();
        let mut state = PerSurfaceLayoutState::default();
        let mut cache = IntrinsicLayoutCache::default();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            false,
            &mut cache,
            None,
        );

        retained.children[0].computed_style.width = Dimension::Px(80.0);
        let mut fresh = retained.clone();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            true,
            false,
            &mut cache,
            None,
        );
        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            &mut fresh,
            200.0,
            100.0,
            &mut IntrinsicLayoutCache::default(),
            None,
        );

        assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
    }

    #[test]
    fn retained_layout_parity_add_node() {
        let mut retained = retained_fixture();
        let mut state = PerSurfaceLayoutState::default();
        let mut cache = IntrinsicLayoutCache::default();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            false,
            &mut cache,
            None,
        );

        retained.children.push(keyed_node(
            "root/2",
            "c",
            Dimension::Px(40.0),
            Dimension::Px(20.0),
        ));
        let mut fresh = retained.clone();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            true,
            &mut cache,
            None,
        );
        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            &mut fresh,
            200.0,
            100.0,
            &mut IntrinsicLayoutCache::default(),
            None,
        );

        assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
    }

    #[test]
    fn retained_layout_parity_remove_node() {
        let mut retained = retained_fixture();
        let mut state = PerSurfaceLayoutState::default();
        let mut cache = IntrinsicLayoutCache::default();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            false,
            &mut cache,
            None,
        );

        retained.children.remove(0);
        let mut fresh = retained.clone();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            true,
            &mut cache,
            None,
        );
        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            &mut fresh,
            200.0,
            100.0,
            &mut IntrinsicLayoutCache::default(),
            None,
        );

        assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
    }

    #[test]
    fn retained_layout_parity_reorder() {
        let mut retained = retained_fixture();
        let mut state = PerSurfaceLayoutState::default();
        let mut cache = IntrinsicLayoutCache::default();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            false,
            &mut cache,
            None,
        );

        retained.children.swap(0, 1);
        let mut fresh = retained.clone();
        LayoutEngine::compute_incremental(
            &mut retained,
            &mut state,
            200.0,
            100.0,
            false,
            true,
            &mut cache,
            None,
        );
        LayoutEngine::compute_with_intrinsic_cache_and_measurer(
            &mut fresh,
            200.0,
            100.0,
            &mut IntrinsicLayoutCache::default(),
            None,
        );

        assert_layout_maps_eq(&keyed_layouts(&retained), &keyed_layouts(&fresh));
    }

    #[test]
    fn remove_taffy_subtree_removes_all_descendants() {
        // Build a 4-node TaffyTree: grandparent → parent → leaf + sibling leaf.
        let mut tree = TaffyTree::<NodeId>::new();
        let leaf1 = tree.new_leaf(taffy_style::Style::default()).unwrap();
        let leaf2 = tree.new_leaf(taffy_style::Style::default()).unwrap();
        let parent = tree
            .new_with_children(taffy_style::Style::default(), &[leaf1, leaf2])
            .unwrap();
        let leaf3 = tree.new_leaf(taffy_style::Style::default()).unwrap();
        let grandparent = tree
            .new_with_children(taffy_style::Style::default(), &[parent, leaf3])
            .unwrap();

        assert!(
            tree.total_node_count() >= 4,
            "sanity: tree should have at least 4 nodes"
        );

        // Remove the root → post-order walks children first.
        remove_taffy_subtree(&mut tree, grandparent).unwrap();

        assert_eq!(
            tree.total_node_count(),
            0,
            "all nodes should be removed after post-order subtree removal"
        );
    }

    #[test]
    fn per_surface_layout_state_default_is_invalid() {
        let state = PerSurfaceLayoutState::new();
        assert!(!state.valid);
        assert!(state.node_map.is_empty());
        assert_eq!(state.last_available, (0.0, 0.0));
        assert_eq!(state.tree.total_node_count(), 0);
    }

    #[test]
    fn fixed_child_positioned_from_viewport() {
        let mut root = make_node("root", Dimension::Px(960.0), Dimension::Px(540.0));
        let mut inner = make_node("inner", Dimension::Px(200.0), Dimension::Px(100.0));
        inner.layout.x = 0.0;
        inner.layout.y = 0.0;
        let mut overlay = make_node("overlay", Dimension::Px(100.0), Dimension::Px(40.0));
        overlay.computed_style.position = Position::Fixed;
        overlay.computed_style.inset_right = Some(10.0);
        overlay.computed_style.inset_bottom = Some(8.0);
        inner.children = vec![overlay];
        root.children = vec![inner];
        LayoutEngine::compute(&mut root, 960.0, 540.0);
        // Fixed: bottom-right corner of the 960x540 viewport
        assert!(
            (root.children[0].children[0].layout.x - 850.0).abs() < 0.5,
            "expected x≈850, got {}",
            root.children[0].children[0].layout.x
        );
        assert!(
            (root.children[0].children[0].layout.y - 492.0).abs() < 0.5,
            "expected y≈492, got {}",
            root.children[0].children[0].layout.y
        );
    }

    #[test]
    fn fixed_child_top_left_positioned_from_viewport() {
        let mut root = make_node("root", Dimension::Px(800.0), Dimension::Px(600.0));
        let mut panel = make_node("panel", Dimension::Px(400.0), Dimension::Px(300.0));
        panel.computed_style.padding = Edges::all(20.0);
        let mut tooltip = make_node("tooltip", Dimension::Px(120.0), Dimension::Px(30.0));
        tooltip.computed_style.position = Position::Fixed;
        tooltip.computed_style.inset_top = Some(50.0);
        tooltip.computed_style.inset_left = Some(100.0);
        panel.children = vec![tooltip];
        root.children = vec![panel];
        LayoutEngine::compute(&mut root, 800.0, 600.0);
        let tooltip_layout = &root.children[0].children[0].layout;
        assert!(
            (tooltip_layout.x - 100.0).abs() < 0.5,
            "expected x≈100, got {}",
            tooltip_layout.x
        );
        assert!(
            (tooltip_layout.y - 50.0).abs() < 0.5,
            "expected y≈50, got {}",
            tooltip_layout.y
        );
    }

    #[test]
    fn fixed_child_full_width_stretch() {
        let mut root = make_node("root", Dimension::Px(1920.0), Dimension::Px(1080.0));
        let mut inner = make_node("inner", Dimension::Px(400.0), Dimension::Px(200.0));
        let mut bar = make_node("bar", Dimension::Auto, Dimension::Px(40.0));
        bar.computed_style.position = Position::Fixed;
        bar.computed_style.inset_top = Some(0.0);
        bar.computed_style.inset_left = Some(0.0);
        bar.computed_style.inset_right = Some(0.0);
        inner.children = vec![bar];
        root.children = vec![inner];
        LayoutEngine::compute(&mut root, 1920.0, 1080.0);
        let bar_layout = &root.children[0].children[0].layout;
        assert!(
            (bar_layout.x - 0.0).abs() < 0.5,
            "expected x=0, got {}",
            bar_layout.x
        );
        assert!(
            (bar_layout.width - 1920.0).abs() < 0.5,
            "expected width=1920, got {}",
            bar_layout.width
        );
    }
}
