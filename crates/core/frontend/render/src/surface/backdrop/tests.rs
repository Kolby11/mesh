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
