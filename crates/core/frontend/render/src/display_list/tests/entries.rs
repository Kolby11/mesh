use super::super::paint_node::*;
use super::super::*;
use super::common::*;
use crate::RenderObjectDirtySummary;
use mesh_core_elements::style::{
    BackgroundPaint, Color, Overflow, StyleImageSource, StyleLinearGradient,
};
use mesh_core_elements::{BoxShadow, VisualFilter, WidgetNode};

#[test]
fn display_list_reuses_unchanged_entries() {
    let root = node(1, "box", 0.0, 0.0, 100.0, 40.0);
    let mut list = RetainedDisplayList::default();

    let first = list.update(&root, 100, 40, false, false);
    assert_eq!(first.entries_rebuilt, 1);
    assert_eq!(first.entries_reused, 0);
    assert_eq!(first.damage_area, 4_000);

    let second = list.update(&root, 100, 40, false, false);
    assert_eq!(second.entries_rebuilt, 0);
    assert_eq!(second.entries_reused, 1);
    assert_eq!(second.damage_area, 0);
    assert_eq!(second.skipped_paint_pixels, 0);
}

#[test]
fn display_list_effect_rebuilds_when_background_paint_changes() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 40.0);
    let mut list = RetainedDisplayList::default();

    list.update(&root, 100, 40, false, false);
    root.computed_style.background_paint = BackgroundPaint::Image(StyleImageSource {
        path: "assets/first.png".to_string(),
    });
    let image_metrics = list.update(&root, 100, 40, false, false);
    assert_eq!(image_metrics.entries_rebuilt, 1);
    assert_eq!(image_metrics.entries_reused, 0);

    root.computed_style.background_paint = BackgroundPaint::LinearGradient(StyleLinearGradient {
        from: Color::from_hex("#112233").unwrap(),
        to: Color::from_hex("#445566").unwrap(),
    });
    let gradient_metrics = list.update(&root, 100, 40, false, false);
    assert_eq!(gradient_metrics.entries_rebuilt, 1);
    assert_eq!(gradient_metrics.entries_reused, 0);
}

#[test]
fn display_list_damages_old_and_new_bounds() {
    let mut root = node(1, "box", 0.0, 0.0, 20.0, 20.0);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, false, true);

    root.layout.x = 30.0;
    root.layout.y = 0.0;
    let metrics = list.update(&root, 100, 100, false, true);

    assert_eq!(metrics.entries_rebuilt, 1);
    assert_eq!(metrics.damage_area, 1_000);
    assert_eq!(metrics.skipped_paint_pixels, 9_000);
}

#[test]
fn display_list_preserves_disjoint_changed_entry_damage_rects() {
    let mut root = node(1, "row", 0.0, 0.0, 200.0, 40.0);
    root.children.push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
    root.children.push(node(3, "box", 160.0, 0.0, 20.0, 20.0));
    let mut list = RetainedDisplayList::default();
    list.update(&root, 200, 40, false, true);

    root.children[0].computed_style.background_color.r = 40;
    root.children[1].computed_style.background_color.r = 50;
    let metrics = list.update(&root, 200, 40, false, true);

    assert_eq!(metrics.entries_rebuilt, 2);
    assert_eq!(list.damage_rects().len(), 2);
    assert!(list.damage_rects().contains(&DamageRect {
        x: 0,
        y: 0,
        width: 20,
        height: 20,
    }));
    assert!(list.damage_rects().contains(&DamageRect {
        x: 160,
        y: 0,
        width: 20,
        height: 20,
    }));
}

#[test]
fn display_list_selects_blurred_background_outside_layout_bounds() {
    let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
    root.computed_style.filter = VisualFilter { blur_radius: 4.0 };

    let mut list = RetainedDisplayList::default();
    list.update(&root, 80, 80, false, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 10,
            y: 24,
            width: 2,
            height: 2,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );

    assert!(
        selected.iter().any(|command| command.node.id == 1),
        "blurred visual bounds should participate in sparse repaint selection"
    );
}

