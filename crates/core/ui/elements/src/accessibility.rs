/// Accessibility tree — semantic representation for AT-SPI and screen readers.
use crate::interaction_contract::{InteractionTarget, NodeEligibility};
use crate::layout::LayoutRect;
use crate::tree::{NodeId, WidgetNode, WidgetTreeValidationError, validate_widget_tree};
use serde::{Deserialize, Serialize};

/// Accessibility roles for semantic tree construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccessibilityRole {
    Button,
    Slider,
    Label,
    TextInput,
    Checkbox,
    Switch,
    Region,
    List,
    ListItem,
    Image,
    Toolbar,
    Menu,
    MenuItem,
    Dialog,
    Alert,
    Status,
    ProgressBar,
    Tab,
    TabPanel,
    Separator,
    #[serde(untagged)]
    Custom(String),
}

impl std::fmt::Display for AccessibilityRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom(s) => write!(f, "{s}"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl AccessibilityRole {
    /// Parse an author-facing role name while keeping unknown roles available
    /// to automation and downstream accessibility adapters.
    pub fn from_name(role: &str) -> Self {
        match role.trim().to_ascii_lowercase().as_str() {
            "button" => Self::Button,
            "slider" => Self::Slider,
            "label" => Self::Label,
            "text-input" | "textinput" | "text_input" => Self::TextInput,
            "checkbox" => Self::Checkbox,
            "switch" => Self::Switch,
            "region" => Self::Region,
            "list" => Self::List,
            "list-item" | "listitem" | "list_item" => Self::ListItem,
            "image" => Self::Image,
            "toolbar" => Self::Toolbar,
            "menu" => Self::Menu,
            "menu-item" | "menuitem" | "menu_item" => Self::MenuItem,
            "dialog" => Self::Dialog,
            "alert" => Self::Alert,
            "status" => Self::Status,
            "progress-bar" | "progressbar" | "progress_bar" => Self::ProgressBar,
            "tab" => Self::Tab,
            "tab-panel" | "tabpanel" | "tab_panel" => Self::TabPanel,
            "separator" => Self::Separator,
            custom => Self::Custom(custom.to_string()),
        }
    }
}

/// Accessibility metadata for a single widget node.
#[derive(Debug, Clone)]
pub struct AccessibilityInfo {
    pub role: AccessibilityRole,
    pub label: Option<String>,
    pub description: Option<String>,
    pub focusable: bool,
    pub focused: bool,
    /// True when author, ARIA, or resolved style state removes this node from
    /// the semantic tree. The node remains in the live widget tree so layout,
    /// animation, and shell promotion can still use it.
    pub hidden: bool,
    /// Resolved visibility after applying ancestor and local hidden state.
    pub visible: bool,
    pub state: AccessibilityState,
    pub keyboard_shortcut: Option<String>,
}

impl Default for AccessibilityInfo {
    fn default() -> Self {
        Self {
            role: AccessibilityRole::Region,
            label: None,
            description: None,
            focusable: false,
            focused: false,
            hidden: false,
            visible: true,
            state: AccessibilityState::default(),
            keyboard_shortcut: None,
        }
    }
}

/// Return the focus state to expose in a semantic snapshot.
///
/// `ElementState::focused` is the live interaction state and therefore the
/// canonical source for accessibility focus. `AccessibilityInfo::focused` is
/// retained as a compatibility projection for callers that inspect the live
/// node, but it must not be used to build a snapshot because it can lag behind
/// input dispatch.
#[inline]
pub fn live_accessibility_focus(node: &WidgetNode) -> bool {
    node.state.focused
}

/// Dynamic state for accessibility.
#[derive(Debug, Clone, Default)]
pub struct AccessibilityState {
    pub disabled: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub selected: bool,
    pub pressed: bool,
    pub busy: bool,
    pub invalid: bool,
    pub required: bool,
    pub value: Option<String>,
    pub value_min: Option<f32>,
    pub value_max: Option<f32>,
}

/// A node in the flat accessibility tree.
#[derive(Debug, Clone)]
pub struct AccessibilityTreeNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub info: AccessibilityInfo,
    pub bounds: LayoutRect,
    pub relationships: AccessibilityRelationships,
}

