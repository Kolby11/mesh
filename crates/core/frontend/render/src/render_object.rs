#[cfg(debug_assertions)]
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
#[cfg(debug_assertions)]
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use mesh_core_elements::style::{Transform2D, TransformOrigin, TransformOriginValue};
use mesh_core_elements::{AccessibilityRole, NodeId, WidgetNode, live_accessibility_focus};

use crate::paint_input::{PaintInput, retained_arc_str};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderObjectDirtySummary {
    pub inserted: usize,
    pub removed: usize,
    pub reordered: usize,
    pub transform: usize,
    pub clip: usize,
    pub opacity: usize,
    pub geometry: usize,
    /// Offset-only changes; layout and scroll extents remain geometry dirt.
    pub scroll: usize,
    pub material: usize,
    pub primitive: usize,
    pub text: usize,
    pub accessibility: usize,
}

impl RenderObjectDirtySummary {
    pub fn any(self) -> bool {
        self.inserted > 0
            || self.removed > 0
            || self.reordered > 0
            || self.transform > 0
            || self.clip > 0
            || self.opacity > 0
            || self.geometry > 0
            || self.scroll > 0
            || self.material > 0
            || self.primitive > 0
            || self.text > 0
            || self.accessibility > 0
    }
}

#[derive(Debug, Default)]
pub struct RenderObjectTree {
    generation: u64,
    retained_tree_generation: Option<u64>,
    #[cfg(debug_assertions)]
    retained_caller_lineage: Option<u64>,
    update_epoch: u64,
    objects: HashMap<NodeId, RenderObjectPaintData>,
    root: Option<NodeId>,
    last_dirty: RenderObjectDirtySummary,
    dirty_nodes: HashSet<NodeId>,
}

impl RenderObjectTree {
    pub fn update(&mut self, root: &WidgetNode) -> RenderObjectDirtySummary {
        self.update_inner(root, None)
    }

    pub fn update_for_retained_generation(
        &mut self,
        root: &WidgetNode,
        retained_tree_generation: u64,
    ) -> RenderObjectDirtySummary {
        if self.can_skip_for_retained_generation(root, retained_tree_generation) {
            self.last_dirty = RenderObjectDirtySummary::default();
            self.dirty_nodes.clear();
            return self.last_dirty;
        }
        self.update_inner(root, Some(retained_tree_generation))
    }

    /// Updates only nodes marked dirty by an authoritative retained-tree diff.
    ///
    /// The caller must use this only for non-structural updates, where node
    /// identity and parent/child relationships are unchanged. We still walk
    /// the tree to avoid building another node lookup table, but clean nodes
    /// bypass paint-data construction and its style/attribute hashing.
    pub fn update_for_retained_dirty_nodes(
        &mut self,
        root: &WidgetNode,
        retained_tree_generation: u64,
        retained_dirty_node_ids: &HashSet<NodeId>,
    ) -> RenderObjectDirtySummary {
        if self.can_skip_for_retained_generation(root, retained_tree_generation) {
            self.last_dirty = RenderObjectDirtySummary::default();
            self.dirty_nodes.clear();
            return self.last_dirty;
        }
        if self.retained_tree_generation == Some(retained_tree_generation) {
            return self.update_inner(root, Some(retained_tree_generation));
        }

        if self.root != Some(root.id) || self.objects.is_empty() {
            return self.update_inner(root, Some(retained_tree_generation));
        }

        let mut dirty = RenderObjectDirtySummary::default();
        let mut dirty_nodes = std::mem::take(&mut self.dirty_nodes);
        dirty_nodes.clear();
        update_dirty_render_objects(
            root,
            None,
            &mut self.objects,
            retained_dirty_node_ids,
            &mut dirty,
            &mut dirty_nodes,
        );

        if dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        self.retained_tree_generation = Some(retained_tree_generation);
        #[cfg(debug_assertions)]
        {
            self.retained_caller_lineage = Some(caller_lineage_fingerprint(root));
        }
        self.last_dirty = dirty;
        self.dirty_nodes = dirty_nodes;
        dirty
    }

