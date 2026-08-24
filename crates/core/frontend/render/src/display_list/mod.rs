use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mesh_core_elements::{NodeId, WidgetNode};

use crate::RenderObjectDirtySummary;

mod blur;
mod build;
mod paint_node;
mod signature;
mod spans;
mod subtree;
mod types;

pub use blur::{backdrop_blur_regions, backdrop_blur_regions_from_tree};
pub use types::*;

use blur::*;
use build::*;
use signature::*;
use spans::*;
use subtree::*;

impl RetainedDisplayList {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Stays stable when paint work elsewhere in the surface changes, so a
    /// promoted child surface can skip repainting for unrelated parent updates.
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

    /// Build for `root` in a target-local viewport, letting a promoted child
    /// surface retain its own command stream even when the authored subtree
    /// lies outside its parent. The origin is part of the cache key, so moving
    /// the subtree cannot replay commands built for an older position.
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

    /// Generation-gated [`Self::update_at`]: an unchanged generation, viewport,
    /// and origin skip both entry collection and command reconstruction.
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

    pub(in crate::display_list) fn update_inner(
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

        let has_authoritative_dirty_summary = dirty_summary.is_some();
        let dirty_summary = dirty_summary.unwrap_or_default();
        let empty_dirty_nodes = HashSet::new();
        let dirty_node_ids = dirty_node_ids.unwrap_or(&empty_dirty_nodes);
        let blur_metadata_reuse_candidate = has_authoritative_dirty_summary
            && self.root_id == Some(root.id)
            && self.surface_size == Some((surface.width, surface.height))
            && self.paint_origin == paint_origin
            && dirty_summary_preserves_blur_metadata(dirty_summary);
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
                // A transform or overflow-clip change changes every
                // descendant's cumulative affine geometry even when their
                // own layout/material slots are clean. Reusing those child
                // subtrees would retain stale world transforms and clip
                // stacks.
                let allow_clean_descendant_reuse = changed_layout_count(dirty_summary) == 0
                    && dirty_summary.transform == 0
                    && dirty_summary.clip == 0;
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

        let topology_changed =
            paint_topology_changed(self.paint_commands.as_ref(), paint_commands.as_ref());

        let (damage, mut damage_rects, reused, rebuilt, removed) =
            self.reconcile_entries(&mut next, patch_sparse_entries, dirty_node_ids, surface);

        // A text change can share a frame with a visibility annotation the
        // summary does not classify as primitive; the command count catches
        // that topology change before blur regions are preserved.
        let can_reuse_blur_metadata = blur_metadata_reuse_candidate
            && removed == 0
            && self.paint_commands.len() == paint_commands.len();
        let updated_blur_metadata = (!can_reuse_blur_metadata).then(|| {
            (
                compute_backdrop_regions(paint_commands.as_ref(), surface),
                // From the full `root` tree, not `paint_commands`, so scoped
                // updates never drop blur nodes (see field docs).
                backdrop_blur_regions_from_tree(root, offset_x, offset_y, surface),
            )
        });

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

        if rebuilt > 0 || removed > 0 || force_full_damage || topology_changed {
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
        self.layer_scopes = collect_layer_scopes(self.paint_commands.as_ref());
        self.filter_layer_regions =
            filter_layer_regions(self.paint_commands.as_ref(), &self.layer_scopes, surface);
        if let Some((backdrop_regions, blur_regions)) = updated_blur_metadata {
            self.backdrop_regions = backdrop_regions;
            self.blur_regions = blur_regions;
        }
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

    pub(in crate::display_list) fn reconcile_entries(
        &mut self,
        next: &mut HashMap<DisplayListKey, DisplayListEntry>,
        patch_sparse_entries: bool,
        dirty_node_ids: &HashSet<NodeId>,
        surface: DamageRect,
    ) -> (Option<DamageRect>, Vec<DamageRect>, u64, u64, u64) {
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
            for (key, next_entry) in next.iter() {
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
        (damage, damage_rects, reused, rebuilt, removed)
    }

    pub(in crate::display_list) fn update_metrics_without_rebuild(
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

    /// Node rects, inflated by the blur kernel reach, where a `backdrop-filter`
    /// node has painted content beneath it in paint order.
    pub fn backdrop_filter_regions(&self) -> &[DamageRect] {
        &self.backdrop_regions
    }

    /// Extents of `filter: blur()` layers, inflated by the kernel reach. Every
    /// pixel of a layer is a function of every other, so partial damage inside
    /// one must grow to the whole region.
    pub fn filter_layer_regions(&self) -> &[DamageRect] {
        &self.filter_layer_regions
    }

    /// Grows every damage rect intersecting a blur region to cover that whole
    /// region, so a blur re-reads fresh pixels rather than mixing frames. Runs
    /// to a fixpoint so overlapping regions cascade; returns whether any grew.
    pub fn expand_damage_for_blur_regions(&self, rects: &mut [DamageRect]) -> bool {
        if (self.backdrop_regions.is_empty() && self.filter_layer_regions.is_empty())
            || rects.is_empty()
        {
            return false;
        }
        let mut expanded = false;
        loop {
            let mut changed = false;
            for region in self
                .backdrop_regions
                .iter()
                .chain(self.filter_layer_regions.iter())
            {
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

    /// Derived from the full widget tree at the last rebuild and handed to
    /// `org_kde_kwin_blur`; empty means the surface has no blur.
    pub fn blur_regions(&self) -> &[DamageRect] {
        &self.blur_regions
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

        let (selected_spans, matched_spans, selected_command_count) =
            self.select_matching_command_spans(|bounds| bounds.intersects(damage));

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

        let (selected_spans, matched_spans, selected_command_count) =
            self.select_matching_command_spans(|bounds| rects_intersect_any(bounds, damages));

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

    pub(in crate::display_list) fn select_matching_command_spans(
        &self,
        matches: impl Fn(DamageRect) -> bool,
    ) -> (Vec<SelectedCommandSpan>, u64, usize) {
        let mut selected = Vec::with_capacity(self.command_spans.len().min(32));
        let mut matched_spans = 0u64;
        for span in self.command_spans.iter() {
            if !matches(span.bounds) {
                continue;
            }
            matched_spans = matched_spans.saturating_add(1);
            insert_selected_command_span(
                &mut selected,
                SelectedCommandSpan {
                    start: span.start,
                    end: span.end,
                },
            );
        }
        if selected.is_empty() {
            for (index, command) in self.paint_commands.iter().enumerate() {
                if matches(command_bounds(command)) {
                    insert_selected_command_span(
                        &mut selected,
                        SelectedCommandSpan {
                            start: index,
                            end: index.saturating_add(1),
                        },
                    );
                }
            }
        }
        self.widen_selection_to_layer_scopes(&mut selected);
        let command_count = selected
            .iter()
            .map(|span| span.end.saturating_sub(span.start))
            .sum();
        (selected, matched_spans, command_count)
    }

    /// Replaying half an effect layer would drop either its push (changing the
    /// compositing/filter semantics) or its pop (leaking the layer into later
    /// commands).
    pub(in crate::display_list) fn widen_selection_to_layer_scopes(
        &self,
        selected: &mut Vec<SelectedCommandSpan>,
    ) {
        if self.layer_scopes.is_empty() || selected.is_empty() {
            return;
        }
        loop {
            let mut widened = None;
            for &(start, end) in &self.layer_scopes {
                if let Some(span) = selected
                    .iter()
                    .find(|span| span.start < end && start < span.end)
                    .filter(|span| span.start > start || span.end < end)
                {
                    widened = Some(SelectedCommandSpan {
                        start: span.start.min(start),
                        end: span.end.max(end),
                    });
                    break;
                }
            }
            let Some(span) = widened else {
                return;
            };
            insert_selected_command_span(selected, span);
        }
    }

    pub(in crate::display_list) fn local_reuse_decision(
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

    pub(in crate::display_list) fn can_patch_sparse_entries(
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

fn paint_topology_changed(previous: &[DisplayPaintCommand], next: &[DisplayPaintCommand]) -> bool {
    previous.len() != next.len()
        || previous
            .iter()
            .zip(next)
            .any(|(previous, next)| previous.node.id != next.node.id || previous.kind != next.kind)
}

#[cfg(test)]
mod tests;
