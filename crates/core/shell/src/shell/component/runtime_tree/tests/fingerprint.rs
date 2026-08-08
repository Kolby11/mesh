use super::*;

#[test]
fn attribute_fingerprint_uses_node_id_instead_of_runtime_key_string() {
    let mut node = WidgetNode::new("box");
    node.id = stable_runtime_node_id("root/0");
    node.attributes.insert("_mesh_key".into(), "root/0".into());
    node.attributes.insert("class".into(), "card".into());
    let original = attributes_fingerprint(&node);

    node.attributes
        .insert("_mesh_key".into(), "different/debug/path".into());
    assert_eq!(attributes_fingerprint(&node), original);

    node.attributes.insert("class".into(), "card active".into());
    assert_ne!(attributes_fingerprint(&node), original);
}

#[test]
fn attribute_fingerprint_tracks_module_identity_for_style_diagnostics() {
    let mut node = WidgetNode::new("box");
    node.set_module_id("@test/first");
    let first = attributes_fingerprint(&node);
    node.set_module_id("@test/second");
    assert_ne!(attributes_fingerprint(&node), first);
}

#[test]
fn attribute_fingerprint_ignores_redundant_focused_annotation() {
    let mut node = WidgetNode::new("input");
    node.attributes.insert("_mesh_key".into(), "root/0".into());
    node.attributes.insert("value".into(), "hello".into());
    let original = attributes_fingerprint(&node);

    node.attributes
        .insert("_mesh_focused".into(), "true".into());
    assert_eq!(attributes_fingerprint(&node), original);

    node.attributes.insert("value".into(), "world".into());
    assert_ne!(attributes_fingerprint(&node), original);
}

#[test]
fn attribute_fingerprint_ignores_scroll_annotations_tracked_by_layout_fingerprint() {
    let mut node = WidgetNode::new("scroll-area");
    node.attributes.insert("_mesh_key".into(), "root/0".into());
    node.attributes.insert("class".into(), "scroller".into());
    let original_attributes = attributes_fingerprint(&node);
    let original_layout = layout_fingerprint(&node);

    node.attributes
        .insert("_mesh_scroll_y".into(), "12.5".into());
    node.attributes
        .insert("_mesh_scroll_max_y".into(), "40".into());
    node.attributes
        .insert("_mesh_content_height".into(), "120".into());

    assert_eq!(attributes_fingerprint(&node), original_attributes);
    assert_ne!(layout_fingerprint(&node), original_layout);

    node.attributes
        .insert("class".into(), "scroller active".into());
    assert_ne!(attributes_fingerprint(&node), original_attributes);
}

#[test]
fn attribute_fingerprint_tracks_typed_handler_arg_changes() {
    let mut node = WidgetNode::new("button");
    node.event_handler_calls.insert(
        "click".into(),
        mesh_core_elements::EventHandlerCall {
            handler: "select".into(),
            args: vec![serde_json::json!({
                "id": "alpha",
                "meta": { "index": 1, "enabled": true },
                "tags": ["a", "b"]
            })],
        },
    );
    let original = attributes_fingerprint(&node);

    node.event_handler_calls
        .get_mut("click")
        .expect("call")
        .args[0]["meta"]["index"] = serde_json::json!(2);

    assert_ne!(attributes_fingerprint(&node), original);
}

#[test]
fn retained_snapshot_keeps_common_child_lists_inline() {
    let mut node = WidgetNode::new("row");
    node.children = (0..8)
        .map(|index| {
            let mut child = WidgetNode::new("box");
            child.id = index + 1;
            child
        })
        .collect();

    let snapshot = retained_snapshot(&node);
    assert_eq!(snapshot.child_ids.len(), 8);
    assert!(!snapshot.child_ids.spilled());

    node.children.push(WidgetNode::new("box"));
    assert!(retained_snapshot(&node).child_ids.spilled());
}

#[test]
fn geometry_only_snapshot_reuses_non_layout_fingerprints() {
    let mut node = WidgetNode::new("box");
    node.attributes.insert("class".into(), "before".into());
    let previous = retained_snapshot(&node);

    node.layout.x = 24.0;
    node.computed_style.opacity = 0.5;
    node.attributes.insert("class".into(), "after".into());
    node.state.hovered = true;

    let geometry_only =
        retained_snapshot_with_render(&node, previous.render.clone(), Some(&previous));
    let (geometry_flags, changed_state_bits) = previous.diff_flags(&geometry_only);
    assert_eq!(geometry_flags, RetainedNodeDirtyFlags::LAYOUT);
    assert_eq!(changed_state_bits, 0);

    let full = retained_snapshot_with_render(&node, previous.render.clone(), None);
    let (full_flags, changed_state_bits) = previous.diff_flags(&full);
    assert!(full_flags.contains(RetainedNodeDirtyFlags::LAYOUT));
    assert!(full_flags.contains(RetainedNodeDirtyFlags::STYLE));
    assert!(full_flags.contains(RetainedNodeDirtyFlags::ATTRIBUTES));
    assert!(full_flags.contains(RetainedNodeDirtyFlags::STATE));
    assert_ne!(changed_state_bits, 0);
}
