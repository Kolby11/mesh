//! Renderer-neutral contract for one coherent interaction/render frame.
//!
//! [`InteractionState`] owns the persistent interaction state and
//! [`InteractionTransaction`] owns one input-boundary mutation. This module
//! owns the cross-phase hand-off: input and state changes are recorded first,
//! then typed invalidation, style invalidation, layout, animation, paint, and
//! semantics advance through one ordered frame with shared revisions and dirty
//! output.

use std::fmt;
use std::time::Instant;

use mesh_core_elements::{FrameSnapshot, NodeId};

use super::transaction::{
    InteractionDecision, InteractionDelta, InteractionDirty, InteractionDirtyFlags,
    InteractionInvalidation, InteractionState, PressOrigin,
};

/// The renderer-neutral phases of one interaction frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InteractionFramePhase {
    InputResolved,
    StateUpdated,
    StyleInvalidated,
    LayoutReady,
    AnimationSampled,
    PaintReady,
    SemanticsReady,
}

impl InteractionFramePhase {
    const ALL: [Self; 7] = [
        Self::InputResolved,
        Self::StateUpdated,
        Self::StyleInvalidated,
        Self::LayoutReady,
        Self::AnimationSampled,
        Self::PaintReady,
        Self::SemanticsReady,
    ];

    const fn index(self) -> usize {
        match self {
            Self::InputResolved => 0,
            Self::StateUpdated => 1,
            Self::StyleInvalidated => 2,
            Self::LayoutReady => 3,
            Self::AnimationSampled => 4,
            Self::PaintReady => 5,
            Self::SemanticsReady => 6,
        }
    }

    const fn next(self) -> Option<Self> {
        match self {
            Self::InputResolved => Some(Self::StateUpdated),
            Self::StateUpdated => Some(Self::StyleInvalidated),
            Self::StyleInvalidated => Some(Self::LayoutReady),
            Self::LayoutReady => Some(Self::AnimationSampled),
            Self::AnimationSampled => Some(Self::PaintReady),
            Self::PaintReady => Some(Self::SemanticsReady),
            Self::SemanticsReady => None,
        }
    }
}

/// A phase stamp tied to one interaction-frame revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InteractionPhaseStamp {
    phase: InteractionFramePhase,
    revision: u64,
}

impl InteractionPhaseStamp {
    pub const fn phase(self) -> InteractionFramePhase {
        self.phase
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// A compact immutable view of the ownership state consumed by downstream
/// frame phases.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InteractionStateSnapshot {
    pub revision: u64,
    pub focus_owner: Option<NodeId>,
    pub focus_visible_owner: Option<NodeId>,
    pub pointer_capture: Option<PressOrigin>,
    pub press_origin: Option<PressOrigin>,
    pub gesture_owner: Option<(NodeId, super::transaction::GestureKind)>,
    pub scroll_owner: Option<super::transaction::ScrollOwner>,
    pub touch_owners: Box<[(i32, NodeId)]>,
}

impl InteractionStateSnapshot {
    pub fn from_state(state: &InteractionState) -> Self {
        let mut touch_owners = state
            .touch_ids()
            .filter_map(|touch_id| {
                state
                    .touch_owner(touch_id)
                    .map(|node_id| (touch_id, node_id))
            })
            .collect::<Vec<_>>();
        touch_owners.sort_by_key(|(touch_id, _)| *touch_id);
        Self {
            revision: state.revision(),
            focus_owner: state.focus_owner(),
            focus_visible_owner: state.focus_visible_owner(),
            pointer_capture: state.pointer_capture(),
            press_origin: state.press_origin(),
            gesture_owner: state.gesture_owner(),
            scroll_owner: state.scroll_owner(),
            touch_owners: touch_owners.into_boxed_slice(),
        }
    }
}

/// A phase-ordering or snapshot-publication failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionFrameError {
    /// A phase was skipped, repeated out of order, or advanced backwards.
    PhaseOrder {
        current: Option<InteractionFramePhase>,
        requested: InteractionFramePhase,
    },
    /// The immutable tree snapshot belongs to a different frame revision.
    SnapshotRevision { expected: u64, actual: u64 },
}