#[test]
fn display_list_effect_visual_clip_includes_shadow_and_filter_overflow() {
    let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
    root.computed_style.filter = VisualFilter { blur_radius: 4.0 };
    root.computed_style.box_shadow = BoxShadow {
        offset_x: 10.0,
        offset_y: 0.0,
        blur_radius: 4.0,
        spread_radius: 0.0,
        color: Color::from_hex("#00000080").unwrap(),
        inset: false,
    };

    let mut list = RetainedDisplayList::default();
    list.update(&root, 90, 90, false, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 50,
            y: 24,
            width: 2,
            height: 2,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );

    assert!(selected.iter().any(|command| command.node.id == 1));
}

#[test]
fn display_list_effect_visual_clip_includes_image_bounds() {
    let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
    root.computed_style.background_paint = BackgroundPaint::Image(StyleImageSource {
        path: "assets/panel.png".to_string(),
    });
    let paint_node = build_paint_node(&root, 0.0, 0.0);
    let visual = visual_clip_for(&paint_node);

    assert_eq!(visual.x, 20);
    assert_eq!(visual.y, 20);
    assert_eq!(visual.width, 20);
    assert_eq!(visual.height, 20);
}

#[test]
fn display_list_effect_visual_clip_includes_gradient_bounds() {
    let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
    root.computed_style.background_paint = BackgroundPaint::LinearGradient(StyleLinearGradient {
        from: Color::from_hex("#112233").unwrap(),
        to: Color::from_hex("#445566").unwrap(),
    });
    let paint_node = build_paint_node(&root, 0.0, 0.0);
    let visual = visual_clip_for(&paint_node);

    assert_eq!(visual.x, 20);
    assert_eq!(visual.y, 20);
    assert_eq!(visual.width, 20);
    assert_eq!(visual.height, 20);
}

#[test]
fn display_list_selects_box_shadow_outside_layout_bounds() {
    let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
    root.computed_style.box_shadow = BoxShadow {
        offset_x: 10.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        spread_radius: 0.0,
        color: Color::from_hex("#00000080").unwrap(),
        inset: false,
    };

    let mut list = RetainedDisplayList::default();
    list.update(&root, 80, 80, false, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 44,
            y: 24,
            width: 2,
            height: 2,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );

    assert!(
        selected.iter().any(|command| command.node.id == 1),
        "box-shadow visual bounds should participate in sparse repaint selection"
    );
}

#[test]
fn display_list_orders_commands_by_z_index_before_replay() {
    let mut root = node(1, "stack", 0.0, 0.0, 100.0, 100.0);
    let mut top = node(2, "box", 0.0, 0.0, 40.0, 40.0);
    top.computed_style.z_index = 10;
    let mut bottom = node(3, "box", 0.0, 0.0, 40.0, 40.0);
    bottom.computed_style.z_index = -1;
    root.children.push(top);
    root.children.push(bottom);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, false, false);
    let node_order: Vec<_> = list
        .paint_commands()
        .iter()
        .filter(|command| command.kind == DisplayPaintCommandKind::Node)
        .map(|command| command.node.id)
        .collect();

    assert_eq!(node_order, vec![1, 3, 2]);
}

#[test]
fn display_list_preclip_uses_visual_bounds_for_effect_overflow() {
    let mut root = node(1, "box", 0.0, 0.0, 40.0, 40.0);
    root.computed_style.overflow_x = Overflow::Hidden;
    root.computed_style.overflow_y = Overflow::Hidden;
    let mut child = node(2, "box", 48.0, 0.0, 10.0, 10.0);
    child.computed_style.box_shadow = BoxShadow {
        offset_x: -15.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        spread_radius: 0.0,
        color: Color::from_hex("#00000080").unwrap(),
        inset: false,
    };
    root.children.push(child);

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 100, false, false);

    assert!(
        list.paint_commands()
            .iter()
            .any(|command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node),
        "effect overflow intersecting a parent clip must not be preclipped by layout bounds"
    );
    assert_eq!(metrics.preclipped_descendants, 0);
    assert_eq!(metrics.effect_overflow_count, 1);
}

