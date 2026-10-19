use super::*;
use skia_safe::{Canvas, Color, Color4f, Paint, Point, RRect, Rect, TileMode, gradient};

/// Hosts without libEGL, the surfaceless platform or a GLES3 driver skip the
/// GPU tests rather than failing; `MESH_REQUIRE_GPU=1` makes absence a failure
/// on machines that are expected to have one.
fn context_or_skip() -> Option<GpuContext> {
    match GpuContext::headless() {
        Ok(context) => Some(context),
        Err(error) if std::env::var_os("MESH_REQUIRE_GPU").is_none() => {
            eprintln!("skipping GPU test: {error}");
            None
        }
        Err(error) => panic!("GPU required but unavailable: {error}"),
    }
}

fn draw_scene(canvas: &Canvas) {
    canvas.clear(Color::from_argb(0, 0, 0, 0));
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    paint.set_color(Color::from_argb(255, 30, 120, 200));
    canvas.draw_rect(Rect::from_xywh(4.0, 4.0, 40.0, 24.0), &paint);

    paint.set_color(Color::from_argb(160, 220, 40, 90));
    canvas.draw_rrect(
        RRect::new_rect_xy(Rect::from_xywh(20.5, 14.25, 52.0, 30.0), 9.0, 9.0),
        &paint,
    );

    let colors = [
        Color4f::from(Color::from_argb(255, 250, 200, 20)),
        Color4f::from(Color::from_argb(64, 20, 200, 120)),
    ];
    let gradient = gradient::Gradient::new(
        gradient::Colors::new_evenly_spaced(&colors, TileMode::Clamp, None),
        gradient::Interpolation::default(),
    );
    paint.set_shader(gradient::shaders::linear_gradient(
        (Point::new(0.0, 48.0), Point::new(96.0, 64.0)),
        &gradient,
        None,
    ));
    canvas.draw_rect(Rect::from_xywh(0.0, 48.0, 96.0, 16.0), &paint);
}

fn raster_reference(width: u32, height: u32) -> PixelBuffer {
    let mut buffer = PixelBuffer::new(width, height);
    assert!(buffer.with_skia_canvas(draw_scene));
    buffer
}

/// Per-pixel worst channel difference between two equally sized buffers.
fn pixel_deltas(left: &PixelBuffer, right: &PixelBuffer) -> Vec<u8> {
    assert_eq!(
        (left.width(), left.height()),
        (right.width(), right.height())
    );
    left.data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(right.data().as_chunks::<4>().0)
        .map(|(a, b)| {
            a.iter()
                .zip(b)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0)
        })
        .collect()
}

#[test]
fn headless_context_reports_a_renderer() {
    let Some(context) = context_or_skip() else {
        return;
    };
    assert!(!context.renderer().is_empty());
}

#[test]
fn gpu_raster_matches_software_raster_within_antialiasing_tolerance() {
    let Some(mut context) = context_or_skip() else {
        return;
    };
    let (width, height) = (96, 64);
    let mut target = context.offscreen_target(width, height).unwrap();
    draw_scene(target.canvas());
    let gpu = target.read_back(&mut context).unwrap();
    let cpu = raster_reference(width, height);

    // Ganesh computes analytic coverage on curved edges where the raster
    // pipeline supersamples, so antialiased curve pixels may differ by tens of
    // levels (measured: 64 of 6,144 pixels, worst 41, on an RTX 4060). Fills, gradients and blending
    // must otherwise agree; a geometry or color error breaks the edge budget.
    let deltas = pixel_deltas(&gpu, &cpu);
    let edge_pixels = deltas.iter().filter(|delta| **delta > 2).count();
    let worst = deltas.iter().copied().max().unwrap_or(0);
    assert!(
        edge_pixels * 50 <= deltas.len() && worst <= 64,
        "{edge_pixels} of {} pixels differ, worst by {worst} levels",
        deltas.len()
    );
    assert!(
        gpu.data()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] == 255)
    );
}

#[test]
fn readback_is_premultiplied_bgra_like_pixel_buffer() {
    let Some(mut context) = context_or_skip() else {
        return;
    };
    let mut target = context.offscreen_target(2, 1).unwrap();
    target.canvas().clear(Color::from_argb(128, 255, 0, 0));
    let pixels = target.read_back(&mut context).unwrap();
    let [b, g, r, a] = pixels.data()[..4] else {
        unreachable!()
    };
    assert_eq!((b, g), (0, 0));
    assert!(a.abs_diff(128) <= 1 && r.abs_diff(128) <= 1, "{r} {a}");
}

