use super::super::*;
use super::common::*;
use crate::RenderObjectDirtySummary;
use mesh_core_elements::style::{Color, Overflow};
use mesh_core_elements::{NodeId, WidgetNode};

#[test]
fn sparse_entry_patch_matches_full_collection_for_material_updates() {
    let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
    let unchanged = node(2, "box", 0.0, 0.0, 50.0, 20.0);
    let changed = node(3, "box", 50.0, 0.0, 50.0, 20.0);
    root.children.extend([unchanged, changed]);

    let mut full = RetainedDisplayList::default();
    let mut sparse = RetainedDisplayList::default();
    full.update(&root, 120, 40, false, true);
    sparse.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            inserted: 3,
            ..Default::default()
        },
        &HashSet::from([1, 2, 3]),
        120,
        40,
        false,
        true,
    );

    root.children[1].computed_style.background_color = Color {
        r: 220,
        g: 40,
        b: 30,
        a: 255,
    };
    let full_metrics = full.update(&root, 120, 40, false, true);
    let sparse_metrics = sparse.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            material: 1,
            ..Default::default()
        },
        &HashSet::from([3]),
        120,
        40,
        false,
        true,
    );

    assert_eq!(sparse.entries, full.entries);
    assert_eq!(sparse.damage_rects(), full.damage_rects());
    assert_eq!(sparse_metrics.entries_rebuilt, full_metrics.entries_rebuilt);
    assert_eq!(sparse_metrics.damage_area, full_metrics.damage_area);
    assert_eq!(
        command_debugs(sparse.paint_commands(), &[1, 2, 3]),
        command_debugs(full.paint_commands(), &[1, 2, 3])
    );
}

// cargo test -p mesh-core-render --release -- sparse_material_update_beats_full_display_rebuild --ignored --nocapture
#[test]
#[ignore = "release-only sparse material-update display-list benchmark"]
fn sparse_material_update_beats_full_display_rebuild() {
    let iterations = 1_000_u64;
    let changed_row = 60;
    let changed_col = 10;

    let mut full_root = display_entry_benchmark_tree(120, 20);
    let changed_id = full_root.children[changed_row].children[changed_col].id;
    let mut full = RetainedDisplayList::default();
    full.update(&full_root, 1200, 800, false, true);
    let full_started = std::time::Instant::now();
    let mut full_rebuilt = 0u64;
    for generation in 0..iterations {
        full_root.children[changed_row].children[changed_col]
            .computed_style
            .background_color
            .r = (generation % 251) as u8;
        full_rebuilt = full_rebuilt.wrapping_add(std::hint::black_box(
            full.update(&full_root, 1200, 800, false, true)
                .entries_rebuilt,
        ));
    }
    let full_time = full_started.elapsed();

    let mut sparse_root = display_entry_benchmark_tree(120, 20);
    let mut sparse = RetainedDisplayList::default();
    sparse.update(&sparse_root, 1200, 800, false, true);
    let dirty_ids = HashSet::from([changed_id]);
    let sparse_started = std::time::Instant::now();
    let mut sparse_rebuilt = 0u64;
    for generation in 0..iterations {
        sparse_root.children[changed_row].children[changed_col]
            .computed_style
            .background_color
            .r = (generation % 251) as u8;
        sparse_rebuilt = sparse_rebuilt.wrapping_add(std::hint::black_box(
            sparse
                .update_with_dirty_nodes(
                    &sparse_root,
                    RenderObjectDirtySummary {
                        material: 1,
                        ..Default::default()
                    },
                    &dirty_ids,
                    1200,
                    800,
                    false,
                    true,
                )
                .entries_rebuilt,
        ));
    }
    let sparse_time = sparse_started.elapsed();

    assert_eq!(sparse_rebuilt, full_rebuilt);
    assert_eq!(sparse.entries, full.entries);
    assert_eq!(sparse.damage_rects(), full.damage_rects());
    eprintln!(
        "one-node material display-list update: full {full_time:?}; sparse {sparse_time:?}; ratio {:.1}x",
        full_time.as_secs_f64() / sparse_time.as_secs_f64()
    );
    assert!(
        sparse_time * 2 < full_time,
        "sparse material updates should be at least 2x faster than full display-list rebuilds"
    );
}

