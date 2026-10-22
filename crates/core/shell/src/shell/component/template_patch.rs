//! Write-time template updates.
//!
//! When script writes change only public members that are read by recorded
//! element bindings, the bound properties are re-evaluated and written into
//! the retained tree in place. The template is not rebuilt and the retained
//! tree is not diffed; the patched nodes go through the targeted restyle.
//! Any member read by something a binding does not cover (a `{#if}`
//! condition, a structural loop change, component props, or an unkeyed loop
//! body) leaves the frame on the narrow rebuild path.

use super::*;
use mesh_core_scripting::LocaleBoundState;

/// A node to patch, addressed by its child-index path from the root.
type NodePath = Vec<usize>;

#[derive(Default)]
pub(super) struct TemplateBindingIndex {
    readers: HashMap<Arc<str>, HashMap<String, Vec<NodePath>>>,
    list_readers: HashMap<Arc<str>, HashMap<String, Vec<usize>>>,
    lists: Vec<IndexedTemplateList>,
}

struct IndexedTemplateList {
    instance: Arc<str>,
    list: Arc<mesh_core_elements::KeyedTemplateList>,
    items: Vec<Vec<NodePath>>,
    values: Vec<serde_json::Value>,
}

impl TemplateBindingIndex {
    pub(super) fn build(tree: &WidgetNode) -> Self {
        fn collect(
            node: &WidgetNode,
            path: &mut NodePath,
            index: &mut TemplateBindingIndex,
            lists: &mut HashMap<usize, usize>,
        ) {
            mesh_core_elements::frame_work::visit(
                mesh_core_elements::frame_work::Pass::BindingTargetDiscovery,
            );
            if let Some(bindings) = node.template_bindings() {
                for expression in bindings.expressions() {
                    index
                        .readers
                        .entry(Arc::clone(&bindings.instance_key))
                        .or_default()
                        .entry(expression.to_owned())
                        .or_default()
                        .push(path.clone());
                }
                if let Some(item) = &bindings.keyed_item {
                    let identity = Arc::as_ptr(&item.list) as usize;
                    let list_index = *lists.entry(identity).or_insert_with(|| {
                        let list_index = index.lists.len();
                        index
                            .list_readers
                            .entry(Arc::clone(&bindings.instance_key))
                            .or_default()
                            .entry(item.list.source.clone())
                            .or_default()
                            .push(list_index);
                        index.lists.push(IndexedTemplateList {
                            instance: Arc::clone(&bindings.instance_key),
                            list: Arc::clone(&item.list),
                            items: vec![Vec::new(); item.list.keys.len()],
                            values: vec![serde_json::Value::Null; item.list.keys.len()],
                        });
                        list_index
                    });
                    let list = &mut index.lists[list_index];
                    list.items[item.index].push(path.clone());
                    list.values[item.index] = bindings
                        .locals
                        .as_ref()
                        .and_then(|locals| locals.get(&item.list.item_name))
                        .cloned()
                        .unwrap_or_default();
                }
            }
            for (child_index, child) in node.children.iter().enumerate() {
                path.push(child_index);
                collect(child, path, index, lists);
                path.pop();
            }
        }
        let mut index = Self::default();
        collect(tree, &mut Vec::new(), &mut index, &mut HashMap::new());
        index
    }
}

impl FrontendSurfaceComponent {
    /// Start a dispatch that moves the retained tree out of `last_tree`.
    /// Handler writes during it cannot be patched against a missing tree, so
    /// they wait for [`Self::end_held_tree_dispatch`]. Returns whether this
    /// call owns the deferral; a nested dispatch leaves it to the outer one,
    /// whose tree is the real one.
    pub(super) fn begin_held_tree_dispatch(&mut self) -> bool {
        if self.held_tree_template_writes.is_some() {
            return false;
        }
        self.held_tree_template_writes = Some(Vec::new());
        true
    }

    /// Finish the dispatch begun by [`Self::begin_held_tree_dispatch`] once
    /// the retained tree is back: patch the deferred writes, or fall back to
    /// the narrow rebuild they would have taken.
    pub(super) fn end_held_tree_dispatch(&mut self, owner: bool) {
        if !owner {
            return;
        }
        let Some(instance_keys) = self.held_tree_template_writes.take() else {
            return;
        };
        if !instance_keys.is_empty() && !self.try_patch_template_writes(&instance_keys) {
            self.invalidate_script_state_narrow();
        }
    }

    /// Queue `instance_key`'s template writes for the end of the current
    /// held-tree dispatch. Returns `false` when no dispatch holds the tree.
    pub(super) fn defer_template_patch_while_tree_held(&mut self, instance_key: &str) -> bool {
        let Some(pending) = self.held_tree_template_writes.as_mut() else {
            return false;
        };
        if !pending.iter().any(|key| key == instance_key) {
            pending.push(instance_key.to_string());
        }
        true
    }

