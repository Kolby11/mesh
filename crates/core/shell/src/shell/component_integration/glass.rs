use super::*;
/// Drives one real glass surface through the shell runtime against the
/// testing presentation backend.
struct ShownSurfaceHarness {
    shell: crate::shell::Shell,
    id: String,
}

impl ShownSurfaceHarness {
    fn wallpapers() -> Self {
        let mut component = GlassCatalog::new().mount("glass-wallpapers");
        let wallpapers: Vec<_> = (0..6)
            .map(|index| {
                serde_json::json!({
                    "path": format!("/walls/{index}.png"),
                    "name": format!("wall {index}"),
                    "thumbnail": "",
                    "color": "#334455",
                })
            })
            .collect();
        update(
            &mut component,
            "mesh.wallpapers",
            serde_json::json!({"available": true, "directory": "/walls", "wallpapers": wallpapers}),
        );
        let id = component.surface_id().to_owned();
        let mut shell = crate::shell::Shell::new();
        shell.presentation_engine =
            mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
        shell.register_component(Box::new(component));
        shell.debug.profiling_enabled = true;
        let mut harness = Self { shell, id };
        harness.set_visible(false);
        harness
    }

    fn set_visible(&mut self, visible: bool) {
        let surface_id = self.id.clone();
        let request = if visible {
            CoreRequest::ShowSurface { surface_id }
        } else {
            CoreRequest::HideSurface { surface_id }
        };
        let mut emitted = self.shell.apply_request(request).unwrap();
        self.shell.drain_requests(&mut emitted).unwrap();
        if let Some(state) = self.shell.core.surfaces.get_mut(&self.id)
            && state.closing_until.is_some()
        {
            state.closing_until = Some(std::time::Instant::now());
            let mut emitted = self.shell.complete_due_surface_transitions().unwrap();
            self.shell.drain_requests(&mut emitted).unwrap();
        }
        self.shell.render_components().unwrap();
        if !visible {
            // The hiding present unmaps the surface; it must be configured
            // again before it can take another buffer.
            self.shell
                .presentation_engine
                .testing_set_surface_configured(&self.id, false);
        }
    }

    fn configures(&self) -> usize {
        self.shell
            .presentation_engine
            .testing_surface_config_history()
            .iter()
            .filter(|(surface, _)| *surface == self.id)
            .count()
    }

    /// Show the surface, answer its configure, and count what that took.
    fn show(&mut self) -> ShowCost {
        use crate::shell::runtime::profiling::CapturedRecord;
        use mesh_core_debug::ProfilingStage;

        self.shell.profiling.capture = Some(Vec::new());
        let configures_before = self.configures();
        self.set_visible(true);
        let configures = self.configures() - configures_before;
        self.shell
            .presentation_engine
            .testing_set_surface_configured(&self.id, true);
        self.shell.render_components().unwrap();
        let records = self.shell.profiling.capture.take().unwrap();
        let count = |stage: ProfilingStage, trigger: Option<&str>| {
            records
                .iter()
                .filter(|record| {
                    matches!(record, CapturedRecord::Stage { surface_id, stage: s, trigger: t, .. }
                        if *surface_id == self.id && *s == stage
                            && trigger.is_none_or(|trigger| t.as_deref() == Some(trigger)))
                })
                .count()
        };
        ShowCost {
            configures,
            rebuilds: count(ProfilingStage::TreeBuild, Some("rebuild")),
            renders: count(ProfilingStage::TotalSurfaceRender, None),
            paints: count(ProfilingStage::Paint, None),
            presents: count(ProfilingStage::PresentCommit, None),
        }
    }
}

