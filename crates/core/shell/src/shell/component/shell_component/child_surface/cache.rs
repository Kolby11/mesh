use super::*;
use mesh_core_elements::{FrameSnapshot, NodeId};

fn visit() {
    mesh_core_elements::frame_work::visit(
        mesh_core_elements::frame_work::Pass::ChildSurfaceRequests,
    );
}

fn reference_names(node: &WidgetNode) -> Vec<String> {
    node.mesh_key()
        .into_iter()
        .chain(node.bind_this())
        .chain(
            [Prop::Ref, Prop::Id]
                .into_iter()
                .filter_map(|prop| node.attributes.text(prop)),
        )
        .map(str::to_owned)
        .collect()
}

fn is_candidate(node: &WidgetNode) -> bool {
    node.is_promoted_window()
        || source_element_tag(node) == "popover"
        || node.computed_style.position == mesh_core_elements::style::Position::Absolute
}

fn nodes_at_path<'a>(root: &'a WidgetNode, path: &[usize]) -> Option<Vec<&'a WidgetNode>> {
    let mut nodes = vec![root];
    visit();
    for &index in path {
        visit();
        nodes.push(nodes.last()?.children.get(index)?);
    }
    Some(nodes)
}

#[derive(Default)]
struct Candidate {
    path: Vec<usize>,
    dependencies: HashSet<NodeId>,
    explicit: Vec<ChildSurfaceRequest>,
    overflow: Vec<ChildSurfaceRequest>,
    diagnostics: Vec<ChildSurfaceDiagnostic>,
}

/// Requests retain their dependencies on trigger geometry, ancestor policy
/// and descendant paint extents. Revisions publish with the finalized frame.
#[derive(Default)]
pub(in crate::shell::component) struct ChildSurfaceCache {
    root: Option<NodeId>,
    revision: Option<u64>,
    candidates: HashMap<NodeId, Candidate>,
    dependents: HashMap<NodeId, HashSet<NodeId>>,
    references: HashMap<String, (NodeId, Vec<usize>)>,
    names: HashMap<NodeId, Vec<String>>,
    requests: Vec<ChildSurfaceRequest>,
    diagnostics: Vec<ChildSurfaceDiagnostic>,
}

impl ChildSurfaceCache {
    pub(in crate::shell::component) fn matches(&self, tree: &WidgetNode, revision: u64) -> bool {
        self.root == Some(tree.id) && self.revision == Some(revision)
    }

    pub(in crate::shell::component) fn requests(&self) -> &[ChildSurfaceRequest] {
        &self.requests
    }
    pub(in crate::shell::component) fn diagnostics(&self) -> &[ChildSurfaceDiagnostic] {
        &self.diagnostics
    }

