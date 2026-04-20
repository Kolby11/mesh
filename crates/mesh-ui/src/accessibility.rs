/// Accessibility tree — semantic representation for AT-SPI and screen readers.
use crate::layout::LayoutRect;
use crate::tree::{NodeId, WidgetNode};
use mesh_component::meta::AccessibilityRole;

/// Accessibility metadata for a single widget node.
#[derive(Debug, Clone)]
pub struct AccessibilityInfo {
    pub role: AccessibilityRole,
    pub label: Option<String>,
    pub description: Option<String>,
    pub focusable: bool,
    pub focused: bool,
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
            state: AccessibilityState::default(),
            keyboard_shortcut: None,
        }
    }
}

/// Dynamic state for accessibility.
#[derive(Debug, Clone, Default)]
pub struct AccessibilityState {
    pub disabled: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub selected: bool,
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
}

/// Flat accessibility tree extracted from a widget tree.
#[derive(Debug, Clone)]
pub struct AccessibilityTree {
    pub nodes: Vec<AccessibilityTreeNode>,
}

impl AccessibilityTree {
    /// Build a flat accessibility tree from a laid-out widget tree.
    pub fn from_widget_tree(root: &WidgetNode) -> Self {
        let mut nodes = Vec::new();
        collect_a11y(root, None, &mut nodes);
        Self { nodes }
    }
}

fn collect_a11y(node: &WidgetNode, parent: Option<NodeId>, out: &mut Vec<AccessibilityTreeNode>) {
    let child_ids: Vec<NodeId> = node.children.iter().map(|c| c.id).collect();

    out.push(AccessibilityTreeNode {
        id: node.id,
        parent,
        children: child_ids,
        info: node.accessibility.clone(),
        bounds: node.layout,
    });

    for child in &node.children {
        collect_a11y(child, Some(node.id), out);
    }
}
