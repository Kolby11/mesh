use super::super::super::*;
use super::super::common::*;
use mesh_core_elements::style::{Corners, Overflow};
use mesh_core_elements::{LayoutRect, WidgetScrollMetrics};

fn bounds() -> ClipRect {
    ClipRect {
        x: 8,
        y: 7,
        width: 80,
        height: 54,
    }
}

fn effect_commands(case: usize, clip: ClipRect, image: &str) -> Vec<PainterCommand> {
    if case == 5 {
        let mut commands = effect_commands(2, clip, image);
        commands.extend(effect_commands(3, clip, image).into_iter().skip(1));
        return commands;
    }
    let rect = bounds();
    let radii = Corners {
        top_left: 17.5,
        top_right: 9.0,
        bottom_right: 21.0,
        bottom_left: 4.25,
    };
    let mut commands = vec![PainterCommand::DrawRect {
        rect: full_clip(104, 80),
        paint: PainterPaint::fill(Color::from_hex("#263447").unwrap()),
        clip,
    }];
    match case {
        0 | 4 => commands.push(PainterCommand::DrawLinearGradient {
            gradient: PainterLinearGradient {
                intermediate: if case == 4 {
                    vec![
                        Color::from_css("lime").unwrap(),
                        Color::from_css("rgba(255, 255, 0, 0.5)").unwrap(),
                    ]
                } else {
                    Vec::new()
                },
                from: Color {
                    r: 240,
                    g: 100,
                    b: 40,
                    a: 130,
                },
                to: Color {
                    r: 30,
                    g: 100,
                    b: 240,
                    a: 220,
                },
            },
            rect,
            radii,
            clip,
        }),
        1 => {
            commands.push(PainterCommand::PushClip(PainterClip::shape(rect, radii, Default::default())));
            commands.push(PainterCommand::PushClip(PainterClip {
                rect: ClipRect {
                    x: 12,
                    y: 10,
                    width: 70,
                    height: 44,
                },
                radii: Corners::all(16.0),
                shapes: Default::default(),
                mode: crate::surface::painter::PainterClipMode::Inside,
            }));
            commands.push(PainterCommand::DrawImage {
                image: PainterImage {
                    source: PainterImageSource::Path(image.into()),
                },
                rect,
                paint: PainterPaint::fill(Color {
                    a: 190,
                    ..Color::WHITE
                }),
                clip,
            });
            commands.push(PainterCommand::PopClip);
            commands.push(PainterCommand::PopClip);
        }
        _ => commands.push(PainterCommand::DrawShadow {
            rect,
            radii,
            shadow: mesh_core_elements::BoxShadow {
                offset_x: -2.5,
                offset_y: 3.0,
                spread_radius: 1.5,
                blur_radius: if case == 2 { 5.0 } else { 0.0 },
                color: Color {
                    r: 90,
                    g: 190,
                    b: 240,
                    a: 170,
                },
                inset: false,
            },
            clip,
        }),
    }
    commands
}

#[test]
fn rounded_gradient_repaints_identically_under_damage_clips() {
    assert_effect_repaint(0);
}
#[test]
fn rounded_image_repaints_identically_under_damage_clips() {
    assert_effect_repaint(1);
}
#[test]
fn blurred_shadow_repaints_identically_under_damage_clips() {
    assert_effect_repaint(2);
}
#[test]
fn sharp_shadow_repaints_identically_under_damage_clips() {
    assert_effect_repaint(3);
}

