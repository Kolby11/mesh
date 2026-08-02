use super::super::super::*;
use super::super::common::*;
use mesh_core_elements::layout::LayoutRect;
use mesh_core_elements::style::Edges;
use std::path::PathBuf;

#[test]
fn skia_shape_rect_fill_uses_command_clip() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();
    let rect = ClipRect {
        x: 2,
        y: 2,
        width: 10,
        height: 10,
    };
    let clip = ClipRect {
        x: 4,
        y: 4,
        width: 4,
        height: 4,
    };

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawRect {
            rect,
            paint: PainterPaint::fill(Color::from_hex("#ff0000").unwrap()),
            clip,
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 3, 3), Color::TRANSPARENT);
    assert_eq!(pixel(&buffer, 4, 4), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 7, 7), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 8, 8), Color::TRANSPARENT);
}

#[test]
fn skia_shape_rect_fill_respects_transparency() {
    let mut buffer = PixelBuffer::new(12, 12);
    let mut diagnostics = Vec::new();
    let color = Color {
        r: 20,
        g: 40,
        b: 60,
        a: 128,
    };

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawRect {
            rect: ClipRect {
                x: 2,
                y: 2,
                width: 6,
                height: 6,
            },
            paint: PainterPaint::fill(color),
            clip: full_clip(12, 12),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 4, 4), color);
    assert_eq!(pixel(&buffer, 1, 1), Color::TRANSPARENT);
}

#[test]
fn skia_effect_layer_opacity_isolates_child_pixels() {
    let mut buffer = PixelBuffer::new(12, 12);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushLayer(PainterLayer::isolated(
                full_clip(12, 12),
                0.5,
                PainterBlendMode::SrcOver,
            )),
            PainterCommand::DrawRect {
                rect: ClipRect {
                    x: 2,
                    y: 2,
                    width: 6,
                    height: 6,
                },
                paint: PainterPaint::fill(Color::from_hex("#ff0000").unwrap()),
                clip: full_clip(12, 12),
            },
            PainterCommand::PopLayer,
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let color = pixel(&buffer, 4, 4);
    assert_eq!(color.r, 255);
    assert!((120..=136).contains(&color.a), "{color:?}");
}

#[test]
fn skia_effect_layer_blur_expands_painted_pixels() {
    let mut buffer = PixelBuffer::new(24, 24);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushLayer(PainterLayer::blurred(
                full_clip(24, 24),
                VisualFilter { blur_radius: 3.0 },
                BlurQuality::default(),
            )),
            PainterCommand::DrawRect {
                rect: ClipRect {
                    x: 8,
                    y: 8,
                    width: 8,
                    height: 8,
                },
                paint: PainterPaint::fill(Color::from_hex("#00ff00").unwrap()),
                clip: full_clip(24, 24),
            },
            PainterCommand::PopLayer,
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(pixel(&buffer, 6, 12).a > 0);
    assert_eq!(pixel(&buffer, 0, 0), Color::TRANSPARENT);
}

#[test]
fn tooltip_chrome_is_drawn_inside_painter_layer() {
    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(96, 48);

    engine.render_tooltip("Audio", 20.0, 10.0, &mut buffer, 1.0);

    let commands = recorded.recorded_commands();
    let classes = painter_command_classes(&commands);
    assert!(
        classes.windows(4).any(|window| window
            == [
                "push_layer",
                "draw_rounded_rect",
                "draw_rounded_rect",
                "pop_layer",
            ]),
        "{classes:?}"
    );
}

#[test]
fn tooltip_rounded_corner_outside_shape_stays_transparent_to_underlay() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(96, 48);
    buffer.clear(Color::WHITE);

    engine.render_tooltip("Audio", 20.0, 10.0, &mut buffer, 1.0);

    assert_eq!(
        pixel(&buffer, 19, 9),
        Color::WHITE,
        "tooltip chrome layer must not prefill pixels outside the rounded corner"
    );
}

