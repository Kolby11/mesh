use crate::node_can_receive_target;
use mesh_core_elements::{InteractionTarget, NodeId, WidgetNode};
use std::collections::HashMap;

/// The kind of continuous interaction that currently owns a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureKind {
    Swipe,
    Pinch,
    Hold,
    Touch(i32),
}

/// A pointer press is kept separate from the current hit-test result.  The
/// capture and press-origin records are updated together by a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PressOrigin {
    pub node_id: NodeId,
    pub button: u32,
}

/// A scroll owner may outlive the raw wheel event while smooth scrolling or
/// inertia is being advanced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollOwner {
    pub node_id: NodeId,
}

/// Typed invalidation produced by an interaction transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InteractionInvalidation(u8);

impl InteractionInvalidation {
    pub const FOCUS: Self = Self(1 << 0);
    pub const POINTER_CAPTURE: Self = Self(1 << 1);
    pub const PRESS_ORIGIN: Self = Self(1 << 2);
    pub const GESTURE: Self = Self(1 << 3);
    pub const SCROLL: Self = Self(1 << 4);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// The result of one committed interaction transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionDelta {
    pub revision: u64,
    pub changed_nodes: Vec<NodeId>,
    pub invalidation: InteractionInvalidation,
    pub focus_before: Option<NodeId>,
    pub focus_after: Option<NodeId>,
    pub pointer_capture_before: Option<PressOrigin>,
    pub pointer_capture_after: Option<PressOrigin>,
    pub press_origin_before: Option<PressOrigin>,
    pub press_origin_after: Option<PressOrigin>,
    pub gesture_before: Option<(NodeId, GestureKind)>,
    pub gesture_after: Option<(NodeId, GestureKind)>,
    pub scroll_before: Option<ScrollOwner>,
    pub scroll_after: Option<ScrollOwner>,
}

impl InteractionDelta {
    pub fn changed(&self) -> bool {
        !self.invalidation.is_empty() || !self.changed_nodes.is_empty()
    }
}

/// The committed ownership state for one surface/seat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InteractionState {
    revision: u64,
    focus_owner: Option<NodeId>,
    focus_visible_owner: Option<NodeId>,
    pointer_capture: Option<PressOrigin>,
    press_origin: Option<PressOrigin>,
    gesture_owner: Option<(NodeId, GestureKind)>,
    scroll_owner: Option<ScrollOwner>,
    touch_owners: HashMap<i32, NodeId>,
}

impl InteractionState {
    pub fn begin(&self) -> InteractionTransaction {
        InteractionTransaction::new(self)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn focus_owner(&self) -> Option<NodeId> {
        self.focus_owner
    }

    pub fn focus_visible_owner(&self) -> Option<NodeId> {
        self.focus_visible_owner
    }

    pub fn pointer_capture(&self) -> Option<PressOrigin> {
        self.pointer_capture
    }

    pub fn press_origin(&self) -> Option<PressOrigin> {
        self.press_origin
    }

    pub fn gesture_owner(&self) -> Option<(NodeId, GestureKind)> {
        self.gesture_owner
    }

    pub fn scroll_owner(&self) -> Option<ScrollOwner> {
        self.scroll_owner
    }

    pub fn touch_owner(&self, touch_id: i32) -> Option<NodeId> {
        self.touch_owners.get(&touch_id).copied()
    }

    pub fn touch_ids(&self) -> impl Iterator<Item = i32> + '_ {
        self.touch_owners.keys().copied()
    }
}

/// A staged, all-or-nothing update to [`InteractionState`].
///
/// Callers resolve targets and handler policy outside this type, then stage
/// ownership changes here.  No state is published until [`Self::commit`] is
/// called, so focus, capture, press origin, gesture ownership, scroll
/// ownership, and the dirty-node set describe one input boundary.
#[derive(Debug, Clone)]
pub struct InteractionTransaction {
    before: InteractionState,
    next: InteractionState,
    changed_nodes: Vec<NodeId>,
    invalidation: InteractionInvalidation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipDecision {
    Acquired,
    Retained,
    Released,
    Rejected,
}

impl InteractionTransaction {
    fn new(state: &InteractionState) -> Self {
        Self {
            before: state.clone(),
            next: state.clone(),
            changed_nodes: Vec::new(),
            invalidation: InteractionInvalidation::empty(),
        }
    }

