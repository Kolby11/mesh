use super::super::*;
use super::common::*;

#[test]
fn frontend_renderer_can_be_constructed_with_pluggable_paint_backend() {
    let engine = FrontendRenderEngine::with_paint_backend(Box::<TestPaintBackend>::default());
    assert_eq!(engine.paint_backend_id(), "test");
}

#[test]
fn painter_command_contract_constructs_required_command_set() {
    let clip = full_clip(16, 16);
    let rect = ClipRect {
        x: 1,
        y: 2,
        width: 8,
        height: 9,
    };
    let paint = PainterPaint::fill(Color::WHITE);
    let commands = vec![
        PainterCommand::PushClip(PainterClip { rect, radius: 2.0 }),
        PainterCommand::PopClip,
        PainterCommand::PushLayer(PainterLayer::isolated(clip, 0.5, PainterBlendMode::SrcOver)),
        PainterCommand::PopLayer,
        PainterCommand::DrawRect { rect, paint, clip },
        PainterCommand::DrawRoundedRect {
            rect,
            radius: 4.0,
            paint,
            clip,
        },
        PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![
                    PainterPathElement::MoveTo(0.0, 0.0),
                    PainterPathElement::LineTo(4.0, 4.0),
                    PainterPathElement::QuadTo(5.0, 5.0, 6.0, 6.0),
                    PainterPathElement::CubicTo(1.0, 1.0, 2.0, 2.0, 3.0, 3.0),
                    PainterPathElement::Close,
                ],
            },
            paint,
            clip,
        },
        PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path("img".into()),
            },
            rect,
            paint,
            clip,
        },
        PainterCommand::DrawLinearGradient {
            gradient: PainterLinearGradient {
                from: Color::BLACK,
                to: Color::WHITE,
            },
            rect,
            radius: 4.0,
            clip,
        },
        PainterCommand::DrawShadow {
            rect,
            radius: 4.0,
            shadow: BoxShadow::default(),
            clip,
        },
        PainterCommand::ApplyFilter {
            rect,
            radius: 4.0,
            filter: PainterFilter::Backdrop(VisualFilter { blur_radius: 2.0 }),
            clip,
        },
        PainterCommand::ApplyFilter {
            rect,
            radius: 4.0,
            filter: PainterFilter::Blur(VisualFilter { blur_radius: 2.0 }),
            clip,
        },
    ];

    assert_eq!(commands.len(), 12);
}
