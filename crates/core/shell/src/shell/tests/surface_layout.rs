use super::common::*;
use super::types::TargetRef;
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
fn wayland_parent_click_does_not_cache_padded_size_for_spanning_surface() {
    let state = Arc::new(Mutex::new(InputSizeRecordingState::default()));
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(InputSizeRecordingComponent::new(
        Arc::clone(&state),
        (1920, 56),
    )));
    let surface = shell
        .surfaces
        .get_mut("@test/input-size")
        .expect("registered test surface");
    // A top/bottom spanning layer surface keeps width 0 in the shell record;
    // the paint buffer is wider/taller because it includes tooltip reserve.
    surface.width = 0;
    surface.height = 56;
    shell.components[0].parent.paint_buffer = Some(mesh_core_render::PixelBuffer::new(1920, 256));

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerButton {
            surface_id: "@test/input-size".into(),
            x: 1840.0,
            y: 28.0,
            pressed: true,
        },
    );
    shell.dispatch_wayland().unwrap();

    assert_eq!(state.lock().unwrap().sizes, vec![(1920, 56)]);
    assert_eq!(
        shell.components[0].parent.known_surface_size,
        Some((1920, 56)),
        "parent input must preserve content geometry instead of caching the padded spanning buffer"
    );
}

#[test]
fn padded_parent_surface_size_is_converted_to_content_geometry() {
    let mut shell = Shell::new();
    shell.register_component(Box::new(InputSizeRecordingComponent::new(
        Arc::new(Mutex::new(InputSizeRecordingState::default())),
        (1920, 56),
    )));
    shell.components[0].parent.last_surface_config = Some(mesh_core_presentation::SurfaceConfig {
        width: 0,
        height: 256,
        padding: mesh_core_presentation::SurfacePadding::trailing(0, 200),
        ..Default::default()
    });

    assert_eq!(
        shell.content_size_for_target(0, TargetRef::Parent, (1920, 256)),
        (1920, 56),
        "component geometry must exclude the tooltip reserve"
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

// ---------------------------------------------------------------------------
// Input-region / dead-zone regression tests
//
// MESH asks the compositor for surfaces that are *larger* than their content:
// a bar reserves 200 logical pixels below itself so tooltips can paint outside
// its content box, and a popover reserves a ring for shadow/filter overshoot.
// Those pixels are transparent. If the surface's input region does not exclude
// them, the compositor routes every click over them to MESH and the windows
// underneath get a dead zone — "the shell blocks a strip under the navigation
// bar", which has been reintroduced roughly twenty times.
//
// The reserve and the input padding now come from one function
// (`surface_geometry_with_overlay_reserve`) and travel together inside
// `SurfaceConfig`/`PopupConfig`, and the backend re-derives the region from
// that padding on every commit. The tests below pin both halves: that the
// reserve is declared, and that the resulting region is content-sized.
// ---------------------------------------------------------------------------

/// The whole bug, stated once: an inflated bar surface must not take input over
/// the strip it reserved for tooltips.
#[test]
fn layer_surface_input_region_excludes_the_tooltip_overlay_reserve() {
    const CONTENT: (u32, u32) = (1920, 56);

    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(MeasuredLayerGeometryComponent::new(
        "@test/bar",
        CONTENT,
        CONTENT,
    )));
    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@test/bar".into(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell.render_components().unwrap();

    let cfg = configured_surface(&shell, "@test/bar");
    assert!(
        cfg.height > CONTENT.1,
        "the bar surface is expected to be inflated by the tooltip overlay reserve; \
         if that reserve is gone this test is measuring nothing: {cfg:?}"
    );

    let region = shell
        .presentation_engine
        .input_region("@test/bar")
        .expect("an inflated surface must confine its input region");
    assert_eq!(
        (region.x, region.y, region.width, region.height),
        (0, 0, CONTENT.0, CONTENT.1),
        "pointer input must stop at the content rect; every logical pixel of \
         {}x{} beyond it is a dead zone over the windows below the bar",
        cfg.width,
        cfg.height
    );
}

#[test]
fn first_dynamic_layer_configure_uses_measured_content_not_one_pixel_fallback() {
    const CONTENT: (u32, u32) = (1920, 56);

    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(MeasuredLayerGeometryComponent::new(
        "@test/kde-navigation",
        CONTENT,
        (CONTENT.0, 1),
    )));
    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@test/kde-navigation".into(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    shell.render_components().unwrap();

    let configured_sizes = shell
        .presentation_engine
        .testing_surface_config_history()
        .iter()
        .filter(|(id, _)| id == "@test/kde-navigation")
        .map(|(_, cfg)| (cfg.width, cfg.height))
        .collect::<Vec<_>>();
    assert_eq!(
        configured_sizes,
        [(CONTENT.0, CONTENT.1 + 200)],
        "the compositor must never see the transient 1px content + 200px reserve geometry"
    );
}

