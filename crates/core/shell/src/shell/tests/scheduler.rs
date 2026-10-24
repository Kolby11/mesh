use super::common::*;
use super::*;

#[test]
fn scheduler_uses_component_tick_deadline() {
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    let deadline = Instant::now() + Duration::from_millis(120);
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            DeadlineTickComponent::new("@test/deadline", Some(deadline)),
        )));

    let sleep = shell.next_runtime_sleep(false);

    assert!(sleep <= Duration::from_millis(120), "{sleep:?}");
    assert!(sleep >= Duration::from_millis(80), "{sleep:?}");
}

#[test]
fn scheduler_wakes_immediately_for_due_component_tick_deadline() {
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            DeadlineTickComponent::new(
                "@test/deadline",
                Some(Instant::now() - Duration::from_millis(1)),
            ),
        )));

    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
}

#[test]
fn scheduler_wakes_for_visible_dirty_component_even_without_previous_present() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/dirty",
        None,
        Arc::clone(&state),
    )));
    {
        shell
            .core
            .surfaces
            .get_mut("@test/dirty")
            .expect("registered core surface")
            .visible = true;
        let surface = shell
            .surfaces
            .get_mut("@test/dirty")
            .expect("registered surface target");
        surface.visible = true;
        surface.width = 120;
        surface.height = 36;
    }

    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
}

#[test]
fn scheduler_ignores_hidden_component_deadlines_and_render_dirtiness() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/hidden",
        Some(Instant::now()),
        Arc::clone(&state),
    )));
    shell
        .core
        .surfaces
        .get_mut("@test/hidden")
        .expect("hidden surface state")
        .visible = false;

    let sleep = shell.next_runtime_sleep(false);

    assert!(
        sleep >= Duration::from_secs(30),
        "hidden dirty component should not force an immediate wake: {sleep:?}"
    );
}

#[test]
fn render_skips_already_hidden_dirty_surface() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/hidden",
        None,
        Arc::clone(&state),
    )));
    shell
        .core
        .surfaces
        .get_mut("@test/hidden")
        .expect("hidden surface state")
        .visible = false;

    shell.render_components().unwrap();

    assert_eq!(state.lock().unwrap().render_calls, 0);
}

struct FloodingTickComponent;

impl super::types::ShellComponent for FloodingTickComponent {
    fn id(&self) -> &str {
        "@test/flood"
    }

    fn surface_id(&self) -> &str {
        "@test/flood"
    }

    fn mount(
        &mut self,
        _ctx: super::types::ComponentContext,
    ) -> Result<Vec<CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn handle_core_event(
        &mut self,
        _event: &super::types::CoreEvent,
    ) -> Result<Vec<CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        _event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, super::types::ComponentError> {
        Ok((0..100)
            .map(|index| CoreRequest::PublishDiagnostics {
                message: format!("flood {index}"),
            })
            .collect())
    }

    fn wants_render(&self) -> bool {
        false
    }

    fn render(
        &mut self,
        _surface: &mut dyn mesh_core_wayland::ShellSurface,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        _extent: crate::shell::types::SurfaceExtent,
        _buffer: &mut mesh_core_render::SurfaceCanvas,
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }
}

/// `PublishDiagnostics` targets no surface, so before attribution a flood of
/// it was charged to `@mesh/shell`, which is never quarantined.
#[test]
fn component_tick_effects_are_charged_to_the_component() {
    let mut shell = Shell::new();
    shell.register_component(Box::new(FloodingTickComponent));
    let run_frame = |shell: &mut Shell| {
        let requests = shell.tick_components().unwrap();
        shell.enqueue_effects(requests);
        shell.process_effects();
    };

    run_frame(&mut shell);
    run_frame(&mut shell);
    assert!(!shell.components[0].quarantined);
    run_frame(&mut shell);

    assert!(shell.components[0].quarantined);
    assert!(shell.diagnostics.snapshot().iter().any(|entry| {
        entry.module_id == "@test/flood"
            && entry
                .health
                .to_string()
                .contains("effect_scheduler_quarantined")
    }));
}

#[test]
fn healthy_watcher_coalesces_reload_checks_with_watch_maintenance() {
    let mut shell = Shell::new();
    shell.file_watcher_active = true;
    let now = Instant::now();
    let maintenance = now + Duration::from_secs(2);
    shell.next_watch_reconcile = maintenance;
    for offset_ms in [0, 3, 11, 30] {
        for fallback_ms in [250, 500, 1000] {
            assert_eq!(
                shell.next_reload_check(
                    now + Duration::from_millis(offset_ms),
                    Duration::from_millis(fallback_ms)
                ),
                maintenance,
            );
        }
    }
    shell.file_watcher_active = false;
    assert_eq!(
        shell.next_reload_check(now, Duration::from_millis(250)),
        now + Duration::from_millis(250)
    );
}

#[test]
fn scheduler_includes_watch_reconciliation_deadline() {
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell.next_watch_reconcile = Instant::now() - Duration::from_millis(1);
    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
}

