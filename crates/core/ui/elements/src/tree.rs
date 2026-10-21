//! Widget tree — the live, evaluated UI structure.
use crate::Prop;
use crate::accessibility::AccessibilityInfo;
use crate::attributes::AttributeMap;
use crate::composition::HandlerTarget;
use crate::layout::LayoutRect;
use crate::style::ComputedStyle;
use std::collections::{BTreeMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

bitflags::bitflags! {
    /// Live node state flags, written by markup, the interaction runtime and
    /// the shell, and read by pseudo-class selectors like `:hover`.
    ///
    /// The CSS-visible bits equal the `bit` of their row in
    /// [`PSEUDO_STATE_TABLE`](crate::PSEUDO_STATE_TABLE), so a node's selector
    /// state mask is `state & NodeState::CSS`. The surface bits are the
    /// compositor's `xdg_toplevel` states projected onto every node: this
    /// engine has no descendant combinators, so a sidebar cannot write
    /// `.window:fullscreen .sidebar` but can write `.sidebar:fullscreen`. They
    /// are always clear on layer surfaces and popups.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct NodeState: u32 {
        const HOVERED = 1 << 0;
        const FOCUSED = 1 << 1;
        const ACTIVE = 1 << 2;
        const DISABLED = 1 << 3;
        const READ_ONLY = 1 << 4;
        const REQUIRED = 1 << 5;
        const SELECTED = 1 << 6;
        const CHECKED = 1 << 7;
        const EXPANDED = 1 << 8;
        const PRESSED = 1 << 9;
        const INVALID = 1 << 10;
        const VALUE = 1 << 11;
        const FOCUS_VISIBLE = 1 << 12;
        const FULLSCREEN = 1 << 13;
        const MAXIMIZED = 1 << 14;
        const ACTIVATED = 1 << 15;
        const TILED = 1 << 16;
        const WINDOWED = 1 << 17;
        /// The node's surface or promoted child is mapping this paint; its
        /// first buffer snaps to the authored entrance styles. Not CSS state.
        const SURFACE_ENTERING = 1 << 24;
        /// The node's surface or promoted child is closing. Not CSS state.
        const SURFACE_EXITING = 1 << 25;

        /// Every pseudo-class bit selectors can match.
        const CSS = (1 << 18) - 1;
        const WINDOW = Self::FULLSCREEN.bits()
            | Self::MAXIMIZED.bits()
            | Self::ACTIVATED.bits()
            | Self::TILED.bits()
            | Self::WINDOWED.bits();
    }
}

impl NodeState {
    pub fn window(self) -> WindowSurfaceState {
        WindowSurfaceState {
            windowed: self.contains(Self::WINDOWED),
            fullscreen: self.contains(Self::FULLSCREEN),
            maximized: self.contains(Self::MAXIMIZED),
            activated: self.contains(Self::ACTIVATED),
            tiled: self.contains(Self::TILED),
        }
    }

    pub fn set_window(&mut self, window: WindowSurfaceState) {
        self.remove(Self::WINDOW);
        self.insert(window.flags());
    }
}

/// The compositor's view of the containing toplevel, as CSS state on every
/// node of the surface's tree. See [`NodeState`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowSurfaceState {
    /// `:windowed` — realized as an `xdg_toplevel` rather than shell chrome.
    /// MESH's own decision, unlike the compositor states below, which are only
    /// ever true when this is. Components restyle against it to draw window
    /// chrome (a "dock back" control instead of "pop out").
    pub windowed: bool,
    /// `:fullscreen` — the window covers a whole output.
    pub fullscreen: bool,
    /// `:maximized` — the window fills its work area.
    pub maximized: bool,
    /// `:activated` — the compositor considers the window focused.
    pub activated: bool,
    /// `:tiled` — some edge abuts a neighbour or screen edge.
    pub tiled: bool,
}

impl WindowSurfaceState {
    pub fn flags(self) -> NodeState {
        let mut flags = NodeState::empty();
        flags.set(NodeState::WINDOWED, self.windowed);
        flags.set(NodeState::FULLSCREEN, self.fullscreen);
        flags.set(NodeState::MAXIMIZED, self.maximized);
        flags.set(NodeState::ACTIVATED, self.activated);
        flags.set(NodeState::TILED, self.tiled);
        flags
    }
}

pub type NodeId = u64;

static NEXT_NODE_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_node_id() -> NodeId {
    NEXT_NODE_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("MESH exhausted the runtime NodeId space")
}

/// A structural invariant violation discovered before a widget tree is handed
/// to a phase that builds identity-indexed maps or publishes projections.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WidgetTreeValidationError {
    #[error("duplicate runtime node id {id}")]
    DuplicateNodeId { id: NodeId },
    #[error("duplicate stable mesh key '{key}'")]
    DuplicateMeshKey { key: String },
    #[error("node {node_id} uses unknown element tag <{tag}>")]
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
}

