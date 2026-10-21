//! Focused conformance matrix against a real Wayland compositor.
//!
//! The deterministic `Testing` backend (see `PresentationEngine`'s `testing_*`
//! methods) proves protocol-state *logic* without a compositor: configure
//! failure injection, close/dismiss, frame pacing, buffer backpressure,
//! output/scale changes, and connection loss. It cannot prove that the real
//! `WaylandSurfaceBackend` drives an actual compositor through the same
//! states. These tests do that, against whatever compositor the process finds
//! on `WAYLAND_DISPLAY`.
//!
//! There is no compositor in most CI/sandbox environments, so every test
//! skips itself (passes trivially, with a printed reason) when
//! `WAYLAND_DISPLAY` is unset rather than failing or hanging. Run for real
//! with `WAYLAND_DISPLAY=wayland-0 cargo test -p mesh-core-presentation
//! --test live_compositor_matrix`.

use mesh_core_presentation::{
    LayerSurfaceSizePolicy, PopupConfig, PopupPlacement, PresentStatus, PresentationEngine,
    SurfaceConfig,
};
use mesh_core_render::{DamageRect, PixelBuffer, SurfaceCanvas};
use std::time::{Duration, Instant};

fn live_engine() -> Option<PresentationEngine> {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skipping live compositor matrix: WAYLAND_DISPLAY is unset in this environment");
        return None;
    }
    // Force the Wayland backend rather than relying on layer-shell being
    // reachable/preferred; a failed connection is a real conformance failure
    // here, not a fallback case.
    // SAFETY: single-threaded test setup before any other thread reads env.
    unsafe {
        std::env::set_var("MESH_BACKEND", "layer-shell");
    }
    Some(PresentationEngine::select())
}