#[test]
fn display_list_profiles_changed_paint_layout_effect_overflow_and_fallbacks() {
    let mut root = node(1, "box", 20.0, 20.0, 20.0, 20.0);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, false, true);

    root.computed_style.box_shadow = BoxShadow {
        offset_x: 10.0,
        offset_y: 0.0,
        blur_radius: 2.0,
        spread_radius: 0.0,
        color: Color::from_hex("#00000080").unwrap(),
        inset: false,
    };
    let metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            material: 1,
            geometry: 1,
            ..Default::default()
        },
        &HashSet::from([1]),
        100,
        100,
        false,
        true,
    );

    assert_eq!(metrics.changed_paint_count, 1);
    assert_eq!(metrics.changed_layout_count, 1);
    assert_eq!(metrics.effect_overflow_count, 1);

    let fallback_metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            geometry: 1,
            ..Default::default()
        },
        &HashSet::new(),
        100,
        100,
        false,
        true,
    );
    assert_eq!(fallback_metrics.full_fallback_count, 1);
    assert_eq!(fallback_metrics.fallback_promotion_count, 1);
}

#[test]
fn display_list_records_removed_entry_damage() {
    let mut root = node(1, "box", 0.0, 0.0, 80.0, 20.0);
    root.children.push(node(2, "text", 10.0, 0.0, 20.0, 10.0));
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, false, false);

    root.children.clear();
    let metrics = list.update(&root, 100, 100, false, false);

    assert_eq!(metrics.entries_removed, 2);
    assert_eq!(metrics.damage_area, 200);
}

// cargo test -p mesh-core-render --release -- removal_scan_guard_skips_equal_key_sets --ignored --nocapture
#[test]
#[ignore = "release-only display-list removal scan microbenchmark"]
fn removal_scan_guard_skips_equal_key_sets() {
    fn old_removed_count(previous: &HashMap<u64, u64>, next: &HashMap<u64, u64>) -> usize {
        let mut removed = 0usize;
        for key in previous.keys() {
            if !next.contains_key(key) {
                removed += 1;
            }
        }
        removed
    }

    fn guarded_removed_count(
        previous: &HashMap<u64, u64>,
        next: &HashMap<u64, u64>,
        inserted: usize,
    ) -> usize {
        if inserted == 0 && previous.len() == next.len() {
            return 0;
        }
        old_removed_count(previous, next)
    }

    let previous = (0..1024_u64)
        .map(|index| (index, index.wrapping_mul(3)))
        .collect::<HashMap<_, _>>();
    let next = (0..1024_u64)
        .map(|index| (index, index.wrapping_mul(5)))
        .collect::<HashMap<_, _>>();
    let iterations = 200_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total +=
            old_removed_count(std::hint::black_box(&previous), std::hint::black_box(&next));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        new_total += guarded_removed_count(
            std::hint::black_box(&previous),
            std::hint::black_box(&next),
            std::hint::black_box(0),
        );
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "display-list removal scan: full previous-entry scan {old_time:?}; guarded skip {new_time:?}; ratio {:.1}x; counts={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time * 10 < old_time);
}

#[test]
fn display_list_clips_damage_to_surface() {
    let mut root = node(1, "box", 80.0, 80.0, 40.0, 40.0);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, false, false);

    root.layout.x = 90.0;
    let metrics = list.update(&root, 100, 100, false, false);

    assert_eq!(metrics.damage_area, 400);
}

#[test]
fn display_list_can_force_full_surface_damage() {
    let root = node(1, "box", 10.0, 10.0, 10.0, 10.0);
    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 50, true, false);

    assert!(metrics.full_surface_damage);
    assert_eq!(metrics.damage_area, 5_000);
}