    pub(in crate::shell::component) fn update(
        &mut self,
        tree: &WidgetNode,
        snapshot: Option<&FrameSnapshot>,
        dirty: &HashSet<NodeId>,
        structural: bool,
        revision: u64,
    ) {
        let mut rebuild = structural || self.root != Some(tree.id) || self.revision.is_none();
        let mut paths = HashMap::new();
        if !rebuild {
            for &id in dirty {
                let Some(path) = snapshot.and_then(|snapshot| snapshot.node_path(id)) else {
                    rebuild = true;
                    break;
                };
                let Some(nodes) = nodes_at_path(tree, path) else {
                    rebuild = true;
                    break;
                };
                let node = *nodes.last().unwrap();
                if node.id != id || self.names.get(&id) != Some(&reference_names(node)) {
                    rebuild = true;
                    break;
                }
                paths.insert(id, path.to_vec());
            }
        }
        let mut affected = HashSet::new();
        if rebuild {
            *self = Self::default();
            fn record(cache: &mut ChildSurfaceCache, node: &WidgetNode, path: &mut Vec<usize>) {
                visit();
                let names = reference_names(node);
                for name in &names {
                    cache
                        .references
                        .entry(name.clone())
                        .or_insert((node.id, path.clone()));
                }
                cache.names.insert(node.id, names);
                if is_candidate(node) {
                    cache.candidates.insert(
                        node.id,
                        Candidate {
                            path: path.clone(),
                            ..Default::default()
                        },
                    );
                }
                for (index, child) in node.children.iter().enumerate() {
                    path.push(index);
                    record(cache, child, path);
                    path.pop();
                }
            }
            record(self, tree, &mut Vec::new());
            affected.extend(self.candidates.keys().copied());
        } else {
            for &id in dirty {
                if let Some(candidates) = self.dependents.get(&id) {
                    affected.extend(candidates);
                }
                let path = &paths[&id];
                let node = *nodes_at_path(tree, path).unwrap().last().unwrap();
                if is_candidate(node) && !self.candidates.contains_key(&id) {
                    self.candidates.insert(
                        id,
                        Candidate {
                            path: path.clone(),
                            ..Default::default()
                        },
                    );
                    affected.insert(id);
                }
            }
        }
        for id in affected.iter().copied() {
            let Some(mut candidate) = self.candidates.remove(&id) else {
                continue;
            };
            for dependency in &candidate.dependencies {
                if let Some(dependents) = self.dependents.get_mut(dependency) {
                    dependents.remove(&id);
                }
            }
            candidate.dependencies.clear();
            candidate.explicit.clear();
            candidate.overflow.clear();
            candidate.diagnostics.clear();
            if let Some(nodes) = nodes_at_path(tree, &candidate.path) {
                let node = *nodes.last().unwrap();
                if node.id == id && is_candidate(node) {
                    self.evaluate(tree, &nodes, &mut candidate);
                    for dependency in &candidate.dependencies {
                        self.dependents.entry(*dependency).or_default().insert(id);
                    }
                    self.candidates.insert(id, candidate);
                }
            }
        }
        if rebuild || !affected.is_empty() {
            let mut ordered: Vec<_> = self.candidates.values().collect();
            ordered.sort_by(|a, b| a.path.cmp(&b.path));
            self.requests.clear();
            self.diagnostics.clear();
            if let Some(root) = ordered
                .first()
                .filter(|candidate| candidate.path.is_empty())
            {
                self.requests.extend(root.explicit.iter().cloned());
            }
            for candidate in &ordered {
                self.requests.extend(candidate.overflow.iter().cloned());
            }
            for candidate in &ordered {
                if !candidate.path.is_empty() {
                    self.requests.extend(candidate.explicit.iter().cloned());
                }
                self.diagnostics
                    .extend(candidate.diagnostics.iter().cloned());
            }
        }
        self.root = Some(tree.id);
        self.revision = Some(revision);
    }

    fn evaluate(&self, tree: &WidgetNode, nodes: &[&WidgetNode], candidate: &mut Candidate) {
        let node = *nodes.last().unwrap();
        let ancestors = &nodes[..nodes.len() - 1];
        candidate
            .dependencies
            .extend(nodes.iter().map(|node| node.id));
        fn descendants(node: &WidgetNode, ids: &mut HashSet<NodeId>) {
            visit();
            ids.insert(node.id);
            for child in &node.children {
                descendants(child, ids);
            }
        }
        let Some(key) = node.mesh_key() else {
            return;
        };
        let path = candidate.path.clone();
        let bounds =
            || mesh_core_interaction::find_node_bounds_by_path(tree, &path, node.id, visit);
        let explicit_blocked = ancestors.iter().any(|ancestor| {
            ancestor.is_promoted_window()
                || (source_element_tag(ancestor) == "popover"
                    && !popover_is_open(ancestor)
                    && ancestor.id != tree.id)
        });
        let overflow_blocked = tree.is_promoted_window()
            || ancestors.iter().any(|ancestor| {
                (source_element_tag(ancestor) == "popover" && ancestor.id != tree.id)
                    || ancestor.computed_style.overflow_x.clips_contents()
                    || ancestor.computed_style.overflow_y.clips_contents()
            });
        let popup_extents = !explicit_blocked
            && !node.is_promoted_window()
            && source_element_tag(node) == "popover"
            && (popover_is_open(node) || node.id == tree.id);
        let overflow_extents = !candidate.path.is_empty()
            && !overflow_blocked
            && source_element_tag(node) != "popover"
            && node.computed_style.position == mesh_core_elements::style::Position::Absolute;
        if popup_extents || overflow_extents {
            descendants(node, &mut candidate.dependencies);
        }
        if !explicit_blocked {
            if node.is_promoted_window() {
                candidate.explicit.push(ChildSurfaceRequest {
                    node_key: key.to_owned(),
                    kind: ChildSurfaceKind::Window,
                    anchor_rect: bounds_to_i32_rect((
                        node.layout.x,
                        node.layout.y,
                        node.layout.x + node.layout.width,
                        node.layout.y + node.layout.height,
                    )),
                    content_size: content_size(node),
                    content_padding: (0, 0, 0, 0),
                    placement: PopoverPlacement::default(),
                    popover_trigger: None,
                });
            } else if source_element_tag(node) == "popover"
                && (popover_is_open(node) || node.id == tree.id)
            {
                self.popover_request(tree, node, candidate, bounds);
            }
        }
        if !candidate.path.is_empty()
            && !overflow_blocked
            && source_element_tag(node) != "popover"
            && node.computed_style.position == mesh_core_elements::style::Position::Absolute
            && let Some(bounds) = bounds()
            && bounds_escape_container(
                bounds,
                (
                    tree.layout.x,
                    tree.layout.y,
                    tree.layout.x + tree.layout.width,
                    tree.layout.y + tree.layout.height,
                ),
            )
        {
            let anchor_rect = bounds_to_i32_rect(bounds);
            let mut placement = PopoverPlacement::default();
            placement.anchor = mesh_core_elements::PopoverAnchor::TopLeft;
            placement.gravity = mesh_core_elements::PopoverGravity::TopLeft;
            candidate.overflow.push(ChildSurfaceRequest {
                node_key: key.to_owned(),
                kind: ChildSurfaceKind::Overflow,
                anchor_rect,
                content_size: (anchor_rect.2 as u32, anchor_rect.3 as u32),
                content_padding: popover_content_padding(node),
                placement,
                popover_trigger: None,
            });
        }
    }

