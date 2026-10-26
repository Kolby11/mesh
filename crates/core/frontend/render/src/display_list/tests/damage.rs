use super::super::*;
use super::common::*;
use crate::RenderObjectDirtySummary;
use crate::{DeviceRect, FractionalScale};
use mesh_core_elements::Prop;
use mesh_core_elements::style::{Color, Overflow, Visibility};
use mesh_core_elements::{NodeId, VisualFilter, WidgetNode};
use std::sync::Arc;

#[test]
fn frame_paint_plan_contains_immutable_inputs_topology_geometry_replay_and_damage() {
    let mut root = node(1, "row", 0.0, 0.0, 160.0, 40.0);
    root.computed_style.transform.translate_x = 1.0;
    root.children.push(node(2, "box", 8.0, 4.0, 40.0, 20.0));
    root.children.push(node(3, "box", 80.0, 4.0, 40.0, 20.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 40, false, true);
    let first = list.frame_paint_plan().clone();

    assert_eq!(first.inputs.root_id, Some(root.id));
    assert_eq!(first.inputs.surface_size, Some((160, 40)));
    assert_eq!(first.nodes().len(), 3);
    assert_eq!(first.topology.commands.len(), first.topology.kinds.len());
    assert_eq!(first.topology.commands.len(), list.paint_commands().len());
    assert_eq!(first.transforms().len(), first.nodes().len());
    assert!(
        first
            .transforms()
            .iter()
            .any(|transform| transform.node_id == 2 && transform.transform.tx > 8.0)
    );
    assert_eq!(first.replay.spans.len(), list.command_spans.len());
    assert_eq!(
        first.replay.layer_scopes.as_ref(),
        list.layer_scopes.as_slice()
    );
    assert_eq!(first.damage.logical.as_ref(), list.damage_rects());
    assert!(!first.damage.logical.is_empty());

    let device_damage = first.damage.device_rects(FractionalScale::new(1.5));
    assert_eq!(device_damage.len(), first.damage.logical.len());
    assert_eq!(
        device_damage[0],
        FractionalScale::new(1.5).device_rect(first.damage.logical[0])
    );
    assert_eq!(
        first
            .damage
            .device_damage_for_buffer(FractionalScale::new(1.5), 240, 60)
            .len(),
        device_damage.len()
    );
    assert_eq!(
        FractionalScale::new(1.5).device_rect(DamageRect {
            x: 1,
            y: 0,
            width: 2,
            height: 1,
        }),
        DeviceRect::new(1, 0, 4, 2)
    );

    let first_generation = first.topology.generation;
    root.children[0].computed_style.z_index = 1;
    root.children[1].computed_style.z_index = 0;
    list.update(&root, 160, 40, false, true);
    let second = list.frame_paint_plan();
    assert!(second.topology.generation > first_generation);
    assert_eq!(first.topology.commands[1].node.id, 2);
    assert_eq!(second.topology.commands[1].node.id, 3);
}

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
        build_command_spans(&root, &list.subtrees).as_ref()
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
    let mut plan = list.frame_paint_plan().clone();
    plan.replay.spans = Arc::new(PaintSpanSequence::default());
    list.frame_plan = Arc::new(plan);
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
    text.attributes.insert(Prop::Content, "hello".into());
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
fn release_display_metrics_preserve_batches_and_barriers() {
    let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    root.children.push(node(2, "box", 0.0, 0.0, 20.0, 20.0));
    root.children.push(node(3, "box", 20.0, 0.0, 20.0, 20.0));
    let mut text = node(4, "text", 40.0, 0.0, 20.0, 20.0);
    text.computed_style.background_color.a = 0;
    text.attributes.insert(Prop::Content, "release".into());
    root.children.push(text);

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 100, 20, false, false);

    assert_eq!(metrics.batch_count, 1);
    assert_eq!(metrics.batched_primitives, 3);
    assert_eq!(metrics.barriers.text, 1);
    assert_eq!(metrics.barrier_count, 1);
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
    let _resources = crate::resource_state_test_lock();
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
        .insert(Prop::Src, opaque_path.to_string_lossy().into_owned());
    let mut translucent = node(3, "icon", 12.0, 0.0, 10.0, 10.0);
    translucent.computed_style.background_color = Color::TRANSPARENT;
    translucent.computed_style.color = tint;
    translucent
        .attributes
        .insert(Prop::Src, translucent_path.to_string_lossy().into_owned());
    let mut unknown = node(4, "icon", 24.0, 0.0, 10.0, 10.0);
    unknown.computed_style.background_color = Color::TRANSPARENT;
    unknown.computed_style.color = tint;
    unknown.attributes.insert(
        Prop::Src,
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
        .insert(Prop::Src, opaque_path.to_string_lossy().into_owned());
    transparent_root.children.push(transparent_icon);

    let mut transparent_list = RetainedDisplayList::default();
    let transparent_metrics = transparent_list.update(&transparent_root, 100, 20, false, false);

    assert_eq!(transparent_metrics.barriers.opacity, 1);
    assert_eq!(transparent_metrics.barriers.icon, 0);
}

#[test]
fn display_list_rebuilds_when_slider_value_changes() {
    let mut root = node(1, "slider", 0.0, 0.0, 100.0, 20.0);
    root.attributes.insert(Prop::Min, "0".into());
    root.attributes.insert(Prop::Max, "100".into());
    root.attributes.insert(Prop::Value, "25".into());
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 20, false, true);

    root.attributes.insert(Prop::Value, "75".into());
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
    root.scroll_metrics.get_or_insert_default().max_x = 40.0;
    root.scroll_metrics.get_or_insert_default().content_width = 120.0;

    let mut text = node(2, "text", 20.0, 30.0, 20.0, 10.0);
    text.attributes.insert(Prop::Content, "hello".into());
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default()).background =
        "#112233".into();
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default()).foreground =
        "#ddeeff".into();
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .anchor
        .0 = 2.0;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .anchor
        .1 = 3.0;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .focus
        .0 = 8.0;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .focus
        .1 = 9.0;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .text_origin
        .0 = 1.0;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .text_origin
        .1 = 1.0;
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
    disabled.attributes.insert(Prop::Disabled, "true".into());
    let mut inert = node(3, "box", 44.0, 4.0, 30.0, 20.0);
    inert.attributes.insert(Prop::Inert, "true".into());
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

