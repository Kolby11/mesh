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
use crate::interaction_contract::NodeEligibility;
use crate::layout::LayoutRect;
use crate::style::ComputedStyle;
use crate::tree::{
    ElementState, NodeId, WidgetNode, WidgetTreeValidationError, validate_widget_tree,
};
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
    #[error("unknown element tag <{tag}> on node {node_id}")]
    UnknownElementTag { node_id: NodeId, tag: String },
    #[error(
        "node {node_id} uses source element <{source_tag}> with runtime tag <{runtime_tag}>; expected <{expected_runtime_tag}>"
    )]
    ElementTagMismatch {
        node_id: NodeId,
        source_tag: String,
        runtime_tag: String,
        expected_runtime_tag: String,
    },
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

impl From<WidgetTreeValidationError> for FrameSnapshotError {
    fn from(error: WidgetTreeValidationError) -> Self {
        match error {
            WidgetTreeValidationError::DuplicateNodeId { id } => Self::DuplicateNodeId { id },
            WidgetTreeValidationError::DuplicateMeshKey { key } => Self::DuplicateIdentity {
                identity: StableNodeIdentity::MeshKey(Arc::from(key)),
            },
            WidgetTreeValidationError::UnknownElementTag { node_id, tag } => {
                Self::UnknownElementTag { node_id, tag }
            }
            WidgetTreeValidationError::ElementTagMismatch {
                node_id,
                source_tag,
                runtime_tag,
                expected_runtime_tag,
            } => Self::ElementTagMismatch {
                node_id,
                source_tag,
                runtime_tag,
                expected_runtime_tag,
            },
        }
    }
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
    attributes: Arc<AttributeMap>,
    style: Arc<ComputedStyle>,
    layout: LayoutRect,
    state: ElementState,
    semantic: Option<Arc<FrameSemanticNode>>,
    local_info: Arc<AccessibilityInfo>,
    semantic_text: Arc<str>,
    policy: NodeEligibility,
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
        self.semantic.as_deref()
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
                            semantic_fields(before.semantic.as_deref(), after.semantic.as_deref());
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

/// Persistent balanced sequence. Updating one record copies only its search
/// path; snapshots never clone a full directory of node pointers.
#[derive(Debug, Clone)]
enum FrameRecords {
    Empty,
    Leaf(FrameNode),
    Branch {
        left: Arc<Self>,
        right: Arc<Self>,
        len: usize,
    },
}

impl FrameRecords {
    fn from_nodes(nodes: &mut std::vec::IntoIter<FrameNode>) -> Self {
        Self::take_nodes(nodes, nodes.len())
    }
    fn take_nodes(nodes: &mut std::vec::IntoIter<FrameNode>, len: usize) -> Self {
        match len {
            0 => Self::Empty,
            1 => Self::Leaf(nodes.next().expect("record exists")),
            _ => Self::Branch {
                left: Arc::new(Self::take_nodes(nodes, len / 2)),
                right: Arc::new(Self::take_nodes(nodes, len - len / 2)),
                len,
            },
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Leaf(_) => 1,
            Self::Branch { len, .. } => *len,
        }
    }
    fn get(&self, index: usize) -> &FrameNode {
        match self {
            Self::Leaf(node) if index == 0 => node,
            Self::Branch { left, right, .. } => {
                if index < left.len() {
                    left.get(index)
                } else {
                    right.get(index - left.len())
                }
            }
            _ => panic!("frame node index out of bounds"),
        }
    }
    fn get_mut(&mut self, index: usize) -> &mut FrameNode {
        match self {
            Self::Leaf(node) if index == 0 => node,
            Self::Branch { left, right, .. } => {
                if index < left.len() {
                    Arc::make_mut(left).get_mut(index)
                } else {
                    let offset = left.len();
                    Arc::make_mut(right).get_mut(index - offset)
                }
            }
            _ => panic!("frame node index out of bounds"),
        }
    }
}

/// Borrowed preorder view of a frame's structurally shared records.
#[derive(Clone, Copy)]
pub struct FrameNodes<'a>(&'a FrameRecords);
impl<'a> FrameNodes<'a> {
    pub fn len(self) -> usize {
        self.0.len()
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    pub fn iter(self) -> FrameNodeIter<'a> {
        self.into_iter()
    }
}
impl<'a> IntoIterator for FrameNodes<'a> {
    type Item = &'a FrameNode;
    type IntoIter = FrameNodeIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        FrameNodeIter {
            pending: vec![self.0],
        }
    }
}
pub struct FrameNodeIter<'a> {
    pending: Vec<&'a FrameRecords>,
}
impl<'a> Iterator for FrameNodeIter<'a> {
    type Item = &'a FrameNode;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(records) = self.pending.pop() {
            match records {
                FrameRecords::Empty => {}
                FrameRecords::Leaf(node) => return Some(node),
                FrameRecords::Branch { left, right, .. } => {
                    self.pending.push(right);
                    self.pending.push(left);
                }
            }
        }
        None
    }
}

