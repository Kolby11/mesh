//! Retained layout through Taffy's public low-level traits. Layout deltas are
//! captured at `set_unrounded_layout`, including clean siblings moved by flex.

use super::*;
use taffy::{
    Cache, CacheTree, Layout, LayoutInput, LayoutOutput, LayoutPartialTree, TraversePartialTree,
};

struct NodeData {
    style: taffy_style::Style,
    layout: Layout,
    cache: Cache,
    context: Option<NodeId>,
    parent: Option<TaffyNodeId>,
    children: Vec<TaffyNodeId>,
    observed_epoch: u64,
    observed_slot: usize,
}

fn key(id: TaffyNodeId) -> slotmap::DefaultKey {
    id.into()
}

/// Retained Taffy input and cache arena with producer-observed, unrounded layouts.
/// Mutation methods invalidate ancestor caches; each compute reports exact
/// before/after layouts without a separate traversal of the layout tree.
pub struct ObservedLayoutTree {
    nodes: slotmap::SlotMap<slotmap::DefaultKey, NodeData>,
    pub(super) changes: Vec<(TaffyNodeId, Layout, Layout)>,
    epoch: u64,
}

impl ObservedLayoutTree {
    pub fn new() -> Self {
        Self {
            nodes: slotmap::SlotMap::new(),
            changes: Vec::new(),
            epoch: 0,
        }
    }

    pub fn new_leaf(
        &mut self,
        style: taffy_style::Style,
    ) -> Result<TaffyNodeId, taffy::TaffyError> {
        Ok(self
            .nodes
            .insert(NodeData {
                style,
                layout: Layout::default(),
                cache: Cache::new(),
                context: None,
                parent: None,
                children: Vec::new(),
                observed_epoch: 0,
                observed_slot: 0,
            })
            .into())
    }

    pub fn new_leaf_with_context(
        &mut self,
        style: taffy_style::Style,
        context: NodeId,
    ) -> Result<TaffyNodeId, taffy::TaffyError> {
        let id = self.new_leaf(style)?;
        self.set_node_context(id, Some(context))?;
        Ok(id)
    }

    pub fn new_with_children(
        &mut self,
        style: taffy_style::Style,
        children: &[TaffyNodeId],
    ) -> Result<TaffyNodeId, taffy::TaffyError> {
        let id = self.new_leaf(style)?;
        self.set_children(id, children)?;
        Ok(id)
    }

    pub fn set_children(
        &mut self,
        id: TaffyNodeId,
        children: &[TaffyNodeId],
    ) -> Result<(), taffy::TaffyError> {
        self.style(id)?;
        for child in children {
            self.style(*child)?;
        }
        for old in std::mem::take(&mut self.nodes[key(id)].children) {
            if self.parent(old) == Some(id) {
                self.nodes[key(old)].parent = None;
            }
        }
        for &child in children {
            if let Some(parent) = self.parent(child) {
                self.nodes[key(parent)]
                    .children
                    .retain(|other| *other != child);
                self.mark_dirty(parent)?;
            }
            self.nodes[key(child)].parent = Some(id);
        }
        self.nodes[key(id)].children.extend_from_slice(children);
        self.mark_dirty(id)
    }

    pub fn remove(&mut self, id: TaffyNodeId) -> Result<TaffyNodeId, taffy::TaffyError> {
        self.style(id)?;
        if let Some(parent) = self.parent(id) {
            self.nodes[key(parent)]
                .children
                .retain(|child| *child != id);
            self.mark_dirty(parent)?;
        }
        let node = self.nodes.remove(key(id)).expect("validated layout node");
        for child in node.children {
            self.nodes[key(child)].parent = None;
        }
        Ok(id)
    }