fn frosted_node(id: NodeId, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
    let mut frosted = node(id, "box", x, y, width, height);
    frosted.computed_style.background_color = Color::TRANSPARENT;
    frosted.computed_style.backdrop_filter = VisualFilter::blur(4.0);
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
        &[BackdropRegion {
            node_id: 3,
            region: DamageRect {
                x: 8,
                y: 8,
                width: 64,
                height: 64,
            },
        }],
        "active backdrop region should be the node rect plus 12px pad"
    );
}

#[test]
fn backdrop_blur_policy_changes_retained_command_topology_and_regions() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    root.children.push(node(2, "box", 0.0, 0.0, 50.0, 100.0));
    root.children.push(frosted_node(3, 20.0, 20.0, 40.0, 40.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);
    let compositor_generation = list.generation();
    assert_eq!(
        list.backdrop_blur_policy(),
        BackdropBlurPolicy::CompositorRegion
    );
    assert!(list.paint_commands().iter().any(|command| {
        command.node.id == 3
            && command.kind == DisplayPaintCommandKind::ApplyBackdropFilterCompositor
    }));
    assert!(!list.blur_regions().is_empty());
    assert!(!list.backdrop_filter_regions().is_empty());

    list.set_backdrop_blur_policy(BackdropBlurPolicy::InSurfaceFilter);
    let in_surface_metrics = list.update(&root, 100, 100, false, true);
    assert!(list.generation() > compositor_generation);
    assert!(in_surface_metrics.full_surface_damage);
    assert!(list.paint_commands().iter().any(|command| {
        command.node.id == 3
            && command.kind == DisplayPaintCommandKind::ApplyBackdropFilterInSurface
    }));
    assert!(list.blur_regions().is_empty());
    assert!(!list.backdrop_filter_regions().is_empty());

    let in_surface_generation = list.generation();
    list.set_backdrop_blur_policy(BackdropBlurPolicy::Rejected);
    list.update(&root, 100, 100, false, true);
    assert!(list.generation() > in_surface_generation);
    assert!(list.paint_commands().iter().any(|command| {
        command.node.id == 3 && command.kind == DisplayPaintCommandKind::ApplyBackdropFilterRejected
    }));
    assert!(list.blur_regions().is_empty());
    assert!(!list.backdrop_filter_regions().is_empty());
}