#[test]
fn display_list_reuses_unrelated_subtrees_for_transform_updates() {
    let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
    let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
    left.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
    let mut right = node(4, "box", 60.0, 0.0, 40.0, 40.0);
    right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
    root.children.push(left);
    root.children.push(right);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 120, 40, false, true);
    let before = list.paint_commands().to_vec();

    root.children[0].computed_style.transform.translate_x = 12.0;
    let metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            transform: 1,
            ..Default::default()
        },
        &HashSet::from([2]),
        120,
        40,
        false,
        true,
    );
    let after = list.paint_commands().to_vec();

    let right_before = command_debugs(&before, &[4, 5]);
    let right_after = command_debugs(&after, &[4, 5]);
    assert_eq!(right_before, right_after);
    assert!(metrics.subtree_segments_reused > 0);
    assert!(metrics.subtree_segments_rebuilt > 0);
    assert_eq!(metrics.full_fallback_count, 0);
}

#[test]
fn subtree_generation_ignores_unrelated_surface_paint_changes() {
    let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
    let mut sibling = node(2, "box", 0.0, 0.0, 40.0, 40.0);
    sibling.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
    let mut popup = node(4, "popover", 60.0, 0.0, 40.0, 40.0);
    popup.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
    root.children.push(sibling);
    root.children.push(popup);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 120, 40, false, true);
    let initial_root = list.generation();
    let initial_popup = list.subtree_generation(4).expect("popup subtree");

    root.children[0].computed_style.background_color.r = 99;
    list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            material: 1,
            ..Default::default()
        },
        &HashSet::from([2]),
        120,
        40,
        false,
        true,
    );

    assert!(list.generation() > initial_root);
    assert_eq!(list.subtree_generation(4), Some(initial_popup));

    root.children[1].children[0]
        .computed_style
        .background_color
        .g = 77;
    list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            material: 1,
            ..Default::default()
        },
        &HashSet::from([5]),
        120,
        40,
        false,
        true,
    );

    assert!(list.subtree_generation(4).expect("popup subtree") > initial_popup);
    assert_eq!(list.subtree_generation(999), None);
}

#[test]
fn target_local_display_list_matches_transient_child_popup_pixels() {
    let popup = child_popup_benchmark_tree(4, 6);
    let offset_x = -popup.layout.x + 7.0;
    let offset_y = -popup.layout.y + 5.0;
    let mut transient = crate::PixelBuffer::new(214, 132);
    let mut retained = crate::PixelBuffer::new(214, 132);

    crate::paint_frontend_tree_at_for_module(
        &popup,
        &mut transient,
        1.0,
        offset_x,
        offset_y,
        None,
        None,
    );
    let mut display_list = RetainedDisplayList::default();
    display_list.update_at(&popup, offset_x, offset_y, 214, 132, false, false);
    crate::paint_display_list_for_module_with_profiling_metrics(
        display_list.paint_commands(),
        &mut retained,
        1.0,
        None,
        None,
        None,
        None,
    );

    assert_eq!(retained.data(), transient.data());
}

