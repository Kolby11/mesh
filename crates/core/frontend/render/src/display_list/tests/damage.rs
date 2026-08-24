use super::super::*;
use super::common::*;
use crate::RenderObjectDirtySummary;
use mesh_core_elements::style::{Color, Overflow, Visibility};
use mesh_core_elements::{NodeId, VisualFilter, WidgetNode};
use std::sync::Arc;

#[test]
fn display_list_records_span_metadata_and_policy_labels() {
    let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
    let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
    left.computed_style.overflow_y = Overflow::Scroll;
    left.children.push(node(3, "text", 4.0, 4.0, 20.0, 12.0));
    let mut right = node(4, "box", 70.0, 0.0, 40.0, 40.0);
    right.children.push(node(5, "text", 4.0, 4.0, 20.0, 12.0));
    root.children.push(left);
    root.children.push(right);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 120, 40, false, true);

    assert_eq!(
        list.command_spans.as_ref(),
        build_command_spans(&root, &list.subtrees)
    );

    let left_spans: Vec<_> = list
        .command_spans
        .iter()
        .filter(|span| span.owner == 2)
        .collect();
    let left_span = left_spans
        .first()
        .expect("left retained subtree should have command span metadata");
    assert!(left_span.start < left_span.end);
    assert!(
        left_spans
            .iter()
            .all(|span| span.end.saturating_sub(span.start) == span.command_count)
    );
    assert!(left_spans.iter().any(|span| span.includes_scrollbars));
    let left_total_commands: usize = left_spans.iter().map(|span| span.command_count).sum();
    assert_eq!(left_total_commands, 2);
    assert_eq!(
        DisplayListRepaintPolicy::MinimalDamage.as_str(),
        "minimal_damage"
    );
    assert_eq!(
        DisplayListRepaintPolicy::BoundingRect.as_str(),
        "bounding_rect"
    );
    assert_eq!(
        DisplayListRepaintPolicy::FullSurface.as_str(),
        "full_surface"
    );
}

