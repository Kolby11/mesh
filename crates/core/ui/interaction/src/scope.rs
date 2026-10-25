use super::*;
use mesh_core_elements::Prop;

fn bool_attribute(node: &WidgetNode, name: Prop, default: bool) -> bool {
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
        mesh_core_elements::frame_work::visit(
            mesh_core_elements::frame_work::Pass::FocusScopeRefresh,
        );
        let policy = policy.child(node);
        if !policy.allows(InteractionTarget::Focus) {
            return;
        }
        if node_is_source(node, &["dialog", "sheet"])
            || bool_attribute(node, Prop::FocusScope, false)
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
    open_focus_scopes(tree).into_iter().rev().find(|node| {
        bool_attribute(
            node,
            Prop::Modal,
            node_is_source(node, &["dialog", "sheet"]),
        )
    })
}

pub fn scope_contains(scope: &WidgetNode, key: &str) -> bool {
    find_node_by_key(scope, key).is_some()
}

/// Where focus starts when `scope` opens, in its Tab order: the first element
/// marked `autofocus`, else the first editable text field, else the first
/// stop. `autofocus="false"` on the scope itself opts out and yields `None`.
pub fn initial_focus_target(tree: &WidgetNode, scope: &WidgetNode) -> Option<String> {
    if scope
        .attributes
        .get(Prop::Autofocus)
        .is_some_and(|value| value == "false")
    {
        return None;
    }
    // Order within the scope alone: a promoted popover's content lies
    // outside its parent surface's clip, so the whole tree's order omits it.
    let traversal = collect_focus_traversal(scope);
    let marked = |key: &&String| {
        find_node_by_key(scope, key).is_some_and(|node| {
            node.attributes
                .get(Prop::Autofocus)
                .is_some_and(|value| value != "false")
        })
    };
    traversal
        .iter()
        .find(marked)
        .or_else(|| {
            traversal
                .iter()
                .find(|key| crate::hit_test::is_editable_input_key(tree, key))
        })
        .or_else(|| traversal.first())
        .cloned()
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
            .get(Prop::Role)
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
                .get(Prop::Role)
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
        .get(Prop::AriaLabel)
        .or_else(|| node.attributes.get(Prop::Label))
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
                .get(Prop::Content)
                .or_else(|| node.attributes.get(Prop::Text))
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
                    .get(Prop::Role)
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
        node.set_mesh_key(key);
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
        closed.attributes.insert(Prop::Open, "false".into());
        closed.children.push(node("button", "hidden"));
        dialog.children.push(closed);
        root.children.push(dialog);
        let mut disabled = node("dialog", "disabled");
        disabled.attributes.insert(Prop::Disabled, "true".into());
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
        scope.attributes.insert(Prop::FocusScope, "true".into());
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
        label.attributes.insert(Prop::Content, "  Édit ".into());
        let mut shortcut = node("shortcut", "shortcut");
        let mut shortcut_text = node("text", "shortcut_text");
        shortcut_text
            .attributes
            .insert(Prop::Content, "Ctrl+E".into());
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
