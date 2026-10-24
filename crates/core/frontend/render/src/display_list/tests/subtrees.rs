use super::super::*;
use super::common::*;
use crate::RenderObjectDirtySummary;
use mesh_core_elements::Prop;
use mesh_core_elements::style::{Color, Overflow};
use mesh_core_elements::{NodeId, WidgetNode};

#[test]
fn scroll_scope_reuses_offscreen_content_commands_and_entries() {
    let mut root = node(1, "scroll-area", 4.0, 6.0, 120.0, 80.0);
    root.computed_style.overflow_y = Overflow::Hidden;
    root.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
        max_y: 720.0,
        content_width: 120.0,
        content_height: 800.0,
        ..Default::default()
    });
    for index in 0..40 {
        root.children.push(node(
            2 + index,
            "text",
            8.0,
            8.0 + index as f32 * 20.0,
            80.0,
            18.0,
        ));
    }
    let mut objects = crate::RenderObjectTree::default();
    objects.update(&root);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 100, true, true);
    let retained = Arc::clone(&list.subtrees[&30].commands);
    let entries = list.entries.clone();
    let original_plan = list.frame_paint_plan().clone();
    let generation = list.generation();
    assert!(list.paint_commands().iter().any(|cmd| cmd.node.id == 30));
    for offset in [20.0, 300.5, 12.25, 0.0] {
        root.scroll_metrics.as_mut().unwrap().y = offset;
        let dirty = objects.update(&root);
        assert_eq!(dirty.scroll, 1);
        assert_eq!(dirty.geometry, 0);
        let metrics = list.update_with_dirty_nodes(
            &root,
            dirty,
            objects.dirty_node_ids(),
            160,
            100,
            false,
            true,
        );
        assert!(Arc::ptr_eq(&retained, &list.subtrees[&30].commands));
        assert_eq!(list.entries, entries);
        assert_eq!(metrics.entries_rebuilt, 0);
        assert_eq!(metrics.subtree_commands_rebuilt, 3); // owner + push/pop
        assert!(metrics.damage_area > 0);
        assert_eq!(metrics.full_fallback_count, 0);
        assert_eq!(
            list.frame_paint_plan().scroll_scopes()[0].translation,
            mesh_core_elements::AffineTransform::translation(0.0, -offset)
        );
        assert!(
            list.frame_paint_plan()
                .transforms()
                .iter()
                .filter(|transform| transform.node_id != root.id)
                .all(|transform| transform.scroll_scope == Some(root.id))
        );
    }
    assert!(list.generation() > generation);
    assert_eq!(
        original_plan.scroll_scopes()[0].translation,
        mesh_core_elements::AffineTransform::translation(0.0, 0.0)
    );
}

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
    left.scroll_metrics.get_or_insert_default().x = 0.0;
    left.children.push(node(3, "text", 30.0, 4.0, 20.0, 12.0));
    let mut right = node(4, "box", 60.0, 0.0, 40.0, 40.0);
    right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
    root.children.push(left);
    root.children.push(right);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 120, 40, false, true);
    let before = list.paint_commands().to_vec();

    root.children[0].scroll_metrics.get_or_insert_default().x = 18.0;
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
fn display_list_rebuilds_descendants_only_when_a_moved_parent_defines_their_space() {
    for scaled in [false, true] {
        let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
        let mut panel = node(2, "box", 0.0, 0.0, 120.0, 40.0);
        if scaled {
            panel.computed_style.transform.scale_x = 1.5;
        }
        panel.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
        panel.children.push(node(4, "text", 30.0, 4.0, 20.0, 12.0));
        root.children.push(panel);

        let mut list = RetainedDisplayList::default();
        list.update(&root, 160, 40, false, true);

        root.children[0].layout.x = 8.0;
        root.children[0].layout.width = 112.0;
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

        // Absolute layout keeps unscaled children in their parent's parent
        // space; a scale pivots on the parent's box and moves them.
        assert_eq!(
            metrics.subtree_commands_rebuilt,
            if scaled { 4 } else { 1 },
            "scaled: {scaled}"
        );
        let mut full = RetainedDisplayList::default();
        full.update(&root, 160, 40, false, true);
        assert_eq!(
            format!("{:?}", list.paint_commands()),
            format!("{:?}", full.paint_commands()),
            "scaled: {scaled}"
        );
        assert_eq!(list.command_spans, full.command_spans, "scaled: {scaled}");
    }
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

/// The early-exit pre-clip must answer exactly what the full union walk did:
/// preclip only when the union of every visible node's visual bounds exists
/// and misses the clip.
#[test]
fn early_exit_preclip_matches_the_full_subtree_union() {
    use super::super::build::{should_preclip_child_subtree, subtree_bounds_at};

    let clip = DisplayListClip {
        x: 0,
        y: 40,
        width: 100,
        height: 20,
    };
    let reference = |child: &WidgetNode| {
        subtree_bounds_at(child, 0.0, 0.0).is_some_and(|bounds| !bounds.intersects_clip(clip))
    };

    // Layout is in surface coordinates. Entirely below the clip, with
    // descendants.
    let mut below = node(10, "column", 0.0, 80.0, 100.0, 20.0);
    below.children.push(node(11, "box", 0.0, 85.0, 50.0, 10.0));
    // Own box inside the clip.
    let inside = node(20, "box", 0.0, 45.0, 100.0, 10.0);
    // An empty own box whose descendants straddle the band: neither paints in
    // it, but their union does, so the walk must not prune it.
    let mut straddle = node(30, "column", 0.0, 0.0, 0.0, 0.0);
    straddle
        .children
        .push(node(31, "box", 0.0, 10.0, 50.0, 10.0));
    straddle
        .children
        .push(node(32, "box", 0.0, 80.0, 50.0, 10.0));
    // Own box above the clip, a deep descendant inside it.
    let mut deep = node(40, "column", 0.0, 0.0, 100.0, 10.0);
    let mut middle = node(41, "column", 0.0, 0.0, 100.0, 10.0);
    middle.children.push(node(42, "box", 0.0, 45.0, 10.0, 10.0));
    deep.children.push(middle);
    // A shadow reaching into the clip from outside.
    let mut shadowed = node(50, "box", 0.0, 20.0, 100.0, 10.0);
    shadowed.computed_style.box_shadow.blur_radius = 6.0;
    shadowed.computed_style.box_shadow.color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 128,
    };
    // Nothing visible at all.
    let empty = node(60, "box", 0.0, 0.0, 0.0, 0.0);

    for (child, expected) in [
        (&below, true),
        (&inside, false),
        (&straddle, false),
        (&deep, false),
        (&shadowed, false),
        (&empty, false),
    ] {
        assert_eq!(
            reference(child),
            expected,
            "reference for node {}",
            child.id
        );
        assert_eq!(
            should_preclip_child_subtree(child, 0.0, 0.0, clip),
            expected,
            "early-exit pre-clip for node {}",
            child.id
        );
    }
}