#[test]
fn display_list_filters_sparse_damage_without_reordering_commands() {
    let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
    let mut left = node(2, "box", 0.0, 0.0, 40.0, 40.0);
    left.computed_style.overflow_y = Overflow::Scroll;
    root.children.push(left);
    root.children.push(node(3, "box", 80.0, 0.0, 40.0, 40.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 40, false, true);
    let full_order: Vec<_> = list
        .paint_commands()
        .iter()
        .map(|command| (command.node.id, command.kind))
        .collect();

    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 45,
            height: 40,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let filtered_order: Vec<_> = selected
        .iter()
        .map(|command| (command.node.id, command.kind))
        .collect();

    assert!(selected.metrics().filtered_command_count < full_order.len() as u64);
    assert!(selected.metrics().filtered_commands_skipped > 0);
    assert!(selected.metrics().filtered_span_count > 0);
    assert!(
        filtered_order
            .iter()
            .any(|item| *item == (2, DisplayPaintCommandKind::Scrollbars))
    );
    let projected_full: Vec<_> = full_order
        .into_iter()
        .filter(|item| filtered_order.contains(item))
        .collect();
    assert_eq!(filtered_order, projected_full);
}

#[test]
fn display_list_partial_damage_replays_intersecting_backgrounds() {
    let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
    root.children.push(node(2, "box", 0.0, 0.0, 40.0, 40.0));
    root.children.push(node(3, "box", 80.0, 0.0, 40.0, 40.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 40, false, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 88,
            y: 8,
            width: 12,
            height: 12,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let ids: Vec<_> = selected.iter().map(|command| command.node.id).collect();

    assert!(
        ids.contains(&1),
        "partial repaint must replay root background under damaged child pixels"
    );
    assert!(ids.contains(&3), "damaged child command should be selected");
}

#[test]
fn display_list_full_surface_policy_keeps_all_commands_and_records_fallback() {
    let mut root = node(1, "row", 0.0, 0.0, 120.0, 40.0);
    root.children.push(node(2, "box", 0.0, 0.0, 40.0, 40.0));
    root.children.push(node(3, "box", 70.0, 0.0, 40.0, 40.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 120, 40, false, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    assert_eq!(selected.len(), list.paint_commands().len());
    assert_eq!(
        selected.metrics().repaint_policy,
        DisplayListRepaintPolicy::FullSurface
    );
    assert_eq!(selected.metrics().filtered_commands_skipped, 0);
    assert_eq!(selected.metrics().filtered_fallback_count, 1);
}

#[test]
fn display_list_select_paint_commands_for_rects_matches_expected_commands() {
    let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
    root.children.push(node(2, "box", 0.0, 0.0, 40.0, 40.0));
    root.children.push(node(3, "box", 80.0, 0.0, 40.0, 40.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 40, false, true);
    let selected_left = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 45,
            height: 40,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let selected_right = list.select_paint_commands(
        Some(DamageRect {
            x: 80,
            y: 0,
            width: 40,
            height: 40,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let selected_multi = list.select_paint_commands_for_rects(
        &[
            DamageRect {
                x: 0,
                y: 0,
                width: 45,
                height: 40,
            },
            DamageRect {
                x: 80,
                y: 0,
                width: 40,
                height: 40,
            },
        ],
        DisplayListRepaintPolicy::MinimalDamage,
    );

    let multi_ids: Vec<_> = selected_multi
        .iter()
        .map(|command| command.node.id)
        .collect();

    assert!(multi_ids.contains(&1));
    assert!(multi_ids.contains(&2));
    assert!(multi_ids.contains(&3));
    assert!(selected_multi.len() >= selected_left.len());
    assert!(selected_multi.len() >= selected_right.len());

    drop(selected_multi);
    drop(selected_right);
    drop(selected_left);
    list.command_spans = Vec::new().into();
    let fallback = list.select_paint_commands_for_rects(
        &[
            DamageRect {
                x: 0,
                y: 0,
                width: 45,
                height: 40,
            },
            DamageRect {
                x: 80,
                y: 0,
                width: 40,
                height: 40,
            },
        ],
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let fallback_ids: Vec<_> = fallback.iter().map(|command| command.node.id).collect();
    assert_eq!(fallback.metrics().filtered_span_count, 0);
    assert!(fallback_ids.contains(&1));
    assert!(fallback_ids.contains(&2));
    assert!(fallback_ids.contains(&3));
}

#[test]
fn display_list_select_paint_commands_for_rects_single_rect_delegates() {
    let mut root = node(1, "row", 0.0, 0.0, 100.0, 40.0);
    root.children.push(node(2, "box", 0.0, 0.0, 40.0, 40.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 40, false, true);
    let damage = DamageRect {
        x: 0,
        y: 0,
        width: 20,
        height: 20,
    };
    let selected_single =
        list.select_paint_commands(Some(damage), DisplayListRepaintPolicy::MinimalDamage);
    let selected_multi =
        list.select_paint_commands_for_rects(&[damage], DisplayListRepaintPolicy::MinimalDamage);

    assert_eq!(selected_single.len(), selected_multi.len());
    assert_eq!(
        selected_single.metrics().filtered_command_count,
        selected_multi.metrics().filtered_command_count
    );
    assert_eq!(
        selected_single.metrics().filtered_span_count,
        selected_multi.metrics().filtered_span_count
    );
    assert_eq!(
        selected_single.metrics().filtered_commands_skipped,
        selected_multi.metrics().filtered_commands_skipped
    );
}

#[test]
fn display_list_falls_back_for_ambiguous_dirty_summaries() {
    let root = node(1, "box", 0.0, 0.0, 100.0, 40.0);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 40, false, true);

    let metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            geometry: 1,
            ..Default::default()
        },
        &HashSet::new(),
        100,
        40,
        false,
        true,
    );

    assert_eq!(metrics.full_fallback_count, 1);
    assert_eq!(metrics.broad_dirty_fallback_count, 0);
}

#[test]
fn display_list_batches_adjacent_compatible_primitives() {
    let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    root.children.push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
    root.children.push(node(3, "box", 20.0, 0.0, 20.0, 20.0));
    let mut list = RetainedDisplayList::default();

    let metrics = list.update(&root, 100, 20, false, false);

    assert_eq!(metrics.batch_count, 1);
    assert_eq!(metrics.batched_primitives, 3);
    assert_eq!(metrics.barrier_count, 0);
}

#[test]
fn display_list_records_batch_barriers() {
    let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    root.children.push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
    let mut text = node(3, "text", 20.0, 0.0, 20.0, 20.0);
    text.attributes.insert("content".into(), "hello".into());
    root.children.push(text);
    let mut clipped = node(4, "box", 40.0, 0.0, 20.0, 20.0);
    clipped.computed_style.overflow_x = Overflow::Hidden;
    root.children.push(clipped);
    let mut list = RetainedDisplayList::default();

    let metrics = list.update(&root, 100, 20, false, false);

    assert_eq!(metrics.barriers.text, 1);
    assert_eq!(metrics.barriers.clip, 1);
    assert_eq!(metrics.barrier_count, 2);
}

#[test]
fn display_list_keeps_opaque_backgrounds_batchable_and_translucent_backgrounds_conservative() {
    let mut opaque_root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    opaque_root
        .children
        .push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
    let mut opaque_list = RetainedDisplayList::default();

    let opaque_metrics = opaque_list.update(&opaque_root, 100, 20, false, false);

    assert_eq!(opaque_metrics.barriers.translucency, 0);
    assert_eq!(opaque_metrics.barrier_count, 0);
    assert_eq!(opaque_metrics.batch_count, 1);

    let mut translucent_root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    let mut translucent = node(2, "box", 0.0, 0.0, 20.0, 20.0);
    translucent.computed_style.background_color.a = 128;
    translucent_root.children.push(translucent);
    let mut translucent_list = RetainedDisplayList::default();

    let translucent_metrics = translucent_list.update(&translucent_root, 100, 20, false, false);

    assert_eq!(translucent_metrics.barriers.translucency, 1);
    assert_eq!(translucent_metrics.barrier_count, 1);
    assert_eq!(translucent_metrics.batch_count, 0);
}

#[test]
fn display_list_uses_cached_icon_opacity_for_conservative_barriers() {
    let td = tempfile::tempdir().unwrap();
    let opaque_path = td.path().join("opaque.png");
    let translucent_path = td.path().join("translucent.png");
    image::ImageBuffer::from_fn(2, 2, |_, _| image::Rgba([255u8, 0, 0, 255]))
        .save(&opaque_path)
        .unwrap();
    image::ImageBuffer::from_fn(2, 2, |x, _| {
        if x == 0 {
            image::Rgba([255u8, 0, 0, 255])
        } else {
            image::Rgba([255u8, 0, 0, 96])
        }
    })
    .save(&translucent_path)
    .unwrap();

    let mut buffer = crate::surface::PixelBuffer::new(16, 16);
    let tint = Color::WHITE;
    crate::surface::icon::draw_icon_from_path(&mut buffer, &opaque_path, 0, 0, 10, 10, tint);
    crate::surface::icon::draw_icon_from_path(&mut buffer, &translucent_path, 0, 0, 10, 10, tint);
    assert_eq!(
        crate::surface::icon::cached_file_resource_opacity(&opaque_path, 10, 10, tint, false),
        crate::surface::icon::CachedResourceOpacity::Opaque
    );
    assert_eq!(
        crate::surface::icon::cached_file_resource_opacity(&translucent_path, 10, 10, tint, false),
        crate::surface::icon::CachedResourceOpacity::Translucent
    );

    let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    let mut opaque = node(2, "icon", 0.0, 0.0, 10.0, 10.0);
    opaque.computed_style.background_color = Color::TRANSPARENT;
    opaque.computed_style.color = tint;
    opaque
        .attributes
        .insert("src".into(), opaque_path.to_string_lossy().into_owned());
    let mut translucent = node(3, "icon", 12.0, 0.0, 10.0, 10.0);
    translucent.computed_style.background_color = Color::TRANSPARENT;
    translucent.computed_style.color = tint;
    translucent.attributes.insert(
        "src".into(),
        translucent_path.to_string_lossy().into_owned(),
    );
    let mut unknown = node(4, "icon", 24.0, 0.0, 10.0, 10.0);
    unknown.computed_style.background_color = Color::TRANSPARENT;
    unknown.computed_style.color = tint;
    unknown.attributes.insert(
        "src".into(),
        td.path().join("missing.png").to_string_lossy().into_owned(),
    );
    root.children.push(opaque);
    root.children.push(translucent);
    root.children.push(unknown);

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 20, false, false);

    assert_eq!(metrics.barriers.icon, 1);
    assert_eq!(metrics.barriers.translucency, 1);
    assert_eq!(metrics.barriers.opacity, 0);

    let mut transparent_root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    transparent_root.computed_style.background_color = Color::TRANSPARENT;
    let mut transparent_icon = node(2, "icon", 0.0, 0.0, 10.0, 10.0);
    transparent_icon.computed_style.background_color = Color::TRANSPARENT;
    transparent_icon.computed_style.color = tint;
    transparent_icon.computed_style.opacity = 0.5;
    transparent_icon
        .attributes
        .insert("src".into(), opaque_path.to_string_lossy().into_owned());
    transparent_root.children.push(transparent_icon);

    let mut transparent_list = RetainedDisplayList::default();
    let transparent_metrics = transparent_list.update(&transparent_root, 100, 20, false, false);

    assert_eq!(transparent_metrics.barriers.opacity, 1);
    assert_eq!(transparent_metrics.barriers.icon, 0);
}

#[test]
fn display_list_rebuilds_when_slider_value_changes() {
    let mut root = node(1, "slider", 0.0, 0.0, 100.0, 20.0);
    root.attributes.insert("min".into(), "0".into());
    root.attributes.insert("max".into(), "100".into());
    root.attributes.insert("value".into(), "25".into());
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 20, false, true);

    root.attributes.insert("value".into(), "75".into());
    let metrics = list.update(&root, 100, 20, false, true);

    assert_eq!(metrics.entries_rebuilt, 1);
    assert_eq!(metrics.damage_area, 2_000);
}

#[test]
fn display_list_rebuilds_when_border_width_changes() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 20.0);
    root.computed_style.border_color = Color::WHITE;
    root.computed_style.border_width = mesh_core_elements::style::Edges::all(1.0);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 20, false, true);

    root.computed_style.border_width = mesh_core_elements::style::Edges::all(4.0);
    let metrics = list.update(&root, 100, 20, false, true);

    assert_eq!(metrics.entries_rebuilt, 2);
    assert_eq!(metrics.damage_area, 2_000);
}

#[test]
fn display_list_stores_compact_paint_payloads() {
    let mut root = node(1, "box", 10.0, 20.0, 80.0, 30.0);
    root.computed_style.transform.translate_x = 5.0;
    root.computed_style.transform.translate_y = 7.0;
    root.computed_style.overflow_x = Overflow::Scroll;
    root.attributes
        .insert("_mesh_scroll_max_x".into(), "40".into());
    root.attributes
        .insert("_mesh_content_width".into(), "120".into());

    let mut text = node(2, "text", 20.0, 30.0, 20.0, 10.0);
    text.attributes.insert("content".into(), "hello".into());
    text.attributes
        .insert("_mesh_selection_background".into(), "#112233".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ddeeff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "2".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "3".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "8".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "9".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "1".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "1".into());
    root.children.push(text);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, false, false);

    let root_command = list
        .paint_commands()
        .iter()
        .find(|command| command.node.id == 1 && command.kind == DisplayPaintCommandKind::Node)
        .expect("root command");
    assert_eq!(root_command.node.layout.x, 15.0);
    assert_eq!(root_command.node.layout.y, 27.0);
    assert_eq!(root_command.node.scrollbars.max_x, 40.0);
    assert_eq!(root_command.node.scrollbars.content_width, 120.0);

    let text_command = list
        .paint_commands()
        .iter()
        .find(|command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node)
        .expect("text command");
    match &text_command.node.content {
        DisplayPaintContent::Text(text) => {
            assert_eq!(text.text.as_ref(), "hello");
            let selection = text.selection.expect("selection payload");
            assert_eq!(selection.anchor_x, 2.0);
            assert_eq!(selection.focus_y, 9.0);
        }
        other => panic!("expected text paint payload, got {other:?}"),
    }
}

#[test]
fn display_list_omits_explicitly_hidden_descendants() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    let mut hidden = node(2, "box", 10.0, 10.0, 20.0, 20.0);
    hidden.computed_style.visibility = Visibility::Hidden;
    hidden.children.push(node(3, "text", 0.0, 0.0, 10.0, 10.0));
    root.children.push(hidden);

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 100, false, false);

    assert!(
        list.paint_commands()
            .iter()
            .all(|command| command.node.id != 2 && command.node.id != 3)
    );
    assert_eq!(metrics.omitted_subtrees, 1);
    assert_eq!(metrics.omitted_nodes, 2);
    assert_eq!(metrics.omitted_commands, 4);
}

