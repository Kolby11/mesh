use super::super::*;
use super::common::*;
use crate::PaintCommandClass;
use crate::display_list::{DamageRect, DisplayListRepaintPolicy, RetainedDisplayList};
use mesh_core_elements::layout::LayoutRect;

#[test]
fn multi_region_selected_paint_matches_repeated_single_region_replay() {
    let (list, clips) = multi_region_fixture();
    let damages: Vec<_> = clips
        .iter()
        .map(|&(x, y, width, height)| DamageRect {
            x,
            y,
            width,
            height,
        })
        .collect();
    let selected =
        list.select_paint_commands_for_rects(&damages, DisplayListRepaintPolicy::MinimalDamage);
    let engine = FrontendRenderEngine::new();
    let mut repeated = PixelBuffer::new(512, 512);
    let mut batched = PixelBuffer::new(512, 512);
    let mut attributed = PixelBuffer::new(512, 512);

    for &clip in &clips {
        engine.render_selected_display_list_for_module(
            &selected,
            &mut repeated,
            1.0,
            Some(clip),
            None,
            None,
        );
    }
    engine.render_selected_display_list_regions_for_module(
        &selected,
        &mut batched,
        1.0,
        &clips,
        None,
        None,
    );
    let attribution = engine.render_selected_display_list_regions_for_module_with_attribution(
        &selected,
        &mut attributed,
        1.0,
        &clips,
        None,
        None,
    );

    assert_eq!(batched.data, repeated.data);
    assert_eq!(attributed.data, repeated.data);
    assert!(attribution.get(PaintCommandClass::Primitive).command_count > 0);
}

#[test]
fn overlapping_multi_region_paint_preserves_region_major_replay() {
    let (list, _) = multi_region_fixture();
    let clips = [(0, 0, 48, 32), (24, 0, 48, 32)];
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 72,
            height: 32,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let engine = FrontendRenderEngine::new();
    let mut repeated = PixelBuffer::new(512, 512);
    let mut batched = PixelBuffer::new(512, 512);

    for &clip in &clips {
        engine.render_selected_display_list_for_module(
            &selected,
            &mut repeated,
            1.0,
            Some(clip),
            None,
            None,
        );
    }
    engine.render_selected_display_list_regions_for_module(
        &selected,
        &mut batched,
        1.0,
        &clips,
        None,
        None,
    );

    assert_eq!(batched.data, repeated.data);
}

#[test]
#[ignore = "release-only multi-rectangle raster-session benchmark"]
fn batched_damage_regions_beat_repeated_raster_sessions() {
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    let (list, all_clips) = multi_region_fixture();
    let all_damages: Vec<_> = all_clips
        .iter()
        .map(|&(x, y, width, height)| DamageRect {
            x,
            y,
            width,
            height,
        })
        .collect();
    let selected =
        list.select_paint_commands_for_rects(&all_damages, DisplayListRepaintPolicy::MinimalDamage);

    for region_count in [1_usize, 4, 16] {
        let clips = &all_clips[..region_count];
        let mut repeated_total = Duration::ZERO;
        let mut batched_total = Duration::ZERO;
        let mut repeated = PixelBuffer::new(512, 512);
        let mut batched = PixelBuffer::new(512, 512);

        for _ in 0..40 {
            paint_regions_repeated(&selected, &mut repeated, clips);
            paint_regions_batched(&selected, &mut batched, clips);
        }

        for sample in 0..6 {
            if sample % 2 == 0 {
                let started = Instant::now();
                for _ in 0..250 {
                    paint_regions_repeated(&selected, &mut repeated, clips);
                }
                repeated_total += started.elapsed();

                let started = Instant::now();
                for _ in 0..250 {
                    paint_regions_batched(&selected, &mut batched, clips);
                }
                batched_total += started.elapsed();
            } else {
                let started = Instant::now();
                for _ in 0..250 {
                    paint_regions_batched(&selected, &mut batched, clips);
                }
                batched_total += started.elapsed();

                let started = Instant::now();
                for _ in 0..250 {
                    paint_regions_repeated(&selected, &mut repeated, clips);
                }
                repeated_total += started.elapsed();
            }
        }

        assert_eq!(batched.data, repeated.data);
        black_box(batched.get_pixel(4, 4));
        let speedup = repeated_total.as_secs_f64() / batched_total.as_secs_f64();
        eprintln!(
            "multi-region raster ({region_count} rects): repeated={repeated_total:?} batched={batched_total:?} speedup={speedup:.3}x"
        );
        eprintln!("MESH_PERF metric=multi_damage_{region_count}_rect_speedup value={speedup:.6}");
        match region_count {
            4 => assert!(
                speedup >= 2.805,
                "four-region batching must retain the checked 3.30x baseline within 15%"
            ),
            16 => assert!(
                speedup >= 4.7175,
                "sixteen-region batching must retain the checked 5.55x baseline within 15%"
            ),
            _ => {}
        }
    }
}