#[test]
fn targets_and_context_drop_in_either_order() {
    let Some(mut context) = context_or_skip() else {
        return;
    };
    let early = context.offscreen_target(8, 8).unwrap();
    let late = context.offscreen_target(8, 8).unwrap();
    drop(early);
    drop(context);
    drop(late);
}

mod painter_parity {
    //! The production painter on a GPU target against the same painter on a
    //! `PixelBuffer`, per canonical workload shape: panel chrome and text, an
    //! icon grid, effect layers, and a retained scroll replay that exercises
    //! damage clears and in-place shifts.

    use super::{context_or_skip, pixel_deltas};
    use crate::display_list::{DamageRect, DisplayListRepaintPolicy, RetainedDisplayList};
    use crate::gpu::{GpuContext, GpuTarget};
    use crate::{FrontendRenderEngine, PixelBuffer, PixelCanvasSession};
    use mesh_core_elements::WidgetNode;
    use mesh_core_elements::layout::LayoutRect;
    use mesh_core_elements::style::{
        BackgroundPaint, BoxShadow, Color, Corners, Dimension, Edges, Overflow,
        StyleLinearGradient, VisualFilter,
    };

    const WIDTH: u32 = 160;
    const HEIGHT: u32 = 120;

    /// Budget for antialiasing and filter differences between Ganesh and the
    /// raster pipeline: the share of pixels off by more than 2 levels, and the
    /// worst level. Measured on an RTX 4060 (2026-10-19): panel 2/19,200 px
    /// (worst 3), icon grid 0 (1), effects 404 (4, backdrop and filter blur),
    /// scroll 0 (2).
    struct Budget {
        differing_per_mille: usize,
        worst: u8,
    }

    fn rgb(hex: &str) -> Color {
        Color::from_hex(hex).unwrap()
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> LayoutRect {
        LayoutRect {
            x,
            y,
            width,
            height,
        }
    }

    fn node(id: u64, tag: &str, layout: LayoutRect, color: Color) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.id = id;
        node.layout = layout;
        node.computed_style.width = Dimension::Px(layout.width);
        node.computed_style.height = Dimension::Px(layout.height);
        node.computed_style.background_color = color;
        node
    }

    fn text(id: u64, content: &str, layout: LayoutRect, color: Color) -> WidgetNode {
        let mut node = node(id, "text", layout, Color::TRANSPARENT);
        node.attributes.insert("content".into(), content.into());
        node.computed_style.color = color;
        node.computed_style.font_size = 13.0;
        node.computed_style.line_height = 1.3;
        node.computed_style.padding = Edges::zero();
        node
    }

