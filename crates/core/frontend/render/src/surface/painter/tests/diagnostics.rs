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
    assert!(capabilities.backdrop_blur);

    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();
    backend.execute_commands(
        &mut buffer,
        &[PainterCommand::ApplyFilter {
            rect: full_clip(8, 8),
            radii: mesh_core_elements::style::Corners::zero(),
            filter: PainterFilter::Blur(VisualFilter::blur(2.0)),
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
                VisualFilter::blur(MAX_EFFECT_BLUR_RADIUS + 1.0),
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

#[test]
fn four_k_image_is_admitted_to_the_skia_image_cache() {
    let dir = std::env::temp_dir().join(format!("mesh-4k-image-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("wallpaper.png");
    image::RgbaImage::from_pixel(3840, 2160, image::Rgba([10, 20, 30, 255]))
        .save(&path)
        .unwrap();
    let mut buffer = PixelBuffer::new(64, 64);
    let mut diagnostics = Vec::new();
    let before = crate::surface::painter::backend::skia_image_cache_len();

    for _ in 0..2 {
        SkiaPaintBackend.execute_commands(
            &mut buffer,
            &[PainterCommand::DrawImage {
                image: PainterImage {
                    source: PainterImageSource::Path(path.to_string_lossy().into_owned()),
                },
                rect: full_clip(64, 64),
                paint: PainterPaint::fill(Color::WHITE),
                clip: full_clip(64, 64),
            }],
            &mut diagnostics,
        );
    }
    let after = crate::surface::painter::backend::skia_image_cache_len();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(after, before + 1);
}