// cargo test -p mesh-core-render --release -- label_resize_scroll_viewport_damage_profile --ignored --nocapture
#[test]
#[ignore = "release-only scroll-viewport damage profile"]
fn label_resize_scroll_viewport_damage_profile() {
    // A settings-shaped surface: sidebar, page, and preview scroll areas,
    // plus a status label outside all three whose width changes every frame.
    fn scroll_area(id: NodeId, x: f32, width: f32) -> WidgetNode {
        let mut area = node(id, "scroll-area", x, 0.0, width, 440.0);
        area.computed_style.overflow_y = Overflow::Hidden;
        area.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
            max_y: 1160.0,
            content_width: width,
            content_height: 1600.0,
            ..Default::default()
        });
        for index in 0..80 {
            area.children.push(node(
                id * 1000 + index,
                "text",
                8.0,
                8.0 + index as f32 * 20.0,
                width - 16.0,
                18.0,
            ));
        }
        area
    }
    let mut root = node(1, "row", 0.0, 0.0, 960.0, 480.0);
    root.children.push(scroll_area(2, 0.0, 240.0));
    root.children.push(scroll_area(3, 240.0, 480.0));
    root.children.push(scroll_area(4, 720.0, 240.0));
    let mut label = node(5, "text", 8.0, 448.0, 120.0, 24.0);
    label.attributes.insert(Prop::Content, "status".into());
    root.children.push(label);

    let mut objects = crate::RenderObjectTree::default();
    objects.update(&root);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 960, 480, true, true);
    let frames = 200u64;
    let mut damaged = 0u64;
    let started = std::time::Instant::now();
    for frame in 0..frames {
        root.children[3].layout.width = 121.0 + (frame % 40) as f32;
        let dirty = objects.update(&root);
        assert!(dirty.geometry > 0);
        list.update_with_dirty_nodes(
            &root,
            dirty,
            objects.dirty_node_ids(),
            960,
            480,
            false,
            true,
        );
        damaged += list
            .damage_rects()
            .iter()
            .map(|rect| rect.area())
            .sum::<u64>();
    }
    let elapsed = started.elapsed();
    let per_frame = damaged as f64 / frames as f64;
    eprintln!(
        "{frames} label-resize frames: {per_frame:.0} damaged px/frame ({:.1}% of 960x480); update {elapsed:?}",
        per_frame / (960.0 * 480.0) * 100.0
    );
    let percent = per_frame / (960.0 * 480.0) * 100.0;
    println!("MESH_PERF metric=label_resize_viewport_damage_percent value={percent:.6}");
    assert!(
        percent < 2.0,
        "a label resize must not damage unrelated scroll viewports"
    );
}