    pub fn focus(&mut self, target: Option<NodeId>, focus_visible: bool) {
        if self.next.focus_owner != target
            || self.next.focus_visible_owner != (focus_visible.then_some(target).flatten())
        {
            self.mark_node(self.next.focus_owner);
            self.mark_node(target);
            self.next.focus_owner = target;
            self.next.focus_visible_owner = focus_visible.then_some(target).flatten();
            self.invalidation.insert(InteractionInvalidation::FOCUS);
        }
    }

    pub fn capture_pointer(&mut self, origin: Option<PressOrigin>) {
        if self.next.pointer_capture != origin {
            self.mark_node(self.next.pointer_capture.map(|value| value.node_id));
            self.mark_node(origin.map(|value| value.node_id));
            self.next.pointer_capture = origin;
            self.invalidation
                .insert(InteractionInvalidation::POINTER_CAPTURE);
        }
        if self.next.press_origin != origin {
            self.mark_node(self.next.press_origin.map(|value| value.node_id));
            self.mark_node(origin.map(|value| value.node_id));
            self.next.press_origin = origin;
            self.invalidation
                .insert(InteractionInvalidation::PRESS_ORIGIN);
        }
    }

    pub fn release_pointer(&mut self, button: Option<u32>) -> Option<PressOrigin> {
        let origin = self.next.press_origin;
        if button.is_none() || origin.is_some_and(|value| Some(value.button) == button) {
            self.capture_pointer(None);
        }
        origin
    }

    pub fn claim_gesture(&mut self, node_id: NodeId, kind: GestureKind) -> OwnershipDecision {
        match self.next.gesture_owner {
            None => {
                self.next.gesture_owner = Some((node_id, kind));
                self.mark_node(Some(node_id));
                self.invalidation.insert(InteractionInvalidation::GESTURE);
                OwnershipDecision::Acquired
            }
            Some(owner) if owner == (node_id, kind) => OwnershipDecision::Retained,
            Some(_) => OwnershipDecision::Rejected,
        }
    }

    pub fn release_gesture(&mut self, node_id: Option<NodeId>) -> OwnershipDecision {
        if node_id.is_none() || self.next.gesture_owner.map(|owner| owner.0) == node_id {
            self.mark_node(self.next.gesture_owner.map(|owner| owner.0));
            self.next.gesture_owner = None;
            self.invalidation.insert(InteractionInvalidation::GESTURE);
            OwnershipDecision::Released
        } else {
            OwnershipDecision::Rejected
        }
    }

    pub fn claim_scroll(&mut self, node_id: NodeId) -> OwnershipDecision {
        match self.next.scroll_owner {
            None => {
                self.next.scroll_owner = Some(ScrollOwner { node_id });
                self.mark_node(Some(node_id));
                self.invalidation.insert(InteractionInvalidation::SCROLL);
                OwnershipDecision::Acquired
            }
            Some(owner) if owner.node_id == node_id => OwnershipDecision::Retained,
            Some(_) => OwnershipDecision::Rejected,
        }
    }

    /// Transfer scroll ownership at an input boundary. A wheel/touchpad
    /// sequence may move from one scroll container to another, but the
    /// previous owner is still released in the same staged delta.
    pub fn transfer_scroll(&mut self, node_id: NodeId) -> OwnershipDecision {
        if self
            .next
            .scroll_owner
            .is_some_and(|owner| owner.node_id == node_id)
        {
            return OwnershipDecision::Retained;
        }
        self.mark_node(self.next.scroll_owner.map(|owner| owner.node_id));
        self.mark_node(Some(node_id));
        self.next.scroll_owner = Some(ScrollOwner { node_id });
        self.invalidation.insert(InteractionInvalidation::SCROLL);
        OwnershipDecision::Acquired
    }

    pub fn release_scroll(&mut self, node_id: Option<NodeId>) -> OwnershipDecision {
        if node_id.is_none() || self.next.scroll_owner.map(|owner| owner.node_id) == node_id {
            self.mark_node(self.next.scroll_owner.map(|owner| owner.node_id));
            self.next.scroll_owner = None;
            self.invalidation.insert(InteractionInvalidation::SCROLL);
            OwnershipDecision::Released
        } else {
            OwnershipDecision::Rejected
        }
    }