    /// Updates retained render objects from direct references produced by the
    /// authoritative widget-tree diff. This is the sparse counterpart to
    /// `update_for_retained_dirty_nodes`: it avoids another full-tree walk and
    /// one hash-set membership probe per clean node.
    pub fn update_for_retained_dirty_node_refs(
        &mut self,
        root: &WidgetNode,
        retained_tree_generation: u64,
        retained_dirty_nodes: &[&WidgetNode],
    ) -> RenderObjectDirtySummary {
        if self.can_skip_for_retained_generation(root, retained_tree_generation) {
            self.last_dirty = RenderObjectDirtySummary::default();
            self.dirty_nodes.clear();
            return self.last_dirty;
        }
        if self.retained_tree_generation == Some(retained_tree_generation) {
            return self.update_inner(root, Some(retained_tree_generation));
        }

        if self.root != Some(root.id) || self.objects.is_empty() {
            return self.update_inner(root, Some(retained_tree_generation));
        }

        let mut dirty = RenderObjectDirtySummary::default();
        let mut dirty_nodes = std::mem::take(&mut self.dirty_nodes);
        dirty_nodes.clear();
        for node in retained_dirty_nodes {
            update_dirty_render_object(node, &mut self.objects, &mut dirty, &mut dirty_nodes);
        }

        if dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        self.retained_tree_generation = Some(retained_tree_generation);
        #[cfg(debug_assertions)]
        {
            self.retained_caller_lineage = Some(caller_lineage_fingerprint(root));
        }
        self.last_dirty = dirty;
        self.dirty_nodes = dirty_nodes;
        dirty
    }

