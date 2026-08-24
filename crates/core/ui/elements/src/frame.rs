//! Immutable, cross-phase view of one rendered widget tree.
//!
//! The live [`WidgetNode`] tree is intentionally mutable: input, style, and
//! layout each need to update it while a frame is being prepared. Consumers
//! must not observe that working state half-way through a frame, however.
//! [`FrameSnapshot`] is the hand-off boundary. It owns copies of all data
//! needed by downstream consumers and can only be changed by constructing a
//! new snapshot.

use crate::accessibility::{AccessibilityInfo, AccessibilityState, AccessibilityTree};
use crate::attributes::AttributeMap;
use crate::element::element_runtime_tag_for_tag;
use crate::layout::LayoutRect;
use crate::style::ComputedStyle;
use crate::tree::{ElementState, NodeId, WidgetNode};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

/// The ordered phases that make a frame safe to publish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FramePhase {
    TreeBuilt,
    StateAnnotated,
    Styled,
    LaidOut,
    SemanticsReady,
}

impl FramePhase {
    pub const ALL: [Self; 5] = [
        Self::TreeBuilt,
        Self::StateAnnotated,
        Self::Styled,
        Self::LaidOut,
        Self::SemanticsReady,
    ];

    const fn index(self) -> usize {
        match self {
            Self::TreeBuilt => 0,
            Self::StateAnnotated => 1,
            Self::Styled => 2,
            Self::LaidOut => 3,
            Self::SemanticsReady => 4,
        }
    }
}

/// A revision stamp for one completed frame phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseStamp {
    phase: FramePhase,
    revision: u64,
}

impl PhaseStamp {
    pub const fn new(phase: FramePhase, revision: u64) -> Self {
        Self { phase, revision }
    }

    pub const fn phase(self) -> FramePhase {
        self.phase
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// Phase completion stamps carried by an immutable frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePhaseStamps {
    stamps: [Option<PhaseStamp>; FramePhase::ALL.len()],
}

impl Default for FramePhaseStamps {
    fn default() -> Self {
        Self {
            stamps: [None; FramePhase::ALL.len()],
        }
    }
}

impl FramePhaseStamps {
    /// Stamp every phase with one frame revision.
    pub fn complete(revision: u64) -> Self {
        let mut stamps = Self::default();
        for phase in FramePhase::ALL {
            stamps.stamps[phase.index()] = Some(PhaseStamp::new(phase, revision));
        }
        stamps
    }

    /// Stamp the phases through `phase`, leaving later phases unpublished.
    pub fn up_to(phase: FramePhase, revision: u64) -> Self {
        let mut stamps = Self::default();
        for candidate in FramePhase::ALL {
            if candidate > phase {
                break;
            }
            stamps.stamps[candidate.index()] = Some(PhaseStamp::new(candidate, revision));
        }
        stamps
    }

    pub fn stamp(&self, phase: FramePhase) -> Option<PhaseStamp> {
        self.stamps[phase.index()]
    }

    pub fn is_complete(&self) -> bool {
        self.stamps.iter().all(Option::is_some)
    }

    pub fn latest(&self) -> Option<PhaseStamp> {
        FramePhase::ALL
            .iter()
            .rev()
            .find_map(|phase| self.stamp(*phase))
    }
}

/// Stable identity used to match nodes across rebuilt or reordered trees.
/// Explicit mesh keys take precedence over the ephemeral construction id;
/// unkeyed nodes retain their runtime `NodeId` identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StableNodeIdentity {
    MeshKey(Arc<str>),
    NodeId(NodeId),
}

impl StableNodeIdentity {
    fn from_node(node: &WidgetNode) -> Self {
        match node.mesh_key().filter(|key| !key.is_empty()) {
            Some(key) => Self::MeshKey(Arc::from(key)),
            None => Self::NodeId(node.id),
        }
    }

    pub fn mesh_key(&self) -> Option<&str> {
        match self {
            Self::MeshKey(key) => Some(key),
            Self::NodeId(_) => None,
        }
    }

    pub fn node_id(&self) -> Option<NodeId> {
        match self {
            Self::MeshKey(_) => None,
            Self::NodeId(id) => Some(*id),
        }
    }
}

impl fmt::Display for StableNodeIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MeshKey(key) => write!(formatter, "mesh-key:{key}"),
            Self::NodeId(id) => write!(formatter, "node-id:{id}"),
        }
    }
}