    pub fn claim_touch(&mut self, touch_id: i32, node_id: NodeId) -> OwnershipDecision {
        match self.next.touch_owners.get(&touch_id).copied() {
            None => {
                self.next.touch_owners.insert(touch_id, node_id);
                self.mark_node(Some(node_id));
                self.invalidation.insert(InteractionInvalidation::GESTURE);
                OwnershipDecision::Acquired
            }
            Some(owner) if owner == node_id => OwnershipDecision::Retained,
            Some(_) => OwnershipDecision::Rejected,
        }
    }

    pub fn release_touch(&mut self, touch_id: i32) -> OwnershipDecision {
        let Some(node_id) = self.next.touch_owners.remove(&touch_id) else {
            return OwnershipDecision::Rejected;
        };
        self.mark_node(Some(node_id));
        self.invalidation.insert(InteractionInvalidation::GESTURE);
        OwnershipDecision::Released
    }

    /// Drop owners that are no longer valid for the current tree. This is the
    /// same eligibility gate used by hit testing, focus, gestures, and scroll.
    pub fn reconcile(&mut self, root: &WidgetNode) {
        if self
            .next
            .focus_owner
            .is_some_and(|id| !node_can_receive_target(root, id, InteractionTarget::Focus))
        {
            self.focus(None, false);
        }
        if self.next.pointer_capture.is_some_and(|origin| {
            !node_can_receive_target(root, origin.node_id, InteractionTarget::Pointer)
        }) || self.next.press_origin.is_some_and(|origin| {
            !node_can_receive_target(root, origin.node_id, InteractionTarget::Pointer)
        }) {
            self.capture_pointer(None);
        }
        if self
            .next
            .gesture_owner
            .is_some_and(|(id, _)| !node_can_receive_target(root, id, InteractionTarget::Gesture))
        {
            self.release_gesture(None);
        }
        if self.next.scroll_owner.is_some_and(|owner| {
            !node_can_receive_target(root, owner.node_id, InteractionTarget::Scroll)
        }) {
            self.release_scroll(None);
        }
        let stale_touches: Vec<i32> = self
            .next
            .touch_owners
            .iter()
            .filter_map(|(touch_id, node_id)| {
                (!node_can_receive_target(root, *node_id, InteractionTarget::Gesture))
                    .then_some(*touch_id)
            })
            .collect();
        for touch_id in stale_touches {
            self.release_touch(touch_id);
        }
    }

    pub fn commit(mut self, state: &mut InteractionState) -> InteractionDelta {
        let changed = self.next != self.before;
        if changed {
            self.next.revision = self.before.revision.saturating_add(1);
            *state = self.next.clone();
        }
        InteractionDelta {
            revision: state.revision,
            changed_nodes: self.changed_nodes,
            invalidation: self.invalidation,
            focus_before: self.before.focus_owner,
            focus_after: state.focus_owner,
            pointer_capture_before: self.before.pointer_capture,
            pointer_capture_after: state.pointer_capture,
            press_origin_before: self.before.press_origin,
            press_origin_after: state.press_origin,
            gesture_before: self.before.gesture_owner,
            gesture_after: state.gesture_owner,
            scroll_before: self.before.scroll_owner,
            scroll_after: state.scroll_owner,
        }
    }