impl fmt::Display for InteractionFrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PhaseOrder { current, requested } => write!(
                formatter,
                "interaction frame cannot advance from {current:?} to {requested:?}"
            ),
            Self::SnapshotRevision { expected, actual } => write!(
                formatter,
                "interaction frame expects tree revision {expected}, got snapshot revision {actual}"
            ),
        }
    }
}

impl std::error::Error for InteractionFrameError {}

/// One coherent, renderer-neutral interaction frame.
///
/// The frame contains no renderer, Wayland, or script handles. Consumers can
/// use the same state snapshot, phase revision, typed decisions, dirty nodes,
/// typed invalidation, and immutable tree snapshot while each subsystem remains
/// responsible for its own work.
#[derive(Debug, Clone)]
pub struct InteractionFrame {
    revision: u64,
    input_revision: u64,
    tree_revision: u64,
    started_at: Instant,
    phase: Option<InteractionFramePhase>,
    phase_stamps: [Option<InteractionPhaseStamp>; InteractionFramePhase::ALL.len()],
    state: InteractionStateSnapshot,
    dirty: InteractionDirty,
    decisions: Vec<InteractionDecision>,
    invalidation: InteractionInvalidation,
    deferred_dirty: InteractionDirty,
    deferred_decisions: Vec<InteractionDecision>,
    deferred_invalidation: InteractionInvalidation,
    tree_snapshot: Option<FrameSnapshot>,
}

impl Default for InteractionFrame {
    fn default() -> Self {
        Self {
            revision: 0,
            input_revision: 0,
            tree_revision: 0,
            started_at: Instant::now(),
            phase: None,
            phase_stamps: [None; InteractionFramePhase::ALL.len()],
            state: InteractionStateSnapshot::default(),
            dirty: InteractionDirty::empty(),
            decisions: Vec::new(),
            invalidation: InteractionInvalidation::empty(),
            deferred_dirty: InteractionDirty::empty(),
            deferred_decisions: Vec::new(),
            deferred_invalidation: InteractionInvalidation::empty(),
            tree_snapshot: None,
        }
    }
}

impl InteractionFrame {
    /// Start a new frame with an explicit state and revision boundary.
    pub fn begin(
        revision: u64,
        input_revision: u64,
        tree_revision: u64,
        started_at: Instant,
        state: InteractionStateSnapshot,
    ) -> Self {
        Self {
            revision,
            input_revision,
            tree_revision,
            started_at,
            ..Self::default()
        }
        .with_state(state)
    }

    fn with_state(mut self, state: InteractionStateSnapshot) -> Self {
        self.state = state;
        self
    }

    /// Prepare a frame for painting. A frame that already contains input and
    /// state decisions keeps those decisions; an incomplete downstream frame
    /// or a completed frame starts fresh.
    pub fn prepare_for_paint(
        &mut self,
        tree_revision: u64,
        state: InteractionStateSnapshot,
        started_at: Instant,
    ) {
        if self.phase.is_none()
            || self
                .phase
                .is_some_and(|phase| phase > InteractionFramePhase::StateUpdated)
        {
            self.begin_next_frame(tree_revision, state, started_at);
        } else {
            self.tree_revision = tree_revision;
            self.input_revision = state.revision;
            self.state = state;
        }
        self.advance_to_state_updated();
    }

