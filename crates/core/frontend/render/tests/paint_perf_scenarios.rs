use std::collections::HashSet;

use mesh_core_elements::style::Color;
use mesh_core_elements::{NodeId, WidgetNode};
use mesh_core_render::{
    DamageRect, DisplayListRepaintPolicy, PixelBuffer, RenderObjectDirtySummary,
    RetainedDisplayList, paint_display_list_for_module_with_profiling_metrics,
};

fn node(id: NodeId, tag: &str, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.id = id;
    node.layout.x = x;
    node.layout.y = y;
    node.layout.width = width;
    node.layout.height = height;
    node.computed_style.background_color = Color::from_hex("#223344").unwrap();
    node
}

#[test]
fn perf_large_tree_retained_reuse_smoke() {
    let mut root = node(1, "column", 0.0, 0.0, 1200.0, 1200.0);
    let mut next_id: NodeId = 2;
    for row in 0..20 {
        for col in 0..20 {
            let x = (col * 56) as f32;
            let y = (row * 56) as f32;
            let mut container = node(next_id, "box", x, y, 52.0, 52.0);
            next_id += 1;
            let mut label = node(next_id, "text", x + 4.0, y + 8.0, 40.0, 16.0);
            label
                .attributes
                .insert("content".into(), format!("r{row}c{col}"));
            next_id += 1;
            container.children.push(label);
            root.children.push(container);
        }
    }

    let mut list = RetainedDisplayList::default();
    let first = list.update(&root, 1280, 1280, false, true);
    let second = list.update(&root, 1280, 1280, false, true);

    assert!(first.entries_total >= 800);
    assert!(first.entries_rebuilt > 0);
    assert_eq!(first.entries_reused, 0);
    assert_eq!(second.entries_rebuilt, 0);
    assert!(second.entries_reused > 0);
    assert_eq!(second.damage_area, 0);
}

#[test]
fn perf_sparse_damage_filters_paint_selection() {
    let mut root = node(1, "row", 0.0, 0.0, 2400.0, 80.0);
    let mut target: NodeId = 0;
    for idx in 0..80 {
        let id = 2 + idx;
        let x = (idx * 30) as f32;
        let child = node(id, "box", x, 0.0, 20.0, 40.0);
        if idx == 35 {
            target = id;
        }
        root.children.push(child);
    }

    let mut list = RetainedDisplayList::default();
    list.update(&root, 2400, 80, false, true);
    let full_len = list.paint_commands().len();
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 35 * 30,
            y: 0,
            width: 30,
            height: 40,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );

    assert!(selected.len() < full_len);
    assert!(selected.metrics().filtered_commands_skipped > 0);
    assert!(selected.iter().any(|command| command.node.id == target));
}

#[test]
fn perf_text_heavy_path_exercises_text_cache() {
    let mut root = node(1, "column", 0.0, 0.0, 800.0, 2000.0);
    for idx in 0..120 {
        let y = (idx * 16) as f32;
        let mut label = node(2 + idx, "text", 0.0, y, 760.0, 14.0);
        label.attributes.insert(
            "content".into(),
            format!("line-{idx}: performance text shaping cache scenario"),
        );
        root.children.push(label);
    }

    let mut list = RetainedDisplayList::default();
    list.update(&root, 800, 2200, false, true);
    let commands: Vec<_> = list.paint_commands().to_vec();
    let mut first_buffer = PixelBuffer::new(800, 2200);
    let first = paint_display_list_for_module_with_profiling_metrics(
        &commands,
        &mut first_buffer,
        1.0,
        None,
        None,
        None,
        None,
    );

    let mut second_buffer = PixelBuffer::new(800, 2200);
    let second = paint_display_list_for_module_with_profiling_metrics(
        &commands,
        &mut second_buffer,
        1.0,
        None,
        None,
        None,
        None,
    );

    assert!(first.text.layout_misses > 0);
    assert!(second.text.layout_hits > 0);
    assert!(second.text.layout_misses <= first.text.layout_misses);
}