#[derive(Debug)]
struct FrameSnapshotData {
    revision: u64,
    phases: FramePhaseStamps,
    nodes: Arc<FrameRecords>,
    index: Arc<HashMap<StableNodeIdentity, usize>>,
    id_index: Arc<HashMap<NodeId, usize>>,
    paths: Arc<Vec<Box<[usize]>>>,
    dependents: Arc<HashMap<usize, Vec<usize>>>,
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

        // Validate before building either identity-indexed frame maps or the
        // semantic projection. In particular, accessibility must not observe
        // a tree that the frame later rejects for duplicate or unknown nodes.
        validate_widget_tree(root).map_err(FrameSnapshotError::from)?;
        let mut nodes = Vec::with_capacity(root.node_count());
        let mut node_ids = HashSet::with_capacity(root.node_count());
        let mut identities = HashSet::with_capacity(root.node_count());
        append_node(
            root,
            None,
            NodeEligibility::ROOT,
            &mut nodes,
            &mut node_ids,
            &mut identities,
        )?;
        let accessibility = AccessibilityTree::from_validated_widget_tree(root);

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
            nodes[node_index].semantic =
                Some(Arc::new(frame_semantic_node(semantic, &identity_by_id)?));
        }

        let mut paths = Vec::with_capacity(nodes.len());
        collect_paths(root, &mut Vec::new(), &mut paths);
        let mut dependents: HashMap<usize, Vec<usize>> = HashMap::new();
        for (dependent, node) in nodes.iter().enumerate() {
            if let Some(semantic) = node.semantic() {
                for reference in semantic
                    .relationships
                    .labelled_by
                    .iter()
                    .chain(semantic.relationships.described_by.iter())
                {
                    dependents
                        .entry(index[reference])
                        .or_default()
                        .push(dependent);
                }
            }
        }
        let mut snapshot = Self(Arc::new(FrameSnapshotData {
            revision,
            phases,
            nodes: Arc::new(FrameRecords::from_nodes(&mut nodes.into_iter())),
            index: Arc::new(index),
            id_index: Arc::new(node_index),
            paths: Arc::new(paths),
            dependents: Arc::new(dependents),
            semantic_diff: SemanticDiff::default(),
        }));
        let semantic_diff = SemanticDiff::between(previous, &snapshot);
        Arc::get_mut(&mut snapshot.0)
            .expect("new frame snapshot has the only data owner")
            .semantic_diff = semantic_diff;
        Ok(snapshot)
    }

    /// Capture an authoritative set of changed nodes in a retained tree.
    ///
    /// The caller must include every changed node, including layout propagation
    /// and interaction state. Structural changes require `capture`; changes to
    /// semantic visibility or relationship indexes automatically take that path.
    /// Only dirty records, their name-bearing ancestors, and ARIA text consumers
    /// are projected and diffed. Clean records and all lookup indexes are shared.
    pub fn capture_dirty(
        root: &WidgetNode,
        revision: u64,
        phases: FramePhaseStamps,
        previous: &Self,
        dirty: &HashSet<NodeId>,
    ) -> Result<Self, FrameSnapshotError> {
        if !phases.is_complete() {
            return Err(FrameSnapshotError::IncompletePhaseStamps);
        }
        if revision < previous.revision() {
            return Err(FrameSnapshotError::NonMonotonicRevision {
                revision,
                previous: previous.revision(),
            });
        }
        let fallback = || Self::capture(root, revision, phases, Some(previous));
        if root.id != previous.root().id {
            return fallback();
        }
        let mut affected = BTreeSet::new();
        let mut changed = BTreeSet::new();
        for id in dirty {
            let Some(&index) = previous.0.id_index.get(id) else {
                return fallback();
            };
            let Some((live, policy)) = previous.live_node(root, index) else {
                return fallback();
            };
            let old = previous.0.nodes.get(index);
            if !same_structure_and_references(live, old)
                || policy.is_semantically_visible() != old.policy.is_semantically_visible()
            {
                return fallback();
            }
            validate_layout(live)?;
            changed.insert(index);
            affected.insert(index);
            if policy.is_disabled() != old.policy.is_disabled() {
                previous.collect_descendants(index, &mut affected);
            }
        }
        // Preorder indices place children after their ancestors. Include the
        // complete ancestor closure before recomputing text bottom-up.
        for index in affected.clone() {
            let mut parent = previous.0.nodes.get(index).parent();
            while let Some(identity) = parent {
                let parent_index = previous.0.index[identity];
                affected.insert(parent_index);
                parent = previous.0.nodes.get(parent_index).parent();
            }
        }
        let mut nodes = previous.0.nodes.clone();
        for &index in affected.iter().rev() {
            let Some((live, policy)) = previous.live_node(root, index) else {
                return fallback();
            };
            let old = previous.0.nodes.get(index);
            if !same_structure_and_references(live, old)
                || policy.is_semantically_visible() != old.policy.is_semantically_visible()
            {
                return fallback();
            }
            validate_layout(live)?;
            let child_text = old
                .children
                .iter()
                .map(|identity| nodes.get(previous.0.index[identity]).semantic_text.as_ref())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let (info, text) = crate::accessibility::frame_local_info(live, &child_text, policy);
            let node = Arc::make_mut(&mut nodes).get_mut(index);
            if changed.contains(&index) {
                node.attributes = Arc::new(live.attributes.clone());
                node.style = Arc::new(live.computed_style.clone());
                node.layout = live.layout;
                node.state = live.state;
            }
            node.local_info = Arc::new(info);
            node.semantic_text = Arc::from(text);
            node.policy = policy;
        }
        let mut projected = affected.clone();
        for index in &affected {
            if let Some(dependents) = previous.0.dependents.get(index) {
                projected.extend(dependents.iter().copied());
            }
        }
        let mut changes = Vec::new();
        for index in projected {
            let old = previous.0.nodes.get(index);
            let node = nodes.get(index);
            let semantic = old.semantic.as_ref().map(|old_semantic| {
                let mut semantic = (**old_semantic).clone();
                semantic.info = (*node.local_info).clone();
                semantic.bounds = node.layout;
                apply_frame_reference_text(&mut semantic, &nodes, &previous.0.index);
                semantic
            });
            if let Some(semantic) = &semantic {
                validate_frame_semantic(node.id, semantic)?;
            }
            let fields = semantic_fields(old.semantic(), semantic.as_ref());
            if !fields.is_empty() {
                changes.push(SemanticChange {
                    identity: old.identity.clone(),
                    kind: SemanticChangeKind::Updated,
                    fields: fields.into_boxed_slice(),
                });
            }
            Arc::make_mut(&mut nodes).get_mut(index).semantic = semantic.map(Arc::new);
        }
        changes.sort_by(|left, right| left.identity.cmp(&right.identity));
        Ok(Self(Arc::new(FrameSnapshotData {
            revision,
            phases,
            nodes,
            index: previous.0.index.clone(),
            id_index: previous.0.id_index.clone(),
            paths: previous.0.paths.clone(),
            dependents: previous.0.dependents.clone(),
            semantic_diff: SemanticDiff {
                changes: changes.into_boxed_slice(),
            },
        })))
    }

    /// Normalize changed retained nodes and their name-bearing ancestors.
    /// Cached child text avoids revisiting unrelated branches. Callers must use
    /// full normalization after structural changes. `dirty_subtrees` includes
    /// descendants restyled through inheritance; `dirty_nodes` contains local
    /// runtime changes. Returns the normalized IDs, or `None` after a full
    /// fallback when the retained topology cannot be reused.
    pub fn normalize_accessibility_dirty(
        &self,
        root: &mut WidgetNode,
        dirty_nodes: &HashSet<NodeId>,
        dirty_subtrees: &HashSet<NodeId>,
    ) -> Option<HashSet<NodeId>> {
        let mut affected = BTreeSet::new();
        for id in dirty_nodes.iter().chain(dirty_subtrees) {
            let Some(&index) = self.0.id_index.get(id) else {
                crate::normalize_accessibility(root);
                return None;
            };
            affected.insert(index);
            let Some((_, policy)) = self.live_node(root, index) else {
                crate::normalize_accessibility(root);
                return None;
            };
            let old = self.0.nodes.get(index).policy;
            if dirty_subtrees.contains(id)
                || policy.is_disabled() != old.is_disabled()
                || policy.is_semantically_visible() != old.is_semantically_visible()
            {
                self.collect_descendants(index, &mut affected);
            }
        }
        for index in affected.clone() {
            let mut parent = self.0.nodes.get(index).parent();
            while let Some(identity) = parent {
                let index = self.0.index[identity];
                affected.insert(index);
                parent = self.0.nodes.get(index).parent();
            }
        }
        // Check every path before writing: a stale structural index must never
        // leave the live tree partially normalized.
        if affected.iter().any(|&index| {
            self.live_node(root, index).is_none_or(|(node, _)| {
                !same_structure_and_references(node, self.0.nodes.get(index))
            })
        }) {
            crate::normalize_accessibility(root);
            return None;
        }
        let mut text = HashMap::<usize, String>::new();
        for &index in affected.iter().rev() {
            let (node, policy) = self.live_node(root, index).expect("validated path");
            let child_text = self
                .0
                .nodes
                .get(index)
                .children
                .iter()
                .map(|identity| {
                    let child = self.0.index[identity];
                    text.get(&child)
                        .map(String::as_str)
                        .unwrap_or(self.0.nodes.get(child).semantic_text.as_ref())
                })
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let (info, visible_text) =
                crate::accessibility::frame_local_info(node, &child_text, policy);
            text.insert(index, visible_text);
            let mut node = &mut *root;
            for &child in self.0.paths[index].iter() {
                node = &mut node.children[child];
            }
            if node.accessibility_baseline().is_none() {
                node.set_accessibility_baseline(node.accessibility.clone());
            }
            node.accessibility = info;
        }
        Some(
            affected
                .into_iter()
                .map(|index| self.0.nodes.get(index).id)
                .collect(),
        )
    }

    fn live_node<'a>(
        &self,
        root: &'a WidgetNode,
        index: usize,
    ) -> Option<(&'a WidgetNode, NodeEligibility)> {
        let mut node = root;
        let mut policy = NodeEligibility::for_root(root);
        for &child in self.0.paths[index].iter() {
            node = node.children.get(child)?;
            policy = policy.child(node);
        }
        (node.id == self.0.nodes.get(index).id).then_some((node, policy))
    }

    fn collect_descendants(&self, index: usize, out: &mut BTreeSet<usize>) {
        for identity in self.0.nodes.get(index).children.iter() {
            let child = self.0.index[identity];
            out.insert(child);
            self.collect_descendants(child, out);
        }
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

    pub fn nodes(&self) -> FrameNodes<'_> {
        FrameNodes(&self.0.nodes)
    }

    pub fn root(&self) -> &FrameNode {
        self.0.nodes.get(0)
    }

    pub fn node(&self, identity: &StableNodeIdentity) -> Option<&FrameNode> {
        self.0
            .index
            .get(identity)
            .map(|index| self.0.nodes.get(*index))
    }

    pub fn node_by_id(&self, id: NodeId) -> Option<&FrameNode> {
        self.0
            .id_index
            .get(&id)
            .map(|index| self.0.nodes.get(*index))
    }

    pub fn semantic_nodes(&self) -> impl Iterator<Item = &FrameNode> {
        self.nodes()
            .into_iter()
            .filter(|node| node.semantic.is_some())
    }

    pub fn semantic_diff(&self) -> &SemanticDiff {
        &self.0.semantic_diff
    }
}