#[test]
fn long_tooltip_paints_past_legacy_width_cap() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(360, 72);
    let underlay = Color::from_hex("#224466").unwrap();
    buffer.clear(underlay);

    engine.render_tooltip(
        "Audio output volume is controlled by the system mixer device",
        8.0,
        10.0,
        &mut buffer,
        1.0,
    );

    assert_ne!(
        pixel(&buffer, 300, 20),
        underlay,
        "long tooltip chrome should extend beyond the old 240px overlay width"
    );
}

#[test]
fn tooltip_clipped_repaint_does_not_mutate_pixels_outside_damage() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(96, 48);
    let underlay = Color::from_hex("#224466").unwrap();
    buffer.clear(underlay);

    engine.render_tooltip_clipped("Audio", 20.0, 10.0, &mut buffer, 1.0, Some((24, 12, 8, 8)));

    assert_eq!(
        pixel(&buffer, 23, 16),
        underlay,
        "tooltip paint must not touch pixels left of the clipped damage rect"
    );
    assert_eq!(
        pixel(&buffer, 40, 16),
        underlay,
        "tooltip paint must not touch pixels right of the clipped damage rect"
    );
}

#[test]
fn skia_effect_linear_gradient_draws_top_and_bottom_colors() {
    let mut buffer = PixelBuffer::new(8, 12);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawLinearGradient {
            gradient: PainterLinearGradient {
                from: Color::from_hex("#ff0000").unwrap(),
                to: Color::from_hex("#0000ff").unwrap(),
            },
            rect: ClipRect {
                x: 0,
                y: 0,
                width: 8,
                height: 12,
            },
            radius: 0.0,
            clip: full_clip(8, 12),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let top = pixel(&buffer, 4, 1);
    let bottom = pixel(&buffer, 4, 10);
    assert!(top.r > top.b, "{top:?}");
    assert!(bottom.b > bottom.r, "{bottom:?}");
}

#[test]
fn skia_effect_linear_gradient_reuses_shader_for_moving_same_size_rects() {
    super::super::super::backend::reset_gradient_shader_cache_for_tests();
    let mut buffer = PixelBuffer::new(24, 20);
    let mut diagnostics = Vec::new();
    let gradient = PainterLinearGradient {
        from: Color::from_hex("#ff0000").unwrap(),
        to: Color::from_hex("#0000ff").unwrap(),
    };

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::DrawLinearGradient {
                gradient,
                rect: ClipRect {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 12,
                },
                radius: 0.0,
                clip: full_clip(24, 20),
            },
            PainterCommand::DrawLinearGradient {
                gradient,
                rect: ClipRect {
                    x: 12,
                    y: 4,
                    width: 8,
                    height: 12,
                },
                radius: 0.0,
                clip: full_clip(24, 20),
            },
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let moved_top = pixel(&buffer, 16, 5);
    let moved_bottom = pixel(&buffer, 16, 14);
    assert!(moved_top.r > moved_top.b, "{moved_top:?}");
    assert!(moved_bottom.b > moved_bottom.r, "{moved_bottom:?}");
    assert_eq!(
        super::super::super::backend::gradient_shader_creations_for_tests(),
        1
    );
}

#[test]
#[ignore = "release-only moving gradient shader cache microbenchmark"]
fn moving_gradient_shader_size_key_beats_position_churn_benchmark() {
    let iterations = 5_000;
    let gradient = PainterLinearGradient {
        from: Color::from_hex("#ff0000").unwrap(),
        to: Color::from_hex("#0000ff").unwrap(),
    };
    let mut buffer = PixelBuffer::new(96, 48);
    let mut diagnostics = Vec::new();

    let old_started = std::time::Instant::now();
    for i in 0..iterations {
        super::super::super::backend::reset_gradient_shader_cache_for_tests();
        SkiaPaintBackend.execute_commands(
            &mut buffer,
            &[PainterCommand::DrawLinearGradient {
                gradient,
                rect: ClipRect {
                    x: (i % 72) as i32,
                    y: (i % 24) as i32,
                    width: 24,
                    height: 24,
                },
                radius: 0.0,
                clip: full_clip(96, 48),
            }],
            &mut diagnostics,
        );
    }
    let old_time = old_started.elapsed();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    diagnostics.clear();
    super::super::super::backend::reset_gradient_shader_cache_for_tests();
    let new_started = std::time::Instant::now();
    for i in 0..iterations {
        SkiaPaintBackend.execute_commands(
            &mut buffer,
            &[PainterCommand::DrawLinearGradient {
                gradient,
                rect: ClipRect {
                    x: (i % 72) as i32,
                    y: (i % 24) as i32,
                    width: 24,
                    height: 24,
                },
                radius: 0.0,
                clip: full_clip(96, 48),
            }],
            &mut diagnostics,
        );
    }
    let new_time = new_started.elapsed();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    println!(
        "moving gradient shader cache: position churn {old_time:?}; size-key reuse {new_time:?}; ratio {:.1}x; shader_creations={}",
        old_time.as_secs_f64() / new_time.as_secs_f64(),
        super::super::super::backend::gradient_shader_creations_for_tests()
    );
    assert!(
        new_time < old_time,
        "size-keyed moving gradients should beat position-churned shader creation"
    );
}

#[test]
fn skia_effect_image_draws_source_pixels() {
    let (_fixture, path) = write_effect_test_image("phase55-image-source.png");
    let mut buffer = PixelBuffer::new(20, 10);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path(path.clone()),
            },
            rect: ClipRect {
                x: 0,
                y: 0,
                width: 20,
                height: 10,
            },
            paint: PainterPaint::fill(Color::WHITE),
            clip: full_clip(20, 10),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(pixel(&buffer, 4, 5).r > pixel(&buffer, 4, 5).g);
    assert!(pixel(&buffer, 15, 5).g > pixel(&buffer, 15, 5).r);
}

