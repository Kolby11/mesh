use super::*;

fn bool_attribute(node: &WidgetNode, name: &str, default: bool) -> bool {
    node.attributes.get(name).map_or(default, |value| {
        let value = value.trim();
        !(value.is_empty()
            || value == "0"
            || value.eq_ignore_ascii_case("false")
            || value.eq_ignore_ascii_case("none"))
    })
}

/// Visible dialog, sheet and explicit focus scopes in paint order.
pub fn open_focus_scopes(tree: &WidgetNode) -> Vec<&WidgetNode> {
    fn visit<'a>(node: &'a WidgetNode, policy: NodeEligibility, out: &mut Vec<&'a WidgetNode>) {
        let policy = policy.child(node);
        if !policy.allows(InteractionTarget::Focus) {
            return;
        }
        if node_is_source(node, &["dialog", "sheet"]) || bool_attribute(node, "focus-scope", false)
        {
            out.push(node);
        }
        for child in mesh_core_elements::children_in_paint_order(node) {
            visit(child, policy, out);
        }
    }
    let mut scopes = Vec::new();
    visit(tree, NodeEligibility::ROOT, &mut scopes);
    scopes
}

/// The last visible modal scope owns keyboard traversal on this tree.
pub fn active_modal_scope(tree: &WidgetNode) -> Option<&WidgetNode> {
    open_focus_scopes(tree)
        .into_iter()
        .rev()
        .find(|node| bool_attribute(node, "modal", node_is_source(node, &["dialog", "sheet"])))
}

pub fn scope_contains(scope: &WidgetNode, key: &str) -> bool {
    find_node_by_key(scope, key).is_some()
}

/// Preserve the tree's transformed visual order while restricting it to a scope.
pub fn scope_focus_traversal(tree: &WidgetNode, scope: &WidgetNode) -> Vec<String> {
    collect_focus_traversal(tree)
        .into_iter()
        .filter(|key| scope_contains(scope, key))
        .collect()
}

fn is_menu(node: &WidgetNode) -> bool {
    node_is_source(node, &["menu"])
        || node
            .attributes
            .get("role")
            .is_some_and(|role| role == "menu")
}

pub fn menu_for_key<'a>(tree: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if !scope_contains(tree, key) {
        return None;
    }
    tree.children
        .iter()
        .find_map(|child| menu_for_key(child, key))
        .or_else(|| is_menu(tree).then_some(tree))
}

/// Enabled command rows belonging to this menu, excluding nested menus.
pub fn menu_items<'a>(tree: &WidgetNode, menu: &'a WidgetNode) -> Vec<&'a WidgetNode> {
    fn visit<'a>(tree: &WidgetNode, node: &'a WidgetNode, items: &mut Vec<&'a WidgetNode>) {
        if is_menu(node) {
            return;
        }
        let item = node_is_source(node, &["menu-item", "command-item", "preference-row"])
            || node
                .attributes
                .get("role")
                .is_some_and(|role| role.starts_with("menuitem"));
        if item
            && node.mesh_key().is_some_and(|key| {
                find_node_by_key(tree, key).is_some_and(|node| {
                    node_can_receive_target(tree, node.id, InteractionTarget::Focus)
                })
            })
        {
            items.push(node);
        }
        for child in &node.children {
            visit(tree, child, items);
        }
    }
    let mut items = Vec::new();
    for child in &menu.children {
        visit(tree, child, &mut items);
    }
    items
}

/// Explicit accessible labels take precedence over rendered text; shortcut and
/// nested menu text never become part of the command's typeahead label.
pub fn menu_label(node: &WidgetNode) -> String {
    if let Some(label) = node
        .attributes
        .get("aria-label")
        .or_else(|| node.attributes.get("label"))
    {
        return label.trim().to_lowercase();
    }
    fn text(node: &WidgetNode, out: &mut String) {
        if is_menu(node) || node_is_source(node, &["shortcut", "icon"]) {
            return;
        }
        if node.tag == "text" {
            if let Some(value) = node
                .attributes
                .get("content")
                .or_else(|| node.attributes.get("text"))
            {
                out.push_str(value);
                out.push(' ');
            }
        }
        for child in &node.children {
            text(child, out);
        }
    }
    let mut label = String::new();
    text(node, &mut label);
    label.trim().to_lowercase()
}

