use super::*;
use mesh_core_elements::Prop;
use skia_safe::{Canvas, Color, Color4f, Paint, Point, RRect, Rect, TileMode, gradient};
use std::os::unix::fs::MetadataExt;

/// Hosts without libEGL, the surfaceless platform or a GLES3 driver skip the
/// GPU tests rather than failing; `MESH_REQUIRE_GPU=1` makes absence a failure
/// on machines that are expected to have one.
pub(super) fn context_or_skip() -> Option<GpuContext> {
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
    use mesh_core_elements::Prop;
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
        node.attributes.insert(Prop::Content, content.into());
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
                .insert(Prop::Name, format!("mesh-parity-missing-{index}"));
            icon.attributes.insert(Prop::Size, "22".into());
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
        area.scroll_metrics.get_or_insert_default().y = scroll_y;
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

    struct RecordedSceneTestRenderer {
        context: GpuContext,
    }
    impl crate::scene_thread::SceneRenderer for RecordedSceneTestRenderer {
        type Resources = crate::prepared_scene::PreparedScene;
        type Output = PixelBuffer;
        type Error = crate::gpu::GpuError;
        fn render(
            &mut self,
            frame: &crate::scene_thread::SceneFrame<Self::Resources>,
        ) -> Result<PixelBuffer, Self::Error> {
            let mut target = self.context.offscreen_target(WIDTH, HEIGHT)?;
            target.canvas().clear(skia_safe::Color::TRANSPARENT);
            frame.snapshot().resources().draw(target.canvas());
            target.read_back(&mut self.context)
        }
        fn remove_surface(&mut self, _: crate::scene_thread::SceneSurface) {}
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
        let list = display_list(root);
        let mut recording = crate::prepared_scene::SceneCanvas::new(WIDTH, HEIGHT).unwrap();
        recording.begin(list.frame_paint_plan());
        paint(&engine, &mut recording.session(), root, None);
        let snapshot = recording
            .snapshot(crate::FractionalScale::identity())
            .unwrap();
        let surface = snapshot.surface();
        let worker = crate::scene_thread::SceneRenderThread::spawn(
            1,
            || {
                Ok::<_, crate::gpu::GpuError>(RecordedSceneTestRenderer {
                    context: GpuContext::headless()?,
                })
            },
            || {},
        )
        .unwrap();
        worker.submit(snapshot).unwrap();
        let result = worker
            .wait_completed(surface, std::time::Duration::from_secs(10))
            .expect("recorded GPU scene completed")
            .output
            .unwrap();
        assert_within(
            &format!("{name} recorded worker"),
            &result,
            &cpu,
            Budget {
                differing_per_mille: budget.differing_per_mille,
                worst: budget.worst,
            },
        );
        assert_within(
            &format!("{name} recorded/direct GPU"),
            &result,
            &gpu,
            Budget {
                differing_per_mille: 1,
                worst: 3,
            },
        );
        worker.shutdown().unwrap();
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

/// The first render node's device number, as a compositor's dmabuf feedback
/// would report it.
fn first_render_node() -> Option<u64> {
    let mut nodes: Vec<_> = std::fs::read_dir("/dev/dri")
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
        .map(|entry| entry.path())
        .collect();
    nodes.sort();
    std::fs::metadata(nodes.first()?).ok().map(|m| m.rdev())
}

fn device_context_or_skip() -> Option<GpuContext> {
    let Some(device) = first_render_node() else {
        if std::env::var_os("MESH_REQUIRE_GPU").is_some() {
            panic!("GPU required but no render node exists");
        }
        eprintln!("skipping GPU test: no render node");
        return None;
    };
    match GpuContext::on_drm_device(device) {
        Ok(context) => Some(context),
        Err(error) if std::env::var_os("MESH_REQUIRE_GPU").is_none() => {
            eprintln!("skipping GPU test: {error}");
            None
        }
        Err(error) => panic!("GPU required but unavailable: {error}"),
    }
}

#[test]
fn device_context_exports_dmabuf_targets_holding_copied_damage() {
    let Some(mut context) = device_context_or_skip() else {
        return;
    };
    if !context.supports_dmabuf_export() {
        eprintln!("skipping: {} cannot export dmabufs", context.renderer());
        return;
    }
    let (width, height) = (64, 32);
    let mut source = context.offscreen_target(width, height).unwrap();
    source.canvas().clear(Color::from_argb(255, 200, 30, 10));
    let mut target = context.dmabuf_target(width, height).unwrap();
    let planes = target.planes();
    assert!(!planes.planes.is_empty());
    assert_eq!(planes.fourcc, DRM_FORMAT_ABGR8888);
    assert!(planes.planes[0].stride >= width * 4);

    target.canvas().clear(Color::TRANSPARENT);
    let damage = DamageRect {
        x: 8,
        y: 4,
        width: 16,
        height: 8,
    };
    target.copy_from(&mut source, &[damage]);
    context.flush();
    // Kernels before 6.0 cannot import sync files; presentation then waits.
    let fenced = context.attach_write_fence(&target);
    assert!(
        fenced || std::env::var_os("MESH_REQUIRE_GPU").is_none(),
        "{} cannot fence a dmabuf for implicit sync",
        context.renderer()
    );
    context.finish();

    let info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    assert!(
        target
            .surface
            .read_pixels(&info, &mut pixels, (width * 4) as usize, (0, 0))
    );
    let pixel = |x: u32, y: u32| {
        let offset = ((y * width + x) * 4) as usize;
        [
            pixels[offset],
            pixels[offset + 1],
            pixels[offset + 2],
            pixels[offset + 3],
        ]
    };
    assert_eq!(pixel(10, 6), [200, 30, 10, 255], "damage is copied");
    assert_eq!(pixel(2, 2), [0, 0, 0, 0], "outside damage is untouched");
}

/// UI-thread cost of one frame on each backend, from paint through the
/// buffer the compositor reads: CPU raster plus the SHM row copy, against
/// GPU paint plus the dmabuf copy and the completion wait. Run with
/// `cargo test -p mesh-core-render --release --features renderer-gpu --lib
/// gpu_frame_cost -- --ignored --nocapture`.
mod frame_cost {
    use super::*;
    use crate::display_list::{DisplayListRepaintPolicy, RetainedDisplayList};
    use crate::{FrontendRenderEngine, PixelCanvasSession};
    use mesh_core_elements::WidgetNode;
    use mesh_core_elements::layout::LayoutRect;
    use mesh_core_elements::style::{
        BackgroundPaint, BoxShadow, Color as StyleColor, Corners, Dimension, Edges,
        StyleLinearGradient, VisualFilter,
    };
    use std::time::{Duration, Instant};

    const LOGICAL: (u32, u32) = (920, 900);
    const SCALE: f32 = 2.0;
    const FRAMES: usize = 40;
    const SAMPLES: usize = 4;

    fn node(id: u64, tag: &str, (x, y, width, height): (f32, f32, f32, f32)) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.id = id;
        node.layout = LayoutRect {
            x,
            y,
            width,
            height,
        };
        node.computed_style.width = Dimension::Px(width);
        node.computed_style.height = Dimension::Px(height);
        node
    }

    fn color(r: u8, g: u8, b: u8, a: u8) -> StyleColor {
        StyleColor { r, g, b, a }
    }

    /// A Settings-sized surface: gradient backdrop, a 6×10 grid of rounded,
    /// shadowed cards with text, and a frosted panel over the top third.
    fn settings_scene() -> WidgetNode {
        let mut root = node(1, "box", (0.0, 0.0, 920.0, 900.0));
        root.computed_style.background_paint =
            BackgroundPaint::LinearGradient(StyleLinearGradient {
                from: color(30, 60, 110, 255),
                to: color(110, 40, 90, 255),
            });
        let mut id = 10;
        for row in 0..10 {
            for column in 0..6 {
                let (x, y) = (16.0 + column as f32 * 150.0, 16.0 + row as f32 * 88.0);
                let mut card = node(id, "box", (x, y, 138.0, 76.0));
                card.computed_style.background_color = color(40, 50, 70, 230);
                card.computed_style.border_radius = Corners::all(12.0);
                card.computed_style.border_width = Edges::all(1.0);
                card.computed_style.border_color = color(120, 150, 190, 255);
                card.computed_style.box_shadow = BoxShadow {
                    offset_x: 0.0,
                    offset_y: 4.0,
                    blur_radius: 10.0,
                    spread_radius: 0.0,
                    color: color(0, 0, 0, 120),
                    inset: false,
                };
                let mut label = node(id + 1, "text", (x + 12.0, y + 12.0, 110.0, 18.0));
                label
                    .attributes
                    .insert(Prop::Content, format!("Setting {row}.{column}").into());
                label.computed_style.color = color(240, 244, 248, 255);
                label.computed_style.font_size = 13.0;
                label.computed_style.line_height = 1.3;
                label.computed_style.padding = Edges::zero();
                card.children.push(label);
                root.children.push(card);
                id += 2;
            }
        }
        let mut frosted = node(id, "box", (40.0, 40.0, 840.0, 260.0));
        frosted.computed_style.background_color = color(255, 255, 255, 36);
        frosted.computed_style.border_radius = Corners::all(18.0);
        frosted.computed_style.backdrop_filter = VisualFilter::blur(12.0);
        root.children.push(frosted);
        root
    }

    /// A text-dense surface: 40 lines of mixed text on a plain background.
    fn text_scene() -> WidgetNode {
        let mut root = node(1, "box", (0.0, 0.0, 920.0, 900.0));
        root.computed_style.background_color = color(24, 28, 36, 255);
        for line in 0..40u64 {
            let mut label = node(
                10 + line,
                "text",
                (16.0, 12.0 + line as f32 * 22.0, 880.0, 20.0),
            );
            label.attributes.insert(
                Prop::Content,
                format!(
                    "{line:02} The quick brown fox jumps over the lazy dog — 0123456789 ABCDEFGHIJKLM"
                )
                .into(),
            );
            label.computed_style.color = color(230, 234, 240, 255);
            label.computed_style.font_size = 14.0;
            label.computed_style.line_height = 1.3;
            label.computed_style.padding = Edges::zero();
            root.children.push(label);
        }
        root
    }

    fn logical(x: u32, y: u32, width: u32, height: u32) -> DamageRect {
        DamageRect {
            x,
            y,
            width,
            height,
        }
    }

    fn device(rect: DamageRect) -> DamageRect {
        let scale = |value: u32| (value as f32 * SCALE) as u32;
        logical(
            scale(rect.x),
            scale(rect.y),
            scale(rect.width),
            scale(rect.height),
        )
    }

    /// Clear and repaint `damage` (logical) through the session.
    fn paint(
        engine: &FrontendRenderEngine,
        list: &RetainedDisplayList,
        session: &mut PixelCanvasSession<'_>,
        damage: DamageRect,
    ) {
        let selected = list
            .select_paint_commands_for_rects(&[damage], DisplayListRepaintPolicy::MinimalDamage);
        let clip = device(damage);
        session.clear_rect(clip, StyleColor::TRANSPARENT);
        engine.render_selected_display_list_regions_for_module_in_session(
            &selected,
            session,
            SCALE,
            &[(clip.x, clip.y, clip.width, clip.height)],
            None,
            None,
        );
    }

    /// The SHM upload: copy damaged rows of `source` into a mapped buffer.
    fn copy_rows(source: &PixelBuffer, target: &mut [u8], rect: DamageRect) {
        let stride = source.stride() as usize;
        let row = rect.width as usize * 4;
        for y in rect.y..rect.y + rect.height {
            let start = y as usize * stride + rect.x as usize * 4;
            target[start..start + row].copy_from_slice(&source.data()[start..start + row]);
        }
    }

    fn summary(samples: &[Duration]) -> String {
        summary_for(samples, FRAMES)
    }

    fn summary_for(samples: &[Duration], frames: usize) -> String {
        let per_frame: Vec<f64> = samples
            .iter()
            .map(|sample| sample.as_secs_f64() * 1000.0 / frames as f64)
            .collect();
        let min = per_frame.iter().copied().fold(f64::MAX, f64::min);
        let max = per_frame.iter().copied().fold(0.0, f64::max);
        format!("{min:.3}–{max:.3} ms/frame")
    }

    fn render_nodes() -> Vec<u64> {
        let mut nodes: Vec<_> = std::fs::read_dir("/dev/dri")
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
            .map(|entry| entry.path())
            .collect();
        nodes.sort();
        nodes
            .iter()
            .filter_map(|path| std::fs::metadata(path).ok().map(|m| m.rdev()))
            .collect()
    }

    struct FinishedSceneRenderer(crate::gpu::scene::GpuSceneRenderer);
    impl crate::scene_thread::SceneRenderer for FinishedSceneRenderer {
        type Resources = crate::prepared_scene::PreparedScene;
        type Output = crate::gpu::scene::GpuSceneOutput;
        type Error = crate::gpu::scene::GpuSceneError;
        fn render(
            &mut self,
            frame: &crate::scene_thread::SceneFrame<Self::Resources>,
        ) -> Result<Self::Output, Self::Error> {
            let output = self.0.render(frame)?;
            self.0.finish();
            Ok(output)
        }
        fn remove_surface(&mut self, surface: crate::scene_thread::SceneSurface) {
            self.0.remove_surface(surface);
        }
    }

    #[test]
    #[ignore = "release scene handoff measurement"]
    fn gpu_scene_handoff_profile() {
        let _guard = crate::resource_state_test_lock();
        let mut list = RetainedDisplayList::default();
        list.set_backdrop_blur_policy(crate::BackdropBlurPolicy::InSurfaceFilter);
        list.update(&settings_scene(), LOGICAL.0, LOGICAL.1, true, true);
        let size = (
            (LOGICAL.0 as f32 * SCALE) as u32,
            (LOGICAL.1 as f32 * SCALE) as u32,
        );
        let full = logical(0, 0, LOGICAL.0, LOGICAL.1);
        let engine = FrontendRenderEngine::new();
        for drm_device in render_nodes() {
            let Ok(mut context) = GpuContext::on_drm_device(drm_device) else {
                continue;
            };
            if context.dmabuf_target(1, 1).is_err() {
                continue;
            }
            let renderer = context.renderer().to_owned();
            let worker = crate::scene_thread::SceneRenderThread::spawn(
                1,
                move || {
                    Ok::<_, crate::gpu::scene::GpuSceneError>(FinishedSceneRenderer(
                        crate::gpu::scene::GpuSceneRenderer::new(drm_device)?,
                    ))
                },
                || {},
            )
            .unwrap();
            let mut retained = context.offscreen_target(size.0, size.1).unwrap();
            let mut ring = context.dmabuf_target(size.0, size.1).unwrap();
            let mut recording = crate::prepared_scene::SceneCanvas::new(size.0, size.1).unwrap();
            for (name, damage) in [
                ("full", full),
                ("frosted", logical(40, 40, 840, 260)),
                ("widget", logical(476, 544, 138, 76)),
            ] {
                let mut direct_samples = Vec::new();
                let mut threaded_samples = Vec::new();
                let mut prepare_samples = Vec::new();
                for sample in 0..4 {
                    let started = Instant::now();
                    for _ in 0..20 {
                        paint(&engine, &list, &mut retained.session(), damage);
                        ring.copy_from(&mut retained, &[device(damage)]);
                        context.submit_for(&ring);
                        context.finish();
                    }
                    let direct = started.elapsed();
                    let started = Instant::now();
                    let mut preparation = Duration::ZERO;
                    for _ in 0..20 {
                        let prepare = Instant::now();
                        let mut plan = list.frame_paint_plan().clone();
                        plan.damage.logical = vec![damage].into();
                        recording.begin(&plan);
                        paint(&engine, &list, &mut recording.session(), full);
                        let snapshot = recording
                            .snapshot(crate::FractionalScale::new(SCALE))
                            .unwrap();
                        preparation += prepare.elapsed();
                        let commit = worker.submit(snapshot).unwrap();
                        let output = worker
                            .wait_completed(recording.surface, Duration::from_secs(10))
                            .unwrap()
                            .output
                            .unwrap();
                        assert!(worker.acknowledge_presented(recording.surface, commit));
                        drop(output);
                    }
                    if sample > 0 {
                        direct_samples.push(direct / 20);
                        threaded_samples.push(started.elapsed() / 20);
                        prepare_samples.push(preparation / 20);
                    }
                }
                eprintln!(
                    "scene handoff {renderer}, {name}, 920x900 logical scale2, 20 frames/sample: direct {} threaded {} UI preparation {}",
                    summary_for(&direct_samples, 1),
                    summary_for(&threaded_samples, 1),
                    summary_for(&prepare_samples, 1)
                );
            }
            worker.shutdown().unwrap();
        }
    }

    struct WorkloadRenderer {
        inner: crate::gpu::scene::GpuSceneRenderer,
        readback: bool,
    }

    impl crate::scene_thread::SceneRenderer for WorkloadRenderer {
        type Resources = crate::prepared_scene::PreparedScene;
        type Output = (
            crate::gpu::scene::GpuSceneOutput,
            Option<PixelBuffer>,
            usize,
            u64,
        );
        type Error = crate::gpu::scene::GpuSceneError;
        fn render(
            &mut self,
            frame: &crate::scene_thread::SceneFrame<Self::Resources>,
        ) -> Result<Self::Output, Self::Error> {
            let output = self.inner.render(frame)?;
            self.inner.finish();
            let pixels = if self.readback {
                Some(self.inner.read_back(frame.snapshot().surface(), &output)?)
            } else {
                None
            };
            Ok((
                output,
                pixels,
                self.inner.residency_admissions(),
                crate::gpu::upload_probe::count(),
            ))
        }
        fn remove_surface(&mut self, surface: crate::scene_thread::SceneSurface) {
            self.inner.remove_surface(surface);
        }
    }

    fn workload_worker(
        device: u64,
        readback: bool,
        baseline: bool,
    ) -> crate::scene_thread::SceneRenderThread<WorkloadRenderer> {
        crate::scene_thread::SceneRenderThread::spawn(
            1,
            move || {
                let mut inner = crate::gpu::scene::GpuSceneRenderer::new(device)?;
                if baseline {
                    inner.disable_residency();
                }
                Ok::<_, crate::gpu::scene::GpuSceneError>(WorkloadRenderer { inner, readback })
            },
            || {},
        )
        .unwrap()
    }

    fn asset_scene(directory: &std::path::Path) -> WidgetNode {
        let mut root = text_scene();
        root.children.truncate(8);
        for index in 0..128 {
            let path = directory.join(format!("asset-{index}.png"));
            image::RgbaImage::from_fn(32, 32, |x, y| {
                image::Rgba([
                    (x * 8) as u8,
                    (y * 8) as u8,
                    (index * 15) as u8,
                    if (x + y) % 3 == 0 { 128 } else { 255 },
                ])
            })
            .save(&path)
            .unwrap();
            for row in 0..2 {
                let mut image = node(
                    100 + index * 2 + row,
                    if row % 2 == 0 { "image" } else { "icon" },
                    (
                        16. + (index % 16) as f32 * 55. + row as f32 * 26.,
                        220. + (index / 16) as f32 * 80.,
                        24.,
                        24.,
                    ),
                );
                image
                    .attributes
                    .insert(Prop::Src, path.to_string_lossy().into_owned());
                image.attributes.insert(Prop::Size, "24".into());
                image.computed_style.color = color(220, 240, 255, 255);
                assert!(image.layout.x + image.layout.width <= LOGICAL.0 as f32);
                assert!(image.layout.y + image.layout.height <= LOGICAL.1 as f32);
                root.children.push(image);
            }
        }
        root
    }

    fn workload_list(root: &WidgetNode) -> RetainedDisplayList {
        let mut list = RetainedDisplayList::default();
        list.set_backdrop_blur_policy(crate::BackdropBlurPolicy::InSurfaceFilter);
        list.update(root, LOGICAL.0, LOGICAL.1, true, true);
        list
    }

    fn record_workload(
        engine: &FrontendRenderEngine,
        list: &RetainedDisplayList,
        recording: &mut crate::prepared_scene::SceneCanvas,
        damage: DamageRect,
    ) -> crate::scene_thread::SceneSnapshot<crate::prepared_scene::PreparedScene> {
        let mut plan = list.frame_paint_plan().clone();
        plan.damage.logical = vec![damage].into();
        recording.begin(&plan);
        paint(
            engine,
            list,
            &mut recording.session(),
            logical(0, 0, LOGICAL.0, LOGICAL.1),
        );
        recording
            .snapshot(crate::FractionalScale::new(SCALE))
            .unwrap()
    }

    fn present_workload(
        worker: &crate::scene_thread::SceneRenderThread<WorkloadRenderer>,
        snapshot: crate::scene_thread::SceneSnapshot<crate::prepared_scene::PreparedScene>,
    ) -> (Option<PixelBuffer>, usize, u64) {
        let surface = snapshot.surface();
        let commit = worker.submit(snapshot).unwrap();
        let (output, pixels, admissions, uploads) = worker
            .wait_completed(surface, Duration::from_secs(10))
            .expect("GPU workload timed out")
            .output
            .unwrap();
        assert!(worker.acknowledge_presented(surface, commit));
        drop(output);
        (pixels, admissions, uploads)
    }

    /// Uses the production scene renderer and export path; required hardware
    /// gates fail when no device can export, rather than passing an empty loop.
    fn workload_devices() -> Vec<u64> {
        let devices: Vec<_> = render_nodes()
            .into_iter()
            .filter(|device| {
                GpuContext::on_drm_device(*device)
                    .is_ok_and(|mut context| context.dmabuf_target(1, 1).is_ok())
            })
            .collect();
        assert!(
            !devices.is_empty() || std::env::var_os("MESH_REQUIRE_GPU").is_none(),
            "GPU workload gate requires a dmabuf-capable device"
        );
        devices
    }

    #[test]
    #[ignore = "required-GPU release visual gate"]
    fn gpu_workload_visual_match_gate() {
        let _guard = crate::resource_state_test_lock();
        let engine = FrontendRenderEngine::new();
        let assets = tempfile::tempdir().unwrap();
        for (name, root) in [
            ("settings", settings_scene()),
            ("text", text_scene()),
            ("assets", asset_scene(assets.path())),
        ] {
            let list = workload_list(&root);
            let full = logical(0, 0, LOGICAL.0, LOGICAL.1);
            let mut cpu = PixelBuffer::new(1840, 1800);
            paint(&engine, &list, &mut PixelCanvasSession::new(&mut cpu), full);
            assert!(
                engine.painter_diagnostics().is_empty(),
                "{name}: {:?}",
                engine.painter_diagnostics()
            );
            for device in workload_devices() {
                let mut context = GpuContext::on_drm_device(device).unwrap();
                let mut target = context.offscreen_target(1840, 1800).unwrap();
                paint(&engine, &list, &mut target.session(), full);
                let direct = target.read_back(&mut context).unwrap();
                let worker = workload_worker(device, true, false);
                let mut recording = crate::prepared_scene::SceneCanvas::new(1840, 1800).unwrap();
                for damage in [full, logical(40, 40, 840, 260), logical(476, 544, 138, 76)] {
                    let snapshot = record_workload(&engine, &list, &mut recording, damage);
                    assert!(
                        !snapshot.resources().images.is_empty(),
                        "{name} must contain real raster assets"
                    );
                    if name == "assets" {
                        assert!(snapshot.resources().images.len() >= 1);
                    }
                    let (pixels, _, _) = present_workload(&worker, snapshot);
                    let pixels = pixels.unwrap();
                    let replay_deltas = pixel_deltas(&pixels, &direct);
                    let replay_differing = replay_deltas.iter().filter(|delta| **delta > 2).count();
                    let replay_worst = replay_deltas.iter().copied().max().unwrap();
                    assert!(
                        replay_differing * 1000 <= replay_deltas.len() && replay_worst <= 3,
                        "{name} recorded/direct GPU: {replay_differing}/{} pixels differ, worst {replay_worst}",
                        replay_deltas.len()
                    );
                    let deltas = pixel_deltas(&pixels, &cpu);
                    let differing = deltas.iter().filter(|delta| **delta > 2).count();
                    let worst = deltas.iter().copied().max().unwrap();
                    eprintln!(
                        "GPU visual {name}, {damage:?}: {differing}/{} pixels >2, worst {worst}",
                        deltas.len()
                    );
                    let (per_mille, max_delta) = if name == "settings" {
                        (100, 40)
                    } else {
                        (10, 4)
                    };
                    assert!(
                        differing * 1000 <= deltas.len() * per_mille && worst <= max_delta,
                        "GPU visual {name}: {differing}/{} pixels differ, worst {worst}",
                        deltas.len()
                    );
                }
                worker.shutdown().unwrap();
            }
        }
    }

    #[test]
    #[ignore = "required-GPU release frame-cost gate"]
    fn gpu_workload_frame_cost_gate() {
        run_gpu_workload_cost(false);
    }

    #[test]
    #[ignore = "release comparison with automatic Ganesh residency"]
    fn gpu_workload_automatic_residency_profile() {
        run_gpu_workload_cost(true);
    }

    fn run_gpu_workload_cost(baseline: bool) {
        let _paging = crate::surface::image_pages::TestPaging::new(!baseline);
        let _guard = crate::resource_state_test_lock();
        let engine = FrontendRenderEngine::new();
        let assets = tempfile::tempdir().unwrap();
        let settings = workload_list(&settings_scene());
        let text = workload_list(&text_scene());
        let images = workload_list(&asset_scene(assets.path()));
        let full = logical(0, 0, LOGICAL.0, LOGICAL.1);
        for device in workload_devices() {
            let renderer = GpuContext::on_drm_device(device)
                .unwrap()
                .renderer()
                .to_owned();
            let worker = workload_worker(device, false, baseline);
            let mut recording = crate::prepared_scene::SceneCanvas::new(1840, 1800).unwrap();
            for (name, list, damage, budget_ms) in [
                ("settings_full", &settings, full, 16.67),
                ("frosted_panel", &settings, logical(40, 40, 840, 260), 12.0),
                ("small_widget", &settings, logical(476, 544, 138, 76), 12.0),
                ("text_full", &text, full, 16.67),
                ("text_line", &text, logical(16, 452, 880, 20), 12.0),
                ("assets_full", &images, full, 16.67),
            ] {
                for _ in 0..4 {
                    present_workload(
                        &worker,
                        record_workload(&engine, list, &mut recording, full),
                    );
                }
                let mut samples = Vec::new();
                let mut uploads = None;
                let mut transfers = None;
                let mut frames = Vec::new();
                for _ in 0..3 {
                    let started = Instant::now();
                    for _ in 0..20 {
                        let frame_start = Instant::now();
                        let (_, count, gpu_uploads) = present_workload(
                            &worker,
                            record_workload(&engine, list, &mut recording, damage),
                        );
                        assert!(
                            gpu_uploads > 0,
                            "GPU upload probe must observe cold bitmap transfers"
                        );
                        if let Some(previous) = uploads {
                            assert_eq!(count, previous, "warm {name} changed a resource identity");
                        }
                        if let Some(previous) = transfers {
                            assert_eq!(
                                gpu_uploads, previous,
                                "warm {name} transferred bitmap data to GLES"
                            );
                        }
                        uploads = Some(count);
                        transfers = Some(gpu_uploads);
                        frames.push(frame_start.elapsed());
                    }
                    samples.push(started.elapsed());
                }
                frames.sort();
                let p90_ms =
                    frames[(frames.len() * 9 / 10).min(frames.len() - 1)].as_secs_f64() * 1000.;
                let worst_ms = samples
                    .iter()
                    .map(|sample| sample.as_secs_f64() * 1000. / 20.)
                    .fold(0., f64::max);
                eprintln!(
                    "GPU gate {renderer}, {name}: {} (budget {budget_ms} ms)",
                    summary_for(&samples, 20)
                );
                eprintln!("GPU gate {name}: p90 {p90_ms:.3} ms, zero warm GLES bitmap transfers");
                if !baseline {
                    assert!(
                        p90_ms <= budget_ms,
                        "GPU {name} p90 {p90_ms:.3} ms exceeds {budget_ms} ms"
                    );
                    assert!(
                        worst_ms <= budget_ms,
                        "GPU {name}: {worst_ms:.3} ms exceeds {budget_ms} ms"
                    );
                }
            }
            worker.shutdown().unwrap();
        }
    }

    // cargo test -p mesh-core-render --features renderer-gpu --release -- parallel_cpu_raster_profile --ignored --nocapture
    #[test]
    #[ignore = "benchmark"]
    fn parallel_cpu_raster_profile() {
        use crate::SurfaceCanvas;

        let _guard = crate::resource_state_test_lock();
        let engine = FrontendRenderEngine::new();
        let list_for = |root: &WidgetNode| {
            let mut list = RetainedDisplayList::default();
            list.set_backdrop_blur_policy(crate::display_list::BackdropBlurPolicy::InSurfaceFilter);
            list.update(root, LOGICAL.0, LOGICAL.1, true, true);
            list
        };
        let settings = list_for(&settings_scene());
        let text = list_for(&text_scene());
        let size = (
            (LOGICAL.0 as f32 * SCALE) as u32,
            (LOGICAL.1 as f32 * SCALE) as u32,
        );
        let full = logical(0, 0, LOGICAL.0, LOGICAL.1);
        let frames: [(&str, [(&RetainedDisplayList, DamageRect); 3]); 2] = [
            (
                "full repaint",
                [(&settings, full), (&text, full), (&settings, full)],
            ),
            (
                "partial damage",
                [
                    (&settings, logical(40, 40, 840, 260)),
                    (&text, logical(16, 452, 880, 20)),
                    (&settings, logical(476, 544, 138, 76)),
                ],
            ),
        ];
        let mut canvases: Vec<SurfaceCanvas> = (0..3)
            .map(|_| PixelBuffer::new(size.0, size.1).into())
            .collect();
        let replay_all = |canvases: &mut [SurfaceCanvas], parallel: bool| {
            let replays = canvases.iter_mut().filter_map(|canvas| match canvas {
                SurfaceCanvas::Raster(raster) => raster.replay(),
                SurfaceCanvas::Scene(_) => None,
            });
            if parallel {
                use rayon::prelude::*;
                // The shell's pool: persistent workers, as in `render_components`.
                replays
                    .collect::<Vec<_>>()
                    .into_par_iter()
                    .for_each(|replay| {
                        replay.run();
                    });
            } else {
                replays.for_each(|replay| {
                    replay.run();
                });
            }
        };
        eprintln!(
            "3 surfaces {}x{} logical at scale {SCALE}, {FRAMES} frames x {SAMPLES} samples",
            LOGICAL.0, LOGICAL.1
        );
        for (name, surfaces) in frames {
            let mut direct = Vec::new();
            let mut parallel = Vec::new();
            let mut recording = Vec::new();
            let mut single_direct = Vec::new();
            let mut single_deferred = Vec::new();
            for _ in 0..=SAMPLES {
                let started = Instant::now();
                for _ in 0..FRAMES {
                    for (canvas, (list, damage)) in canvases.iter_mut().zip(surfaces) {
                        canvas.set_deferred_raster(false);
                        paint(&engine, list, &mut canvas.session(), damage);
                    }
                }
                direct.push(started.elapsed());

                let started = Instant::now();
                let mut recorded = Duration::ZERO;
                for _ in 0..FRAMES {
                    let record = Instant::now();
                    for (canvas, (list, damage)) in canvases.iter_mut().zip(surfaces) {
                        canvas.set_deferred_raster(true);
                        paint(&engine, list, &mut canvas.session(), damage);
                    }
                    recorded += record.elapsed();
                    replay_all(&mut canvases, true);
                }
                parallel.push(started.elapsed());
                recording.push(recorded);

                let (list, damage) = surfaces[0];
                let started = Instant::now();
                for _ in 0..FRAMES {
                    canvases[0].set_deferred_raster(false);
                    paint(&engine, list, &mut canvases[0].session(), damage);
                }
                single_direct.push(started.elapsed());
                let started = Instant::now();
                for _ in 0..FRAMES {
                    canvases[0].set_deferred_raster(true);
                    paint(&engine, list, &mut canvases[0].session(), damage);
                    replay_all(&mut canvases[..1], false);
                }
                single_deferred.push(started.elapsed());
            }
            for samples in [
                &mut direct,
                &mut parallel,
                &mut recording,
                &mut single_direct,
                &mut single_deferred,
            ] {
                samples.remove(0);
            }
            eprintln!(
                "{name}: serial raster {} | record {} + parallel replay = {} | one surface direct {} vs record+replay {}",
                summary(&direct),
                summary(&recording),
                summary(&parallel),
                summary(&single_direct),
                summary(&single_deferred),
            );
        }
    }

    #[test]
    #[ignore = "benchmark"]
    fn gpu_frame_cost_workload() {
        let _guard = crate::resource_state_test_lock();
        let engine = FrontendRenderEngine::new();
        let list_for = |root: &WidgetNode| {
            let mut list = RetainedDisplayList::default();
            list.set_backdrop_blur_policy(crate::display_list::BackdropBlurPolicy::InSurfaceFilter);
            list.update(root, LOGICAL.0, LOGICAL.1, true, true);
            list
        };
        let settings = list_for(&settings_scene());
        let text = list_for(&text_scene());
        let (width, height) = (
            (LOGICAL.0 as f32 * SCALE) as u32,
            (LOGICAL.1 as f32 * SCALE) as u32,
        );
        let workloads = [
            (
                "full surface",
                &settings,
                logical(0, 0, LOGICAL.0, LOGICAL.1),
            ),
            ("frosted panel", &settings, logical(40, 40, 840, 260)),
            ("small widget", &settings, logical(476, 544, 138, 76)),
            ("40 text lines", &text, logical(0, 0, LOGICAL.0, LOGICAL.1)),
            ("one text line", &text, logical(16, 452, 880, 20)),
        ];
        eprintln!(
            "surface {}x{} logical at scale {SCALE} ({width}x{height} px), {FRAMES} frames x {SAMPLES} samples",
            LOGICAL.0, LOGICAL.1
        );
        let mut cpu = PixelBuffer::new(width, height);
        let mut shm = vec![0u8; cpu.data().len()];
        for (name, list, damage) in workloads {
            let mut samples = Vec::new();
            for _ in 0..=SAMPLES {
                let started = Instant::now();
                for _ in 0..FRAMES {
                    paint(
                        &engine,
                        &list,
                        &mut PixelCanvasSession::new(&mut cpu),
                        damage,
                    );
                    copy_rows(&cpu, &mut shm, device(damage));
                }
                samples.push(started.elapsed());
            }
            samples.remove(0);
            eprintln!("cpu raster + shm copy, {name}: {}", summary(&samples));
        }
        for node in render_nodes() {
            let Ok(mut context) = GpuContext::on_drm_device(node) else {
                continue;
            };
            if !context.supports_dmabuf_export() {
                continue;
            }
            let renderer = context.renderer().to_owned();
            let mut retained = context.offscreen_target(width, height).unwrap();
            let mut ring = [
                context.dmabuf_target(width, height).unwrap(),
                context.dmabuf_target(width, height).unwrap(),
            ];
            for (name, list, damage) in workloads {
                let mut samples = Vec::new();
                for _ in 0..=SAMPLES {
                    let started = Instant::now();
                    for frame in 0..FRAMES {
                        paint(&engine, list, &mut retained.session(), damage);
                        ring[frame % 2].copy_from(&mut retained, &[device(damage)]);
                        context.submit_for(&ring[frame % 2]);
                    }
                    context.finish();
                    samples.push(started.elapsed());
                }
                samples.remove(0);
                eprintln!(
                    "gpu ({renderer}) + dmabuf copy, {name}: {}",
                    summary(&samples)
                );
            }
        }
    }
}