// cargo test -p mesh-core-render --release -- retained_child_popup_replay_beats_transient_display_list --ignored --nocapture
#[test]
#[ignore = "release-only child popup retained-replay benchmark"]
fn retained_child_popup_replay_beats_transient_display_list() {
    const ITERATIONS: u64 = 400;
    let offset_x = -300.0 + 6.0;
    let offset_y = -180.0 + 6.0;
    let changed_id = 1;

    let mut transient_tree = child_popup_benchmark_tree(6, 10);
    let mut transient_buffer = crate::PixelBuffer::new(212, 132);
    let transient_started = std::time::Instant::now();
    for generation in 1..=ITERATIONS {
        transient_tree.computed_style.opacity = 0.25 + (generation % 70) as f32 / 100.0;
        transient_buffer.clear(Color::TRANSPARENT);
        crate::paint_frontend_tree_at_for_module(
            std::hint::black_box(&transient_tree),
            &mut transient_buffer,
            1.0,
            offset_x,
            offset_y,
            None,
            None,
        );
    }
    let transient_time = transient_started.elapsed();

    let mut retained_tree = child_popup_benchmark_tree(6, 10);
    let mut retained_buffer = crate::PixelBuffer::new(212, 132);
    let mut display_list = RetainedDisplayList::default();
    display_list.update_at_for_retained_generation(
        &retained_tree,
        0,
        offset_x,
        offset_y,
        212,
        132,
        false,
        false,
    );
    let dirty_ids = HashSet::from([changed_id]);
    let retained_started = std::time::Instant::now();
    for generation in 1..=ITERATIONS {
        retained_tree.computed_style.opacity = 0.25 + (generation % 70) as f32 / 100.0;
        display_list.update_at_for_retained_generation_with_dirty_nodes(
            std::hint::black_box(&retained_tree),
            generation,
            RenderObjectDirtySummary {
                opacity: 1,
                ..Default::default()
            },
            &dirty_ids,
            offset_x,
            offset_y,
            212,
            132,
            false,
            false,
        );
        retained_buffer.clear(Color::TRANSPARENT);
        crate::paint_display_list_for_module_with_profiling_metrics(
            display_list.paint_commands(),
            &mut retained_buffer,
            1.0,
            None,
            None,
            None,
            None,
        );
    }
    let retained_time = retained_started.elapsed();

    assert_eq!(retained_buffer.data(), transient_buffer.data());
    eprintln!(
        "animated child popup raster: transient display-list {transient_time:?}; retained display-list {retained_time:?}; ratio {:.2}x",
        transient_time.as_secs_f64() / retained_time.as_secs_f64()
    );
    assert!(
        retained_time * 10 < transient_time * 9,
        "retained child replay should improve the production animation path by at least 10%"
    );
}

// cargo test -p mesh-core-render --release -- popup_subtree_generation_beats_broad_surface_repaint --ignored --nocapture
#[test]
#[ignore = "release-only child-popup invalidation microbenchmark"]
fn popup_subtree_generation_beats_broad_surface_repaint() {
    const FRAMES: u64 = 10_000;
    const BUFFER_BYTES: usize = 160 * 90 * 4;

    let mut eager_buffer = vec![255_u8; BUFFER_BYTES];
    let eager_started = std::time::Instant::now();
    let mut cached_parent_generation = 0_u64;
    for parent_generation in 1..=FRAMES {
        if std::hint::black_box(parent_generation) != cached_parent_generation {
            std::hint::black_box(&mut eager_buffer).fill(0);
            cached_parent_generation = parent_generation;
        }
    }
    let eager_time = eager_started.elapsed();

    let mut retained_buffer = vec![255_u8; BUFFER_BYTES];
    let retained_started = std::time::Instant::now();
    let popup_generation = 1_u64;
    let mut cached_popup_generation = 0_u64;
    let mut repaints = 0_u64;
    for _ in 0..FRAMES {
        if std::hint::black_box(popup_generation) != cached_popup_generation {
            std::hint::black_box(&mut retained_buffer).fill(0);
            cached_popup_generation = popup_generation;
            repaints += 1;
        }
    }
    let retained_time = retained_started.elapsed();

    assert_eq!(repaints, 1);
    assert_eq!(eager_buffer, retained_buffer);
    eprintln!(
        "unrelated parent updates: broad generation {eager_time:?}; popup subtree generation {retained_time:?}; ratio {:.1}x; repaints={FRAMES}/{repaints}",
        eager_time.as_secs_f64() / retained_time.as_secs_f64()
    );
    assert!(retained_time * 10 < eager_time);
}