/// Validate the identity and element-definition invariants of a live tree.
///
/// `WidgetNode::Clone` intentionally preserves ids so memoized subtrees can
/// retain their identity. Reusing such a subtree twice is therefore invalid;
/// this traversal is the single shared guard before layout, input, and
/// semantic projections build identity-indexed state.
pub fn validate_widget_tree(root: &WidgetNode) -> Result<(), WidgetTreeValidationError> {
    fn visit(
        node: &WidgetNode,
        node_ids: &mut HashSet<NodeId>,
        mesh_keys: &mut HashSet<String>,
    ) -> Result<(), WidgetTreeValidationError> {
        crate::frame_work::visit(crate::frame_work::Pass::Validation);
        if !node_ids.insert(node.id) {
            return Err(WidgetTreeValidationError::DuplicateNodeId { id: node.id });
        }

        if let Some(key) = node.mesh_key().filter(|key| !key.is_empty())
            && !mesh_keys.insert(key.to_owned())
        {
            return Err(WidgetTreeValidationError::DuplicateMeshKey {
                key: key.to_owned(),
            });
        }

        let source_tag = node
            .attributes
            .get(Prop::DataMeshElement)
            .map(String::as_str);
        let definition_tag = source_tag.unwrap_or(node.tag.as_str());
        let Some(contract) = crate::element::element_contract_for_tag(definition_tag) else {
            return Err(WidgetTreeValidationError::UnknownElementTag {
                node_id: node.id,
                tag: definition_tag.to_owned(),
            });
        };

        if let Some(source_tag) = source_tag
            && contract.runtime_tag != node.tag
        {
            return Err(WidgetTreeValidationError::ElementTagMismatch {
                node_id: node.id,
                source_tag: source_tag.to_owned(),
                runtime_tag: node.tag.clone(),
                expected_runtime_tag: contract.runtime_tag.to_owned(),
            });
        }

        for child in &node.children {
            visit(child, node_ids, mesh_keys)?;
        }
        Ok(())
    }

    visit(root, &mut HashSet::new(), &mut HashSet::new())
}

/// Pre-bound handler call from markup like `onclick={handler(arg)}`, kept out
/// of `event_handlers` so arguments need no JSON reparse at dispatch.
#[derive(Debug, Clone, PartialEq)]
pub struct EventHandlerCall {
    pub handler: HandlerTarget,
    pub args: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WidgetScrollMetrics {
    pub x: f32,
    pub y: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub content_width: f32,
    pub content_height: f32,
}

/// Copy-on-write child topology shared by memo entries and live trees. A
/// mutable walk copies only the immediate child overlays; authored payloads and
/// descendants stay shared until that level is itself mutated.
#[derive(Clone, Default)]
pub struct SharedWidgetChildren(Arc<Vec<WidgetNode>>);

impl SharedWidgetChildren {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(Arc::new(Vec::with_capacity(capacity)))
    }

    #[doc(hidden)]
    pub fn shares_allocation_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for SharedWidgetChildren {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Deref for SharedWidgetChildren {
    type Target = Vec<WidgetNode>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SharedWidgetChildren {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

impl From<Vec<WidgetNode>> for SharedWidgetChildren {
    fn from(children: Vec<WidgetNode>) -> Self {
        Self(Arc::new(children))
    }
}

impl FromIterator<WidgetNode> for SharedWidgetChildren {
    fn from_iter<T: IntoIterator<Item = WidgetNode>>(iter: T) -> Self {
        Vec::from_iter(iter).into()
    }
}

impl Extend<WidgetNode> for SharedWidgetChildren {
    fn extend<T: IntoIterator<Item = WidgetNode>>(&mut self, iter: T) {
        self.deref_mut().extend(iter);
    }
}

impl<'a> IntoIterator for &'a SharedWidgetChildren {
    type Item = &'a WidgetNode;
    type IntoIter = std::slice::Iter<'a, WidgetNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut SharedWidgetChildren {
    type Item = &'a mut WidgetNode;
    type IntoIter = std::slice::IterMut<'a, WidgetNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl IntoIterator for SharedWidgetChildren {
    type Item = WidgetNode;
    type IntoIter = std::vec::IntoIter<WidgetNode>;

    fn into_iter(self) -> Self::IntoIter {
        match Arc::try_unwrap(self.0) {
            Ok(children) => children.into_iter(),
            Err(children) => children.as_ref().clone().into_iter(),
        }
    }
}

/// Immutable template/build payload shared by memo entries and live nodes.
/// Public fields reach callers through `WidgetNode`'s `Deref`; mutation is
/// copy-on-write and normally touches only the nodes that actually change.
#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct WidgetNodeAuthored {
    pub tag: String,
    pub attributes: AttributeMap,
    pub event_handlers: BTreeMap<String, HandlerTarget>,
    pub event_handler_calls: BTreeMap<String, EventHandlerCall>,
    /// Static semantic metadata supplied by the compiler or an embedding
    /// caller. The live `WidgetNode::accessibility` value is a normalized
    /// projection and must not become the source for the next normalization
    /// pass, or removed ARIA attributes can leave stale values behind.
    accessibility_baseline: Option<AccessibilityInfo>,
    module_id: Option<Arc<str>>,
    pub service_field_reads: Vec<(String, String)>,
    /// Where this node's bound properties come from, so a script write can
    /// re-evaluate exactly them instead of rebuilding the tree.
    template_bindings: Option<Arc<TemplateBindings>>,
    /// Shell-owned composition flags that must survive memoized subtree reuse.
    composition: WidgetCompositionMetadata,
    /// The script variable a `bind:this={name}` binding publishes this node as.
    bind_this: Option<Arc<str>>,
    /// The script variable a disclosure's `open` is two-way bound to.
    open_binding: Option<OpenBinding>,
    /// The `.mesh` source file that authored this node, for inspection.
    source_file: Option<Arc<str>>,
    /// The module that contributed this node into a slot.
    slot_source: Option<Arc<str>>,
}

/// A disclosure's `open={variable}` binding, written back when the shell
/// opens or closes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenBinding {
    pub variable: Arc<str>,
    /// The component instance whose script owns `variable`; `None` for the
    /// surface root's script.
    pub owner: Option<Arc<str>>,
}

/// The IME preedit spliced into an input's displayed value, as byte offsets
/// into that value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct InputPreedit {
    pub start: usize,
    pub end: usize,
    pub cursor_begin: usize,
    pub cursor_end: usize,
}

/// A text selection projected onto the selectable `<text>` that owns its
/// anchor. Points are surface coordinates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextSelection {
    /// Theme colour values, parsed by the painter.
    pub background: Arc<str>,
    pub foreground: Arc<str>,
    pub anchor: (f32, f32),
    pub focus: (f32, f32),
    /// The text origin the points were measured against.
    pub text_origin: (f32, f32),
}

