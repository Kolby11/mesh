use super::super::super::*;
use super::super::common::*;
use crate::display_list::{DamageRect, DisplayListRepaintPolicy, RetainedDisplayList};
use mesh_core_elements::layout::LayoutRect;
use mesh_core_elements::style::Edges;

#[test]
fn painter_helper_lowering_routes_rect_helper_through_command_backend() {
    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(16, 16);
    let rect = ClipRect {
        x: 1,
        y: 2,
        width: 8,
        height: 9,
    };

    engine.fill_rect_clipped(&mut buffer, rect, Color::WHITE, full_clip(16, 16));

    let commands = recorded.recorded_commands();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        commands[0],
        PainterCommand::DrawRect {
            rect: recorded_rect,
            paint: PainterPaint {
                style: PainterPaintStyle::Fill,
                ..
            },
            ..
        } if recorded_rect == rect
    ));
}

#[test]
fn painter_helper_lowering_routes_effect_helpers_through_command_backend() {
    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(32, 32);
    let rect = ClipRect {
        x: 4,
        y: 4,
        width: 12,
        height: 12,
    };
    let clip = full_clip(32, 32);

    engine.fill_rounded_rect_clipped(&mut buffer, rect, 6.0, Color::WHITE, clip);
    engine.draw_box_shadow(
        &mut buffer,
        rect,
        6.0,
        BoxShadow {
            offset_x: 2.0,
            offset_y: 2.0,
            blur_radius: 4.0,
            spread_radius: 1.0,
            color: Color::BLACK,
            inset: false,
        },
        clip,
    );

    let commands = recorded.recorded_commands();
    assert_eq!(commands.len(), 2);
    assert!(matches!(
        commands[0],
        PainterCommand::DrawRoundedRect { .. }
    ));
    assert!(matches!(commands[1], PainterCommand::DrawShadow { .. }));
}

#[test]
fn painter_backend_diagnostics_are_observable_on_frontend_render_engine() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(16, 16);
    engine.execute_painter_commands(
        &mut buffer,
        &[PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path("img".into()),
            },
            rect: full_clip(8, 8),
            paint: PainterPaint::fill(Color::WHITE),
            clip: full_clip(16, 16),
        }],
    );

    let diagnostics = engine.painter_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].backend_id, "skia");
    assert_eq!(diagnostics[0].feature, UnsupportedPainterFeature::Image);

    let snapshot = engine.paint_backend_snapshot();
    assert_eq!(snapshot.backend_id, "skia");
    assert_eq!(snapshot.rollback_authority, "mesh-software-renderer");
    assert!(
        snapshot
            .capabilities
            .iter()
            .any(|capability| capability.feature == "images" && capability.supported)
    );
    assert!(
        snapshot
            .capabilities
            .iter()
            .any(|capability| capability.feature == "text" && !capability.supported)
    );
    assert_eq!(snapshot.recent_diagnostics.len(), 1);
    assert_eq!(snapshot.recent_diagnostics[0].feature, "image");

    engine.clear_painter_diagnostics();
    assert!(engine.painter_diagnostics().is_empty());
    assert!(
        engine
            .paint_backend_snapshot()
            .recent_diagnostics
            .is_empty()
    );
}

#[test]
fn painter_draws_border_from_computed_edges() {
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
fn painter_draws_rounded_border_without_square_corners() {
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
fn painter_applies_opacity_to_skia_filled_background() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::WHITE,
    );
    root.computed_style.opacity = 0.5;

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    let center = pixel(&buffer, 10, 10);
    assert_eq!(
        center,
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 128,
        }
    );
}

#[test]
fn painter_draws_box_shadow_outside_node_bounds() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 8.0,
            y: 8.0,
            width: 16.0,
            height: 16.0,
        },
        Color::from_hex("#ffffff").unwrap(),
    );
    root.computed_style.box_shadow = BoxShadow {
        offset_x: 8.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        spread_radius: 0.0,
        color: Color::from_hex("#000000ff").unwrap(),
        inset: false,
    };

    let mut buffer = PixelBuffer::new(40, 32);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 12, 12), Color::from_hex("#ffffff").unwrap());
    assert_eq!(pixel(&buffer, 28, 12), Color::from_hex("#000000").unwrap());
}

#[test]
fn painter_blurs_background_fill_beyond_node_bounds() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 12.0,
            y: 12.0,
            width: 16.0,
            height: 16.0,
        },
        Color::from_hex("#000000ff").unwrap(),
    );
    root.computed_style.filter = VisualFilter { blur_radius: 4.0 };

    let mut buffer = PixelBuffer::new(40, 40);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert!(pixel(&buffer, 10, 20).a > 0);
    assert!(pixel(&buffer, 10, 20).a < 255);
    assert_eq!(pixel(&buffer, 0, 0), Color::TRANSPARENT);
}

#[test]
fn retained_display_list_paints_opacity_through_skia_path() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::WHITE,
    );
    root.computed_style.opacity = 0.5;

    let mut list = RetainedDisplayList::default();
    list.update(&root, 24, 24, false, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 24,
            height: 24,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_selected_display_list_for_module(
        &selected,
        &mut buffer,
        1.0,
        None,
        None,
        None,
    );

    let center = pixel(&buffer, 10, 10);
    assert_eq!(
        center,
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 128,
        }
    );
}

#[test]
fn painter_clips_children_when_overflow_hidden() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.overflow_x = Overflow::Hidden;
    root.computed_style.overflow_y = Overflow::Hidden;
    root.children = vec![node(
        "box",
        LayoutRect {
            x: 8.0,
            y: 0.0,
            width: 8.0,
            height: 10.0,
        },
        Color::from_hex("#00ff00").unwrap(),
    )]
    .into();

    let mut buffer = PixelBuffer::new(20, 12);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 9, 5), Color::from_hex("#00ff00").unwrap());
    assert_eq!(pixel(&buffer, 11, 5), Color::TRANSPARENT);
}

#[test]
fn compatibility_tree_painter_omits_explicitly_hidden_descendants() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 24.0,
            height: 24.0,
        },
        Color::TRANSPARENT,
    );
    let mut hidden = node(
        "box",
        LayoutRect {
            x: 4.0,
            y: 4.0,
            width: 12.0,
            height: 12.0,
        },
        Color::from_hex("#00ff00").unwrap(),
    );
    hidden.attributes.insert("hidden".into(), "true".into());
    root.children.push(hidden);

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 8, 8), Color::TRANSPARENT);
}

#[test]
fn painter_orders_children_by_z_index() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
        },
        Color::TRANSPARENT,
    );

    let mut bottom = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        Color::from_hex("#ff0000").unwrap(),
    );
    bottom.computed_style.z_index = 0;
    let mut top = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        Color::from_hex("#0000ff").unwrap(),
    );
    top.computed_style.z_index = 1;
    root.children = vec![top, bottom].into();

    let mut buffer = PixelBuffer::new(20, 20);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 5, 5), Color::from_hex("#0000ff").unwrap());
}
