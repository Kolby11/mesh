/// Flexbox-subset layout engine.
///
/// Computes `LayoutRect` for every node in a widget tree. Supports row/column
/// direction, flex-grow/shrink, gap, padding, and margin.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::lru::LruCache;
use crate::style::{
    AlignContent, AlignItems, AlignSelf, Dimension, Display, Edges, FlexDirection, JustifyContent,
    Overflow, Position, TextDirection,
};
use crate::tree::{NodeId, WidgetNode};
use taffy::TaffyTree;
use taffy::geometry::{Point as TaffyPoint, Rect as TaffyRect, Size as TaffySize};
use taffy::prelude::{AvailableSpace as TaffyAvailableSpace, NodeId as TaffyNodeId};
use taffy::style as taffy_style;

/// Trait for measuring text dimensions. Implemented outside `mesh-core-elements` (in the
/// shell render stack) and injected so the layout engine can shrink-wrap text
mod lowering;
mod retained;

use lowering::*;
use retained::*;

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
    content: Arc<str>,
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
    /// Maps stable runtime [`NodeId`] values to retained Taffy nodes.
    ///
    /// Runtime IDs are hash-chained from the same stable paths formerly kept
    /// here as `_mesh_key` strings, so retained lookup no longer hashes or
    /// clones long ancestor paths.
    pub node_map: HashMap<NodeId, TaffyNodeId>,
    /// The subset of [`Self::node_map`] whose identity survives structural
    /// reconciliation. Unkeyed nodes still have entries in `node_map` so
    /// layout write-back never needs a second full-tree index, but are
    /// deliberately recreated when the tree shape changes.
    stable_node_ids: HashSet<NodeId>,
    /// Text measurement inputs keyed by stable node identity. Keeping these
    /// alongside the retained Taffy nodes avoids rebuilding text content and
    /// style contexts on every layout-dirty frame.
    text_nodes: HashMap<NodeId, TextMeasureData>,
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
            stable_node_ids: HashSet::new(),
            text_nodes: HashMap::new(),
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

    fn compute_taffy_layout_with_cache(
        root: &mut WidgetNode,
        available_width: f32,
        available_height: f32,
        intrinsic_cache: &mut IntrinsicLayoutCache,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        let _span = tracing::debug_span!("layout").entered();
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
        Self::compute_incremental_with_dirty_nodes(
            root,
            state,
            available_width,
            available_height,
            dirty_layout,
            dirty_structural,
            None,
            intrinsic_cache,
            measurer,
        );
    }

    /// Compute layout by mutating a retained per-surface Taffy tree, optionally
    /// limiting style/context synchronization to known layout-relevant dirty
    /// nodes on non-structural frames.
    pub fn compute_incremental_with_dirty_nodes(
        root: &mut WidgetNode,
        state: &mut PerSurfaceLayoutState,
        available_width: f32,
        available_height: f32,
        dirty_layout: bool,
        dirty_structural: bool,
        dirty_node_ids: Option<&HashSet<NodeId>>,
        intrinsic_cache: &mut IntrinsicLayoutCache,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        Self::compute_incremental_with_dirty_sources(
            root,
            state,
            available_width,
            available_height,
            dirty_layout,
            dirty_structural,
            dirty_node_ids,
            None,
            intrinsic_cache,
            measurer,
        );
    }

    /// Compute layout using owned snapshots of the nodes whose layout inputs
    /// changed. The snapshots are intentionally owned because the caller's
    /// live tree must be mutably borrowed for Taffy's layout write-back.
    /// `WidgetNode` uses copy-on-write authored and child payloads, so this is
    /// proportional to the sparse dirty set rather than the whole tree.
    pub fn compute_incremental_with_dirty_node_snapshots(
        root: &mut WidgetNode,
        state: &mut PerSurfaceLayoutState,
        available_width: f32,
        available_height: f32,
        dirty_layout: bool,
        dirty_structural: bool,
        dirty_node_snapshots: Option<&[WidgetNode]>,
        intrinsic_cache: &mut IntrinsicLayoutCache,
        measurer: Option<&dyn TextMeasurer>,
    ) {
        Self::compute_incremental_with_dirty_sources(
            root,
            state,
            available_width,
            available_height,
            dirty_layout,
            dirty_structural,
            None,
            dirty_node_snapshots,
            intrinsic_cache,
            measurer,
        );
    }

    fn compute_incremental_with_dirty_sources(
        root: &mut WidgetNode,
        state: &mut PerSurfaceLayoutState,
        available_width: f32,
        available_height: f32,
        dirty_layout: bool,
        dirty_structural: bool,
        dirty_node_ids: Option<&HashSet<NodeId>>,
        dirty_node_snapshots: Option<&[WidgetNode]>,
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

        let available_changed = state.last_available != (available_width, available_height);
        // Paint-only frames cannot change geometry. Leave the retained Taffy
        // tree untouched and defer style/context synchronization until a
        // layout-dirty frame, when it is needed immediately before layout.
        // This avoids rebuilding two maps, converting every ComputedStyle,
        // and calling set_style for every node on animation/repaint frames.
        if !available_changed && !dirty_layout {
            return;
        }

        let mut report = TaffyLayoutReport::default();
        if let Some(dirty_node_snapshots) = dirty_node_snapshots {
            update_retained_node_snapshots(state, dirty_layout, dirty_node_snapshots, &mut report);
        } else {
            update_retained_node_styles(root, state, dirty_layout, dirty_node_ids, &mut report);
        }

        if available_changed || dirty_layout {
            let available_space = taffy_available_space(available_width, available_height);
            let (tree, text_nodes) = (&mut state.tree, &state.text_nodes);
            if let Err(error) = tree.compute_layout_with_measure(
                root_id,
                available_space,
                |known_dimensions, available_space, _node_id, context, _style| {
                    measure_taffy_node(
                        known_dimensions,
                        available_space,
                        context.map(|node_id| *node_id),
                        text_nodes,
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
                write_taffy_layout(
                    root,
                    &state.tree,
                    &state.node_map,
                    available_width,
                    available_height,
                );
                state.last_available = (available_width, available_height);
            }
        }

        log_taffy_report(&report);
    }
}

#[cfg(test)]
mod tests;
