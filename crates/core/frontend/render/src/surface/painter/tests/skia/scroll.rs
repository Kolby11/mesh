use super::super::super::*;
use super::super::common::*;
use crate::display_list::{DamageRect, DisplayListRepaintPolicy, RetainedDisplayList};
use crate::{FractionalScale, RenderObjectTree};
use mesh_core_elements::LayoutRect;
use mesh_core_elements::Prop;
use mesh_core_elements::style::{Overflow, Position};

#[test]
fn shifted_content_raster_with_exposed_strip_repair_matches_full_text_paint() {
    for scale in [1.0, 1.25, 2.0] {
        let scaling = FractionalScale::new(scale);
        let mut root = node(
            "scroll-area",
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 112.0,
                height: 64.0,
            },
            Color::TRANSPARENT,
        );
        root.id = 1;
        root.computed_style.overflow_y = Overflow::Hidden;
        root.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
            max_y: 336.0,
            content_width: 112.0,
            content_height: 400.0,
            ..Default::default()
        });
        for index in 0..20 {
            let mut text = text_node(
                &format!("row {index}"),
                4.0,
                index as f32 * 20.0,
                100.0,
                18.0,
                Color::from_hex("#b0e040").unwrap(),
            );
            text.id = 2 + index;
            root.children.push(text);
        }
        let width = scaling.physical_extent(112);
        let height = scaling.physical_extent(64);
        let engine = FrontendRenderEngine::new();
        let mut raster = PixelBuffer::new(width, height);
        engine.render_tree(&root, &mut raster, scale);
        let mut previous = 0.0;
        for offset in [4.0, 12.0, 8.0, 52.0, 0.0, 120.0] {
            let displacement = (previous - offset) * scale;
            assert_eq!(displacement.fract(), 0.0);
            let strips = raster.shift_raster_pixels(0, displacement as i32);
            root.scroll_metrics.as_mut().unwrap().y = offset;
            let mut list = RetainedDisplayList::default();
            list.update(&root, 112, 64, true, true);
            let selected = list.select_paint_commands(
                Some(DamageRect {
                    x: 0,
                    y: 0,
                    width: 112,
                    height: 64,
                }),
                DisplayListRepaintPolicy::FullSurface,
            );
            for strip in strips {
                engine.render_selected_display_list_for_module(
                    &selected,
                    &mut raster,
                    scale,
                    Some((strip.x, strip.y, strip.width, strip.height)),
                    None,
                    None,
                );
            }
            let mut fresh = PixelBuffer::new(width, height);
            engine.render_tree(&root, &mut fresh, scale);
            assert!(
                raster.data() == fresh.data(),
                "scale={scale}, offset={offset}"
            );
            previous = offset;
        }
    }
}