#[test]
fn display_list_skips_rebuild_when_retained_generation_is_unchanged() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 40.0);
    root.computed_style.overflow_y = Overflow::Scroll;
    let mut list = RetainedDisplayList::default();

    let first = list.update_for_retained_generation(
        &root,
        1,
        RenderObjectDirtySummary {
            inserted: 1,
            ..Default::default()
        },
        &HashSet::from([1]),
        100,
        40,
        false,
        true,
    );
    assert_eq!(first.entries_rebuilt, 1);
    assert_eq!(list.paint_commands().len(), 2);

    let mut child = node(2, "text", 10.0, 0.0, 20.0, 10.0);
    child.computed_style.overflow_y = Overflow::Scroll;
    root.children.push(child);
    let skipped = list.update_for_retained_generation(
        &root,
        1,
        RenderObjectDirtySummary {
            inserted: 1,
            ..Default::default()
        },
        &HashSet::from([1]),
        100,
        40,
        true,
        true,
    );
    assert_eq!(skipped.entries_rebuilt, 0);
    assert_eq!(skipped.entries_reused, 1);
    assert_eq!(skipped.damage_area, 4_000);
    assert!(skipped.full_surface_damage);
    assert_eq!(
        list.paint_commands().len(),
        2,
        "paint command cache should be reused while retained generation is unchanged"
    );

    let rebuilt = list.update_for_retained_generation(
        &root,
        2,
        RenderObjectDirtySummary {
            inserted: 1,
            ..Default::default()
        },
        &HashSet::from([1]),
        100,
        40,
        false,
        true,
    );
    assert_eq!(rebuilt.entries_rebuilt, 2);
    assert_eq!(list.paint_commands().len(), 4);
}

#[test]
fn sparse_entry_patch_matches_full_collection_for_text_updates() {
    let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
    let mut unchanged = node(2, "text", 0.0, 0.0, 50.0, 20.0);
    unchanged
        .attributes
        .insert("content".into(), "unchanged".into());
    let mut changed = node(3, "text", 50.0, 0.0, 50.0, 20.0);
    changed.attributes.insert("content".into(), "before".into());
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

    root.children[1]
        .attributes
        .insert("content".into(), "after".into());
    let full_metrics = full.update(&root, 120, 40, false, true);
    let sparse_metrics = sparse.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            text: 1,
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
}

#[test]
fn text_only_updates_preserve_cached_blur_metadata() {
    let mut root = display_entry_benchmark_tree(8, 12);
    root.children[3].children[6]
        .computed_style
        .backdrop_filter
        .blur_radius = 8.0;
    let changed_id = root.children[5].children[4].id;
    let dirty_ids = HashSet::from([changed_id]);
    let mut retained = RetainedDisplayList::default();
    retained.update(&root, 144, 64, false, true);
    let expected_backdrop = retained.backdrop_filter_regions().to_vec();
    let expected_compositor = retained.blur_regions().to_vec();

    root.children[5].children[4]
        .attributes
        .insert("content".into(), "changed".into());
    retained.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            text: 1,
            ..Default::default()
        },
        &dirty_ids,
        144,
        64,
        false,
        true,
    );

    assert_eq!(retained.backdrop_filter_regions(), expected_backdrop);
    assert_eq!(retained.blur_regions(), expected_compositor);
}

#[test]
fn text_update_with_visibility_change_recomputes_blur_metadata() {
    let mut root = display_entry_benchmark_tree(8, 12);
    let blur_id = root.children[3].children[6].id;
    root.children[3].children[6]
        .computed_style
        .backdrop_filter
        .blur_radius = 8.0;
    let text_id = root.children[5].children[4].id;
    let dirty_ids = HashSet::from([blur_id, text_id]);
    let mut retained = RetainedDisplayList::default();
    retained.update(&root, 144, 64, false, true);
    assert!(!retained.blur_regions().is_empty());

    root.children[3].children[6]
        .attributes
        .insert("hidden".into(), "true".into());
    root.children[5].children[4]
        .attributes
        .insert("content".into(), "changed".into());
    retained.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            text: 1,
            ..Default::default()
        },
        &dirty_ids,
        144,
        64,
        false,
        true,
    );

    assert!(retained.blur_regions().is_empty());
}