    /// Apply pending service fields and public-member writes of `instance_keys` by patching
    /// the bindings that read them. Returns `false`, changing nothing, when
    /// any write needs the template rebuild.
    pub(super) fn try_patch_template_writes(&mut self, instance_keys: &[String]) -> bool {
        if (instance_keys.is_empty()
            && self.pending_service_template_nodes.is_none()
            && self.pending_service_binding_readers.is_empty())
            || self.surface_exiting
            || self.surface_entering
            || !self.closing_child_keys.is_empty()
            || !self.entering_child_keys.is_empty()
            || self.template_patch_recovery_pending.get()
        {
            return false;
        }
        #[cfg(test)]
        if self.force_full_template_build {
            return false;
        }
        let Some(tree) = self.last_tree.as_ref() else {
            return false;
        };

        let mut readers = self.pending_service_binding_readers.clone();
        let mut module_ids: HashMap<String, String> = HashMap::new();
        {
            let runtimes = self.runtimes.lock().unwrap();
            for instance_key in instance_keys {
                let Some(runtime) = runtimes.get(instance_key.as_str()) else {
                    return false;
                };
                if !runtime.script_ctx.dirty_state_affects_template() {
                    continue;
                }
                let Some((expressions, _members)) =
                    runtime.script_ctx.template_expressions_reading_changes()
                else {
                    return false;
                };
                if expressions.iter().any(|source| {
                    runtime
                        .unpatchable_template_expressions
                        .contains(source.trim())
                }) {
                    return false;
                }
                readers
                    .entry(instance_key.clone())
                    .or_default()
                    .extend(expressions);
                module_ids.insert(instance_key.clone(), runtime.module_id.clone());
            }
        }
        if readers.is_empty() {
            return false;
        }
        if self.pending_service_template_nodes.is_some()
            && self.pending_service_binding_readers.is_empty()
        {
            return false;
        }
        {
            let runtimes = self.runtimes.lock().unwrap();
            for (instance_key, runtime) in runtimes.iter() {
                if !runtime.script_ctx.state().is_dirty()
                    || instance_keys.iter().any(|key| key == instance_key.as_ref())
                {
                    continue;
                }
                if let Some((expressions, _)) =
                    runtime.script_ctx.template_expressions_reading_changes()
                {
                    readers
                        .entry(instance_key.to_string())
                        .or_default()
                        .extend(expressions);
                }
            }
            for (instance_key, expressions) in &readers {
                let Some(runtime) = runtimes.get(instance_key.as_str()) else {
                    return false;
                };
                if expressions.iter().any(|source| {
                    runtime
                        .unpatchable_template_expressions
                        .contains(source.trim())
                }) {
                    return false;
                }
                module_ids.insert(instance_key.clone(), runtime.module_id.clone());
            }
        }

        let mut targets: Vec<NodePath> = Vec::new();
        let mut list_targets = HashSet::new();
        for (instance, expressions) in &readers {
            for expression in expressions {
                let bindings = self
                    .template_binding_index
                    .readers
                    .get(instance.as_str())
                    .and_then(|readers| readers.get(expression));
                let lists = self
                    .template_binding_index
                    .list_readers
                    .get(instance.as_str())
                    .and_then(|readers| readers.get(expression));
                if bindings.is_none() && lists.is_none() {
                    return false;
                }
                if let Some(bindings) = bindings {
                    targets.extend(bindings.iter().cloned());
                }
                if let Some(lists) = lists {
                    list_targets.extend(lists.iter().copied());
                }
            }
        }
        let mut refreshed_lists = Vec::new();
        let mut refreshed_bindings = HashMap::new();
        for list_index in list_targets {
            let list = &self.template_binding_index.lists[list_index];
            let instance = list.instance.as_ref();
            let Some(state) = self.runtime_state(instance) else {
                return false;
            };
            let Some(module_id) = module_ids.get(instance) else {
                return false;
            };
            let bound = LocaleBoundState::with_localized_misses(
                &state,
                self.locale.module_translator(module_id),
                self.localized_misses_handle(instance),
            );
            let Some(items) = mesh_core_frontend::refresh_keyed_template_list(
                &list.list,
                &bound,
                instance,
                Some(&*self),
                &list.values,
            ) else {
                return false;
            };
            for (index, value) in items.iter().enumerate() {
                if list.values[index] == *value {
                    continue;
                }
                for path in &list.items[index] {
                    let Some(bindings) =
                        node_at_path(tree, path).and_then(WidgetNode::template_bindings)
                    else {
                        return false;
                    };
                    let mut next = (**bindings).clone();
                    let mut locals = next.locals.as_deref().cloned().unwrap_or_default();
                    locals.insert(list.list.item_name.clone(), value.clone());
                    next.locals = Some(Arc::new(locals));
                    refreshed_bindings.insert(path.clone(), Arc::new(next));
                    targets.push(path.clone());
                }
            }
            refreshed_lists.push((list_index, items));
        }
        if readers
            .keys()
            .any(|instance| self.has_template_expression_failure_for(instance))
        {
            return false;
        }
        targets.sort_unstable();
        targets.dedup();

        let mut tree = self.last_tree.take().expect("checked above");
        // Children are shared copy-on-write, so this keeps the last good
        // tree for a failed expression at the cost of the patched paths.
        let original = tree.clone();
        let mut states: HashMap<String, Arc<mesh_core_scripting::ScriptState>> = HashMap::new();
        let mut patched = HashSet::new();
        let mut changes = mesh_core_elements::FrameDirty::default();
        let mut dependencies_changed = false;
        let mut evaluated_instances = HashSet::new();
        for path in &targets {
            let Some(node) = node_at_path_mut(&mut tree, path) else {
                continue;
            };
            if let Some(bindings) = refreshed_bindings.get(path) {
                node.set_template_bindings(Arc::clone(bindings));
            }
            let Some(instance_key) = node
                .template_bindings()
                .map(|bindings| bindings.instance_key.to_string())
            else {
                continue;
            };
            evaluated_instances.insert(instance_key.clone());
            let state = match states.get(&instance_key) {
                Some(state) => Arc::clone(state),
                None => {
                    let Some(state) = self.runtime_state(&instance_key) else {
                        self.last_tree = Some(original);
                        return false;
                    };
                    states.insert(instance_key.clone(), Arc::clone(&state));
                    state
                }
            };
            if !module_ids.contains_key(&instance_key) {
                let runtimes = self.runtimes.lock().unwrap();
                let Some(runtime) = runtimes.get(instance_key.as_str()) else {
                    self.last_tree = Some(original);
                    return false;
                };
                module_ids.insert(instance_key.clone(), runtime.module_id.clone());
            }
            let module_id = &module_ids[&instance_key];
            let bound = LocaleBoundState::with_localized_misses(
                &state,
                self.locale.module_translator(module_id),
                self.localized_misses_handle(&instance_key),
            );
            let patch = mesh_core_frontend::patch_template_bindings(node, &bound, Some(&*self));
            dependencies_changed |= patch.dependencies_changed;
            if patch.changed() {
                patched.insert(node.id);
                changes.mark(node.id, patch.change);
            }
        }
        // A failing expression must not publish a partially evaluated tree;
        // the narrow rebuild owns failure handling and its diagnostics.
        let failed = evaluated_instances
            .iter()
            .any(|instance_key| self.has_template_expression_failure_for(instance_key));
        if failed {
            self.last_tree = Some(original);
            return false;
        }
        if dependencies_changed {
            self.node_service_field_deps = NodeServiceFieldDependencies::build(&tree);
        }
        for (index, values) in refreshed_lists {
            self.template_binding_index.lists[index].values = values;
        }
        self.last_tree = Some(tree);
        self.pending_service_template_nodes = None;
        self.pending_service_binding_readers.clear();

        let mut runtimes = self.runtimes.lock().unwrap();
        for instance_key in readers.keys() {
            if let Some(runtime) = runtimes.get_mut(instance_key.as_str()) {
                runtime.script_ctx.consume_template_changes();
            }
        }
        drop(runtimes);

        if !patched.is_empty() {
            for (id, change) in changes.iter() {
                self.frame_dirty.mark(id, change);
                self.patched_binding_layout |=
                    change.contains(mesh_core_elements::ChangeClass::LAYOUT);
            }
            self.patched_binding_nodes.extend(patched);
            // Like the narrow script path, leave the pixels valid: the retained
            // display list diffs the patched nodes' fingerprints and damages
            // only what they paint.
            self.animation_only_dirty = false;
            self.next_frame_only_dirty = false;
            self.dirty_types |= ComponentDirtyFlags::INTERACTION_RESTYLE;
            self.style_only_dirty = true;
        }
        true
    }
}

fn node_at_path<'tree>(root: &'tree WidgetNode, path: &[usize]) -> Option<&'tree WidgetNode> {
    let mut node = root;
    for &index in path {
        node = node.children.get(index)?;
    }
    Some(node)
}

fn node_at_path_mut<'tree>(
    root: &'tree mut WidgetNode,
    path: &[usize],
) -> Option<&'tree mut WidgetNode> {
    let mut node = root;
    for &index in path {
        node = node.children.get_mut(index)?;
    }
    Some(node)
}
