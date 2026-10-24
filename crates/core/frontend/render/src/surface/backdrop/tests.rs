use super::*;
use std::time::{Duration, Instant};

thread_local! {
    static REFERENCE_EFFECT: RuntimeEffect = RuntimeEffect::make_for_shader(include_str!("reference.sksl"), None).unwrap();
}

fn reference_lens_shader(
    rect: Rect,
    radii: Corners,
    filter: VisualFilter,
    backdrop: Shader,
) -> Option<Shader> {
    REFERENCE_EFFECT.with(|effect| make_lens_shader(effect, rect, radii, filter, backdrop))
}

fn source() -> BackdropSource {
    let mut surface = surfaces::raster_n32_premul((128, 64)).unwrap();
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    for x in 0..128 {
        let mut paint = Paint::default();
        paint.set_color(skia_safe::Color::from_argb(
            100 + x as u8,
            (x * 2) as u8,
            170,
            255 - x as u8,
        ));
        surface
            .canvas()
            .draw_rect(Rect::from_xywh(x as f32, 0.0, 1.0, 64.0), &paint);
    }
    BackdropSource {
        path: PathBuf::new(),
        image: surface.image_snapshot(),
        prepared: Mutex::new(Vec::new()),
    }
}

fn backdrop(image: &skia_safe::Image, origin: (f32, f32)) -> Shader {
    image
        .to_shader(
            (TileMode::Clamp, TileMode::Clamp),
            SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
            &Matrix::translate((-origin.0, -origin.1)),
        )
        .unwrap()
}

fn pixels(surface: &mut skia_safe::Surface) -> Vec<u8> {
    let info = surface.image_info();
    let mut pixels = vec![0; info.compute_min_byte_size()];
    assert!(surface.read_pixels(&info, &mut pixels, info.min_row_bytes(), (0, 0)));
    pixels
}

#[test]
fn unrefracted_backdrop_matches_lens_pixels() {
    let source = source();
    for blur in [0.0, 6.0] {
        let image = source.prepared(160, 90, blur).unwrap();
        for origin in [(0.0, 0.0), (27.5, 13.25), (-12.0, -7.0)] {
            for radii in [
                Corners::zero(),
                Corners {
                    top_left: 6.0,
                    top_right: 11.0,
                    bottom_right: 3.0,
                    bottom_left: 17.0,
                },
            ] {
                let mut reference = surfaces::raster_n32_premul((96, 64)).unwrap();
                let mut optimized = surfaces::raster_n32_premul((96, 64)).unwrap();
                reference.canvas().clear(skia_safe::Color::TRANSPARENT);
                optimized.canvas().clear(skia_safe::Color::TRANSPARENT);
                let rect = Rect::from_xywh(3.25, 4.5, 88.0, 51.0);
                let filter = VisualFilter {
                    refract: true,
                    blur_radius: blur,
                    ..VisualFilter::NONE
                };
                let lens =
                    reference_lens_shader(rect, radii, filter, backdrop(&image, origin)).unwrap();
                draw_shader(reference.canvas(), rect, radii, lens);
                assert!(draw_backdrop(
                    optimized.canvas(),
                    rect,
                    radii,
                    filter,
                    backdrop(&image, origin)
                ));
                let before = pixels(&mut reference);
                let after = pixels(&mut optimized);
                let max_error = before
                    .iter()
                    .zip(&after)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    max_error <= 1,
                    "max channel error={max_error}, blur={blur}, origin={origin:?}, radii={radii:?}"
                );
            }
        }
    }
}

#[test]
fn lens_effects_keep_the_reference_pixels() {
    let source = source();
    let image = source.prepared(160, 90, 6.0).unwrap();
    for (edge, depth, dispersion) in [
        (12.0, 18.0, 3.0),
        (12.0, 18.0, 0.0),
        (12.0, 0.0, 0.0),
        (0.0, 0.0, 3.0),
    ] {
        let mut reference = surfaces::raster_n32_premul((96, 64)).unwrap();
        let mut optimized = surfaces::raster_n32_premul((96, 64)).unwrap();
        reference.canvas().clear(skia_safe::Color::TRANSPARENT);
        optimized.canvas().clear(skia_safe::Color::TRANSPARENT);
        let rect = Rect::from_xywh(4.0, 4.0, 88.0, 56.0);
        let radii = Corners::all(16.0);
        let filter = VisualFilter {
            refract: true,
            refract_depth: depth,
            refract_edge: edge,
            dispersion,
            ..VisualFilter::NONE
        };
        draw_shader(
            reference.canvas(),
            rect,
            radii,
            reference_lens_shader(rect, radii, filter, backdrop(&image, (0.0, 0.0))).unwrap(),
        );
        assert!(draw_backdrop(
            optimized.canvas(),
            rect,
            radii,
            filter,
            backdrop(&image, (0.0, 0.0))
        ));
        assert_eq!(pixels(&mut reference), pixels(&mut optimized));
    }
}