    pub fn parent(&self, id: TaffyNodeId) -> Option<TaffyNodeId> {
        self.nodes.get(key(id))?.parent
    }
    pub fn children(&self, id: TaffyNodeId) -> Result<Vec<TaffyNodeId>, taffy::TaffyError> {
        self.style(id)?;
        Ok(self.nodes[key(id)].children.clone())
    }
    pub fn total_node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.changes.clear();
    }
    pub fn style(&self, id: TaffyNodeId) -> Result<&taffy_style::Style, taffy::TaffyError> {
        self.nodes
            .get(key(id))
            .map(|node| &node.style)
            .ok_or(taffy::TaffyError::InvalidInputNode(id))
    }
    pub fn set_style(
        &mut self,
        id: TaffyNodeId,
        style: taffy_style::Style,
    ) -> Result<(), taffy::TaffyError> {
        self.style(id)?;
        self.nodes[key(id)].style = style;
        self.mark_dirty(id)
    }
    pub fn get_node_context(&self, id: TaffyNodeId) -> Option<&NodeId> {
        self.nodes.get(key(id))?.context.as_ref()
    }
    pub fn set_node_context(
        &mut self,
        id: TaffyNodeId,
        context: Option<NodeId>,
    ) -> Result<(), taffy::TaffyError> {
        self.style(id)?;
        self.nodes[key(id)].context = context;
        self.mark_dirty(id)
    }
    pub fn mark_dirty(&mut self, id: TaffyNodeId) -> Result<(), taffy::TaffyError> {
        self.style(id)?;
        let mut current = Some(id);
        while let Some(id) = current {
            crate::frame_work::visit(crate::frame_work::Pass::LayoutProducer);
            if matches!(
                self.nodes[key(id)].cache.clear(),
                taffy::tree::ClearState::AlreadyEmpty
            ) {
                break;
            }
            current = self.parent(id);
        }
        Ok(())
    }
    pub fn dirty(&self, id: TaffyNodeId) -> Result<bool, taffy::TaffyError> {
        self.style(id)?;
        Ok(self.nodes[key(id)].cache.is_empty())
    }
    pub fn layout(&self, id: TaffyNodeId) -> Result<&Layout, taffy::TaffyError> {
        self.style(id)?;
        Ok(&self.nodes[key(id)].layout)
    }
    pub fn unrounded_layout(&self, id: TaffyNodeId) -> &Layout {
        &self.nodes[key(id)].layout
    }
    pub fn compute_layout_with_measure<F>(
        &mut self,
        root: TaffyNodeId,
        available: TaffySize<TaffyAvailableSpace>,
        measure: F,
    ) -> Result<(), taffy::TaffyError>
    where
        F: FnMut(
            TaffySize<Option<f32>>,
            TaffySize<TaffyAvailableSpace>,
            TaffyNodeId,
            Option<&mut NodeId>,
            &taffy_style::Style,
        ) -> TaffySize<f32>,
    {
        self.style(root)?;
        self.changes.clear();
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            for node in self.nodes.values_mut() {
                crate::frame_work::visit(crate::frame_work::Pass::LayoutProducer);
                node.observed_epoch = 0;
            }
            self.epoch = 1;
        }
        taffy::compute_root_layout(
            &mut View {
                tree: self,
                measure,
            },
            root,
            available,
        );
        Ok(())
    }
}

impl Default for ObservedLayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

struct View<'a, F> {
    tree: &'a mut ObservedLayoutTree,
    measure: F,
}

fn producer_child(_: &TaffyNodeId) {
    crate::frame_work::visit(crate::frame_work::Pass::LayoutProducer);
}

impl<F> TraversePartialTree for View<'_, F> {
    type ChildIter<'a>
        = std::iter::Inspect<std::iter::Copied<std::slice::Iter<'a, TaffyNodeId>>, fn(&TaffyNodeId)>
    where
        Self: 'a;
    fn child_ids(&self, id: TaffyNodeId) -> Self::ChildIter<'_> {
        self.tree.nodes[key(id)]
            .children
            .iter()
            .copied()
            .inspect(producer_child as fn(&TaffyNodeId))
    }
    fn child_count(&self, id: TaffyNodeId) -> usize {
        self.tree.nodes[key(id)].children.len()
    }
    fn get_child_id(&self, id: TaffyNodeId, index: usize) -> TaffyNodeId {
        crate::frame_work::visit(crate::frame_work::Pass::LayoutProducer);
        self.tree.nodes[key(id)].children[index]
    }
}

impl<F> CacheTree for View<'_, F> {
    fn cache_get(&self, id: TaffyNodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.tree.nodes[key(id)].cache.get(input)
    }
    fn cache_store(&mut self, id: TaffyNodeId, input: &LayoutInput, output: LayoutOutput) {
        self.tree
            .nodes
            .get_mut(key(id))
            .unwrap()
            .cache
            .store(input, output);
    }
    fn cache_clear(&mut self, id: TaffyNodeId) {
        self.tree.nodes.get_mut(key(id)).unwrap().cache.clear();
    }
}