#[test]
fn display_list_keeps_plain_opacity_zero_nodes_paintable() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    let mut transparent = node(2, "box", 10.0, 10.0, 20.0, 20.0);
    transparent.computed_style.opacity = 0.0;
    root.children.push(transparent);

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 100, false, false);

    assert!(
        list.paint_commands()
            .iter()
            .any(|command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node)
    );
    assert_eq!(metrics.omitted_subtrees, 0);
    assert_eq!(metrics.omitted_nodes, 0);
}

#[test]
fn display_list_keeps_disabled_and_inert_nodes_paintable() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 60.0);
    let mut disabled = node(2, "button", 4.0, 4.0, 30.0, 20.0);
    disabled.attributes.insert("disabled".into(), "true".into());
    let mut inert = node(3, "box", 44.0, 4.0, 30.0, 20.0);
    inert.attributes.insert("inert".into(), "true".into());
    root.children.push(disabled);
    root.children.push(inert);

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 60, false, false);

    assert!(
        list.paint_commands()
            .iter()
            .any(|command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node)
    );
    assert!(
        list.paint_commands()
            .iter()
            .any(|command| command.node.id == 3 && command.kind == DisplayPaintCommandKind::Node)
    );
    assert_eq!(metrics.omitted_subtrees, 0);
}