#[test]
fn refraction_without_source_routes_blur_to_compositor_and_paints_rim() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    let mut glass = frosted_node(2, 20.0, 20.0, 40.0, 40.0);
    glass.computed_style.backdrop_filter.refract = true;
    glass.computed_style.backdrop_filter.refract_depth = 12.0;
    glass.computed_style.backdrop_filter.refract_edge = 10.0;
    root.children.push(glass);
    let kinds = |list: &RetainedDisplayList| -> Vec<DisplayPaintCommandKind> {
        list.paint_commands()
            .iter()
            .filter(|command| command.node.id == 2 && command.kind != DisplayPaintCommandKind::Node)
            .map(|command| command.kind)
            .collect()
    };

    // Above windows the image is not what lies beneath: the compositor blurs
    // the real backdrop through an exported region, and only the rim paints.
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);
    assert_eq!(
        kinds(&list),
        [
            DisplayPaintCommandKind::ApplyBackdropFilterCompositor,
            DisplayPaintCommandKind::ApplyBackdropRim,
        ]
    );
    assert_eq!(
        list.blur_regions(),
        &[DamageRect {
            x: 20,
            y: 20,
            width: 40,
            height: 40,
        }]
    );

    // Below every window the source stands for the backdrop and is refracted
    // client-side, with no compositor region.
    let generation = list.generation();
    list.set_refraction_source(true);
    let metrics = list.update(&root, 100, 100, false, true);
    assert!(list.generation() > generation);
    assert!(metrics.full_surface_damage);
    assert_eq!(
        kinds(&list),
        [DisplayPaintCommandKind::ApplyBackdropRefraction]
    );
    assert!(list.blur_regions().is_empty());
}

#[test]
fn transformed_backdrop_regions_follow_cumulative_geometry() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    root.children.push(node(2, "box", 0.0, 0.0, 50.0, 100.0));
    let mut frosted = frosted_node(3, 20.0, 20.0, 40.0, 20.0);
    frosted.computed_style.transform.rotation = std::f32::consts::FRAC_PI_2;
    root.children.push(frosted);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);

    assert_eq!(
        list.backdrop_filter_regions(),
        &[BackdropRegion {
            node_id: 3,
            region: DamageRect {
                x: 17,
                y: 0,
                width: 45,
                height: 62,
            },
        }]
    );
    assert_eq!(
        list.blur_regions(),
        &[DamageRect {
            x: 29,
            y: 9,
            width: 21,
            height: 41,
        }]
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

    // In-surface content lies beneath the frosted element, so MESH blurs it
    // under every policy but `Rejected`. The blurred output overwrote the raw
    // backdrop, so damage touching the read region repaints all of it.
    let touching_rect = DamageRect {
        x: 0,
        y: 30,
        width: 10,
        height: 10,
    };
    let region = list.backdrop_filter_regions()[0].region;
    assert_eq!(
        region,
        DamageRect {
            x: 8,
            y: 8,
            width: 64,
            height: 64,
        }
    );
    for policy in [
        BackdropBlurPolicy::CompositorRegion,
        BackdropBlurPolicy::InSurfaceFilter,
    ] {
        list.set_backdrop_blur_policy(policy);
        list.update(&root, 100, 100, true, true);
        let mut touching = [touching_rect];
        assert!(list.expand_damage_for_blur_regions(&mut touching));
        assert_eq!(touching[0], touching_rect.union(region), "{policy:?}");
    }

    // A rejected backdrop is painted flat; nothing reads it.
    list.set_backdrop_blur_policy(BackdropBlurPolicy::Rejected);
    list.update(&root, 100, 100, true, true);
    let mut touching = [touching_rect];
    assert!(!list.expand_damage_for_blur_regions(&mut touching));
    assert_eq!(touching[0], touching_rect);
}