    /// Record one committed input/state transition in this frame.
    pub fn record_interaction_delta(&mut self, delta: &InteractionDelta, state: &InteractionState) {
        if self.phase == Some(InteractionFramePhase::SemanticsReady) {
            self.begin_next_frame(
                self.tree_revision,
                InteractionStateSnapshot::from_state(state),
                Instant::now(),
            );
        } else if self
            .phase
            .is_some_and(|phase| phase > InteractionFramePhase::StateUpdated)
        {
            // A render-time reconciliation can discover that an input target
            // disappeared after layout has already started. Keep that delta
            // for the next frame rather than mutating this frame's published
            // state or attempting to move its phase backwards.
            merge_dirty(
                &mut self.deferred_dirty,
                delta.dirty.nodes.iter().copied(),
                delta.dirty.flags,
            );
            self.deferred_decisions
                .extend(delta.decisions.iter().copied());
            self.deferred_invalidation = self.deferred_invalidation.union(delta.invalidation);
            return;
        } else if self.phase.is_none() && self.revision == 0 {
            self.begin_next_frame(
                self.tree_revision,
                InteractionStateSnapshot::from_state(state),
                Instant::now(),
            );
        }
        self.input_revision = delta.revision;
        self.state = InteractionStateSnapshot::from_state(state);
        self.advance_to_state_updated();
        self.record_dirty(delta.dirty.nodes.iter().copied(), delta.dirty.flags);
        self.decisions.extend(delta.decisions.iter().copied());
        self.invalidation = self.invalidation.union(delta.invalidation);
    }

    /// Add dirty output without coupling the frame contract to a renderer.
    pub fn record_dirty<I>(&mut self, nodes: I, flags: InteractionDirtyFlags)
    where
        I: IntoIterator<Item = NodeId>,
    {
        for node_id in nodes {
            if !self.dirty.nodes.contains(&node_id) {
                self.dirty.nodes.push(node_id);
            }
        }
        self.dirty.flags = self.dirty.flags | flags;
    }

    /// Advance exactly one phase. Skipping a phase is rejected so ordering
    /// mistakes cannot silently publish a frame with mixed revisions.
    pub fn advance(
        &mut self,
        requested: InteractionFramePhase,
    ) -> Result<(), InteractionFrameError> {
        if self.phase == Some(requested) {
            return Ok(());
        }
        let valid = match self.phase {
            None => requested == InteractionFramePhase::InputResolved,
            Some(current) => current.next() == Some(requested),
        };
        if !valid {
            return Err(InteractionFrameError::PhaseOrder {
                current: self.phase,
                requested,
            });
        }
        let stamp = InteractionPhaseStamp {
            phase: requested,
            revision: self.revision,
        };
        self.phase = Some(requested);
        self.phase_stamps[requested.index()] = Some(stamp);
        Ok(())
    }

