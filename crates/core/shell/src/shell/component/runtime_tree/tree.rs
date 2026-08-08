use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::shell::component) struct RetainedTreeDirtySummary {
    pub(in crate::shell::component) inserted: usize,
    pub(in crate::shell::component) removed: usize,
    pub(in crate::shell::component) layout: usize,
    pub(in crate::shell::component) style: usize,
    pub(in crate::shell::component) attributes: usize,
    pub(in crate::shell::component) children: usize,
    pub(in crate::shell::component) state: usize,
    /// Bitmask of state bits that flipped this frame (old_state ^ new_state),
    /// OR'd across all nodes that had STATE dirty. Zero if no state changed.
    /// Bits correspond to STATE_HOVERED, STATE_FOCUSED, STATE_ACTIVE, etc.
    pub(in crate::shell::component) changed_state_bits: u32,
}

impl RetainedTreeDirtySummary {
    pub(in crate::shell::component) fn any(self) -> bool {
        self.inserted > 0
            || self.removed > 0
            || self.layout > 0
            || self.style > 0
            || self.attributes > 0
            || self.children > 0
            || self.state > 0
    }

    pub(super) fn add_flags(&mut self, flags: RetainedNodeDirtyFlags) {
        if flags.contains(RetainedNodeDirtyFlags::LAYOUT) {
            self.layout += 1;
        }
        if flags.contains(RetainedNodeDirtyFlags::STYLE) {
            self.style += 1;
        }
        if flags.contains(RetainedNodeDirtyFlags::ATTRIBUTES) {
            self.attributes += 1;
        }
        if flags.contains(RetainedNodeDirtyFlags::CHILDREN) {
            self.children += 1;
        }
        if flags.contains(RetainedNodeDirtyFlags::STATE) {
            self.state += 1;
        }
    }

    pub(in crate::shell::component) fn to_debug_counts(
        self,
    ) -> mesh_core_debug::RetainedInvalidationCounts {
        mesh_core_debug::RetainedInvalidationCounts {
            inserted: self.inserted as u64,
            removed: self.removed as u64,
            layout: self.layout as u64,
            style: self.style as u64,
            attributes: self.attributes as u64,
            children: self.children as u64,
            state: self.state as u64,
        }
    }
}

new_key_type! {
    pub(in crate::shell::component) struct RetainedNodeKey;
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(in crate::shell::component) struct RetainedNodeDirtyFlags: u16 {
        const INSERTED = 1 << 0;
        const LAYOUT = 1 << 1;
        const STYLE = 1 << 2;
        const ATTRIBUTES = 1 << 3;
        const CHILDREN = 1 << 4;
        const STATE = 1 << 5;
    }
}

#[derive(Debug, Default)]
pub(in crate::shell::component) struct RetainedWidgetTree {
    pub(super) generation: u64,
    pub(super) update_epoch: u64,
    pub(super) nodes: SlotMap<RetainedNodeKey, RetainedNodeSnapshot>,
    pub(super) node_keys: HashMap<NodeId, RetainedNodeKey>,
    pub(super) dirty: SecondaryMap<RetainedNodeKey, RetainedNodeDirtyFlags>,
    pub(super) dirty_node_ids: HashSet<NodeId>,
    pub(super) render_dirty: RenderObjectDirtySummary,
    pub(super) render_dirty_node_ids: HashSet<NodeId>,
    pub(super) last_dirty: RetainedTreeDirtySummary,
    // Dirty slots are transient but interaction frames repopulate them often.
    // Swap the previous map into scratch so its slot allocation is retained.
    pub(super) next_dirty_scratch: SecondaryMap<RetainedNodeKey, RetainedNodeDirtyFlags>,
    pub(super) next_dirty_node_ids_scratch: HashSet<NodeId>,
    pub(super) next_render_dirty_node_ids_scratch: HashSet<NodeId>,
    #[cfg(test)]
    pub(super) last_update_was_scoped: bool,
}

impl RetainedWidgetTree {
    pub(in crate::shell::component) fn update(
        &mut self,
        root: &WidgetNode,
    ) -> RetainedTreeDirtySummary {
        self.update_inner(root, true)
    }

    #[cfg(test)]
    pub(super) fn update_without_render_fingerprints(
        &mut self,
        root: &WidgetNode,
    ) -> RetainedTreeDirtySummary {
        self.update_inner(root, false)
    }