impl std::hash::Hash for TextSelection {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        self.background.hash(hasher);
        self.foreground.hash(hasher);
        for (x, y) in [self.anchor, self.focus, self.text_origin] {
            x.to_bits().hash(hasher);
            y.to_bits().hash(hasher);
        }
    }
}

/// The template sources of an element's bound properties, recorded at build
/// time, including captured locals in keyed `{#for}` bodies. Re-evaluating them against the
/// owning instance's script state reproduces exactly what a rebuild would
/// write into these properties.
#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct TemplateBindings {
    /// The component instance whose script state the expressions read.
    pub instance_key: Arc<str>,
    pub locals: Option<Arc<serde_json::Map<String, serde_json::Value>>>,
    pub keyed_item: Option<KeyedTemplateItem>,
    /// `(attribute, expression)` for every non-handler `name={expression}`.
    pub attributes: Vec<(String, String)>,
    /// The inline children of a `<text>` whose `content` they produce.
    pub inline_content: Option<Arc<[mesh_core_component::TemplateNode]>>,
}

impl TemplateBindings {
    /// Every expression source these bindings evaluate.
    pub fn expressions(&self) -> impl Iterator<Item = &str> {
        self.attributes
            .iter()
            .map(|(_, expression)| expression.as_str())
            .chain(
                self.inline_content
                    .iter()
                    .flat_map(|parts| parts.iter())
                    .filter_map(|part| match part {
                        mesh_core_component::TemplateNode::Expr(expr) => {
                            Some(expr.expression.as_str())
                        }
                        _ => None,
                    }),
            )
    }
}

/// A fixed-shape native loop's source and last published ordered keys.
#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct KeyedTemplateList {
    pub source: String,
    pub item_name: String,
    pub key: String,
    pub valid_keys: bool,
    pub keys: Arc<[serde_json::Value]>,
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct KeyedTemplateItem {
    pub list: Arc<KeyedTemplateList>,
    pub index: usize,
}

/// Typed metadata added while component trees are composed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WidgetCompositionMetadata {
    pub promoted_popover: bool,
    pub promoted_window: bool,
    /// Generated component-error content; the shell constrains its layout.
    pub error_placeholder: bool,
}

/// A node produced by evaluating a template against script state.
#[derive(Debug, Clone)]
pub struct WidgetNode {
    pub id: NodeId,
    pub computed_style: ComputedStyle,
    pub layout: LayoutRect,
    pub children: SharedWidgetChildren,
    pub accessibility: AccessibilityInfo,
    pub state: NodeState,
    /// Typed runtime scroll state, kept out of the string attribute map.
    pub scroll_metrics: Option<WidgetScrollMetrics>,
    /// Set by the shell on the focused input while an IME composes.
    pub input_preedit: Option<InputPreedit>,
    /// Set by the shell on the selectable text holding the selection anchor.
    pub text_selection: Option<Arc<TextSelection>>,
    /// Stable runtime identity for this node, kept out of the string attribute map.
    mesh_key: Option<String>,
    /// Consumed while assigning shell runtime keys; never exposed to styles.
    loop_identity: Option<Arc<str>>,
    /// Cached split `class` tokens derived from the raw `class` attribute.
    cached_class_attr: Option<String>,
    cached_classes: Vec<String>,
    /// The first authored root of a top-level surface. Its CSS margins are
    /// lowered to the compositor surface rather than consumed by Taffy's
    /// synthetic wrapper.
    surface_root: bool,
    pub(crate) finalized_style: Option<Arc<crate::style::FinalizedStyle>>,
    pub(crate) finalized_accessibility: Option<Arc<crate::accessibility::FinalizedAccessibility>>,
    pub(crate) finalized_style_valid: bool,
    pub(crate) finalized_style_pristine: bool,
    pub(crate) finalized_accessibility_valid: bool,
    memo_instance: Option<Arc<str>>,
    authored: Arc<WidgetNodeAuthored>,
}

