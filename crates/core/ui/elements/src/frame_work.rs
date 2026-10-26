//! Opt-in node-visit accounting for synchronous frame preparation.
use std::cell::Cell;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::rc::Rc;

macro_rules! passes {
    ($($name:ident => $label:literal),+ $(,)?) => {
        #[derive(Clone, Copy)]
        pub enum Pass { $($name),+ }
        const PASSES: &[(Pass, &str)] = &[$((Pass::$name, $label)),+];
    };
}
passes! {
    BindingTargetDiscovery => "binding_target_discovery",
    BindingPatch => "binding_patch",
    KeyedItemCompare => "keyed_item_compare",
    KeyedKeyValidation => "keyed_key_validation",
    StyleResolve => "style_resolve",
    RestyleScan => "restyle_scan",
    LayoutBuild => "layout_build",
    LayoutSync => "layout_sync",
    LayoutProducer => "layout_producer",
    LayoutProducerWrite => "layout_producer_write",
    BoundsIndex => "bounds_index",
    LayoutWrite => "layout_write",
    LayoutFit => "layout_fit",
    LayoutExtents => "layout_extents",
    LayoutDiscovery => "layout_discovery",
    AnimationSample => "animation_sample",
    AnimationLookup => "animation_lookup",
    AnimationPreviousStyle => "animation_previous_style",
    AccessibilityNormalize => "accessibility_normalize",
    AccessibilityProject => "accessibility_project",
    Validation => "validation",
    Annotation => "annotation",
    RetainedDiscovery => "retained_discovery",
    RetainedSync => "retained_sync",
    RetainedLookup => "retained_lookup",
    DisplayEntries => "display_entries",
    DisplayAncestors => "display_ancestors",
    DisplayBuild => "display_build",
    ChildSurfaceRequests => "child_surface_requests",
    FocusScopeRefresh => "focus_scope_refresh",
    DisclosureRefresh => "disclosure_refresh",
    InteractionReconcile => "interaction_reconcile",
    InteractionLookup => "interaction_lookup",
}

#[derive(Clone, Copy)]
struct State {
    enabled: bool,
    counts: [u64; PASSES.len()],
}
impl State {
    const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            counts: [0; PASSES.len()],
        }
    }
}
struct Counters {
    enabled: Cell<bool>,
    counts: [Cell<u64>; PASSES.len()],
}
impl Counters {
    fn replace(&self, state: State) -> State {
        State {
            enabled: self.enabled.replace(state.enabled),
            counts: std::array::from_fn(|index| self.counts[index].replace(state.counts[index])),
        }
    }
}
thread_local! {
    static COUNTERS: Counters = const { Counters {
        enabled: Cell::new(false), counts: [const { Cell::new(0) }; PASSES.len()],
    } };
}

/// Count a node actually examined, including clean nodes and repeated visits.
/// Does nothing outside an enabled scope; no allocation occurs on the hot path.
#[inline]
pub fn visit(pass: Pass) {
    COUNTERS.with(|state| {
        if state.enabled.get() {
            let count = &state.counts[pass as usize];
            count.set(count.get().saturating_add(1));
        }
    });
}

/// A thread-local measurement scope. Nested scopes restore their caller's
/// counters on drop, including during unwinding; worker threads are excluded.
pub struct Scope {
    previous: State,
    _thread: PhantomData<Rc<()>>,
}
impl Scope {
    pub fn new(enabled: bool) -> Self {
        Self {
            previous: COUNTERS.with(|state| state.replace(State::new(enabled))),
            _thread: PhantomData,
        }
    }

    /// Includes zero counts so skipped passes are distinct from absent data.
    pub fn snapshot(&self) -> BTreeMap<String, u64> {
        COUNTERS.with(|state| {
            if !state.enabled.get() {
                return BTreeMap::new();
            }
            PASSES
                .iter()
                .map(|&(pass, label)| (label.to_owned(), state.counts[pass as usize].get()))
                .collect()
        })
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        COUNTERS.with(|state| {
            state.replace(self.previous);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_and_disabled_scopes_restore_counts() {
        let outer = Scope::new(true);
        visit(Pass::Validation);
        {
            let disabled = Scope::new(false);
            visit(Pass::Validation);
            assert!(disabled.snapshot().is_empty());
        }
        assert_eq!(outer.snapshot()["validation"], 1);
        {
            let nested = Scope::new(true);
            visit(Pass::Validation);
            visit(Pass::Validation);
            assert_eq!(nested.snapshot()["validation"], 2);
        }
        assert_eq!(outer.snapshot()["validation"], 1);
        assert_eq!(outer.snapshot()["layout_write"], 0);
    }
    #[test]
    fn unwind_and_worker_threads_do_not_leak_counts() {
        let scope = Scope::new(true);
        visit(Pass::Validation);
        std::thread::spawn(|| visit(Pass::Validation))
            .join()
            .unwrap();
        let _ = std::panic::catch_unwind(|| {
            let _nested = Scope::new(true);
            visit(Pass::Validation);
            panic!("abort nested frame");
        });
        assert_eq!(scope.snapshot()["validation"], 1);
    }

    #[test]
    fn validation_counts_actual_nodes_and_resets_between_frames() {
        let mut root = crate::WidgetNode::new("box");
        root.children.push(crate::WidgetNode::new("box"));
        root.children.push(crate::WidgetNode::new("box"));
        for _ in 0..2 {
            let scope = Scope::new(true);
            crate::validate_widget_tree(&root).unwrap();
            assert_eq!(scope.snapshot()["validation"], 3);
        }
    }
}
