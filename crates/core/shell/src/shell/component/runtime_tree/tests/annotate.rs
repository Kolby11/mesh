use super::*;

#[test]
fn annotate_runtime_tree_assigns_stable_ids_from_runtime_keys() {
    let mut first = WidgetNode::new("row");
    first.children.push(WidgetNode::new("button"));
    let mut second = WidgetNode::new("row");
    second.children.push(WidgetNode::new("button"));

    annotate_with_empty_context(&mut first);
    annotate_with_empty_context(&mut second);

    assert_eq!(first.id, second.id);
    assert_eq!(first.children[0].id, second.children[0].id);
    assert_ne!(first.id, first.children[0].id);
    assert_eq!(first.mesh_key(), Some("root"));
    assert_eq!(first.children[0].mesh_key(), Some("root/0"));
    assert!(!first.attributes.contains_key("_mesh_key"));
    assert!(!first.children[0].attributes.contains_key("_mesh_key"));
}