#[test]
fn blur_metadata_reuse_rejects_every_sensitive_dirty_category() {
    assert!(dirty_summary_preserves_blur_metadata(
        RenderObjectDirtySummary {
            text: 1,
            accessibility: 1,
            ..Default::default()
        }
    ));
    for dirty in [
        RenderObjectDirtySummary {
            inserted: 1,
            ..Default::default()
        },
        RenderObjectDirtySummary {
            removed: 1,
            ..Default::default()
        },
        RenderObjectDirtySummary {
            reordered: 1,
            ..Default::default()
        },
        RenderObjectDirtySummary {
            transform: 1,
            ..Default::default()
        },
        RenderObjectDirtySummary {
            clip: 1,
            ..Default::default()
        },
        RenderObjectDirtySummary {
            opacity: 1,
            ..Default::default()
        },
        RenderObjectDirtySummary {
            geometry: 1,
            ..Default::default()
        },
        RenderObjectDirtySummary {
            material: 1,
            ..Default::default()
        },
        RenderObjectDirtySummary {
            primitive: 1,
            ..Default::default()
        },
    ] {
        assert!(!dirty_summary_preserves_blur_metadata(dirty));
    }
}

// cargo test -p mesh-core-render --release -- cached_blur_metadata_beats_text_update_rescans --ignored --nocapture
#[test]
#[ignore = "release-only text-update blur metadata benchmark"]
fn cached_blur_metadata_beats_text_update_rescans() {
    fn blur_benchmark_tree() -> WidgetNode {
        let mut root = display_entry_benchmark_tree(60, 20);
        for row in root.children.iter_mut().step_by(3) {
            row.children[10].computed_style.backdrop_filter.blur_radius = 8.0;
        }
        root
    }

    let iterations = 2_000_u64;
    let surface = DamageRect {
        x: 0,
        y: 0,
        width: 240,
        height: 480,
    };

    let mut rescanned_root = blur_benchmark_tree();
    let changed_id = rescanned_root.children[30].children[5].id;
    let dirty_ids = HashSet::from([changed_id]);
    let mut rescanned = RetainedDisplayList::default();
    rescanned.update(&rescanned_root, surface.width, surface.height, false, true);
    let mut cached_root = blur_benchmark_tree();
    let mut cached = RetainedDisplayList::default();
    cached.update(&cached_root, surface.width, surface.height, false, true);

    let mut rescan_time = std::time::Duration::ZERO;
    let mut cached_time = std::time::Duration::ZERO;
    let mut rescan_total = 0usize;
    let mut cached_total = 0usize;
    for generation in 0..iterations {
        rescanned_root.children[30].children[5]
            .attributes
            .insert("content".into(), generation.to_string());
        let rescan_started = std::time::Instant::now();
        rescanned.update_with_dirty_nodes(
            &rescanned_root,
            RenderObjectDirtySummary {
                text: 1,
                ..Default::default()
            },
            &dirty_ids,
            surface.width,
            surface.height,
            false,
            true,
        );
        let backdrop = compute_backdrop_regions(rescanned.paint_commands.as_ref(), surface);
        let compositor = backdrop_blur_regions_from_tree(&rescanned_root, 0.0, 0.0, surface);
        rescan_time += rescan_started.elapsed();
        rescan_total =
            rescan_total.wrapping_add(std::hint::black_box(backdrop.len() + compositor.len()));

        cached_root.children[30].children[5]
            .attributes
            .insert("content".into(), generation.to_string());
        let cached_started = std::time::Instant::now();
        cached.update_with_dirty_nodes(
            &cached_root,
            RenderObjectDirtySummary {
                text: 1,
                ..Default::default()
            },
            &dirty_ids,
            surface.width,
            surface.height,
            false,
            true,
        );
        cached_time += cached_started.elapsed();
        cached_total = cached_total.wrapping_add(std::hint::black_box(
            cached.backdrop_filter_regions().len() + cached.blur_regions().len(),
        ));
    }

    assert_eq!(rescan_total, cached_total);
    assert_eq!(
        rescanned.backdrop_filter_regions(),
        cached.backdrop_filter_regions()
    );
    assert_eq!(rescanned.blur_regions(), cached.blur_regions());
    let speedup = rescan_time.as_secs_f64() / cached_time.as_secs_f64();
    eprintln!(
        "text-only display updates over {iterations} 1,261-node blur-bearing frames: rescan {rescan_time:?}; cached metadata {cached_time:?}; ratio {speedup:.2}x"
    );
    println!("MESH_PERF metric=text_blur_metadata_cache_speedup value={speedup:.6}");
    assert!(
        cached_time * 100 < rescan_time * 98,
        "cached blur metadata should make text updates at least 1.02x faster"
    );
}
