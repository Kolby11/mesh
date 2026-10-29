//! Component-level render memoization.
//!
//! `render_import` re-evaluates every embedded/local component's template on
//! every surface rebuild, even when nothing that instance depends on changed.
//! This module caches each instance's built subtree and reuses it wholesale
//! when the cached inputs still hold:
//!
//! - the resolved props (fingerprint over props + typed handler calls),
//! - the aggregate runtime-generation stamp for the instance and every
//!   descendant (a nested child whose state changed bumps each enclosing
//!   cached subtree),
//! - the active theme (`Arc` pointer identity — `refresh_active_theme` swaps
//!   the `Arc` whenever the theme actually changes),
//! - the active locale,
//! - the container size the subtree was built against.
//!
//! Build side effects are replayed or vetoed via the mark counters on
//! `FrontendSurfaceComponent`: promoted-popover wrappers and error
//! placeholders inside a cached subtree re-set their presence flags on reuse;
//! surface-portal state writes (`pending_surface_states`) veto caching because
//! they must re-run every build.
//!
//! Cached nodes hold finalized styles and semantics before animation sampling.
//! Authored inputs remain position-independent. Runtime identity and interaction
//! annotations are refreshed by `finalize_tree`; event handlers are already
//! namespaced by instance key during attribute
//! construction. The cache is cleared by `reset_render_caches` (theme change,
//! locale change, source reload — the same sites that clear or invalidate the
//! runtimes map).
//!
//! Known contract limits (same reactivity contract as the rest of the shell):
//! only public script members are reactive. A template expression reading a
//! private `local` mutated by a handler was never guaranteed to re-render —
//! without memoization it merely re-evaluated opportunistically on unrelated
//! rebuilds. Repeated source occurrences and loop-rendered instances have
//! distinct runtime/cache identities. A `{#for}` can supply `key={expression}`
//! to retain that identity across reorders; unkeyed loops remain positional.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use mesh_core_elements::{ComponentCompositionProps, EventHandlerCall, WidgetNode};

use super::FrontendSurfaceComponent;

pub(super) struct ComponentMemoEntry {
    props_fingerprint: u64,
    container_bits: (u32, u32),
    theme_ptr: usize,
    locale: String,
    /// Aggregate generation for the instance and all currently registered
    /// descendant runtimes. The runtime-generation index bumps this stamp on
    /// the affected ancestor path when a runtime changes or is added.
    subtree_generation: u64,
    /// The cached subtree contains promoted `<popover>` wrappers; reuse must
    /// re-set `has_promoted_popover_wrappers` so `finalize_tree` collapses them.
    marks_popover: bool,
    /// The cached subtree contains error placeholders; reuse must re-set
    /// `has_error_placeholders` so `finalize_tree` constrains them.
    marks_error: bool,
    node: WidgetNode,
}

pub(super) use mesh_core_frontend_runtime::RuntimeGenerationIndex;

/// Snapshot of the side-effect mark counters taken before a child build.
#[derive(Clone, Copy)]
pub(super) struct MemoEffectMarks {
    popover: u64,
    error: u64,
    portal: u64,
}