fn assert_effect_repaint(case: usize) {
    let (_fixture, path) = write_effect_test_image("rounded-damage.png");
    let outside = Color::from_hex("#982675").unwrap();
    {
        let paint = |clip: ClipRect| {
            let mut buffer = PixelBuffer::new(104, 80);
            buffer.clear(outside);
            let mut diagnostics = Vec::new();
            SkiaPaintBackend.execute_commands(
                &mut buffer,
                &effect_commands(case, clip, &path),
                &mut diagnostics,
            );
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            buffer
        };
        let full = paint(full_clip(104, 80));
        for start in [0, 7, 8, 11, 16, 22, 31, 46, 60, 72] {
            for end in [12, 23, 38, 57, 69, 80, 104] {
                if end <= start {
                    continue;
                }
                for clip in [
                    ClipRect {
                        x: start,
                        y: 0,
                        width: end - start,
                        height: 80,
                    },
                    ClipRect {
                        x: 0,
                        y: start,
                        width: 104,
                        height: end - start,
                    },
                    ClipRect {
                        x: start,
                        y: start,
                        width: end - start,
                        height: end - start,
                    },
                ] {
                    let partial = paint(clip);
                    for y in 0..80 {
                        for x in 0..104 {
                            let inside = x >= clip.x
                                && x < clip.x + clip.width
                                && y >= clip.y
                                && y < clip.y + clip.height;
                            let expected = if inside {
                                pixel(&full, x as u32, y as u32)
                            } else {
                                outside
                            };
                            assert_eq!(
                                pixel(&partial, x as u32, y as u32),
                                expected,
                                "case {case}, ({x},{y}), clip {clip:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn rounded_overflow_and_background_images_clip_retained_content() {
    let (_fixture, path) = write_effect_test_image("rounded-overflow.png");
    for scale in [1.0, 1.25, 2.0] {
        for scroll in [false, true] {
            let mut root = node(
                "box",
                LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 104.0,
                    height: 80.0,
                },
                Color::TRANSPARENT,
            );
            let mut parent = node(
                "box",
                LayoutRect {
                    x: 8.0,
                    y: 8.0,
                    width: 80.0,
                    height: 56.0,
                },
                Color::TRANSPARENT,
            );
            parent.computed_style.border_radius = Corners::all(20.0);
            parent.computed_style.overflow_y = Overflow::Hidden;
            if scroll {
                parent.scroll_metrics = Some(WidgetScrollMetrics {
                    y: 5.0,
                    max_y: 30.0,
                    content_height: 86.0,
                    content_width: 80.0,
                    ..Default::default()
                });
            }
            let mut child = node(
                "box",
                LayoutRect {
                    x: 8.0,
                    y: 8.0,
                    width: 80.0,
                    height: 86.0,
                },
                Color::WHITE,
            );
            child.computed_style.background_paint =
                mesh_core_elements::style::BackgroundPaint::Image(
                    mesh_core_elements::style::StyleImageSource { path: path.clone() },
                );
            child.computed_style.border_radius = Corners::all(8.0);
            parent.children.push(child);
            root.children.push(parent);
            let engine = FrontendRenderEngine::new();
            let (width, height) = ((104.0 * scale) as u32, (80.0 * scale) as u32);
            let mut full = PixelBuffer::new(width, height);
            engine.render_tree(&root, &mut full, scale);
            assert_eq!(
                pixel(&full, (8.0 * scale) as u32, (8.0 * scale) as u32),
                Color::TRANSPARENT
            );
            assert!(pixel(&full, (40.0 * scale) as u32, (30.0 * scale) as u32).a > 0);
            for (x, y, w, h) in [(0, 0, 23, 80), (12, 9, 80, 12), (0, 42, 104, 15)] {
                let clip = (
                    (x as f32 * scale) as u32,
                    (y as f32 * scale) as u32,
                    (w as f32 * scale) as u32,
                    (h as f32 * scale) as u32,
                );
                let mut partial = PixelBuffer::new(width, height);
                engine.render_tree_at_for_module_clipped(
                    &root,
                    &mut partial,
                    scale,
                    0.0,
                    0.0,
                    clip,
                    None,
                );
                for py in 0..height {
                    for px in 0..width {
                        let inside = px >= clip.0
                            && px < clip.0 + clip.2
                            && py >= clip.1
                            && py < clip.1 + clip.3;
                        let expected = if inside {
                            pixel(&full, px, py)
                        } else {
                            Color::TRANSPARENT
                        };
                        assert_eq!(
                            pixel(&partial, px, py),
                            expected,
                            "scroll={scroll} scale={scale} ({px},{py})"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn rounded_overflow_radius_changes_refresh_clean_descendant_clips() {
    use crate::display_list::{DisplayListRepaintPolicy, RetainedDisplayList};
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.overflow_y = Overflow::Hidden;
    root.computed_style.border_radius = Corners::all(20.0);
    root.children.push(node("box", root.layout, Color::WHITE));
    let mut objects = crate::RenderObjectTree::default();
    let mut list = RetainedDisplayList::default();
    let engine = FrontendRenderEngine::new();
    objects.update(&root);
    list.update(&root, 80, 60, false, true);
    for radius in [8.0, 0.0, 20.0] {
        root.computed_style.border_radius = Corners::all(radius);
        let dirty = objects.update(&root);
        assert!(dirty.clip > 0);
        list.update_with_dirty_nodes(&root, dirty, objects.dirty_node_ids(), 80, 60, false, true);
        let selected = list.select_paint_commands(
            Some(crate::display_list::DamageRect {
                x: 0,
                y: 0,
                width: 80,
                height: 60,
            }),
            DisplayListRepaintPolicy::FullSurface,
        );
        let mut retained = PixelBuffer::new(80, 60);
        engine.render_selected_display_list_for_module(
            &selected,
            &mut retained,
            1.0,
            None,
            None,
            None,
        );
        let mut fresh = PixelBuffer::new(80, 60);
        engine.render_tree(&root, &mut fresh, 1.0);
        assert!(retained.data() == fresh.data(), "radius={radius}");
    }
}

#[test]
#[ignore = "release rounded-effect raster cost comparison"]
fn rounded_effect_raster_cost_workload() {
    use skia_safe::gradient as gradient_api;
    use skia_safe::{BlurStyle, Color4f, MaskFilter, Paint, Point, RRect, Rect, TileMode, Vector};
    use std::time::Instant;
    let rects: Vec<_> = (0..140)
        .map(|i| ClipRect {
            x: 12 + (i % 10) * 90,
            y: 12 + (i / 10) * 62,
            width: 80,
            height: 54,
        })
        .collect();
    let radii = Corners::all(12.0);
    let from = Color {
        r: 240,
        g: 100,
        b: 40,
        a: 170,
    };
    let to = Color {
        r: 30,
        g: 100,
        b: 240,
        a: 220,
    };
    let colors = [
        Color4f::from(crate::surface::buffer::skia_color(from)),
        Color4f::from(crate::surface::buffer::skia_color(to)),
    ];
    let shader = gradient_api::shaders::linear_gradient(
        (Point::new(0.0, 0.0), Point::new(0.0, 54.0)),
        &gradient_api::Gradient::new(
            gradient_api::Colors::new_evenly_spaced(colors.as_slice(), TileMode::Clamp, None),
            gradient_api::Interpolation::default(),
        ),
        None,
    )
    .unwrap();
    let mut buffer = PixelBuffer::new(920, 900);
    for shadow in [false, true] {
        let commands: Vec<_> = rects
            .iter()
            .map(|rect| {
                if shadow {
                    PainterCommand::DrawShadow {
                        rect: *rect,
                        radii,
                        shadow: mesh_core_elements::BoxShadow {
                            blur_radius: 5.0,
                            color: from,
                            ..Default::default()
                        },
                        clip: full_clip(920, 900),
                    }
                } else {
                    PainterCommand::DrawLinearGradient {
                        rect: *rect,
                        radii,
                        gradient: PainterLinearGradient {
                            intermediate: Vec::new(),
                            from,
                            to,
                        },
                        clip: full_clip(920, 900),
                    }
                }
            })
            .collect();
        let mut diagnostics = Vec::new();
        SkiaPaintBackend.execute_commands(&mut buffer, &commands, &mut diagnostics);
        for sample in 0..4 {
            let start = Instant::now();
            for _ in 0..20 {
                let _ = buffer.with_skia_canvas(|canvas| {
                    for rect in &rects {
                        let save = canvas.save();
                        canvas.clip_rect(Rect::from_xywh(0.0, 0.0, 920.0, 900.0), None, false);
                        canvas.translate((rect.x as f32, rect.y as f32));
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        if shadow {
                            paint.set_color(crate::surface::buffer::skia_color(from));
                            paint.set_mask_filter(MaskFilter::blur(
                                BlurStyle::Normal,
                                5.0 * 0.57735 + 0.5,
                                Some(false),
                            ));
                        } else {
                            paint.set_shader(shader.clone());
                        }
                        canvas.draw_rrect(
                            RRect::new_rect_radii(
                                Rect::from_wh(80.0, 54.0),
                                &[Vector::new(12.0, 12.0); 4],
                            ),
                            &paint,
                        );
                        canvas.restore_to_count(save);
                    }
                });
            }
            let direct = start.elapsed();
            let start = Instant::now();
            for _ in 0..20 {
                SkiaPaintBackend.execute_commands(&mut buffer, &commands, &mut diagnostics);
            }
            eprintln!(
                "rounded_effect shadow={shadow} sample={sample} shapes=140 frames=20 direct_reference={direct:?} stable={:?}",
                start.elapsed()
            );
            assert!(diagnostics.is_empty());
        }
    }
}

#[test]
fn css_profile_rounded_multistop_gradient_repaints_exactly_under_damage_clips() {
    assert_effect_repaint(4);
}

#[test]
fn css_profile_multiple_shadows_repaint_exactly_under_damage_clips() {
    assert_effect_repaint(5);
}