#[test]
fn perf_icon_heavy_path_records_raster_cache_hits() {
    let td = tempfile::tempdir().unwrap();
    let icon_path = td.path().join("opaque.png");
    image::ImageBuffer::from_fn(12, 12, |_, _| image::Rgba([220u8, 40, 30, 255]))
        .save(&icon_path)
        .unwrap();

    let mut root = node(1, "row", 0.0, 0.0, 1600.0, 40.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    for idx in 0..100 {
        let x = (idx * 16) as f32;
        let mut icon = node(2 + idx, "icon", x, 0.0, 12.0, 12.0);
        icon.computed_style.background_color = Color::TRANSPARENT;
        icon.computed_style.color = Color::WHITE;
        icon.attributes
            .insert("src".into(), icon_path.to_string_lossy().into_owned());
        root.children.push(icon);
    }

    let mut list = RetainedDisplayList::default();
    list.update(&root, 1600, 40, false, true);
    let commands: Vec<_> = list.paint_commands().to_vec();

    let mut first_buffer = PixelBuffer::new(1600, 40);
    let first = paint_display_list_for_module_with_profiling_metrics(
        &commands,
        &mut first_buffer,
        1.0,
        None,
        None,
        None,
        None,
    );

    let mut second_buffer = PixelBuffer::new(1600, 40);
    let second = paint_display_list_for_module_with_profiling_metrics(
        &commands,
        &mut second_buffer,
        1.0,
        None,
        None,
        None,
        None,
    );

    assert!(first.raster_cache_hits > 0);
    assert!(second.raster_cache_hits > 0);
    assert!(second.raster_cache_misses <= first.raster_cache_misses);
}

#[test]
fn perf_animation_dirty_update_reuses_unrelated_subtrees() {
    let mut root = node(1, "row", 0.0, 0.0, 1200.0, 80.0);
    let mut left = node(2, "row", 0.0, 0.0, 560.0, 80.0);
    let mut right = node(3, "row", 620.0, 0.0, 560.0, 80.0);

    for idx in 0..20 {
        let lx = (idx * 26) as f32;
        let rx = (idx * 26) as f32;
        left.children
            .push(node(10 + idx, "box", lx, 0.0, 20.0, 20.0));
        right
            .children
            .push(node(100 + idx, "box", rx, 0.0, 20.0, 20.0));
    }
    root.children.push(left);
    root.children.push(right);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 1200, 80, false, true);

    root.children[0].computed_style.transform.translate_x = 8.0;
    let metrics = list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            transform: 1,
            ..Default::default()
        },
        &HashSet::from([2]),
        1200,
        80,
        false,
        true,
    );
    let selected = list.select_paint_commands(
        Some(metrics.damage_rect),
        DisplayListRepaintPolicy::MinimalDamage,
    );

    assert!(metrics.subtree_segments_reused > 0);
    assert!(metrics.subtree_segments_rebuilt > 0);
    assert_eq!(metrics.full_fallback_count, 0);
    assert!(selected.metrics().filtered_command_count > 0);
    assert!(selected.len() <= list.paint_commands().len());
}

#[test]
fn service_field_reads_populated_on_nodes() {
    let mut root = node(1, "column", 0.0, 0.0, 400.0, 200.0);

    let mut child_a = node(2, "text", 0.0, 0.0, 100.0, 20.0);
    child_a
        .service_field_reads
        .push(("audio".to_string(), "percent".to_string()));
    child_a
        .service_field_reads
        .push(("audio".to_string(), "muted".to_string()));

    let child_b = node(3, "text", 100.0, 0.0, 100.0, 20.0);

    let mut child_c = node(4, "icon", 200.0, 0.0, 20.0, 20.0);
    child_c
        .service_field_reads
        .push(("network".to_string(), "connected".to_string()));

    root.children.push(child_a);
    root.children.push(child_b);
    root.children.push(child_c);

    assert_eq!(root.children[0].service_field_reads.len(), 2);
    assert_eq!(
        root.children[0].service_field_reads[0],
        ("audio".to_string(), "percent".to_string())
    );
    assert_eq!(
        root.children[0].service_field_reads[1],
        ("audio".to_string(), "muted".to_string())
    );
    assert!(root.children[1].service_field_reads.is_empty());
    assert_eq!(root.children[2].service_field_reads.len(), 1);
    assert_eq!(
        root.children[2].service_field_reads[0],
        ("network".to_string(), "connected".to_string())
    );
    assert!(root.service_field_reads.is_empty());
}