/// The invariant behind the fix, checked over whatever the shell actually
/// configured rather than over one hand-built case: a surface may only be
/// inflated if it declares that same inflation as input padding.
///
/// This is the test to keep. Any future code that grows a surface — a new
/// overlay reserve, a drop-shadow margin, a resize grip — trips it unless the
/// growth is declared, which is exactly the mistake that keeps coming back.
#[test]
fn every_configured_surface_declares_its_inflation_as_input_padding() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(MeasuredLayerGeometryComponent::new(
        "@test/bar",
        (1920, 56),
        (1920, 56),
    )));
    shell.register_component(Box::new(MeasuredLayerGeometryComponent::new(
        "@test/popover",
        (280, 164),
        (280, 164),
    )));
    for surface_id in ["@test/bar", "@test/popover"] {
        let mut emitted = shell
            .apply_request(CoreRequest::ShowSurface {
                surface_id: surface_id.into(),
            })
            .unwrap();
        shell.drain_requests(&mut emitted).unwrap();
    }
    shell.render_components().unwrap();

    let configs = shell.presentation_engine.testing_surface_configs();
    assert!(
        !configs.is_empty(),
        "no surface was configured; the assertions below would be vacuous"
    );
    for (surface_id, cfg) in configs {
        let content = shell
            .surfaces
            .get(&surface_id)
            .map(|surface| (surface.width, surface.height))
            .unwrap_or_else(|| panic!("configured surface {surface_id} has no shell record"));
        assert_eq!(
            (
                cfg.width - cfg.padding.left - cfg.padding.right,
                cfg.height - cfg.padding.top - cfg.padding.bottom,
            ),
            content,
            "{surface_id} was configured at {}x{} for {content:?} of content, so \
             {:?} of that must be declared input padding — otherwise the \
             difference silently becomes a click dead zone",
            cfg.width,
            cfg.height,
            cfg.padding
        );
    }
}

/// A toplevel's size *is* its content size, so it is never inflated and takes
/// input over its whole area. Guards the other direction: a change that starts
/// padding windows would make them unclickable at the edges.
#[test]
fn window_surface_is_not_inflated_and_takes_input_over_its_whole_area() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    shell.register_component(Box::new(MeasuredLayerGeometryComponent::new(
        "@test/window",
        (640, 480),
        (640, 480),
    )));
    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@test/window".into(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell
        .surfaces
        .get_mut("@test/window")
        .expect("surface registered")
        .role = mesh_core_wayland::SurfaceRole::Window;
    shell.render_components().unwrap();

    let cfg = configured_surface(&shell, "@test/window");
    assert_eq!(
        (cfg.width, cfg.height),
        (640, 480),
        "a window is sized by its content, never inflated by an overlay reserve"
    );
    assert!(
        cfg.padding.is_zero(),
        "an uninflated surface declares no padding: {:?}",
        cfg.padding
    );
    assert!(
        shell
            .presentation_engine
            .input_region("@test/window")
            .is_none(),
        "with nothing reserved the whole window takes input"
    );
}

fn configured_surface(shell: &Shell, surface_id: &str) -> mesh_core_presentation::SurfaceConfig {
    shell
        .presentation_engine
        .testing_surface_configs()
        .into_iter()
        .find(|(id, _)| id == surface_id)
        .unwrap_or_else(|| panic!("{surface_id} was never configured"))
        .1
}
