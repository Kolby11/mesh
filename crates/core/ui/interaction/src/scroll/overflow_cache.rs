use super::*;
use mesh_core_elements::bounds_index::ChildBoundsIndex;
use std::collections::BTreeSet;

struct Entry {
    children: ChildBoundsIndex,
    bounds: ContentBounds,
}

/// Exact propagated child bounds for one structurally stable widget tree.
/// Rebuild after topology changes or unknown invalidation. Sparse refreshes
/// require authoritative paths for all geometry and overflow-style changes.
#[derive(Default)]
pub struct RetainedOverflow {
    nodes: HashMap<NodeId, Entry>,
    root: Option<NodeId>,
}

impl RetainedOverflow {
    pub fn invalidate(&mut self) {
        self.root = None;
    }
    pub fn is_current(&self, root: NodeId) -> bool {
        self.root == Some(root)
    }

    pub fn annotate_tree(
        &mut self,
        tree: &mut WidgetNode,
        offsets: &mut HashMap<NodeId, ScrollOffsetState>,
    ) -> Vec<NodeId> {
        self.nodes.clear();
        let mut changed = Vec::new();
        self.annotate(tree, offsets, &mut changed);
        self.root = Some(tree.id);
        changed
    }

    fn annotate(
        &mut self,
        node: &mut WidgetNode,
        offsets: &mut HashMap<NodeId, ScrollOffsetState>,
        changed: &mut Vec<NodeId>,
    ) -> ContentBounds {
        mesh_core_elements::frame_work::visit(mesh_core_elements::frame_work::Pass::Annotation);
        let children = node
            .children
            .iter_mut()
            .map(|child| Some(self.annotate(child, offsets, changed)))
            .collect();
        let children = ChildBoundsIndex::new(children);
        let previous = node.scroll_metrics;
        let bounds = annotate_overflow_node(node, offsets, children.bounds());
        if previous != node.scroll_metrics {
            changed.push(node.id);
        }
        self.nodes.insert(node.id, Entry { children, bounds });
        bounds
    }

    /// Update touched nodes and their ancestors, deepest first. Returns `None`
    /// without mutation when retained history or a supplied path is missing.
    pub fn refresh(
        &mut self,
        tree: &mut WidgetNode,
        paths: &[&[usize]],
        offsets: &mut HashMap<NodeId, ScrollOffsetState>,
    ) -> Option<Vec<NodeId>> {
        if !self.is_current(tree.id) {
            return None;
        }
        let mut prefixes = BTreeSet::new();
        for path in paths {
            for depth in 0..=path.len() {
                prefixes.insert(&path[..depth]);
            }
        }
        let mut ordered: Vec<_> = prefixes.into_iter().collect();
        ordered.sort_by_key(|path| std::cmp::Reverse(path.len()));
        for path in &ordered {
            let mut node = &*tree;
            for &index in *path {
                node = node.children.get(index)?;
            }
            if self.nodes.get(&node.id)?.children.len() != node.children.len() {
                return None;
            }
        }
        let mut changed = Vec::new();
        for path in ordered {
            mesh_core_elements::frame_work::visit(mesh_core_elements::frame_work::Pass::Annotation);
            let mut node = &mut *tree;
            for &index in path {
                node = &mut node.children[index];
            }
            let previous = node.scroll_metrics;
            let entry = self
                .nodes
                .get_mut(&node.id)
                .expect("validated overflow history");
            let bounds = annotate_overflow_node(node, offsets, entry.children.bounds());
            let bounds_changed = entry.bounds != bounds;
            entry.bounds = bounds;
            if previous != node.scroll_metrics {
                changed.push(node.id);
            }
            if bounds_changed && let Some((&index, parent_path)) = path.split_last() {
                let mut parent = &*tree;
                for &index in parent_path {
                    parent = &parent.children[index];
                }
                self.nodes
                    .get_mut(&parent.id)
                    .expect("validated parent history")
                    .children
                    .set(index, Some(bounds));
            }
        }
        Some(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{
        LayoutRect,
        style::{Edges, Overflow, Position},
    };

    fn compare(actual: &WidgetNode, expected: &WidgetNode) {
        assert_eq!(
            actual.scroll_metrics, expected.scroll_metrics,
            "node {}",
            actual.id
        );
        for (a, b) in actual.children.iter().zip(&expected.children) {
            compare(a, b);
        }
    }

    #[test]
    fn exact_overflow_unions_match_full_recomputation_and_clamp_offsets() {
        for count in [16, 1024] {
            for target in [0, count / 2, count - 1] {
                let mut root = WidgetNode::new("scroll");
                root.layout = LayoutRect {
                    x: 0.25,
                    y: 0.75,
                    width: 80.5,
                    height: 50.25,
                };
                root.computed_style.padding = Edges::all(0.35);
                root.computed_style.overflow_x = Overflow::Auto;
                root.computed_style.overflow_y = Overflow::Auto;
                for i in 0..count {
                    let mut child = WidgetNode::new("box");
                    child.layout = LayoutRect {
                        x: 1.25,
                        y: 1.75,
                        width: 3.5,
                        height: 5.25,
                    };
                    let mut descendant = WidgetNode::new("box");
                    descendant.layout = LayoutRect {
                        x: -2.25,
                        y: 3.75,
                        width: 4.25,
                        height: 5.25,
                    };
                    if i == target {
                        descendant.computed_style.position = Position::Fixed;
                    }
                    child.children.push(descendant);
                    root.children.push(child);
                }
                let mut retained = RetainedOverflow::default();
                let mut offsets = HashMap::new();
                retained.annotate_tree(&mut root, &mut offsets);
                for turn in 0..8 {
                    let child = &mut root.children[target];
                    child.layout.width = if turn % 2 == 0 { 250.25 } else { 3.5 };
                    child.layout.height = if turn % 2 == 0 { 160.75 } else { 5.25 };
                    child.computed_style.overflow_x = if turn % 3 == 0 {
                        Overflow::Hidden
                    } else {
                        Overflow::Visible
                    };
                    child.computed_style.padding.left = if turn % 2 == 0 { 1.75 } else { 0.35 };
                    child.children[0].layout.width = if turn % 2 == 0 { 300.25 } else { 4.25 };
                    if turn % 2 == 0 {
                        child.mark_promoted_popover();
                    }
                    offsets.insert(root.id, ScrollOffsetState { x: 500.0, y: 500.0 });
                    let mut expected = root.clone();
                    let mut full_offsets = offsets.clone();
                    annotate_overflow_tree(&mut expected, &mut full_offsets);
                    let scope = mesh_core_elements::frame_work::Scope::new(true);
                    assert!(
                        retained
                            .refresh(&mut root, &[&[target, 0]], &mut offsets)
                            .is_some()
                    );
                    let counts = scope.snapshot();
                    drop(scope);
                    compare(&root, &expected);
                    assert_eq!(offsets.len(), full_offsets.len());
                    for (id, offset) in &offsets {
                        let expected = &full_offsets[id];
                        assert_eq!((offset.x, offset.y), (expected.x, expected.y));
                    }
                    assert_eq!(counts["annotation"], 3);
                    assert!(counts["bounds_index"] <= 24, "{counts:?}");
                }
                let before = root.scroll_metrics;
                assert!(
                    retained
                        .refresh(&mut root, &[&[count]], &mut offsets)
                        .is_none()
                );
                assert_eq!(root.scroll_metrics, before);
                retained.invalidate();
                assert!(
                    retained
                        .refresh(&mut root, &[&[target]], &mut offsets)
                        .is_none()
                );
            }
        }
    }
}
