use std::sync::Arc;

use mesh_core_elements::NodeId;

use super::blur::*;
use super::build::*;
use super::paint_node::*;
use super::types::*;

#[derive(Debug, Clone)]
pub(super) struct RetainedPaintSubtree {
    pub(super) generation: u64,
    pub(super) commands: Arc<[DisplayPaintCommand]>,
    pub(super) kinds: Arc<[DisplayPaintCommandKind]>,
    pub(super) effect_overflow_count: u64,
    pub(super) pruning: PruningMetrics,
    pub(super) command_span: Option<RetainedSubtreeSpan>,
    pub(super) child_order: Option<Arc<[usize]>>,
    /// This subtree's commands open and close a blur layer, so its command
    /// range is atomic: a partial repaint may not replay part of it.
    pub(super) filter_layer: bool,
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
            filter_layer: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RetainedSubtreeSpan {
    pub(super) bounds: DamageRect,
    pub(super) local_bounds: DamageRect,
    pub(super) command_count: usize,
    pub(super) includes_scrollbars: bool,
}

#[derive(Debug, Default)]
pub(super) struct PaintSubtreeBuilder {
    pub(super) commands: Vec<DisplayPaintCommand>,
    pub(super) kinds: Vec<DisplayPaintCommandKind>,
    pub(super) effect_overflow_count: u64,
    pub(super) pruning: PruningMetrics,
    pub(super) bounds: DamageRect,
    pub(super) local_bounds: DamageRect,
    /// Union of the children's bounds only. A blur layer inflates this — and
    /// not the node's own bounds, which `visual_clip_for` already padded — to
    /// find the region the layer composites.
    pub(super) child_bounds: DamageRect,
    pub(super) includes_scrollbars: bool,
    pub(super) local_command_count: usize,
    pub(super) child_order: Option<Arc<[usize]>>,
}

impl PaintSubtreeBuilder {
    pub(super) fn push_command(&mut self, command: DisplayPaintCommand) {
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

    pub(super) fn append_child(&mut self, child_subtree: &RetainedPaintSubtree) {
        self.effect_overflow_count = self
            .effect_overflow_count
            .saturating_add(child_subtree.effect_overflow_count);
        if let Some(span) = child_subtree.command_span {
            self.bounds = if self.bounds.width == 0 || self.bounds.height == 0 {
                span.bounds
            } else {
                self.bounds.union(span.bounds)
            };
            self.child_bounds = if self.child_bounds.width == 0 || self.child_bounds.height == 0 {
                span.bounds
            } else {
                self.child_bounds.union(span.bounds)
            };
            self.includes_scrollbars |= span.includes_scrollbars;
        }

        self.commands.reserve(child_subtree.commands.len());
        self.commands.extend_from_slice(&child_subtree.commands);
        self.kinds.reserve(child_subtree.kinds.len());
        self.kinds.extend_from_slice(&child_subtree.kinds);
    }

    pub(super) fn append_pruning(&mut self, child_subtree: &RetainedPaintSubtree) {
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

    /// Widens the open filter layer's push clip to the region the layer
    /// composites: this node's own visual bounds (already padded by
    /// `visual_clip_for`) unioned with its descendants' bounds grown by the
    /// blur kernel's reach, clipped to what the node was allowed to paint
    /// into. The subtree's extent is only known once its children have been
    /// appended, so the push command is emitted first and patched here.
    pub(super) fn grow_filter_layer_bounds(&mut self, clip: DisplayListClip, blur_pad: f32) {
        let pad = blur_pad.ceil() as i32;
        let mut region = DisplayListClip {
            x: self.local_bounds.x as i32,
            y: self.local_bounds.y as i32,
            width: self.local_bounds.width as i32,
            height: self.local_bounds.height as i32,
        };
        if self.child_bounds.width > 0 && self.child_bounds.height > 0 {
            let padded_children = DisplayListClip {
                x: self.child_bounds.x as i32 - pad,
                y: self.child_bounds.y as i32 - pad,
                width: self.child_bounds.width as i32 + pad * 2,
                height: self.child_bounds.height as i32 + pad * 2,
            };
            region = union_display_clip(region, padded_children);
        }
        let region = intersect_display_clip(clip, region);
        let Some(push) = self.commands.first_mut() else {
            return;
        };
        debug_assert_eq!(push.kind, DisplayPaintCommandKind::PushFilterLayer);
        push.clip = region;
        // The layer composites the whole padded region, so the subtree's
        // damage bounds have to cover it or a repaint would leave a stale
        // blur ring behind.
        let padded = DamageRect {
            x: region.x.max(0) as u32,
            y: region.y.max(0) as u32,
            width: region.width.max(0) as u32,
            height: region.height.max(0) as u32,
        };
        self.bounds = self.bounds.union(padded);
        self.local_bounds = self.local_bounds.union(padded);
    }

    pub(super) fn into_retained(self, generation: u64, filter_layer: bool) -> RetainedPaintSubtree {
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
            filter_layer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RetainedCommandSpan {
    pub(super) owner: NodeId,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) bounds: DamageRect,
    pub(super) command_count: usize,
    pub(super) includes_scrollbars: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SelectedCommandSpan {
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct LocalReuseMetrics {
    pub(super) reused_segments: u64,
    pub(super) rebuilt_segments: u64,
    pub(super) rebuilt_commands: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalReuseDecision {
    RebuildDirtySubtrees,
    FallbackFull { broad_dirty: bool },
}