fn same_structure_and_references(live: &WidgetNode, old: &FrameNode) -> bool {
    live.tag == old.tag.as_ref()
        && StableNodeIdentity::from_node(live) == old.identity
        && live.children.len() == old.children.len()
        && live
            .children
            .iter()
            .zip(old.children.iter())
            .all(|(child, identity)| StableNodeIdentity::from_node(child) == *identity)
        && [
            "data-mesh-element",
            "id",
            "ref",
            "_mesh_bind_this",
            "aria-labelledby",
            "aria-describedby",
            "aria-controls",
            "aria-owns",
            "aria-details",
            "aria-errormessage",
            "tooltip-for",
            "anchor-ref",
            "anchor-target",
            "anchor-element",
            "target",
        ]
        .iter()
        .all(|name| live.attributes.get(name) == old.attributes.get(name))
}

fn apply_frame_reference_text(
    semantic: &mut FrameSemanticNode,
    nodes: &FrameRecords,
    index: &HashMap<StableNodeIdentity, usize>,
) {
    let resolve = |references: &[StableNodeIdentity], description: bool| {
        references
            .iter()
            .filter_map(|identity| {
                let node = nodes.get(index[identity]);
                let info = &node.local_info;
                let text = if description {
                    info.description.as_deref().or(info.label.as_deref())
                } else {
                    info.label.as_deref()
                }
                .unwrap_or(&node.semantic_text);
                (!text.trim().is_empty()).then_some(text)
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let labels = resolve(&semantic.relationships.labelled_by, false);
    if !labels.is_empty() {
        semantic.info.label = Some(labels);
    }
    let descriptions = resolve(&semantic.relationships.described_by, true);
    if !descriptions.is_empty() {
        semantic.info.description = Some(match semantic.info.description.take() {
            Some(existing) if !existing.trim().is_empty() => format!("{existing} {descriptions}"),
            _ => descriptions,
        });
    }
}

fn validate_frame_semantic(
    id: NodeId,
    semantic: &FrameSemanticNode,
) -> Result<(), FrameSnapshotError> {
    for (field, value) in [
        ("value_min", semantic.info.state.value_min),
        ("value_max", semantic.info.state.value_max),
    ] {
        if value.is_some_and(|value| !value.is_finite()) {
            return Err(FrameSnapshotError::NonFiniteAccessibilityValue { node_id: id, field });
        }
    }
    Ok(())
}

fn append_node(
    node: &WidgetNode,
    parent: Option<StableNodeIdentity>,
    parent_policy: NodeEligibility,
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
    let policy = parent_policy.child(node);
    let position = nodes.len();
    nodes.push(FrameNode {
        identity: identity.clone(),
        id: node.id,
        parent,
        children,
        tag: Arc::from(node.tag.as_str()),
        runtime_tag: element_runtime_tag_for_tag(&node.tag).map(Arc::from),
        attributes: Arc::new(node.attributes.clone()),
        style: Arc::new(node.computed_style.clone()),
        layout: node.layout,
        state: node.state,
        semantic: None,
        local_info: Arc::new(node.accessibility.clone()),
        semantic_text: Arc::from(""),
        policy,
    });

    for child in &node.children {
        append_node(
            child,
            Some(identity.clone()),
            policy,
            nodes,
            node_ids,
            identities,
        )?;
    }
    let child_text = nodes[position + 1..]
        .iter()
        .filter(|child| child.parent.as_ref() == Some(&identity))
        .map(|child| child.semantic_text.as_ref())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let (info, text) = crate::accessibility::frame_local_info(node, &child_text, policy);
    nodes[position].local_info = Arc::new(info);
    nodes[position].semantic_text = Arc::from(text);
    Ok(())
}

fn collect_paths(node: &WidgetNode, path: &mut Vec<usize>, paths: &mut Vec<Box<[usize]>>) {
    paths.push(path.clone().into_boxed_slice());
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        collect_paths(child, path, paths);
        path.pop();
    }
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
    fn snapshot_rejects_unknown_element_tags_before_semantic_projection() {
        let root = WidgetNode::new("not-an-element");

        assert!(matches!(
            FrameSnapshot::capture(&root, 1, FramePhaseStamps::complete(1), None),
            Err(FrameSnapshotError::UnknownElementTag { ref tag, .. })
                if tag == "not-an-element"
        ));
    }

    /// `capture_dirty` shares clean records and reprojects only the dirty ones,
    /// their name-bearing ancestors, and their ARIA text consumers. Its result
    /// must be indistinguishable from a full capture of the same tree — the
    /// same nodes, the same semantics, and the same semantic diff.
    #[test]
    fn dirty_capture_matches_a_full_capture() {
        fn assert_equivalent(dirty: &FrameSnapshot, full: &FrameSnapshot, case: &str) {
            assert_eq!(
                dirty.nodes().len(),
                full.nodes().len(),
                "node count: {case}"
            );
            for (left, right) in dirty.nodes().iter().zip(full.nodes()) {
                assert_eq!(left.identity(), right.identity(), "identity: {case}");
                // These projections carry no `PartialEq`, and their debug form
                // is the complete field set, so it is the comparison.
                assert_eq!(
                    format!("{:?}", left.layout()),
                    format!("{:?}", right.layout()),
                    "layout: {case}"
                );
                assert_eq!(
                    format!("{:?}", left.state()),
                    format!("{:?}", right.state()),
                    "state: {case}"
                );
                assert_eq!(
                    format!("{:?}", left.semantic()),
                    format!("{:?}", right.semantic()),
                    "semantics for {:?}: {case}",
                    left.identity()
                );
            }
            assert_eq!(
                dirty.semantic_diff().changes(),
                full.semantic_diff().changes(),
                "semantic diff: {case}"
            );
        }

        for seed in 1..12u64 {
            let mut root = generated_tree(seed);
            // A label relationship so the reprojection of ARIA text consumers
            // is exercised, not just the changed node itself.
            let labeller = root.children[0].mesh_key().unwrap().to_string();
            if root.children.len() > 1 {
                root.children[1]
                    .attributes
                    .insert("aria-labelledby".into(), labeller);
            }
            let base = FrameSnapshot::complete(&root, 1, None).expect("base snapshot");

            for (revision, case) in [(2u64, "label"), (3, "layout"), (4, "state")] {
                let target = &mut root.children[0];
                match case {
                    "label" => {
                        target
                            .attributes
                            .insert("aria-label".into(), format!("renamed {revision}"));
                    }
                    "layout" => target.layout.width += 7.0,
                    _ => target.state.hovered = !target.state.hovered,
                }
                let changed = HashSet::from([target.id]);

                let dirty = FrameSnapshot::capture_dirty(
                    &root,
                    revision,
                    FramePhaseStamps::complete(revision),
                    &base,
                    &changed,
                )
                .unwrap_or_else(|error| panic!("dirty capture for {case}: {error}"));
                let full = FrameSnapshot::capture(
                    &root,
                    revision,
                    FramePhaseStamps::complete(revision),
                    Some(&base),
                )
                .expect("full capture");
                assert_equivalent(&dirty, &full, &format!("seed {seed}, {case}"));
            }
        }
    }

    #[test]
    fn dirty_normalization_matches_full_including_inherited_policy_and_names() {
        fn compare(left: &WidgetNode, right: &WidgetNode) {
            assert_eq!(
                format!("{:?}", left.accessibility),
                format!("{:?}", right.accessibility),
                "node {}",
                left.id
            );
            for (left, right) in left.children.iter().zip(&right.children) {
                compare(left, right);
            }
        }
        for seed in 1..12 {
            let mut original = generated_tree(seed);
            original.children[0].attributes.remove("aria-label");
            original.children[0].attributes.remove("aria-hidden");
            let mut label = WidgetNode::new("text");
            label.id = 500;
            label.set_mesh_key("root/item-0/label");
            label
                .attributes
                .insert("content".into(), "descendant name".into());
            original.children[0].children.push(label);
            crate::normalize_accessibility(&mut original);
            let previous = FrameSnapshot::complete(&original, 1, None).unwrap();
            for case in [
                "text",
                "hidden",
                "disabled",
                "focus",
                "restyle",
                "structural",
            ] {
                let mut root = original.clone();
                let target = &mut root.children[0];
                let id = if case == "text" {
                    target.children[0].id
                } else {
                    target.id
                };
                match case {
                    "text" => {
                        target.children[0]
                            .attributes
                            .insert("content".into(), "replacement".into());
                    }
                    "hidden" => {
                        target
                            .attributes
                            .insert("aria-hidden".into(), "true".into());
                    }
                    "disabled" => {
                        target.attributes.insert("disabled".into(), "true".into());
                    }
                    "focus" => {
                        target.state.focused = true;
                    }
                    "restyle" => {
                        target.children[0].computed_style.display = crate::style::Display::None;
                    }
                    _ => {
                        target.children.push(WidgetNode::new("text"));
                    }
                }
                let mut full = root.clone();
                crate::normalize_accessibility(&mut full);
                previous.normalize_accessibility_dirty(
                    &mut root,
                    &HashSet::from([id]),
                    &if case == "restyle" {
                        HashSet::from([id])
                    } else {
                        HashSet::new()
                    },
                );
                compare(&root, &full);
            }
        }
    }

    #[test]
    #[ignore = "release-only semantic finalization benchmark"]
    fn dirty_semantic_finalization_benchmark() {
        let mut tree = WidgetNode::new("column");
        for index in 0..1024 {
            let mut label = WidgetNode::new("text");
            label
                .attributes
                .insert("content".into(), format!("label {index}"));
            tree.children.push(label);
        }
        crate::normalize_accessibility(&mut tree);
        let base = FrameSnapshot::complete(&tree, 1, None).unwrap();
        let id = tree.children[512].id;
        let dirty = HashSet::from([id]);
        let mut full_tree = tree.clone();
        let start = std::time::Instant::now();
        for revision in 2..102 {
            full_tree.children[512].state.focused = revision % 2 == 0;
            crate::normalize_accessibility(&mut full_tree);
            std::hint::black_box(
                FrameSnapshot::complete(&full_tree, revision, Some(&base)).unwrap(),
            );
        }
        let full = start.elapsed();
        let start = std::time::Instant::now();
        for revision in 2..102 {
            tree.children[512].state.focused = revision % 2 == 0;
            base.normalize_accessibility_dirty(&mut tree, &dirty, &HashSet::new());
            std::hint::black_box(
                FrameSnapshot::capture_dirty(
                    &tree,
                    revision,
                    FramePhaseStamps::complete(revision),
                    &base,
                    &dirty,
                )
                .unwrap(),
            );
        }
        let scoped = start.elapsed();
        eprintln!(
            "semantic finalization: 1025 nodes, 100 leaf-focus frames: full {full:?}, scoped {scoped:?}"
        );
    }

    /// Structural edits are outside the dirty contract and must take the full
    /// capture path rather than patching stale records.
    #[test]
    fn dirty_capture_falls_back_for_structural_change() {
        let mut root = generated_tree(3);
        let base = FrameSnapshot::complete(&root, 1, None).expect("base snapshot");

        let mut added = WidgetNode::new("text");
        added.id = 900;
        added.set_mesh_key("root/added");
        added.layout = LayoutRect {
            x: 0.0,
            y: 400.0,
            width: 40.0,
            height: 16.0,
        };
        root.children.push(added);

        let dirty = FrameSnapshot::capture_dirty(
            &root,
            2,
            FramePhaseStamps::complete(2),
            &base,
            &HashSet::from([root.id]),
        )
        .expect("structural change falls back rather than failing");
        let full = FrameSnapshot::capture(&root, 2, FramePhaseStamps::complete(2), Some(&base))
            .expect("full capture");
        assert_eq!(dirty.nodes().len(), full.nodes().len());
        assert_eq!(dirty.nodes().len(), base.nodes().len() + 1);
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