    fn panel_scene() -> WidgetNode {
        let mut root = node(1, "box", rect(0.0, 0.0, 160.0, 120.0), rgb("#1b2430"));
        let mut card = node(2, "box", rect(10.0, 8.0, 140.0, 56.0), rgb("#2c3a4c"));
        card.computed_style.border_radius = Corners::all(10.0);
        card.computed_style.border_width = Edges::all(1.5);
        card.computed_style.border_color = rgb("#6d8db0");
        card.computed_style.box_shadow = BoxShadow {
            offset_x: 0.0,
            offset_y: 3.0,
            blur_radius: 6.0,
            spread_radius: 0.0,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 140,
            },
            inset: false,
        };
        card.children.push(text(
            3,
            "Volume 42%",
            rect(20.0, 14.0, 120.0, 18.0),
            rgb("#f0f4f8"),
        ));
        card.children.push(text(
            4,
            "Wi-Fi · connected",
            rect(20.0, 36.0, 120.0, 18.0),
            rgb("#a8bccf"),
        ));
        root.children.push(card);
        let mut bar = node(5, "box", rect(10.0, 76.0, 140.0, 12.0), Color::TRANSPARENT);
        bar.computed_style.border_radius = Corners::all(6.0);
        bar.computed_style.background_paint =
            BackgroundPaint::LinearGradient(StyleLinearGradient {
                from: rgb("#40a0ff"),
                to: rgb("#a040ff"),
            });
        root.children.push(bar);
        let mut chip = node(
            6,
            "box",
            rect(10.0, 96.0, 60.0, 18.0),
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 48,
            },
        );
        chip.computed_style.border_radius = Corners::all(9.0);
        root.children.push(chip);
        root
    }

    fn icon_grid_scene() -> WidgetNode {
        let mut root = node(1, "box", rect(0.0, 0.0, 160.0, 120.0), rgb("#202020"));
        for index in 0..20u64 {
            let column = (index % 5) as f32;
            let row = (index / 5) as f32;
            let mut icon = node(
                10 + index,
                "icon",
                rect(8.0 + column * 30.0, 8.0 + row * 28.0, 22.0, 22.0),
                Color::TRANSPARENT,
            );
            icon.attributes
                .insert("name".into(), format!("mesh-parity-missing-{index}"));
            icon.attributes.insert("size".into(), "22".into());
            icon.computed_style.color = rgb(["#ffffff", "#80c0ff", "#ffd060"][index as usize % 3]);
            root.children.push(icon);
        }
        root
    }

    fn effects_scene() -> WidgetNode {
        let mut root = node(1, "box", rect(0.0, 0.0, 160.0, 120.0), Color::TRANSPARENT);
        root.children
            .push(node(2, "box", rect(0.0, 0.0, 80.0, 120.0), rgb("#e04040")));
        root.children
            .push(node(3, "box", rect(80.0, 0.0, 80.0, 120.0), rgb("#4040e0")));
        let mut frosted = node(
            4,
            "box",
            rect(40.0, 20.0, 80.0, 40.0),
            Color {
                r: 255,
                g: 255,
                b: 255,
                a: 40,
            },
        );
        frosted.computed_style.border_radius = Corners::all(12.0);
        frosted.computed_style.backdrop_filter = VisualFilter::blur(6.0);
        root.children.push(frosted);
        let mut blurred = node(5, "box", rect(20.0, 76.0, 50.0, 30.0), rgb("#40e080"));
        blurred.computed_style.filter = VisualFilter::blur(3.0);
        root.children.push(blurred);
        let mut faded = node(6, "box", rect(96.0, 76.0, 48.0, 30.0), rgb("#f0d040"));
        faded.computed_style.opacity = 0.5;
        faded.computed_style.border_radius = Corners::all(8.0);
        root.children.push(faded);
        root
    }

    fn scroll_scene(scroll_y: f32) -> WidgetNode {
        let mut root = node(1, "box", rect(0.0, 0.0, 160.0, 120.0), rgb("#182838"));
        let mut area = node(
            2,
            "scroll-area",
            rect(10.0, 10.0, 120.0, 100.0),
            rgb("#304050"),
        );
        area.computed_style.overflow_y = Overflow::Hidden;
        area.attributes
            .insert("_mesh_scroll_y".into(), scroll_y.to_string());
        for index in 0..8u64 {
            let y = 14.0 + index as f32 * 28.0;
            let mut row = node(
                10 + index,
                "box",
                rect(14.0, y, 112.0, 24.0),
                Color {
                    r: 50 + index as u8 * 20,
                    g: 90,
                    b: 140,
                    a: 255,
                },
            );
            row.computed_style.border_radius = Corners::all(5.0);
            area.children.push(row);
            area.children.push(text(
                30 + index,
                &format!("Row {index}"),
                rect(20.0, y + 4.0, 100.0, 16.0),
                rgb("#ffffff"),
            ));
        }
        root.children.push(area);
        root
    }

    fn full_damage() -> DamageRect {
        DamageRect {
            x: 0,
            y: 0,
            width: WIDTH,
            height: HEIGHT,
        }
    }

    fn display_list(root: &WidgetNode) -> RetainedDisplayList {
        let mut list = RetainedDisplayList::default();
        list.set_backdrop_blur_policy(crate::display_list::BackdropBlurPolicy::InSurfaceFilter);
        list.update(root, WIDTH, HEIGHT, true, true);
        list
    }

    /// Clear `clips` and paint `root` into them; `None` repaints everything.
    fn paint(
        engine: &FrontendRenderEngine,
        session: &mut PixelCanvasSession<'_>,
        root: &WidgetNode,
        clips: Option<&[DamageRect]>,
    ) {
        let list = display_list(root);
        let selected =
            list.select_paint_commands(Some(full_damage()), DisplayListRepaintPolicy::FullSurface);
        let full = [full_damage()];
        let clips = clips.unwrap_or(&full);
        for &clip in clips {
            session.clear_rect(clip, Color::TRANSPARENT);
        }
        let regions: Vec<_> = clips
            .iter()
            .map(|clip| (clip.x, clip.y, clip.width, clip.height))
            .collect();
        engine.render_selected_display_list_regions_for_module_in_session(
            &selected, session, 1.0, &regions, None, None,
        );
    }

    fn gpu_target(context: &mut GpuContext) -> GpuTarget {
        context.offscreen_target(WIDTH, HEIGHT).unwrap()
    }

    fn assert_within(name: &str, gpu: &PixelBuffer, cpu: &PixelBuffer, budget: Budget) {
        let deltas = pixel_deltas(gpu, cpu);
        let differing = deltas.iter().filter(|delta| **delta > 2).count();
        let worst = deltas.iter().copied().max().unwrap_or(0);
        eprintln!(
            "{name}: {differing} of {} pixels differ, worst by {worst} levels",
            deltas.len()
        );
        assert!(
            differing * 1000 <= deltas.len() * budget.differing_per_mille && worst <= budget.worst,
            "{name}: {differing} of {} pixels differ, worst by {worst} levels",
            deltas.len()
        );
        assert!(
            cpu.data().iter().any(|channel| *channel != 0),
            "{name}: reference painted nothing"
        );
    }

    fn assert_full_paint_parity(name: &str, root: &WidgetNode, budget: Budget) -> PixelBuffer {
        let _guard = crate::resource_state_test_lock();
        let engine = FrontendRenderEngine::new();
        let mut cpu = PixelBuffer::new(WIDTH, HEIGHT);
        paint(&engine, &mut PixelCanvasSession::new(&mut cpu), root, None);
        let Some(mut context) = context_or_skip() else {
            return cpu;
        };
        let mut target = gpu_target(&mut context);
        paint(&engine, &mut target.session(), root, None);
        let gpu = target.read_back(&mut context).unwrap();
        assert!(
            engine.painter_diagnostics().is_empty(),
            "{name}: {:?}",
            engine.painter_diagnostics()
        );
        assert_within(name, &gpu, &cpu, budget);
        cpu
    }

    #[test]
    fn panel_chrome_and_text_paint_alike_on_gpu() {
        let cpu = assert_full_paint_parity(
            "panel",
            &panel_scene(),
            Budget {
                differing_per_mille: 5,
                worst: 16,
            },
        );
        let text_ink = (20..140)
            .flat_map(|x| (14..32).map(move |y| (x, y)))
            .filter(|&(x, y)| cpu.get_pixel(x, y).r > 200)
            .count();
        assert!(text_ink > 20, "panel text painted {text_ink} light pixels");
    }

    #[test]
    fn icon_grid_paints_alike_on_gpu() {
        let _ = assert_full_paint_parity(
            "icon grid",
            &icon_grid_scene(),
            Budget {
                differing_per_mille: 2,
                worst: 8,
            },
        );
    }

    #[test]
    fn effect_layers_paint_alike_on_gpu() {
        let _ = assert_full_paint_parity(
            "effects",
            &effects_scene(),
            Budget {
                differing_per_mille: 40,
                worst: 16,
            },
        );
    }

    /// A whole-pixel scroll keeps the viewport's pixels, shifts them in place
    /// and repaints only the exposed strip. On the GPU target that replay must
    /// equal a fresh GPU paint, and match the raster replay within budget.
    #[test]
    fn retained_scroll_replay_matches_fresh_paint_on_gpu() {
        let _guard = crate::resource_state_test_lock();
        let Some(mut context) = context_or_skip() else {
            return;
        };
        let engine = FrontendRenderEngine::new();
        let viewport = DamageRect {
            x: 10,
            y: 10,
            width: 120,
            height: 100,
        };
        let replay = |session: &mut PixelCanvasSession<'_>| {
            paint(&engine, session, &scroll_scene(0.0), None);
            for (from, to) in [(0.0, 17.0), (17.0, 45.0), (45.0, 30.0)] {
                let exposed = session.shift_region(viewport, 0, (from - to) as i32);
                paint(&engine, session, &scroll_scene(to), Some(&exposed));
            }
        };

        let mut cpu = PixelBuffer::new(WIDTH, HEIGHT);
        replay(&mut PixelCanvasSession::new(&mut cpu));
        let mut cpu_fresh = PixelBuffer::new(WIDTH, HEIGHT);
        paint(
            &engine,
            &mut PixelCanvasSession::new(&mut cpu_fresh),
            &scroll_scene(30.0),
            None,
        );
        assert!(cpu.data() == cpu_fresh.data(), "raster replay drifted");

        let mut target = gpu_target(&mut context);
        replay(&mut target.session());
        let gpu = target.read_back(&mut context).unwrap();
        let mut fresh = gpu_target(&mut context);
        paint(&engine, &mut fresh.session(), &scroll_scene(30.0), None);
        let gpu_fresh = fresh.read_back(&mut context).unwrap();
        assert!(gpu.data() == gpu_fresh.data(), "GPU replay drifted");

        assert_within(
            "scroll",
            &gpu,
            &cpu,
            Budget {
                differing_per_mille: 5,
                worst: 16,
            },
        );
    }
}
