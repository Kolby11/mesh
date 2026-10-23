use super::*;
use mesh_core_render::{BackdropFrame, PaintContext, SurfaceCanvas};

const WIDTH: u32 = 2880;
const HEIGHT: u32 = 114;
const OUTPUT_HEIGHT: u32 = 1800;

struct Wallpaper {
    _dir: tempfile::TempDir,
}

impl Wallpaper {
    fn load() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallpaper.bmp");
        // A nonuniform output-sized source makes displacement and dispersion
        // observable without depending on the developer's wallpaper files.
        let bytes = WIDTH * OUTPUT_HEIGHT * 4;
        let mut bmp = Vec::with_capacity(bytes as usize + 54);
        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&(bytes + 54).to_le_bytes());
        bmp.extend_from_slice(&[0; 4]);
        bmp.extend_from_slice(&54u32.to_le_bytes());
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(WIDTH as i32).to_le_bytes());
        bmp.extend_from_slice(&(OUTPUT_HEIGHT as i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&32u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 24]);
        for y in 0..OUTPUT_HEIGHT {
            for x in 0..WIDTH {
                bmp.extend_from_slice(&[
                    ((x / 7 + y / 11) % 256) as u8,
                    ((x / 19 + y / 3) % 256) as u8,
                    ((x / 5 + y / 17) % 256) as u8,
                    255,
                ]);
            }
        }
        std::fs::write(&path, bmp).unwrap();
        let selected = std::env::var_os("MESH_BENCH_WALLPAPER")
            .map(PathBuf::from)
            .unwrap_or(path);
        mesh_core_render::set_backdrop_image(Some(&selected)).unwrap();
        assert!(mesh_core_render::has_backdrop_image());
        Self { _dir: dir }
    }
}

impl Drop for Wallpaper {
    fn drop(&mut self) {
        mesh_core_render::set_backdrop_image(None).unwrap();
    }
}

fn context() -> PaintContext {
    PaintContext {
        backdrop: BackdropFrame {
            origin: (0.0, (OUTPUT_HEIGHT - HEIGHT) as f32),
            output: (WIDTH as f32, OUTPUT_HEIGHT as f32),
            source_beneath: true,
        },
        ..Default::default()
    }
}

fn replay(canvas: &mut SurfaceCanvas) -> Duration {
    let SurfaceCanvas::Raster(raster) = canvas else {
        unreachable!()
    };
    raster.replay().map(|job| job.run()).unwrap_or_default()
}

fn frame(component: &mut FrontendSurfaceComponent, theme: &Theme, canvas: &mut SurfaceCanvas) {
    component
        .paint(theme, SurfaceExtent::unpadded(WIDTH, HEIGHT), canvas, 1.0)
        .unwrap();
}

fn mounted(
    catalog: &GlassCatalog,
    theme: &Theme,
    paint_context: PaintContext,
    deferred: bool,
) -> (FrontendSurfaceComponent, SurfaceCanvas) {
    let mut component = catalog.mount("glass-dock");
    component.set_paint_context(paint_context);
    let mut canvas = SurfaceCanvas::from(PixelBuffer::new(WIDTH, HEIGHT));
    canvas.set_deferred_raster(deferred);
    // Mount the service consumer before delivering the initial service state,
    // matching the live replay's post-mount injection.
    frame(&mut component, theme, &mut canvas);
    replay(&mut canvas);
    update(&mut component, "mesh.apps", glass_apps_payload(10, 150));
    (component, canvas)
}

