use super::*;

#[test]
fn stable_runtime_node_id_is_deterministic_and_non_zero() {
    let first = stable_runtime_node_id("root/0/2");
    let second = stable_runtime_node_id("root/0/2");

    assert_ne!(first, 0);
    assert_eq!(first, second);
    assert_ne!(first, stable_runtime_node_id("root/0/3"));
}

#[test]
fn structural_key_conversion_matches_annotation_parent_chain() {
    let root = stable_runtime_node_id("root");
    let child = child_runtime_node_id(root, 2);
    let grandchild = child_runtime_node_id(child, 5);

    assert_eq!(runtime_node_id_for_key("root"), root);
    assert_eq!(runtime_node_id_for_key("root/2"), child);
    assert_eq!(runtime_node_id_for_key("root/2/5"), grandchild);
}

#[test]
fn chained_runtime_node_ids_are_deterministic_and_distinguish_siblings() {
    let parent = stable_runtime_node_id("root/0");
    assert_eq!(
        child_runtime_node_id(parent, 2),
        child_runtime_node_id(parent, 2)
    );
    assert_ne!(
        child_runtime_node_id(parent, 2),
        child_runtime_node_id(parent, 3)
    );
    assert_ne!(child_runtime_node_id(parent, 2), 0);
}