#[test]
fn skia_effect_image_respects_command_clip() {
    let (_fixture, path) = write_effect_test_image("phase55-image-clip.png");
    let mut buffer = PixelBuffer::new(20, 10);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path(path),
            },
            rect: ClipRect {
                x: 0,
                y: 0,
                width: 20,
                height: 10,
            },
            paint: PainterPaint::fill(Color::WHITE),
            clip: ClipRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(pixel(&buffer, 4, 5).a > 0);
    assert_eq!(pixel(&buffer, 15, 5), Color::TRANSPARENT);
}

#[test]
fn skia_effect_image_gradient_suite_runs_supported_cases() {
    skia_effect_linear_gradient_draws_top_and_bottom_colors();
    skia_effect_image_draws_source_pixels();
    skia_effect_image_respects_command_clip();
}

#[test]
fn painter_effect_clipped_shadow_stays_inside_clip() {
    let mut buffer = PixelBuffer::new(32, 32);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawShadow {
            rect: ClipRect {
                x: 8,
                y: 8,
                width: 10,
                height: 10,
            },
            radius: 0.0,
            shadow: BoxShadow {
                offset_x: 1.0,
                offset_y: 1.0,
                blur_radius: 0.0,
                spread_radius: 0.0,
                color: Color::from_hex("#000000ff").unwrap(),
                inset: false,
            },
            clip: ClipRect {
                x: 10,
                y: 10,
                width: 4,
                height: 4,
            },
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 9, 9), Color::TRANSPARENT);
    assert_eq!(
        pixel(&buffer, 10, 10),
        Color::from_hex("#000000ff").unwrap()
    );
    assert_eq!(pixel(&buffer, 14, 14), Color::TRANSPARENT);
}

#[test]
fn painter_effect_gradient_respects_rounded_clip() {
    let mut buffer = PixelBuffer::new(24, 24);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawLinearGradient {
            gradient: PainterLinearGradient {
                from: Color::from_hex("#ff0000").unwrap(),
                to: Color::from_hex("#0000ff").unwrap(),
            },
            rect: ClipRect {
                x: 4,
                y: 4,
                width: 16,
                height: 16,
            },
            radius: 8.0,
            clip: full_clip(24, 24),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 4, 4), Color::TRANSPARENT);
    assert!(pixel(&buffer, 12, 12).a > 0);
}

