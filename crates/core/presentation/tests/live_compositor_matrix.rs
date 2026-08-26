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
use mesh_core_render::{DamageRect, PixelBuffer};
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
    let buffer =
        PixelBuffer::try_new(width.max(1), height.max(1)).expect("buffer allocation must succeed");
    let full_damage = DamageRect {
        x: 0,
        y: 0,
        width: buffer.width().max(1),
        height: buffer.height().max(1),
    };

    let status = engine
        .present_with_damage(surface_id, "live-matrix", true, &buffer, &[full_damage])
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

    let buffer = PixelBuffer::try_new(1, 1).expect("buffer allocation must succeed");
    let damage = DamageRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    let status = engine
        .present_with_damage(surface_id, "live-matrix", true, &buffer, &[damage])
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