/// Clicking the bar claims the keyboard for it. When the compositor then
/// focuses an exclusive surface as it maps, key routing follows, and a press
/// outside the exclusive surface (which the compositor still delivers to it)
/// dismisses it.
#[test]
fn exclusive_surface_takes_key_routing_and_dismisses_on_outside_press() {
    let catalog = GlassCatalog::new();
    let bar = catalog.mount("glass-bar");
    let mut carousel = catalog.mount("wallpaper-carousel");
    update(
        &mut carousel,
        "mesh.wallpapers",
        serde_json::json!({"available": true, "directory": "/walls", "wallpapers": []}),
    );
    let bar_id = bar.surface_id().to_owned();
    let carousel_id = carousel.surface_id().to_owned();
    let mut shell = crate::shell::Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(bar));
    shell.register_component(Box::new(carousel));
    let mut emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: carousel_id.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell.render_components().unwrap();

    shell.claim_keyboard_focus_for_surface(&bar_id);
    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: carousel_id.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell
        .pending_wayland_events
        .push_back(mesh_core_presentation::WindowEvent::KeyboardEnter {
            surface_id: carousel_id.as_str().into(),
        });
    shell.dispatch_wayland().unwrap();
    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some(carousel_id.as_str())
    );

    shell.render_components().unwrap();
    shell
        .presentation_engine
        .testing_set_surface_configured(&carousel_id, true);
    shell.render_components().unwrap();
    shell
        .pending_wayland_events
        .push_back(mesh_core_presentation::WindowEvent::PointerButton {
            surface_id: carousel_id.as_str().into(),
            x: 10.0,
            y: 5000.0,
            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
            pressed: true,
        });
    shell.dispatch_wayland().unwrap();
    let state = &shell.core.surfaces[&carousel_id];
    assert!(
        !state.visible || state.closing_until.is_some(),
        "an outside press dismisses the exclusive surface"
    );
}

/// Enter on the bar's dashboard button shows the dashboard, which takes key
/// routing. The Enter release that follows must stay with the bar: delivered
/// to the dashboard it activates the dashboard's focused close button.
#[test]
fn key_release_after_opening_exclusive_surface_stays_with_the_opener() {
    let catalog = GlassCatalog::new();
    let mut bar = catalog.mount("glass-bar");
    paint(&mut bar, 1920, 64);
    focus_by_label(&mut bar, "Dashboard");
    // Closing the dashboard with its close button leaves that button focused
    // for the next open.
    let mut dashboard = catalog.mount("glass-dashboard");
    paint(&mut dashboard, 1920, 1080);
    focus_by_label(&mut dashboard, "Close dashboard");
    let bar_id = bar.surface_id().to_owned();
    let dashboard_id = dashboard.surface_id().to_owned();
    let mut shell = crate::shell::Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(bar));
    shell.register_component(Box::new(dashboard));
    let mut emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: dashboard_id.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell.render_components().unwrap();
    shell.claim_keyboard_focus_for_surface(&bar_id);

    let key = |surface_id: &str, event| mesh_core_presentation::WindowEvent::Key {
        surface_id: surface_id.into(),
        event,
    };
    shell.pending_wayland_events.push_back(key(
        &bar_id,
        mesh_core_presentation::WindowKeyEvent::Pressed("Enter".into(), Default::default()),
    ));
    shell.dispatch_wayland().unwrap();
    shell.render_components().unwrap();
    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some(dashboard_id.as_str())
    );
    shell
        .presentation_engine
        .testing_set_surface_configured(&dashboard_id, true);
    shell.render_components().unwrap();

    shell.pending_wayland_events.push_back(key(
        &dashboard_id,
        mesh_core_presentation::WindowKeyEvent::Released("Enter".into()),
    ));
    shell.dispatch_wayland().unwrap();
    shell.render_components().unwrap();
    let state = &shell.core.surfaces[&dashboard_id];
    assert!(
        state.visible && state.closing_until.is_none(),
        "the opening key's release must not close the dashboard"
    );
    assert_eq!(
        shell.presentation_engine.testing_keyboard_focus_requests(),
        [dashboard_id.clone()],
        "a surface the user opens asks the compositor for the keyboard"
    );

    let mut emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: dashboard_id.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some(bar_id.as_str())
    );
    assert_eq!(
        shell.presentation_engine.testing_keyboard_focus_requests(),
        [dashboard_id.clone(), bar_id.clone()],
        "hiding it hands the keyboard back to the surface that opened it"
    );
}