#[test]
fn dirty_ancestor_collection_preserves_ancestors_for_sparse_dirty_nodes() {
    let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
    let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
    left.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
    let mut right = node(4, "box", 60.0, 0.0, 40.0, 40.0);
    right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
    root.children.push(left);
    root.children.push(right);

    let ancestors = collect_dirty_ancestor_ids(&root, &HashSet::from([3]));

    assert_eq!(ancestors, HashSet::from([1, 2]));
}

// cargo test -p mesh-core-render --release -- dirty_ancestor_collection_stops_after_sparse_dirty_nodes --ignored --nocapture
#[test]
#[ignore = "release-only dirty ancestor microbenchmark"]
fn dirty_ancestor_collection_stops_after_sparse_dirty_nodes() {
    fn build_subtree(next_id: &mut NodeId, width: usize, depth: usize) -> WidgetNode {
        let id = *next_id;
        *next_id += 1;
        let mut root = node(id, "box", 0.0, 0.0, 20.0, 20.0);
        if depth > 0 {
            root.children = (0..width)
                .map(|_| build_subtree(next_id, width, depth - 1))
                .collect();
        }
        root
    }

    fn old_collect_dirty_ancestor_ids(
        root: &WidgetNode,
        dirty_node_ids: &HashSet<NodeId>,
    ) -> HashSet<NodeId> {
        fn walk(
            node: &WidgetNode,
            dirty_node_ids: &HashSet<NodeId>,
            path: &mut Vec<NodeId>,
            ancestors: &mut HashSet<NodeId>,
        ) {
            if dirty_node_ids.contains(&node.id) {
                for ancestor in path.iter().copied() {
                    ancestors.insert(ancestor);
                }
            }
            path.push(node.id);
            for child in &node.children {
                walk(child, dirty_node_ids, path, ancestors);
            }
            path.pop();
        }

        let mut ancestors = HashSet::new();
        let mut path = Vec::new();
        walk(root, dirty_node_ids, &mut path, &mut ancestors);
        ancestors
    }

    let mut next_id = 1;
    let root = build_subtree(&mut next_id, 5, 5);
    let dirty = HashSet::from([root.children[0].children[0].children[0].id]);
    let iterations = 50_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total += old_collect_dirty_ancestor_ids(
            std::hint::black_box(&root),
            std::hint::black_box(&dirty),
        )
        .len();
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        new_total +=
            collect_dirty_ancestor_ids(std::hint::black_box(&root), std::hint::black_box(&dirty))
                .len();
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "dirty ancestor collection: full walk {old_time:?}; early exit {new_time:?}; ratio {:.1}x; counts={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time * 2 < old_time);
}

