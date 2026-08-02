use super::StyleResolver;
use super::attrs::*;
use super::cache::*;
use super::index::*;
use super::matching::*;
use crate::style::*;
use mesh_core_component::style::StyleRule;

impl<'a> StyleResolver<'a> {
    /// Re-resolves the computed style of `node` and its descendants, reusing a
    /// caller-provided index. The index must have been built from the same
    /// `rules` slice; this is verified with `is_for()` and the index is rebuilt
    /// in place if not.
    pub fn restyle_subtree_cached(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index_cache: &mut Option<StyleRuleIndex>,
    ) {
        let index = ensure_index(rules, index_cache);
        self.restyle_subtree_with_index(node, rules, index, context, None);
    }

    pub fn restyle_subtree_cached_profiled(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index_cache: &mut Option<StyleRuleIndex>,
        attribution: &mut StyleRuleAttribution,
    ) {
        let index = ensure_index(rules, index_cache);
        self.restyle_subtree_with_index_profiled(node, rules, index, context, None, attribution);
    }

    /// Re-resolves the computed style of every child of `node` (but not `node`
    /// itself), reusing a caller-provided index.
    pub fn restyle_subtree_children_cached(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index_cache: &mut Option<StyleRuleIndex>,
    ) {
        let index = ensure_index(rules, index_cache);
        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_subtree_with_index(child, rules, index, context, Some(&parent));
        }
    }

    pub fn restyle_subtree_children_cached_profiled(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index_cache: &mut Option<StyleRuleIndex>,
        attribution: &mut StyleRuleAttribution,
    ) {
        let index = ensure_index(rules, index_cache);
        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_subtree_with_index_profiled(
                child,
                rules,
                index,
                context,
                Some(&parent),
                attribution,
            );
        }
    }

    pub(super) fn restyle_subtree_with_index(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        parent_style: Option<&ParentInheritedStyle>,
    ) {
        let attrs = StyleNodeAttrs::from_node(node);
        node.computed_style = self
            .resolve_node_style_with_attrs_indexed_no_diagnostics(rules, index, &attrs, context);
        if let Some(parent) = parent_style {
            inherit_retained_text_style(&mut node.computed_style, parent);
        }

        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_subtree_with_index(child, rules, index, context, Some(&parent));
        }
    }

    pub(super) fn restyle_subtree_with_index_profiled(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        parent_style: Option<&ParentInheritedStyle>,
        attribution: &mut StyleRuleAttribution,
    ) {
        let attrs = StyleNodeAttrs::from_node(node);
        node.computed_style = self.resolve_node_style_with_attrs_indexed_inner(
            rules,
            index,
            &attrs,
            context,
            None,
            Some(attribution),
        );
        if let Some(parent) = parent_style {
            inherit_retained_text_style(&mut node.computed_style, parent);
        }
        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_subtree_with_index_profiled(
                child,
                rules,
                index,
                context,
                Some(&parent),
                attribution,
            );
        }
    }

    pub fn restyle_subtree_for_ids(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
    ) {
        let index = StyleRuleIndex::new(rules);
        self.restyle_subtree_for_ids_with_index(node, rules, &index, context, target_ids);
    }

    pub fn restyle_subtree_for_ids_cached(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index: &mut Option<StyleRuleIndex>,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
    ) {
        let idx = ensure_index(rules, index);
        self.restyle_subtree_for_ids_with_index(node, rules, idx, context, target_ids);
    }

    pub fn restyle_subtree_for_ids_cached_profiled(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index: &mut Option<StyleRuleIndex>,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
        attribution: &mut StyleRuleAttribution,
    ) {
        let idx = ensure_index(rules, index);
        self.restyle_subtree_for_ids_with_index_and_inheritance_profiled(
            node,
            rules,
            idx,
            context,
            target_ids,
            None,
            false,
            attribution,
        );
    }

    pub(super) fn restyle_subtree_for_ids_with_index(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
    ) {
        self.restyle_subtree_for_ids_with_index_and_inheritance(
            node, rules, index, context, target_ids, None, false,
        );
    }

    pub(super) fn restyle_subtree_for_ids_with_index_and_inheritance(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
        parent_style: Option<&ParentInheritedStyle>,
        inherited_dirty: bool,
    ) {
        let is_target = target_ids.contains(&node.id);
        // A node should have its style recomputed if it is a direct target or
        // an inherited field changed on its parent. Parent context itself is
        // carried independently so a deep direct target retains inherited
        // values without forcing clean ancestors through style resolution.
        let should_restyle = is_target || inherited_dirty;

        if should_restyle {
            let previous_inherited = ParentInheritedStyle::from(&node.computed_style);
            // Recompute this node's style.
            // For target nodes: apply new pseudo-class rules.
            // For descendants of targets: inherit updated values from the
            // restyled ancestor.
            let attrs = StyleNodeAttrs::from_node(node);
            node.computed_style = self.resolve_node_style_with_attrs_indexed_no_diagnostics(
                rules, index, &attrs, context,
            );
            if let Some(parent) = parent_style {
                inherit_retained_text_style(&mut node.computed_style, parent);
            }

            // Descendants only need their rules re-resolved when an inherited
            // field actually changed. For the common background/border/opacity
            // interaction change, continue searching for other direct targets
            // without re-applying every descendant's declarations.
            let child_parent = ParentInheritedStyle::from(&node.computed_style);
            let inheritance_changed = previous_inherited != child_parent;
            for child in &mut node.children {
                self.restyle_subtree_for_ids_with_index_and_inheritance(
                    child,
                    rules,
                    index,
                    context,
                    target_ids,
                    Some(&child_parent),
                    inheritance_changed,
                );
            }
        } else {
            // This node is not a target and is not in an affected subtree.
            // Don't restyle it, but keep recursing — target nodes may be
            // deeper in the tree.
            let child_parent = ParentInheritedStyle::from(&node.computed_style);
            for child in &mut node.children {
                self.restyle_subtree_for_ids_with_index_and_inheritance(
                    child,
                    rules,
                    index,
                    context,
                    target_ids,
                    Some(&child_parent),
                    false,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn restyle_subtree_for_ids_with_index_and_inheritance_profiled(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
        parent_style: Option<&ParentInheritedStyle>,
        inherited_dirty: bool,
        attribution: &mut StyleRuleAttribution,
    ) {
        let should_restyle = target_ids.contains(&node.id) || inherited_dirty;
        if should_restyle {
            let previous_inherited = ParentInheritedStyle::from(&node.computed_style);
            let attrs = StyleNodeAttrs::from_node(node);
            node.computed_style = self.resolve_node_style_with_attrs_indexed_inner(
                rules,
                index,
                &attrs,
                context,
                None,
                Some(attribution),
            );
            if let Some(parent) = parent_style {
                inherit_retained_text_style(&mut node.computed_style, parent);
            }
            let child_parent = ParentInheritedStyle::from(&node.computed_style);
            let inheritance_changed = previous_inherited != child_parent;
            for child in &mut node.children {
                self.restyle_subtree_for_ids_with_index_and_inheritance_profiled(
                    child,
                    rules,
                    index,
                    context,
                    target_ids,
                    Some(&child_parent),
                    inheritance_changed,
                    attribution,
                );
            }
        } else {
            let child_parent = ParentInheritedStyle::from(&node.computed_style);
            for child in &mut node.children {
                self.restyle_subtree_for_ids_with_index_and_inheritance_profiled(
                    child,
                    rules,
                    index,
                    context,
                    target_ids,
                    Some(&child_parent),
                    false,
                    attribution,
                );
            }
        }
    }
}
