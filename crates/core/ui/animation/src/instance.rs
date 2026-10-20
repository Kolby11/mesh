//! Stable identity and lifecycle decisions for in-flight animations.

use std::time::Instant;

use mesh_core_elements::NodeId;

/// When a running animation's sampled value can next differ from the value it
/// shows now. Callers may skip frames until then; waking early is harmless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextChange {
    /// The value moves continuously and needs every frame.
    EveryFrame,
    /// The value is held until this instant (a delay or a `steps()` plateau).
    At(Instant),
    /// The value will not change again (finished or paused).
    Never,
}

impl NextChange {
    /// The sooner of two changes.
    pub fn earliest(self, other: Self) -> Self {
        match (self, other) {
            (Self::EveryFrame, _) | (_, Self::EveryFrame) => Self::EveryFrame,
            (Self::At(a), Self::At(b)) => Self::At(a.min(b)),
            (Self::At(at), Self::Never) | (Self::Never, Self::At(at)) => Self::At(at),
            (Self::Never, Self::Never) => Self::Never,
        }
    }
}

/// Identity of one animation declaration on one retained node.
///
/// The list position distinguishes duplicate animation names. The declaration
/// generation changes when the authored timing or keyframe definition changes,
/// which makes that update an explicit replacement instead of stale state
/// being reused under the old name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimationInstanceId {
    pub node_id: NodeId,
    pub list_index: u32,
    pub declaration_generation: u64,
}

impl AnimationInstanceId {
    pub const fn new(node_id: NodeId, list_index: u32, declaration_generation: u64) -> Self {
        Self {
            node_id,
            list_index,
            declaration_generation,
        }
    }

    /// Name used to isolate one instance in a keyframe registry.
    pub fn registry_key(self, animation_name: &str) -> String {
        format!(
            "node:{}::animation:{}::slot:{}::generation:{:016x}",
            self.node_id, animation_name, self.list_index, self.declaration_generation
        )
    }
}

/// The explicit result of reconciling an animation declaration with its prior
/// frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationLifecycle {
    /// There is no active animation for this declaration.
    Idle,
    /// A declaration appeared without a prior instance in its slot.
    Started,
    /// The same stable instance continued on its existing timeline.
    Continued,
    /// A declaration in the same slot superseded a different instance.
    Replaced,
    /// A declaration reversed its prior target and continues from the current
    /// displayed value.
    Reversed,
    /// An active declaration was removed before completion.
    Cancelled,
    /// An active declaration reached its endpoint.
    Completed,
}

/// Result of stepping an animation controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationStep {
    pub lifecycle: AnimationLifecycle,
    pub active: bool,
}

impl AnimationStep {
    pub const fn idle() -> Self {
        Self {
            lifecycle: AnimationLifecycle::Idle,
            active: false,
        }
    }
}
