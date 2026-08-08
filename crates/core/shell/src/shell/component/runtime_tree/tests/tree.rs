use super::*;

#[test]
fn retained_widget_tree_reports_dirty_categories_by_stable_id() {
    let mut tree = WidgetNode::new("row");
    tree.children.push(WidgetNode::new("button"));
    annotate_with_empty_context(&mut tree);

    let mut retained = RetainedWidgetTree::default();
    let first = retained.update(&tree);
    assert_eq!(first.inserted, 2);
    assert_eq!(retained.generation(), 1);
    let child_id = tree.children[0].id;
    let child_key = retained
        .retained_key_for_node_id(child_id)
        .expect("child should be stored in retained slotmap");
    assert_eq!(
        retained.dirty_flags_for(child_id),
        RetainedNodeDirtyFlags::INSERTED
    );
    assert!(retained.is_node_dirty(child_id));
    assert!(retained.dirty_node_ids().is_empty());

    let clean = retained.update(&tree);
    assert!(!clean.any());
    assert_eq!(retained.generation(), 1);
    assert_eq!(retained.retained_key_for_node_id(child_id), Some(child_key));
    assert!(retained.dirty_flags_for(child_id).is_empty());
    assert!(!retained.is_node_dirty(child_id));
    assert!(retained.dirty_node_ids().is_empty());

    tree.children[0].layout.width = 42.0;
    tree.children[0].computed_style.background_color = Color::BLACK;
    tree.children[0]
        .attributes
        .insert("title".into(), "changed".into());
    tree.children[0].state.hovered = true;

    let dirty = retained.update(&tree);
    assert_eq!(dirty.layout, 1);
    assert_eq!(dirty.style, 1);
    assert_eq!(dirty.attributes, 1);
    assert_eq!(dirty.state, 1);
    assert_eq!(dirty.inserted, 0);
    assert_eq!(dirty.removed, 0);
    assert_eq!(retained.last_dirty(), dirty);
    assert_eq!(retained.generation(), 2);
    assert_eq!(retained.retained_key_for_node_id(child_id), Some(child_key));
    assert!(retained.is_node_dirty(child_id));
    assert_eq!(retained.dirty_node_ids(), &HashSet::from([child_id]));
    assert_eq!(
        retained.dirty_flags_for(child_id),
        RetainedNodeDirtyFlags::LAYOUT
            | RetainedNodeDirtyFlags::STYLE
            | RetainedNodeDirtyFlags::ATTRIBUTES
            | RetainedNodeDirtyFlags::STATE
    );
}

#[test]
fn retained_widget_tree_owns_render_fingerprint_diff() {
    use mesh_core_render::RenderObjectTree;

    let mut tree = WidgetNode::new("row");
    tree.children.push(WidgetNode::new("text"));
    annotate_with_empty_context(&mut tree);

    let mut retained = RetainedWidgetTree::default();
    let mut separate = RenderObjectTree::default();
    retained.update(&tree);
    assert_eq!(retained.render_dirty(), separate.update(&tree));

    let child = &mut tree.children[0];
    child.layout.x = 12.0;
    child.computed_style.background_color = Color::BLACK;
    child.computed_style.box_shadow.blur_radius = 4.0;
    child.attributes.insert("content".into(), "changed".into());
    child.accessibility.role = AccessibilityRole::Label;
    child.accessibility.label = Some("Changed label".into());
    child.accessibility.focusable = true;

    retained.update(&tree);
    assert_eq!(retained.render_dirty(), separate.update(&tree));
    assert_eq!(retained.render_dirty_node_ids(), separate.dirty_node_ids());
    assert_eq!(retained.render_dirty().geometry, 1);
    assert_eq!(retained.render_dirty().material, 1);
    assert_eq!(retained.render_dirty().text, 1);
    assert_eq!(retained.render_dirty().accessibility, 1);

    tree.children[0].accessibility.label = Some("Accessibility only".into());
    retained.update(&tree);
    assert_eq!(retained.render_dirty(), separate.update(&tree));
    assert_eq!(retained.render_dirty().accessibility, 1);

    tree.children[0].computed_style.box_shadow.blur_radius = 8.0;
    retained.update(&tree);
    assert_eq!(retained.render_dirty(), separate.update(&tree));
    assert_eq!(retained.render_dirty().material, 1);
}

