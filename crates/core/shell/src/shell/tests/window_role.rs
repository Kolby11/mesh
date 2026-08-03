use super::common::*;
use super::*;

#[test]
fn window_close_request_hides_the_surface_and_keeps_the_component() {
    // xdg-shell's close is a request, not a destruction. Closing a window
    // surface must hide it — leaving the component, its services, and its Lua
    // state alive so reopening is the same cheap show as reopening a panel.
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/settings",
        state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@mesh/settings".into(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/settings")
            .is_some_and(|state| state.visible)
    );

    shell
        .presentation_engine
        .testing_push_close_request("@mesh/settings");
    shell.render_components().unwrap();

    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/settings")
            .is_some_and(|state| !state.visible),
        "a close request must hide the window surface"
    );
    assert!(
        shell
            .components
            .iter()
            .any(|runtime| runtime.surface_id == "@mesh/settings"),
        "closing a window must not tear down its component"
    );
}

/// Register a shown, promotable surface and give it one configured frame, so a
/// role change afterwards is exercised against real cached shell state rather
/// than against a surface that was never configured.
fn promotable_surface_shell(surface_id: &str) -> (Shell, Arc<Mutex<FocusRecordingState>>) {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::promotable(
        surface_id,
        Arc::clone(&state),
    )));
    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: surface_id.into(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    if let Some(runtime) = shell
        .components
        .iter_mut()
        .find(|runtime| runtime.surface_id == surface_id)
    {
        runtime.parent.known_surface_size = Some((920, 700));
        runtime.parent.last_surface_config = Some(mesh_core_presentation::SurfaceConfig::default());
    }
    (shell, state)
}

#[test]
fn promoting_a_surface_keeps_its_component_and_invalidates_the_cached_config() {
    // Runtime promotion is a presentation-layer swap: the component runtime is
    // the whole point of it surviving. What the shell must drop is the state
    // that describes the destroyed compositor object, or the render loop
    // compares against it and never sends the configure that creates the
    // replacement.
    let (mut shell, state) = promotable_surface_shell("@mesh/settings");

    shell
        .apply_request(CoreRequest::SetSurfaceRole {
            surface_id: "@mesh/settings".into(),
            role: mesh_core_wayland::SurfaceRole::Window,
        })
        .unwrap();

    assert_eq!(
        state.lock().unwrap().applied_roles,
        vec![mesh_core_wayland::SurfaceRole::Window]
    );
    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/settings")
        .expect("promotion must not tear down the component");
    assert!(
        runtime.parent.last_surface_config.is_none(),
        "the cached config describes the surface that was just destroyed"
    );
    assert!(
        runtime.parent.known_surface_size.is_none(),
        "sizing inverts with the role, so the new surface must be measured afresh"
    );
    assert!(runtime.parent.force_full_present);
    // The old compositor object must be gone *before* the next render frame
    // reads any size from it. Leaving it for `configure` to swap lazily —
    // partway through that frame, after the loop has already read
    // `window_configured_size` — makes the surface lay out against the size the
    // other role was given, which strands a demoted surface at the 1x1 its
    // unmeasured 0x0 request gets clamped to. Found against a live compositor.
    assert_eq!(
        shell.presentation_engine.testing_destroyed_surfaces(),
        ["@mesh/settings"]
    );
}

#[test]
fn toggling_a_surface_role_alternates_between_chrome_and_window() {
    let (mut shell, state) = promotable_surface_shell("@mesh/settings");

    for _ in 0..2 {
        shell
            .apply_request(CoreRequest::ToggleSurfaceRole {
                surface_id: "@mesh/settings".into(),
            })
            .unwrap();
    }

    assert_eq!(
        state.lock().unwrap().applied_roles,
        vec![
            mesh_core_wayland::SurfaceRole::Window,
            mesh_core_wayland::SurfaceRole::Layer
        ],
        "docking back must return the surface to chrome, not leave it a window"
    );
}

#[test]
fn setting_the_role_a_surface_already_has_is_not_a_change() {
    let (mut shell, state) = promotable_surface_shell("@mesh/settings");

    shell
        .apply_request(CoreRequest::SetSurfaceRole {
            surface_id: "@mesh/settings".into(),
            role: mesh_core_wayland::SurfaceRole::Layer,
        })
        .unwrap();

    assert!(
        state.lock().unwrap().applied_roles.is_empty(),
        "a no-op role request must not destroy and recreate the compositor object"
    );
    assert!(
        shell
            .components
            .iter()
            .find(|runtime| runtime.surface_id == "@mesh/settings")
            .is_some_and(|runtime| runtime.parent.last_surface_config.is_some()),
        "a no-op must leave the cached config in place"
    );
}

#[test]
fn a_surface_that_is_not_promotable_refuses_a_role_change() {
    // The opt-in is the point: a component laid out for one role is not
    // automatically usable in the other, so the author declares that both were
    // designed for.
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        Arc::clone(&state),
    )));

    shell
        .apply_request(CoreRequest::SetSurfaceRole {
            surface_id: "@mesh/navigation-bar".into(),
            role: mesh_core_wayland::SurfaceRole::Window,
        })
        .unwrap();

    assert!(state.lock().unwrap().applied_roles.is_empty());
}

#[test]
fn a_role_change_for_an_unknown_surface_is_ignored() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleSurfaceRole {
            surface_id: "@mesh/not-installed".into(),
        })
        .unwrap();
}

#[test]
fn compositor_window_states_reach_the_component_each_render() {
    // Fullscreen, maximize, and tiling are compositor decisions that arrive on
    // the toplevel configure. The render loop must hand them to the component
    // before it resolves a size, so the surface can restyle (and re-measure)
    // for the size it was given rather than the one it asked for.
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/settings",
        Arc::clone(&state),
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@mesh/settings".into(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    // A real frontend sets its role from `mesh.surface.role` on its first
    // render; the recording component paints nothing, so declare it here.
    shell
        .surfaces
        .get_mut("@mesh/settings")
        .expect("surface registered")
        .role = mesh_core_wayland::SurfaceRole::Window;
    shell.render_components().unwrap();
    assert_eq!(
        state.lock().unwrap().window_states,
        vec![mesh_core_wayland::WindowStates::default()],
        "a window with no configure yet is neither fullscreen nor activated"
    );

    let fullscreen = mesh_core_wayland::WindowStates {
        fullscreen: true,
        activated: true,
        ..mesh_core_wayland::WindowStates::default()
    };
    shell
        .presentation_engine
        .testing_set_window_states("@mesh/settings", fullscreen);
    shell.render_components().unwrap();
    shell.render_components().unwrap();

    assert_eq!(
        state.lock().unwrap().window_states,
        vec![mesh_core_wayland::WindowStates::default(), fullscreen],
        "states must be delivered once per change, not once per frame"
    );
}

#[test]
fn window_close_request_for_an_unknown_surface_is_ignored() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);

    shell
        .presentation_engine
        .testing_push_close_request("@mesh/never-registered");

    shell.render_components().unwrap();
    assert!(
        shell.core.surfaces.get("@mesh/never-registered").is_none(),
        "a close request for an unknown surface must not invent surface state"
    );
}