    fn popover_request(
        &self,
        tree: &WidgetNode,
        node: &WidgetNode,
        candidate: &mut Candidate,
        own_bounds: impl FnOnce() -> Option<(f32, f32, f32, f32)>,
    ) {
        let key = node.mesh_key().unwrap();
        let placement = match PopoverPlacement::from_node(node) {
            Ok(placement) => placement,
            Err(diagnostics) => {
                candidate
                    .diagnostics
                    .extend(diagnostics.into_iter().map(|diagnostic| {
                        ChildSurfaceDiagnostic::Placement {
                            node_key: key.to_owned(),
                            diagnostic,
                        }
                    }));
                return;
            }
        };
        let trigger = popover_anchor_reference(node).map(|reference| PopoverTriggerReference {
            reference: reference.to_owned(),
        });
        if trigger.is_none()
            && let Some(reference) = popover_anchor_attribute_value(node)
        {
            candidate
                .diagnostics
                .push(ChildSurfaceDiagnostic::MissingTrigger {
                    node_key: key.to_owned(),
                    reference: PopoverTriggerReference {
                        reference: reference.to_owned(),
                    },
                });
            return;
        }
        let bounds = if let Some(trigger) = &trigger {
            self.references
                .get(&trigger.reference)
                .and_then(|(_, path)| {
                    let nodes = nodes_at_path(tree, path)?;
                    candidate
                        .dependencies
                        .extend(nodes.iter().map(|node| node.id));
                    let node = *nodes.last()?;
                    let (mut x, mut y) = (0.0, 0.0);
                    for ancestor in &nodes[..nodes.len() - 1] {
                        let scroll = ancestor.resolved_scroll_metrics();
                        x -= scroll.x;
                        y -= scroll.y;
                    }
                    Some((
                        node.layout.x + x,
                        node.layout.y + y,
                        node.layout.x + x + node.layout.width,
                        node.layout.y + y + node.layout.height,
                    ))
                })
        } else {
            own_bounds()
        };
        if let Some(bounds) = bounds {
            candidate.explicit.push(ChildSurfaceRequest {
                node_key: key.to_owned(),
                kind: ChildSurfaceKind::Popover,
                anchor_rect: bounds_to_i32_rect(bounds),
                content_size: content_size(node),
                content_padding: popover_content_padding(node),
                placement,
                popover_trigger: trigger,
            });
        } else if let Some(reference) = trigger {
            candidate
                .diagnostics
                .push(ChildSurfaceDiagnostic::MissingTrigger {
                    node_key: key.to_owned(),
                    reference,
                });
        }
    }
}

fn content_size(node: &WidgetNode) -> (u32, u32) {
    let scale = mesh_core_render::FractionalScale::identity();
    (
        scale.physical_extent_f32(node.layout.width),
        scale.physical_extent_f32(node.layout.height),
    )
}