/// A validated snapshot construction failure.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FrameSnapshotError {
    #[error("frame snapshot is missing one or more phase stamps")]
    IncompletePhaseStamps,
    #[error("frame revision {revision} is older than previous revision {previous}")]
    NonMonotonicRevision { revision: u64, previous: u64 },
    #[error("duplicate runtime node id {id}")]
    DuplicateNodeId { id: NodeId },
    #[error("duplicate stable node identity {identity}")]
    DuplicateIdentity { identity: StableNodeIdentity },
    #[error("node {node_id} has a non-finite {field} value")]
    NonFiniteLayout {
        node_id: NodeId,
        field: &'static str,
    },
    #[error("node {node_id} has a non-finite accessibility value {field}")]
    NonFiniteAccessibilityValue {
        node_id: NodeId,
        field: &'static str,
    },
    #[error("semantic node {node_id} refers to missing node {referenced_id}")]
    DanglingSemanticReference {
        node_id: NodeId,
        referenced_id: NodeId,
    },
}

/// The semantic projection captured for one frame node.
#[derive(Debug, Clone)]
pub struct FrameSemanticNode {
    pub info: AccessibilityInfo,
    pub bounds: LayoutRect,
    pub parent: Option<StableNodeIdentity>,
    pub children: Box<[StableNodeIdentity]>,
    pub relationships: FrameSemanticRelationships,
}

/// Accessibility relationships represented with stable identities instead of
/// the source tree's runtime ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameSemanticRelationships {
    pub labelled_by: Box<[StableNodeIdentity]>,
    pub described_by: Box<[StableNodeIdentity]>,
    pub controls: Box<[StableNodeIdentity]>,
    pub owns: Box<[StableNodeIdentity]>,
    pub details: Box<[StableNodeIdentity]>,
    pub error_message: Box<[StableNodeIdentity]>,
    pub tooltip_for: Option<StableNodeIdentity>,
    pub popover_trigger: Option<StableNodeIdentity>,
}

/// One immutable node record in a [`FrameSnapshot`].
#[derive(Debug, Clone)]
pub struct FrameNode {
    identity: StableNodeIdentity,
    id: NodeId,
    parent: Option<StableNodeIdentity>,
    children: Box<[StableNodeIdentity]>,
    tag: Arc<str>,
    runtime_tag: Option<Arc<str>>,
    attributes: AttributeMap,
    style: Arc<ComputedStyle>,
    layout: LayoutRect,
    state: ElementState,
    semantic: Option<FrameSemanticNode>,
}

