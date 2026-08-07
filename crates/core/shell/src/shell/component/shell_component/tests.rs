use super::*;

#[test]
fn fractional_damage_scales_edges_into_physical_buffer_space() {
    let scaled = scale_damage_rect_to_buffer(
        DamageRect {
            x: 1,
            y: 3,
            width: 2,
            height: 2,
        },
        1.5,
        100,
        100,
    );
    assert_eq!(
        scaled,
        DamageRect {
            x: 1,
            y: 4,
            width: 4,
            height: 4,
        }
    );
}

#[test]
fn physical_damage_is_clipped_to_buffer_bounds() {
    let scaled = scale_damage_rect_to_buffer(
        DamageRect {
            x: 9,
            y: 9,
            width: 4,
            height: 4,
        },
        1.5,
        16,
        16,
    );
    assert_eq!(
        scaled,
        DamageRect {
            x: 13,
            y: 13,
            width: 3,
            height: 3,
        }
    );
}

#[test]
fn element_metrics_gate_ignores_paint_and_state_only_diffs() {
    let visual_only = RetainedTreeDirtySummary {
        style: 4,
        state: 2,
        ..Default::default()
    };
    assert!(!retained_dirty_affects_element_metrics(visual_only));

    for changed in [
        RetainedTreeDirtySummary {
            layout: 1,
            ..Default::default()
        },
        RetainedTreeDirtySummary {
            attributes: 1,
            ..Default::default()
        },
        RetainedTreeDirtySummary {
            inserted: 1,
            ..Default::default()
        },
        RetainedTreeDirtySummary {
            removed: 1,
            ..Default::default()
        },
        RetainedTreeDirtySummary {
            children: 1,
            ..Default::default()
        },
    ] {
        assert!(retained_dirty_affects_element_metrics(changed));
    }
}

// cargo test -p mesh-core-shell --release -- element_metrics_dirty_gate_beats_unchanged_snapshot_build --ignored --nocapture
#[test]
#[ignore = "release-only element metrics gate microbenchmark"]
fn element_metrics_dirty_gate_beats_unchanged_snapshot_build() {
    fn build(key: String, width: usize, depth: usize) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.attributes.insert("_mesh_key".into(), key.clone());
        node.layout.width = 20.0;
        node.layout.height = 20.0;
        if depth > 0 {
            node.children = (0..width)
                .map(|index| build(format!("{key}/{index}"), width, depth - 1))
                .collect();
        }
        node
    }

    let tree = build("root".into(), 4, 5);
    let iterations = 2_000;
    let old_started = std::time::Instant::now();
    for _ in 0..iterations {
        let mut elements = serde_json::Map::new();
        let mut refs = serde_json::Map::new();
        let mut ref_keys = HashMap::new();
        collect_element_metrics(
            std::hint::black_box(&tree),
            0.0,
            0.0,
            true,
            true,
            &mut elements,
            &mut refs,
            &mut ref_keys,
        );
        std::hint::black_box((elements, refs, ref_keys));
    }
    let old_time = old_started.elapsed();

    let visual_only = RetainedTreeDirtySummary {
        style: 1,
        ..Default::default()
    };
    let gate_started = std::time::Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(retained_dirty_affects_element_metrics(visual_only));
    }
    let gate_time = gate_started.elapsed();

    eprintln!(
        "unchanged element metrics: snapshot {old_time:?}; dirty gate {gate_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / gate_time.as_secs_f64()
    );
    assert!(gate_time * 10 < old_time);
}