/// A `backdrop-filter` on the surface root — the shipped navigation bar has
/// one. Over an empty buffer it has no read region, so partial repaints stay
/// partial; over in-surface content the blur needs the whole read region.
#[test]
fn root_backdrop_damage_depends_on_in_surface_content_beneath() {
    let small = DamageRect {
        x: 180,
        y: 20,
        width: 20,
        height: 20,
    };

    let mut bar = node(1, "box", 0.0, 0.0, 400.0, 60.0);
    bar.computed_style.background_color = Color::TRANSPARENT;
    bar.computed_style.backdrop_filter = VisualFilter::blur(4.0);
    bar.children.push(node(2, "box", 180.0, 20.0, 20.0, 20.0));
    let mut list = RetainedDisplayList::default();
    list.update(&bar, 400, 60, true, true);
    assert!(list.backdrop_filter_regions().is_empty());
    let mut damage = [small];
    assert!(!list.expand_damage_for_blur_regions(&mut damage));
    assert_eq!(damage[0], small);

    let mut root = node(1, "box", 0.0, 0.0, 400.0, 60.0);
    root.computed_style.background_color = Color::TRANSPARENT;
    root.children.push(node(2, "box", 0.0, 0.0, 400.0, 60.0));
    root.children.push(frosted_node(3, 0.0, 0.0, 400.0, 60.0));
    let mut list = RetainedDisplayList::default();
    list.update(&root, 400, 60, true, true);
    assert_eq!(list.backdrop_filter_regions().len(), 1);
    let mut damage = [small];
    assert!(list.expand_damage_for_blur_regions(&mut damage));
    assert_eq!(
        damage[0],
        DamageRect {
            x: 0,
            y: 0,
            width: 400,
            height: 60,
        }
    );
}

fn blurred_node(id: NodeId, x: f32, y: f32, w: f32, h: f32, radius: f32) -> WidgetNode {
    let mut node = node(id, "box", x, y, w, h);
    node.computed_style.filter = VisualFilter::blur(radius);
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
fn composited_node_wraps_its_whole_subtree_in_a_layer_scope() {
    let mut root = node(1, "box", 0.0, 0.0, 100.0, 100.0);
    let mut composited = node(2, "box", 20.0, 20.0, 40.0, 40.0);
    composited.computed_style.opacity = 0.5;
    composited.computed_style.mix_blend_mode = mesh_core_elements::BlendMode::Multiply;
    composited
        .children
        .push(node(3, "box", 4.0, 4.0, 16.0, 16.0));
    root.children.push(composited);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 100, true, true);

    let kinds: Vec<_> = list
        .paint_commands()
        .iter()
        .map(|command| command.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            DisplayPaintCommandKind::Node,
            DisplayPaintCommandKind::PushCompositingLayer,
            DisplayPaintCommandKind::Node,
            DisplayPaintCommandKind::Node,
            DisplayPaintCommandKind::PopCompositingLayer,
        ],
        "opacity and blend mode must scope the node and all descendants"
    );
    let push = &list.paint_commands()[1];
    assert_eq!(push.node.style.opacity, 0.5);
    assert_eq!(
        push.node.style.mix_blend_mode,
        mesh_core_elements::BlendMode::Multiply
    );
    assert!(push.clip.width >= 40 && push.clip.height >= 40);
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