impl FrameNode {
    pub fn identity(&self) -> &StableNodeIdentity {
        &self.identity
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn parent(&self) -> Option<&StableNodeIdentity> {
        self.parent.as_ref()
    }

    pub fn children(&self) -> &[StableNodeIdentity] {
        &self.children
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    pub fn runtime_tag(&self) -> Option<&str> {
        self.runtime_tag.as_deref()
    }

    pub fn attributes(&self) -> &AttributeMap {
        &self.attributes
    }

    pub fn style(&self) -> &ComputedStyle {
        &self.style
    }

    pub fn layout(&self) -> LayoutRect {
        self.layout
    }

    pub fn state(&self) -> ElementState {
        self.state
    }

    pub fn semantic(&self) -> Option<&FrameSemanticNode> {
        self.semantic.as_ref()
    }
}

/// Semantic fields that can change without changing a node's stable identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticField {
    Tree,
    Role,
    Name,
    Description,
    Focusability,
    Focus,
    State,
    Visibility,
    Bounds,
    Relationships,
}

impl SemanticField {
    const ALL: [Self; 10] = [
        Self::Tree,
        Self::Role,
        Self::Name,
        Self::Description,
        Self::Focusability,
        Self::Focus,
        Self::State,
        Self::Visibility,
        Self::Bounds,
        Self::Relationships,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticChangeKind {
    Added,
    Removed,
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticChange {
    pub identity: StableNodeIdentity,
    pub kind: SemanticChangeKind,
    pub fields: Box<[SemanticField]>,
}

/// Deterministic semantic delta between two immutable frame snapshots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticDiff {
    changes: Box<[SemanticChange]>,
}

impl SemanticDiff {
    pub fn between(previous: Option<&FrameSnapshot>, current: &FrameSnapshot) -> Self {
        let mut identities = BTreeSet::new();
        if let Some(previous) = previous {
            identities.extend(previous.nodes().iter().map(|node| node.identity.clone()));
        }
        identities.extend(current.nodes().iter().map(|node| node.identity.clone()));

        let changes = identities
            .into_iter()
            .filter_map(|identity| {
                let before = previous.and_then(|snapshot| snapshot.node(&identity));
                let after = current.node(&identity);
                match (before, after) {
                    (None, Some(node)) if node.semantic.is_some() => Some(SemanticChange {
                        identity,
                        kind: SemanticChangeKind::Added,
                        fields: SemanticField::ALL.into(),
                    }),
                    (Some(node), None) if node.semantic.is_some() => Some(SemanticChange {
                        identity,
                        kind: SemanticChangeKind::Removed,
                        fields: SemanticField::ALL.into(),
                    }),
                    (Some(before), Some(after)) => {
                        let fields =
                            semantic_fields(before.semantic.as_ref(), after.semantic.as_ref());
                        (!fields.is_empty()).then_some(SemanticChange {
                            identity,
                            kind: SemanticChangeKind::Updated,
                            fields: fields.into_boxed_slice(),
                        })
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self { changes }
    }

    pub fn changes(&self) -> &[SemanticChange] {
        &self.changes
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn added(&self) -> impl Iterator<Item = &SemanticChange> {
        self.changes
            .iter()
            .filter(|change| change.kind == SemanticChangeKind::Added)
    }

    pub fn removed(&self) -> impl Iterator<Item = &SemanticChange> {
        self.changes
            .iter()
            .filter(|change| change.kind == SemanticChangeKind::Removed)
    }

    pub fn updated(&self) -> impl Iterator<Item = &SemanticChange> {
        self.changes
            .iter()
            .filter(|change| change.kind == SemanticChangeKind::Updated)
    }
}

#[derive(Debug)]
struct FrameSnapshotData {
    revision: u64,
    phases: FramePhaseStamps,
    nodes: Box<[FrameNode]>,
    index: HashMap<StableNodeIdentity, usize>,
    semantic_diff: SemanticDiff,
}

/// Immutable frame hand-off shared by rendering, interaction inspection, and
/// semantic consumers. Cloning it only clones an `Arc`.
#[derive(Clone, Debug)]
pub struct FrameSnapshot(Arc<FrameSnapshotData>);

impl FrameSnapshot {
    /// Capture a complete tree. `previous` is used only to compute the
    /// semantic diff stored in the new snapshot.
    pub fn capture(
        root: &WidgetNode,
        revision: u64,
        phases: FramePhaseStamps,
        previous: Option<&Self>,
    ) -> Result<Self, FrameSnapshotError> {
        if !phases.is_complete() {
            return Err(FrameSnapshotError::IncompletePhaseStamps);
        }
        if previous.is_some_and(|snapshot| revision < snapshot.revision()) {
            return Err(FrameSnapshotError::NonMonotonicRevision {
                revision,
                previous: previous.expect("checked above").revision(),
            });
        }

        let accessibility = AccessibilityTree::from_widget_tree(root);
        let mut nodes = Vec::with_capacity(root.node_count());
        let mut node_ids = HashSet::with_capacity(root.node_count());
        let mut identities = HashSet::with_capacity(root.node_count());
        append_node(root, None, &mut nodes, &mut node_ids, &mut identities)?;

        let mut index = HashMap::with_capacity(nodes.len());
        let mut node_index = HashMap::with_capacity(nodes.len());
        let mut identity_by_id = HashMap::with_capacity(nodes.len());
        for (node_index_value, node) in nodes.iter().enumerate() {
            index.insert(node.identity.clone(), node_index_value);
            node_index.insert(node.id, node_index_value);
            identity_by_id.insert(node.id, node.identity.clone());
        }

        for semantic in &accessibility.nodes {
            let Some(&node_index) = node_index.get(&semantic.id) else {
                return Err(FrameSnapshotError::DanglingSemanticReference {
                    node_id: semantic.id,
                    referenced_id: semantic.id,
                });
            };
            nodes[node_index].semantic = Some(frame_semantic_node(semantic, &identity_by_id)?);
        }

        let mut snapshot = Self(Arc::new(FrameSnapshotData {
            revision,
            phases,
            nodes: nodes.into_boxed_slice(),
            index,
            semantic_diff: SemanticDiff::default(),
        }));
        let semantic_diff = SemanticDiff::between(previous, &snapshot);
        Arc::get_mut(&mut snapshot.0)
            .expect("new frame snapshot has the only data owner")
            .semantic_diff = semantic_diff;
        Ok(snapshot)
    }

    pub fn complete(
        root: &WidgetNode,
        revision: u64,
        previous: Option<&Self>,
    ) -> Result<Self, FrameSnapshotError> {
        Self::capture(
            root,
            revision,
            FramePhaseStamps::complete(revision),
            previous,
        )
    }

    pub fn revision(&self) -> u64 {
        self.0.revision
    }

    pub fn phases(&self) -> &FramePhaseStamps {
        &self.0.phases
    }

    pub fn nodes(&self) -> &[FrameNode] {
        &self.0.nodes
    }

    pub fn root(&self) -> &FrameNode {
        self.0
            .nodes
            .first()
            .expect("a frame snapshot always contains its root node")
    }

    pub fn node(&self, identity: &StableNodeIdentity) -> Option<&FrameNode> {
        self.0
            .index
            .get(identity)
            .map(|index| &self.0.nodes[*index])
    }

    pub fn node_by_id(&self, id: NodeId) -> Option<&FrameNode> {
        self.0.nodes.iter().find(|node| node.id == id)
    }

    pub fn semantic_nodes(&self) -> impl Iterator<Item = &FrameNode> {
        self.0.nodes.iter().filter(|node| node.semantic.is_some())
    }

    pub fn semantic_diff(&self) -> &SemanticDiff {
        &self.0.semantic_diff
    }
}

fn append_node(
    node: &WidgetNode,
    parent: Option<StableNodeIdentity>,
    nodes: &mut Vec<FrameNode>,
    node_ids: &mut HashSet<NodeId>,
    identities: &mut HashSet<StableNodeIdentity>,
) -> Result<(), FrameSnapshotError> {
    if !node_ids.insert(node.id) {
        return Err(FrameSnapshotError::DuplicateNodeId { id: node.id });
    }
    let identity = StableNodeIdentity::from_node(node);
    if !identities.insert(identity.clone()) {
        return Err(FrameSnapshotError::DuplicateIdentity { identity });
    }
    validate_layout(node)?;

    let children = node
        .children
        .iter()
        .map(StableNodeIdentity::from_node)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    nodes.push(FrameNode {
        identity: identity.clone(),
        id: node.id,
        parent,
        children,
        tag: Arc::from(node.tag.as_str()),
        runtime_tag: element_runtime_tag_for_tag(&node.tag).map(Arc::from),
        attributes: node.attributes.clone(),
        style: Arc::new(node.computed_style.clone()),
        layout: node.layout,
        state: node.state,
        semantic: None,
    });

    for child in &node.children {
        append_node(child, Some(identity.clone()), nodes, node_ids, identities)?;
    }
    Ok(())
}

fn validate_layout(node: &WidgetNode) -> Result<(), FrameSnapshotError> {
    for (field, value) in [
        ("x", node.layout.x),
        ("y", node.layout.y),
        ("width", node.layout.width),
        ("height", node.layout.height),
    ] {
        if !value.is_finite() {
            return Err(FrameSnapshotError::NonFiniteLayout {
                node_id: node.id,
                field,
            });
        }
    }
    Ok(())
}

fn frame_semantic_node(
    semantic: &crate::accessibility::AccessibilityTreeNode,
    identity_by_id: &HashMap<NodeId, StableNodeIdentity>,
) -> Result<FrameSemanticNode, FrameSnapshotError> {
    validate_accessibility(semantic)?;
    let identity_for = |id: NodeId| {
        identity_by_id
            .get(&id)
            .cloned()
            .ok_or(FrameSnapshotError::DanglingSemanticReference {
                node_id: semantic.id,
                referenced_id: id,
            })
    };
    let relationships = &semantic.relationships;
    let map_many = |ids: &[NodeId]| {
        ids.iter()
            .map(|id| identity_for(*id))
            .collect::<Result<Vec<_>, _>>()
            .map(Vec::into_boxed_slice)
    };
    Ok(FrameSemanticNode {
        info: semantic.info.clone(),
        bounds: semantic.bounds,
        parent: semantic.parent.map(identity_for).transpose()?,
        children: map_many(&semantic.children)?,
        relationships: FrameSemanticRelationships {
            labelled_by: map_many(&relationships.labelled_by)?,
            described_by: map_many(&relationships.described_by)?,
            controls: map_many(&relationships.controls)?,
            owns: map_many(&relationships.owns)?,
            details: map_many(&relationships.details)?,
            error_message: map_many(&relationships.error_message)?,
            tooltip_for: relationships.tooltip_for.map(identity_for).transpose()?,
            popover_trigger: relationships
                .popover_trigger
                .map(identity_for)
                .transpose()?,
        },
    })
}

fn validate_accessibility(
    semantic: &crate::accessibility::AccessibilityTreeNode,
) -> Result<(), FrameSnapshotError> {
    for (field, value) in [
        ("value_min", semantic.info.state.value_min),
        ("value_max", semantic.info.state.value_max),
    ] {
        if value.is_some_and(|value| !value.is_finite()) {
            return Err(FrameSnapshotError::NonFiniteAccessibilityValue {
                node_id: semantic.id,
                field,
            });
        }
    }
    if !semantic.bounds.x.is_finite()
        || !semantic.bounds.y.is_finite()
        || !semantic.bounds.width.is_finite()
        || !semantic.bounds.height.is_finite()
    {
        return Err(FrameSnapshotError::NonFiniteLayout {
            node_id: semantic.id,
            field: "semantic bounds",
        });
    }
    Ok(())
}

fn semantic_fields(
    before: Option<&FrameSemanticNode>,
    after: Option<&FrameSemanticNode>,
) -> Vec<SemanticField> {
    let (Some(before), Some(after)) = (before, after) else {
        return (!matches!((before, after), (None, None)))
            .then(|| SemanticField::ALL.to_vec())
            .unwrap_or_default();
    };
    let mut fields = Vec::new();
    if before.parent != after.parent || before.children != after.children {
        fields.push(SemanticField::Tree);
    }
    if before.info.role != after.info.role {
        fields.push(SemanticField::Role);
    }
    if before.info.label != after.info.label {
        fields.push(SemanticField::Name);
    }
    if before.info.description != after.info.description {
        fields.push(SemanticField::Description);
    }
    if before.info.focusable != after.info.focusable {
        fields.push(SemanticField::Focusability);
    }
    if before.info.focused != after.info.focused {
        fields.push(SemanticField::Focus);
    }
    if before.info.hidden != after.info.hidden || before.info.visible != after.info.visible {
        fields.push(SemanticField::Visibility);
    }
    if !accessibility_state_equal(&before.info.state, &after.info.state) {
        fields.push(SemanticField::State);
    }
    if !layout_equal(before.bounds, after.bounds) {
        fields.push(SemanticField::Bounds);
    }
    if before.relationships != after.relationships {
        fields.push(SemanticField::Relationships);
    }
    fields
}

fn layout_equal(left: LayoutRect, right: LayoutRect) -> bool {
    left.x == right.x
        && left.y == right.y
        && left.width == right.width
        && left.height == right.height
}

fn accessibility_state_equal(left: &AccessibilityState, right: &AccessibilityState) -> bool {
    left.disabled == right.disabled
        && left.checked == right.checked
        && left.expanded == right.expanded
        && left.selected == right.selected
        && left.pressed == right.pressed
        && left.busy == right.busy
        && left.invalid == right.invalid
        && left.required == right.required
        && left.value == right.value
        && float_option_equal(left.value_min, right.value_min)
        && float_option_equal(left.value_max, right.value_max)
}

fn float_option_equal(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.to_bits() == right.to_bits(),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accessibility::AccessibilityRole;

    #[derive(Clone, Copy)]
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0
        }
    }

    fn generated_tree(seed: u64) -> WidgetNode {
        let mut random = Lcg(seed.max(1));
        let mut root = WidgetNode::new("box");
        root.id = 1;
        root.set_mesh_key("root");
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 640.0,
            height: 480.0,
        };
        let child_count = (random.next() % 7 + 1) as usize;
        for index in 0..child_count {
            let mut child = WidgetNode::new(if index % 2 == 0 { "button" } else { "text" });
            child.id = 10 + index as u64;
            child.set_mesh_key(format!("root/item-{index}"));
            child.layout = LayoutRect {
                x: (index as f32) * 16.0,
                y: (random.next() % 120) as f32,
                width: 80.0 + (random.next() % 40) as f32,
                height: 24.0,
            };
            if child.tag == "button" {
                child.accessibility.role = AccessibilityRole::Button;
                child.accessibility.focusable = true;
                child
                    .attributes
                    .insert("aria-label".into(), format!("item {index}"));
                child.state.focused = random.next() & 1 == 0;
            } else {
                child
                    .attributes
                    .insert("content".into(), format!("item {index}"));
            }
            if random.next() & 3 == 0 {
                child.attributes.insert("aria-hidden".into(), "true".into());
            }
            root.children.push(child);
        }
        root
    }

    fn assert_snapshot_invariants(snapshot: &FrameSnapshot) {
        assert!(snapshot.phases().is_complete());
        assert_eq!(snapshot.root().parent(), None);
        let identities: HashSet<_> = snapshot
            .nodes()
            .iter()
            .map(|node| node.identity().clone())
            .collect();
        assert_eq!(identities.len(), snapshot.nodes().len());
        for node in snapshot.nodes() {
            assert_eq!(
                snapshot.node(node.identity()).map(FrameNode::id),
                Some(node.id())
            );
            for child in node.children() {
                let child_node = snapshot.node(child).expect("child identity is present");
                assert_eq!(child_node.parent(), Some(node.identity()));
            }
            if let Some(semantic) = node.semantic() {
                for child in &semantic.children {
                    assert!(snapshot.node(child).is_some());
                }
            }
        }
    }

    #[test]
    fn property_generated_trees_preserve_snapshot_invariants() {
        for seed in 1..=256 {
            let tree = generated_tree(seed);
            let snapshot =
                FrameSnapshot::complete(&tree, seed, None).expect("valid generated tree");
            assert_snapshot_invariants(&snapshot);

            // Capturing copies the frame boundary: later live-tree mutation
            // cannot mutate any data previously handed to consumers.
            let root_tag = snapshot.root().tag().to_owned();
            let mut changed = tree;
            changed.tag = "changed-after-capture".into();
            assert_eq!(snapshot.root().tag(), root_tag);
        }
    }

    #[test]
    fn semantic_diff_matches_keyed_nodes_after_reordering() {
        let mut first = generated_tree(1);
        let mut second = first.clone();
        second.children.reverse();
        for (index, child) in second.children.iter_mut().enumerate() {
            child.id = 100 + index as u64;
        }

        first.children[0]
            .attributes
            .insert("aria-hidden".into(), "false".into());
        let before = FrameSnapshot::complete(&first, 1, None).expect("first frame");
        let after = FrameSnapshot::complete(&second, 2, Some(&before)).expect("second frame");
        assert!(after.semantic_diff().added().next().is_none());
        assert!(after.semantic_diff().removed().next().is_none());
        assert!(
            after
                .semantic_diff()
                .updated()
                .any(|change| change.fields.contains(&SemanticField::Tree))
        );

        first.children[0]
            .attributes
            .insert("aria-label".into(), "new label".into());
        first.children[0]
            .attributes
            .insert("aria-hidden".into(), "false".into());
        let relabelled =
            FrameSnapshot::complete(&first, 3, Some(&before)).expect("relabelled frame");
        let change = relabelled
            .semantic_diff()
            .updated()
            .find(|change| change.identity.mesh_key() == Some("root/item-0"))
            .expect("label change is reported");
        assert_eq!(change.fields.as_ref(), &[SemanticField::Name]);
    }

    #[test]
    fn snapshot_rejects_duplicate_identities_and_non_finite_geometry() {
        let mut duplicate = generated_tree(1);
        duplicate.children[1].set_mesh_key("root/item-0");
        assert!(matches!(
            FrameSnapshot::complete(&duplicate, 1, None),
            Err(FrameSnapshotError::DuplicateIdentity { .. })
        ));

        let mut duplicate_id = generated_tree(1);
        duplicate_id.children[1].set_mesh_key("root/item-unique");
        duplicate_id.children[1].id = duplicate_id.children[0].id;
        assert!(matches!(
            FrameSnapshot::complete(&duplicate_id, 1, None),
            Err(FrameSnapshotError::DuplicateNodeId { .. })
        ));

        let mut non_finite = generated_tree(1);
        non_finite.children[0].layout.width = f32::NAN;
        assert!(matches!(
            FrameSnapshot::complete(&non_finite, 1, None),
            Err(FrameSnapshotError::NonFiniteLayout { .. })
        ));
    }

    #[test]
    fn phase_stamps_are_ordered_and_explicit() {
        let partial = FramePhaseStamps::up_to(FramePhase::LaidOut, 7);
        assert_eq!(partial.stamp(FramePhase::TreeBuilt).unwrap().revision(), 7);
        assert_eq!(
            partial.stamp(FramePhase::LaidOut).unwrap().phase(),
            FramePhase::LaidOut
        );
        assert!(partial.stamp(FramePhase::SemanticsReady).is_none());
        assert!(!partial.is_complete());

        let complete = FramePhaseStamps::complete(8);
        assert!(complete.is_complete());
        assert_eq!(
            complete.latest().unwrap().phase(),
            FramePhase::SemanticsReady
        );
    }
}