fn surface(width: u32, height: u32) -> DamageRect {
    DamageRect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

fn visual_node() -> WidgetNode {
    let mut node = WidgetNode::new("box");
    node.id = 1;
    node.layout.x = 10.0;
    node.layout.y = 10.0;
    node.layout.width = 20.0;
    node.layout.height = 10.0;
    node
}

fn metrics(surface_area: u64) -> DisplayListMetrics {
    DisplayListMetrics {
        surface_area,
        ..Default::default()
    }
}

fn keyed_node(tag: &str, key: &str, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.attributes.insert("_mesh_key".into(), key.into());
    node.layout.x = x;
    node.layout.y = y;
    node.layout.width = width;
    node.layout.height = height;
    node
}

#[test]
fn open_popover_nodes_derive_child_surface_requests() {
    let mut root = keyed_node("row", "root", 0.0, 0.0, 200.0, 40.0);
    let mut popover = keyed_node("popover", "root/menu", 20.0, 42.0, 96.0, 36.0);
    popover.attributes.insert("open".into(), "true".into());
    popover.attributes.insert("anchor".into(), "bottom".into());
    popover.attributes.insert("offset-y".into(), "6".into());
    let child = keyed_node("button", "root/menu/option", 20.0, 54.0, 96.0, 24.0);
    popover.children.push(child);
    root.children.push(popover);

    let mut requests = Vec::new();
    collect_child_surface_requests(&root, &root, &mut requests);

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].node_key, "root/menu");
    assert_eq!(requests[0].kind, ChildSurfaceKind::Popover);
    assert_eq!(requests[0].anchor_rect, (20, 42, 96, 36));
    assert_eq!(requests[0].content_size, (96, 36));
    assert_eq!(requests[0].content_padding, (0, 0, 0, 0));
    assert_eq!(requests[0].placement.offset_y, 6);
}

#[test]
fn popover_with_descendant_box_shadow_gets_buffer_padding() {
    // No shadow on the popover node itself; a descendant (e.g. a
    // floating bubble button) carries the shadow, mirroring how
    // language-popover/theme-selector's bubble options are built.
    let mut root = keyed_node("row", "root", 0.0, 0.0, 200.0, 40.0);
    let mut popover = keyed_node("popover", "root/menu", 20.0, 42.0, 96.0, 36.0);
    popover.attributes.insert("open".into(), "true".into());
    let mut child = keyed_node("button", "root/menu/option", 20.0, 42.0, 96.0, 36.0);
    child.computed_style.box_shadow = mesh_core_elements::style::BoxShadow {
        offset_x: 0.0,
        offset_y: 6.0,
        blur_radius: 8.0,
        spread_radius: 1.0,
        color: mesh_core_elements::style::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 200,
        },
        inset: false,
    };
    popover.children.push(child);
    root.children.push(popover);

    let mut requests = Vec::new();
    collect_child_surface_requests(&root, &root, &mut requests);

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].content_size, (96, 36));
    // blur_pad = 8 * 3 = 24, spread = 1: unshifted overshoot is 25px.
    // The 6px downward `offset_y` reduces the top overshoot (shadow moves
    // away from the top edge) and adds to the bottom overshoot.
    let (left, top, right, bottom) = requests[0].content_padding;
    assert_eq!((left, top, right, bottom), (25, 19, 25, 31));
}

#[test]
fn open_popover_anchor_ref_uses_trigger_bounds() {
    let mut root = keyed_node("row", "root", 0.0, 0.0, 200.0, 80.0);
    let mut trigger = keyed_node("button", "root/trigger", 12.0, 8.0, 44.0, 20.0);
    trigger
        .attributes
        .insert("ref".into(), "menu_button".into());
    let mut popover = keyed_node("popover", "root/menu", 20.0, 42.0, 80.0, 10.0);
    popover.attributes.insert("open".into(), "true".into());
    popover
        .attributes
        .insert("anchor-ref".into(), "menu_button".into());
    popover.attributes.insert("gravity".into(), "bottom".into());
    root.children.push(trigger);
    root.children.push(popover);

    let mut requests = Vec::new();
    collect_child_surface_requests(&root, &root, &mut requests);

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].anchor_rect, (12, 8, 44, 20));
}

#[test]
fn closed_popover_nodes_stay_inline() {
    let mut root = keyed_node("row", "root", 0.0, 0.0, 200.0, 40.0);
    let mut popover = keyed_node("popover", "root/menu", 20.0, 42.0, 80.0, 10.0);
    popover.attributes.insert("open".into(), "false".into());
    root.children.push(popover);

    let mut requests = Vec::new();
    collect_child_surface_requests(&root, &root, &mut requests);

    assert!(requests.is_empty());
}