#[test]
#[ignore = "deterministic release-only idle reload deadline count gate"]
fn d9_coalesced_reload_wakeup_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    let mut shell = Shell::new();
    shell.file_watcher_active = true;
    let start = Instant::now();
    let interval = Duration::from_secs(2);
    // Model three checks completing at different times during each maintenance
    // pass. The old policy added an interval to each completion independently.
    let mut max_wakeups_per_minute = 0;
    for run in 0..3 {
        let mut independent = std::collections::BTreeSet::new();
        let mut coalesced = std::collections::BTreeSet::new();
        for pass in 1..=60 {
            let anchor = start + interval * pass;
            shell.next_watch_reconcile = anchor + interval;
            for offset in [1, 5 + run, 12 + run] {
                let completed = anchor + Duration::from_millis(offset);
                independent.insert(completed + interval);
                coalesced.insert(shell.next_reload_check(completed, Duration::from_millis(250)));
            }
        }
        eprintln!(
            "idle reload deadlines: 60 passes / 120 s run={run}, independent={} coalesced={}",
            independent.len(),
            coalesced.len()
        );
        assert_eq!(independent.len(), coalesced.len() * 3);
        max_wakeups_per_minute = max_wakeups_per_minute.max(coalesced.len() / 2);
    }
    shell.next_frontend_reload_check = Instant::now() + interval;
    shell.next_theme_reload_check = shell.next_frontend_reload_check;
    shell.next_shell_settings_reload_check = shell.next_frontend_reload_check;
    shell.next_watch_reconcile = shell.next_frontend_reload_check;
    assert!(
        shell.next_runtime_sleep(false) >= Duration::from_millis(1900),
        "idle scheduler must sleep until maintenance"
    );
    println!("MESH_PERF metric=d9_idle_wakeups_per_minute value={max_wakeups_per_minute}");
}

#[test]
#[ignore = "release-only idle watch-set reconcile profile"]
fn idle_watch_set_reconcile_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    let mut shell = Shell::new();
    shell.load_installed_module_graph_cached().unwrap();
    shell.reconcile_file_watcher();
    let paths = shell.file_watch_set.paths.len();
    let modules = shell
        .installed_module_graph
        .as_ref()
        .map_or(0, |graph| graph.modules().len());
    let iterations = 2_000u32;
    let started = Instant::now();
    for _ in 0..iterations {
        shell.reconcile_file_watcher();
    }
    let elapsed = started.elapsed();
    eprintln!(
        "idle reconcile: {modules} modules, {paths} watch paths, {iterations} passes {elapsed:?} ({:.1} us/pass)",
        elapsed.as_secs_f64() * 1e6 / f64::from(iterations)
    );
}

#[test]
fn scheduler_uses_focus_grab_expiry_without_component_work() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    park_reload_deadlines(&mut shell);
    shell
        .presentation_engine
        .testing_set_focus_grab_deadline(Some(Instant::now() + Duration::from_millis(120)));
    let sleep = shell.next_runtime_sleep(false);
    assert!(sleep <= Duration::from_millis(120), "{sleep:?}");
    assert!(sleep >= Duration::from_millis(80), "{sleep:?}");
    shell
        .presentation_engine
        .testing_set_focus_grab_deadline(Some(Instant::now() - Duration::from_millis(1)));
    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
}

#[test]
fn scheduler_uses_frame_callback_escape_for_dirty_visible_targets() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    park_reload_deadlines(&mut shell);
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/callback",
        None,
        state,
    )));
    shell
        .core
        .surfaces
        .get_mut("@test/callback")
        .unwrap()
        .visible = true;
    let surface = shell.surfaces.get_mut("@test/callback").unwrap();
    surface.visible = true;
    surface.width = 120;
    surface.height = 36;
    shell
        .presentation_engine
        .testing_set_surface_frame_pending("@test/callback", true);
    shell
        .presentation_engine
        .testing_set_frame_callback_deadline(
            "@test/callback",
            Instant::now() + Duration::from_millis(50),
        );
    let sleep = shell.next_runtime_sleep(false);
    assert!(sleep <= Duration::from_millis(50), "{sleep:?}");
    assert!(sleep >= Duration::from_millis(20), "{sleep:?}");
    shell
        .presentation_engine
        .testing_set_frame_callback_deadline(
            "@test/callback",
            Instant::now() - Duration::from_millis(1),
        );
    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
    shell
        .core
        .surfaces
        .get_mut("@test/callback")
        .unwrap()
        .visible = false;
    assert!(shell.next_runtime_sleep(false) >= Duration::from_secs(30));
}

#[test]
fn scheduler_wakes_for_host_requests_queued_after_paint() {
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    assert!(shell.next_runtime_sleep(false) > Duration::ZERO);
    shell.enqueue_effects([CoreRequest::PublishDiagnostics {
        message: "paint-time effect".into(),
    }]);
    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
    shell.process_effects();
    assert!(shell.next_runtime_sleep(false) > Duration::ZERO);
}