/// Resolved relationships between visible semantic nodes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessibilityRelationships {
    pub labelled_by: Vec<NodeId>,
    pub described_by: Vec<NodeId>,
    pub controls: Vec<NodeId>,
    pub owns: Vec<NodeId>,
    pub details: Vec<NodeId>,
    pub error_message: Vec<NodeId>,
    pub tooltip_for: Option<NodeId>,
    pub popover_trigger: Option<NodeId>,
}

/// Flat accessibility tree extracted from a widget tree.
#[derive(Debug, Clone)]
pub struct AccessibilityTree {
    pub nodes: Vec<AccessibilityTreeNode>,
}

impl AccessibilityTree {
    /// Build a flat accessibility tree from a laid-out widget tree.
    pub fn from_widget_tree(root: &WidgetNode) -> Self {
        match Self::try_from_widget_tree(root) {
            Ok(tree) => tree,
            Err(error) => {
                tracing::error!(
                    target: "mesh::accessibility",
                    error = %error,
                    "rejecting invalid widget tree before accessibility projection"
                );
                Self { nodes: Vec::new() }
            }
        }
    }

    /// Build a flat accessibility tree, rejecting invalid identity or element
    /// definitions before any semantic node is projected.
    pub fn try_from_widget_tree(root: &WidgetNode) -> Result<Self, WidgetTreeValidationError> {
        validate_widget_tree(root)?;
        Ok(Self::from_validated_widget_tree(root))
    }

    /// Build a projection after the caller has validated the tree at the
    /// frame boundary. This avoids repeating the full validation traversal.
    pub(crate) fn from_validated_widget_tree(root: &WidgetNode) -> Self {
        let Some(mut root) = build_snapshot_node(root, NodeEligibility::ROOT) else {
            return Self { nodes: Vec::new() };
        };
        let mut index = std::collections::HashMap::new();
        collect_reference_index(&root, &mut index);
        let mut reference_text = std::collections::HashMap::new();
        collect_reference_text(&root, &mut reference_text);
        apply_reference_text(&mut root, &index, &reference_text);
        let mut nodes = Vec::new();
        flatten_snapshot_node(&root, None, &index, &mut nodes);
        Self { nodes }
    }

    /// Publish the current semantic projection under an explicit name at
    /// integration boundaries such as AccessKit and automation.
    pub fn publish(root: &WidgetNode) -> Self {
        Self::from_widget_tree(root)
    }
}

/// Normalize live node metadata after its children have been constructed.
///
/// This mutates only the semantic projection; authored attributes, layout,
/// and interaction state remain the source of truth for their own domains.
pub fn normalize_accessibility(root: &mut WidgetNode) {
    if let Err(error) = validate_widget_tree(root) {
        tracing::error!(
            target: "mesh::accessibility",
            error = %error,
            "rejecting invalid widget tree before accessibility normalization"
        );
        return;
    }
    normalize_node(root, NodeEligibility::ROOT);
}

/// Normalize live semantic metadata after validating the complete tree.
pub fn try_normalize_accessibility(root: &mut WidgetNode) -> Result<(), WidgetTreeValidationError> {
    validate_widget_tree(root)?;
    normalize_node(root, NodeEligibility::ROOT);
    Ok(())
}

#[derive(Debug, Clone)]
struct RawRelationships {
    labelled_by: Vec<String>,
    described_by: Vec<String>,
    controls: Vec<String>,
    owns: Vec<String>,
    details: Vec<String>,
    error_message: Vec<String>,
    tooltip_for: Option<String>,
    popover_trigger: Option<String>,
}

impl Default for RawRelationships {
    fn default() -> Self {
        Self {
            labelled_by: Vec::new(),
            described_by: Vec::new(),
            controls: Vec::new(),
            owns: Vec::new(),
            details: Vec::new(),
            error_message: Vec::new(),
            tooltip_for: None,
            popover_trigger: None,
        }
    }
}