#[test]
fn absolute_child_escaping_root_derives_overflow_surface_request() {
    let mut root = keyed_node("row", "root", 0.0, 0.0, 120.0, 40.0);
    let mut overlay = keyed_node("box", "root/overlay", 90.0, 24.0, 80.0, 32.0);
    overlay.computed_style.position = mesh_core_elements::style::Position::Absolute;
    root.children.push(overlay);

    let mut requests = Vec::new();
    collect_child_surface_requests(&root, &root, &mut requests);

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].kind, ChildSurfaceKind::Overflow);
    assert_eq!(requests[0].node_key, "root/overlay");
    assert_eq!(requests[0].anchor_rect, (90, 24, 80, 32));
}

#[test]
fn nested_absolute_child_escaping_root_derives_overflow_surface_request() {
    let mut root = keyed_node("row", "root", 0.0, 0.0, 120.0, 40.0);
    let mut wrapper = keyed_node("box", "root/wrapper", 0.0, 0.0, 120.0, 40.0);
    let mut overlay = keyed_node("box", "root/wrapper/overlay", 96.0, 8.0, 64.0, 24.0);
    overlay.computed_style.position = mesh_core_elements::style::Position::Absolute;
    wrapper.children.push(overlay);
    root.children.push(wrapper);

    let mut requests = Vec::new();
    collect_child_surface_requests(&root, &root, &mut requests);

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].kind, ChildSurfaceKind::Overflow);
    assert_eq!(requests[0].node_key, "root/wrapper/overlay");
    assert_eq!(requests[0].anchor_rect, (96, 8, 64, 24));
}

#[test]
fn clipped_absolute_descendant_stays_in_parent_surface() {
    let mut root = keyed_node("row", "root", 0.0, 0.0, 120.0, 40.0);
    let mut clipped = keyed_node("box", "root/clipped", 0.0, 0.0, 120.0, 40.0);
    clipped.computed_style.overflow_x = mesh_core_elements::style::Overflow::Hidden;
    let mut overlay = keyed_node("box", "root/clipped/overlay", 96.0, 8.0, 64.0, 24.0);
    overlay.computed_style.position = mesh_core_elements::style::Position::Absolute;
    clipped.children.push(overlay);
    root.children.push(clipped);

    let mut requests = Vec::new();
    collect_child_surface_requests(&root, &root, &mut requests);

    assert!(requests.is_empty());
}

#[test]
fn child_surface_input_is_translated_from_popup_local_coordinates() {
    let input = translate_child_surface_input(
        ComponentInput::PointerButton {
            x: 8.0,
            y: 12.0,
            pressed: true,
        },
        20.0,
        42.0,
    );

    match input {
        ComponentInput::PointerButton { x, y, pressed } => {
            assert_eq!((x, y, pressed), (28.0, 54.0, true));
        }
        other => panic!("expected pointer button input, got {other:?}"),
    }

    let scroll = translate_child_surface_input(
        ComponentInput::Scroll {
            x: 1.0,
            y: 2.0,
            dx: 0.0,
            dy: -1.0,
        },
        20.0,
        42.0,
    );
    match scroll {
        ComponentInput::Scroll { x, y, dx, dy } => {
            assert_eq!((x, y, dx, dy), (21.0, 44.0, 0.0, -1.0));
        }
        other => panic!("expected scroll input, got {other:?}"),
    }
}

#[test]
fn child_surface_debug_tree_offsets_layout_to_local_origin() {
    let mut root = WidgetNode::new("popover");
    root.layout.x = 48.0;
    root.layout.y = 72.0;
    let mut child = WidgetNode::new("button");
    child.layout.x = 60.0;
    child.layout.y = 84.0;
    root.children.push(child);

    offset_widget_tree_layout(&mut root, -48.0, -72.0);

    assert_eq!(root.layout.x, 0.0);
    assert_eq!(root.layout.y, 0.0);
    assert_eq!(root.children[0].layout.x, 12.0);
    assert_eq!(root.children[0].layout.y, 12.0);
}

