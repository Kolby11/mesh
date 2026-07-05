use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use mesh_core_elements::style::{
    BackgroundPaint, Color, ComputedStyle, Display, Edges, Transform2D,
};
use mesh_core_elements::{AccessibilityRole, NodeId, WidgetNode};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderObjectDirtySummary {
    pub inserted: usize,
    pub removed: usize,
    pub reordered: usize,
    pub transform: usize,
    pub clip: usize,
    pub opacity: usize,
    pub geometry: usize,
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
    update_epoch: u64,
    objects: HashMap<NodeId, RenderObjectPaintData>,
    root: Option<NodeId>,
    last_dirty: RenderObjectDirtySummary,
    dirty_nodes: HashSet<NodeId>,
}

struct RenderObjectHasher(u64);

impl Default for RenderObjectHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for RenderObjectHasher {
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

impl RenderObjectHasher {
    #[inline]
    fn write_mix(&mut self, value: u64) {
        self.0 ^= value;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        self.0 ^= self.0 >> 32;
    }
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
        if self.retained_tree_generation == Some(retained_tree_generation) {
            self.last_dirty = RenderObjectDirtySummary::default();
            self.dirty_nodes.clear();
            return self.last_dirty;
        }
        self.update_inner(root, Some(retained_tree_generation))
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

        update_retained_render_objects(
            root,
            None,
            &mut self.objects,
            update_epoch,
            &mut dirty,
            &mut dirty_nodes,
        );

        let before_remove = self.objects.len();
        self.objects
            .retain(|_, object| object.last_seen_epoch == update_epoch);
        dirty.removed = before_remove.saturating_sub(self.objects.len());

