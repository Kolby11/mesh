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
//! Cached nodes are position-independent: `_mesh_key` identity and
//! interaction annotations are applied later by `finalize_tree`, and event
//! handlers are already namespaced by instance key during attribute
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

use std::collections::{BTreeMap, HashMap, HashSet};
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

struct RuntimeGenerationEntry {
    parent: Option<Arc<str>>,
    children: HashSet<Arc<str>>,
    generation: u64,
    subtree_generation: u64,
    runtime: bool,
}

/// Parent/child index for runtime mutation generations.
///
/// Memo validity only needs to know whether anything in an instance's
/// subtree changed. Keeping that aggregate in the index avoids scanning every
/// runtime when a memo entry is stored or probed. Parent placeholders are
/// retained while a child is registered so registration order does not matter;
/// only entries marked as real runtimes are returned to memo lookups.
#[derive(Default)]
pub(super) struct RuntimeGenerationIndex {
    entries: HashMap<Arc<str>, RuntimeGenerationEntry>,
    next_subtree_generation: u64,
}

impl RuntimeGenerationIndex {
    fn ensure_entry(&mut self, key: Arc<str>) {
        if self.entries.contains_key(&key) {
            return;
        }
        let parent = parent_instance_key(&key);
        if let Some(parent_key) = parent.as_ref() {
            self.ensure_entry(Arc::clone(parent_key));
            self.entries
                .get_mut(parent_key)
                .expect("parent runtime-generation entry")
                .children
                .insert(Arc::clone(&key));
        }
        self.entries.insert(
            key,
            RuntimeGenerationEntry {
                parent,
                children: HashSet::new(),
                generation: 0,
                subtree_generation: 0,
                runtime: false,
            },
        );
    }

    fn bump_path(&mut self, key: &Arc<str>) {
        self.next_subtree_generation = self.next_subtree_generation.wrapping_add(1);
        if self.next_subtree_generation == 0 {
            self.next_subtree_generation = 1;
        }
        let stamp = self.next_subtree_generation;
        let mut current = Some(Arc::clone(key));
        while let Some(current_key) = current {
            let Some(entry) = self.entries.get_mut(&current_key) else {
                break;
            };
            entry.subtree_generation = stamp;
            current = entry.parent.clone();
        }
    }

    pub(super) fn register(&mut self, key: &str, generation: u64) {
        let key: Arc<str> = Arc::from(key);
        self.ensure_entry(Arc::clone(&key));
        let changed = {
            let entry = self
                .entries
                .get_mut(&key)
                .expect("registered runtime-generation entry");
            let changed = !entry.runtime || entry.generation != generation;
            entry.runtime = true;
            entry.generation = generation;
            changed
        };
        if changed {
            self.bump_path(&key);
        }
    }

    pub(super) fn sync(&mut self, key: &str, generation: u64) {
        let key: Arc<str> = Arc::from(key);
        self.ensure_entry(Arc::clone(&key));
        let changed = {
            let entry = self
                .entries
                .get_mut(&key)
                .expect("synced runtime-generation entry");
            let changed = !entry.runtime || entry.generation != generation;
            entry.runtime = true;
            entry.generation = generation;
            changed
        };
        if changed {
            self.bump_path(&key);
        }
    }

    pub(super) fn subtree_generation(&self, key: &str) -> Option<u64> {
        self.entries
            .get(key)
            .filter(|entry| entry.runtime)
            .map(|entry| entry.subtree_generation)
    }

    fn runtime_count(&self) -> usize {
        self.entries.values().filter(|entry| entry.runtime).count()
    }

