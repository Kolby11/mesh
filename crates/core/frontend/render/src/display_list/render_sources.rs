use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use mesh_core_elements::style::{Dimension, Edges, Position};
use mesh_core_elements::{LayoutEngine, NodeId, Prop, WidgetNode, node_eligibility};

use super::batch_index::BatchIndex;
use super::build::build_paint_subtree;
use super::*;

const MAX_SOURCES: usize = 16;
const MAX_OFFSCREEN_NODES: usize = 16_384;
const MAX_TARGET_EXTENT: u16 = 2048;

#[derive(Debug, Default)]
pub(super) struct RenderSourceInputs {
    paths: BTreeMap<NodeId, Box<[usize]>>,
}

impl RenderSourceInputs {
    pub(super) fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub(super) fn update(
        &mut self,
        root: &WidgetNode,
        index: &BatchIndex,
        dirty: &HashSet<NodeId>,
        rescan: bool,
    ) {
        if rescan || !index.matches_root(root) {
            self.paths.clear();
            record_inputs(root, &mut Vec::new(), &mut self.paths);
            return;
        }
        for id in dirty {
            let Some(path) = index.node_path(*id) else {
                continue;
            };
            let Some(node) = node_at_path(root, path, false) else {
                continue;
            };
            if node.id == *id && is_source_input(node) {
                self.paths.entry(*id).or_insert_with(|| path.into());
            } else {
                self.paths.remove(id);
            }
        }
    }

    pub(super) fn collect(
        &self,
        root: &WidgetNode,
        commands: &PaintSequence<DisplayPaintCommand>,
        subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
        previous: &[NamedRenderSource],
    ) -> Arc<[NamedRenderSource]> {
        let mut sources = Vec::new();
        for command in commands.iter().filter(|command| {
            command.kind == DisplayPaintCommandKind::Node && command.node.render_source.is_some()
        }) {
            if sources.len() == MAX_SOURCES {
                return Arc::default();
            }
            let source_commands = subtrees
                .get(&command.node.id)
                .map(|subtree| Arc::clone(&subtree.commands))
                .or_else(|| {
                    // A reused branch can leave a descendant producer outside the updated subtree map.
                    previous.iter().find_map(|cached| match cached {
                        NamedRenderSource::Capture {
                            node,
                            commands,
                            viewport: None,
                        } if Arc::ptr_eq(node, &command.node) => Some(Arc::clone(commands)),
                        _ => None,
                    })
                })
                .unwrap_or_default();
            sources.push(NamedRenderSource::Capture {
                node: Arc::clone(&command.node),
                commands: source_commands,
                viewport: None,
            });
        }

        let mut remaining_nodes = MAX_OFFSCREEN_NODES;
        let mut aliases = Vec::new();
        for (id, path) in &self.paths {
            let Some(node) = node_at_path(root, path, true).filter(|node| node.id == *id) else {
                continue;
            };
            if node.is_render_target() && node.attributes.text(Prop::RenderSource).is_some() {
                if sources.len() == MAX_SOURCES {
                    return Arc::default();
                }
                if let Some(source) = build_offscreen_source(node, &mut remaining_nodes) {
                    sources.push(source);
                }
            }
            if let Some(imports) = node.render_imports() {
                if aliases.len() + imports.names.len() > MAX_SOURCES {
                    return Arc::default();
                }
                aliases.extend(imports.names.iter().map(|name| NamedRenderSource::Alias {
                    key: RenderSourceKey {
                        scope: Arc::clone(&imports.parent),
                        name: Arc::clone(&name.local),
                    },
                    exported: RenderSourceKey {
                        scope: Arc::clone(&imports.child),
                        name: Arc::clone(&name.exported),
                    },
                }));
            }
        }
        sources.extend(aliases);
        sources.into()
    }
}

fn is_source_input(node: &WidgetNode) -> bool {
    node.is_render_target() || node.render_imports().is_some()
}

fn record_inputs(
    node: &WidgetNode,
    path: &mut Vec<usize>,
    paths: &mut BTreeMap<NodeId, Box<[usize]>>,
) {
    if is_source_input(node) {
        paths.insert(node.id, path.as_slice().into());
    }
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        record_inputs(child, path, paths);
        path.pop();
    }
}

