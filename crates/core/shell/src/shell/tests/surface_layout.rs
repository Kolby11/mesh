use super::common::*;
use super::*;

#[test]
fn content_size_is_the_root_laid_out_box() {
    // Sizing is fully CSS-driven now: the surface root's own laid-out box is
    // the measured size. The layout engine resolved the root's CSS width/height
    // (here `fit-content` shrank it to 336x332) with its clamps already
    // applied, so measurement just reads that box — no manifest inputs.
    let mut root = node("root", 0.0, 0.0, 336.0, 332.0);
    root.children.push(node("column", 12.0, 12.0, 312.0, 308.0));

    assert_eq!(measure_content_size(&root, 640, 360), (336, 332));
}

#[test]
fn content_size_falls_back_when_root_has_no_extent() {
    // A degenerate first frame (root not laid out yet) falls back to the
    // available size passed by the caller.
    let root = node("root", 0.0, 0.0, 0.0, 0.0);

    assert_eq!(measure_content_size(&root, 1920, 32), (1920, 32));
}

#[test]
fn frontend_settings_override_surface_layout_defaults() {
    let manifest = minimal_manifest("@mesh/base-surface");
    let mut store = mesh_core_config::SettingsStore::default();
    store.set_namespace(
        &manifest.package.id,
        serde_json::json!({
            "surface": {
                "anchor": "left",
                "layer": "overlay",
                "exclusive_zone": 12,
                "keyboard_mode": "exclusive",
                "visible_on_start": true
            }
        }),
    );

    let settings = resolve_frontend_module_settings(
        &manifest.package.id,
        store.namespace(&manifest.package.id),
        &manifest,
    );

    assert_eq!(settings.layout.edge, mesh_core_wayland::Edge::Left);
    assert_eq!(settings.layout.layer, mesh_core_wayland::Layer::Overlay);
    assert_eq!(settings.layout.exclusive_zone, 12);
    assert_eq!(
        settings.layout.keyboard_mode,
        mesh_core_wayland::KeyboardMode::Exclusive
    );
    assert!(settings.layout.visible_on_start);
}

#[test]
fn hide_surface_uses_configured_exit_transition_before_unmapping() {
    let state = Arc::new(Mutex::new(TransitionRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(TransitionRecordingComponent::new(
        "@test/transition",
        120,
        Arc::clone(&state),
    )));

    let emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: "@test/transition".into(),
        })
        .unwrap();
    assert!(
        emitted.is_empty(),
        "hide transition should not broadcast hidden until the timer elapses"
    );
    let surface = shell.core.surfaces.get("@test/transition").unwrap();
    assert!(surface.visible);
    assert!(surface.closing_until.is_some());
    assert_eq!(state.lock().unwrap().exiting, vec![true]);

    shell
        .core
        .surfaces
        .get_mut("@test/transition")
        .unwrap()
        .closing_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    let emitted = shell.complete_due_surface_transitions().unwrap();
    assert!(emitted.is_empty());
    let surface = shell.core.surfaces.get("@test/transition").unwrap();
    assert!(!surface.visible);
    assert!(surface.closing_until.is_none());
    assert_eq!(state.lock().unwrap().exiting, vec![true, false]);
}

#[test]
fn hide_surface_without_transition_unmaps_immediately() {
    let state = Arc::new(Mutex::new(TransitionRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(TransitionRecordingComponent::new(
        "@test/immediate",
        0,
        Arc::clone(&state),
    )));

    let emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: "@test/immediate".into(),
        })
        .unwrap();
    assert!(emitted.is_empty());
    let surface = shell.core.surfaces.get("@test/immediate").unwrap();
    assert!(!surface.visible);
    assert!(surface.closing_until.is_none());
    assert_eq!(state.lock().unwrap().exiting, vec![false]);
}

#[test]
fn wayland_parent_input_uses_content_size_not_tooltip_inflated_surface_size() {
    let state = Arc::new(Mutex::new(InputSizeRecordingState::default()));
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(InputSizeRecordingComponent::new(
        Arc::clone(&state),
        (100, 50),
    )));
    let surface = shell
        .surfaces
        .get_mut("@test/input-size")
        .expect("registered test surface");
    surface.width = 100;
    surface.height = 350;

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerMove {
            surface_id: "@test/input-size".into(),
            x: 20.0,
            y: 25.0,
        },
    );
    shell.dispatch_wayland().unwrap();

    assert_eq!(
        state.lock().unwrap().sizes,
        vec![(100, 50)],
        "parent input must rebuild/hit-test against the real content size, not the tooltip-padded buffer"
    );
}

#[test]
fn unconfigured_surface_keeps_pending_frame_until_configure_retry() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(MeasuredLayerGeometryComponent::new(
        "@test/configure-retry",
        (80, 40),
        (80, 40),
    )));
    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@test/configure-retry".into(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    let pending = mesh_core_render::DamageRect {
        x: 3,
        y: 4,
        width: 10,
        height: 8,
    };
    shell.components[0].parent.pending_present_damage = vec![pending];
    shell
        .presentation_engine
        .testing_set_surface_configured("@test/configure-retry", false);

    shell.render_components().unwrap();

    assert_eq!(
        shell.components[0].parent.pending_present_damage,
        [pending],
        "an unconfigured surface must retain its already-painted frame"
    );
    assert!(
        shell
            .presentation_engine
            .testing_presented_surfaces()
            .is_empty()
    );
    assert!(
        !shell.components_have_ready_render_work(),
        "pending configure work must wait for the Wayland fd instead of spinning"
    );

    shell
        .presentation_engine
        .testing_set_surface_configured("@test/configure-retry", true);
    assert!(shell.components_have_ready_render_work());
    shell.render_components().unwrap();

    assert!(shell.components[0].parent.pending_present_damage.is_empty());
    assert_eq!(
        shell.presentation_engine.testing_presented_surfaces(),
        ["@test/configure-retry"]
    );
}

#[test]
fn blur_settings_clamp_into_painter_quality() {
    let defaults = blur_quality_from_settings(&mesh_core_config::BlurSettings::default());
    assert_eq!(defaults.passes, 1);
    assert_eq!(defaults.max_radius, 96.0);

    // A hand-edited settings file is the point of the store, so out-of-range
    // values clamp to what the painter supports rather than disabling blur.
    let extreme = blur_quality_from_settings(&mesh_core_config::BlurSettings {
        passes: 9,
        max_radius: -4.0,
    });
    assert_eq!(extreme.passes, mesh_core_render::MAX_BLUR_PASSES);
    assert_eq!(extreme.max_radius, 0.0);
}