#[test]
fn direct_retained_update_preserves_structural_insert_remove_and_reorder() {
    let mut tree = WidgetNode::new("row");
    tree.children.push(WidgetNode::new("button"));
    tree.children.push(WidgetNode::new("text"));
    annotate_with_empty_context(&mut tree);

    let mut retained = RetainedWidgetTree::default();
    assert_eq!(retained.update(&tree).inserted, 3);
    let removed_id = tree.children[0].id;

    tree.children.swap(0, 1);
    let reordered = retained.update(&tree);
    assert_eq!(reordered.children, 1);
    assert_eq!(reordered.inserted, 0);
    assert_eq!(reordered.removed, 0);

    tree.children.remove(1);
    let removed = retained.update(&tree);
    assert_eq!(removed.children, 1);
    assert_eq!(removed.removed, 1);
    assert!(!retained.node_keys.contains_key(&removed_id));

    tree.children.push(WidgetNode::new("slider"));
    let inserted = retained.update(&tree);
    assert_eq!(inserted.children, 1);
    assert_eq!(inserted.inserted, 1);
    assert_eq!(inserted.removed, 0);

    tree.children[1] = WidgetNode::new("input");
    let replaced = retained.update(&tree);
    assert_eq!(replaced.children, 1);
    assert_eq!(replaced.inserted, 1);
    assert_eq!(replaced.removed, 1);
}

#[test]
#[should_panic(expected = "runtime NodeId collision while updating retained snapshots")]
fn retained_update_rejects_duplicate_live_node_ids_in_release_too() {
    let mut tree = WidgetNode::new("row");
    tree.children.push(WidgetNode::new("button"));
    annotate_with_empty_context(&mut tree);
    tree.children[0].id = tree.id;

    RetainedWidgetTree::default().update(&tree);
}

#[test]
fn scoped_retained_update_matches_full_diff_and_tracks_propagated_layout() {
    let mut tree = benchmark_plain_tree(3, 4);
    annotate_with_empty_context(&mut tree);
    let mut full = RetainedWidgetTree::default();
    let mut scoped = RetainedWidgetTree::default();
    full.update(&tree);
    scoped.update(&tree);

    let dirty_id = {
        let leaf = first_deep_leaf_mut(&mut tree);
        leaf.computed_style.background_color = Color::BLACK;
        leaf.attributes.insert("title".into(), "changed".into());
        leaf.state.hovered = true;
        leaf.id
    };
    let propagated_layout_id = tree.children[1].id;
    tree.children[1].layout.x = 37.0;

    let full_dirty = full.update(&tree);
    let (scoped_dirty, dirty_node_refs) =
        scoped.update_for_dirty_roots_collect(&tree, &HashSet::from([dirty_id]));
    assert_eq!(scoped_dirty, full_dirty);
    assert_eq!(scoped.dirty_node_ids(), full.dirty_node_ids());
    assert_eq!(
        dirty_node_refs
            .expect("sparse update")
            .iter()
            .map(|node| node.id)
            .collect::<HashSet<_>>(),
        *full.dirty_node_ids()
    );
    assert!(scoped.dirty_node_ids().contains(&dirty_id));
    assert!(scoped.dirty_node_ids().contains(&propagated_layout_id));
    for node_id in full.dirty_node_ids() {
        assert_eq!(
            scoped.dirty_flags_for(*node_id),
            full.dirty_flags_for(*node_id)
        );
    }
}

