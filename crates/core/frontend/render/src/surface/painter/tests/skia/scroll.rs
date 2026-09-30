use super::super::super::*;
use super::super::common::*;
use crate::display_list::{DamageRect, DisplayListRepaintPolicy, RetainedDisplayList};
use crate::{FractionalScale, RenderObjectTree};
use mesh_core_elements::LayoutRect;
use mesh_core_elements::style::{Overflow, Position};

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
        outer.attributes.insert("_mesh_scroll_y".into(), "0".into());
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
        nested
            .attributes
            .insert("_mesh_scroll_y".into(), "0".into());
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
            root.children[0]
                .attributes
                .insert("_mesh_scroll_y".into(), outer_offset.to_string());
            root.children[0].children[4]
                .attributes
                .insert("_mesh_scroll_y".into(), inner_offset.to_string());
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