impl<F> View<'_, F>
where
    F: FnMut(
        TaffySize<Option<f32>>,
        TaffySize<TaffyAvailableSpace>,
        TaffyNodeId,
        Option<&mut NodeId>,
        &taffy_style::Style,
    ) -> TaffySize<f32>,
{
    fn compute(
        &mut self,
        id: TaffyNodeId,
        input: LayoutInput,
        block: Option<&mut taffy::BlockContext<'_>>,
    ) -> LayoutOutput {
        crate::frame_work::visit(crate::frame_work::Pass::LayoutProducer);
        if input.run_mode == taffy::RunMode::PerformHiddenLayout {
            return taffy::compute_hidden_layout(self, id);
        }
        taffy::compute_cached_layout(self, id, input, |view, id, input| {
            let display = view.tree.nodes[key(id)].style.display;
            match (display, view.child_count(id) > 0) {
                (taffy_style::Display::None, _) => taffy::compute_hidden_layout(view, id),
                (taffy_style::Display::Flex, true) => {
                    taffy::compute_flexbox_layout(view, id, input)
                }
                (taffy_style::Display::Grid, true) => taffy::compute_grid_layout(view, id, input),
                (taffy_style::Display::Block, true) => {
                    taffy::compute_block_layout(view, id, input, block)
                }
                (_, false) => {
                    let node = &mut view.tree.nodes[key(id)];
                    let style = &node.style;
                    let mut context = node.context.as_mut();
                    let measure = &mut view.measure;
                    taffy::compute_leaf_layout(
                        input,
                        style,
                        |_, _| 0.0,
                        |known, available| {
                            measure(known, available, id, context.as_deref_mut(), style)
                        },
                    )
                }
            }
        })
    }
}

impl<F> LayoutPartialTree for View<'_, F>
where
    F: FnMut(
        TaffySize<Option<f32>>,
        TaffySize<TaffyAvailableSpace>,
        TaffyNodeId,
        Option<&mut NodeId>,
        &taffy_style::Style,
    ) -> TaffySize<f32>,
{
    type CoreContainerStyle<'a>
        = &'a taffy_style::Style
    where
        Self: 'a;
    type CustomIdent = String;
    fn get_core_container_style(&self, id: TaffyNodeId) -> Self::CoreContainerStyle<'_> {
        &self.tree.nodes[key(id)].style
    }
    fn set_unrounded_layout(&mut self, id: TaffyNodeId, layout: &Layout) {
        crate::frame_work::visit(crate::frame_work::Pass::LayoutProducerWrite);
        let node = self.tree.nodes.get_mut(key(id)).unwrap();
        if node.layout != *layout {
            // An epoch deduplicates repeated writes without clearing a table
            // sized for a previous, potentially much larger geometry change.
            if node.observed_epoch != self.tree.epoch {
                node.observed_epoch = self.tree.epoch;
                node.observed_slot = self.tree.changes.len();
                self.tree.changes.push((id, node.layout, *layout));
            } else {
                self.tree.changes[node.observed_slot].2 = *layout;
            }
            node.layout = *layout;
        }
    }
    fn compute_child_layout(&mut self, id: TaffyNodeId, input: LayoutInput) -> LayoutOutput {
        self.compute(id, input, None)
    }
}

macro_rules! container {
    ($trait:ident, $container:ident, $item:ident, $get_container:ident, $get_item:ident) => {
        impl<F> taffy::$trait for View<'_, F>
        where
            F: FnMut(
                TaffySize<Option<f32>>,
                TaffySize<TaffyAvailableSpace>,
                TaffyNodeId,
                Option<&mut NodeId>,
                &taffy_style::Style,
            ) -> TaffySize<f32>,
        {
            type $container<'a>
                = &'a taffy_style::Style
            where
                Self: 'a;
            type $item<'a>
                = &'a taffy_style::Style
            where
                Self: 'a;
            fn $get_container(&self, id: TaffyNodeId) -> Self::$container<'_> {
                &self.tree.nodes[key(id)].style
            }
            fn $get_item(&self, id: TaffyNodeId) -> Self::$item<'_> {
                &self.tree.nodes[key(id)].style
            }
        }
    };
}
container!(
    LayoutFlexboxContainer,
    FlexboxContainerStyle,
    FlexboxItemStyle,
    get_flexbox_container_style,
    get_flexbox_child_style
);
container!(
    LayoutGridContainer,
    GridContainerStyle,
    GridItemStyle,
    get_grid_container_style,
    get_grid_child_style
);
impl<F> taffy::LayoutBlockContainer for View<'_, F>
where
    F: FnMut(
        TaffySize<Option<f32>>,
        TaffySize<TaffyAvailableSpace>,
        TaffyNodeId,
        Option<&mut NodeId>,
        &taffy_style::Style,
    ) -> TaffySize<f32>,
{
    type BlockContainerStyle<'a>
        = &'a taffy_style::Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a taffy_style::Style
    where
        Self: 'a;
    fn get_block_container_style(&self, id: TaffyNodeId) -> Self::BlockContainerStyle<'_> {
        &self.tree.nodes[key(id)].style
    }
    fn get_block_child_style(&self, id: TaffyNodeId) -> Self::BlockItemStyle<'_> {
        &self.tree.nodes[key(id)].style
    }
    fn compute_block_child_layout(
        &mut self,
        id: TaffyNodeId,
        input: LayoutInput,
        block: Option<&mut taffy::BlockContext<'_>>,
    ) -> LayoutOutput {
        self.compute(id, input, block)
    }
}

