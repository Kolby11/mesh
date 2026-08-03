use super::*;

#[test]
fn node_service_field_deps_forward_lookup() {
    let mut node = WidgetNode::new("text");
    node.service_field_reads
        .push(("audio".to_string(), "percent".to_string()));
    let id = node.id;
    let mut root = WidgetNode::new("column");
    root.children.push(node);

    let deps = NodeServiceFieldDependencies::build(&root);
    let fields = deps
        .fields_read_by_node(id)
        .expect("node should be tracked");
    assert!(fields.contains(&("audio".to_string(), "percent".to_string())));
}

#[test]
fn node_service_field_deps_reverse_lookup() {
    let mut node = WidgetNode::new("text");
    node.service_field_reads
        .push(("audio".to_string(), "percent".to_string()));
    let id = node.id;
    let mut root = WidgetNode::new("column");
    root.children.push(node);

    let deps = NodeServiceFieldDependencies::build(&root);
    let nodes = deps.nodes_reading_field("audio", "percent");
    assert!(nodes.contains(&id));
}

#[test]
fn node_service_field_deps_empty_node_not_in_forward() {
    let root = WidgetNode::new("column");
    let id = root.id;
    let deps = NodeServiceFieldDependencies::build(&root);
    assert!(deps.fields_read_by_node(id).is_none());
}

#[test]
fn node_service_field_deps_unknown_field_empty() {
    let root = WidgetNode::new("column");
    let deps = NodeServiceFieldDependencies::build(&root);
    let result = deps.nodes_reading_field("bogus", "x");
    assert!(result.is_empty());
}