/// The first nested menu owned by a command row, including one currently closed.
pub fn nested_menu(node: &WidgetNode) -> Option<&WidgetNode> {
    node.children.iter().find_map(|child| {
        if is_menu(child) {
            Some(child)
        } else {
            nested_menu(child)
        }
    })
}

/// Resolve the enclosing command row and menu, including intervening wrappers.
pub fn parent_menu_trigger<'a>(
    tree: &'a WidgetNode,
    menu: &WidgetNode,
) -> Option<(&'a WidgetNode, String)> {
    fn path_to<'a>(node: &'a WidgetNode, key: &str, path: &mut Vec<&'a WidgetNode>) -> bool {
        path.push(node);
        if node.mesh_key() == Some(key) {
            return true;
        }
        for child in &node.children {
            if path_to(child, key, path) {
                return true;
            }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(tree, menu.mesh_key()?, &mut path) {
        return None;
    }
    let trigger = path
        .into_iter()
        .rev()
        .skip(1)
        .find(|node| {
            node_is_source(node, &["menu-item", "command-item", "preference-row"])
                || node
                    .attributes
                    .get("role")
                    .is_some_and(|role| role.starts_with("menuitem"))
        })?
        .mesh_key()?;
    Some((menu_for_key(tree, trigger)?, trigger.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(tag: &str, key: &str) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.attributes.insert("_mesh_key".into(), key.into());
        node.layout.width = 100.0;
        node.layout.height = 50.0;
        node
    }

    #[test]
    fn modal_scope_limits_traversal_and_closed_disabled_scopes_are_ignored() {
        let mut root = node("column", "root");
        root.children.push(node("button", "away"));
        let mut dialog = node("dialog", "dialog");
        dialog.children.push(node("button", "inside"));
        let mut closed = node("sheet", "closed");
        closed.attributes.insert("open".into(), "false".into());
        closed.children.push(node("button", "hidden"));
        dialog.children.push(closed);
        root.children.push(dialog);
        let mut disabled = node("dialog", "disabled");
        disabled.attributes.insert("disabled".into(), "true".into());
        root.children.push(disabled);
        assert_eq!(
            active_modal_scope(&root).and_then(WidgetNode::mesh_key),
            Some("dialog")
        );
        assert_eq!(collect_focus_traversal(&root), ["inside"]);
        assert_eq!(
            next_focus_target(&root, Some("away"), true).as_deref(),
            Some("inside")
        );
    }

    #[test]
    fn explicit_nonmodal_focus_scope_preserves_background_tab_order() {
        let mut root = node("column", "root");
        root.children.push(node("button", "away"));
        let mut scope = node("box", "scope");
        scope.attributes.insert("focus-scope".into(), "true".into());
        scope.children.push(node("button", "inside"));
        root.children.push(scope);
        assert!(active_modal_scope(&root).is_none());
        assert_eq!(open_focus_scopes(&root).len(), 1);
        assert_eq!(collect_focus_traversal(&root), ["away", "inside"]);
    }

    #[test]
    fn menu_items_stay_at_the_nearest_menu_level_and_labels_skip_shortcuts() {
        let mut menu = node("menu", "menu");
        let mut item = node("menu-item", "item");
        let mut label = node("text", "label");
        label.attributes.insert("content".into(), "  Édit ".into());
        let mut shortcut = node("shortcut", "shortcut");
        let mut shortcut_text = node("text", "shortcut_text");
        shortcut_text
            .attributes
            .insert("content".into(), "Ctrl+E".into());
        shortcut.children.push(shortcut_text);
        let mut submenu = node("menu", "submenu");
        submenu.children.push(node("menu-item", "child"));
        item.children.extend([label, shortcut, submenu]);
        menu.children.push(item);
        assert_eq!(
            menu_items(&menu, &menu)
                .iter()
                .filter_map(|item| item.mesh_key())
                .collect::<Vec<_>>(),
            ["item"]
        );
        assert_eq!(
            menu_for_key(&menu, "child").and_then(WidgetNode::mesh_key),
            Some("submenu")
        );
        assert_eq!(menu_label(&menu.children[0]), "édit");
    }
}