#[test]
fn animation_damage_includes_transform_visual_bounds() {
    let mut node = visual_node();
    node.computed_style.transform.translate_x = 15.0;
    node.computed_style.transform.translate_y = 5.0;
    node.computed_style.transform.scale_x = 2.0;
    node.computed_style.transform.scale_y = 2.0;

    let damage = visual_damage_rect_for_widget_node(&node, surface(200, 100));

    assert_eq!(
        damage,
        Some(DamageRect {
            x: 25,
            y: 15,
            width: 40,
            height: 20,
        })
    );
}

#[test]
fn animation_damage_includes_shadow_filter_visual_bounds() {
    let mut node = visual_node();
    node.computed_style.box_shadow = mesh_core_elements::BoxShadow {
        offset_x: 4.0,
        offset_y: 6.0,
        blur_radius: 2.0,
        spread_radius: 1.0,
        color: mesh_core_elements::style::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 128,
        },
        inset: false,
    };
    node.computed_style.filter = mesh_core_elements::VisualFilter { blur_radius: 3.0 };

    let damage = visual_damage_rect_for_widget_node(&node, surface(200, 100));

    assert_eq!(
        damage,
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 50,
            height: 42,
        })
    );
}

#[test]
fn animation_damage_unions_previous_and_current_transform_bounds() {
    let mut node = visual_node();
    node.computed_style.transform.translate_x = 30.0;
    let previous = HashMap::from([(
        1,
        DamageRect {
            x: 10,
            y: 10,
            width: 20,
            height: 10,
        },
    )]);

    let damage = damage_rect_for_node_ids(&node, &HashSet::from([1]), &previous, surface(200, 100));

    assert_eq!(
        damage,
        Some(DamageRect {
            x: 10,
            y: 10,
            width: 50,
            height: 10,
        })
    );
}

#[test]
fn animation_damage_unions_previous_and_current_shadow_bounds() {
    let mut node = visual_node();
    node.layout.x = 20.0;
    node.layout.y = 20.0;
    node.computed_style.box_shadow = mesh_core_elements::BoxShadow {
        offset_x: 4.0,
        offset_y: 6.0,
        blur_radius: 2.0,
        spread_radius: 1.0,
        color: mesh_core_elements::style::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 128,
        },
        inset: false,
    };
    let previous = HashMap::from([(
        1,
        DamageRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        },
    )]);

    let damage = damage_rect_for_node_ids(&node, &HashSet::from([1]), &previous, surface(200, 100));

    assert_eq!(
        damage,
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 51,
            height: 43,
        })
    );
}

#[test]
fn policy_keeps_zero_candidate_area_minimal() {
    let policy = select_damage_policy(metrics(10_000), false, false, 0);

    assert_eq!(policy, DisplayListRepaintPolicy::MinimalDamage);
}

#[test]
fn policy_keeps_small_single_damage_minimal() {
    let policy = select_damage_policy(metrics(10_000), false, false, 900);

    assert_eq!(policy, DisplayListRepaintPolicy::MinimalDamage);
}

#[test]
fn policy_keeps_small_overlay_damage_as_bounding_rect() {
    let metrics = metrics(10_000);
    let tooltip = Some(DamageRect {
        x: 10,
        y: 10,
        width: 40,
        height: 20,
    });

    let effective = select_effective_damage(metrics, surface(100, 100), false, None, tooltip);

    assert_eq!(
        effective.rect, tooltip,
        "small tooltip invalidation should stay as a bounded repaint"
    );
    assert!(!effective.full_surface);
    assert_eq!(effective.policy, DisplayListRepaintPolicy::BoundingRect);
}