#[test]
fn display_list_preclips_fully_out_of_viewport_descendants() {
    let mut root = node(1, "box", 0.0, 0.0, 40.0, 40.0);
    root.computed_style.overflow_x = Overflow::Hidden;
    root.computed_style.overflow_y = Overflow::Hidden;
    let child = node(2, "box", 60.0, 0.0, 20.0, 20.0);
    root.children.push(child);

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 100, false, false);

    assert!(
        list.paint_commands()
            .iter()
            .all(|command| command.node.id != 2),
        "fully out-of-viewport descendants should be omitted before paint traversal"
    );
    assert_eq!(metrics.omitted_subtrees, 1);
    assert_eq!(metrics.omitted_nodes, 1);
    assert_eq!(metrics.preclipped_descendants, 1);
}

#[test]
fn display_list_keeps_partially_intersecting_descendants_paintable() {
    let mut root = node(1, "box", 0.0, 0.0, 40.0, 40.0);
    root.computed_style.overflow_x = Overflow::Hidden;
    root.computed_style.overflow_y = Overflow::Hidden;
    let child = node(2, "box", 30.0, 0.0, 20.0, 20.0);
    root.children.push(child);

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 100, false, false);

    assert!(
        list.paint_commands()
            .iter()
            .any(|command| command.node.id == 2 && command.kind == DisplayPaintCommandKind::Node)
    );
    assert_eq!(metrics.omitted_subtrees, 0);
    assert_eq!(metrics.preclipped_descendants, 0);
}