#[test]
fn spans_survive_a_wholesale_subtree_reuse() {
    let mut root = node(1, "column", 0.0, 0.0, 300.0, 200.0);
    let mut button = node(2, "button", 0.0, 0.0, 300.0, 56.0);
    button.children.push(node(3, "icon", 8.0, 16.0, 24.0, 24.0));
    let mut copy = node(4, "column", 40.0, 8.0, 200.0, 40.0);
    copy.children.push(node(5, "text", 40.0, 8.0, 200.0, 18.0));
    copy.children.push(node(6, "text", 40.0, 28.0, 200.0, 18.0));
    button.children.push(copy);
    root.children.push(button);
    root.children
        .push(node(7, "button", 0.0, 60.0, 300.0, 56.0));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 300, 200, true, true);

    root.children[0].computed_style.background_color = Color {
        r: 90,
        g: 90,
        b: 90,
        a: 255,
    };
    let mut dirty = std::collections::HashSet::new();
    dirty.insert(2 as NodeId);
    list.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary {
            material: 1,
            ..Default::default()
        },
        &dirty,
        300,
        200,
        false,
        true,
    );

    let damage = DamageRect {
        x: 0,
        y: 0,
        width: 300,
        height: 56,
    };
    let selected = list.select_paint_commands(Some(damage), DisplayListRepaintPolicy::BoundingRect);
    let SelectedDisplayListSelection::Spans { spans, .. } = &selected.selection else {
        panic!(
            "a partial repaint must select spans, got {:?}",
            selected.selection
        );
    };
    let painted: Vec<_> = spans
        .iter()
        .flat_map(|span| list.paint_commands()[span.start..span.end].iter())
        .map(|command| command.node.id)
        .collect();
    assert!(
        painted.contains(&5) && painted.contains(&6),
        "the reused label subtree lies inside the damaged button, so its text \
         commands have to be replayed, got {painted:?}"
    );
    assert!(
        !painted.contains(&7),
        "the untouched second button is outside the damage, got {painted:?}"
    );
}