fn timed_draws(
    image: &skia_safe::Image,
    size: (i32, i32),
    filter: VisualFilter,
    reference: bool,
    frames: usize,
) -> Duration {
    let mut surface = surfaces::raster_n32_premul(size).unwrap();
    let rect = Rect::from_wh(size.0 as f32, size.1 as f32);
    let radii = if filter.refract_edge > 0.0 {
        Corners::all(25.0_f32.min(size.1 as f32 / 2.0))
    } else {
        Corners::zero()
    };
    let start = Instant::now();
    for _ in 0..frames {
        let shader = backdrop(image, (0.0, 0.0));
        if reference {
            draw_shader(
                surface.canvas(),
                rect,
                radii,
                reference_lens_shader(rect, radii, filter, shader).unwrap(),
            );
        } else {
            assert!(draw_backdrop(surface.canvas(), rect, radii, filter, shader));
        }
        std::hint::black_box(&surface);
    }
    start.elapsed()
}

#[test]
#[ignore = "release-only glass raster profile and unrefracted wallpaper gate"]
fn glass_raster_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    let source = source();
    let plain = VisualFilter {
        refract: true,
        ..VisualFilter::NONE
    };
    let glass = VisualFilter {
        refract: true,
        refract_depth: 40.0,
        refract_edge: 30.0,
        dispersion: 4.0,
        blur_radius: 6.0,
    };
    for size in [(1920, 1080), (3840, 2160)] {
        let started = Instant::now();
        let image = source.prepared(size.0, size.1, 0.0).unwrap();
        let cold = started.elapsed();
        let _ = timed_draws(&image, size, plain, true, 2);
        let _ = timed_draws(&image, size, plain, false, 2);
        for run in 0..3 {
            let (before, after) = if run % 2 == 0 {
                (
                    timed_draws(&image, size, plain, true, 10),
                    timed_draws(&image, size, plain, false, 10),
                )
            } else {
                let after = timed_draws(&image, size, plain, false, 10);
                (timed_draws(&image, size, plain, true, 10), after)
            };
            eprintln!(
                "wallpaper {size:?} run={run} cold_prepare={cold:?} frames=10 lens={before:?} direct={after:?} ratio={:.2}",
                before.as_secs_f64() / after.as_secs_f64()
            );
            assert!(
                after.as_secs_f64() * 1.2 < before.as_secs_f64(),
                "unrefracted_wallpaper_speedup must exceed 1.2x"
            );
        }
    }
    let started = Instant::now();
    let _ = source.prepared(1920, 1080, glass.blur_radius).unwrap();
    eprintln!("glass cold_prepare={:?}", started.elapsed());
    for size in [(180, 40), (360, 240), (1920, 1080)] {
        let glass = if size.1 == 40 {
            VisualFilter {
                refract_depth: 12.0,
                refract_edge: 11.0,
                dispersion: 1.5,
                blur_radius: 5.0,
                ..glass
            }
        } else {
            glass
        };
        let image = source.prepared(1920, 1080, glass.blur_radius).unwrap();
        let _ = timed_draws(&image, size, glass, true, 2);
        let _ = timed_draws(&image, size, glass, false, 2);
        for run in 0..3 {
            let elapsed = timed_draws(&image, size, glass, false, 10);
            eprintln!(
                "glass {size:?} run={run} frames=10 total={elapsed:?} per_frame={:?}",
                elapsed / 10
            );
        }
    }
}

fn full_resolution_blur(source: &BackdropSource, size: (i32, i32), blur: f32) -> Vec<u8> {
    let mut surface = surfaces::raster_n32_premul(size).unwrap();
    let (iw, ih) = (source.image.width() as f32, source.image.height() as f32);
    let scale = (size.0 as f32 / iw).max(size.1 as f32 / ih);
    let (dw, dh) = (iw * scale, ih * scale);
    let mut paint = Paint::default();
    paint.set_image_filter(image_filters::blur(
        (blur / 2.0, blur / 2.0),
        TileMode::Clamp,
        None,
        None,
    ));
    surface.canvas().draw_image_rect_with_sampling_options(
        &source.image,
        None,
        Rect::from_xywh(
            (size.0 as f32 - dw) / 2.0,
            (size.1 as f32 - dh) / 2.0,
            dw,
            dh,
        ),
        SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear),
        &paint,
    );
    pixels(&mut surface)
}