// cargo test -p mesh-core-render --release -- display_entry_scratch_reuse_beats_fresh_allocations_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only display-list scratch allocation microbenchmark"]
fn display_entry_scratch_reuse_beats_fresh_allocations_benchmark() {
    let tree = display_entry_benchmark_tree(120, 20);
    let iterations = 2_000;

    let old_start = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let mut ordered_entries = Vec::new();
        let mut next = HashMap::new();
        collect_display_entries(&tree, 0.0, 0.0, Some(&mut ordered_entries), None, &mut next);
        old_total = old_total
            .saturating_add(std::hint::black_box(ordered_entries.len()))
            .saturating_add(std::hint::black_box(next.len()));
    }
    let old_elapsed = old_start.elapsed();

    let new_start = std::time::Instant::now();
    let mut new_total = 0usize;
    let mut ordered_entries = Vec::new();
    let mut next = HashMap::new();
    for _ in 0..iterations {
        ordered_entries.clear();
        next.clear();
        collect_display_entries(&tree, 0.0, 0.0, Some(&mut ordered_entries), None, &mut next);
        new_total = new_total
            .saturating_add(std::hint::black_box(ordered_entries.len()))
            .saturating_add(std::hint::black_box(next.len()));
    }
    let new_elapsed = new_start.elapsed();

    assert_eq!(old_total, new_total);
    println!(
        "display entry collection over {iterations} iterations: fresh allocations {:?}, scratch reuse {:?}",
        old_elapsed, new_elapsed
    );
    assert!(
        new_elapsed < old_elapsed,
        "scratch reuse should be faster than fresh allocations"
    );
}