    /// Publish the immutable post-layout tree snapshot for this frame.
    pub fn publish_tree_snapshot(
        &mut self,
        snapshot: FrameSnapshot,
    ) -> Result<(), InteractionFrameError> {
        if snapshot.revision() != self.tree_revision {
            return Err(InteractionFrameError::SnapshotRevision {
                expected: self.tree_revision,
                actual: snapshot.revision(),
            });
        }
        if !snapshot.semantic_diff().is_empty() {
            let nodes = snapshot
                .semantic_diff()
                .changes()
                .iter()
                .filter_map(|change| snapshot.node(&change.identity).map(|node| node.id()))
                .collect::<Vec<_>>();
            self.record_dirty(nodes, InteractionDirtyFlags::ACCESSIBILITY);
        }
        self.tree_snapshot = Some(snapshot);
        Ok(())
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn input_revision(&self) -> u64 {
        self.input_revision
    }

    pub fn tree_revision(&self) -> u64 {
        self.tree_revision
    }

    pub fn started_at(&self) -> Instant {
        self.started_at
    }

    pub fn phase(&self) -> Option<InteractionFramePhase> {
        self.phase
    }

    pub fn phase_stamp(&self, phase: InteractionFramePhase) -> Option<InteractionPhaseStamp> {
        self.phase_stamps[phase.index()]
    }

    pub fn is_complete(&self) -> bool {
        self.phase == Some(InteractionFramePhase::SemanticsReady)
    }

    pub fn is_publishable(&self) -> bool {
        self.is_complete() && self.tree_snapshot.is_some()
    }

    pub fn state(&self) -> &InteractionStateSnapshot {
        &self.state
    }

    pub fn dirty(&self) -> &InteractionDirty {
        &self.dirty
    }

    pub fn decisions(&self) -> &[InteractionDecision] {
        &self.decisions
    }

    pub fn invalidation(&self) -> InteractionInvalidation {
        self.invalidation
    }

    pub fn tree_snapshot(&self) -> Option<&FrameSnapshot> {
        self.tree_snapshot.as_ref()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn advance_required(&mut self, phase: InteractionFramePhase) {
        self.advance(phase)
            .expect("interaction frame phase order is internal")
    }

    fn advance_to_state_updated(&mut self) {
        match self.phase {
            None => self.advance_required(InteractionFramePhase::InputResolved),
            Some(InteractionFramePhase::InputResolved) => {}
            Some(InteractionFramePhase::StateUpdated) => return,
            Some(phase) => {
                panic!("interaction frame phase {phase:?} cannot be rewound to state update")
            }
        }
        self.advance_required(InteractionFramePhase::StateUpdated);
    }

    fn begin_next_frame(
        &mut self,
        tree_revision: u64,
        state: InteractionStateSnapshot,
        started_at: Instant,
    ) {
        let revision = self.revision.saturating_add(1);
        let deferred_dirty = std::mem::take(&mut self.deferred_dirty);
        let deferred_decisions = std::mem::take(&mut self.deferred_decisions);
        let deferred_invalidation = std::mem::take(&mut self.deferred_invalidation);
        *self = Self::begin(revision, state.revision, tree_revision, started_at, state);
        self.dirty = deferred_dirty;
        self.decisions = deferred_decisions;
        self.invalidation = deferred_invalidation;
    }
}

fn merge_dirty<I>(dirty: &mut InteractionDirty, nodes: I, flags: InteractionDirtyFlags)
where
    I: IntoIterator<Item = NodeId>,
{
    for node_id in nodes {
        if !dirty.nodes.contains(&node_id) {
            dirty.nodes.push(node_id);
        }
    }
    dirty.flags = dirty.flags | flags;
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{Dimension, LayoutEngine, WidgetNode};

    fn state_with_focus() -> (InteractionState, NodeId, InteractionDelta) {
        let mut root = WidgetNode::new("button");
        root.computed_style.width = Dimension::Px(32.0);
        root.computed_style.height = Dimension::Px(24.0);
        LayoutEngine::compute(&mut root, 32.0, 24.0);
        let node_id = root.id;
        let mut state = InteractionState::default();
        let mut transaction = state.begin();
        transaction.focus(Some(node_id), true);
        let delta = transaction.commit(&mut state);
        (state, node_id, delta)
    }

    #[test]
    fn frame_orders_phases_and_keeps_one_revision() {
        let (state, node_id, delta) = state_with_focus();
        let mut frame = InteractionFrame::default();
        frame.record_interaction_delta(&delta, &state);
        assert_eq!(frame.phase(), Some(InteractionFramePhase::StateUpdated));
        assert_eq!(frame.input_revision(), state.revision());
        assert!(frame.dirty().nodes.contains(&node_id));
        assert!(
            frame
                .invalidation()
                .contains(InteractionInvalidation::FOCUS)
        );

        let error = frame
            .advance(InteractionFramePhase::LayoutReady)
            .expect_err("a frame cannot skip style invalidation");
        assert!(matches!(error, InteractionFrameError::PhaseOrder { .. }));

        for phase in [
            InteractionFramePhase::StyleInvalidated,
            InteractionFramePhase::LayoutReady,
            InteractionFramePhase::AnimationSampled,
            InteractionFramePhase::PaintReady,
            InteractionFramePhase::SemanticsReady,
        ] {
            frame.advance(phase).expect("ordered phase");
        }
        assert!(frame.is_complete());
        assert!(
            frame
                .phase_stamp(InteractionFramePhase::PaintReady)
                .is_some_and(|stamp| stamp.revision() == frame.revision())
        );
    }

    #[test]
    fn frame_merges_dirty_nodes_and_semantic_snapshot_output() {
        let (state, first_id, delta) = state_with_focus();
        let mut frame = InteractionFrame::begin(
            7,
            state.revision(),
            7,
            Instant::now(),
            InteractionStateSnapshot::from_state(&state),
        );
        frame.record_interaction_delta(&delta, &state);
        frame.record_dirty(
            [first_id],
            InteractionDirtyFlags::LAYOUT | InteractionDirtyFlags::ANIMATION,
        );
        let mut root = WidgetNode::new("box");
        root.computed_style.width = Dimension::Px(40.0);
        root.computed_style.height = Dimension::Px(20.0);
        LayoutEngine::compute(&mut root, 40.0, 20.0);
        let snapshot = FrameSnapshot::complete(&root, 7, None).expect("valid snapshot");
        frame
            .publish_tree_snapshot(snapshot)
            .expect("matching tree revision");

        assert!(frame.dirty().contains(InteractionDirtyFlags::LAYOUT));
        assert!(frame.dirty().contains(InteractionDirtyFlags::ANIMATION));
        assert!(frame.dirty().contains(InteractionDirtyFlags::ACCESSIBILITY));
        assert_eq!(frame.tree_snapshot().map(FrameSnapshot::revision), Some(7));
    }

    #[test]
    fn frame_carries_all_typed_interaction_invalidation() {
        let (mut state, node_id, _) = state_with_focus();
        let mut transaction = state.begin();
        transaction.capture_pointer(Some(PressOrigin {
            node_id,
            button: 0x110,
        }));
        transaction.claim_gesture(node_id, super::super::transaction::GestureKind::Swipe);
        transaction.claim_scroll(node_id);
        let delta = transaction.commit(&mut state);

        let mut frame = InteractionFrame::default();
        frame.record_interaction_delta(&delta, &state);

        for invalidation in [
            InteractionInvalidation::POINTER_CAPTURE,
            InteractionInvalidation::PRESS_ORIGIN,
            InteractionInvalidation::GESTURE,
            InteractionInvalidation::SCROLL,
        ] {
            assert!(frame.invalidation().contains(invalidation));
        }
    }

    #[test]
    fn preparing_after_completion_starts_a_new_frame() {
        let (state, _node_id, delta) = state_with_focus();
        let mut frame = InteractionFrame::default();
        frame.record_interaction_delta(&delta, &state);
        for phase in [
            InteractionFramePhase::StyleInvalidated,
            InteractionFramePhase::LayoutReady,
            InteractionFramePhase::AnimationSampled,
            InteractionFramePhase::PaintReady,
            InteractionFramePhase::SemanticsReady,
        ] {
            frame.advance(phase).expect("ordered phase");
        }
        let previous_revision = frame.revision();
        frame.prepare_for_paint(
            2,
            InteractionStateSnapshot::from_state(&state),
            Instant::now(),
        );
        assert_eq!(frame.revision(), previous_revision + 1);
        assert_eq!(frame.tree_revision(), 2);
        assert_eq!(frame.phase(), Some(InteractionFramePhase::StateUpdated));
        assert!(frame.dirty().is_empty());
    }

    #[test]
    fn late_state_updates_are_deferred_to_the_next_frame() {
        let (mut state, node_id, delta) = state_with_focus();
        let mut frame = InteractionFrame::default();
        frame.record_interaction_delta(&delta, &state);
        frame
            .advance(InteractionFramePhase::StyleInvalidated)
            .expect("style phase");

        let mut transaction = state.begin();
        transaction.focus(None, false);
        let late_delta = transaction.commit(&mut state);
        frame.record_interaction_delta(&late_delta, &state);
        assert_eq!(frame.state().focus_owner, Some(node_id));
        assert_eq!(frame.decisions().len(), 1);
        assert!(
            frame
                .invalidation()
                .contains(InteractionInvalidation::FOCUS)
        );

        for phase in [
            InteractionFramePhase::LayoutReady,
            InteractionFramePhase::AnimationSampled,
            InteractionFramePhase::PaintReady,
            InteractionFramePhase::SemanticsReady,
        ] {
            frame.advance(phase).expect("ordered phase");
        }
        let previous_revision = frame.revision();
        frame.prepare_for_paint(
            8,
            InteractionStateSnapshot::from_state(&state),
            Instant::now(),
        );

        assert_eq!(frame.revision(), previous_revision + 1);
        assert_eq!(frame.state().focus_owner, None);
        assert_eq!(frame.decisions().len(), 1);
        assert!(
            frame
                .invalidation()
                .contains(InteractionInvalidation::FOCUS)
        );
    }
}
