//! Write-time template updates.
//!
//! When script writes change only public members that are read by recorded
//! element bindings, the bound properties are re-evaluated and written into
//! the retained tree in place. The template is not rebuilt and the retained
//! tree is not diffed; the patched nodes go through the targeted restyle.
//! Any member read by something a binding does not cover (a `{#if}`
//! condition, a `{#for}` source or key, component props, or an unkeyed loop
//! body) leaves the frame on the narrow rebuild path.

use super::*;
use mesh_core_scripting::LocaleBoundState;

/// A node to patch, addressed by its child-index path from the root.
type NodePath = Vec<usize>;

impl FrontendSurfaceComponent {
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

        // Every expression that reads a changed member must be a recorded
        // binding somewhere in the tree; collect the nodes those bind.
        let mut covered: HashMap<&str, HashSet<&str>> = HashMap::new();
        let mut targets: Vec<NodePath> = Vec::new();
        collect_binding_targets(tree, &readers, &mut Vec::new(), &mut covered, &mut targets);
        targets.sort_unstable();
        targets.dedup();
        for (instance_key, expressions) in &readers {
            let bound = covered.get(instance_key.as_str());
            if expressions
                .iter()
                .any(|expression| !bound.is_some_and(|bound| bound.contains(expression.as_str())))
            {
                return false;
            }
        }

        let mut tree = self.last_tree.take().expect("checked above");
        // Children are shared copy-on-write, so this keeps the last good
        // tree for a failed expression at the cost of the patched paths.
        let original = tree.clone();
        let mut states: HashMap<String, Arc<mesh_core_scripting::ScriptState>> = HashMap::new();
        let mut patched = HashSet::new();
        let mut dependencies_changed = false;
        let mut evaluated_instances = HashSet::new();
        for path in &targets {
            let Some(node) = node_at_path_mut(&mut tree, path) else {
                continue;
            };
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
            if patch.changed {
                patched.insert(node.id);
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

fn collect_binding_targets<'tree>(
    node: &'tree WidgetNode,
    readers: &HashMap<String, HashSet<String>>,
    path: &mut NodePath,
    covered: &mut HashMap<&'tree str, HashSet<&'tree str>>,
    targets: &mut Vec<NodePath>,
) {
    if let Some(bindings) = node.template_bindings()
        && let Some(changed) = readers.get(bindings.instance_key.as_ref())
    {
        let entry = covered.entry(bindings.instance_key.as_ref()).or_default();
        let mut reads_change = false;
        for expression in bindings.expressions() {
            entry.insert(expression);
            reads_change |= changed.contains(expression);
        }
        if reads_change {
            targets.push(path.clone());
        }
    }
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        collect_binding_targets(child, readers, path, covered, targets);
        path.pop();
    }
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
