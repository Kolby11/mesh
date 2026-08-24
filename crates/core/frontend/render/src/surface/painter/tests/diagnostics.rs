use super::super::*;
use super::common::*;

#[test]
fn painter_backend_capabilities_identify_skia_and_unsupported_commands_diagnose() {
    let backend = SkiaPaintBackend;
    let capabilities = backend.capabilities();
    assert_eq!(capabilities.backend_id, "skia");
    assert!(capabilities.rects);
    assert!(capabilities.rounded_rects);
    assert!(capabilities.shadows);
    assert!(capabilities.filters);
    assert!(capabilities.clips);
    assert!(capabilities.layers);
    assert!(capabilities.paths);

    assert!(capabilities.blend_modes);

    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();
    backend.execute_commands(
        &mut buffer,
        &[PainterCommand::ApplyFilter {
            rect: full_clip(8, 8),
            radii: mesh_core_elements::style::Corners::zero(),
            filter: PainterFilter::Blur(VisualFilter { blur_radius: 2.0 }),
            clip: full_clip(16, 16),
        }],
        &mut diagnostics,
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].backend_id, "skia");
    assert_eq!(diagnostics[0].feature, UnsupportedPainterFeature::Filter);
}

#[test]
fn painter_effect_diagnostic_reports_excessive_blur() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushLayer(PainterLayer::blurred(
                full_clip(16, 16),
                VisualFilter {
                    blur_radius: MAX_EFFECT_BLUR_RADIUS + 1.0,
                },
                BlurQuality::default(),
            )),
            PainterCommand::DrawRect {
                rect: full_clip(8, 8),
                paint: PainterPaint::fill(Color::WHITE),
                clip: full_clip(16, 16),
            },
            PainterCommand::PopLayer,
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.feature == UnsupportedPainterFeature::Filter
            && diagnostic.message.contains("excessive blur")
            && diagnostic.source.is_none()
    }));
}

#[test]
fn painter_effect_diagnostic_reports_missing_image() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path("target/phase55-effects/missing.png".into()),
            },
            rect: full_clip(8, 8),
            paint: PainterPaint::fill(Color::WHITE),
            clip: full_clip(16, 16),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.feature == UnsupportedPainterFeature::Image
            && diagnostic.message.contains("missing image asset")
            && diagnostic.source.is_none()
    }));
}

#[test]
fn painter_layer_blend_mode_is_supported_without_diagnostics() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushLayer(PainterLayer::isolated(
                full_clip(16, 16),
                1.0,
                PainterBlendMode::Multiply,
            )),
            PainterCommand::PopLayer,
        ],
        &mut diagnostics,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.feature == UnsupportedPainterFeature::BlendMode),
        "blend modes are now applied, not diagnosed as unsupported"
    );
}