#[test]
fn policy_keeps_distant_extra_damage_as_multiple_rects() {
    let metrics = metrics(10_000);
    let left = DamageRect {
        x: 5,
        y: 5,
        width: 10,
        height: 10,
    };
    let right = DamageRect {
        x: 80,
        y: 80,
        width: 10,
        height: 10,
    };

    let effective = select_effective_damage_rects(
        metrics,
        &[],
        surface(100, 100),
        false,
        &[left, right],
        &[],
        Vec::new(),
    );

    assert_eq!(effective.rects, vec![left, right]);
    assert_eq!(
        effective.rect,
        Some(DamageRect {
            x: 5,
            y: 5,
            width: 85,
            height: 85,
        })
    );
    assert_eq!(effective.damage_area(10_000), 200);
    assert_eq!(effective.damage_rect_count(), 2);
    assert!(!effective.full_surface);
    assert_eq!(effective.policy, DisplayListRepaintPolicy::BoundingRect);
}

#[test]
fn damage_rect_limit_recoalesces_after_forced_merge() {
    let surface = surface(100, 100);
    let top_left = DamageRect {
        x: 0,
        y: 0,
        width: 10,
        height: 10,
    };
    let bottom_left = DamageRect {
        x: 0,
        y: 20,
        width: 10,
        height: 10,
    };
    let far_top = DamageRect {
        x: 70,
        y: 0,
        width: 10,
        height: 10,
    };
    let far_bottom = DamageRect {
        x: 70,
        y: 20,
        width: 10,
        height: 10,
    };
    let bridge = DamageRect {
        x: 20,
        y: 0,
        width: 10,
        height: 30,
    };
    let mut rects = vec![top_left, bottom_left, far_top, far_bottom];

    push_damage_rect(&mut rects, bridge, surface);

    assert_eq!(rects.len(), 3);
    assert!(rects.contains(&DamageRect {
        x: 0,
        y: 0,
        width: 30,
        height: 30,
    }));
}

#[test]
fn policy_keeps_below_threshold_extra_damage_as_bounding_rect() {
    let policy = select_damage_policy(metrics(10_000), false, true, 6_600);

    assert_eq!(policy, DisplayListRepaintPolicy::BoundingRect);
}

#[test]
fn policy_promotes_two_thirds_surface_damage_to_full_repaint() {
    let metrics = metrics(9_000);
    let reorder = Some(DamageRect {
        x: 0,
        y: 0,
        width: 60,
        height: 100,
    });

    let effective = select_effective_damage(metrics, surface(90, 100), false, reorder, None);

    assert!(effective.full_surface);
    assert_eq!(effective.rect, Some(surface(90, 100)));
    assert_eq!(effective.policy, DisplayListRepaintPolicy::FullSurface);
}

#[test]
fn policy_promotes_large_bounding_damage_to_full_repaint() {
    let metrics = DisplayListMetrics {
        surface_area: 10_000,
        ..Default::default()
    };
    let reorder = Some(DamageRect {
        x: 0,
        y: 0,
        width: 82,
        height: 82,
    });

    let effective = select_effective_damage(metrics, surface(100, 100), false, reorder, None);

    assert!(effective.full_surface);
    assert_eq!(effective.rect, Some(surface(100, 100)));
    assert_eq!(effective.policy, DisplayListRepaintPolicy::FullSurface);
}

#[test]
fn policy_promotes_tree_rebuild_when_three_quarters_entries_changed() {
    let metrics = DisplayListMetrics {
        surface_area: 10_000,
        entries_total: 8,
        entries_rebuilt: 5,
        entries_removed: 1,
        ..Default::default()
    };

    let policy = select_damage_policy(metrics, true, false, 1_000);

    assert_eq!(policy, DisplayListRepaintPolicy::FullSurface);
}

#[test]
fn policy_keeps_tree_rebuild_below_entry_threshold_non_full_surface() {
    let metrics = DisplayListMetrics {
        surface_area: 10_000,
        entries_total: 8,
        entries_rebuilt: 5,
        entries_removed: 0,
        ..Default::default()
    };

    let policy = select_damage_policy(metrics, true, false, 1_000);

    assert_eq!(policy, DisplayListRepaintPolicy::MinimalDamage);
}