pub(super) trait LayoutStore {
    fn child_nodes(&self, id: TaffyNodeId) -> Result<Vec<TaffyNodeId>, taffy::TaffyError>;
    fn delete(&mut self, id: TaffyNodeId) -> Result<TaffyNodeId, taffy::TaffyError>;
    fn leaf(
        &mut self,
        style: taffy_style::Style,
        context: NodeId,
    ) -> Result<TaffyNodeId, taffy::TaffyError>;
    fn branch(
        &mut self,
        style: taffy_style::Style,
        children: &[TaffyNodeId],
    ) -> Result<TaffyNodeId, taffy::TaffyError>;
    fn rect(&self, id: TaffyNodeId) -> Result<&Layout, taffy::TaffyError>;
}
macro_rules! store {
    ($type:ty) => {
        impl LayoutStore for $type {
            fn child_nodes(&self, id: TaffyNodeId) -> Result<Vec<TaffyNodeId>, taffy::TaffyError> {
                self.children(id)
            }
            fn delete(&mut self, id: TaffyNodeId) -> Result<TaffyNodeId, taffy::TaffyError> {
                self.remove(id)
            }
            fn leaf(
                &mut self,
                style: taffy_style::Style,
                context: NodeId,
            ) -> Result<TaffyNodeId, taffy::TaffyError> {
                self.new_leaf_with_context(style, context)
            }
            fn branch(
                &mut self,
                style: taffy_style::Style,
                children: &[TaffyNodeId],
            ) -> Result<TaffyNodeId, taffy::TaffyError> {
                self.new_with_children(style, children)
            }
            fn rect(&self, id: TaffyNodeId) -> Result<&Layout, taffy::TaffyError> {
                self.layout(id)
            }
        }
    };
}
store!(TaffyTree<NodeId>);
store!(ObservedLayoutTree);

#[cfg(test)]
mod tests {
    use super::*;
    use taffy::prelude::TaffyAuto;

    #[test]
    fn public_adapter_matches_native_taffy_for_each_layout_algorithm() {
        for display in [
            taffy_style::Display::Flex,
            taffy_style::Display::Grid,
            taffy_style::Display::Block,
        ] {
            let mut observed = ObservedLayoutTree::new();
            let mut native = unrounded_taffy_tree();
            let mut observed_ids = Vec::new();
            let mut native_ids = Vec::new();
            for index in 0..16 {
                let style = taffy_style::Style {
                    size: TaffySize {
                        width: taffy_style::Dimension::length(8.25 + index as f32),
                        height: taffy_style::Dimension::length(4.75),
                    },
                    ..Default::default()
                };
                observed_ids.push(observed.new_leaf(style.clone()).unwrap());
                native_ids.push(native.new_leaf(style).unwrap());
            }
            let style = taffy_style::Style {
                display,
                size: TaffySize {
                    width: taffy_style::Dimension::length(160.5),
                    height: taffy_style::Dimension::AUTO,
                },
                ..Default::default()
            };
            let observed_root = observed
                .new_with_children(style.clone(), &observed_ids)
                .unwrap();
            let native_root = native.new_with_children(style, &native_ids).unwrap();
            for turn in 0..4 {
                let mut style = observed.style(observed_ids[3]).unwrap().clone();
                style.size.width =
                    taffy_style::Dimension::length(if turn % 2 == 0 { 30.75 } else { 11.25 });
                observed.set_style(observed_ids[3], style.clone()).unwrap();
                native.set_style(native_ids[3], style).unwrap();
                observed
                    .compute_layout_with_measure(
                        observed_root,
                        taffy_available_space(160.5, 100.0),
                        |_, _, _, _, _| TaffySize::ZERO,
                    )
                    .unwrap();
                native
                    .compute_layout(native_root, taffy_available_space(160.5, 100.0))
                    .unwrap();
                for (a, b) in observed_ids
                    .iter()
                    .chain([&observed_root])
                    .zip(native_ids.iter().chain([&native_root]))
                {
                    assert_eq!(
                        observed.layout(*a).unwrap(),
                        native.unrounded_layout(*b),
                        "{display:?}/{turn}"
                    );
                }
            }
            let stale = observed_ids[0];
            observed.remove(stale).unwrap();
            let replacement = observed.new_leaf(Default::default()).unwrap();
            assert_ne!(stale, replacement);
            assert!(observed.layout(stale).is_err());
        }
    }
}