    pub(super) fn update_inner(
        &mut self,
        root: &WidgetNode,
        synchronize_render_fingerprints: bool,
    ) -> RetainedTreeDirtySummary {
        let _span = tracing::debug_span!("retained_tree_update").entered();
        #[cfg(test)]
        {
            self.last_update_was_scoped = false;
        }
        let mut dirty = RetainedTreeDirtySummary::default();
        let mut next_dirty = std::mem::take(&mut self.next_dirty_scratch);
        next_dirty.clear();
        let mut next_dirty_node_ids = std::mem::take(&mut self.next_dirty_node_ids_scratch);
        next_dirty_node_ids.clear();
        let mut render_dirty = RenderObjectDirtySummary::default();
        let mut next_render_dirty_node_ids =
            std::mem::take(&mut self.next_render_dirty_node_ids_scratch);
        next_render_dirty_node_ids.clear();
        let retained_len = self.nodes.len();
        if self.update_epoch == u64::MAX {
            self.update_epoch = 0;
            for snapshot in self.nodes.values_mut() {
                snapshot.last_seen_epoch = 0;
            }
        }
        self.update_epoch += 1;
        let update_epoch = self.update_epoch;

        let visited = update_retained_snapshots(
            root,
            &mut self.nodes,
            &mut self.node_keys,
            update_epoch,
            &mut next_dirty,
            &mut next_dirty_node_ids,
            &mut dirty,
            &mut render_dirty,
            &mut next_render_dirty_node_ids,
            synchronize_render_fingerprints,
        );

        // A non-structural pass visits exactly the retained slot count and
        // cannot leave stale nodes. Only structural changes pay for the map
        // scan; live slots carry the current traversal epoch.
        if self.nodes.len() != retained_len || visited != retained_len {
            self.node_keys.retain(|_, key| {
                if self
                    .nodes
                    .get(*key)
                    .is_some_and(|snapshot| snapshot.last_seen_epoch == update_epoch)
                {
                    return true;
                }
                self.nodes.remove(*key);
                dirty.removed += 1;
                render_dirty.removed += 1;
                false
            });
        }

        if dirty.any() || render_dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        let previous_dirty = std::mem::replace(&mut self.dirty, next_dirty);
        self.next_dirty_scratch = previous_dirty;
        let previous_dirty_node_ids =
            std::mem::replace(&mut self.dirty_node_ids, next_dirty_node_ids);
        self.next_dirty_node_ids_scratch = previous_dirty_node_ids;
        let previous_render_dirty_node_ids =
            std::mem::replace(&mut self.render_dirty_node_ids, next_render_dirty_node_ids);
        self.next_render_dirty_node_ids_scratch = previous_render_dirty_node_ids;
        self.render_dirty = render_dirty;
        self.last_dirty = dirty;
        dirty
    }

    /// Updates authoritative dirty subtrees while checking clean-node layout.
    ///
    /// `dirty_roots` is valid only when the caller retained the existing tree
    /// structure and changed style/attributes/state exclusively at those roots
    /// or below them. Clean nodes still compare their cheap layout fingerprint,
    /// because layout changes can propagate from a dirty leaf to ancestors and
    /// siblings. A structural mismatch falls back to the full update before any
    /// retained snapshot is mutated.
    pub(in crate::shell::component) fn update_for_dirty_roots(
        &mut self,
        root: &WidgetNode,
        dirty_roots: &HashSet<NodeId>,
    ) -> RetainedTreeDirtySummary {
        self.update_for_dirty_roots_collect(root, dirty_roots).0
    }