#[test]
fn retained_content_scope_scroll_matches_full_paint_with_text_and_damage() {
    for scale in [1.0, 1.25, 2.0] {
        let scaling = FractionalScale::new(scale);
        let mut root = node(
            "scroll-area",
            LayoutRect {
                x: 4.0,
                y: 6.0,
                width: 112.0,
                height: 68.0,
            },
            Color::from_hex("#203040").unwrap(),
        );
        root.id = 1;
        root.computed_style.overflow_y = Overflow::Hidden;
        root.computed_style.border_radius = mesh_core_elements::style::Corners::all(12.0);
        root.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
            max_y: 332.0,
            content_width: 112.0,
            content_height: 400.0,
            ..Default::default()
        });
        for index in 0..20 {
            let mut text = text_node(
                &format!("row {index}"),
                8.0,
                8.0 + index as f32 * 20.0,
                100.0,
                18.0,
                Color::from_hex("#d0e0f0").unwrap(),
            );
            text.id = 2 + index;
            root.children.push(text);
        }
        let width = scaling.physical_extent(120);
        let height = scaling.physical_extent(80);
        let engine = FrontendRenderEngine::new();
        let mut objects = RenderObjectTree::default();
        objects.update(&root);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 120, 80, true, true);
        let mut buffer = PixelBuffer::new(width, height);
        engine.render_tree(&root, &mut buffer, scale);
        for (step, offset) in [24.0, 200.5, 12.25, 0.0, 40.0, 60.0, 64.0, 10.0]
            .into_iter()
            .enumerate()
        {
            root.scroll_metrics.as_mut().unwrap().y = offset;
            if step == 2 {
                root.children[1]
                    .attributes
                    .insert(Prop::Content, "changed row".into());
            }
            if step == 4 {
                root.children[1].computed_style.opacity = 0.5;
            }
            if step == 5 {
                root.children[1].computed_style.opacity = 1.0;
            }
            if step == 6 {
                root.layout.width = 104.0;
                root.scroll_metrics.as_mut().unwrap().content_height = 420.0;
                root.scroll_metrics.as_mut().unwrap().max_y = 352.0;
            }
            let dirty = objects.update(&root);
            let metrics = list.update_with_dirty_nodes(
                &root,
                dirty,
                objects.dirty_node_ids(),
                120,
                80,
                false,
                true,
            );
            if step < 4 && step != 2 {
                assert_eq!(metrics.entries_rebuilt, 0);
                assert_eq!(metrics.subtree_commands_rebuilt, 3);
            }
            let damage = list.damage_rects().to_vec();
            let selected = list
                .select_paint_commands_for_rects(&damage, DisplayListRepaintPolicy::MinimalDamage);
            for rect in damage {
                let rect = scaling.clip_damage_rect(rect, width, height).unwrap();
                buffer.clear_rect(rect.x, rect.y, rect.width, rect.height, Color::TRANSPARENT);
                engine.render_selected_display_list_for_module(
                    &selected,
                    &mut buffer,
                    scale,
                    Some((rect.x, rect.y, rect.width, rect.height)),
                    None,
                    None,
                );
            }
            let mut fresh = PixelBuffer::new(width, height);
            engine.render_tree(&root, &mut fresh, scale);
            assert!(buffer.data() == fresh.data(), "scale={scale}, step={step}");
        }
    }
}

#[test]
fn nested_scroll_inside_retained_content_matches_fresh_paint() {
    let mut root = node(
        "scroll-area",
        LayoutRect {
            x: 4.0,
            y: 4.0,
            width: 80.0,
            height: 64.0,
        },
        Color::from_hex("#203040").unwrap(),
    );
    root.id = 1;
    root.computed_style.overflow_y = Overflow::Hidden;
    root.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
        max_y: 100.0,
        content_height: 164.0,
        ..Default::default()
    });
    let mut inner = node(
        "scroll-area",
        LayoutRect {
            x: 12.0,
            y: 40.0,
            width: 50.0,
            height: 32.0,
        },
        Color::from_hex("#507090").unwrap(),
    );
    inner.id = 2;
    inner.computed_style.overflow_y = Overflow::Scroll;
    inner.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
        max_y: 100.0,
        content_height: 132.0,
        ..Default::default()
    });
    let mut text = text_node(
        "nested text",
        16.0,
        88.0,
        40.0,
        18.0,
        Color::from_hex("#e0f040").unwrap(),
    );
    text.id = 3;
    inner.children.push(text);
    root.children.push(inner);
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(100, 80);
    engine.render_tree(&root, &mut buffer, 1.0);
    let mut objects = RenderObjectTree::default();
    objects.update(&root);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 100, 80, true, true);
    assert_eq!(list.frame_paint_plan().scroll_scopes().len(), 1);
    for (outer, inner) in [(20.0, 0.0), (20.0, 44.0), (12.25, 55.5), (0.0, 0.0)] {
        root.scroll_metrics.as_mut().unwrap().y = outer;
        root.children[0].scroll_metrics.as_mut().unwrap().y = inner;
        let dirty = objects.update(&root);
        list.update_with_dirty_nodes(&root, dirty, objects.dirty_node_ids(), 100, 80, false, true);
        let damage = list.damage_rects().to_vec();
        assert!(!damage.is_empty());
        let selected =
            list.select_paint_commands_for_rects(&damage, DisplayListRepaintPolicy::MinimalDamage);
        for rect in damage {
            buffer.clear_rect(rect.x, rect.y, rect.width, rect.height, Color::TRANSPARENT);
            engine.render_selected_display_list_for_module(
                &selected,
                &mut buffer,
                1.0,
                Some((rect.x, rect.y, rect.width, rect.height)),
                None,
                None,
            );
        }
        let mut fresh = PixelBuffer::new(100, 80);
        engine.render_tree(&root, &mut fresh, 1.0);
        assert!(
            buffer.data() == fresh.data(),
            "outer={outer}, inner={inner}"
        );
    }
}