    pub(super) fn rebuild(&mut self, runtimes: &[(Arc<str>, u64)]) {
        self.entries.clear();
        self.next_subtree_generation = 0;
        for (key, generation) in runtimes {
            self.register(key, *generation);
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.next_subtree_generation = 0;
    }

    #[cfg(test)]
    fn aggregate_for(&self, key: &str) -> Option<u64> {
        self.entries.get(key).map(|entry| entry.subtree_generation)
    }
}

fn parent_instance_key(key: &str) -> Option<Arc<str>> {
    key.rsplit_once('/').map(|(parent, _)| Arc::from(parent))
}

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

/// Stable fingerprint for manifest-provided slot props. Slot contributions use
/// JSON values rather than template attributes, but otherwise have the same
/// cache contract as ordinary embedded components.
pub(super) fn slot_props_fingerprint(props: &serde_json::Map<String, serde_json::Value>) -> u64 {
    // `serde_json::Map` is the ordered map representation in this workspace;
    // contribution props originate in a manifest and therefore already have a
    // stable key order. Avoid allocating a sortable scratch vector on every
    // memo probe.
    let mut hasher = std::hash::DefaultHasher::new();
    props.len().hash(&mut hasher);
    for (key, value) in props {
        key.hash(&mut hasher);
        hash_json_value(value, &mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
fn descendant_instance_prefix(instance_key: &str) -> String {
    let mut prefix = String::with_capacity(instance_key.len() + 1);
    prefix.push_str(instance_key);
    prefix.push('/');
    prefix
}

fn hash_json_value(value: &serde_json::Value, hasher: &mut impl Hasher) {
    match value {
        serde_json::Value::Null => 0u8.hash(hasher),
        serde_json::Value::Bool(flag) => {
            1u8.hash(hasher);
            flag.hash(hasher);
        }
        serde_json::Value::Number(number) => {
            2u8.hash(hasher);
            if let Some(int) = number.as_i64() {
                int.hash(hasher);
            } else if let Some(uint) = number.as_u64() {
                uint.hash(hasher);
            } else {
                number.as_f64().unwrap_or(0.0).to_bits().hash(hasher);
            }
        }
        serde_json::Value::String(text) => {
            3u8.hash(hasher);
            text.hash(hasher);
        }
        serde_json::Value::Array(items) => {
            4u8.hash(hasher);
            items.len().hash(hasher);
            for item in items {
                hash_json_value(item, hasher);
            }
        }
        serde_json::Value::Object(entries) => {
            5u8.hash(hasher);
            entries.len().hash(hasher);
            for (key, entry) in entries {
                key.hash(hasher);
                hash_json_value(entry, hasher);
            }
        }
    }
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
        node: &WidgetNode,
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

    #[test]
    fn runtime_generation_index_aggregates_descendants_without_scanning() {
        let mut index = RuntimeGenerationIndex::default();
        index.register("root", 1);
        let root_generation = index.aggregate_for("root").unwrap();
        index.register("root/local:Child", 1);
        let child_generation = index.aggregate_for("root/local:Child").unwrap();
        let root_after_child = index.aggregate_for("root").unwrap();

        assert_ne!(root_after_child, root_generation);
        assert_ne!(child_generation, 0);

        index.sync("root/local:Child", 2);
        assert_ne!(index.aggregate_for("root").unwrap(), root_after_child);
        assert_eq!(
            index.subtree_generation("root/local:Child"),
            index.aggregate_for("root/local:Child")
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

    // cargo test -p mesh-core-shell --release -- descendant_instance_prefix_presizing_beats_format_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only memo descendant-prefix microbenchmark"]
    fn descendant_instance_prefix_presizing_beats_format_benchmark() {
        let instance_key = "@mesh/panel/local:StatusCluster/import:NetworkControls";
        let iterations = 1_000_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total ^= std::hint::black_box(format!("{instance_key}/").len());
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_total ^= std::hint::black_box(descendant_instance_prefix(instance_key).len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "memo descendant prefix: format {old_time:?}; presized {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    fn sorted_scratch_fingerprint(props: &serde_json::Map<String, serde_json::Value>) -> u64 {
        let mut entries: Vec<_> = props.iter().collect();
        entries.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        let mut hasher = std::hash::DefaultHasher::new();
        entries.len().hash(&mut hasher);
        for (key, value) in entries {
            key.hash(&mut hasher);
            hash_json_value(value, &mut hasher);
        }
        hasher.finish()
    }

    // cargo test -p mesh-core-shell --release -- slot_props_fingerprint_avoids_sort_scratch_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only slot prop fingerprint microbenchmark"]
    fn slot_props_fingerprint_avoids_sort_scratch_benchmark() {
        let props = (0..24)
            .map(|index| {
                (
                    format!("prop_{index:02}"),
                    serde_json::json!({ "enabled": index % 2 == 0, "value": index }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let iterations = 1_000_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0u64;
        for _ in 0..iterations {
            old_total ^= std::hint::black_box(sorted_scratch_fingerprint(&props));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0u64;
        for _ in 0..iterations {
            new_total ^= std::hint::black_box(slot_props_fingerprint(&props));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "slot prop fingerprint: sort scratch {old_time:?}; ordered map {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- slot_catalog_precompute_avoids_render_time_hash_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only slot catalog fingerprint microbenchmark"]
    fn slot_catalog_precompute_avoids_render_time_hash_benchmark() {
        let props = (0..24)
            .map(|index| {
                (
                    format!("prop_{index:02}"),
                    serde_json::json!({ "enabled": index % 2 == 0, "value": index }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let precomputed = slot_props_fingerprint(&props);
        let iterations = 1_000_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0u64;
        for _ in 0..iterations {
            old_total ^= std::hint::black_box(slot_props_fingerprint(&props));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0u64;
        for _ in 0..iterations {
            new_total ^= std::hint::black_box(precomputed);
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "slot catalog fingerprint: render-time hash {old_time:?}; precomputed read {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }
}