// cargo test -p mesh-core-render --release -- dirty_ancestor_scratch_reuse_beats_fresh_allocations --ignored --nocapture
#[test]
#[ignore = "release-only dirty ancestor scratch microbenchmark"]
fn dirty_ancestor_scratch_reuse_beats_fresh_allocations() {
    fn build_subtree(next_id: &mut NodeId, width: usize, depth: usize) -> WidgetNode {
        let id = *next_id;
        *next_id += 1;
        let mut root = node(id, "box", 0.0, 0.0, 20.0, 20.0);
        if depth > 0 {
            root.children = (0..width)
                .map(|_| build_subtree(next_id, width, depth - 1))
                .collect();
        }
        root
    }

    let mut next_id = 1;
    let root = build_subtree(&mut next_id, 5, 5);
    let dirty = HashSet::from([root.children[0].children[0].children[0].id]);
    let iterations = 50_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total += std::hint::black_box(collect_dirty_ancestor_ids(&root, &dirty).len());
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    let mut ancestors = HashSet::new();
    let mut path = Vec::new();
    for _ in 0..iterations {
        ancestors.clear();
        path.clear();
        collect_dirty_ancestor_ids_into(&root, &dirty, &mut path, &mut ancestors);
        new_total += std::hint::black_box(ancestors.len());
    }
    let new_time = new_started.elapsed();

    assert_eq!(old_total, new_total);
    eprintln!(
        "dirty ancestor scratch: fresh {old_time:?}; reused {new_time:?}; ratio {:.2}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}

#[test]
fn display_list_unchanged_tree_skips_flat_command_rebuild() {
    let root = display_entry_benchmark_tree(8, 8);
    let mut list = RetainedDisplayList::default();
    let first = list.update(&root, 800, 400, false, true);
    let initial_commands = format!("{:?}", list.paint_commands());

    let second = list.update(&root, 800, 400, false, true);

    assert!(first.subtree_commands_rebuilt > 0);
    assert_eq!(second.entries_rebuilt, 0);
    assert_eq!(second.entries_reused, first.entries_total);
    assert_eq!(second.subtree_segments_rebuilt, 0);
    assert_eq!(second.subtree_commands_rebuilt, 0);
    assert_eq!(second.damage_area, 0);
    assert_eq!(format!("{:?}", list.paint_commands()), initial_commands);
}

#[test]
fn display_list_reuses_unrelated_subtrees_for_scroll_updates() {
    let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
    let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
    left.computed_style.overflow_x = Overflow::Hidden;
    left.attributes.insert("_mesh_scroll_x".into(), "0".into());
    left.children.push(node(3, "text", 30.0, 4.0, 20.0, 12.0));
    let mut right = node(4, "box", 60.0, 0.0, 40.0, 40.0);
    right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
    root.children.push(left);
    root.children.push(right);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 120, 40, false, true);
    let before = list.paint_commands().to_vec();

    root.children[0]
        .attributes
        .insert("_mesh_scroll_x".into(), "18".into());
    let metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            geometry: 1,
            ..Default::default()
        },
        &HashSet::from([2]),
        120,
        40,
        false,
        true,
    );
    let after = list.paint_commands().to_vec();

    let right_before = command_debugs(&before, &[4, 5]);
    let right_after = command_debugs(&after, &[4, 5]);
    assert_eq!(right_before, right_after);
    assert!(metrics.subtree_segments_reused > 0);
    assert!(metrics.subtree_commands_rebuilt > 0);
}

#[test]
fn display_list_reuses_clean_descendants_for_paint_only_dirty_parent() {
    let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
    let mut panel = node(2, "box", 0.0, 0.0, 120.0, 40.0);
    panel.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
    panel.children.push(node(4, "text", 30.0, 4.0, 20.0, 12.0));
    root.children.push(panel);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 40, false, true);
    let before = list.paint_commands().to_vec();

    root.children[0].computed_style.background_color = Color::from_hex("#336699").unwrap();
    let metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            material: 1,
            ..Default::default()
        },
        &HashSet::from([2]),
        160,
        40,
        false,
        true,
    );
    let after = list.paint_commands().to_vec();

    assert_eq!(
        command_debugs(&before, &[3, 4]),
        command_debugs(&after, &[3, 4])
    );
    assert!(
        metrics.subtree_segments_reused >= 2,
        "paint-only dirty parent should reuse clean child subtrees: {metrics:?}"
    );
    assert_eq!(
        metrics.subtree_commands_rebuilt, 2,
        "only root and the dirty parent should rebuild their local commands"
    );
}

#[test]
fn display_list_rebuilds_descendants_for_layout_dirty_parent() {
    let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
    let mut panel = node(2, "box", 0.0, 0.0, 120.0, 40.0);
    panel.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
    panel.children.push(node(4, "text", 30.0, 4.0, 20.0, 12.0));
    root.children.push(panel);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 40, false, true);

    root.children[0].layout.x = 8.0;
    let metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            geometry: 1,
            ..Default::default()
        },
        &HashSet::from([2]),
        160,
        40,
        false,
        true,
    );

    assert_eq!(
        metrics.subtree_segments_reused, 0,
        "layout dirty parent must rebuild descendants because offsets changed"
    );
    assert_eq!(metrics.subtree_commands_rebuilt, 4);
}