fn banded_source() -> BackdropSource {
    let mut surface = surfaces::raster_n32_premul((256, 160)).unwrap();
    for band in 0..32 {
        let mut paint = Paint::default();
        paint.set_color(if band % 2 == 0 {
            skia_safe::Color::from_argb(255, 250, 240, 20)
        } else {
            skia_safe::Color::from_argb(255, 10, 30, 200)
        });
        surface
            .canvas()
            .draw_rect(Rect::from_xywh(band as f32 * 8.0, 0.0, 8.0, 160.0), &paint);
    }
    BackdropSource {
        path: PathBuf::new(),
        image: surface.image_snapshot(),
        prepared: Mutex::new(Vec::new()),
    }
}

#[test]
fn downsampled_blur_matches_full_resolution_blur() {
    let source = banded_source();
    let size = (330, 190);
    for blur in [8.0, 20.0, 48.0] {
        let image = source.prepared(size.0, size.1, blur).unwrap();
        assert!(image.width() < size.0, "blur {blur} should downsample");
        let mut surface = surfaces::raster_n32_premul(size).unwrap();
        surface.canvas().clear(skia_safe::Color::TRANSPARENT);
        let frame = BackdropFrame {
            origin: (0.0, 0.0),
            output: (size.0 as f32, size.1 as f32),
            source_beneath: true,
        };
        let mut local = Matrix::translate((-frame.origin.0, -frame.origin.1));
        local.pre_scale(
            (
                size.0 as f32 / image.width() as f32,
                size.1 as f32 / image.height() as f32,
            ),
            None,
        );
        let shader = image
            .to_shader(
                (TileMode::Clamp, TileMode::Clamp),
                SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
                &local,
            )
            .unwrap();
        let filter = VisualFilter {
            refract: true,
            blur_radius: blur,
            ..VisualFilter::NONE
        };
        let rect = Rect::from_wh(size.0 as f32, size.1 as f32);
        assert!(draw_backdrop(
            surface.canvas(),
            rect,
            Corners::zero(),
            filter,
            shader
        ));
        let reference = full_resolution_blur(&source, size, blur);
        let actual = pixels(&mut surface);
        let max_error = reference
            .iter()
            .zip(&actual)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        let mean_error = reference
            .iter()
            .zip(&actual)
            .map(|(a, b)| a.abs_diff(*b) as f64)
            .sum::<f64>()
            / reference.len() as f64;
        // Hard 8px bands sit at the blur cutoff, the worst case for resampling;
        // Ganesh's own blur downsamples further than this.
        assert!(
            max_error <= 20,
            "blur {blur}: max channel error {max_error}"
        );
        assert!(
            mean_error <= 5.0,
            "blur {blur}: mean channel error {mean_error}"
        );
    }
}

#[test]
fn unblurred_and_light_blur_copies_keep_output_resolution() {
    let source = source();
    for blur in [0.0, 5.0, 7.5] {
        let image = source.prepared(160, 90, blur).unwrap();
        assert_eq!((image.width(), image.height()), (160, 90), "blur {blur}");
    }
}

#[test]
fn prepared_cache_keeps_every_glass_theme_radius() {
    let source = source();
    let radii = [0.0, 5.0, 8.0, 12.0, 14.0, 16.0, 20.0, 22.0, 24.0];
    let first: Vec<_> = radii
        .iter()
        .map(|blur| source.prepared(2880, 1800, *blur).unwrap().unique_id())
        .collect();
    let second: Vec<_> = radii
        .iter()
        .map(|blur| source.prepared(2880, 1800, *blur).unwrap().unique_id())
        .collect();
    assert_eq!(first, second, "a theme's radii must not evict each other");
}

#[test]
#[ignore = "release-only cold backdrop preparation profile"]
fn backdrop_prepare_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    let mut wallpaper = surfaces::raster_n32_premul((3840, 2160)).unwrap();
    for y in (0..2160).step_by(8) {
        let mut paint = Paint::default();
        paint.set_color(skia_safe::Color::from_argb(
            255,
            (y % 251) as u8,
            (y % 113) as u8,
            140,
        ));
        wallpaper
            .canvas()
            .draw_rect(Rect::from_xywh(0.0, y as f32, 3840.0, 8.0), &paint);
    }
    let image = wallpaper.image_snapshot();
    let image = image.with_default_mipmaps().unwrap_or(image);
    for run in 0..3 {
        let source = BackdropSource {
            path: PathBuf::new(),
            image: image.clone(),
            prepared: Mutex::new(Vec::new()),
        };
        let mut total = Duration::ZERO;
        for blur in [0.0, 5.0, 8.0, 12.0, 14.0, 16.0, 20.0, 22.0, 24.0] {
            let started = Instant::now();
            let prepared = source.prepared(2880, 1800, blur).unwrap();
            let elapsed = started.elapsed();
            total += elapsed;
            eprintln!(
                "run={run} blur={blur} copy={}x{} cold_prepare={elapsed:?}",
                prepared.width(),
                prepared.height()
            );
        }
        eprintln!("run={run} all nine radii cold_prepare={total:?}");
    }
}