#[test]
#[ignore = "release CPU glass timing: output backdrop, refraction and deferred replay"]
fn glass_dock_cpu_raster_timing_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    let _wallpaper = Wallpaper::load();
    let theme = theme();
    let catalog = GlassCatalog::new();
    // Snap animations for the pixel proof; the timed sweep below retains the
    // real wall-clock transitions and paints only requested frames.
    let mut reference_pixels = Vec::new();
    for (index, (paint_context, deferred)) in [
        (context(), false),
        (context(), true),
        (PaintContext::default(), true),
    ]
    .into_iter()
    .enumerate()
    {
        let (mut component, mut canvas) = mounted(&catalog, &theme, paint_context, deferred);
        Arc::make_mut(&mut component.settings)
            .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
        for _ in 0..3 {
            frame(&mut component, &theme, &mut canvas);
            replay(&mut canvas);
        }
        let pixels = canvas.as_raster().unwrap().data();
        match index {
            0 => reference_pixels = pixels.to_vec(),
            1 => assert!(reference_pixels == pixels, "deferred/direct pixel parity"),
            _ => assert!(
                reference_pixels != pixels,
                "backdrop/refraction must affect pixels"
            ),
        }
    }
    let mode = std::env::var("MESH_GLASS_TIMING_MODE").unwrap_or_else(|_| "deferred".into());
    assert!(matches!(
        mode.as_str(),
        "deferred" | "direct" | "no-backdrop"
    ));
    let (mut component, mut canvas) = mounted(
        &catalog,
        &theme,
        if mode == "no-backdrop" {
            PaintContext::default()
        } else {
            context()
        },
        mode != "direct",
    );
    let settle = |component: &mut FrontendSurfaceComponent, canvas: &mut SurfaceCanvas| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while component.wants_render() {
            assert!(std::time::Instant::now() < deadline, "dock did not settle");
            frame(component, &theme, canvas);
            replay(canvas);
            std::thread::sleep(Duration::from_millis(8));
        }
    };
    settle(&mut component, &mut canvas);
    assert_eq!(runtime_number(&component, "hovered_index"), 0.0);
    let tiles: Vec<_> = (0..10)
        .map(|i| {
            let key = reference(&component, &format!("app-{i}"));
            find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &key, 0.0, 0.0).unwrap()
        })
        .collect();
    eprintln!(
        "CPU glass mode={mode} surface={WIDTH}x{HEIGHT} output={WIDTH}x{OUTPUT_HEIGHT} origin={:?} apps=150 nodes={}",
        context().backdrop.origin,
        count_nodes(component.last_tree.as_ref().unwrap())
    );
    for run in 0..3 {
        component
            .handle_input(&theme, WIDTH, HEIGHT, ComponentInput::PointerLeave)
            .unwrap();
        settle(&mut component, &mut canvas);
        let (mut total, mut preparation, mut raster) = (Vec::new(), Vec::new(), Vec::new());
        for step in 0..60 {
            std::thread::sleep(Duration::from_millis(16));
            let started = std::time::Instant::now();
            if step % 6 == 0 {
                let (left, top, right, bottom) = tiles[(step / 6) % tiles.len()];
                component
                    .handle_input(
                        &theme,
                        WIDTH,
                        HEIGHT,
                        ComponentInput::PointerMove {
                            x: (left + right) / 2.0,
                            y: (top + bottom) / 2.0,
                        },
                    )
                    .unwrap();
                assert_eq!(
                    runtime_number(&component, "hovered_index"),
                    ((step / 6) % tiles.len() + 1) as f64
                );
            }
            if component.wants_render() {
                frame(&mut component, &theme, &mut canvas);
                preparation.push(started.elapsed());
                raster.push(replay(&mut canvas));
                total.push(started.elapsed());
            }
        }
        assert!(total.len() >= 30, "hover must produce animation frames");
        for (name, mut samples) in [
            ("total", total),
            ("prepare", preparation),
            ("replay", raster),
        ] {
            samples.sort();
            let mean =
                samples.iter().sum::<Duration>().as_secs_f64() * 1000.0 / samples.len() as f64;
            eprintln!(
                "CPU glass {mode} run={run} {name}: frames={} mean={mean:.3}ms p50={:.3}ms p90={:.3}ms",
                samples.len(),
                samples[samples.len() / 2].as_secs_f64() * 1000.0,
                samples[samples.len() * 9 / 10].as_secs_f64() * 1000.0
            );
        }
    }
}