/// Locks the pixel contract before scroll commands move to content space:
/// newly exposed children, nested clips, fixed content, reverse scrolling,
/// and a simultaneous material update must survive partial retained replay.
#[test]
fn retained_scroll_reveals_content_and_matches_fresh_full_paint() {
    for scale in [1.0, 1.25] {
        let mut root = node(
            "box",
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 80.0,
            },
            Color::from_hex("#182838").unwrap(),
        );
        root.id = 1;
        let mut outer = node(
            "scroll-area",
            LayoutRect {
                x: 8.0,
                y: 8.0,
                width: 70.0,
                height: 60.0,
            },
            Color::from_hex("#304050").unwrap(),
        );
        outer.id = 2;
        outer.computed_style.overflow_y = Overflow::Hidden;
        outer.scroll_metrics.get_or_insert_default().y = 0.0;
        for (index, y) in [12.0, 44.0, 82.0, 120.0].into_iter().enumerate() {
            let mut child = node(
                "box",
                LayoutRect {
                    x: 12.0,
                    y,
                    width: 24.0,
                    height: 20.0,
                },
                Color {
                    r: 60 + index as u8 * 40,
                    g: 90,
                    b: 140,
                    a: 255,
                },
            );
            child.id = 3 + index as u64;
            outer.children.push(child);
        }
        let mut nested = node(
            "scroll-area",
            LayoutRect {
                x: 40.0,
                y: 28.0,
                width: 30.0,
                height: 30.0,
            },
            Color::from_hex("#508030").unwrap(),
        );
        nested.id = 7;
        nested.computed_style.overflow_y = Overflow::Hidden;
        nested.scroll_metrics.get_or_insert_default().y = 0.0;
        let mut inner = node(
            "box",
            LayoutRect {
                x: 44.0,
                y: 62.0,
                width: 20.0,
                height: 18.0,
            },
            Color::from_hex("#e0b030").unwrap(),
        );
        inner.id = 8;
        nested.children.push(inner);
        outer.children.push(nested);
        let mut fixed = node(
            "box",
            LayoutRect {
                x: 85.0,
                y: 5.0,
                width: 10.0,
                height: 10.0,
            },
            Color::from_hex("#c03090").unwrap(),
        );
        fixed.id = 9;
        fixed.computed_style.position = Position::Fixed;
        outer.children.push(fixed);
        root.children.push(outer);

        let engine = FrontendRenderEngine::new();
        let scaling = FractionalScale::new(scale);
        let width = scaling.physical_extent(100);
        let height = scaling.physical_extent(80);
        let mut buffer = PixelBuffer::new(width, height);
        let mut objects = RenderObjectTree::default();
        objects.update(&root);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 100, 80, true, true);
        let selected = list.select_paint_commands(
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 100,
                height: 80,
            }),
            DisplayListRepaintPolicy::FullSurface,
        );
        engine.render_selected_display_list_for_module(
            &selected,
            &mut buffer,
            scale,
            None,
            None,
            None,
        );
        let initial = buffer.data().to_vec();

        for (step, (outer_offset, inner_offset)) in [
            (24.0, 16.0),
            (64.5, 24.25),
            (100.0, 0.0),
            (12.25, 8.0),
            (0.0, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            root.children[0].scroll_metrics.get_or_insert_default().y = outer_offset;
            root.children[0].children[4]
                .scroll_metrics
                .get_or_insert_default()
                .y = inner_offset;
            if step == 1 {
                root.children[0].children[2].computed_style.background_color =
                    Color::from_hex("#20d060").unwrap();
            }
            let dirty = objects.update(&root);
            list.update_with_dirty_nodes(
                &root,
                dirty,
                objects.dirty_node_ids(),
                100,
                80,
                false,
                true,
            );
            let damage = list.damage_rects().to_vec();
            assert!(!damage.is_empty());
            let selected = list
                .select_paint_commands_for_rects(&damage, DisplayListRepaintPolicy::MinimalDamage);
            for rect in damage {
                let rect = scaling.clip_damage_rect(rect, width, height).unwrap();
                buffer.clear_rect(rect.x, rect.y, rect.width, rect.height, Color::TRANSPARENT);
                engine.render_selected_display_list_for_module(
                    &selected,
                    &mut buffer,
                    scale,
                    Some((rect.x, rect.y, rect.width, rect.height)),
                    None,
                    None,
                );
            }
            let mut fresh = PixelBuffer::new(width, height);
            engine.render_tree(&root, &mut fresh, scale);
            assert!(buffer.data() == fresh.data(), "scale={scale}, step={step}");
            if step == 0 {
                assert_ne!(
                    buffer.data(),
                    initial.as_slice(),
                    "scroll must change visible pixels"
                );
            }
        }
    }
}