/// Poll the connection until the compositor has answered the first configure,
/// or fail with a clear timeout rather than hanging forever on a wedged
/// compositor.
fn wait_ready(engine: &mut PresentationEngine, surface_id: &str, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        engine
            .pump()
            .expect("pump must not error against a live compositor");
        if engine.surface_ready_to_present(surface_id) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn top_layer_config(namespace: &str) -> SurfaceConfig {
    SurfaceConfig {
        namespace: namespace.to_string(),
        size_policy: LayerSurfaceSizePolicy::Fixed,
        ..SurfaceConfig::default()
    }
}

#[test]
fn layer_surface_configures_and_presents_on_a_live_compositor() {
    let Some(mut engine) = live_engine() else {
        return;
    };
    let surface_id = "mesh-live-matrix-configure-present";

    engine
        .configure(surface_id, top_layer_config("@mesh/live-matrix-a"))
        .expect("a fresh layer surface must be creatable on a live compositor");

    assert!(
        wait_ready(&mut engine, surface_id, Duration::from_secs(5)),
        "compositor never answered the first configure"
    );

    let (width, height) = engine
        .surface_size(surface_id)
        .expect("surface_size must succeed once configured")
        .unwrap_or((1, 1));
    let mut buffer = SurfaceCanvas::from(
        PixelBuffer::try_new(width.max(1), height.max(1)).expect("buffer allocation must succeed"),
    );
    let full_damage = DamageRect {
        x: 0,
        y: 0,
        width: buffer.width().max(1),
        height: buffer.height().max(1),
    };

    let status = engine
        .present_with_damage(surface_id, "live-matrix", true, &mut buffer, &[full_damage])
        .expect("present must not error once the surface is ready");
    assert_eq!(
        status,
        PresentStatus::Presented,
        "a ready, configured surface must accept its first frame"
    );
    engine.finish_frame().expect("finish_frame must not error");

    engine.destroy_surface(surface_id);
}

#[test]
fn destroyed_surface_reports_missing_instead_of_reconjuring_state() {
    let Some(mut engine) = live_engine() else {
        return;
    };
    let surface_id = "mesh-live-matrix-destroy-then-present";

    engine
        .configure(surface_id, top_layer_config("@mesh/live-matrix-b"))
        .expect("a fresh layer surface must be creatable on a live compositor");
    assert!(
        wait_ready(&mut engine, surface_id, Duration::from_secs(5)),
        "compositor never answered the first configure"
    );

    engine.destroy_surface(surface_id);

    let mut buffer =
        SurfaceCanvas::from(PixelBuffer::try_new(1, 1).expect("buffer allocation must succeed"));
    let damage = DamageRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    let status = engine
        .present_with_damage(surface_id, "live-matrix", true, &mut buffer, &[damage])
        .expect("presenting a destroyed surface id must not error, only report missing");
    assert_eq!(
        status,
        PresentStatus::SurfaceMissing,
        "a destroyed compositor object must not silently reappear as presentable"
    );
}

#[test]
fn popup_configure_rejects_an_unknown_parent_on_a_live_compositor() {
    let Some(mut engine) = live_engine() else {
        return;
    };

    let err = engine
        .configure_popup(
            "mesh-live-matrix-orphan-popup",
            PopupConfig {
                input_passthrough: false,
                parent_surface_id: "mesh-live-matrix-no-such-parent".to_string(),
                placement: PopupPlacement::default(),
                padding: Default::default(),
                grab: false,
                grab_identity: None,
            },
        )
        .expect_err("a popup cannot be promoted with no live parent surface");

    assert!(
        matches!(
            err,
            mesh_core_presentation::PresentationError::SurfaceCreate(_)
        ),
        "expected a surface-create rejection for a missing popup parent, got {err:?}"
    );
}

#[cfg(feature = "gpu")]
#[test]
fn scene_worker_exports_and_presents_frames_on_a_live_compositor() {
    let Some(mut engine) = live_engine() else {
        return;
    };
    let surface_id = "mesh-live-matrix-scene-worker";
    engine
        .configure(surface_id, top_layer_config("@mesh/live-matrix-scene"))
        .unwrap();
    assert!(wait_ready(&mut engine, surface_id, Duration::from_secs(5)));
    if !engine.gpu_canvas_available() {
        assert!(
            std::env::var_os("MESH_REQUIRE_GPU").is_none(),
            "live GPU scene presentation required"
        );
        eprintln!(
            "skipping live GPU scene test: compositor/device cannot export importable dmabufs"
        );
        engine.destroy_surface(surface_id);
        return;
    }
    let (width, height) = engine.surface_size(surface_id).unwrap().unwrap();
    let scale = mesh_core_render::FractionalScale::new(engine.surface_scale(surface_id));
    let mut canvas = engine
        .allocate_canvas(
            scale.physical_extent(width),
            scale.physical_extent(height),
            true,
        )
        .unwrap();
    assert!(canvas.is_scene());
    let damage = DamageRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    for _ in 0..6 {
        assert!(wait_ready(&mut engine, surface_id, Duration::from_secs(5)));
        let SurfaceCanvas::Scene(recording) = &mut canvas else {
            panic!("GPU canvas must contain a scene")
        };
        let list = mesh_core_render::RetainedDisplayList::default();
        let mut plan = list.frame_paint_plan().clone();
        plan.inputs.surface_size = Some((width, height));
        plan.damage.logical = vec![damage].into();
        recording.begin(&plan);
        recording.session().clear_rect(
            DamageRect {
                x: 0,
                y: 0,
                width: scale.physical_extent(width),
                height: scale.physical_extent(height),
            },
            PixelBuffer::new(1, 1).get_pixel(0, 0),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = engine
                .present_with_damage(surface_id, "scene-worker", true, &mut canvas, &[damage])
                .unwrap();
            if status == PresentStatus::Submitted {
                assert!(
                    engine.surface_waiting_for_buffer_release(surface_id),
                    "a submitted scene gates its surface until the worker publishes"
                );
                // A frame the compositor still holds all buffers for completes
                // unpresented; the caller repaints once a buffer is released.
                if wait_scene(&mut engine, surface_id, deadline) {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "scene presentation did not recover from backpressure"
                );
                engine.pump().unwrap();
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            assert_eq!(status, PresentStatus::NotReady);
            assert!(
                Instant::now() < deadline,
                "scene presentation did not recover from backpressure"
            );
            engine.pump().unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        engine.finish_frame().unwrap();
    }
    engine.destroy_surface(surface_id);
    drop(canvas);
    drop(engine);
}

/// Pump the connection until the submitted scene frame of `surface_id` is
/// committed. False on timeout or when the frame did not reach the compositor.
#[cfg(feature = "gpu")]
fn wait_scene(engine: &mut PresentationEngine, surface_id: &str, deadline: Instant) -> bool {
    loop {
        engine.pump().unwrap();
        for done in engine.take_scene_completions() {
            if done.surface_id == surface_id {
                return done.presented;
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_micros(200));
    }
}

#[cfg(feature = "gpu")]
fn heavy_scene(engine: &mut PresentationEngine, surface_id: &str) -> Option<SurfaceCanvas> {
    assert!(wait_ready(engine, surface_id, Duration::from_secs(5)));
    if !engine.gpu_canvas_available() {
        assert!(
            std::env::var_os("MESH_REQUIRE_GPU").is_none(),
            "live GPU scene presentation required"
        );
        return None;
    }
    let (width, height) = engine.surface_size(surface_id).unwrap().unwrap();
    let scale = mesh_core_render::FractionalScale::new(engine.surface_scale(surface_id));
    engine.allocate_canvas(
        scale.physical_extent(width),
        scale.physical_extent(height),
        true,
    )
}

#[cfg(feature = "gpu")]
fn record_heavy(canvas: &mut SurfaceCanvas, width: u32, height: u32, frame: usize) {
    let SurfaceCanvas::Scene(recording) = canvas else {
        panic!("GPU canvas must contain a scene")
    };
    let list = mesh_core_render::RetainedDisplayList::default();
    let mut plan = list.frame_paint_plan().clone();
    plan.inputs.surface_size = Some((width, height));
    plan.damage.logical = vec![DamageRect {
        x: 0,
        y: 0,
        width,
        height,
    }]
    .into();
    recording.begin(&plan);
    let (w, h) = (recording.width() as f32, recording.height() as f32);
    recording.session().with_canvas(|canvas| {
        canvas.clear(skia_safe::Color::from_argb(255, 20, 24, 32));
        for index in 0..160 {
            let t = (index * 37 + frame * 5) as f32;
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(skia_safe::Color::from_argb(
                200,
                (index * 7 % 255) as u8,
                (index * 13 % 255) as u8,
                200,
            ));
            paint.set_mask_filter(skia_safe::MaskFilter::blur(
                skia_safe::BlurStyle::Normal,
                14.0,
                false,
            ));
            let x = (t * 3.1) % (w * 0.8);
            let y = (t * 1.7) % (h * 0.8);
            canvas.draw_round_rect(
                skia_safe::Rect::from_xywh(x, y, w * 0.2, h * 0.15),
                18.0,
                18.0,
                &paint,
            );
        }
    });
}

/// Hiding, resizing and destroying a surface while its frame is on the worker
/// must neither commit stale pixels nor wedge the surface.
#[cfg(feature = "gpu")]
#[test]
fn scene_frame_in_flight_survives_hide_resize_and_destroy() {
    let Some(mut engine) = live_engine() else {
        return;
    };
    let surface_id = "mesh-live-matrix-scene-inflight";
    engine
        .configure(surface_id, top_layer_config("@mesh/live-matrix-inflight"))
        .unwrap();
    let Some(mut canvas) = heavy_scene(&mut engine, surface_id) else {
        engine.destroy_surface(surface_id);
        return;
    };
    let (width, height) = engine.surface_size(surface_id).unwrap().unwrap();
    let damage = DamageRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let present = |engine: &mut PresentationEngine, canvas: &mut SurfaceCanvas, visible| {
        engine
            .present_with_damage(surface_id, "inflight", visible, canvas, &[damage])
            .unwrap()
    };
    // Hide while in flight: the frame is dropped, the surface is not gated.
    record_heavy(&mut canvas, width, height, 0);
    assert_eq!(present(&mut engine, &mut canvas, true), PresentStatus::Submitted);
    assert!(engine.surface_waiting_for_buffer_release(surface_id));
    assert_eq!(present(&mut engine, &mut canvas, false), PresentStatus::Presented);
    assert!(!engine.surface_waiting_for_buffer_release(surface_id));
    std::thread::sleep(Duration::from_millis(50));
    assert!(engine.take_scene_completions().is_empty());
    engine.finish_frame().unwrap();
    // Visible again presents normally.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(wait_ready(&mut engine, surface_id, Duration::from_secs(5)));
        record_heavy(&mut canvas, width, height, 1);
        match present(&mut engine, &mut canvas, true) {
            PresentStatus::Submitted => {
                assert!(wait_scene(&mut engine, surface_id, deadline));
                break;
            }
            _ => {
                assert!(Instant::now() < deadline);
                engine.pump().unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    engine.finish_frame().unwrap();
    // Replace the canvas (a resize) and destroy the surface while in flight.
    let (w, h) = (canvas.width(), canvas.height());
    let mut resized = engine.allocate_canvas(w, h, true).unwrap();
    drop(canvas);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(wait_ready(&mut engine, surface_id, Duration::from_secs(5)));
        record_heavy(&mut resized, width, height, 2);
        match present(&mut engine, &mut resized, true) {
            PresentStatus::Submitted => break,
            _ => {
                assert!(Instant::now() < deadline);
                engine.pump().unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    engine.destroy_surface(surface_id);
    std::thread::sleep(Duration::from_millis(50));
    for done in engine.take_scene_completions() {
        assert!(!done.presented, "a destroyed surface must not present");
    }
    assert!(!engine.surface_waiting_for_buffer_release(surface_id));
    drop(resized);
    drop(engine);
}

/// `MESH_PIPELINE_PROFILE=1`-style measurement: per-frame "next frame work"
/// (`NEXT_WORK_MS`, standing in for scripts and layout) runs either after the
/// worker has published (`serialized`, the pre-pipelining behavior) or while
/// the worker renders (`overlapped`). Prints p50/p90 frame interval and
/// frame N+1 prepare-start latency.
#[cfg(feature = "gpu")]
#[test]
#[ignore = "live compositor measurement"]
fn scene_pipelining_profile() {
    let Some(mut engine) = live_engine() else {
        return;
    };
    let surface_id = "mesh-live-matrix-pipelining";
    engine
        .configure(surface_id, top_layer_config("@mesh/live-matrix-pipelining"))
        .unwrap();
    let Some(mut canvas) = heavy_scene(&mut engine, surface_id) else {
        engine.destroy_surface(surface_id);
        return;
    };
    let (width, height) = engine.surface_size(surface_id).unwrap().unwrap();
    let damage = DamageRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let work = Duration::from_micros(
        std::env::var("NEXT_WORK_US")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(4000),
    );
    let spin = |duration: Duration| {
        let start = Instant::now();
        while start.elapsed() < duration {
            std::hint::spin_loop();
        }
    };
    let percentile = |values: &mut Vec<f64>, p: f64| {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        values[((values.len() as f64 * p) as usize).min(values.len() - 1)]
    };
    for round in 0..6 {
        let overlapped = round % 2 == 1;
        let mut intervals = Vec::new();
        let mut latencies = Vec::new();
        let mut last_commit: Option<Instant> = None;
        let mut frame = 0;
        while intervals.len() < 120 {
            assert!(frame < 4000, "frames stopped presenting");
            engine.pump().unwrap();
            for done in engine.take_scene_completions() {
                assert!(done.presented);
                let now = Instant::now();
                if let Some(previous) = last_commit.replace(now) {
                    intervals.push((now - previous).as_secs_f64() * 1000.0);
                }
            }
            if engine.surface_waiting_for_frame_callback(surface_id)
                || engine.surface_waiting_for_buffer_release(surface_id)
            {
                std::thread::sleep(Duration::from_micros(100));
                continue;
            }
            record_heavy(&mut canvas, width, height, frame);
            frame += 1;
            let status = engine
                .present_with_damage(surface_id, "pipeline", true, &mut canvas, &[damage])
                .unwrap();
            if status != PresentStatus::Submitted {
                continue;
            }
            let submitted = Instant::now();
            if overlapped {
                latencies.push(submitted.elapsed().as_secs_f64() * 1000.0);
                spin(work);
            } else {
                // Serialized: block until the worker publishes, as the
                // synchronous present did.
                let deadline = Instant::now() + Duration::from_secs(5);
                while engine.surface_waiting_for_buffer_release(surface_id) {
                    assert!(Instant::now() < deadline);
                    engine.pump().unwrap();
                    for done in engine.take_scene_completions() {
                        assert!(done.presented);
                        let now = Instant::now();
                        if let Some(previous) = last_commit.replace(now) {
                            intervals.push((now - previous).as_secs_f64() * 1000.0);
                        }
                    }
                    std::thread::sleep(Duration::from_micros(50));
                }
                latencies.push(submitted.elapsed().as_secs_f64() * 1000.0);
                spin(work);
            }
        }
        eprintln!(
            "PIPELINE {} work={:?}: interval p50 {:.2} p90 {:.2} ms; frame N+1 script/layout start latency after N submit p50 {:.2} p90 {:.2} ms",
            if overlapped { "overlapped" } else { "serialized" },
            work,
            percentile(&mut intervals, 0.5),
            percentile(&mut intervals, 0.9),
            percentile(&mut latencies, 0.5),
            percentile(&mut latencies, 0.9),
        );
    }
    engine.destroy_surface(surface_id);
    drop(canvas);
    drop(engine);
}