impl Deref for WidgetNode {
    type Target = WidgetNodeAuthored;

    fn deref(&self) -> &Self::Target {
        &self.authored
    }
}

impl DerefMut for WidgetNode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.authored)
    }
}

/// Child indices in paint order, or `None` when that is the authored order.
///
/// Paint sorts siblings by ascending `z-index` and keeps authored order for
/// equal values; the last child is painted on top. Every pointer walk must
/// visit children in the reverse of this order so the child it hits is the
/// one drawn on top.
pub fn paint_order_child_indices(node: &WidgetNode) -> Option<Vec<usize>> {
    let children = &node.children;
    if children
        .windows(2)
        .all(|pair| pair[0].computed_style.z_index <= pair[1].computed_style.z_index)
    {
        return None;
    }
    let mut order = (0..children.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| children[index].computed_style.z_index);
    Some(order)
}

/// Children in paint order, bottom first. Iterate it in reverse for hit
/// testing; see [`paint_order_child_indices`].
pub fn children_in_paint_order(node: &WidgetNode) -> PaintOrderChildren<'_> {
    match paint_order_child_indices(node) {
        None => PaintOrderChildren::Authored(node.children.iter()),
        Some(order) => PaintOrderChildren::Sorted {
            children: &node.children,
            order: order.into_iter(),
        },
    }
}

pub enum PaintOrderChildren<'a> {
    Authored(std::slice::Iter<'a, WidgetNode>),
    Sorted {
        children: &'a [WidgetNode],
        order: std::vec::IntoIter<usize>,
    },
}

impl<'a> Iterator for PaintOrderChildren<'a> {
    type Item = &'a WidgetNode;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Authored(children) => children.next(),
            Self::Sorted { children, order } => order.next().map(|index| &children[index]),
        }
    }
}

impl DoubleEndedIterator for PaintOrderChildren<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Authored(children) => children.next_back(),
            Self::Sorted { children, order } => order.next_back().map(|index| &children[index]),
        }
    }
}