#[test]
fn scroll_shift_plans_only_scopes_over_a_uniform_backdrop() {
    fn scene(beneath: Option<mesh_core_elements::style::BackgroundPaint>) -> WidgetNode {
        let mut root = node(1, "box", 0.0, 0.0, 200.0, 160.0);
        if let Some(paint) = beneath {
            root.computed_style.background_paint = paint;
        }
        let mut area = node(2, "scroll-area", 20.0, 20.0, 120.0, 100.0);
        area.computed_style.background_color = Color::TRANSPARENT;
        area.computed_style.overflow_y = Overflow::Hidden;
        area.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
            max_y: 300.0,
            content_width: 120.0,
            content_height: 400.0,
            ..Default::default()
        });
        for index in 0..20 {
            area.children.push(node(
                10 + index,
                "box",
                24.0,
                24.0 + index as f32 * 20.0,
                100.0,
                16.0,
            ));
        }
        root.children.push(area);
        root
    }
    let scroll = |root: &mut WidgetNode,
                  list: &mut RetainedDisplayList,
                  objects: &mut crate::RenderObjectTree,
                  y: f32| {
        root.children[0].scroll_metrics.as_mut().unwrap().y = y;
        let dirty = objects.update(root);
        list.update_with_dirty_nodes(root, dirty, objects.dirty_node_ids(), 200, 160, false, true);
    };

    let mut root = scene(None);
    let mut objects = crate::RenderObjectTree::default();
    objects.update(&root);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 200, 160, true, true);
    // Without a painted base no shift is planned.
    scroll(&mut root, &mut list, &mut objects, 4.0);
    assert!(list.scroll_shifts().is_empty());
    list.mark_painted();
    scroll(&mut root, &mut list, &mut objects, 12.0);
    let shifts = list.scroll_shifts();
    assert_eq!(shifts.len(), 1);
    assert_eq!(shifts[0].owner, 2);
    assert_eq!(shifts[0].scroll_delta, (0.0, 8.0));
    assert_eq!(
        shifts[0].viewport,
        DamageRect {
            x: 20,
            y: 20,
            width: 120,
            height: 100
        }
    );
    // A vertical move replays horizontal edge bands only.
    assert!(shifts[0].anchored.iter().all(|band| band.width == 120));

    // A gradient beneath varies across the viewport.
    let mut root = scene(Some(
        mesh_core_elements::style::BackgroundPaint::LinearGradient(Default::default()),
    ));
    let mut objects = crate::RenderObjectTree::default();
    objects.update(&root);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 200, 160, true, true);
    list.mark_painted();
    scroll(&mut root, &mut list, &mut objects, 8.0);
    assert!(list.scroll_shifts().is_empty());
}

#[test]
fn display_ancestors_visit_only_retained_paths() {
    let mut visits = Vec::new();
    for count in [16, 1024] {
        let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
        for index in 0..count {
            root.children
                .push(node(10 + index, "box", 0.0, 0.0, 1.0, 1.0));
        }
        let mut branch = node(2, "box", 0.0, 0.0, 20.0, 20.0);
        branch.children.push(node(3, "box", 0.0, 0.0, 10.0, 10.0));
        root.children.push(branch);
        let mut index = super::super::batch_index::BatchIndex::default();
        index.rebuild(&root, 0.0, 0.0, &HashMap::new());
        let dirty = HashSet::from([3]);
        let expected = collect_dirty_ancestor_ids(&root, &dirty);
        let scope = mesh_core_elements::frame_work::Scope::new(true);
        let mut actual = HashSet::new();
        assert!(index.collect_dirty_ancestors(&root, &dirty, &mut actual));
        assert_eq!(actual, expected);
        visits.push(scope.snapshot()["display_ancestors"]);
        drop(scope);
        root.children.swap(0, count as usize);
        assert!(!index.collect_dirty_ancestors(&root, &dirty, &mut actual));
    }
    assert_eq!(visits, vec![3, 3]);
}