pub(super) fn component_props_fingerprint(
    props: &ComponentCompositionProps,
    prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    props.values.len().hash(&mut hasher);
    for (key, value) in &props.values {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    props.bindings.len().hash(&mut hasher);
    for (key, value) in &props.bindings {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    props.callbacks.hash(&mut hasher);
    props.bind_this.hash(&mut hasher);
    prop_handler_calls.len().hash(&mut hasher);
    for (name, call) in prop_handler_calls {
        name.hash(&mut hasher);
        call.handler.hash(&mut hasher);
        call.args.len().hash(&mut hasher);
        for arg in &call.args {
            hash_json_value(arg, &mut hasher);
        }
    }
    hasher.finish()
}

use mesh_core_frontend_runtime::hash_json_value;
pub(super) use mesh_core_frontend_runtime::slot_props_fingerprint;

#[cfg(test)]
fn descendant_instance_prefix(instance_key: &str) -> String {
    let mut prefix = String::with_capacity(instance_key.len() + 1);
    prefix.push_str(instance_key);
    prefix.push('/');
    prefix
}

impl FrontendSurfaceComponent {
    pub(super) fn register_runtime_generation(&self, instance_key: &str, generation: u64) {
        self.runtime_generations
            .borrow_mut()
            .register(instance_key, generation);
    }

    pub(super) fn sync_runtime_generation(&self, instance_key: &str, generation: u64) {
        self.runtime_generations
            .borrow_mut()
            .sync(instance_key, generation);
    }

    pub(super) fn rebuild_runtime_generation_index(&self) {
        let runtimes = self.runtimes.lock().unwrap();
        let snapshot = runtimes
            .iter()
            .map(|(key, runtime)| {
                (
                    Arc::clone(key),
                    runtime.script_ctx.state().mutation_generation(),
                )
            })
            .collect::<Vec<_>>();
        drop(runtimes);
        self.runtime_generations.borrow_mut().rebuild(&snapshot);
    }

    fn ensure_runtime_generation_index(&self) {
        let runtime_count = self.runtimes.lock().unwrap().len();
        if self.runtime_generations.borrow().runtime_count() != runtime_count {
            self.rebuild_runtime_generation_index();
        }
    }

    fn runtime_subtree_generation(&self, instance_key: &str) -> Option<u64> {
        self.ensure_runtime_generation_index();
        self.runtime_generations
            .borrow()
            .subtree_generation(instance_key)
    }

    /// Returns a copy-on-write clone of the memoized subtree for `instance_key`
    /// when every cached input still holds, replaying the presence-flag side
    /// effects the cached subtree carries. Authored payload and descendant
    /// topology remain shared until runtime finalization mutates a specific
    /// node or child list. Returns `None` on any mismatch.
    pub(super) fn lookup_component_memo(
        &self,
        instance_key: &str,
        props_fingerprint: u64,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode> {
        let memo = self.component_memo.borrow();
        let entry = memo.get(instance_key)?;
        if entry.props_fingerprint != props_fingerprint
            || entry.container_bits != (container_width.to_bits(), container_height.to_bits())
            || entry.theme_ptr != std::sync::Arc::as_ptr(&self.active_theme.borrow()) as usize
            || entry.locale != self.locale.current()
        {
            return None;
        }
        if entry.subtree_generation != self.runtime_subtree_generation(instance_key)? {
            return None;
        }
        if entry.marks_popover {
            self.has_promoted_popover_wrappers.set(true);
            self.popover_wrapper_marks
                .set(self.popover_wrapper_marks.get().wrapping_add(1));
        }
        if entry.marks_error {
            self.has_error_placeholders.set(true);
            self.error_placeholder_marks
                .set(self.error_placeholder_marks.get().wrapping_add(1));
        }
        self.component_memo_hits
            .set(self.component_memo_hits.get().wrapping_add(1));
        Some(entry.node.clone())
    }

    pub(super) fn memo_effect_marks(&self) -> MemoEffectMarks {
        MemoEffectMarks {
            popover: self.popover_wrapper_marks.get(),
            error: self.error_placeholder_marks.get(),
            portal: self.portal_state_writes.get(),
        }
    }

    /// Stores the built subtree for `instance_key` unless the build performed
    /// side effects that cannot be replayed from cache (surface-portal state
    /// writes). Popover-wrapper and error-placeholder marks are recorded on
    /// the entry so reuse re-sets the corresponding presence flags.
    pub(super) fn store_component_memo(
        &self,
        instance_key: &str,
        props_fingerprint: u64,
        container_width: f32,
        container_height: f32,
        marks_before: MemoEffectMarks,
        node: &mut WidgetNode,
    ) {
        if self.portal_state_writes.get() != marks_before.portal {
            // A nested surface portal published visibility state during this
            // build; that write must re-run on every build, so the subtree is
            // not cacheable.
            self.component_memo.borrow_mut().remove(instance_key);
            return;
        }
        let Some(subtree_generation) = self.runtime_subtree_generation(instance_key) else {
            return;
        };
        node.set_memo_instance(self.instance_keys.borrow_mut().intern(instance_key));
        self.component_memo.borrow_mut().insert(
            self.instance_keys.borrow_mut().intern(instance_key),
            ComponentMemoEntry {
                props_fingerprint,
                container_bits: (container_width.to_bits(), container_height.to_bits()),
                theme_ptr: std::sync::Arc::as_ptr(&self.active_theme.borrow()) as usize,
                locale: self.locale.current().to_string(),
                subtree_generation,
                marks_popover: self.popover_wrapper_marks.get() != marks_before.popover,
                marks_error: self.error_placeholder_marks.get() != marks_before.error,
                node: node.clone(),
            },
        );
    }

    /// Publish pre-animation projections only. Layout and runtime identities
    /// are refreshed on reuse; styles and semantics carry their own validity.
    pub(super) fn finalize_component_memo(&self, tree: &WidgetNode) {
        fn visit(node: &WidgetNode, memo: &mut HashMap<Arc<str>, ComponentMemoEntry>) {
            if let Some(key) = node.memo_instance()
                && let Some(entry) = memo.get_mut(key)
            {
                entry.node.retain_finalization_from(node);
            }
            for child in &node.children {
                visit(child, memo);
            }
        }
        visit(tree, &mut self.component_memo.borrow_mut());
    }

    pub(super) fn clear_component_memo(&self) {
        self.component_memo.borrow_mut().clear();
    }

    pub(super) fn clear_runtime_generation_index(&self) {
        self.runtime_generations.borrow_mut().clear();
    }

    #[cfg(test)]
    pub(super) fn component_memo_hit_count(&self) -> u64 {
        self.component_memo_hits.get()
    }

    #[cfg(test)]
    pub(super) fn component_memo_shares_authored_payload(
        &self,
        instance_key: &str,
        node: &WidgetNode,
    ) -> bool {
        self.component_memo
            .borrow()
            .get(instance_key)
            .is_some_and(|entry| entry.node.contains_shared_authored_payload_with(node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendant_instance_prefix_matches_legacy_format() {
        assert_eq!(
            descendant_instance_prefix("@mesh/panel/local:Toolbar"),
            "@mesh/panel/local:Toolbar/"
        );
    }

    // cargo test -p mesh-core-shell --release -- runtime_generation_index_beats_descendant_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only runtime-generation index benchmark"]
    fn runtime_generation_index_beats_descendant_scan() {
        let mut index = RuntimeGenerationIndex::default();
        let keys = (0..4096)
            .map(|index| Arc::<str>::from(format!("root/local:Child{index}")))
            .collect::<Vec<_>>();
        index.register("root", 1);
        for key in &keys {
            index.register(key, 1);
        }

        let iterations = 100_000;
        let scan_started = std::time::Instant::now();
        let mut scan_total = 0u64;
        for _ in 0..iterations {
            for key in &keys {
                scan_total = scan_total.wrapping_add(key.len() as u64);
            }
        }
        let scan_time = scan_started.elapsed();

        let index_started = std::time::Instant::now();
        let mut index_total = 0u64;
        for _ in 0..iterations {
            index_total = index_total
                .wrapping_add(index.subtree_generation("root").expect("root generation"));
        }
        let index_time = index_started.elapsed();

        eprintln!(
            "runtime-generation index: descendant scan {scan_time:?}; aggregate lookup {index_time:?}; ratio {:.2}x",
            scan_time.as_secs_f64() / index_time.as_secs_f64()
        );
        assert_ne!(scan_total, 0);
        assert_ne!(index_total, 0);
        assert!(index_time < scan_time);
    }
}