/// A surface shown without a user gesture, such as a notification a service
/// raises, leaves the keyboard where it is.
#[test]
fn surface_shown_without_user_gesture_leaves_the_keyboard() {
    let catalog = GlassCatalog::new();
    let bar = catalog.mount("glass-bar");
    let dashboard = catalog.mount("glass-dashboard");
    let bar_id = bar.surface_id().to_owned();
    let dashboard_id = dashboard.surface_id().to_owned();
    let mut shell = crate::shell::Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(bar));
    shell.register_component(Box::new(dashboard));
    let mut emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: dashboard_id.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell.render_components().unwrap();
    shell.claim_keyboard_focus_for_surface(&bar_id);

    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: dashboard_id.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some(bar_id.as_str())
    );
    assert!(
        shell
            .presentation_engine
            .testing_keyboard_focus_requests()
            .is_empty()
    );
}

/// `mesh-shell focus <surface>` lets a compositor keybind reach `on_demand`
/// chrome without a pointer press.
#[test]
fn focus_surface_request_gives_visible_chrome_the_keyboard() {
    let catalog = GlassCatalog::new();
    let bar = catalog.mount("glass-bar");
    let dashboard = catalog.mount("glass-dashboard");
    let bar_id = bar.surface_id().to_owned();
    let dashboard_id = dashboard.surface_id().to_owned();
    let mut shell = crate::shell::Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(bar));
    shell.register_component(Box::new(dashboard));
    let mut emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: dashboard_id.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell.render_components().unwrap();
    assert_eq!(shell.keyboard_focus_surface, None);

    let mut emitted = shell
        .apply_request(
            crate::shell::ipc::parse_ipc_command(&format!("shell:focus_surface:{dashboard_id}"))
                .unwrap(),
        )
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    assert_eq!(
        shell.keyboard_focus_surface, None,
        "a hidden surface is not focused"
    );

    let mut emitted = shell
        .apply_request(
            crate::shell::ipc::parse_ipc_command(&format!("shell:focus_surface:{bar_id}")).unwrap(),
        )
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some(bar_id.as_str())
    );
    assert_eq!(
        shell.presentation_engine.testing_keyboard_focus_requests(),
        [bar_id.clone()]
    );
}


#[derive(Debug, PartialEq)]
struct ShowCost {
    configures: usize,
    rebuilds: usize,
    renders: usize,
    paints: usize,
    presents: usize,
}

/// One configure, then one paint at the size the compositor grants, from the
/// retained tree.
const RETAINED_SHOW: ShowCost = ShowCost {
    configures: 1,
    rebuilds: 0,
    renders: 1,
    paints: 1,
    presents: 1,
};

/// Re-showing a hidden surface reuses its retained tree and paints once, at
/// the size the compositor configures, instead of painting an unconfigured
/// pass and rebuilding the tree for each.
#[test]
fn reshown_glass_surface_reuses_its_tree_and_paints_once() {
    let mut harness = ShownSurfaceHarness::wallpapers();
    harness.show();
    harness.set_visible(false);
    assert_eq!(harness.show(), RETAINED_SHOW);
}

/// An idle prewarm gives a never-shown surface the retained tree a re-shown
/// one has, so its first show costs the same.
#[test]
fn prewarmed_glass_surface_first_show_paints_once() {
    let mut harness = ShownSurfaceHarness::wallpapers();
    assert!(harness.shell.prewarm_hidden_surface().unwrap());
    assert!(
        !harness.shell.prewarm_hidden_surface().unwrap(),
        "each hidden surface is prewarmed once"
    );
    assert_eq!(harness.configures(), 0, "a prewarm configures nothing");
    assert!(
        harness
            .shell
            .presentation_engine
            .testing_presented_surfaces()
            .is_empty(),
        "a prewarm presents nothing"
    );
    assert_eq!(harness.show(), RETAINED_SHOW);
}