#[test]
fn attributed_selected_paint_preserves_pixels_and_classifies_commands() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 96.0,
            height: 32.0,
        },
        Color {
            r: 20,
            g: 30,
            b: 40,
            a: 255,
        },
    );
    root.children
        .push(text_node("profile me", 4.0, 4.0, 80.0, 20.0, Color::WHITE));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 96, 32, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 96,
            height: 32,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    let engine = FrontendRenderEngine::new();
    let mut normal = PixelBuffer::new(96, 32);
    engine.render_selected_display_list_for_module(&selected, &mut normal, 1.0, None, None, None);
    let mut attributed = PixelBuffer::new(96, 32);
    let metrics = engine.render_selected_display_list_for_module_with_attribution(
        &selected,
        &mut attributed,
        1.0,
        None,
        None,
        None,
    );

    assert_eq!(attributed.data, normal.data);
    assert!(
        metrics.get(PaintCommandClass::Primitive).command_count > 0,
        "the root's batched self paint must be attributed as a primitive"
    );
    assert!(
        metrics.get(PaintCommandClass::Text).command_count > 0,
        "the text node must be attributed as text"
    );
}

#[test]
fn blend_mode_multiply_and_screen_composite_with_backdrop() {
    let red = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let blue = Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };
    let clip = full_clip(8, 8);
    let rect = clip;

    // Multiply: red (255,0,0) * blue (0,0,255) / 255 = black on every channel.
    let mut buffer = PixelBuffer::new(8, 8);
    let mut diagnostics = Vec::new();
    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(red),
                clip,
            },
            PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(blue).with_blend_mode(PainterBlendMode::Multiply),
                clip,
            },
        ],
        &mut diagnostics,
    );
    assert!(diagnostics.is_empty(), "blend modes must not diagnose");
    let p = pixel(&buffer, 4, 4);
    assert!(
        p.r < 8 && p.g < 8 && p.b < 8,
        "multiply over red should be black, got {p:?}"
    );

    // Screen: 255 - (255-dst)*(255-src)/255 → red over blue yields magenta.
    let mut buffer = PixelBuffer::new(8, 8);
    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(red),
                clip,
            },
            PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(blue).with_blend_mode(PainterBlendMode::Screen),
                clip,
            },
        ],
        &mut Vec::new(),
    );
    let p = pixel(&buffer, 4, 4);
    assert!(
        p.r > 247 && p.g < 8 && p.b > 247,
        "screen of red and blue should be magenta, got {p:?}"
    );
}

#[test]
fn checked_checkbox_rasterizes_checkmark_glyph() {
    let layout = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 24.0,
        height: 24.0,
    };
    let bg = Color {
        r: 10,
        g: 20,
        b: 30,
        a: 255,
    };
    let white = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    let count_light_pixels = |node: &WidgetNode| {
        let engine = FrontendRenderEngine::new();
        let mut buffer = PixelBuffer::new(24, 24);
        engine.render_tree(node, &mut buffer, 1.0);
        let mut light = 0;
        for y in 0..24 {
            for x in 0..24 {
                let p = pixel(&buffer, x, y);
                if p.r > 180 && p.g > 180 && p.b > 180 {
                    light += 1;
                }
            }
        }
        light
    };

    let mut checked = node("checkbox", layout, bg);
    checked.computed_style.color = white;
    checked.attributes.insert("checked".into(), "true".into());
    assert!(
        count_light_pixels(&checked) > 0,
        "a checked checkbox must rasterize a light checkmark over its dark box"
    );

    let mut unchecked = node("checkbox", layout, bg);
    unchecked.computed_style.color = white;
    assert_eq!(
        count_light_pixels(&unchecked),
        0,
        "an unchecked checkbox paints no checkmark"
    );
}