// cargo test -p mesh-core-render --release -- frame_plan_derivation_and_visual_damage_profile --ignored --nocapture
#[test]
#[ignore = "release-only frame-plan and visual-damage benchmark"]
fn frame_plan_derivation_and_visual_damage_profile() {
    use std::collections::{HashMap, HashSet};
    use std::time::{Duration, Instant};

    let iterations = 2_000_u64;
    let (width, height) = (240, 480);
    let surface = DamageRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let mut eager_root = display_entry_benchmark_tree(60, 20);
    let mut lazy_root = display_entry_benchmark_tree(60, 20);
    let changed_id = eager_root.children[30].children[5].id;
    let dirty_ids = HashSet::from([changed_id]);
    let mut eager = RetainedDisplayList::default();
    eager.update(&eager_root, width, height, false, true);
    let mut lazy = RetainedDisplayList::default();
    lazy.update(&lazy_root, width, height, false, true);
    let summary = RenderObjectDirtySummary {
        text: 1,
        ..Default::default()
    };

    // Eager: the per-paint work this replaced — plan nodes, transforms and
    // scroll scopes derived on every update, plus a whole-tree visual-damage
    // map. Lazy: the update alone, as on a frame without visual dirty bits.
    let mut eager_time = Duration::ZERO;
    let mut lazy_time = Duration::ZERO;
    let mut sink = 0usize;
    for generation in 0..iterations {
        let content = generation.to_string();
        eager_root.children[30].children[5]
            .attributes
            .insert(Prop::Content, content.clone());
        lazy_root.children[30].children[5]
            .attributes
            .insert(Prop::Content, content);
        let started = Instant::now();
        eager.update_with_dirty_nodes(&eager_root, summary, &dirty_ids, width, height, false, true);
        let plan = eager.frame_paint_plan();
        sink = sink.wrapping_add(std::hint::black_box(
            plan.nodes().len() + plan.transforms().len() + plan.scroll_scopes().len(),
        ));
        sink = sink.wrapping_add(std::hint::black_box(
            eager.visual_damage_rects(surface).len(),
        ));
        eager_time += started.elapsed();

        let started = Instant::now();
        lazy.update_with_dirty_nodes(&lazy_root, summary, &dirty_ids, width, height, false, true);
        lazy_time += started.elapsed();
    }

    // A visual-dirty frame: the whole-tree map versus the dirty node's bounds
    // looked up in the previous and current command snapshots.
    let previous = lazy.paint_commands_snapshot();
    let mut full_time = Duration::ZERO;
    let mut filtered_time = Duration::ZERO;
    let mut scratch = (HashMap::new(), HashMap::new());
    for _ in 0..iterations {
        let started = Instant::now();
        let map = lazy.visual_damage_rects(surface);
        sink = sink.wrapping_add(std::hint::black_box(map.get(&changed_id).is_some() as usize));
        full_time += started.elapsed();

        let started = Instant::now();
        scratch.0.clear();
        scratch.1.clear();
        let current = lazy.paint_commands_snapshot();
        visual_damage_rects_into(
            &previous,
            surface,
            |id| dirty_ids.contains(&id),
            &mut scratch.0,
        );
        visual_damage_rects_into(
            &current,
            surface,
            |id| dirty_ids.contains(&id),
            &mut scratch.1,
        );
        sink = sink.wrapping_add(std::hint::black_box(scratch.0.len() + scratch.1.len()));
        filtered_time += started.elapsed();
    }
    assert_eq!(
        scratch.1.get(&changed_id),
        lazy.visual_damage_rects(surface).get(&changed_id)
    );
    std::hint::black_box(sink);

    let frame_speedup = eager_time.as_secs_f64() / lazy_time.as_secs_f64();
    let visual_speedup = full_time.as_secs_f64() / filtered_time.as_secs_f64();
    eprintln!(
        "{iterations} sparse text updates of a 1,261-node list: eager plan + visual map {eager_time:?}; lazy {lazy_time:?}; ratio {frame_speedup:.2}x"
    );
    eprintln!(
        "{iterations} visual-dirty lookups: whole-tree map {full_time:?}; two filtered scans {filtered_time:?}; ratio {visual_speedup:.2}x"
    );
    println!("MESH_PERF metric=lazy_frame_plan_speedup value={frame_speedup:.6}");
    println!("MESH_PERF metric=filtered_visual_damage_speedup value={visual_speedup:.6}");
    assert!(
        frame_speedup >= 2.0,
        "lazy frame-plan derivation should be at least 2x"
    );
    assert!(
        visual_speedup >= 1.5,
        "filtered visual damage should be at least 1.5x"
    );
}

#[test]
fn compositor_blur_bands_leave_scooped_corners_out() {
    use mesh_core_elements::style::{CornerShapes, Corners};
    let bounds = DamageRect {
        x: 0,
        y: 0,
        width: 20,
        height: 20,
    };
    let radii = Corners {
        top_left: 10.0,
        ..Corners::zero()
    };
    let bands = rounded_region_bands(
        bounds,
        radii,
        CornerShapes::from_css("scoop round round round"),
        bounds,
    );
    let covers = |x: u32, y: u32| {
        bands
            .iter()
            .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
    };
    assert!(!covers(1, 1), "inside the scoop: {bands:?}");
    assert!(!covers(6, 6), "inside the scoop: {bands:?}");
    assert!(covers(11, 1) && covers(1, 11) && covers(10, 10));
}