    fn mark_node(&mut self, node_id: Option<NodeId>) {
        if let Some(node_id) = node_id
            && !self.changed_nodes.contains(&node_id)
        {
            self.changed_nodes.push(node_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{Dimension, LayoutEngine};

    fn tree() -> (WidgetNode, NodeId, NodeId) {
        let mut root = WidgetNode::new("row");
        root.computed_style.width = Dimension::Px(100.0);
        root.computed_style.height = Dimension::Px(40.0);
        let mut first = WidgetNode::new("button");
        first.computed_style.width = Dimension::Px(50.0);
        first.computed_style.height = Dimension::Px(40.0);
        let first_id = first.id;
        let mut second = WidgetNode::new("scroll");
        second.computed_style.width = Dimension::Px(50.0);
        second.computed_style.height = Dimension::Px(40.0);
        let second_id = second.id;
        root.children = vec![first, second].into();
        LayoutEngine::compute(&mut root, 100.0, 40.0);
        (root, first_id, second_id)
    }

    #[test]
    fn commit_publishes_all_owners_and_dirty_nodes_together() {
        let (_root, focus_id, scroll_id) = tree();
        let mut state = InteractionState::default();
        let mut transaction = state.begin();
        transaction.focus(Some(focus_id), true);
        transaction.capture_pointer(Some(PressOrigin {
            node_id: focus_id,
            button: 0x110,
        }));
        assert_eq!(
            transaction.claim_gesture(focus_id, GestureKind::Swipe),
            OwnershipDecision::Acquired
        );
        assert_eq!(
            transaction.claim_scroll(scroll_id),
            OwnershipDecision::Acquired
        );
        let delta = transaction.commit(&mut state);

        assert_eq!(state.focus_owner(), Some(focus_id));
        assert_eq!(
            state.pointer_capture().map(|value| value.node_id),
            Some(focus_id)
        );
        assert_eq!(
            state.press_origin().map(|value| value.node_id),
            Some(focus_id)
        );
        assert_eq!(state.gesture_owner(), Some((focus_id, GestureKind::Swipe)));
        assert_eq!(
            state.scroll_owner().map(|value| value.node_id),
            Some(scroll_id)
        );
        assert!(delta.invalidation.contains(InteractionInvalidation::FOCUS));
        assert!(
            delta
                .invalidation
                .contains(InteractionInvalidation::POINTER_CAPTURE)
        );
        assert!(
            delta
                .invalidation
                .contains(InteractionInvalidation::PRESS_ORIGIN)
        );
        assert!(
            delta
                .invalidation
                .contains(InteractionInvalidation::GESTURE)
        );
        assert!(delta.invalidation.contains(InteractionInvalidation::SCROLL));
        assert_eq!(delta.changed_nodes, vec![focus_id, scroll_id]);
    }

    #[test]
    fn competing_owners_are_rejected_without_partial_state() {
        let (_root, first_id, second_id) = tree();
        let mut state = InteractionState::default();
        let mut initial = state.begin();
        assert_eq!(
            initial.claim_gesture(first_id, GestureKind::Pinch),
            OwnershipDecision::Acquired
        );
        assert_eq!(initial.claim_scroll(second_id), OwnershipDecision::Acquired);
        initial.commit(&mut state);

        let mut competing = state.begin();
        assert_eq!(
            competing.claim_gesture(second_id, GestureKind::Swipe),
            OwnershipDecision::Rejected
        );
        assert_eq!(
            competing.claim_scroll(first_id),
            OwnershipDecision::Rejected
        );
        let delta = competing.commit(&mut state);
        assert!(!delta.changed());
        assert_eq!(state.gesture_owner(), Some((first_id, GestureKind::Pinch)));
        assert_eq!(
            state.scroll_owner().map(|value| value.node_id),
            Some(second_id)
        );
    }

    #[test]
    fn reconcile_cancels_ineligible_owners_in_the_same_delta() {
        let (mut root, focus_id, _scroll_id) = tree();
        let mut state = InteractionState::default();
        let mut transaction = state.begin();
        transaction.focus(Some(focus_id), true);
        transaction.capture_pointer(Some(PressOrigin {
            node_id: focus_id,
            button: 0x110,
        }));
        transaction.commit(&mut state);

        root.children[0]
            .attributes
            .insert("disabled".into(), "true".into());
        let mut reconcile = state.begin();
        reconcile.reconcile(&root);
        let delta = reconcile.commit(&mut state);

        assert_eq!(state.focus_owner(), None);
        assert_eq!(state.pointer_capture(), None);
        assert!(delta.changed_nodes.contains(&focus_id));
        assert!(delta.invalidation.contains(InteractionInvalidation::FOCUS));
        assert!(
            delta
                .invalidation
                .contains(InteractionInvalidation::POINTER_CAPTURE)
        );
        assert!(
            delta
                .invalidation
                .contains(InteractionInvalidation::PRESS_ORIGIN)
        );
    }
}