        if self.root != Some(root.id) {
            dirty.reordered += usize::from(self.root.is_some());
        }
        if dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        self.root = Some(root.id);
        self.retained_tree_generation = retained_tree_generation;
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
}

impl RenderObjectDirtySummary {
    fn add_diff(&mut self, previous: &RenderObjectPaintData, next: &RenderObjectPaintData) {
        if previous.transform != next.transform {
            self.transform += 1;
        }
        if previous.clip != next.clip {
            self.clip += 1;
        }
        if previous.opacity != next.opacity {
            self.opacity += 1;
        }
        if previous.geometry != next.geometry {
            self.geometry += 1;
        }
        if previous.material != next.material {
            self.material += 1;
        }
        if previous.primitive != next.primitive {
            self.primitive += 1;
        }
        if previous.text != next.text {
            self.text += 1;
        }
        if previous.accessibility != next.accessibility {
            self.accessibility += 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderObjectPaintData {
    last_seen_epoch: u64,
    parent: Option<NodeId>,
    child_ids: Vec<NodeId>,
    transform: TransformSlot,
    clip: ClipSlot,
    opacity: u32,
    geometry: GeometrySlot,
    material: u64,
    primitive: u64,
    text: TextSlot,
    accessibility: AccessibilitySlot,
}

type TransformSlot = (u32, u32, u32, u32, u32);
type GeometrySlot = (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32);
type ClipSlot = (bool, u32, u32, u32, u32);
type AccessibilitySlot = (AccessibilityRoleSlot, Option<Arc<str>>, bool, bool);

#[derive(Debug, Clone, PartialEq, Eq)]
enum AccessibilityRoleSlot {
    Builtin(u8),
    Custom(Arc<str>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextSlot {
    content: Option<Arc<str>>,
    family: Arc<str>,
    size: u32,
    weight: u16,
    line_height: u32,
    color: ColorSlot,
}

type ColorSlot = (u8, u8, u8, u8);

fn update_retained_render_objects(
    node: &WidgetNode,
    parent: Option<NodeId>,
    objects: &mut HashMap<NodeId, RenderObjectPaintData>,
    update_epoch: u64,
    dirty: &mut RenderObjectDirtySummary,
    dirty_nodes: &mut HashSet<NodeId>,
) {
    match objects.get_mut(&node.id) {
        Some(current) => {
            let before = *dirty;
            if current.parent != parent
                || current.child_ids.len() != node.children.len()
                || !current
                    .child_ids
                    .iter()
                    .copied()
                    .eq(node.children.iter().map(|child| child.id))
            {
                dirty.reordered += 1;
            }
            let child_ids = refill_child_id_slot(node, std::mem::take(&mut current.child_ids));
            let next = render_object_paint_data(node, parent, Some(current), child_ids);
            dirty.add_diff(current, &next);
            if *dirty != before {
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

    for child in &node.children {
        update_retained_render_objects(
            child,
            Some(node.id),
            objects,
            update_epoch,
            dirty,
            dirty_nodes,
        );
    }
}

fn render_object_paint_data(
    node: &WidgetNode,
    parent: Option<NodeId>,
    previous: Option<&RenderObjectPaintData>,
    child_ids: Vec<NodeId>,
) -> RenderObjectPaintData {
    let geometry = geometry_slot(node);
    RenderObjectPaintData {
        last_seen_epoch: 0,
        parent,
        child_ids,
        transform: transform_slot(node.computed_style.transform),
        clip: clip_slot(node, geometry),
        opacity: node.computed_style.opacity.to_bits(),
        geometry,
        material: material_hash(&node.computed_style),
        primitive: primitive_hash(node),
        text: text_slot(node, previous.map(|data| &data.text)),
        accessibility: accessibility_slot(node, previous.map(|data| &data.accessibility)),
    }
}

fn child_id_slot(node: &WidgetNode) -> Vec<NodeId> {
    refill_child_id_slot(node, Vec::new())
}

fn refill_child_id_slot(node: &WidgetNode, mut child_ids: Vec<NodeId>) -> Vec<NodeId> {
    child_ids.clear();
    child_ids.extend(node.children.iter().map(|child| child.id));
    child_ids
}

fn transform_slot(transform: Transform2D) -> TransformSlot {
    (
        transform.translate_x.to_bits(),
        transform.translate_y.to_bits(),
        transform.scale_x.to_bits(),
        transform.scale_y.to_bits(),
        transform.rotation.to_bits(),
    )
}

fn clip_slot(node: &WidgetNode, geometry: GeometrySlot) -> ClipSlot {
    let clips = node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents();
    (clips, geometry.0, geometry.1, geometry.2, geometry.3)
}

fn geometry_slot(node: &WidgetNode) -> GeometrySlot {
    let layout = node.layout;
    let scroll = node.resolved_scroll_metrics();
    (
        layout.x.to_bits(),
        layout.y.to_bits(),
        layout.width.to_bits(),
        layout.height.to_bits(),
        scroll.x.to_bits(),
        scroll.y.to_bits(),
        scroll.max_x.to_bits(),
        scroll.max_y.to_bits(),
        scroll.content_width.to_bits(),
        scroll.content_height.to_bits(),
    )
}

fn text_slot(node: &WidgetNode, previous: Option<&TextSlot>) -> TextSlot {
    TextSlot {
        content: retained_arc_str(
            node.attributes.get("content").map(String::as_str),
            previous.and_then(|slot| slot.content.as_ref()),
        ),
        family: node.computed_style.font_family.clone(),
        size: node.computed_style.font_size.to_bits(),
        weight: node.computed_style.font_weight,
        line_height: node.computed_style.line_height.to_bits(),
        color: color_slot(node.computed_style.color),
    }
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
        node.accessibility.focused,
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

fn retained_arc_str(value: Option<&str>, previous: Option<&Arc<str>>) -> Option<Arc<str>> {
    value.map(|value| match previous {
        Some(previous) if previous.as_ref() == value => Arc::clone(previous),
        _ => Arc::from(value),
    })
}

fn material_hash(style: &ComputedStyle) -> u64 {
    let mut hasher = RenderObjectHasher::default();
    color_slot(style.background_color).hash(&mut hasher);
    match &style.background_paint {
        BackgroundPaint::None => 0_u8.hash(&mut hasher),
        BackgroundPaint::Image(source) => {
            1_u8.hash(&mut hasher);
            source.path.hash(&mut hasher);
        }
        BackgroundPaint::LinearGradient(gradient) => {
            2_u8.hash(&mut hasher);
            color_slot(gradient.from).hash(&mut hasher);
            color_slot(gradient.to).hash(&mut hasher);
        }
    }
    color_slot(style.border_color).hash(&mut hasher);
    hash_edges(style.border_width, &mut hasher);
    hash_edges(style.padding, &mut hasher);
    style.border_radius.top_left.to_bits().hash(&mut hasher);
    style.border_radius.top_right.to_bits().hash(&mut hasher);
    style.border_radius.bottom_right.to_bits().hash(&mut hasher);
    style.border_radius.bottom_left.to_bits().hash(&mut hasher);
    display_slot(style.display).hash(&mut hasher);
    style.z_index.hash(&mut hasher);
    style.box_shadow.offset_x.to_bits().hash(&mut hasher);
    style.box_shadow.offset_y.to_bits().hash(&mut hasher);
    style.box_shadow.blur_radius.to_bits().hash(&mut hasher);
    style.box_shadow.spread_radius.to_bits().hash(&mut hasher);
    color_slot(style.box_shadow.color).hash(&mut hasher);
    style.box_shadow.inset.hash(&mut hasher);
    style.filter.blur_radius.to_bits().hash(&mut hasher);
    style
        .backdrop_filter
        .blur_radius
        .to_bits()
        .hash(&mut hasher);
    hasher.finish()
}

fn primitive_hash(node: &WidgetNode) -> u64 {
    let mut hasher = RenderObjectHasher::default();
    node.tag.hash(&mut hasher);
    match node.tag.as_str() {
        "input" => {
            node.attributes.get("value").hash(&mut hasher);
            node.attributes.get("placeholder").hash(&mut hasher);
            node.attributes.get("type").hash(&mut hasher);
            node.attributes.get("_mesh_focused").hash(&mut hasher);
        }
        "slider" => {
            attr_f32_with_default(node, "min", 0.0)
                .to_bits()
                .hash(&mut hasher);
            attr_f32_with_default(node, "max", 100.0)
                .to_bits()
                .hash(&mut hasher);
            attr_f32_with_default(node, "value", 50.0)
                .to_bits()
                .hash(&mut hasher);
            node.attributes.get("orient").hash(&mut hasher);
        }
        "icon" => {
            node.attributes.get("src").hash(&mut hasher);
            node.attributes.get("name").hash(&mut hasher);
            node.attributes.get("size").hash(&mut hasher);
        }
        _ => {}
    }
    hasher.finish()
}

fn hash_edges(edges: Edges, hasher: &mut impl Hasher) {
    edges.top.to_bits().hash(hasher);
    edges.right.to_bits().hash(hasher);
    edges.bottom.to_bits().hash(hasher);
    edges.left.to_bits().hash(hasher);
}

fn display_slot(display: Display) -> u8 {
    match display {
        Display::Flex => 0,
        Display::None => 1,
    }
}

fn color_slot(color: Color) -> ColorSlot {
    (color.r, color.g, color.b, color.a)
}

fn attr_f32_with_default(node: &WidgetNode, key: &str, default: f32) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::WidgetNode;
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

    // cargo test -p mesh-core-render --release -- render_object_epoch_marks_beat_visited_set --ignored --nocapture
    #[test]
    #[ignore = "release-only render-object visited-state microbenchmark"]
    fn render_object_epoch_marks_beat_visited_set() {
        let ids = (0..512_u64).collect::<Vec<_>>();
        let iterations = 20_000;

        let visited_started = Instant::now();
        let mut visited_count = 0usize;
        let mut visited = HashSet::with_capacity(ids.len());
        let mut dirty = HashSet::with_capacity(ids.len().min(1024));
        for _ in 0..iterations {
            visited.clear();
            dirty.clear();
            for id in &ids {
                visited.insert(*id);
                if id % 4 == 0 {
                    dirty.insert(*id);
                }
            }
            visited_count += std::hint::black_box(
                ids.iter().filter(|id| visited.contains(id)).count() + dirty.len(),
            );
        }
        let visited_time = visited_started.elapsed();

        let epoch_started = Instant::now();
        let mut epoch_count = 0usize;
        let mut epochs = ids
            .iter()
            .copied()
            .map(|id| (id, 0_u64))
            .collect::<HashMap<_, _>>();
        let mut dirty = HashSet::with_capacity(ids.len().min(1024));
        for epoch in 1..=iterations as u64 {
            dirty.clear();
            for id in &ids {
                *epochs.get_mut(id).expect("id present") = epoch;
                if id % 4 == 0 {
                    dirty.insert(*id);
                }
            }
            epoch_count += std::hint::black_box(
                epochs.values().filter(|seen| **seen == epoch).count() + dirty.len(),
            );
        }
        let epoch_time = epoch_started.elapsed();

        eprintln!(
            "render object visited state: set {visited_time:?}; epoch {epoch_time:?}; ratio {:.1}x; counts={visited_count}/{epoch_count}",
            visited_time.as_secs_f64() / epoch_time.as_secs_f64()
        );
        assert_eq!(visited_count, epoch_count);
        assert!(epoch_time < visited_time);
    }

    // cargo test -p mesh-core-render --release -- typed_scroll_geometry_beats_string_parsing --ignored --nocapture
    #[test]
    #[ignore = "release-only typed scroll geometry microbenchmark"]
    fn typed_scroll_geometry_beats_string_parsing() {
        let mut legacy = retained_visual_node();
        for (key, value) in [
            ("_mesh_scroll_x", "12.3456"),
            ("_mesh_scroll_y", "23.4567"),
            ("_mesh_scroll_max_x", "100.25"),
            ("_mesh_scroll_max_y", "200.5"),
            ("_mesh_content_width", "960.75"),
            ("_mesh_content_height", "720.125"),
        ] {
            legacy.attributes.insert(key.into(), value.into());
        }
        let mut typed = retained_visual_node();
        typed.scroll_metrics = Some(legacy.resolved_scroll_metrics());
        let iterations = 2_000_000;

        let old_started = Instant::now();
        let mut old_total = 0_u64;
        for _ in 0..iterations {
            let geometry = geometry_slot(std::hint::black_box(&legacy));
            old_total = old_total.wrapping_add(std::hint::black_box(geometry.4 as u64));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0_u64;
        for _ in 0..iterations {
            let geometry = geometry_slot(std::hint::black_box(&typed));
            new_total = new_total.wrapping_add(std::hint::black_box(geometry.4 as u64));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "scroll geometry: string parse {old_time:?}; typed fields {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    #[test]
    fn render_object_reuses_unchanged_text_and_accessibility_strings() {
        let mut node = retained_visual_node();
        node.attributes
            .insert("content".into(), "retained text".into());
        node.accessibility.label = Some("retained label".into());
        let mut tree = RenderObjectTree::default();
        tree.update(&node);
        let first = tree.objects.get(&node.id).expect("first render object");
        let first_text = Arc::clone(first.text.content.as_ref().expect("text content"));
        let first_label = Arc::clone(first.accessibility.1.as_ref().expect("label"));

        tree.update(&node);
        let second = tree.objects.get(&node.id).expect("second render object");
        assert!(Arc::ptr_eq(
            &first_text,
            second.text.content.as_ref().expect("text content")
        ));
        assert!(Arc::ptr_eq(
            &first_label,
            second.accessibility.1.as_ref().expect("label")
        ));
    }

    // cargo test -p mesh-core-render --release -- retained_render_strings_beat_string_clones --ignored --nocapture
    #[test]
    #[ignore = "release-only retained render string microbenchmark"]
    fn retained_render_strings_beat_string_clones() {
        let value = "render object text and accessibility content retained across dirty frames";
        let retained: Arc<str> = Arc::from(value);
        let iterations = 5_000_000;

        let old_started = Instant::now();
        let mut old_total = 0_usize;
        for _ in 0..iterations {
            let cloned = std::hint::black_box(value).to_owned();
            old_total = old_total.saturating_add(std::hint::black_box(cloned.len()));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0_usize;
        for _ in 0..iterations {
            let cloned = retained_arc_str(Some(value), Some(&retained)).expect("retained value");
            new_total = new_total.saturating_add(std::hint::black_box(cloned.len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "render object strings: String clone {old_time:?}; retained Arc {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-render --release -- reused_child_id_vec_beats_fresh_allocation --ignored --nocapture
    #[test]
    #[ignore = "release-only retained child-id vector microbenchmark"]
    fn reused_child_id_vec_beats_fresh_allocation() {
        let mut node = WidgetNode::new("row");
        for id in 1..=6 {
            let mut child = WidgetNode::new("box");
            child.id = id;
            node.children.push(child);
        }
        let iterations = 5_000_000;

        let old_started = Instant::now();
        let mut old_total = 0_usize;
        for _ in 0..iterations {
            let child_ids = child_id_slot(std::hint::black_box(&node));
            old_total = old_total.saturating_add(std::hint::black_box(child_ids.len()));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0_usize;
        let mut child_ids = Vec::new();
        for _ in 0..iterations {
            child_ids = refill_child_id_slot(std::hint::black_box(&node), child_ids);
            new_total = new_total.saturating_add(std::hint::black_box(child_ids.len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "render object child ids: fresh Vec {old_time:?}; retained Vec {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
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
    fn render_object_tree_skips_rebuild_when_retained_generation_is_unchanged() {
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
        assert!(!skipped.any());
        assert!(tree.dirty_node_ids().is_empty());
        assert_eq!(tree.generation(), 1);

        let dirty = tree.update_for_retained_generation(&root, 2);
        assert_eq!(dirty.text, 1);
        assert_eq!(tree.generation(), 2);
    }

    #[test]
    fn render_object_tree_marks_scroll_updates_as_geometry_dirty() {
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

        assert_eq!(dirty.geometry, 1);
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
