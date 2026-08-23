//! AccessKit retained-node runtime update adapter.
//!
//! Feature-gated per Phase 50. This builds real `accesskit::TreeUpdate`
//! values from retained MESH `WidgetNode` trees while platform publication
//! remains deferred.

#![cfg(feature = "renderer-accesskit")]

use accesskit::{
    Action, Node, NodeId as AccessKitNodeId, Rect, Role, Toggled, Tree, TreeId, TreeUpdate,
};
use mesh_core_elements::{
    AccessibilityRole, AccessibilityTree, AccessibilityTreeNode, NodeId, WidgetNode,
};

pub fn build_accesskit_runtime_update(root: &WidgetNode) -> TreeUpdate {
    let snapshot = AccessibilityTree::publish(root);
    let root_id = snapshot
        .nodes
        .first()
        .map(|node| accesskit_node_id(node.id))
        .unwrap_or_else(|| accesskit_node_id(root.id));
    let focus = snapshot
        .nodes
        .iter()
        .find(|node| node.info.focused)
        .map(|node| accesskit_node_id(node.id))
        .unwrap_or(root_id);
    let nodes = snapshot.nodes.iter().map(accesskit_node).collect();

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(root_id)),
        tree_id: TreeId::ROOT,
        focus,
    }
}

fn accesskit_node(node: &AccessibilityTreeNode) -> (AccessKitNodeId, Node) {
    let mut accesskit_node = Node::new(role_for(&node.info.role));

    if let Some(label) = node.info.label.clone() {
        accesskit_node.set_label(label);
    }
    if let Some(description) = node.info.description.clone() {
        accesskit_node.set_description(description);
    }
    if let Some(value) = node.info.state.value.clone() {
        accesskit_node.set_value(value);
    }
    if node.info.focusable {
        accesskit_node.add_action(Action::Focus);
    }
    if node.info.state.disabled {
        accesskit_node.set_disabled();
    }
    if node.info.state.selected {
        accesskit_node.set_selected(true);
    }
    if let Some(checked) = node.info.state.checked {
        accesskit_node.set_toggled(if checked {
            Toggled::True
        } else {
            Toggled::False
        });
        accesskit_node.add_action(Action::Click);
    }
    if let Some(expanded) = node.info.state.expanded {
        accesskit_node.set_expanded(expanded);
    }
    if let Some(min) = node.info.state.value_min {
        accesskit_node.set_min_numeric_value(min as f64);
    }
    if let Some(max) = node.info.state.value_max {
        accesskit_node.set_max_numeric_value(max as f64);
    }
    if let Some(value) = node
        .info
        .state
        .value
        .as_ref()
        .and_then(|value| value.parse::<f64>().ok())
    {
        accesskit_node.set_numeric_value(value);
    }
    if matches!(
        node.info.role,
        AccessibilityRole::Button | AccessibilityRole::Switch | AccessibilityRole::Checkbox
    ) {
        accesskit_node.add_action(Action::Click);
    }
    if matches!(
        node.info.role,
        AccessibilityRole::Slider | AccessibilityRole::TextInput
    ) {
        accesskit_node.add_action(Action::SetValue);
    }

    accesskit_node.set_bounds(Rect {
        x0: node.bounds.x as f64,
        y0: node.bounds.y as f64,
        x1: (node.bounds.x + node.bounds.width) as f64,
        y1: (node.bounds.y + node.bounds.height) as f64,
    });
    if !node.children.is_empty() {
        let children: Vec<AccessKitNodeId> = node
            .children
            .iter()
            .map(|child| accesskit_node_id(*child))
            .collect();
        accesskit_node.set_children(children);
    }
    let related_ids = |ids: &[NodeId]| {
        ids.iter()
            .copied()
            .map(accesskit_node_id)
            .collect::<Vec<_>>()
    };
    if !node.relationships.controls.is_empty() {
        accesskit_node.set_controls(related_ids(&node.relationships.controls));
    }
    if !node.relationships.described_by.is_empty() {
        accesskit_node.set_described_by(related_ids(&node.relationships.described_by));
    }
    if !node.relationships.labelled_by.is_empty() {
        accesskit_node.set_labelled_by(related_ids(&node.relationships.labelled_by));
    }
    if !node.relationships.owns.is_empty() {
        accesskit_node.set_owns(related_ids(&node.relationships.owns));
    }
    if !node.relationships.details.is_empty() {
        accesskit_node.set_details(related_ids(&node.relationships.details));
    }
    if let Some(error_message) = node.relationships.error_message.first() {
        accesskit_node.set_error_message(accesskit_node_id(*error_message));
    }
    if let Some(popover_trigger) = node.relationships.popover_trigger {
        accesskit_node.set_popup_for(accesskit_node_id(popover_trigger));
    }

    (accesskit_node_id(node.id), accesskit_node)
}

fn accesskit_node_id(node_id: NodeId) -> AccessKitNodeId {
    AccessKitNodeId(node_id)
}