    fn update_inner(
        &mut self,
        root: &WidgetNode,
        retained_tree_generation: Option<u64>,
    ) -> RenderObjectDirtySummary {
        let mut dirty = RenderObjectDirtySummary::default();
        let retained_len = self.objects.len();
        let mut dirty_nodes = std::mem::take(&mut self.dirty_nodes);
        dirty_nodes.clear();
        if dirty_nodes.capacity() < retained_len.min(1024) {
            dirty_nodes.reserve(retained_len.min(1024) - dirty_nodes.capacity());
        }
        if self.update_epoch == u64::MAX {
            self.update_epoch = 0;
            for object in self.objects.values_mut() {
                object.last_seen_epoch = 0;
            }
        }
        self.update_epoch += 1;
        let update_epoch = self.update_epoch;

        let visited = update_retained_render_objects(
            root,
            None,
            &mut self.objects,
            update_epoch,
            &mut dirty,
            &mut dirty_nodes,
        );

        // A normal non-structural update visits every retained object and
        // cannot leave stale entries behind. Avoid scanning the entire map
        // with `retain` unless the visit count or map length proves that the
        // tree structure changed.
        if self.objects.len() != retained_len || visited != retained_len {
            let before_remove = self.objects.len();
            self.objects
                .retain(|_, object| object.last_seen_epoch == update_epoch);
            dirty.removed = before_remove.saturating_sub(self.objects.len());
        }

        if self.root != Some(root.id) {
            dirty.reordered += usize::from(self.root.is_some());
        }
        if dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        self.root = Some(root.id);
        self.retained_tree_generation = retained_tree_generation;
        #[cfg(debug_assertions)]
        {
            self.retained_caller_lineage = Some(caller_lineage_fingerprint(root));
        }
        self.last_dirty = dirty;
        self.dirty_nodes = dirty_nodes;
        dirty
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn last_dirty(&self) -> RenderObjectDirtySummary {
        self.last_dirty
    }

    pub fn dirty_node_ids(&self) -> &HashSet<NodeId> {
        &self.dirty_nodes
    }

    fn can_skip_for_retained_generation(
        &self,
        root: &WidgetNode,
        retained_tree_generation: u64,
    ) -> bool {
        if self.retained_tree_generation != Some(retained_tree_generation) {
            return false;
        }

        self.caller_lineage_is_valid(root, retained_tree_generation)
    }

    fn caller_lineage_is_valid(&self, root: &WidgetNode, retained_tree_generation: u64) -> bool {
        #[cfg(not(debug_assertions))]
        let _ = (root, retained_tree_generation);

        #[cfg(debug_assertions)]
        {
            let caller_lineage = caller_lineage_fingerprint(root);
            if self.retained_caller_lineage != Some(caller_lineage) {
                tracing::warn!(
                    retained_tree_generation,
                    expected_lineage = ?self.retained_caller_lineage,
                    actual_lineage = caller_lineage,
                    "retained render-object generation changed caller lineage; rebuilding"
                );
                return false;
            }
        }

        true
    }
}

impl RenderObjectDirtySummary {
    pub fn add_fingerprint_diff(
        &mut self,
        previous: &RenderObjectFingerprint,
        next: &RenderObjectFingerprint,
    ) -> bool {
        let mut changed = false;
        if previous.transform != next.transform {
            self.transform += 1;
            changed = true;
        }
        if previous.clip != next.clip {
            self.clip += 1;
            changed = true;
        }
        if previous.paint.opacity != next.paint.opacity {
            self.opacity += 1;
            changed = true;
        }
        if previous.geometry != next.geometry {
            self.geometry += 1;
            changed = true;
        }
        if previous.scroll != next.scroll {
            self.scroll += 1;
            changed = true;
        }
        if previous.paint.material != next.paint.material
            || previous.paint.variables != next.paint.variables
        {
            self.material += 1;
            changed = true;
        }
        if previous.paint.primitive != next.paint.primitive
            || previous.paint.icon != next.paint.icon
        {
            self.primitive += 1;
            changed = true;
        }
        if previous.paint.text != next.paint.text {
            self.text += 1;
            changed = true;
        }
        if previous.accessibility != next.accessibility {
            self.accessibility += 1;
            changed = true;
        }
        changed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderObjectFingerprint {
    transform: TransformSlot,
    clip: ClipSlot,
    geometry: GeometrySlot,
    scroll: (u32, u32),
    pub(crate) paint: PaintInput,
    accessibility: AccessibilitySlot,
}

impl RenderObjectFingerprint {
    pub fn for_node(node: &WidgetNode, previous: Option<&Self>) -> Self {
        render_object_fingerprint(node, previous)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderObjectPaintData {
    last_seen_epoch: u64,
    parent: Option<NodeId>,
    child_ids: Vec<NodeId>,
    fingerprint: RenderObjectFingerprint,
}

type TransformSlot = (u32, u32, u32, u32, u32, u32, u32, u32, u32);
type GeometrySlot = (u32, u32, u32, u32, u32, u32, u32, u32);
type ClipSlot = (bool, u32, u32, u32, u32, [u32; 4]);
type AccessibilitySlot = (AccessibilityRoleSlot, Option<Arc<str>>, bool, bool);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AccessibilityRoleSlot {
    Builtin(u8),
    Custom(Arc<str>),
}

/// Computes a debug-only fingerprint of the tree a retained generation was
/// produced from. The generation remains the production fast-path authority;
/// this catches callers that reuse a plain generation number for a different
/// tree while making the violation observable before stale paint data escapes.
/// Global resource revisions are validated separately by the display list;
/// loading an icon or font does not change the caller's widget-tree lineage.
#[cfg(debug_assertions)]
pub(crate) fn caller_lineage_fingerprint(root: &WidgetNode) -> u64 {
    fn visit(node: &WidgetNode, hasher: &mut DefaultHasher) {
        node.id.hash(hasher);
        node.tag.hash(hasher);
        node.children.len().hash(hasher);
        let mut fingerprint = RenderObjectFingerprint::for_node(node, None);
        fingerprint.paint.resource_revision = 0;
        fingerprint.hash(hasher);
        for child in &node.children {
            visit(child, hasher);
        }
    }

    let mut hasher = DefaultHasher::new();
    visit(root, &mut hasher);
    hasher.finish()
}

fn update_retained_render_objects(
    node: &WidgetNode,
    parent: Option<NodeId>,
    objects: &mut HashMap<NodeId, RenderObjectPaintData>,
    update_epoch: u64,
    dirty: &mut RenderObjectDirtySummary,
    dirty_nodes: &mut HashSet<NodeId>,
) -> usize {
    match objects.get_mut(&node.id) {
        Some(current) => {
            let parent_changed = current.parent != parent;
            let (child_ids, children_changed) =
                refresh_child_id_slot(node, std::mem::take(&mut current.child_ids));
            if parent_changed || children_changed {
                dirty.reordered += 1;
            }
            let next = render_object_paint_data(node, parent, Some(current), child_ids);
            if dirty.add_fingerprint_diff(&current.fingerprint, &next.fingerprint)
                || parent_changed
                || children_changed
            {
                dirty_nodes.insert(node.id);
            }
            *current = RenderObjectPaintData {
                last_seen_epoch: update_epoch,
                ..next
            };
        }
        None => {
            let next = render_object_paint_data(node, parent, None, child_id_slot(node));
            dirty.inserted += 1;
            dirty_nodes.insert(node.id);
            objects.insert(
                node.id,
                RenderObjectPaintData {
                    last_seen_epoch: update_epoch,
                    ..next
                },
            );
        }
    }

    let mut visited = 1;
    for child in &node.children {
        visited += update_retained_render_objects(
            child,
            Some(node.id),
            objects,
            update_epoch,
            dirty,
            dirty_nodes,
        );
    }
    visited
}

fn update_dirty_render_objects(
    node: &WidgetNode,
    parent: Option<NodeId>,
    objects: &mut HashMap<NodeId, RenderObjectPaintData>,
    retained_dirty_node_ids: &HashSet<NodeId>,
    dirty: &mut RenderObjectDirtySummary,
    dirty_nodes: &mut HashSet<NodeId>,
) {
    if retained_dirty_node_ids.contains(&node.id) {
        update_dirty_render_object(node, objects, dirty, dirty_nodes);
        debug_assert_eq!(objects[&node.id].parent, parent);
    }

    for child in &node.children {
        update_dirty_render_objects(
            child,
            Some(node.id),
            objects,
            retained_dirty_node_ids,
            dirty,
            dirty_nodes,
        );
    }
}

fn update_dirty_render_object(
    node: &WidgetNode,
    objects: &mut HashMap<NodeId, RenderObjectPaintData>,
    dirty: &mut RenderObjectDirtySummary,
    dirty_nodes: &mut HashSet<NodeId>,
) {
    let current = objects
        .get_mut(&node.id)
        .expect("non-structural retained update must preserve render-object identity");
    debug_assert!(
        current
            .child_ids
            .iter()
            .copied()
            .eq(node.children.iter().map(|child| child.id))
    );
    let parent = current.parent;
    let child_ids = std::mem::take(&mut current.child_ids);
    let next = render_object_paint_data(node, parent, Some(current), child_ids);
    if dirty.add_fingerprint_diff(&current.fingerprint, &next.fingerprint) {
        dirty_nodes.insert(node.id);
    }
    *current = RenderObjectPaintData {
        last_seen_epoch: current.last_seen_epoch,
        ..next
    };
}

fn render_object_paint_data(
    node: &WidgetNode,
    parent: Option<NodeId>,
    previous: Option<&RenderObjectPaintData>,
    child_ids: Vec<NodeId>,
) -> RenderObjectPaintData {
    RenderObjectPaintData {
        last_seen_epoch: 0,
        parent,
        child_ids,
        fingerprint: render_object_fingerprint(node, previous.map(|data| &data.fingerprint)),
    }
}

fn render_object_fingerprint(
    node: &WidgetNode,
    previous: Option<&RenderObjectFingerprint>,
) -> RenderObjectFingerprint {
    let geometry = geometry_slot(node);
    RenderObjectFingerprint {
        transform: transform_slot(
            node.computed_style.transform,
            node.computed_style.transform_origin,
        ),
        clip: clip_slot(node, geometry),
        geometry,
        scroll: {
            let scroll = node.resolved_scroll_metrics();
            (scroll.x.to_bits(), scroll.y.to_bits())
        },
        paint: PaintInput::for_node(node, previous.map(|fingerprint| &fingerprint.paint)),
        accessibility: accessibility_slot(node, previous.map(|data| &data.accessibility)),
    }
}

fn child_id_slot(node: &WidgetNode) -> Vec<NodeId> {
    refill_child_id_slot(node, Vec::new())
}

fn refresh_child_id_slot(node: &WidgetNode, mut child_ids: Vec<NodeId>) -> (Vec<NodeId>, bool) {
    let unchanged = child_ids.len() == node.children.len()
        && child_ids
            .iter()
            .copied()
            .eq(node.children.iter().map(|child| child.id));
    if unchanged {
        return (child_ids, false);
    }
    child_ids.clear();
    child_ids.extend(node.children.iter().map(|child| child.id));
    (child_ids, true)
}

fn refill_child_id_slot(node: &WidgetNode, mut child_ids: Vec<NodeId>) -> Vec<NodeId> {
    child_ids.clear();
    child_ids.extend(node.children.iter().map(|child| child.id));
    child_ids
}

fn transform_slot(transform: Transform2D, origin: TransformOrigin) -> TransformSlot {
    let (origin_x_kind, origin_x_value) = origin_value_slot(origin.x);
    let (origin_y_kind, origin_y_value) = origin_value_slot(origin.y);
    (
        transform.translate_x.to_bits(),
        transform.translate_y.to_bits(),
        transform.scale_x.to_bits(),
        transform.scale_y.to_bits(),
        transform.rotation.to_bits(),
        origin_x_kind,
        origin_x_value,
        origin_y_kind,
        origin_y_value,
    )
}

fn origin_value_slot(value: TransformOriginValue) -> (u32, u32) {
    match value {
        TransformOriginValue::Percent(value) => (0, value.to_bits()),
        TransformOriginValue::Px(value) => (1, value.to_bits()),
    }
}

fn clip_slot(node: &WidgetNode, geometry: GeometrySlot) -> ClipSlot {
    let clips = node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents();
    let radius = node.computed_style.border_radius;
    let radii = if clips {
        [
            radius.top_left,
            radius.top_right,
            radius.bottom_right,
            radius.bottom_left,
        ]
        .map(f32::to_bits)
    } else {
        [0; 4]
    };
    (clips, geometry.0, geometry.1, geometry.2, geometry.3, radii)
}

fn geometry_slot(node: &WidgetNode) -> GeometrySlot {
    let layout = node.layout;
    let scroll = node.resolved_scroll_metrics();
    (
        layout.x.to_bits(),
        layout.y.to_bits(),
        layout.width.to_bits(),
        layout.height.to_bits(),
        scroll.max_x.to_bits(),
        scroll.max_y.to_bits(),
        scroll.content_width.to_bits(),
        scroll.content_height.to_bits(),
    )
}

fn accessibility_slot(
    node: &WidgetNode,
    previous: Option<&AccessibilitySlot>,
) -> AccessibilitySlot {
    (
        accessibility_role_slot(&node.accessibility.role, previous.map(|slot| &slot.0)),
        retained_arc_str(
            node.accessibility.label.as_deref(),
            previous.and_then(|slot| slot.1.as_ref()),
        ),
        node.accessibility.focusable,
        live_accessibility_focus(node),
    )
}

fn accessibility_role_slot(
    role: &AccessibilityRole,
    previous: Option<&AccessibilityRoleSlot>,
) -> AccessibilityRoleSlot {
    let slot = match role {
        AccessibilityRole::Button => 0,
        AccessibilityRole::Slider => 1,
        AccessibilityRole::Label => 2,
        AccessibilityRole::TextInput => 3,
        AccessibilityRole::Checkbox => 4,
        AccessibilityRole::Switch => 5,
        AccessibilityRole::Region => 6,
        AccessibilityRole::List => 7,
        AccessibilityRole::ListItem => 8,
        AccessibilityRole::Image => 9,
        AccessibilityRole::Toolbar => 10,
        AccessibilityRole::Menu => 11,
        AccessibilityRole::MenuItem => 12,
        AccessibilityRole::Dialog => 13,
        AccessibilityRole::Alert => 14,
        AccessibilityRole::Status => 15,
        AccessibilityRole::ProgressBar => 16,
        AccessibilityRole::Tab => 17,
        AccessibilityRole::TabPanel => 18,
        AccessibilityRole::Separator => 19,
        AccessibilityRole::Custom(value) => {
            let previous = match previous {
                Some(AccessibilityRoleSlot::Custom(value)) => Some(value),
                _ => None,
            };
            return AccessibilityRoleSlot::Custom(
                retained_arc_str(Some(value), previous).expect("custom role value"),
            );
        }
    };
    AccessibilityRoleSlot::Builtin(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::WidgetNode;
    use mesh_core_elements::style::Color;
    use std::time::Instant;

    fn retained_visual_node() -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.id = 1;
        node.layout.x = 10.0;
        node.layout.y = 20.0;
        node.layout.width = 100.0;
        node.layout.height = 40.0;
        node
    }

    #[test]
    fn stable_child_id_slots_reuse_their_backing_storage() {
        let mut root = retained_visual_node();
        root.children = (2..=17)
            .map(|id| {
                let mut child = WidgetNode::new("text");
                child.id = id;
                child
            })
            .collect();
        let slot = child_id_slot(&root);
        let pointer = slot.as_ptr();

        let (slot, changed) = refresh_child_id_slot(&root, slot);

        assert!(!changed);
        assert_eq!(slot.as_ptr(), pointer);
        assert_eq!(
            slot,
            root.children
                .iter()
                .map(|child| child.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn changed_child_id_slots_are_refilled() {
        let mut root = retained_visual_node();
        root.children = (2..=5)
            .map(|id| {
                let mut child = WidgetNode::new("text");
                child.id = id;
                child
            })
            .collect();
        let mut slot = child_id_slot(&root);
        root.children.swap(1, 2);

        let (refreshed, changed) = refresh_child_id_slot(&root, std::mem::take(&mut slot));

        assert!(changed);
        assert_eq!(
            refreshed,
            root.children
                .iter()
                .map(|child| child.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn render_object_focus_snapshot_uses_live_state() {
        let mut node = retained_visual_node();
        node.accessibility.focusable = true;
        node.accessibility.focused = true;
        node.state.focused = false;

        let mut tree = RenderObjectTree::default();
        tree.update(&node);
        assert!(
            !tree
                .objects
                .get(&node.id)
                .expect("initial render object")
                .fingerprint
                .accessibility
                .3
        );

        node.state.focused = true;
        let dirty = tree.update(&node);
        assert_eq!(dirty.accessibility, 1);
        assert!(
            tree.objects
                .get(&node.id)
                .expect("focused render object")
                .fingerprint
                .accessibility
                .3
        );
    }

    // cargo test -p mesh-core-render --release -- stable_child_id_reuse_beats_rewriting_slots --ignored --nocapture
    #[test]
    #[ignore = "release-only render-object child-ID slot microbenchmark"]
    fn stable_child_id_reuse_beats_rewriting_slots() {
        let mut root = retained_visual_node();
        root.children = (2..=257)
            .map(|id| {
                let mut child = WidgetNode::new("text");
                child.id = id;
                child
            })
            .collect();
        let expected = root
            .children
            .iter()
            .map(|child| child.id)
            .collect::<Vec<_>>();
        let iterations = 200_000;

        let mut rewritten = expected.clone();
        let rewrite_started = Instant::now();
        let mut rewrite_total = 0u64;
        for _ in 0..iterations {
            let unchanged = rewritten.len() == root.children.len()
                && rewritten
                    .iter()
                    .copied()
                    .eq(root.children.iter().map(|child| child.id));
            std::hint::black_box(unchanged);
            rewritten.clear();
            rewritten.extend(root.children.iter().map(|child| child.id));
            rewrite_total = rewrite_total.wrapping_add(std::hint::black_box(
                rewritten.last().copied().unwrap_or_default(),
            ));
        }
        let rewrite_time = rewrite_started.elapsed();

        let mut reused = expected.clone();
        let reuse_started = Instant::now();
        let mut reuse_total = 0u64;
        for _ in 0..iterations {
            let (next, changed) = refresh_child_id_slot(&root, reused);
            reused = next;
            std::hint::black_box(changed);
            reuse_total = reuse_total.wrapping_add(std::hint::black_box(
                reused.last().copied().unwrap_or_default(),
            ));
        }
        let reuse_time = reuse_started.elapsed();

        assert_eq!(rewrite_total, reuse_total);
        assert_eq!(rewritten, reused);
        let speedup = rewrite_time.as_secs_f64() / reuse_time.as_secs_f64();
        eprintln!(
            "stable render child-ID slots over {iterations} 256-child updates: rewrite {rewrite_time:?}; reuse {reuse_time:?}; ratio {speedup:.2}x"
        );
        println!("MESH_PERF metric=render_child_id_reuse_speedup value={speedup:.6}");
        assert!(
            reuse_time * 5 < rewrite_time * 4,
            "stable child-ID reuse should be at least 1.25x faster"
        );
    }

    #[test]
    fn render_object_reuses_unchanged_text_and_accessibility_strings() {
        let mut node = retained_visual_node();
        node.tag = "text".into();
        node.attributes
            .insert("content".into(), "retained text".into());
        node.accessibility.label = Some("retained label".into());
        let mut tree = RenderObjectTree::default();
        tree.update(&node);
        let first = tree.objects.get(&node.id).expect("first render object");
        let first_text = Arc::clone(
            first
                .fingerprint
                .paint
                .text
                .content
                .as_ref()
                .expect("text content"),
        );
        let first_label = Arc::clone(first.fingerprint.accessibility.1.as_ref().expect("label"));

        tree.update(&node);
        let second = tree.objects.get(&node.id).expect("second render object");
        assert!(Arc::ptr_eq(
            &first_text,
            second
                .fingerprint
                .paint
                .text
                .content
                .as_ref()
                .expect("text content")
        ));
        assert!(Arc::ptr_eq(
            &first_label,
            second.fingerprint.accessibility.1.as_ref().expect("label")
        ));
    }

    #[test]
    fn render_object_tree_preserves_identity_and_reports_slot_diffs() {
        let mut root = WidgetNode::new("row");
        root.id = 1;
        let mut child = WidgetNode::new("text");
        child.id = 2;
        child.attributes.insert("content".into(), "hello".into());
        root.children.push(child);

        let mut tree = RenderObjectTree::default();
        let first = tree.update(&root);
        assert_eq!(first.inserted, 2);
        assert_eq!(tree.generation(), 1);

        let clean = tree.update(&root);
        assert!(!clean.any());
        assert_eq!(tree.generation(), 1);

        root.children[0]
            .attributes
            .insert("content".into(), "goodbye".into());
        root.children[0].layout.width = 42.0;
        root.children[0].computed_style.opacity = 0.5;

        let dirty = tree.update(&root);
        assert_eq!(dirty.text, 1);
        assert_eq!(dirty.geometry, 1);
        assert_eq!(dirty.opacity, 1);
        assert_eq!(
            tree.dirty_node_ids(),
            &HashSet::from([2]),
            "dirty node ids should identify changed render objects"
        );
        assert_eq!(dirty.inserted, 0);
        assert_eq!(dirty.removed, 0);
        assert_eq!(tree.generation(), 2);
        assert_eq!(tree.last_dirty(), dirty);
    }

    #[test]
    fn render_object_tree_marks_animated_transform_without_geometry_dirty() {
        let mut root = retained_visual_node();
        let mut tree = RenderObjectTree::default();
        tree.update(&root);

        root.computed_style.transform.translate_x = 24.0;
        let dirty = tree.update(&root);

        assert_eq!(dirty.transform, 1);
        assert_eq!(dirty.geometry, 0);
        assert_eq!(dirty.opacity, 0);
        assert_eq!(dirty.material, 0);
        assert_eq!(tree.dirty_node_ids(), &HashSet::from([1]));
    }

    #[test]
    fn render_object_tree_marks_animated_opacity_without_geometry_dirty() {
        let mut root = retained_visual_node();
        let mut tree = RenderObjectTree::default();
        tree.update(&root);

        root.computed_style.opacity = 0.42;
        let dirty = tree.update(&root);

        assert_eq!(dirty.opacity, 1);
        assert_eq!(dirty.geometry, 0);
        assert_eq!(dirty.transform, 0);
        assert_eq!(dirty.material, 0);
        assert_eq!(tree.dirty_node_ids(), &HashSet::from([1]));
    }

    #[test]
    fn render_object_tree_marks_animated_material_without_geometry_dirty() {
        let mut root = retained_visual_node();
        let mut tree = RenderObjectTree::default();
        tree.update(&root);

        root.computed_style.box_shadow = mesh_core_elements::BoxShadow {
            offset_x: 4.0,
            offset_y: 6.0,
            blur_radius: 12.0,
            spread_radius: 2.0,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 128,
            },
            inset: false,
        };
        let dirty = tree.update(&root);

        assert_eq!(dirty.material, 1);
        assert_eq!(dirty.geometry, 0);
        assert_eq!(dirty.transform, 0);
        assert_eq!(dirty.opacity, 0);
        assert_eq!(tree.dirty_node_ids(), &HashSet::from([1]));
    }

    #[test]
    fn render_object_tree_removes_unvisited_retained_paint_data() {
        let mut root = WidgetNode::new("row");
        root.id = 1;
        let mut first_child = WidgetNode::new("text");
        first_child.id = 2;
        let mut second_child = WidgetNode::new("icon");
        second_child.id = 3;
        root.children.push(first_child);
        root.children.push(second_child);

        let mut tree = RenderObjectTree::default();
        let first = tree.update(&root);
        assert_eq!(first.inserted, 3);
        assert_eq!(tree.len(), 3);

        root.children.pop();
        let dirty = tree.update(&root);
        assert_eq!(dirty.removed, 1);
        assert_eq!(dirty.reordered, 1);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.generation(), 2);
    }

    #[test]
    fn render_object_tree_validates_caller_lineage_for_retained_generation() {
        let mut root = WidgetNode::new("row");
        root.id = 1;
        let mut child = WidgetNode::new("text");
        child.id = 2;
        child.attributes.insert("content".into(), "hello".into());
        root.children.push(child);

        let mut tree = RenderObjectTree::default();
        let first = tree.update_for_retained_generation(&root, 1);
        assert_eq!(first.inserted, 2);
        assert_eq!(tree.generation(), 1);

        root.children[0]
            .attributes
            .insert("content".into(), "goodbye".into());
        let skipped = tree.update_for_retained_generation(&root, 1);
        if cfg!(debug_assertions) {
            assert_eq!(skipped.text, 1);
            assert_eq!(tree.dirty_node_ids(), &HashSet::from([2]));
            assert_eq!(tree.generation(), 2);
        } else {
            assert!(!skipped.any());
            assert!(tree.dirty_node_ids().is_empty());
            assert_eq!(tree.generation(), 1);
        }

        let dirty = tree.update_for_retained_generation(&root, 2);
        if cfg!(debug_assertions) {
            assert!(!dirty.any());
            assert_eq!(tree.generation(), 2);
        } else {
            assert_eq!(dirty.text, 1);
            assert_eq!(tree.generation(), 2);
        }
    }

    #[test]
    fn render_object_tree_rehashes_only_retained_dirty_nodes() {
        let mut root = WidgetNode::new("row");
        root.id = 1;
        for id in 2..=5 {
            let mut child = WidgetNode::new("text");
            child.id = id;
            child
                .attributes
                .insert("content".into(), format!("child {id}"));
            root.children.push(child);
        }

        let mut tree = RenderObjectTree::default();
        tree.update_for_retained_generation(&root, 1);

        root.children[2]
            .attributes
            .insert("content".into(), "changed".into());
        let retained_dirty_ids = HashSet::from([4]);
        let dirty = tree.update_for_retained_dirty_nodes(&root, 2, &retained_dirty_ids);

        assert_eq!(dirty.text, 1);
        assert_eq!(tree.dirty_node_ids(), &HashSet::from([4]));
        assert_eq!(tree.generation(), 2);

        let clean = tree.update_for_retained_dirty_nodes(&root, 2, &HashSet::new());
        assert!(!clean.any());
        assert!(tree.dirty_node_ids().is_empty());
    }

    #[test]
    fn render_object_tree_accepts_direct_retained_dirty_node_refs() {
        let mut root = WidgetNode::new("row");
        root.id = 1;
        for id in 2..=5 {
            let mut child = WidgetNode::new("text");
            child.id = id;
            child
                .attributes
                .insert("content".into(), format!("child {id}"));
            root.children.push(child);
        }

        let mut direct = RenderObjectTree::default();
        let mut traversed = RenderObjectTree::default();
        direct.update_for_retained_generation(&root, 1);
        traversed.update_for_retained_generation(&root, 1);

        root.children[2]
            .attributes
            .insert("content".into(), "changed".into());
        let direct_dirty =
            direct.update_for_retained_dirty_node_refs(&root, 2, &[&root.children[2]]);
        let traversed_dirty =
            traversed.update_for_retained_dirty_nodes(&root, 2, &HashSet::from([4]));

        assert_eq!(direct_dirty, traversed_dirty);
        assert_eq!(direct.dirty_node_ids(), traversed.dirty_node_ids());
        assert_eq!(direct.generation(), traversed.generation());
    }

    #[test]
    fn render_object_tree_distinguishes_scroll_offsets_from_geometry() {
        let mut root = WidgetNode::new("scroll");
        root.id = 1;
        root.attributes.insert("_mesh_scroll_x".into(), "0".into());
        let mut child = WidgetNode::new("text");
        child.id = 2;
        root.children.push(child);

        let mut tree = RenderObjectTree::default();
        tree.update(&root);

        root.attributes.insert("_mesh_scroll_x".into(), "24".into());
        let dirty = tree.update(&root);

        assert_eq!(dirty.geometry, 0);
        assert_eq!(dirty.scroll, 1);
        assert_eq!(
            tree.dirty_node_ids(),
            &HashSet::from([1]),
            "scroll updates should dirty the scrolled render object so retained paint can rebuild its subtree locally"
        );
    }

    #[test]
    fn render_object_tree_marks_slider_value_updates_as_primitive_dirty() {
        let mut root = WidgetNode::new("slider");
        root.id = 1;
        root.attributes.insert("min".into(), "0".into());
        root.attributes.insert("max".into(), "1".into());
        root.attributes.insert("value".into(), "0.25".into());

        let mut tree = RenderObjectTree::default();
        tree.update(&root);

        root.attributes.insert("value".into(), "0.735".into());
        let dirty = tree.update(&root);

        assert_eq!(dirty.primitive, 1);
        assert_eq!(
            tree.dirty_node_ids(),
            &HashSet::from([1]),
            "slider value updates should dirty the render object so retained paint rebuilds the thumb immediately"
        );
    }
}