#[test]
fn layout_change_damages_only_scroll_viewports_whose_scope_moved() {
    fn scroll_area(id: u64, x: f32) -> WidgetNode {
        let mut area = node(
            "scroll-area",
            LayoutRect {
                x,
                y: 4.0,
                width: 60.0,
                height: 48.0,
            },
            Color::from_hex("#203040").unwrap(),
        );
        area.id = id;
        area.computed_style.overflow_y = Overflow::Hidden;
        area.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
            max_y: 112.0,
            content_width: 60.0,
            content_height: 160.0,
            ..Default::default()
        });
        for index in 0..8 {
            let mut text = text_node(
                &format!("row {index}"),
                4.0,
                4.0 + index as f32 * 20.0,
                52.0,
                18.0,
                Color::from_hex("#d0e0f0").unwrap(),
            );
            text.id = id * 100 + index;
            area.children.push(text);
        }
        area
    }
    let mut root = node(
        "column",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 80.0,
        },
        Color::from_hex("#101010").unwrap(),
    );
    root.id = 1;
    root.children.push(scroll_area(2, 4.0));
    root.children.push(scroll_area(3, 80.0));
    let mut label = text_node(
        "label",
        4.0,
        58.0,
        40.0,
        16.0,
        Color::from_hex("#f0f0f0").unwrap(),
    );
    label.id = 4;
    root.children.push(label);

    let engine = FrontendRenderEngine::new();
    let mut objects = RenderObjectTree::default();
    objects.update(&root);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 160, 80, true, true);
    let mut buffer = PixelBuffer::new(160, 80);
    engine.render_tree(&root, &mut buffer, 1.0);
    let left = DamageRect {
        x: 4,
        y: 4,
        width: 60,
        height: 48,
    };
    for step in 0..4 {
        match step {
            0 => root.children[2].layout.width = 72.0,
            1 => root.children[1].layout.x = 88.0,
            2 => root.children[0].scroll_metrics.as_mut().unwrap().y = 30.0,
            _ => root.children[2].layout.width = 48.0,
        }
        let dirty = objects.update(&root);
        list.update_with_dirty_nodes(&root, dirty, objects.dirty_node_ids(), 160, 80, false, true);
        let damage = list.damage_rects().to_vec();
        let right = DamageRect {
            x: root.children[1].layout.x as u32,
            y: 4,
            width: 60,
            height: 48,
        };
        let touches = |viewport: DamageRect| damage.iter().any(|rect| rect.intersects(viewport));
        match step {
            0 | 3 => assert!(
                !touches(left) && !touches(right),
                "step {step}: a label resize must not damage either viewport: {damage:?}"
            ),
            1 => assert!(
                touches(right) && !touches(left),
                "step {step}: only the moved viewport is damaged: {damage:?}"
            ),
            _ => assert!(touches(left), "step {step}: scrolled viewport is damaged"),
        }
        let selected =
            list.select_paint_commands_for_rects(&damage, DisplayListRepaintPolicy::MinimalDamage);
        for rect in damage {
            buffer.clear_rect(rect.x, rect.y, rect.width, rect.height, Color::TRANSPARENT);
            engine.render_selected_display_list_for_module(
                &selected,
                &mut buffer,
                1.0,
                Some((rect.x, rect.y, rect.width, rect.height)),
                None,
                None,
            );
        }
        let mut fresh = PixelBuffer::new(160, 80);
        engine.render_tree(&root, &mut fresh, 1.0);
        assert!(buffer.data() == fresh.data(), "step={step}");
    }
}