#[test]
fn rim_lights_only_the_glass_edge_over_what_is_beneath() {
    let gray = skia_safe::Color::from_argb(255, 60, 60, 60);
    let mut surface = surfaces::raster_n32_premul((96, 64)).unwrap();
    surface.canvas().clear(gray);
    let rect = Rect::from_xywh(8.0, 8.0, 80.0, 48.0);
    let radii = Corners {
        top_left: 12.0,
        top_right: 12.0,
        bottom_right: 12.0,
        bottom_left: 12.0,
    };
    let filter = VisualFilter {
        refract: true,
        refract_depth: 12.0,
        refract_edge: 20.0,
        ..VisualFilter::NONE
    };
    assert!(draw_rim(surface.canvas(), rect, radii, filter));
    let full = pixels(&mut surface);
    let red = |x: usize, y: usize| full[(y * 96 + x) * 4 + 2].max(full[(y * 96 + x) * 4]);
    assert_eq!(red(48, 32), 60, "the interior shows what is beneath");
    assert_eq!(red(2, 2), 60, "outside the glass is untouched");
    assert!(red(48, 8) > 60, "the top edge is lit");
    assert!(
        red(8, 32) > red(87, 32),
        "light comes from the top-left: left {} vs right {}",
        red(8, 32),
        red(87, 32)
    );

    // A partial repaint whose clip cuts the corner arc reproduces the full
    // paint's pixels.
    let mut clipped = surfaces::raster_n32_premul((96, 64)).unwrap();
    clipped.canvas().clear(gray);
    clipped
        .canvas()
        .clip_rect(Rect::from_xywh(13.0, 11.0, 40.0, 40.0), None, false);
    assert!(draw_rim(clipped.canvas(), rect, radii, filter));
    let clipped = pixels(&mut clipped);
    for y in 11..51 {
        for x in 13..53 {
            let index = (y * 96 + x) * 4;
            assert_eq!(
                &clipped[index..index + 4],
                &full[index..index + 4],
                "clipped rim pixel {x},{y}"
            );
        }
    }

    let flat = VisualFilter {
        refract: true,
        ..VisualFilter::NONE
    };
    assert!(!draw_rim(surface.canvas(), rect, radii, flat));
}

#[test]
fn rim_band_clip_matches_shading_the_whole_rect() {
    let corners = |top_left, top_right, bottom_right, bottom_left| Corners {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    };
    let cases = [
        (
            Rect::from_xywh(8.0, 8.0, 160.0, 96.0),
            corners(12.0, 12.0, 12.0, 12.0),
            20.0,
        ),
        (
            Rect::from_xywh(7.3, 9.6, 150.5, 90.2),
            corners(28.0, 4.0, 0.0, 16.0),
            25.0,
        ),
        (
            Rect::from_xywh(10.0, 10.0, 140.0, 80.0),
            corners(0.0, 0.0, 0.0, 0.0),
            8.0,
        ),
        (
            Rect::from_xywh(12.0, 6.0, 40.0, 100.0),
            corners(60.0, 60.0, 60.0, 60.0),
            30.0,
        ),
    ];
    for (rect, radii, edge) in cases {
        let filter = VisualFilter {
            refract: true,
            refract_depth: 12.0,
            refract_edge: edge,
            ..VisualFilter::NONE
        };
        let gray = skia_safe::Color::from_argb(255, 60, 60, 60);
        let mut banded = surfaces::raster_n32_premul((180, 116)).unwrap();
        banded.canvas().clear(gray);
        assert!(draw_rim(banded.canvas(), rect, radii, filter));
        let mut whole = surfaces::raster_n32_premul((180, 116)).unwrap();
        whole.canvas().clear(gray);
        let paint = rim_paint(rect, radii, rim_width(filter)).unwrap();
        whole
            .canvas()
            .draw_rect(rect.with_outset((1.0, 1.0)), &paint);
        assert_eq!(
            pixels(&mut banded),
            pixels(&mut whole),
            "rect {rect:?} radii {radii:?} edge {edge}"
        );
    }
}