    /// Scoped update variant that also returns direct references to nodes whose
    /// retained fingerprints changed. Downstream retained layers can consume
    /// these references without walking the entire widget tree again.
    ///
    /// The second tuple field is `None` when the scope was promoted to the full
    /// update path, since structural and broad updates require downstream full
    /// synchronization as well.
    pub(in crate::shell::component) fn update_for_dirty_roots_collect<'a>(
        &mut self,
        root: &'a WidgetNode,
        dirty_roots: &HashSet<NodeId>,
    ) -> (
        RetainedTreeDirtySummary,
        Option<SmallVec<[&'a WidgetNode; 8]>>,
    ) {
        if self.nodes.is_empty() || (self.nodes.len() >= 64 && dirty_roots.contains(&root.id)) {
            return (self.update(root), None);
        }

        let mut update_nodes = Vec::new();
        if !collect_scoped_update_nodes(
            root,
            false,
            dirty_roots,
            &self.nodes,
            &self.node_keys,
            &mut update_nodes,
        ) {
            return (self.update(root), None);
        }
        // Once half the retained tree needs full fingerprints, the scoped
        // bookkeeping no longer repays its extra traversal. Promote broad
        // updates before mutating retained state.
        if self.nodes.len() >= 64 && update_nodes.len().saturating_mul(2) >= self.nodes.len() {
            return (self.update(root), None);
        }

        let mut dirty = RetainedTreeDirtySummary::default();
        let mut next_dirty = std::mem::take(&mut self.next_dirty_scratch);
        next_dirty.clear();
        let mut next_dirty_node_ids = std::mem::take(&mut self.next_dirty_node_ids_scratch);
        next_dirty_node_ids.clear();
        let mut render_dirty = RenderObjectDirtySummary::default();
        let mut next_render_dirty_node_ids =
            std::mem::take(&mut self.next_render_dirty_node_ids_scratch);
        next_render_dirty_node_ids.clear();
        if self.update_epoch == u64::MAX {
            self.update_epoch = 0;
            for snapshot in self.nodes.values_mut() {
                snapshot.last_seen_epoch = 0;
            }
        }
        self.update_epoch += 1;
        let update_epoch = self.update_epoch;

        let mut dirty_nodes = SmallVec::new();
        for node in update_nodes {
            if update_retained_node(
                node,
                &mut self.nodes,
                &mut self.node_keys,
                update_epoch,
                &mut next_dirty,
                &mut next_dirty_node_ids,
                &mut dirty,
                &mut render_dirty,
                &mut next_render_dirty_node_ids,
                true,
            ) {
                dirty_nodes.push(node);
            }
        }

        if dirty.any() || render_dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        let previous_dirty = std::mem::replace(&mut self.dirty, next_dirty);
        self.next_dirty_scratch = previous_dirty;
        let previous_dirty_node_ids =
            std::mem::replace(&mut self.dirty_node_ids, next_dirty_node_ids);
        self.next_dirty_node_ids_scratch = previous_dirty_node_ids;
        let previous_render_dirty_node_ids =
            std::mem::replace(&mut self.render_dirty_node_ids, next_render_dirty_node_ids);
        self.next_render_dirty_node_ids_scratch = previous_render_dirty_node_ids;
        self.render_dirty = render_dirty;
        self.last_dirty = dirty;
        #[cfg(test)]
        {
            self.last_update_was_scoped = true;
        }
        (dirty, Some(dirty_nodes))
    }

    pub(in crate::shell::component) fn generation(&self) -> u64 {
        self.generation
    }

    pub(in crate::shell::component) fn last_dirty(&self) -> RetainedTreeDirtySummary {
        self.last_dirty
    }

    /// Existing node IDs marked dirty by the most recent authoritative diff.
    ///
    /// Insertions are intentionally omitted: structural updates take the full
    /// downstream synchronization path and do not consume this sparse set.
    pub(in crate::shell::component) fn dirty_node_ids(&self) -> &HashSet<NodeId> {
        &self.dirty_node_ids
    }

    pub(in crate::shell::component) fn render_dirty(&self) -> RenderObjectDirtySummary {
        self.render_dirty
    }

    pub(in crate::shell::component) fn render_dirty_node_ids(&self) -> &HashSet<NodeId> {
        &self.render_dirty_node_ids
    }

    #[cfg(test)]
    pub(in crate::shell::component) fn last_update_was_scoped(&self) -> bool {
        self.last_update_was_scoped
    }

    #[cfg(test)]
    pub(in crate::shell::component) fn is_node_dirty(&self, node_id: NodeId) -> bool {
        self.node_keys
            .get(&node_id)
            .is_some_and(|key| self.dirty.contains_key(*key))
    }

    pub(in crate::shell::component) fn layout_dirty_node_snapshots(
        &self,
        root: &WidgetNode,
    ) -> Option<Vec<WidgetNode>> {
        // The result is normally sparse, so only the changed nodes are cloned
        // into owned COW snapshots. The live tree must be mutably borrowed by
        // the layout engine while these snapshots are consumed.
        fn collect(
            retained: &RetainedWidgetTree,
            node: &WidgetNode,
            dirty_nodes: &mut Vec<WidgetNode>,
            total: &mut usize,
        ) -> bool {
            let Some(key) = retained.node_keys.get(&node.id).copied() else {
                return false;
            };
            let Some(previous) = retained.nodes.get(key) else {
                return false;
            };
            if previous.child_ids.len() != node.children.len()
                || previous
                    .child_ids
                    .iter()
                    .zip(&node.children)
                    .any(|(previous_id, child)| *previous_id != child.id)
            {
                return false;
            }

            let fresh = retained_snapshot_with_render(node, previous.render.clone());
            let (flags, _) = previous.diff_flags(&fresh);
            *total += 1;
            if flags.intersects(RetainedNodeDirtyFlags::INSERTED | RetainedNodeDirtyFlags::CHILDREN)
            {
                return false;
            }
            if flags.intersects(
                RetainedNodeDirtyFlags::LAYOUT
                    | RetainedNodeDirtyFlags::STYLE
                    | RetainedNodeDirtyFlags::ATTRIBUTES,
            ) {
                dirty_nodes.push(node.clone());
            }

            node.children
                .iter()
                .all(|child| collect(retained, child, dirty_nodes, total))
        }

        let mut dirty_nodes = Vec::with_capacity(self.node_keys.len().min(256));
        let mut total = 0;
        if !collect(self, root, &mut dirty_nodes, &mut total) || total != self.node_keys.len() {
            return None;
        }
        Some(dirty_nodes)
    }

    #[cfg(test)]
    pub(super) fn dirty_flags_for(&self, node_id: NodeId) -> RetainedNodeDirtyFlags {
        self.node_keys
            .get(&node_id)
            .and_then(|key| self.dirty.get(*key))
            .copied()
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(super) fn retained_key_for_node_id(&self, node_id: NodeId) -> Option<RetainedNodeKey> {
        self.node_keys.get(&node_id).copied()
    }

    #[cfg(test)]
    pub(in crate::shell::component) fn narrow_script_diff(
        &self,
        root: &WidgetNode,
    ) -> Option<(HashSet<NodeId>, usize)> {
        let mut affected = HashSet::with_capacity(self.node_keys.len().min(256));
        let total = self.visit_fresh_snapshots(root, &mut |node_id, previous, fresh| {
            let (flags, _) = previous.diff_flags(fresh);
            if flags.is_empty() {
                return true;
            }
            if flags.contains(RetainedNodeDirtyFlags::CHILDREN) {
                return false; // structural change
            }
            let ancestor_only_flags =
                RetainedNodeDirtyFlags::LAYOUT | RetainedNodeDirtyFlags::ATTRIBUTES;
            if !fresh.child_ids.is_empty() && flags.difference(ancestor_only_flags).is_empty() {
                return true;
            }
            affected.insert(node_id);
            true
        })?;

        (total == self.node_keys.len()).then_some((affected, total))
    }

    /// Compare a fresh widget tree directly with the retained slotmap.
    ///
    /// The analysis callers only need each node's previous snapshot; they do
    /// not need a second `NodeId -> snapshot` table. Walking the tree directly
    /// avoids allocating and populating that temporary map on every narrow or
    /// layout analysis pass. Returning the visited count preserves detection
    /// of removed nodes, while a missing retained key detects inserted nodes.
    pub(super) fn visit_fresh_snapshots(
        &self,
        node: &WidgetNode,
        visit: &mut impl FnMut(NodeId, &RetainedNodeSnapshot, &RetainedNodeSnapshot) -> bool,
    ) -> Option<usize> {
        fn walk(
            retained: &RetainedWidgetTree,
            node: &WidgetNode,
            visit: &mut impl FnMut(NodeId, &RetainedNodeSnapshot, &RetainedNodeSnapshot) -> bool,
            total: &mut usize,
        ) -> bool {
            let Some(key) = retained.node_keys.get(&node.id).copied() else {
                return false;
            };
            let Some(previous) = retained.nodes.get(key) else {
                return false;
            };
            let fresh = retained_snapshot_with_render(node, previous.render.clone());
            *total += 1;
            if !visit(node.id, previous, &fresh) {
                return false;
            }
            node.children
                .iter()
                .all(|child| walk(retained, child, visit, total))
        }

        let mut total = 0;
        walk(self, node, visit, &mut total).then_some(total)
    }
}

/// Finds non-structural changes without hashing clean nodes.
///
/// Narrow script builds already produce a fresh tree while retaining the last
/// painted tree. Direct equality checks are substantially cheaper than building
/// retained fingerprints for every clean node. The retained scoped update will
/// still validate child identity and compare layout across the whole tree.
pub(in crate::shell::component) fn narrow_script_dirty_roots(
    previous: &WidgetNode,
    fresh: &WidgetNode,
) -> Option<HashSet<NodeId>> {
    fn collect(
        previous: &WidgetNode,
        fresh: &WidgetNode,
        dirty_roots: &mut HashSet<NodeId>,
    ) -> bool {
        if previous.id != fresh.id
            || previous.children.len() != fresh.children.len()
            || previous
                .children
                .iter()
                .zip(&fresh.children)
                .any(|(previous_child, fresh_child)| previous_child.id != fresh_child.id)
        {
            return false;
        }

        if previous.tag != fresh.tag
            || previous.computed_style != fresh.computed_style
            || previous.attributes != fresh.attributes
            || previous.event_handlers != fresh.event_handlers
            || previous.event_handler_calls != fresh.event_handler_calls
            || previous.state != fresh.state
        {
            dirty_roots.insert(fresh.id);
            // The scoped retained pass fingerprints this complete subtree and
            // independently validates its structure, so descendants need no
            // further comparison here.
            return true;
        }

        previous
            .children
            .iter()
            .zip(&fresh.children)
            .all(|(previous_child, fresh_child)| collect(previous_child, fresh_child, dirty_roots))
    }

    let mut dirty_roots = HashSet::with_capacity(8);
    collect(previous, fresh, &mut dirty_roots).then_some(dirty_roots)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetainedNodeSnapshot {
    pub(super) layout: LayoutFingerprint,
    pub(super) style_hash: u64,
    pub(super) attributes_hash: u64,
    pub(super) child_ids: SmallVec<[NodeId; 8]>,
    pub(super) state: ElementState,
    pub(super) render: RenderObjectFingerprint,
    pub(super) last_seen_epoch: u64,
}

pub(super) type LayoutFingerprint = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32);

impl RetainedNodeSnapshot {
    pub(super) fn diff_flags(&self, next: &Self) -> (RetainedNodeDirtyFlags, u32) {
        let mut flags = RetainedNodeDirtyFlags::empty();
        if self.layout != next.layout {
            flags |= RetainedNodeDirtyFlags::LAYOUT;
        }
        if self.style_hash != next.style_hash {
            flags |= RetainedNodeDirtyFlags::STYLE;
        }
        if self.attributes_hash != next.attributes_hash {
            flags |= RetainedNodeDirtyFlags::ATTRIBUTES;
        }
        if self.child_ids != next.child_ids {
            flags |= RetainedNodeDirtyFlags::CHILDREN;
        }
        let changed_state_bits = if self.state != next.state {
            flags |= RetainedNodeDirtyFlags::STATE;
            state_bitmask(self.state) ^ state_bitmask(next.state)
        } else {
            0
        };
        (flags, changed_state_bits)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update_retained_snapshots(
    node: &WidgetNode,
    nodes: &mut SlotMap<RetainedNodeKey, RetainedNodeSnapshot>,
    node_keys: &mut HashMap<NodeId, RetainedNodeKey>,
    update_epoch: u64,
    dirty_slots: &mut SecondaryMap<RetainedNodeKey, RetainedNodeDirtyFlags>,
    dirty_node_ids: &mut HashSet<NodeId>,
    dirty: &mut RetainedTreeDirtySummary,
    render_dirty: &mut RenderObjectDirtySummary,
    render_dirty_node_ids: &mut HashSet<NodeId>,
    synchronize_render_fingerprints: bool,
) -> usize {
    update_retained_node(
        node,
        nodes,
        node_keys,
        update_epoch,
        dirty_slots,
        dirty_node_ids,
        dirty,
        render_dirty,
        render_dirty_node_ids,
        synchronize_render_fingerprints,
    );

    let mut visited = 1;
    for child in &node.children {
        visited += update_retained_snapshots(
            child,
            nodes,
            node_keys,
            update_epoch,
            dirty_slots,
            dirty_node_ids,
            dirty,
            render_dirty,
            render_dirty_node_ids,
            synchronize_render_fingerprints,
        );
    }
    visited
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update_retained_node(
    node: &WidgetNode,
    nodes: &mut SlotMap<RetainedNodeKey, RetainedNodeSnapshot>,
    node_keys: &mut HashMap<NodeId, RetainedNodeKey>,
    update_epoch: u64,
    dirty_slots: &mut SecondaryMap<RetainedNodeKey, RetainedNodeDirtyFlags>,
    dirty_node_ids: &mut HashSet<NodeId>,
    dirty: &mut RetainedTreeDirtySummary,
    render_dirty: &mut RenderObjectDirtySummary,
    render_dirty_node_ids: &mut HashSet<NodeId>,
    synchronize_render_fingerprints: bool,
) -> bool {
    match node_keys.get(&node.id).copied() {
        Some(previous_key) => match nodes.get_mut(previous_key) {
            Some(previous) => {
                assert_ne!(
                    previous.last_seen_epoch,
                    update_epoch,
                    "runtime NodeId collision while updating retained snapshots: id={} key={:?}",
                    node.id,
                    node.mesh_key()
                );
                let mut next = retained_snapshot_with_render(node, previous.render.clone());
                next.last_seen_epoch = update_epoch;
                let (flags, node_state_bits) = previous.diff_flags(&next);
                let render_changed = if flags.is_empty() || !synchronize_render_fingerprints {
                    false
                } else {
                    next.render = RenderObjectFingerprint::for_node(node, Some(&previous.render));
                    render_dirty.add_fingerprint_diff(&previous.render, &next.render)
                };
                if synchronize_render_fingerprints
                    && flags.contains(RetainedNodeDirtyFlags::CHILDREN)
                {
                    render_dirty.reordered += 1;
                }
                if render_changed
                    || synchronize_render_fingerprints
                        && flags.contains(RetainedNodeDirtyFlags::CHILDREN)
                {
                    render_dirty_node_ids.insert(node.id);
                }
                if !flags.is_empty() {
                    dirty.add_flags(flags);
                    dirty.changed_state_bits |= node_state_bits;
                    dirty_slots.insert(previous_key, flags);
                    dirty_node_ids.insert(node.id);
                    *previous = next;
                    true
                } else if render_changed {
                    *previous = next;
                    true
                } else {
                    previous.last_seen_epoch = update_epoch;
                    false
                }
            }
            None => {
                let mut next = retained_snapshot(node);
                next.last_seen_epoch = update_epoch;
                let key = nodes.insert(next);
                node_keys.insert(node.id, key);
                dirty_slots.insert(key, RetainedNodeDirtyFlags::INSERTED);
                dirty.inserted += 1;
                render_dirty.inserted += 1;
                render_dirty_node_ids.insert(node.id);
                true
            }
        },
        None => {
            let mut next = retained_snapshot(node);
            next.last_seen_epoch = update_epoch;
            let key = nodes.insert(next);
            node_keys.insert(node.id, key);
            dirty_slots.insert(key, RetainedNodeDirtyFlags::INSERTED);
            dirty.inserted += 1;
            render_dirty.inserted += 1;
            render_dirty_node_ids.insert(node.id);
            true
        }
    }
}

pub(super) fn collect_scoped_update_nodes<'a>(
    node: &'a WidgetNode,
    ancestor_is_dirty: bool,
    dirty_roots: &HashSet<NodeId>,
    nodes: &SlotMap<RetainedNodeKey, RetainedNodeSnapshot>,
    node_keys: &HashMap<NodeId, RetainedNodeKey>,
    update_nodes: &mut Vec<&'a WidgetNode>,
) -> bool {
    let Some(previous) = node_keys.get(&node.id).and_then(|key| nodes.get(*key)) else {
        return false;
    };
    if previous.child_ids.len() != node.children.len()
        || previous
            .child_ids
            .iter()
            .zip(&node.children)
            .any(|(previous_id, child)| *previous_id != child.id)
    {
        return false;
    }

    let node_is_dirty = ancestor_is_dirty || dirty_roots.contains(&node.id);
    if node_is_dirty || previous.layout != layout_fingerprint(node) {
        update_nodes.push(node);
    }
    node.children.iter().all(|child| {
        collect_scoped_update_nodes(
            child,
            node_is_dirty,
            dirty_roots,
            nodes,
            node_keys,
            update_nodes,
        )
    })
}

#[cfg(test)]
pub(super) fn collect_retained_snapshots(
    node: &WidgetNode,
    snapshots: &mut HashMap<NodeId, RetainedNodeSnapshot>,
) {
    let previous = snapshots.insert(node.id, retained_snapshot(node));
    assert!(
        previous.is_none(),
        "runtime NodeId collision while collecting retained snapshots: id={} key={:?}",
        node.id,
        node.mesh_key()
    );
    for child in &node.children {
        collect_retained_snapshots(child, snapshots);
    }
}
