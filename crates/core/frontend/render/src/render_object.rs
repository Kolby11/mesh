use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use mesh_core_elements::style::{BackgroundPaint, Color, ComputedStyle, Edges, Transform2D};
use mesh_core_elements::{NodeId, WidgetNode};

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
        let mut dirty_nodes = HashSet::new();
        let mut visited = HashSet::new();

        update_retained_render_objects(
            root,
            None,
            &mut self.objects,
            &mut visited,
            &mut dirty,
            &mut dirty_nodes,
        );

        let before_remove = self.objects.len();
        self.objects.retain(|id, _| visited.contains(id));
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
        if previous.parent != next.parent || previous.child_ids != next.child_ids {
            self.reordered += 1;
        }
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
type AccessibilitySlot = (String, Option<String>, bool, bool);

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextSlot {
    content: Option<String>,
    family: String,
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
    visited: &mut HashSet<NodeId>,
    dirty: &mut RenderObjectDirtySummary,
    dirty_nodes: &mut HashSet<NodeId>,
) {
    visited.insert(node.id);
    let next = render_object_paint_data(node, parent);
    match objects.get_mut(&node.id) {
        Some(current) => {
            let before = *dirty;
            dirty.add_diff(current, &next);
            if *dirty != before {
                dirty_nodes.insert(node.id);
            }
            *current = next;
        }
        None => {
            dirty.inserted += 1;
            dirty_nodes.insert(node.id);
            objects.insert(node.id, next);
        }
    }

    for child in &node.children {
        update_retained_render_objects(child, Some(node.id), objects, visited, dirty, dirty_nodes);
    }
}

fn render_object_paint_data(node: &WidgetNode, parent: Option<NodeId>) -> RenderObjectPaintData {
    RenderObjectPaintData {
        parent,
        child_ids: node.children.iter().map(|child| child.id).collect(),
        transform: transform_slot(node.computed_style.transform),
        clip: clip_slot(node),
        opacity: node.computed_style.opacity.to_bits(),
        geometry: geometry_slot(node),
        material: material_hash(&node.computed_style),
        primitive: primitive_hash(node),
        text: text_slot(node),
        accessibility: accessibility_slot(node),
    }
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

fn clip_slot(node: &WidgetNode) -> ClipSlot {
    let clips = node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents();
    let geometry = geometry_slot(node);
    (clips, geometry.0, geometry.1, geometry.2, geometry.3)
}

fn geometry_slot(node: &WidgetNode) -> GeometrySlot {
    let layout = node.layout;
    (
        layout.x.to_bits(),
        layout.y.to_bits(),
        layout.width.to_bits(),
        layout.height.to_bits(),
        attr_f32(node, "_mesh_scroll_x").to_bits(),
        attr_f32(node, "_mesh_scroll_y").to_bits(),
        attr_f32(node, "_mesh_scroll_max_x").to_bits(),
        attr_f32(node, "_mesh_scroll_max_y").to_bits(),
        attr_f32(node, "_mesh_content_width").to_bits(),
        attr_f32(node, "_mesh_content_height").to_bits(),
    )
}

fn text_slot(node: &WidgetNode) -> TextSlot {
    TextSlot {
        content: node.attributes.get("content").cloned(),
        family: node.computed_style.font_family.clone(),
        size: node.computed_style.font_size.to_bits(),
        weight: node.computed_style.font_weight,
        line_height: node.computed_style.line_height.to_bits(),
        color: color_slot(node.computed_style.color),
    }
}

fn accessibility_slot(node: &WidgetNode) -> AccessibilitySlot {
    (
        node.accessibility.role.to_string(),
        node.accessibility.label.clone(),
        node.accessibility.focusable,
        node.accessibility.focused,
    )
}

fn material_hash(style: &ComputedStyle) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
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
    format!("{:?}", style.display).hash(&mut hasher);
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
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
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

fn color_slot(color: Color) -> ColorSlot {
    (color.r, color.g, color.b, color.a)
}

fn attr_f32(node: &WidgetNode, key: &str) -> f32 {
    node.attributes
        .get(key)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
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