#[test]
fn skia_shape_push_clip_intersects_command_clip() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushClip(PainterClip {
                rect: ClipRect {
                    x: 4,
                    y: 4,
                    width: 4,
                    height: 4,
                },
                radius: 0.0,
            }),
            PainterCommand::DrawRect {
                rect: ClipRect {
                    x: 2,
                    y: 2,
                    width: 10,
                    height: 10,
                },
                paint: PainterPaint::fill(Color::from_hex("#ff0000").unwrap()),
                clip: full_clip(16, 16),
            },
            PainterCommand::PopClip,
            PainterCommand::DrawRect {
                rect: ClipRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                paint: PainterPaint::fill(Color::from_hex("#0000ff").unwrap()),
                clip: full_clip(16, 16),
            },
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 3, 3), Color::TRANSPARENT);
    assert_eq!(pixel(&buffer, 4, 4), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 0, 0), Color::from_hex("#0000ff").unwrap());
}

#[test]
fn skia_border_square_border_matches_existing_pixels() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = Color::from_hex("#ff0000").unwrap();

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 1, 1), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
}

#[test]
fn skia_border_rounded_border_keeps_corners_clear() {
    let border = Color::from_hex("#ff0000").unwrap();
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = border;
    root.computed_style.border_radius.top_left = 8.0;

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 0, 0), Color::TRANSPARENT);
    assert!(pixel(&buffer, 10, 0).a > 0);
    assert!(pixel(&buffer, 0, 10).a > 0);
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
}

#[test]
fn skia_path_fill_triangle_paints_expected_pixels() {
    let mut buffer = PixelBuffer::new(18, 14);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![
                    PainterPathElement::MoveTo(2.0, 11.0),
                    PainterPathElement::LineTo(9.0, 2.0),
                    PainterPathElement::LineTo(16.0, 11.0),
                    PainterPathElement::Close,
                ],
            },
            paint: PainterPaint::fill(Color::from_hex("#00ff00").unwrap()),
            clip: full_clip(18, 14),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 9, 7), Color::from_hex("#00ff00").unwrap());
    assert_eq!(pixel(&buffer, 1, 1), Color::TRANSPARENT);
}

#[test]
fn skia_path_stroke_line_paints_expected_pixels() {
    let mut buffer = PixelBuffer::new(18, 8);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![
                    PainterPathElement::MoveTo(2.0, 4.0),
                    PainterPathElement::LineTo(16.0, 4.0),
                ],
            },
            paint: PainterPaint::stroke(Color::from_hex("#0000ff").unwrap(), 2.0),
            clip: full_clip(18, 8),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(pixel(&buffer, 9, 4).a > 0);
    assert_eq!(pixel(&buffer, 9, 0), Color::TRANSPARENT);
}

#[test]
fn skia_text_highlight_selection_background_uses_theme_color() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text].into();

    let mut buffer = PixelBuffer::new(180, 80);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    let saw_selection_background = buffer.data.chunks_exact(4).any(|px| {
        Color {
            b: px[0],
            g: px[1],
            r: px[2],
            a: px[3],
        } == Color::from_hex("#00ff00").unwrap()
    });
    assert!(saw_selection_background);
}

#[test]
fn skia_text_highlight_does_not_change_glyph_handoff() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text].into();

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(180, 80);
    engine.render_tree(&root, &mut buffer, 1.0);

    let classes = painter_command_classes(&recorded.recorded_commands());
    assert!(classes.contains(&"draw_rect"));
    assert!(!classes.contains(&"draw_text"));
}

#[test]
fn painter_command_contract_keeps_retained_structures_free_of_skia_types() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut sources = vec![manifest_dir.join("src/render_object.rs")];
    sources.extend(
        std::fs::read_dir(manifest_dir.join("src/display_list"))
            .expect("display_list module directory")
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs")),
    );

    for path in sources {
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            !contents.contains("skia_safe"),
            "{} must stay backend-neutral",
            path.display()
        );
    }
}