#[derive(Debug, Clone)]
struct SnapshotNode {
    id: NodeId,
    info: AccessibilityInfo,
    bounds: LayoutRect,
    relations: RawRelationships,
    reference_names: Vec<String>,
    children: Vec<SnapshotNode>,
    visible_text: String,
}

#[derive(Debug, Clone)]
struct SnapshotReferenceText {
    label: Option<String>,
    description: Option<String>,
    visible_text: String,
}

fn normalize_node(node: &mut WidgetNode, parent_policy: NodeEligibility) -> String {
    let authored = if let Some(info) = node.accessibility_baseline() {
        info.clone()
    } else {
        // Preserve metadata assigned directly to a newly constructed node for
        // compatibility with embedding callers, then keep it separate from
        // the derived projection on subsequent passes.
        let info = node.accessibility.clone();
        node.set_accessibility_baseline(info.clone());
        info
    };
    let policy = parent_policy.child(node);
    let hidden = !policy.allows(InteractionTarget::Semantics);
    let mut child_text = Vec::new();
    for child in &mut node.children {
        let text = normalize_node(child, policy);
        if !text.is_empty() {
            child_text.push(text);
        }
    }
    let child_text = child_text.join(" ");
    let mut info = normalized_info(node, hidden, &child_text, &authored);
    info.state.disabled |= policy.is_disabled();
    let name_text = semantic_text(
        &info,
        visible_text(node, &child_text, info.hidden),
        info.hidden,
    );
    node.accessibility = info;
    name_text
}

