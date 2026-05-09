use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use mesh_core_elements::style::{Color, ComputedStyle, Edges, Transform2D};
use mesh_core_elements::{LayoutRect, NodeId, WidgetNode};

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
            || self.text > 0
            || self.accessibility > 0
    }
}

#[derive(Debug, Default)]
pub struct RenderObjectTree {
    generation: u64,
    nodes: HashMap<NodeId, RenderObjectSnapshot>,
    root: Option<NodeId>,
    last_dirty: RenderObjectDirtySummary,
}

impl RenderObjectTree {
    pub fn update(&mut self, root: &WidgetNode) -> RenderObjectDirtySummary {
        let mut next = HashMap::new();
        collect_render_objects(root, None, &mut next);

        let mut dirty = RenderObjectDirtySummary::default();
        for (&id, next_snapshot) in &next {
            match self.nodes.get(&id) {
                Some(previous) => dirty.add_diff(previous, next_snapshot),
                None => dirty.inserted += 1,
            }
        }

        let next_ids: HashSet<_> = next.keys().copied().collect();
        dirty.removed = self
            .nodes
            .keys()
            .filter(|id| !next_ids.contains(id))
            .count();

        if self.root != Some(root.id) {
            dirty.reordered += usize::from(self.root.is_some());
        }
        if dirty.any() {
            self.generation = self.generation.saturating_add(1);
        }
        self.root = Some(root.id);
        self.nodes = next;
        self.last_dirty = dirty;
        dirty
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn last_dirty(&self) -> RenderObjectDirtySummary {
        self.last_dirty
    }
}

impl RenderObjectDirtySummary {
    fn add_diff(&mut self, previous: &RenderObjectSnapshot, next: &RenderObjectSnapshot) {
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
        if previous.text != next.text {
            self.text += 1;
        }
        if previous.accessibility != next.accessibility {
            self.accessibility += 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderObjectSnapshot {
    parent: Option<NodeId>,
    child_ids: Vec<NodeId>,
    transform: TransformSlot,
    clip: ClipSlot,
    opacity: u32,
    geometry: GeometrySlot,
    material: u64,
    text: TextSlot,
    accessibility: AccessibilitySlot,
}

type TransformSlot = (u32, u32, u32, u32, u32);
type GeometrySlot = (u32, u32, u32, u32);
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

fn collect_render_objects(
    node: &WidgetNode,
    parent: Option<NodeId>,
    out: &mut HashMap<NodeId, RenderObjectSnapshot>,
) {
    out.insert(node.id, render_object_snapshot(node, parent));
    for child in &node.children {
        collect_render_objects(child, Some(node.id), out);
    }
}

fn render_object_snapshot(node: &WidgetNode, parent: Option<NodeId>) -> RenderObjectSnapshot {
    RenderObjectSnapshot {
        parent,
        child_ids: node.children.iter().map(|child| child.id).collect(),
        transform: transform_slot(node.computed_style.transform),
        clip: clip_slot(node),
        opacity: node.computed_style.opacity.to_bits(),
        geometry: geometry_slot(node.layout),
        material: material_hash(&node.computed_style),
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
    let geometry = geometry_slot(node.layout);
    (clips, geometry.0, geometry.1, geometry.2, geometry.3)
}

fn geometry_slot(layout: LayoutRect) -> GeometrySlot {
    (
        layout.x.to_bits(),
        layout.y.to_bits(),
        layout.width.to_bits(),
        layout.height.to_bits(),
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
    color_slot(style.border_color).hash(&mut hasher);
    hash_edges(style.border_width, &mut hasher);
    hash_edges(style.padding, &mut hasher);
    style.border_radius.top_left.to_bits().hash(&mut hasher);
    style.border_radius.top_right.to_bits().hash(&mut hasher);
    style.border_radius.bottom_right.to_bits().hash(&mut hasher);
    style.border_radius.bottom_left.to_bits().hash(&mut hasher);
    format!("{:?}", style.display).hash(&mut hasher);
    style.z_index.hash(&mut hasher);
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

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::WidgetNode;

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
        assert_eq!(dirty.inserted, 0);
        assert_eq!(dirty.removed, 0);
        assert_eq!(tree.generation(), 2);
        assert_eq!(tree.last_dirty(), dirty);
    }
}