impl WidgetNode {
    /// Explicitly closed disclosure containers take no space and expose no targets.
    /// An omitted `open` preserves always-visible structural containers.
    pub fn is_closed_container(&self) -> bool {
        let source = self
            .attributes
            .get(Prop::DataMeshElement)
            .map(String::as_str)
            .unwrap_or(self.tag.as_str());
        matches!(source, "dialog" | "sheet" | "menu")
            && self.attributes.get(Prop::Open).is_some_and(|value| {
                let value = value.trim();
                value.is_empty()
                    || value == "0"
                    || value.eq_ignore_ascii_case("false")
                    || value.eq_ignore_ascii_case("none")
            })
    }

    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            id: next_node_id(),
            computed_style: ComputedStyle::default(),
            layout: LayoutRect::default(),
            children: SharedWidgetChildren::new(),
            accessibility: AccessibilityInfo::default(),
            state: NodeState::default(),
            scroll_metrics: None,
            input_preedit: None,
            text_selection: None,
            mesh_key: None,
            loop_identity: None,
            cached_class_attr: None,
            cached_classes: Vec::new(),
            surface_root: false,
            finalized_style: None,
            finalized_accessibility: None,
            finalized_style_valid: false,
            finalized_style_pristine: false,
            finalized_accessibility_valid: false,
            memo_instance: None,
            authored: Arc::new(WidgetNodeAuthored {
                tag: tag.into(),
                attributes: AttributeMap::new(),
                event_handlers: BTreeMap::new(),
                event_handler_calls: BTreeMap::new(),
                accessibility_baseline: None,
                module_id: None,
                service_field_reads: Vec::new(),
                template_bindings: None,
                composition: WidgetCompositionMetadata::default(),
                bind_this: None,
                open_binding: None,
                source_file: None,
                slot_source: None,
            }),
        }
    }

    /// Identify a component memo boundary independently of its tree position.
    #[doc(hidden)]
    pub fn set_memo_instance(&mut self, instance: Arc<str>) {
        self.memo_instance = Some(instance);
    }

    #[doc(hidden)]
    pub fn memo_instance(&self) -> Option<&str> {
        self.memo_instance.as_deref()
    }

    /// Copy finalized projections into the original, position-independent memo
    /// tree. Authored inputs stay separate from runtime annotation overrides.
    /// Panics if corresponding nodes have different child counts.
    #[doc(hidden)]
    pub fn retain_finalization_from(&mut self, live: &Self) {
        let same_style = self
            .finalized_style
            .as_ref()
            .zip(live.finalized_style.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
        let same_semantics = self
            .finalized_accessibility
            .as_ref()
            .zip(live.finalized_accessibility.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b));
        if same_style
            && same_semantics
            && live.finalized_style_valid
            && live.finalized_accessibility_valid
        {
            return;
        }
        self.computed_style = live.computed_style.clone();
        self.accessibility = live.accessibility.clone();
        if self.accessibility_baseline().is_none() {
            if let Some(baseline) = live.accessibility_baseline() {
                self.set_accessibility_baseline(baseline.clone());
            }
        }
        self.finalized_style = live.finalized_style.clone();
        self.finalized_accessibility = live.finalized_accessibility.clone();
        self.finalized_style_valid = live.finalized_style_valid;
        self.finalized_style_pristine = live.finalized_style_pristine;
        self.finalized_accessibility_valid = live.finalized_accessibility_valid;
        assert_eq!(self.children.len(), live.children.len());
        for (cached, live) in self.children.iter_mut().zip(live.children.iter()) {
            cached.retain_finalization_from(live);
        }
    }

    /// Prevent sampled animation values in a retained tree from being treated
    /// as authored styles during a later selective rebuild.
    #[doc(hidden)]
    pub fn mark_style_sampled(&mut self) {
        self.finalized_style_pristine = false;
        self.finalized_style_valid = false;
        self.finalized_accessibility_valid = false;
    }

    /// Validate cached finalization after runtime annotation, bottom-up.
    /// Call after annotating children and before using a finalized memo clone.
    #[doc(hidden)]
    pub fn validate_finalized_subtree(&mut self) {
        self.finalized_style_valid = self.finalized_style_pristine
            && self
                .finalized_style
                .as_ref()
                .is_some_and(|cached| cached.matches_node(self))
            && self
                .children
                .iter()
                .all(|child| child.finalized_style_valid);
        self.finalized_accessibility_valid = self
            .finalized_accessibility
            .as_ref()
            .is_some_and(|cached| cached.matches_node(self))
            && self
                .children
                .iter()
                .all(|child| child.finalized_accessibility_valid);
    }

    #[doc(hidden)]
    pub fn set_template_bindings(&mut self, bindings: Arc<TemplateBindings>) {
        self.template_bindings = Some(bindings);
    }

    #[doc(hidden)]
    pub fn template_bindings(&self) -> Option<&Arc<TemplateBindings>> {
        self.template_bindings.as_ref()
    }

    #[doc(hidden)]
    pub fn shares_authored_payload_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.authored, &other.authored)
    }

    /// Whether any node here still shares authored payload with `other` —
    /// true at composition boundaries, where finalization may copy the root
    /// overlay while leaving clean descendants shared with a memo entry.
    #[doc(hidden)]
    pub fn contains_shared_authored_payload_with(&self, other: &Self) -> bool {
        self.shares_authored_payload_with(other)
            || self
                .children
                .iter()
                .any(|child| child.contains_shared_authored_payload_with(other))
    }

    #[doc(hidden)]
    pub fn authored_payload(&self) -> &WidgetNodeAuthored {
        &self.authored
    }

    /// Store the static semantic metadata used to rebuild the live
    /// accessibility projection. Runtime normalization and annotations stay
    /// in `accessibility` and never overwrite this baseline.
    #[doc(hidden)]
    pub fn set_accessibility_baseline(&mut self, info: AccessibilityInfo) {
        self.accessibility_baseline = Some(info);
    }

    /// Return static semantic metadata, when the compiler or an embedding
    /// caller supplied it explicitly.
    #[doc(hidden)]
    pub fn accessibility_baseline(&self) -> Option<&AccessibilityInfo> {
        self.accessibility_baseline.as_ref()
    }

    pub fn mark_promoted_popover(&mut self) {
        self.composition.promoted_popover = true;
    }

    pub fn is_promoted_popover(&self) -> bool {
        self.composition.promoted_popover
    }

    pub fn mark_promoted_window(&mut self) {
        self.composition.promoted_window = true;
    }

    pub fn is_promoted_window(&self) -> bool {
        self.composition.promoted_window
    }

    pub fn set_mesh_key(&mut self, key: impl Into<String>) {
        self.mesh_key = Some(key.into());
    }

    pub fn clear_mesh_key(&mut self) {
        self.mesh_key = None;
    }

    pub fn mesh_key(&self) -> Option<&str> {
        self.mesh_key.as_deref()
    }

    pub fn has_mesh_key(&self) -> bool {
        self.mesh_key().is_some()
    }

    /// Mark this node as the authored root of a top-level surface. The shell
    /// still reads its computed CSS margins for layer-shell placement, while
    /// layout keeps the synthetic surface wrapper from applying those margins
    /// a second time inside the surface buffer.
    pub fn mark_surface_root(&mut self) {
        self.surface_root = true;
    }

    pub fn clear_surface_root(&mut self) {
        self.surface_root = false;
    }

    pub fn is_surface_root(&self) -> bool {
        self.surface_root
    }

    pub fn set_loop_identity(&mut self, identity: impl Into<Arc<str>>) {
        self.loop_identity = Some(identity.into());
    }

    pub fn loop_identity(&self) -> Option<&str> {
        self.loop_identity.as_deref()
    }

    pub fn set_module_id(&mut self, module_id: impl Into<Arc<str>>) {
        self.module_id = Some(module_id.into());
    }

    /// Shared so sibling and child nodes reuse the same allocation.
    pub fn shared_module_id(&self) -> Option<&Arc<str>> {
        self.module_id.as_ref()
    }

    pub fn clear_module_id(&mut self) {
        self.module_id = None;
    }

    pub fn module_id(&self) -> Option<&str> {
        self.module_id.as_deref()
    }

    pub fn resolved_scroll_metrics(&self) -> WidgetScrollMetrics {
        self.scroll_metrics.unwrap_or_default()
    }

    pub fn mark_error_placeholder(&mut self) {
        self.composition.error_placeholder = true;
    }

    pub fn is_error_placeholder(&self) -> bool {
        self.composition.error_placeholder
    }

    pub fn set_bind_this(&mut self, name: impl Into<Arc<str>>) {
        self.bind_this = Some(name.into());
    }

    pub fn bind_this(&self) -> Option<&str> {
        self.bind_this.as_deref()
    }

    pub fn set_open_binding(&mut self, binding: OpenBinding) {
        self.open_binding = Some(binding);
    }

    pub fn open_binding(&self) -> Option<&OpenBinding> {
        self.open_binding.as_ref()
    }

    pub fn set_source_file(&mut self, path: impl Into<Arc<str>>) {
        self.source_file = Some(path.into());
    }

    pub fn source_file(&self) -> Option<&str> {
        self.source_file.as_deref()
    }

    pub fn set_slot_source(&mut self, module_id: impl Into<Arc<str>>) {
        self.slot_source = Some(module_id.into());
    }

    pub fn slot_source(&self) -> Option<&str> {
        self.slot_source.as_deref()
    }

    pub fn refresh_class_tokens_cache(&mut self) {
        let class_attr = self.attributes.get(Prop::Class).cloned();
        if self.cached_class_attr != class_attr {
            self.cached_classes = class_attr
                .as_deref()
                .into_iter()
                .flat_map(str::split_whitespace)
                .filter(|class| !class.is_empty())
                .map(str::to_owned)
                .collect();
            self.cached_class_attr = class_attr;
        }
    }

    pub fn class_tokens(&self) -> &[String] {
        &self.cached_classes
    }

    pub fn find(&self, id: NodeId) -> Option<&WidgetNode> {
        if self.id == id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(id) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut WidgetNode> {
        if self.id == id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_mut(id) {
                return Some(found);
            }
        }
        None
    }

    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn representative_node(index: usize, depth: usize, width: usize) -> WidgetNode {
        let mut node = WidgetNode::new(format!("component-node-{index}"));
        node.attributes.insert(
            Prop::Content,
            format!("node {index}: {}", "memoized component payload ".repeat(8)),
        );
        node.attributes
            .insert(Prop::Class, "surface-card primary interactive".into());
        node.event_handlers
            .insert("click".into(), format!("instance/{index}:activate").into());
        node.event_handler_calls.insert(
            "change".into(),
            EventHandlerCall {
                handler: format!("instance/{index}:change").into(),
                args: vec![serde_json::json!({ "index": index, "enabled": true })],
            },
        );
        node.service_field_reads
            .push(("audio".into(), format!("field_{index}")));
        if depth > 0 {
            node.children = (0..width)
                .map(|child| representative_node(index * width + child + 1, depth - 1, width))
                .collect();
        }
        node
    }

    fn legacy_deep_clone(node: &WidgetNode) -> WidgetNode {
        WidgetNode {
            id: node.id,
            computed_style: node.computed_style.clone(),
            layout: node.layout,
            children: node.children.iter().map(legacy_deep_clone).collect(),
            accessibility: node.accessibility.clone(),
            state: node.state,
            scroll_metrics: node.scroll_metrics,
            input_preedit: node.input_preedit,
            text_selection: node.text_selection.clone(),
            mesh_key: node.mesh_key.clone(),
            loop_identity: node.loop_identity.clone(),
            cached_class_attr: node.cached_class_attr.clone(),
            cached_classes: node.cached_classes.clone(),
            surface_root: node.surface_root,
            finalized_style: node.finalized_style.clone(),
            finalized_accessibility: node.finalized_accessibility.clone(),
            finalized_style_valid: node.finalized_style_valid,
            finalized_style_pristine: node.finalized_style_pristine,
            finalized_accessibility_valid: node.finalized_accessibility_valid,
            memo_instance: node.memo_instance.clone(),
            authored: Arc::new((*node.authored).clone()),
        }
    }

    fn dynamic_heap_bytes(node: &WidgetNode) -> usize {
        node.computed_style.transitions.capacity()
            * std::mem::size_of::<crate::style::TransitionStyle>()
            + node.computed_style.animations.capacity()
                * std::mem::size_of::<crate::style::AnimationStyle>()
            + node
                .accessibility
                .label
                .as_ref()
                .map_or(0, String::capacity)
            + node
                .accessibility
                .description
                .as_ref()
                .map_or(0, String::capacity)
            + node
                .accessibility
                .keyboard_shortcut
                .as_ref()
                .map_or(0, String::capacity)
            + node.mesh_key.as_ref().map_or(0, String::capacity)
            + node.cached_class_attr.as_ref().map_or(0, String::capacity)
            + node.cached_classes.capacity() * std::mem::size_of::<String>()
            + node
                .cached_classes
                .iter()
                .map(String::capacity)
                .sum::<usize>()
    }

    fn authored_heap_bytes(node: &WidgetNode) -> usize {
        node.tag.capacity()
            + node
                .attributes
                .iter()
                .map(|(key, value)| key.len() + value.capacity())
                .sum::<usize>()
            + node.event_handlers.len() * std::mem::size_of::<(String, HandlerTarget)>()
            + node
                .event_handlers
                .iter()
                .map(|(event, handler)| event.capacity() + handler.dynamic_heap_bytes())
                .sum::<usize>()
            + node.event_handler_calls.len() * std::mem::size_of::<(String, EventHandlerCall)>()
            + node
                .event_handler_calls
                .iter()
                .map(|(event, call)| {
                    event.capacity()
                        + call.handler.dynamic_heap_bytes()
                        + call.args.capacity() * std::mem::size_of::<serde_json::Value>()
                })
                .sum::<usize>()
            + node.service_field_reads.capacity() * std::mem::size_of::<(String, String)>()
            + node
                .service_field_reads
                .iter()
                .map(|(service, field)| service.capacity() + field.capacity())
                .sum::<usize>()
    }

    /// Heap owned only by this clone. Allocator headers and JSON-nested
    /// allocations are omitted, so the reported reduction is a lower bound.
    fn exclusive_heap_bytes(node: &WidgetNode) -> usize {
        let mut bytes = dynamic_heap_bytes(node);
        if Arc::strong_count(&node.authored) == 1 {
            bytes += std::mem::size_of::<WidgetNodeAuthored>() + authored_heap_bytes(node);
        }
        if Arc::strong_count(&node.children.0) == 1 {
            bytes += node.children.capacity() * std::mem::size_of::<WidgetNode>();
            bytes += node
                .children
                .iter()
                .map(exclusive_heap_bytes)
                .sum::<usize>();
        }
        bytes
    }

    #[test]
    fn new_widget_node_has_empty_service_field_reads() {
        assert!(WidgetNode::new("text").service_field_reads.is_empty());
    }

    #[test]
    fn widget_node_clone_copies_only_the_mutated_cow_path() {
        let mut root = representative_node(0, 2, 2);
        let mut cloned = root.clone();

        assert!(root.shares_authored_payload_with(&cloned));
        assert!(root.children.shares_allocation_with(&cloned.children));
        assert!(root.children[0].shares_authored_payload_with(&cloned.children[0]));
        assert!(root.children[1].shares_authored_payload_with(&cloned.children[1]));

        cloned.children[0]
            .attributes
            .insert(Prop::Content, "changed".into());

        assert_ne!(
            root.children[0].attributes.get(Prop::Content),
            cloned.children[0].attributes.get(Prop::Content)
        );
        assert!(!root.children[0].shares_authored_payload_with(&cloned.children[0]));
        assert!(
            root.children[1].shares_authored_payload_with(&cloned.children[1]),
            "an untouched sibling should keep sharing its authored payload"
        );
        assert!(
            root.shares_authored_payload_with(&cloned),
            "mutating a descendant must not copy the root payload"
        );

        root.children[0]
            .attributes
            .insert(Prop::Content, "original changed separately".into());
        assert_eq!(
            cloned.children[0]
                .attributes
                .get(Prop::Content)
                .map(String::as_str),
            Some("changed")
        );
    }

    // cargo test --release -p mesh-core-elements --lib cow_widget_tree_clone_beats_legacy_deep_clone -- --ignored --nocapture
    #[test]
    #[ignore = "release-only component memo tree-clone benchmark"]
    fn cow_widget_tree_clone_beats_legacy_deep_clone() {
        const ITERATIONS: usize = 2_000;
        let wide = representative_node(0, 2, 16);
        let deep = representative_node(0, 96, 1);

        fn measure(tree: &WidgetNode) -> (std::time::Duration, std::time::Duration, usize, usize) {
            let legacy_started = std::time::Instant::now();
            for _ in 0..ITERATIONS {
                std::hint::black_box(legacy_deep_clone(std::hint::black_box(tree)));
                std::hint::black_box(legacy_deep_clone(std::hint::black_box(tree)));
            }
            let legacy = legacy_started.elapsed();

            let cow_started = std::time::Instant::now();
            for _ in 0..ITERATIONS {
                std::hint::black_box(std::hint::black_box(tree).clone());
                std::hint::black_box(std::hint::black_box(tree).clone());
            }
            let cow = cow_started.elapsed();

            let legacy_clone = legacy_deep_clone(tree);
            let cow_clone = tree.clone();
            (
                legacy,
                cow,
                exclusive_heap_bytes(&legacy_clone),
                exclusive_heap_bytes(&cow_clone),
            )
        }

        let (wide_legacy, wide_cow, wide_legacy_bytes, wide_cow_bytes) = measure(&wide);
        let (deep_legacy, deep_cow, deep_legacy_bytes, deep_cow_bytes) = measure(&deep);
        let wide_speedup = wide_legacy.as_secs_f64() / wide_cow.as_secs_f64();
        let deep_speedup = deep_legacy.as_secs_f64() / deep_cow.as_secs_f64();

        eprintln!(
            "component memo COW clone: wide_nodes={} legacy={wide_legacy:?} cow={wide_cow:?} speedup={wide_speedup:.2}x retained={wide_legacy_bytes}->{wide_cow_bytes}B; deep_nodes={} legacy={deep_legacy:?} cow={deep_cow:?} speedup={deep_speedup:.2}x retained={deep_legacy_bytes}->{deep_cow_bytes}B",
            wide.node_count(),
            deep.node_count(),
        );
        assert!(
            wide_speedup >= 5.0,
            "wide COW clone should be at least 5x faster, measured {wide_speedup:.2}x"
        );
        assert!(
            deep_speedup >= 5.0,
            "deep COW clone should be at least 5x faster, measured {deep_speedup:.2}x"
        );
        assert!(
            wide_cow_bytes * 10 <= wide_legacy_bytes,
            "wide COW clone should retain at least 10x fewer unique heap bytes"
        );
        assert!(
            deep_cow_bytes * 10 <= deep_legacy_bytes,
            "deep COW clone should retain at least 10x fewer unique heap bytes"
        );
    }

    #[test]
    fn class_tokens_refresh_when_class_attribute_changes() {
        let mut node = WidgetNode::new("text");
        node.refresh_class_tokens_cache();
        assert!(node.class_tokens().is_empty());

        node.attributes
            .insert(Prop::Class, "primary compact".into());
        node.refresh_class_tokens_cache();
        assert_eq!(node.class_tokens(), ["primary", "compact"]);

        node.attributes.insert(Prop::Class, "compact active".into());
        node.refresh_class_tokens_cache();
        assert_eq!(node.class_tokens(), ["compact", "active"]);

        node.attributes.remove(Prop::Class);
        node.refresh_class_tokens_cache();
        assert!(node.class_tokens().is_empty());
    }

    #[test]
    fn widget_tree_validation_accepts_unique_runtime_and_source_identity() {
        let mut root = WidgetNode::new("box");
        let mut authored_grid = WidgetNode::new("box");
        authored_grid
            .attributes
            .insert(Prop::DataMeshElement, "grid".into());
        root.children.push(authored_grid);

        validate_widget_tree(&root).expect("known source and runtime tags should validate");
    }

    #[test]
    fn widget_tree_validation_rejects_duplicate_node_ids() {
        let mut root = WidgetNode::new("box");
        let mut child = WidgetNode::new("text");
        child.id = root.id;
        root.children.push(child);

        assert_eq!(
            validate_widget_tree(&root),
            Err(WidgetTreeValidationError::DuplicateNodeId { id: root.id })
        );
    }

    #[test]
    fn widget_tree_validation_rejects_duplicate_mesh_keys() {
        let mut root = WidgetNode::new("box");
        root.set_mesh_key("same");
        let mut child = WidgetNode::new("text");
        child.set_mesh_key("same");
        root.children.push(child);

        assert_eq!(
            validate_widget_tree(&root),
            Err(WidgetTreeValidationError::DuplicateMeshKey { key: "same".into() })
        );
    }

    #[test]
    fn widget_tree_validation_rejects_unknown_and_mismatched_element_tags() {
        let unknown = WidgetNode::new("not-an-element");
        assert!(matches!(
            validate_widget_tree(&unknown),
            Err(WidgetTreeValidationError::UnknownElementTag { tag, .. })
                if tag == "not-an-element"
        ));

        let mut mismatch = WidgetNode::new("button");
        mismatch
            .attributes
            .insert(Prop::DataMeshElement, "grid".into());
        assert!(matches!(
            validate_widget_tree(&mismatch),
            Err(WidgetTreeValidationError::ElementTagMismatch {
                source_tag,
                runtime_tag,
                expected_runtime_tag,
                ..
            }) if source_tag == "grid"
                && runtime_tag == "button"
                && expected_runtime_tag == "box"
        ));
    }
}