fn node_at_path<'a>(
    mut node: &'a WidgetNode,
    path: &[usize],
    check_visibility: bool,
) -> Option<&'a WidgetNode> {
    for &index in path {
        if check_visibility && !node_eligibility(node).is_visible() {
            return None;
        }
        node = node.children.get(index)?;
    }
    Some(node)
}

fn build_offscreen_source(
    source: &WidgetNode,
    remaining_nodes: &mut usize,
) -> Option<NamedRenderSource> {
    let mut dimensions = source
        .attributes
        .text(Prop::RenderTarget)?
        .split_ascii_whitespace();
    let width = dimensions.next()?.parse::<u16>().ok()?;
    let height = dimensions.next()?.parse::<u16>().ok()?;
    if dimensions.next().is_some()
        || !(1..=MAX_TARGET_EXTENT).contains(&width)
        || !(1..=MAX_TARGET_EXTENT).contains(&height)
    {
        return None;
    }
    *remaining_nodes = count_offscreen_nodes(source, *remaining_nodes)?;
    let mut target = source.clone();
    target.attributes.remove(Prop::RenderTarget);
    if !node_eligibility(&target).is_visible() {
        return None;
    }

    let (logical_width, logical_height) = (f32::from(width), f32::from(height));
    let style = &mut target.computed_style;
    style.width = Dimension::Px(logical_width);
    style.height = Dimension::Px(logical_height);
    style.min_width = style.width;
    style.min_height = style.height;
    style.max_width = style.width;
    style.max_height = style.height;
    style.margin = Edges::zero();
    style.position = Position::Static;
    style.inset_left = None;
    style.inset_right = None;
    style.inset_top = None;
    style.inset_bottom = None;
    style.transform = Default::default();
    LayoutEngine::compute_with_measurer(
        &mut target,
        logical_width,
        logical_height,
        Some(&crate::SharedTextMeasurer),
    );
    let clip = DisplayListClip {
        x: 0,
        y: 0,
        width: i32::from(width),
        height: i32::from(height),
    };
    let subtree = build_paint_subtree(
        &target,
        0.0,
        0.0,
        clip,
        clip,
        true,
        false,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut LocalReuseMetrics::default(),
        BackdropBlurPolicy::InSurfaceFilter,
        false,
        false,
    );
    let node = subtree
        .commands
        .iter()
        .find(|command| command.kind == DisplayPaintCommandKind::Node)?;
    node.node.render_source.as_ref()?;
    Some(NamedRenderSource::Capture {
        node: Arc::clone(&node.node),
        commands: Arc::clone(&subtree.commands),
        viewport: Some(clip),
    })
}

fn count_offscreen_nodes(node: &WidgetNode, budget: usize) -> Option<usize> {
    node.children
        .iter()
        .try_fold(budget.checked_sub(1)?, |left, child| {
            count_offscreen_nodes(child, left)
        })
}

pub(super) fn is_extended_source(source: &NamedRenderSource) -> bool {
    !matches!(source, NamedRenderSource::Capture { viewport: None, .. })
}

pub(super) fn same_sources(previous: &[NamedRenderSource], next: &[NamedRenderSource]) -> bool {
    previous.len() == next.len()
        && previous
            .iter()
            .zip(next)
            .all(|(old, new)| match (old, new) {
                (
                    NamedRenderSource::Capture {
                        node: old_node,
                        commands: old_commands,
                        ..
                    },
                    NamedRenderSource::Capture {
                        node: new_node,
                        commands: new_commands,
                        ..
                    },
                ) => Arc::ptr_eq(old_node, new_node) && Arc::ptr_eq(old_commands, new_commands),
                (
                    NamedRenderSource::Alias {
                        key: old_key,
                        exported: old_export,
                    },
                    NamedRenderSource::Alias {
                        key: new_key,
                        exported: new_export,
                    },
                ) => old_key == new_key && old_export == new_export,
                _ => false,
            })
}