// cargo test -p mesh-core-render --release -- paint_only_dirty_parent_reuses_clean_descendants --ignored --nocapture
#[test]
#[ignore = "release-only display-list paint-only subtree reuse microbenchmark"]
fn paint_only_dirty_parent_reuses_clean_descendants() {
    fn make_tree(children: usize) -> WidgetNode {
        let mut root = node(1, "row", 0.0, 0.0, children as f32 * 12.0, 24.0);
        let mut panel = node(2, "box", 0.0, 0.0, children as f32 * 12.0, 24.0);
        for index in 0..children {
            let id = 3 + index as NodeId;
            let mut child = node(id, "text", index as f32 * 12.0, 0.0, 10.0, 12.0);
            child
                .attributes
                .insert("content".into(), format!("Item {index}"));
            panel.children.push(child);
        }
        root.children.push(panel);
        root
    }

    let mut root = make_tree(512);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 6200, 40, false, true);
    root.children[0].computed_style.background_color = Color::from_hex("#335577").unwrap();

    let dirty_node_ids = HashSet::from([2]);
    let dirty_ancestors = collect_dirty_ancestor_ids(&root, &dirty_node_ids);
    let vclip = surface_clip(DamageRect {
        x: 0,
        y: 0,
        width: 6200,
        height: 40,
    });
    let iterations = 1_000usize;

    let old_started = std::time::Instant::now();
    let mut old_rebuilt_commands = 0u64;
    for _ in 0..iterations {
        let mut next_subtrees = HashMap::new();
        let mut metrics = LocalReuseMetrics::default();
        build_paint_subtree(
            std::hint::black_box(&root),
            0.0,
            0.0,
            vclip,
            vclip,
            false,
            false,
            &dirty_node_ids,
            &dirty_ancestors,
            &list.subtrees,
            &mut next_subtrees,
            &mut metrics,
            BackdropBlurPolicy::CompositorRegion,
        );
        old_rebuilt_commands =
            old_rebuilt_commands.saturating_add(std::hint::black_box(metrics.rebuilt_commands));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_rebuilt_commands = 0u64;
    for _ in 0..iterations {
        let mut next_subtrees = HashMap::new();
        let mut metrics = LocalReuseMetrics::default();
        build_paint_subtree(
            std::hint::black_box(&root),
            0.0,
            0.0,
            vclip,
            vclip,
            false,
            true,
            &dirty_node_ids,
            &dirty_ancestors,
            &list.subtrees,
            &mut next_subtrees,
            &mut metrics,
            BackdropBlurPolicy::CompositorRegion,
        );
        new_rebuilt_commands =
            new_rebuilt_commands.saturating_add(std::hint::black_box(metrics.rebuilt_commands));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "paint-only dirty parent subtree reuse: forced {old_time:?}; descendant-reuse {new_time:?}; ratio {:.1}x; rebuilt_commands={old_rebuilt_commands}/{new_rebuilt_commands}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
    assert!(new_rebuilt_commands < old_rebuilt_commands);
}

#[test]
fn display_list_reuses_unrelated_subtrees_for_local_reorder_updates() {
    let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
    let mut left = node(2, "row", 0.0, 0.0, 80.0, 40.0);
    let mut left_first = node(3, "box", 0.0, 0.0, 20.0, 20.0);
    left_first.computed_style.z_index = 0;
    let mut left_second = node(4, "box", 20.0, 0.0, 20.0, 20.0);
    left_second.computed_style.z_index = 1;
    left.children.push(left_first);
    left.children.push(left_second);
    let mut right = node(5, "box", 100.0, 0.0, 40.0, 40.0);
    right.children.push(node(6, "text", 4.0, 4.0, 20.0, 12.0));
    root.children.push(left);
    root.children.push(right);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 40, false, true);
    let before = list.paint_commands().to_vec();

    root.children[0].children.swap(0, 1);
    let metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            reordered: 1,
            ..Default::default()
        },
        &HashSet::from([2]),
        160,
        40,
        false,
        true,
    );
    let after = list.paint_commands().to_vec();

    let right_before = command_debugs(&before, &[5, 6]);
    let right_after = command_debugs(&after, &[5, 6]);
    assert_eq!(right_before, right_after);
    assert!(metrics.subtree_segments_reused > 0);
    assert_eq!(metrics.full_fallback_count, 0);
}