#[test]
fn subtree_visual_bounds_clip_scrolled_content_and_preserve_outer_shadow() {
    let mut root = node(1, "popover", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.box_shadow = mesh_core_elements::BoxShadow {
        offset_y: 8.0,
        blur_radius: 12.0,
        color: Color::from_hex("#00000080").unwrap(),
        ..Default::default()
    };
    let expected = widget_visual_bounds(&root).unwrap();
    let mut viewport = node(2, "scroll-area", 10.0, 10.0, 80.0, 60.0);
    viewport.computed_style.overflow_y = Overflow::Auto;
    viewport.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
        max_y: 940.0,
        content_width: 80.0,
        content_height: 1000.0,
        ..Default::default()
    });
    let mut content = node(3, "column", 10.0, 10.0, 80.0, 1000.0);
    let mut tile = node(4, "button", 10.0, 10.0, 80.0, 40.0);
    tile.computed_style.transform.scale_x = 1.04;
    tile.computed_style.transform.scale_y = 1.04;
    content.children.push(tile);
    viewport.children.push(content);
    root.children.push(viewport);
    for offset in [0.0, 28.5, 400.0, 940.0, 0.0] {
        root.children[0].scroll_metrics.as_mut().unwrap().y = offset;
        assert_eq!(
            widget_subtree_visual_bounds(&root),
            Some(expected),
            "offset={offset}"
        );
    }
}

