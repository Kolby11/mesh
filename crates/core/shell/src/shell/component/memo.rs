//! Component-level render memoization.
//!
//! `render_import` re-evaluates every embedded/local component's template on
//! every surface rebuild, even when nothing that instance depends on changed.
//! This module caches each instance's built subtree and reuses it wholesale
//! when the cached inputs still hold:
//!
//! - the resolved props (fingerprint over props + typed handler calls),
//! - the instance's own `ScriptState::mutation_generation` **and** every
//!   descendant instance's generation (descendants are identified by the
//!   hierarchical instance-key prefix, so a nested child whose state changed
//!   invalidates every enclosing cached subtree),
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
//! rebuilds. Repeated instances of the same alias share one runtime and one
//! cache slot; alternating props simply miss the cache (never reused wrongly).

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use mesh_core_elements::{EventHandlerCall, WidgetNode};

use super::FrontendSurfaceComponent;

pub(super) struct ComponentMemoEntry {
    props_fingerprint: u64,
    container_bits: (u32, u32),
    theme_ptr: usize,
    locale: String,
    /// `(instance_key, mutation_generation)` for the instance itself plus
    /// every descendant runtime that existed when the entry was stored.
    generations: Vec<(String, u64)>,
    /// The cached subtree contains promoted `<popover>` wrappers; reuse must
    /// re-set `has_promoted_popover_wrappers` so `finalize_tree` collapses them.
    marks_popover: bool,
    /// The cached subtree contains error placeholders; reuse must re-set
    /// `has_error_placeholders` so `finalize_tree` constrains them.
    marks_error: bool,
    node: WidgetNode,
}

/// Snapshot of the side-effect mark counters taken before a child build.
#[derive(Clone, Copy)]
pub(super) struct MemoEffectMarks {
    popover: u64,
    error: u64,
    portal: u64,
}

pub(super) fn component_props_fingerprint(
    props: &BTreeMap<String, String>,
    prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    props.len().hash(&mut hasher);
    for (key, value) in props {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
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
    /// Returns a clone of the memoized subtree for `instance_key` when every
    /// cached input still holds, replaying the presence-flag side effects the
    /// cached subtree carries. Returns `None` on any mismatch.
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
        {
            let runtimes = self.runtimes.lock().unwrap();
            for (key, generation) in &entry.generations {
                let runtime = runtimes.get(key)?;
                if runtime.script_ctx.state().mutation_generation() != *generation {
                    return None;
                }
            }
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
        let descendant_prefix = format!("{instance_key}/");
        let generations = {
            let runtimes = self.runtimes.lock().unwrap();
            let mut generations = Vec::new();
            for (key, runtime) in runtimes.iter() {
                if key == instance_key || key.starts_with(&descendant_prefix) {
                    generations.push((
                        key.clone(),
                        runtime.script_ctx.state().mutation_generation(),
                    ));
                }
            }
            generations
        };
        self.component_memo.borrow_mut().insert(
            instance_key.to_string(),
            ComponentMemoEntry {
                props_fingerprint,
                container_bits: (container_width.to_bits(), container_height.to_bits()),
                theme_ptr: std::sync::Arc::as_ptr(&self.active_theme.borrow()) as usize,
                locale: self.locale.current().to_string(),
                generations,
                marks_popover: self.popover_wrapper_marks.get() != marks_before.popover,
                marks_error: self.error_placeholder_marks.get() != marks_before.error,
                node: node.clone(),
            },
        );
    }

    pub(super) fn clear_component_memo(&self) {
        self.component_memo.borrow_mut().clear();
    }

    #[cfg(test)]
    pub(super) fn component_memo_hit_count(&self) -> u64 {
        self.component_memo_hits.get()
    }
}