// cargo test -p mesh-core-render --release -- release_entry_collection_skips_debug_ordered_sink --ignored --nocapture
#[test]
#[ignore = "release-only display-list debug sink microbenchmark"]
fn release_entry_collection_skips_debug_ordered_sink() {
    let tree = display_entry_benchmark_tree(120, 20);
    let iterations = 2_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0_usize;
    let mut ordered_entries = Vec::new();
    let mut old_next = HashMap::new();
    for _ in 0..iterations {
        ordered_entries.clear();
        old_next.clear();
        collect_display_entries(
            &tree,
            0.0,
            0.0,
            Some(&mut ordered_entries),
            None,
            &mut old_next,
        );
        old_total = old_total.saturating_add(std::hint::black_box(old_next.len()));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0_usize;
    let mut new_next = HashMap::new();
    for _ in 0..iterations {
        new_next.clear();
        collect_display_entries(&tree, 0.0, 0.0, None, None, &mut new_next);
        new_total = new_total.saturating_add(std::hint::black_box(new_next.len()));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "display entry debug sink: ordered {old_time:?}; release sink omitted {new_time:?}; ratio {:.2}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-render --release -- retained_effect_count_beats_command_scan --ignored --nocapture
#[test]
#[ignore = "release-only retained effect-count microbenchmark"]
fn retained_effect_count_beats_command_scan() {
    let mut tree = display_entry_benchmark_tree(120, 20);
    for (index, child) in tree.children.iter_mut().enumerate() {
        if index % 8 == 0 {
            child.computed_style.box_shadow.blur_radius = 4.0;
            child.computed_style.box_shadow.color = Color::BLACK;
        }
    }
    let mut list = RetainedDisplayList::default();
    list.update(&tree, 4096, 4096, false, false);
    let retained_count = list
        .subtrees
        .get(&tree.id)
        .expect("retained root subtree")
        .effect_overflow_count;
    assert!(retained_count > 0);
    assert_eq!(
        retained_count,
        count_effect_overflow_commands(list.paint_commands.as_ref())
    );
    let iterations = 20_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0_u64;
    for _ in 0..iterations {
        old_total = old_total.wrapping_add(std::hint::black_box(count_effect_overflow_commands(
            std::hint::black_box(list.paint_commands.as_ref()),
        )));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0_u64;
    for _ in 0..iterations {
        new_total = new_total.wrapping_add(std::hint::black_box(retained_count));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "effect overflow metric: command scan {old_time:?}; retained count {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-render --release -- retained_command_spans_beat_tree_walk --ignored --nocapture
#[test]
#[ignore = "release-only retained command-span microbenchmark"]
fn retained_command_spans_beat_tree_walk() {
    let tree = display_entry_benchmark_tree(120, 20);
    let mut list = RetainedDisplayList::default();
    list.update(&tree, 4096, 4096, false, false);
    let traversed = build_command_spans(&tree, &list.subtrees);
    assert_eq!(list.command_spans.as_ref(), traversed);
    let iterations = 10_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0_usize;
    for _ in 0..iterations {
        old_total = old_total.saturating_add(std::hint::black_box(
            build_command_spans(std::hint::black_box(&tree), &list.subtrees).len(),
        ));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0_usize;
    for _ in 0..iterations {
        let spans = std::hint::black_box(Arc::clone(&list.command_spans));
        new_total = new_total.saturating_add(std::hint::black_box(spans.len()));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "command span assembly: tree walk {old_time:?}; retained root handle {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-render --release -- single_root_command_span_assembly_beats_ancestor_copying --ignored --nocapture
#[test]
#[ignore = "release-only retained command-span assembly microbenchmark"]
fn single_root_command_span_assembly_beats_ancestor_copying() {
    let tree = display_entry_benchmark_tree(120, 20);
    let mut list = RetainedDisplayList::default();
    list.update(&tree, 4096, 4096, false, false);

    let copied = build_command_spans_with_ancestor_copying(&tree, &list.subtrees);
    let assembled = build_command_spans(&tree, &list.subtrees);
    assert_eq!(copied, assembled);

    let iterations = 1_000;
    let old_started = std::time::Instant::now();
    let mut old_total = 0_usize;
    for _ in 0..iterations {
        old_total = old_total.saturating_add(std::hint::black_box(
            build_command_spans_with_ancestor_copying(
                std::hint::black_box(&tree),
                std::hint::black_box(&list.subtrees),
            )
            .len(),
        ));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0_usize;
    for _ in 0..iterations {
        new_total = new_total.saturating_add(std::hint::black_box(
            build_command_spans(
                std::hint::black_box(&tree),
                std::hint::black_box(&list.subtrees),
            )
            .len(),
        ));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "command span construction: ancestor-copying {old_time:?}; single-root {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time * 10 < old_time * 9);
}

fn frosted_node(id: NodeId, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
    let mut frosted = node(id, "box", x, y, width, height);
    frosted.computed_style.background_color = Color::TRANSPARENT;
    frosted.computed_style.backdrop_filter = VisualFilter { blur_radius: 4.0 };
    frosted
}

#[test]
fn backdrop_regions_require_painted_content_beneath() {
    // Frosted node with nothing painted beneath it (transparent root):
    // its in-surface backdrop is empty, so it must not widen damage.
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    root.children.push(frosted_node(2, 20.0, 20.0, 40.0, 40.0));
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);
    assert!(
        list.backdrop_filter_regions().is_empty(),
        "backdrop with empty in-surface backdrop must contribute no region"
    );

    // Opaque content painted beneath the frosted node activates it. The
    // region is the node rect inflated by the 3x blur-kernel pad.
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    root.children.push(node(2, "box", 0.0, 0.0, 50.0, 100.0));
    root.children.push(frosted_node(3, 20.0, 20.0, 40.0, 40.0));
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);
    assert_eq!(
        list.backdrop_filter_regions(),
        &[DamageRect {
            x: 8,
            y: 8,
            width: 64,
            height: 64,
        }],
        "active backdrop region should be the node rect plus 12px pad"
    );
}

#[test]
fn blur_regions_come_from_full_tree_not_scoped_paint_commands() {
    // Popup-shaped case: transparent root with an opaque sibling beneath a
    // frosted node. The frosted node's compositor blur region must be the
    // node rect regardless of which subtree a scoped repaint selects.
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    root.children.push(node(2, "box", 0.0, 0.0, 50.0, 100.0));
    root.children.push(frosted_node(3, 20.0, 20.0, 40.0, 40.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);
    let expected = vec![DamageRect {
        x: 20,
        y: 20,
        width: 40,
        height: 40,
    }];
    assert_eq!(
        list.blur_regions(),
        expected.as_slice(),
        "full rebuild should expose the frosted node's blur region"
    );

    // A scoped repaint that only marks the opaque sibling dirty rebuilds a
    // partial paint-command set that need not contain the frosted node.
    // Blur regions are computed from the full tree, so they must not
    // collapse to empty (which the compositor reads as whole-surface blur).
    list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            material: 1,
            ..Default::default()
        },
        &HashSet::from([2]),
        100,
        100,
        false,
        true,
    );
    assert_eq!(
        list.blur_regions(),
        expected.as_slice(),
        "scoped repaint must not drop the frosted node's blur region"
    );
}

#[test]
fn expand_damage_for_blur_regions_grows_intersecting_rects() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    root.children.push(node(2, "box", 0.0, 0.0, 50.0, 100.0));
    root.children.push(frosted_node(3, 20.0, 20.0, 40.0, 40.0));
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);

    // Damage far from the frosted region stays untouched.
    let mut disjoint = [DamageRect {
        x: 80,
        y: 80,
        width: 10,
        height: 10,
    }];
    assert!(!list.expand_damage_for_blur_regions(&mut disjoint));
    assert_eq!(
        disjoint[0],
        DamageRect {
            x: 80,
            y: 80,
            width: 10,
            height: 10,
        }
    );

    // Damage touching the read region grows to cover the whole region so
    // the blur re-reads a consistently repainted backdrop.
    let mut touching = [DamageRect {
        x: 0,
        y: 30,
        width: 10,
        height: 10,
    }];
    assert!(list.expand_damage_for_blur_regions(&mut touching));
    assert_eq!(
        touching[0],
        DamageRect {
            x: 0,
            y: 8,
            width: 72,
            height: 64,
        },
        "expanded damage must union the backdrop read region"
    );
}

fn blurred_node(id: NodeId, x: f32, y: f32, w: f32, h: f32, radius: f32) -> WidgetNode {
    let mut node = node(id, "box", x, y, w, h);
    node.computed_style.filter = VisualFilter {
        blur_radius: radius,
    };
    node
}

#[test]
fn filtered_node_wraps_its_whole_subtree_in_a_layer_scope() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    let mut blurred = blurred_node(2, 20.0, 20.0, 40.0, 40.0, 4.0);
    blurred.children.push(node(3, "box", 24.0, 24.0, 8.0, 8.0));
    root.children.push(blurred);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);

    let kinds: Vec<_> = list.paint_commands().iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        vec![
            DisplayPaintCommandKind::Node,
            DisplayPaintCommandKind::PushFilterLayer,
            DisplayPaintCommandKind::Node,
            DisplayPaintCommandKind::Node,
            DisplayPaintCommandKind::PopFilterLayer,
        ],
        "the child's commands must fall inside the blurred node's layer scope"
    );

    // The push carries the region the layer composites: the subtree grown
    // by the blur kernel's reach, clipped to the surface.
    let push = &list.paint_commands()[1];
    assert_eq!(
        (push.clip.x, push.clip.y),
        (8, 8),
        "layer bounds must include the blur pad, not just the element box"
    );
    assert!(
        push.clip.width >= 40 + 24 && push.clip.height >= 40 + 24,
        "layer bounds {}x{} must cover the padded subtree",
        push.clip.width,
        push.clip.height
    );
}

#[test]
fn unfiltered_tree_emits_no_layer_scopes() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.children.push(node(2, "box", 10.0, 10.0, 20.0, 20.0));
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);

    assert!(
        list.paint_commands()
            .iter()
            .all(|command| command.kind.draws_content())
    );
    assert!(list.filter_layer_regions().is_empty());
}

#[test]
fn damage_inside_a_blur_layer_selects_the_whole_scope() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    let mut blurred = blurred_node(2, 20.0, 20.0, 40.0, 40.0, 4.0);
    blurred.children.push(node(3, "box", 24.0, 24.0, 8.0, 8.0));
    root.children.push(blurred);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);

    // Damage covering only the inner child: replaying it alone would paint
    // the child outside the layer, unblurred.
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 25,
            y: 25,
            width: 4,
            height: 4,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let kinds: Vec<_> = selected.iter_with_kinds().map(|(_, kind)| kind).collect();
    let push = kinds
        .iter()
        .position(|kind| *kind == DisplayPaintCommandKind::PushFilterLayer)
        .unwrap_or_else(|| panic!("selection must replay the layer push, got {kinds:?}"));
    let pop = kinds
        .iter()
        .position(|kind| *kind == DisplayPaintCommandKind::PopFilterLayer)
        .unwrap_or_else(|| panic!("selection must replay the layer pop, got {kinds:?}"));
    assert_eq!(
        pop - push,
        3,
        "the layer's own node, its child, and nothing else belong between \
         the push and the pop, got {kinds:?}"
    );
}

#[test]
fn damage_inside_a_blur_layer_grows_to_the_layer_region() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.children
        .push(blurred_node(2, 20.0, 20.0, 40.0, 40.0, 4.0));
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);

    let region = *list
        .filter_layer_regions()
        .first()
        .expect("a blurred node contributes one layer region");
    let mut damage = [DamageRect {
        x: 30,
        y: 30,
        width: 4,
        height: 4,
    }];
    assert!(list.expand_damage_for_blur_regions(&mut damage));
    assert_eq!(
        damage[0], region,
        "every pixel of a blur layer depends on the rest, so partial damage \
         inside it has to grow to the whole layer"
    );
}