fn role_for(role: &AccessibilityRole) -> Role {
    match role {
        AccessibilityRole::Button => Role::Button,
        AccessibilityRole::Slider => Role::Slider,
        AccessibilityRole::Label => Role::Label,
        AccessibilityRole::TextInput => Role::TextInput,
        AccessibilityRole::Checkbox => Role::CheckBox,
        AccessibilityRole::Switch => Role::Switch,
        AccessibilityRole::Region => Role::Region,
        AccessibilityRole::List => Role::List,
        AccessibilityRole::ListItem => Role::ListItem,
        AccessibilityRole::Image => Role::Image,
        AccessibilityRole::Toolbar => Role::Toolbar,
        AccessibilityRole::Menu => Role::Menu,
        AccessibilityRole::MenuItem => Role::MenuItem,
        AccessibilityRole::Dialog => Role::Dialog,
        AccessibilityRole::Alert => Role::Alert,
        AccessibilityRole::Status => Role::Status,
        AccessibilityRole::ProgressBar => Role::ProgressIndicator,
        AccessibilityRole::Tab => Role::Tab,
        AccessibilityRole::TabPanel => Role::TabPanel,
        AccessibilityRole::Separator => Role::Splitter,
        AccessibilityRole::Custom(_) => Role::GenericContainer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::style::Dimension;
    use mesh_core_elements::{LayoutRect, WidgetNode};

    fn node(tag: &str, id: NodeId) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.id = id;
        node.layout = LayoutRect {
            x: id as f32,
            y: 2.0,
            width: 30.0,
            height: 12.0,
        };
        node.computed_style.width = Dimension::Px(30.0);
        node.computed_style.height = Dimension::Px(12.0);
        node
    }

    #[test]
    fn accesskit_update_uses_retained_node_ids_and_children() {
        let mut root = node("box", 10);
        root.accessibility.role = AccessibilityRole::Region;
        let mut button = node("button", 11);
        button.accessibility.role = AccessibilityRole::Button;
        button.accessibility.label = Some("Open audio controls".to_string());
        button.accessibility.focusable = true;
        root.children.push(button);

        let update = build_accesskit_runtime_update(&root);

        assert_eq!(
            update.tree.as_ref().expect("tree").root,
            AccessKitNodeId(10)
        );
        assert_eq!(update.focus, AccessKitNodeId(10));
        assert_eq!(update.nodes.len(), 2);
        let root_node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == AccessKitNodeId(10))
            .map(|(_, node)| node)
            .expect("root");
        assert_eq!(root_node.children(), &[AccessKitNodeId(11)]);
        let button_node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == AccessKitNodeId(11))
            .map(|(_, node)| node)
            .expect("button");
        assert_eq!(button_node.role(), Role::Button);
        assert_eq!(button_node.label(), Some("Open audio controls"));
        assert!(button_node.supports_action(Action::Focus));
        assert!(button_node.supports_action(Action::Click));
    }

    #[test]
    fn accesskit_update_preserves_control_state_and_focus() {
        let mut root = node("box", 1);
        let mut slider = node("slider", 2);
        slider.accessibility.role = AccessibilityRole::Slider;
        slider.accessibility.focusable = true;
        // The compatibility projection may lag; the live state owns focus.
        slider.accessibility.focused = false;
        slider.state.focused = true;
        slider.accessibility.state.value = Some("42".to_string());
        slider.accessibility.state.value_min = Some(0.0);
        slider.accessibility.state.value_max = Some(100.0);
        root.children.push(slider);

        let update = build_accesskit_runtime_update(&root);
        assert_eq!(update.focus, AccessKitNodeId(2));
        let slider_node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == AccessKitNodeId(2))
            .map(|(_, node)| node)
            .expect("slider");
        assert_eq!(slider_node.role(), Role::Slider);
        assert_eq!(slider_node.value(), Some("42"));
        assert_eq!(slider_node.numeric_value(), Some(42.0));
        assert_eq!(slider_node.min_numeric_value(), Some(0.0));
        assert_eq!(slider_node.max_numeric_value(), Some(100.0));
        assert!(slider_node.supports_action(Action::SetValue));
    }

    #[test]
    fn accesskit_update_publishes_normalized_names_and_visibility() {
        let mut root = node("box", 20);
        let mut button = node("button", 21);
        button.accessibility.role = AccessibilityRole::Button;
        button.accessibility.focusable = true;
        button.state.focused = true;

        let mut hidden = node("text", 22);
        hidden
            .attributes
            .insert("content".into(), "Hidden name".into());
        hidden
            .attributes
            .insert("aria-hidden".into(), "true".into());
        let mut visible = node("text", 23);
        visible
            .attributes
            .insert("content".into(), "Visible name".into());
        button.children.push(hidden);
        button.children.push(visible);
        root.children.push(button);

        let update = build_accesskit_runtime_update(&root);

        assert_eq!(update.focus, AccessKitNodeId(21));
        assert_eq!(update.nodes.len(), 3);
        let button_node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == AccessKitNodeId(21))
            .map(|(_, node)| node)
            .expect("button");
        assert_eq!(button_node.label(), Some("Visible name"));
        assert_eq!(button_node.children(), &[AccessKitNodeId(23)]);
        assert!(
            !update
                .nodes
                .iter()
                .any(|(id, _)| *id == AccessKitNodeId(22))
        );
    }
}