#[test]
fn subtree_visual_bounds_keep_fixed_descendants_outside_scroll_clip() {
    let mut root = node(1, "popover", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.transform.translate_x = 50.0;
    let mut viewport = node(2, "scroll-area", 10.0, 10.0, 80.0, 60.0);
    viewport.computed_style.overflow_y = Overflow::Hidden;
    viewport.scroll_metrics.get_or_insert_default().y = 500.0;
    let mut content = node(3, "column", 10.0, 10.0, 80.0, 1000.0);
    let mut fixed = node(4, "box", 300.0, 400.0, 10.0, 10.0);
    fixed.computed_style.position = mesh_core_elements::style::Position::Fixed;
    content.children.push(fixed);
    viewport.children.push(content);
    root.children.push(viewport);
    let bounds = widget_subtree_visual_bounds(&root).unwrap();
    assert_eq!(bounds.x, 50.0);
    assert_eq!(bounds.y, 0.0);
    assert_eq!(bounds.x + bounds.width, 310.0);
    assert_eq!(bounds.y + bounds.height, 410.0);
}

#[test]
fn text_command_patches_skip_siblings_and_preserve_effect_coverage() {
    for count in [16, 1024] {
        for effect in ["plain", "clip", "opacity", "filter"] {
            let mut root = node(1, "column", 0.0, 0.0, 200.0, 100.0);
            match effect {
                "clip" => {
                    root.computed_style.overflow_y = Overflow::Hidden;
                    root.computed_style.border_radius = mesh_core_elements::Corners::all(8.0);
                }
                "opacity" => root.computed_style.opacity = 0.5,
                "filter" => root.computed_style.filter.blur_radius = 3.0,
                _ => {}
            }
            let mut branch = node(2, "column", 0.0, 0.0, 200.0, 100.0);
            for index in 0..count {
                let mut text = node(3 + index, "text", 4.0, 4.0, 40.0, 20.0);
                text.attributes.insert(Prop::Content, "20%".into());
                branch.children.push(text);
            }
            root.children.push(branch);
            let mut objects = crate::RenderObjectTree::default();
            objects.update(&root);
            let mut list = RetainedDisplayList::default();
            list.update(&root, 200, 100, false, true);
            let original = list.paint_commands_snapshot();
            let original_debug = format!("{original:?}");
            for turn in 0..4 {
                let index = if turn % 2 == 0 { 0 } else { count as usize - 1 };
                root.children[0].children[index]
                    .attributes
                    .insert(Prop::Content, format!("{}%", 21 + turn));
                if turn == 2 {
                    root.children[0].children[count as usize - 1]
                        .attributes
                        .insert(Prop::Content, "second".into());
                }
                let dirty = objects.update(&root);
                let work = mesh_core_elements::frame_work::Scope::new(true);
                list.update_with_dirty_nodes(
                    &root,
                    dirty,
                    objects.dirty_node_ids(),
                    200,
                    100,
                    false,
                    true,
                );
                assert_eq!(
                    work.snapshot()["display_build"],
                    if turn == 2 { 2 } else { 1 },
                    "{effect}, {count}"
                );
                drop(work);
                let mut full = RetainedDisplayList::default();
                full.update(&root, 200, 100, false, true);
                assert_eq!(
                    format!("{:?}", list.paint_commands()),
                    format!("{:?}", full.paint_commands()),
                    "{effect}, {count}"
                );
                assert_eq!(list.command_spans, full.command_spans);
                assert_eq!(list.layer_scopes, full.layer_scopes);
                assert_eq!(list.filter_layer_regions, full.filter_layer_regions);
                assert_eq!(format!("{original:?}"), original_debug);
            }
            // Rebuilding an ancestor must retain the previously patched leaves.
            root.children[0].computed_style.background_color.r = 200;
            let dirty = objects.update(&root);
            list.update_with_dirty_nodes(
                &root,
                dirty,
                objects.dirty_node_ids(),
                200,
                100,
                false,
                true,
            );
            let mut full = RetainedDisplayList::default();
            full.update(&root, 200, 100, false, true);
            assert_eq!(
                format!("{:?}", list.paint_commands()),
                format!("{:?}", full.paint_commands())
            );
        }
    }
}

/// Geometry-changing patches replace the changed commands and update ancestor
/// coverage without rebuilding siblings, and match a full build exactly.
#[test]
fn geometry_command_patches_skip_siblings_and_match_full_build() {
    let mut by_count = Vec::new();
    for count in [16, 1024] {
        for effect in ["plain", "clip", "opacity"] {
            let mut root = node(1, "column", 0.0, 0.0, 200.0, 100.0);
            match effect {
                "clip" => root.computed_style.overflow_y = Overflow::Hidden,
                "opacity" => root.computed_style.opacity = 0.5,
                _ => {}
            }
            let mut branch = node(2, "column", 0.0, 0.0, 200.0, 100.0);
            for index in 0..count {
                let y = (index % 25) as f32 * 4.0;
                branch
                    .children
                    .push(node(3 + index, "box", 0.0, y, 10.0, 2.0));
            }
            root.children.push(branch);
            let mut objects = crate::RenderObjectTree::default();
            objects.update(&root);
            let mut list = RetainedDisplayList::default();
            list.update(&root, 200, 100, false, true);
            let middle = count as usize / 2;
            let mut visits = Vec::new();
            for turn in 0..6 {
                let item = &mut root.children[0].children[middle];
                match turn % 3 {
                    0 => item.layout.width = 40.0,
                    1 => item.layout.width = 10.0,
                    _ => item.layout.height = 3.0 - item.layout.height + 2.0,
                }
                let dirty = objects.update(&root);
                let work = mesh_core_elements::frame_work::Scope::new(true);
                let metrics = list.update_with_dirty_nodes(
                    &root,
                    dirty,
                    objects.dirty_node_ids(),
                    200,
                    100,
                    false,
                    true,
                );
                let counts = work.snapshot();
                drop(work);
                let mut full = RetainedDisplayList::default();
                full.update(&root, 200, 100, false, true);
                assert_eq!(
                    format!("{:?}", list.paint_commands()),
                    format!("{:?}", full.paint_commands()),
                    "{effect}, {count}, {turn}"
                );
                assert_eq!(
                    list.command_spans, full.command_spans,
                    "{effect}, {count}, {turn}"
                );
                assert_eq!(list.layer_scopes, full.layer_scopes);
                assert_eq!(list.filter_layer_regions, full.filter_layer_regions);
                for id in [1, 2, 3 + middle as u64] {
                    let (patched, rebuilt) = (
                        list.subtrees[&id].command_span.unwrap(),
                        full.subtrees[&id].command_span.unwrap(),
                    );
                    assert_eq!(
                        patched.bounds, rebuilt.bounds,
                        "{effect}, {count}, {turn}, {id}"
                    );
                    assert_eq!(patched.local_bounds, rebuilt.local_bounds);
                    assert_eq!(patched.child_bounds, rebuilt.child_bounds);
                }
                assert!(metrics.damage_rect.width > 0, "{effect}, {count}, {turn}");
                if effect == "plain" {
                    assert_eq!(counts["display_build"], 1, "{count}, {turn}: {counts:?}");
                    assert_eq!(counts["display_entries"], 0, "{count}, {turn}: {counts:?}");
                    visits.push((turn, counts["display_ancestors"]));
                }
            }
            if effect == "plain" {
                by_count.push(visits);
            }
        }
    }
    // Growth and edge-free changes are constant; only a shrink of the edge
    // that defined the parent's union re-unions that parent's children.
    for (small, large) in by_count[0].iter().zip(&by_count[1]) {
        if small.0 % 3 == 1 {
            assert!(large.1 > small.1, "{small:?} {large:?}");
        } else {
            assert_eq!(small, large);
        }
    }
}