fn build_snapshot_node(node: &WidgetNode, parent_policy: NodeEligibility) -> Option<SnapshotNode> {
    let policy = parent_policy.child(node);
    if !policy.allows(InteractionTarget::Semantics) {
        return None;
    }
    let children: Vec<_> = node
        .children
        .iter()
        .filter_map(|child| build_snapshot_node(child, policy))
        .collect();
    let child_text = children
        .iter()
        .map(|child| child.visible_text.as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let authored = node
        .accessibility_baseline()
        .cloned()
        .unwrap_or_else(|| node.accessibility.clone());
    let mut info = normalized_info(node, false, &child_text, &authored);
    info.state.disabled |= policy.is_disabled();
    let visible_text = semantic_text(&info, visible_text(node, &child_text, false), false);
    Some(SnapshotNode {
        id: node.id,
        info,
        bounds: node.layout,
        relations: raw_relationships(node),
        reference_names: authored_reference_names(node),
        children,
        visible_text,
    })
}

fn normalized_info(
    node: &WidgetNode,
    hidden: bool,
    child_text: &str,
    authored: &AccessibilityInfo,
) -> AccessibilityInfo {
    let mut info = authored.clone();
    let attributes = &node.attributes;

    if !attributes.contains_key("aria-role")
        && !attributes.contains_key("role")
        && info.role == AccessibilityRole::Region
        && let Some(contract) = crate::element::element_contract_for_tag(
            attributes
                .get("data-mesh-element")
                .map(String::as_str)
                .unwrap_or(node.tag.as_str()),
        )
    {
        info.role = contract.accessibility.role.clone();
        info.focusable = contract.accessibility.focusable;
    }

    if let Some(role) = attributes
        .get("aria-role")
        .or_else(|| attributes.get("role"))
        .filter(|value| !value.trim().is_empty())
    {
        info.role = AccessibilityRole::from_name(role);
    }

    let visible_text = visible_text(node, child_text, hidden);
    let label = attributes
        .get("label")
        .filter(|value| !value.trim().is_empty())
        .cloned();
    let aria_label = attributes
        .get("aria-label")
        .filter(|value| !value.trim().is_empty())
        .cloned();
    let alt = attributes
        .get("alt")
        .filter(|value| !value.trim().is_empty())
        .cloned();
    let preserved_label = (!hidden && (node.tag == "surface" || node.children.is_empty()))
        .then_some(info.label.clone())
        .flatten();
    info.label = if hidden {
        None
    } else if node.tag == "surface" {
        preserved_label
            .or_else(|| non_empty(visible_text))
            .or(label)
            .or(aria_label)
            .or(alt)
    } else {
        non_empty(visible_text)
            .or(label)
            .or(aria_label)
            .or(alt)
            .or(preserved_label)
    };

    let preserved_description = (!hidden && (node.tag == "surface" || node.children.is_empty()))
        .then_some(info.description.clone())
        .flatten();
    info.description = if hidden {
        None
    } else {
        attributes
            .get("aria-description")
            .or_else(|| attributes.get("title"))
            .or_else(|| attributes.get("tooltip"))
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .or(preserved_description)
    };
    info.keyboard_shortcut = attributes
        .get("aria-keyshortcuts")
        .or_else(|| attributes.get("key"))
        .or_else(|| attributes.get("keybind"))
        .or_else(|| attributes.get("shortcut"))
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .or_else(|| info.keyboard_shortcut.clone());

    info.state.disabled =
        node.state.disabled || bool_alias(attributes, &["disabled", "aria-disabled"]);
    info.state.checked =
        alias_bool(attributes, &["checked", "aria-checked"]).or(info.state.checked);
    info.state.selected =
        alias_bool(attributes, &["selected", "aria-selected"]).unwrap_or(info.state.selected);
    info.state.pressed =
        alias_bool(attributes, &["pressed", "aria-pressed"]).unwrap_or(info.state.pressed);
    info.state.expanded =
        alias_bool(attributes, &["expanded", "open", "aria-expanded"]).or(info.state.expanded);
    info.state.busy = alias_bool(attributes, &["busy", "aria-busy"]).unwrap_or(info.state.busy);
    info.state.invalid =
        alias_bool(attributes, &["invalid", "aria-invalid"]).unwrap_or(info.state.invalid);
    info.state.required =
        alias_bool(attributes, &["required", "aria-required"]).unwrap_or(info.state.required);
    info.state.checked = info.state.checked.or(node.state.checked.then_some(true));
    info.state.selected |= node.state.selected;
    info.state.pressed |= node.state.pressed;
    info.state.invalid |= node.state.invalid;
    info.state.required |= node.state.required;
    info.state.expanded = info.state.expanded.or(node.state.expanded.then_some(true));
    info.state.value = attributes
        .get("aria-valuenow")
        .or_else(|| attributes.get("value"))
        .cloned()
        .or(info.state.value);
    info.state.value_min = attributes
        .get("aria-valuemin")
        .and_then(|value| value.trim().parse().ok())
        .or_else(|| {
            attributes
                .get("min")
                .and_then(|value| value.trim().parse().ok())
        })
        .or(info.state.value_min);
    info.state.value_max = attributes
        .get("aria-valuemax")
        .and_then(|value| value.trim().parse().ok())
        .or_else(|| {
            attributes
                .get("max")
                .and_then(|value| value.trim().parse().ok())
        })
        .or(info.state.value_max);

    info.focused = live_accessibility_focus(node);
    info.hidden = hidden;
    info.visible = !hidden;
    info
}

fn visible_text(node: &WidgetNode, child_text: &str, hidden: bool) -> String {
    if hidden {
        return String::new();
    }
    let own_text = (node.tag == "text")
        .then(|| node.attributes.get("content"))
        .flatten()
        .map(String::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or_default();
    [own_text, child_text]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn semantic_text(info: &AccessibilityInfo, visible_text: String, hidden: bool) -> String {
    if hidden {
        String::new()
    } else if !visible_text.is_empty() {
        visible_text
    } else {
        info.label.clone().unwrap_or_default()
    }
}

fn bool_alias(attributes: &crate::AttributeMap, names: &[&str]) -> bool {
    names.iter().any(|name| {
        attributes
            .get_value(name)
            .is_some_and(|value| value.legacy_bool())
    })
}

fn alias_bool(attributes: &crate::AttributeMap, names: &[&str]) -> Option<bool> {
    names
        .iter()
        .find_map(|name| attributes.get_value(name).map(|value| value.legacy_bool()))
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn raw_relationships(node: &WidgetNode) -> RawRelationships {
    let list = |name: &str| {
        node.attributes
            .get(name)
            .into_iter()
            .flat_map(|value| value.split_whitespace().map(str::to_owned))
            .collect()
    };
    let source_tag = node
        .attributes
        .get("data-mesh-element")
        .map(String::as_str)
        .unwrap_or(node.tag.as_str());
    let popover = source_tag == "popover";
    RawRelationships {
        labelled_by: list("aria-labelledby"),
        described_by: list("aria-describedby"),
        controls: list("aria-controls"),
        owns: list("aria-owns"),
        details: list("aria-details"),
        error_message: list("aria-errormessage"),
        tooltip_for: node.attributes.get("tooltip-for").cloned(),
        popover_trigger: popover
            .then(|| {
                ["anchor-ref", "anchor-target", "anchor-element", "target"]
                    .iter()
                    .find_map(|name| node.attributes.get(name).cloned())
            })
            .flatten(),
    }
}

fn authored_reference_names(node: &WidgetNode) -> Vec<String> {
    [
        node.attributes.get("id").map(String::as_str),
        node.attributes.get("ref").map(String::as_str),
        node.mesh_key(),
        node.attributes.get("_mesh_bind_this").map(String::as_str),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .map(str::to_owned)
    .collect()
}

fn collect_reference_text(
    node: &SnapshotNode,
    text: &mut std::collections::HashMap<NodeId, SnapshotReferenceText>,
) {
    text.insert(
        node.id,
        SnapshotReferenceText {
            label: node.info.label.clone(),
            description: node.info.description.clone(),
            visible_text: node.visible_text.clone(),
        },
    );
    for child in &node.children {
        collect_reference_text(child, text);
    }
}

fn apply_reference_text(
    node: &mut SnapshotNode,
    index: &std::collections::HashMap<String, NodeId>,
    text: &std::collections::HashMap<NodeId, SnapshotReferenceText>,
) {
    let resolve = |names: &[String], description: bool| {
        names
            .iter()
            .filter_map(|name| index.get(name).and_then(|id| text.get(id)))
            .filter_map(|reference| reference_text_value(reference, description))
            .collect::<Vec<_>>()
    };

    let labels = resolve(&node.relations.labelled_by, false);
    if !labels.is_empty() {
        node.info.label = Some(labels.join(" "));
    }
    let descriptions = resolve(&node.relations.described_by, true);
    if !descriptions.is_empty() {
        node.info.description = Some(match node.info.description.take() {
            Some(existing) if !existing.trim().is_empty() => {
                format!("{existing} {}", descriptions.join(" "))
            }
            _ => descriptions.join(" "),
        });
    }
    for child in &mut node.children {
        apply_reference_text(child, index, text);
    }
}

fn reference_text_value(reference: &SnapshotReferenceText, description: bool) -> Option<String> {
    let value = if description {
        reference
            .description
            .clone()
            .or_else(|| reference.label.clone())
            .or_else(|| non_empty(reference.visible_text.clone()))
    } else {
        reference
            .label
            .clone()
            .or_else(|| non_empty(reference.visible_text.clone()))
    };
    value.filter(|value| !value.trim().is_empty())
}

fn collect_reference_index(
    node: &SnapshotNode,
    index: &mut std::collections::HashMap<String, NodeId>,
) {
    for name in &node.reference_names {
        index.insert(name.clone(), node.id);
    }
    index.insert(node.id.to_string(), node.id);
    for child in &node.children {
        collect_reference_index(child, index);
    }
}

fn flatten_snapshot_node(
    node: &SnapshotNode,
    parent: Option<NodeId>,
    index: &std::collections::HashMap<String, NodeId>,
    out: &mut Vec<AccessibilityTreeNode>,
) {
    let child_ids = node.children.iter().map(|child| child.id).collect();
    out.push(AccessibilityTreeNode {
        id: node.id,
        parent,
        children: child_ids,
        info: node.info.clone(),
        bounds: node.bounds,
        relationships: resolve_relationships(&node.relations, index),
    });
    for child in &node.children {
        flatten_snapshot_node(child, Some(node.id), index, out);
    }
}

fn resolve_relationships(
    raw: &RawRelationships,
    index: &std::collections::HashMap<String, NodeId>,
) -> AccessibilityRelationships {
    let resolve = |values: &[String]| {
        values
            .iter()
            .filter_map(|value| index.get(value).copied())
            .collect()
    };
    AccessibilityRelationships {
        labelled_by: resolve(&raw.labelled_by),
        described_by: resolve(&raw.described_by),
        controls: resolve(&raw.controls),
        owns: resolve(&raw.owns),
        details: resolve(&raw.details),
        error_message: resolve(&raw.error_message),
        tooltip_for: raw
            .tooltip_for
            .as_ref()
            .and_then(|value| index.get(value).copied()),
        popover_trigger: raw
            .popover_trigger
            .as_ref()
            .and_then(|value| index.get(value).copied()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Display, Visibility};

    fn attr(node: &mut WidgetNode, name: &str, value: &str) {
        node.attributes.insert(name.into(), value.into());
    }

    #[test]
    fn normalization_uses_visible_descendants_and_aria_aliases() {
        let mut root = WidgetNode::new("box");
        let mut button = WidgetNode::new("button");
        button.accessibility.role = AccessibilityRole::Button;
        button.accessibility.focusable = true;
        button.state.focused = true;
        attr(&mut button, "aria-role", "switch");
        attr(&mut button, "aria-description", "Toggles the panel");
        attr(&mut button, "aria-pressed", "true");

        let mut hidden_text = WidgetNode::new("text");
        attr(&mut hidden_text, "content", "Do not announce");
        attr(&mut hidden_text, "aria-hidden", "true");
        let mut visible_text = WidgetNode::new("text");
        attr(&mut visible_text, "content", "Open panel");
        button.children.push(hidden_text);
        button.children.push(visible_text);
        root.children.push(button);

        normalize_accessibility(&mut root);

        let button = &root.children[0];
        assert_eq!(button.accessibility.role, AccessibilityRole::Switch);
        assert_eq!(button.accessibility.label.as_deref(), Some("Open panel"));
        assert_eq!(
            button.accessibility.description.as_deref(),
            Some("Toggles the panel")
        );
        assert!(button.accessibility.state.pressed);
        assert!(button.accessibility.focused);
        assert_eq!(button.children[0].accessibility.label, None);
        assert!(button.children[0].accessibility.hidden);
        assert!(!button.children[0].accessibility.visible);

        let snapshot = AccessibilityTree::publish(&root);
        assert_eq!(snapshot.nodes.len(), 3);
        assert!(snapshot.nodes.iter().all(|node| node.info.visible));
        assert!(
            snapshot
                .nodes
                .iter()
                .all(|node| node.info.label.as_deref() != Some("Do not announce"))
        );
    }

    #[test]
    fn snapshot_focus_uses_live_state_over_stale_accessibility_projection() {
        let mut root = WidgetNode::new("box");
        let mut button = WidgetNode::new("button");
        button.accessibility.role = AccessibilityRole::Button;
        button.accessibility.focusable = true;

        // The projection may be stale between input dispatch and shell
        // annotation. Snapshot generation must still report the live state.
        button.accessibility.focused = true;
        button.state.focused = false;
        let button_id = button.id;
        root.children.push(button);

        let snapshot = AccessibilityTree::from_widget_tree(&root);
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .find(|node| node.id == button_id)
                .map(|node| node.info.focused),
            Some(false)
        );

        root.children[0].state.focused = true;
        root.children[0].accessibility.focused = false;
        let snapshot = AccessibilityTree::from_widget_tree(&root);
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .find(|node| node.id == button_id)
                .map(|node| node.info.focused),
            Some(true)
        );
    }

    #[test]
    fn accessible_name_precedence_tracks_hidden_children_and_locale_updates() {
        let mut named = WidgetNode::new("button");
        attr(&mut named, "label", "Fallback label");
        attr(&mut named, "aria-label", "Screen-reader label");
        let mut visible = WidgetNode::new("text");
        attr(&mut visible, "content", "Visible text");
        named.children.push(visible);
        normalize_accessibility(&mut named);
        assert_eq!(named.accessibility.label.as_deref(), Some("Visible text"));

        let mut localized = WidgetNode::new("button");
        let mut localized_text = WidgetNode::new("text");
        attr(&mut localized_text, "content", "English");
        localized.children.push(localized_text);
        normalize_accessibility(&mut localized);
        assert_eq!(localized.accessibility.label.as_deref(), Some("English"));

        attr(&mut localized.children[0], "aria-hidden", "true");
        normalize_accessibility(&mut localized);
        assert_eq!(localized.accessibility.label, None);
        assert!(localized.children[0].accessibility.hidden);

        attr(&mut localized.children[0], "aria-hidden", "false");
        attr(&mut localized.children[0], "content", "Slovak");
        normalize_accessibility(&mut localized);
        assert_eq!(localized.accessibility.label.as_deref(), Some("Slovak"));

        let mut icon_button = WidgetNode::new("button");
        let mut icon = WidgetNode::new("icon");
        attr(&mut icon, "alt", "Mute");
        icon_button.children.push(icon);
        normalize_accessibility(&mut icon_button);
        assert_eq!(icon_button.accessibility.label.as_deref(), Some("Mute"));

        attr(&mut icon_button.children[0], "aria-hidden", "true");
        normalize_accessibility(&mut icon_button);
        assert_eq!(icon_button.accessibility.label, None);
    }

    #[test]
    fn normalization_rebuilds_removed_aria_metadata_from_current_attributes() {
        let mut button = WidgetNode::new("button");
        attr(&mut button, "aria-role", "switch");
        attr(&mut button, "aria-label", "Mute");
        attr(&mut button, "aria-pressed", "true");
        normalize_accessibility(&mut button);
        assert_eq!(button.accessibility.role, AccessibilityRole::Switch);
        assert_eq!(button.accessibility.label.as_deref(), Some("Mute"));
        assert!(button.accessibility.state.pressed);

        button.attributes.remove("aria-role");
        button.attributes.remove("aria-label");
        button.attributes.remove("aria-pressed");
        normalize_accessibility(&mut button);

        assert_eq!(button.accessibility.role, AccessibilityRole::Button);
        assert_eq!(button.accessibility.label, None);
        assert!(!button.accessibility.state.pressed);
    }

    #[test]
    fn snapshot_resolves_relationships_only_to_visible_nodes() {
        let mut root = WidgetNode::new("box");
        let mut trigger = WidgetNode::new("button");
        attr(&mut trigger, "id", "trigger");
        attr(&mut trigger, "aria-labelledby", "label");
        attr(&mut trigger, "aria-describedby", "description");
        attr(&mut trigger, "aria-controls", "panel");

        let mut label = WidgetNode::new("text");
        attr(&mut label, "id", "label");
        attr(&mut label, "content", "Panel");
        let mut description = WidgetNode::new("text");
        attr(&mut description, "id", "description");
        attr(&mut description, "content", "Panel description");
        let mut panel = WidgetNode::new("box");
        attr(&mut panel, "id", "panel");
        let mut hidden_panel = WidgetNode::new("box");
        attr(&mut hidden_panel, "id", "hidden-panel");
        attr(&mut hidden_panel, "hidden", "true");
        let mut popover = WidgetNode::new("popover");
        attr(&mut popover, "id", "popover");
        attr(&mut popover, "anchor-ref", "trigger");
        let mut tooltip = WidgetNode::new("tooltip");
        attr(&mut tooltip, "tooltip-for", "trigger");

        let trigger_id = trigger.id;
        let label_id = label.id;
        let description_id = description.id;
        let panel_id = panel.id;
        let popover_id = popover.id;
        let tooltip_id = tooltip.id;
        root.children.push(trigger);
        root.children.push(label);
        root.children.push(description);
        root.children.push(panel);
        root.children.push(hidden_panel);
        root.children.push(popover);
        root.children.push(tooltip);

        let snapshot = AccessibilityTree::publish(&root);
        let trigger = snapshot
            .nodes
            .iter()
            .find(|node| node.id == trigger_id)
            .expect("trigger in snapshot");
        assert_eq!(trigger.info.label.as_deref(), Some("Panel"));
        assert_eq!(
            trigger.info.description.as_deref(),
            Some("Panel description")
        );
        assert_eq!(trigger.relationships.labelled_by, vec![label_id]);
        assert_eq!(trigger.relationships.described_by, vec![description_id]);
        assert_eq!(trigger.relationships.controls, vec![panel_id]);
        assert!(
            !trigger
                .relationships
                .controls
                .contains(&hidden_panel_id(&root))
        );

        let popover = snapshot
            .nodes
            .iter()
            .find(|node| node.id == popover_id)
            .expect("popover in snapshot");
        assert_eq!(popover.relationships.popover_trigger, Some(trigger_id));
        let tooltip = snapshot
            .nodes
            .iter()
            .find(|node| node.id == tooltip_id)
            .expect("tooltip in snapshot");
        assert_eq!(tooltip.relationships.tooltip_for, Some(trigger_id));
    }

    #[test]
    fn snapshot_filters_style_hidden_nodes() {
        let mut root = WidgetNode::new("box");
        let mut hidden = WidgetNode::new("text");
        attr(&mut hidden, "content", "Hidden by style");
        hidden.computed_style.display = Display::None;
        let mut collapsed = WidgetNode::new("text");
        attr(&mut collapsed, "content", "Collapsed by style");
        collapsed.computed_style.visibility = Visibility::Hidden;
        root.children.push(hidden);
        root.children.push(collapsed);

        let snapshot = AccessibilityTree::publish(&root);
        assert_eq!(snapshot.nodes.len(), 1);
        assert!(snapshot.nodes[0].children.is_empty());
    }

    #[test]
    fn accessibility_rejects_an_unknown_element_tree() {
        let root = WidgetNode::new("not-an-element");

        assert!(matches!(
            AccessibilityTree::try_from_widget_tree(&root),
            Err(WidgetTreeValidationError::UnknownElementTag { tag, .. })
                if tag == "not-an-element"
        ));
        assert!(AccessibilityTree::from_widget_tree(&root).nodes.is_empty());
    }

    #[test]
    fn snapshot_exposes_disabled_controls_but_removes_inert_subtrees() {
        let mut root = WidgetNode::new("box");
        let mut disabled = WidgetNode::new("button");
        let disabled_id = disabled.id;
        attr(&mut disabled, "aria-disabled", "true");
        let mut inert = WidgetNode::new("box");
        let inert_id = inert.id;
        attr(&mut inert, "inert", "true");
        let mut descendant = WidgetNode::new("button");
        let descendant_id = descendant.id;
        attr(&mut descendant, "label", "Not available");
        inert.children.push(descendant);
        root.children.push(disabled);
        root.children.push(inert);

        let snapshot = AccessibilityTree::publish(&root);
        let disabled = snapshot
            .nodes
            .iter()
            .find(|node| node.id == disabled_id)
            .expect("disabled control stays exposed");
        assert!(disabled.info.state.disabled);
        assert!(
            snapshot
                .nodes
                .iter()
                .all(|node| node.id != inert_id && node.id != descendant_id)
        );
    }

    fn hidden_panel_id(root: &WidgetNode) -> NodeId {
        root.children
            .iter()
            .find(|node| node.attributes.get("id").map(String::as_str) == Some("hidden-panel"))
            .expect("hidden panel")
            .id
    }
}
