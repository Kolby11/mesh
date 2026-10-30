use super::StyleResolver;
use super::attrs::*;
use super::cache::*;
use super::index::*;
use super::matching::*;
use crate::style::*;
use mesh_core_component::style::StyleRule;

impl<'a> StyleResolver<'a> {
    fn finalized_style_reuse(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        parent: Option<&ParentInheritedStyle>,
    ) -> (bool, bool) {
        if !self.reuse_finalized || !node.finalized_style_pristine {
            return (false, false);
        }
        let Some(cached) = node.finalized_style.clone() else {
            return (false, false);
        };
        if !cached.same_environment(parent, self, index, context) {
            return (false, false);
        }
        if node.finalized_style_valid {
            return (true, true);
        }
        (
            cached.same_matching_rules(node, rules, index, context, self),
            false,
        )
    }

    fn remember_finalized_style(
        &self,
        node: &mut crate::tree::WidgetNode,
        index: &StyleRuleIndex,
        context: StyleContext,
        parent: Option<&ParentInheritedStyle>,
    ) {
        node.finalized_accessibility_valid &= node
            .children
            .iter()
            .all(|child| child.finalized_accessibility_valid);
        if !self.reuse_finalized && !self.incremental_matching {
            node.finalized_style = None;
            node.finalized_style_valid = false;
            return;
        }
        let unchanged = node.finalized_style.as_ref().is_some_and(|cached| {
            cached.matches_node(node) && cached.same_environment(parent, self, index, context)
        });
        if !unchanged {
            node.finalized_style = Some(std::sync::Arc::new(FinalizedStyle::new(
                node, parent, self, index, context,
            )));
        }
        node.finalized_style_valid = true;
        node.finalized_style_pristine = true;
    }

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
        crate::frame_work::visit(crate::frame_work::Pass::RestyleScan);
        if node.control_part().is_some() {
            // Styled with its host by `restyle_control_parts`.
            return;
        }
        let (reuse_node, reuse_subtree) =
            self.finalized_style_reuse(node, rules, index, context, parent_style);
        if reuse_subtree {
            return;
        }
        if !reuse_node {
            node.finalized_accessibility_valid = false;
        }
        if !reuse_node && !preserve_synthetic_surface_style(node, parent_style) {
            let attrs = StyleNodeAttrs::from_node(node);
            node.computed_style = self.resolve_node_style_with_attrs_indexed_inner(
                rules,
                index,
                &attrs,
                context,
                None,
                None,
                parent_style.map(|parent| &parent.custom_properties),
            );
            if let Some(parent) = parent_style {
                inherit_retained_text_style(&mut node.computed_style, parent);
            }
        }
        self.restyle_control_parts(node, rules, index, context, None, None);

        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_subtree_with_index(child, rules, index, context, Some(&parent));
        }
        self.remember_finalized_style(node, index, context, parent_style);
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
        crate::frame_work::visit(crate::frame_work::Pass::RestyleScan);
        if node.control_part().is_some() {
            // Styled with its host by `restyle_control_parts`.
            return;
        }
        let (reuse_node, reuse_subtree) =
            self.finalized_style_reuse(node, rules, index, context, parent_style);
        if reuse_subtree {
            return;
        }
        if !reuse_node {
            node.finalized_accessibility_valid = false;
        }
        if !reuse_node && !preserve_synthetic_surface_style(node, parent_style) {
            let attrs = StyleNodeAttrs::from_node(node);
            node.computed_style = self.resolve_node_style_with_attrs_indexed_inner(
                rules,
                index,
                &attrs,
                context,
                None,
                Some(attribution),
                parent_style.map(|parent| &parent.custom_properties),
            );
            if let Some(parent) = parent_style {
                inherit_retained_text_style(&mut node.computed_style, parent);
            }
        }
        self.restyle_control_parts(node, rules, index, context, None, None);
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
        self.remember_finalized_style(node, index, context, parent_style);
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

    /// Restyles `targets` reached through `snapshot`'s child paths, without
    /// walking clean branches. Selectors have no combinators, so only targets
    /// and the descendants whose inherited values changed are re-resolved;
    /// each re-resolved ID is added to `restyled`.
    ///
    /// Returns `false` without mutating the tree when a target's path no
    /// longer leads to it. Targets absent from the snapshot are not in the
    /// tree and are ignored. `layout_changed` is raised when a re-resolved
    /// style differs in a layout input. With `surface_root`, the root keeps its style,
    /// matching [`Self::restyle_subtree_children_cached`].
    #[allow(clippy::too_many_arguments)]
    pub fn restyle_targets_by_path(
        &self,
        root: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index_cache: &mut Option<StyleRuleIndex>,
        snapshot: &crate::FrameSnapshot,
        targets: &std::collections::HashSet<crate::tree::NodeId>,
        surface_root: bool,
        restyled: &mut std::collections::HashSet<crate::tree::NodeId>,
        layout_changed: &mut bool,
    ) -> bool {
        let Some(mut paths) = snapshot.live_paths(root, targets, || {
            crate::frame_work::visit(crate::frame_work::Pass::RestyleScan)
        }) else {
            return false;
        };
        paths.sort_unstable();
        let index = ensure_index(rules, index_cache);
        for path in paths {
            if surface_root && path.is_empty() {
                continue;
            }
            let (last, ancestors) = match path.split_last() {
                Some((&last, ancestors)) => (Some(last), ancestors),
                None => (None, &[][..]),
            };
            let mut node = &mut *root;
            for &child in ancestors {
                crate::frame_work::visit(crate::frame_work::Pass::RestyleScan);
                node.finalized_style_valid = false;
                node = &mut node.children[child];
            }
            let parent_style = last.map(|_| ParentInheritedStyle::from(&node.computed_style));
            if let Some(last) = last {
                node.finalized_style_valid = false;
                node = &mut node.children[last];
            }
            if restyled.contains(&node.id) {
                continue;
            }
            self.restyle_scoped_node(
                node,
                rules,
                index,
                context,
                parent_style.as_ref(),
                false,
                restyled,
                layout_changed,
            );
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn restyle_scoped_node(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        parent_style: Option<&ParentInheritedStyle>,
        inherited_dirty: bool,
        restyled: &mut std::collections::HashSet<crate::tree::NodeId>,
        layout_changed: &mut bool,
    ) {
        crate::frame_work::visit(crate::frame_work::Pass::RestyleScan);
        if node.control_part().is_some() {
            // Styled with its host by `restyle_control_parts`.
            return;
        }
        node.finalized_style_valid = false;
        restyled.insert(node.id);
        let was_pristine = node.finalized_style_pristine;
        let reuse_node = !inherited_dirty
            && self.incremental_matching
            && node.finalized_style.clone().is_some_and(|cached| {
                cached.same_environment(parent_style, self, index, context)
                    && cached.same_matching_rules(node, rules, index, context, self)
            });
        if !reuse_node {
            node.finalized_accessibility_valid = false;
            let previous_inherited = ParentInheritedStyle::from(&node.computed_style);
            let previous_style = (!*layout_changed).then(|| node.computed_style.clone());
            if !preserve_synthetic_surface_style(node, parent_style) {
                let attrs = StyleNodeAttrs::from_node(node);
                node.computed_style = self.resolve_node_style_with_attrs_indexed_inner(
                    rules,
                    index,
                    &attrs,
                    context,
                    None,
                    None,
                    parent_style.map(|parent| &parent.custom_properties),
                );
                if let Some(parent) = parent_style {
                    inherit_retained_text_style(&mut node.computed_style, parent);
                }
            }
            if let Some(previous) = previous_style {
                *layout_changed |= !previous.same_layout_inputs(&node.computed_style);
            }
            let child_parent = ParentInheritedStyle::from(&node.computed_style);
            if previous_inherited != child_parent {
                for child in &mut node.children {
                    self.restyle_scoped_node(
                        child,
                        rules,
                        index,
                        context,
                        Some(&child_parent),
                        true,
                        restyled,
                        layout_changed,
                    );
                }
            }
        }
        // A host whose own rules are unchanged may still have changed part
        // rules (`slider:active::thumb`), so its parts restyle either way.
        self.restyle_control_parts(
            node,
            rules,
            index,
            context,
            Some(restyled),
            Some(layout_changed),
        );
        self.remember_finalized_style(node, index, context, parent_style);
        node.finalized_style_pristine = !reuse_node || was_pristine;
        node.finalized_style_valid = false;
    }

    /// Targeted restyle of `node`'s children with `node` as their inherited
    /// context, as [`Self::restyle_subtree_children_cached`] resolves them;
    /// `node` itself keeps its style.
    pub fn restyle_subtree_children_for_ids_cached(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index: &mut Option<StyleRuleIndex>,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
        mut attribution: Option<&mut StyleRuleAttribution>,
    ) {
        let idx = ensure_index(rules, index);
        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            if let Some(attribution) = attribution.as_deref_mut() {
                self.restyle_subtree_for_ids_with_index_and_inheritance_profiled(
                    child,
                    rules,
                    idx,
                    context,
                    target_ids,
                    Some(&parent),
                    false,
                    attribution,
                );
            } else {
                self.restyle_subtree_for_ids_with_index_and_inheritance(
                    child,
                    rules,
                    idx,
                    context,
                    target_ids,
                    Some(&parent),
                    false,
                );
            }
        }
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
        crate::frame_work::visit(crate::frame_work::Pass::RestyleScan);
        if node.control_part().is_some() {
            // Styled with its host by `restyle_control_parts`.
            return;
        }
        node.finalized_style_valid = false;
        let is_target = target_ids.contains(&node.id);
        // A node should have its style recomputed if it is a direct target or
        // an inherited field changed on its parent. Parent context itself is
        // carried independently so a deep direct target retains inherited
        // values without forcing clean ancestors through style resolution.
        let should_restyle = is_target || inherited_dirty;
        let was_pristine = node.finalized_style_pristine;

        let reuse_node = should_restyle
            && !inherited_dirty
            && self.incremental_matching
            && node.finalized_style.clone().is_some_and(|cached| {
                cached.same_environment(parent_style, self, index, context)
                    && cached.same_matching_rules(node, rules, index, context, self)
            });
        if should_restyle && !reuse_node {
            node.finalized_accessibility_valid = false;
            let previous_inherited = ParentInheritedStyle::from(&node.computed_style);
            if !preserve_synthetic_surface_style(node, parent_style) {
                // Recompute this node's style.
                // For target nodes: apply new pseudo-class rules.
                // For descendants of targets: inherit updated values from the
                // restyled ancestor.
                let attrs = StyleNodeAttrs::from_node(node);
                node.computed_style = self.resolve_node_style_with_attrs_indexed_inner(
                    rules,
                    index,
                    &attrs,
                    context,
                    None,
                    None,
                    parent_style.map(|parent| &parent.custom_properties),
                );
                if let Some(parent) = parent_style {
                    inherit_retained_text_style(&mut node.computed_style, parent);
                }
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
        if should_restyle {
            self.restyle_control_parts(node, rules, index, context, None, None);
            self.remember_finalized_style(node, index, context, parent_style);
            node.finalized_style_pristine = !reuse_node || was_pristine;
            node.finalized_style_valid = false;
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
        crate::frame_work::visit(crate::frame_work::Pass::RestyleScan);
        if node.control_part().is_some() {
            // Styled with its host by `restyle_control_parts`.
            return;
        }
        node.finalized_style_valid = false;
        let should_restyle = target_ids.contains(&node.id) || inherited_dirty;
        let was_pristine = node.finalized_style_pristine;
        let reuse_node = should_restyle
            && !inherited_dirty
            && self.incremental_matching
            && node.finalized_style.clone().is_some_and(|cached| {
                cached.same_environment(parent_style, self, index, context)
                    && cached.same_matching_rules(node, rules, index, context, self)
            });
        if should_restyle && !reuse_node {
            node.finalized_accessibility_valid = false;
            let previous_inherited = ParentInheritedStyle::from(&node.computed_style);
            if !preserve_synthetic_surface_style(node, parent_style) {
                let attrs = StyleNodeAttrs::from_node(node);
                node.computed_style = self.resolve_node_style_with_attrs_indexed_inner(
                    rules,
                    index,
                    &attrs,
                    context,
                    None,
                    Some(attribution),
                    parent_style.map(|parent| &parent.custom_properties),
                );
                if let Some(parent) = parent_style {
                    inherit_retained_text_style(&mut node.computed_style, parent);
                }
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
        if should_restyle {
            self.restyle_control_parts(node, rules, index, context, None, None);
            self.remember_finalized_style(node, index, context, parent_style);
            node.finalized_style_pristine = !reuse_node || was_pristine;
            node.finalized_style_valid = false;
        }
    }

    /// Style `host`'s control-part subtree from the host's matching inputs:
    /// `host::part` rules over each part's initial style, then the geometry
    /// core owns. Restyled parts join `restyled`; `layout_changed` is raised
    /// when one changes a layout input. A host without parts is untouched.
    fn restyle_control_parts(
        &self,
        host: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        mut restyled: Option<&mut std::collections::HashSet<crate::tree::NodeId>>,
        mut layout_changed: Option<&mut bool>,
    ) {
        // The compiler appends parts after any authored children.
        if !host
            .children
            .last()
            .is_some_and(|child| child.control_part().is_some())
        {
            return;
        }
        let host_attrs = ControlHostAttrs::of(host);
        let geometry = crate::control_parts::ControlGeometry::of(host);
        crate::control_parts::apply_host_layout(&mut host.computed_style, geometry);
        let scope = ControlPartScope {
            host_attrs: &host_attrs,
            host_style: &host.computed_style.clone(),
            geometry,
            rules,
            index,
            context,
        };
        let parent = ParentInheritedStyle::from(scope.host_style);
        for part in &mut host.children {
            self.restyle_control_part(part, &scope, &parent, &mut restyled, &mut layout_changed);
        }
    }

    fn restyle_control_part(
        &self,
        node: &mut crate::tree::WidgetNode,
        scope: &ControlPartScope<'_>,
        parent: &ParentInheritedStyle,
        restyled: &mut Option<&mut std::collections::HashSet<crate::tree::NodeId>>,
        layout_changed: &mut Option<&mut bool>,
    ) {
        let Some(part) = node.control_part() else {
            return;
        };
        let initial = crate::control_parts::initial_part_style(part, scope.host_style, scope.geometry);
        let resolve_named = |name: &str, initial: &ComputedStyle| {
            self.resolve_node_style_with_attrs_indexed_inner(
                scope.rules,
                scope.index,
                &scope.host_attrs.for_part(name, initial),
                scope.context,
                None,
                None,
                Some(&parent.custom_properties),
            )
        };
        let mut style = match part.name() {
            Some(name) => resolve_named(name, &initial),
            None => {
                let mut style = ComputedStyle {
                    custom_properties: parent.custom_properties.clone(),
                    ..initial
                };
                if matches!(
                    part,
                    crate::ControlPart::SwitchLead | crate::ControlPart::SwitchTail
                ) {
                    let thumb = crate::control_parts::initial_part_style(
                        crate::ControlPart::SwitchThumb,
                        scope.host_style,
                        scope.geometry,
                    );
                    style.transitions = crate::control_parts::glide_transitions(
                        &resolve_named("thumb", &thumb).transitions,
                    );
                }
                style
            }
        };
        inherit_retained_text_style(&mut style, parent);
        if scope.geometry.vertical {
            crate::control_parts::transpose_for_vertical(&mut style);
        }
        crate::control_parts::apply_part_geometry(part, &mut style, scope.geometry);
        if node.computed_style != style {
            if let Some(layout_changed) = layout_changed.as_deref_mut() {
                *layout_changed |= !node.computed_style.same_layout_inputs(&style);
            }
            if let Some(restyled) = restyled.as_deref_mut() {
                restyled.insert(node.id);
            }
            node.computed_style = style;
        }
        // The renderer draws an indicator's mark from the host's checked state.
        node.state
            .set(crate::tree::NodeState::CHECKED, scope.geometry.checked);
        // Parts have no semantics of their own, so they never hold back the
        // host's finalized accessibility.
        node.finalized_accessibility_valid = true;
        let child_parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_control_part(child, scope, &child_parent, restyled, layout_changed);
        }
    }
}

/// What every part of one control host is styled against.
struct ControlPartScope<'a> {
    host_attrs: &'a ControlHostAttrs,
    host_style: &'a ComputedStyle,
    geometry: crate::control_parts::ControlGeometry,
    rules: &'a [StyleRule],
    index: &'a StyleRuleIndex,
    context: StyleContext,
}

/// Embedded component instances use a synthetic `surface` node to retain
/// instance identity and slot provenance. Its compiler-assigned layout style
/// is structural, not author CSS, so a normal restyle must not replace it.
/// Inherited variables and text values still cross the wrapper boundary for
/// the authored contribution subtree below it.
fn preserve_synthetic_surface_style(
    node: &mut crate::tree::WidgetNode,
    parent_style: Option<&ParentInheritedStyle>,
) -> bool {
    if node.tag != "surface" {
        return false;
    }

    if let Some(parent_style) = parent_style {
        node.computed_style.custom_properties = parent_style.custom_properties.clone();
        inherit_retained_text_style(&mut node.computed_style, parent_style);
    }
    true
}