#[test]
fn scoped_retained_update_falls_back_when_structure_changes() {
    let mut tree = benchmark_plain_tree(2, 3);
    annotate_with_empty_context(&mut tree);
    let mut full = RetainedWidgetTree::default();
    let mut scoped = RetainedWidgetTree::default();
    full.update(&tree);
    scoped.update(&tree);

    tree.children.push(WidgetNode::new("input"));
    let full_dirty = full.update(&tree);
    let scoped_dirty = scoped.update_for_dirty_roots(&tree, &HashSet::new());
    assert_eq!(scoped_dirty, full_dirty);
    assert_eq!(scoped.dirty_node_ids(), full.dirty_node_ids());
    assert_eq!(scoped.node_keys.len(), full.node_keys.len());
}

#[test]
fn scoped_retained_update_promotes_broad_dirty_root_to_full_update() {
    let mut tree = benchmark_plain_tree(4, 3);
    annotate_with_empty_context(&mut tree);
    let root_id = tree.id;
    let mut retained = RetainedWidgetTree::default();
    retained.update(&tree);

    tree.computed_style.opacity = 0.5;
    retained.update_for_dirty_roots(&tree, &HashSet::from([root_id]));

    assert!(!retained.last_update_was_scoped());
    assert_eq!(retained.last_dirty().style, 1);
}

#[test]
fn narrow_script_direct_diff_finds_leaf_and_rejects_structure_changes() {
    let mut previous = benchmark_plain_tree(3, 4);
    annotate_with_empty_context(&mut previous);
    let mut full = RetainedWidgetTree::default();
    let mut scoped = RetainedWidgetTree::default();
    full.update(&previous);
    scoped.update(&previous);
    let mut fresh = previous.clone();
    let dirty_id = first_deep_leaf_mut(&mut fresh).id;
    first_deep_leaf_mut(&mut fresh)
        .attributes
        .insert("content".into(), "changed".into());
    fresh.children[1].layout.x = 19.0;

    let dirty_roots = narrow_script_dirty_roots(&previous, &fresh)
        .expect("stable structure should produce authoritative roots");
    assert_eq!(dirty_roots, HashSet::from([dirty_id]));
    let full_dirty = full.update(&fresh);
    let scoped_dirty = scoped.update_for_dirty_roots(&fresh, &dirty_roots);
    assert_eq!(scoped_dirty, full_dirty);
    assert_eq!(scoped.dirty_node_ids(), full.dirty_node_ids());

    fresh.children.push(WidgetNode::new("input"));
    assert!(narrow_script_dirty_roots(&previous, &fresh).is_none());
}

#[test]
fn direct_snapshot_analysis_preserves_layout_dirty_detection() {
    let mut tree = WidgetNode::new("row");
    tree.children.push(WidgetNode::new("button"));
    annotate_with_empty_context(&mut tree);

    let mut retained = RetainedWidgetTree::default();
    retained.update(&tree);
    let dirty_snapshot_ids = |snapshots: Option<Vec<WidgetNode>>| {
        snapshots
            .unwrap_or_default()
            .into_iter()
            .map(|node| node.id)
            .collect::<HashSet<_>>()
    };
    assert_eq!(
        dirty_snapshot_ids(retained.layout_dirty_node_snapshots(&tree)),
        HashSet::new()
    );

    tree.children[0].layout.width = 42.0;
    assert_eq!(
        dirty_snapshot_ids(retained.layout_dirty_node_snapshots(&tree)),
        HashSet::from([tree.children[0].id])
    );

    tree.children.push(WidgetNode::new("text"));
    assert!(retained.layout_dirty_node_snapshots(&tree).is_none());
}

#[test]
#[should_panic(expected = "runtime NodeId collision")]
fn direct_retained_update_panics_on_duplicate_node_ids() {
    let mut root = WidgetNode::new("row");
    root.id = 42;
    root.attributes
        .insert("_mesh_key".into(), "root".to_string());
    let mut child = WidgetNode::new("button");
    child.id = 42;
    child
        .attributes
        .insert("_mesh_key".into(), "root/0".to_string());
    root.children.push(child);

    RetainedWidgetTree::default().update(&root);
}
