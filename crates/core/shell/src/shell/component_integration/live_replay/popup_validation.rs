use super::*;
use mesh_core_frontend_shell_adapter::TabFocusTarget;
use mesh_core_presentation::WindowKeyEvent;

#[derive(Clone, Copy)]
enum FocusPhase {
    Boot,
    Open(usize),
    Hold(usize),
    Close(usize),
    Closed(usize),
    Idle,
    Dashboard,
    Cpu,
    Spectrum,
    Finish,
}

struct FocusReplay {
    base: Replay,
    phase: FocusPhase,
    since: Instant,
    next: Instant,
    counter: u32,
}

impl FocusReplay {
    fn enter(&mut self, phase: FocusPhase, now: Instant) {
        self.phase = phase;
        self.since = now;
        self.next = now;
        self.counter = 0;
    }

    fn window(&self, label: &'static str, now: Instant) {
        self.base
            .outcome
            .borrow_mut()
            .windows
            .push((label, self.since, now));
    }

    fn bar_index(&self, shell: &Shell) -> usize {
        shell
            .components
            .iter()
            .position(|runtime| runtime.surface_id == self.base.surface_ids["glass-bar"])
            .unwrap()
    }

    fn near_timer(&self, shell: &Shell, now: Instant) -> bool {
        shell.components[self.bar_index(shell)]
            .component
            .next_tick_deadline()
            .is_some_and(|at| at.saturating_duration_since(now) < Duration::from_millis(60))
    }

    fn key(&self, shell: &mut Shell, surface_id: Arc<str>, key: &str) {
        for event in [
            WindowKeyEvent::Pressed(key.into(), Default::default()),
            WindowKeyEvent::Released(key.into()),
        ] {
            shell.pending_wayland_events.push_back(WindowEvent::Key {
                surface_id: surface_id.clone(),
                event,
            });
        }
    }

    fn completed_since(&self, shell: &Shell, surface: &str, since: Instant) -> bool {
        self.base.records(shell).iter().any(|record| matches!(record,
            CapturedRecord::SceneCompletion {at, surface_id, presented: true} if surface_id == surface && *at >= since))
    }

    fn step(&mut self, shell: &mut Shell) -> ReplayStep {
        let now = Instant::now();
        if self.base.outcome.borrow().started.is_none() && !self.base.start(shell, now) {
            return ReplayStep { wake_at: None };
        }
        if now < self.next {
            return ReplayStep {
                wake_at: Some(self.next),
            };
        }
        if now - self.since > Duration::from_secs(45) {
            return self
                .base
                .fail(shell, "focused replay phase timed out".into());
        }
        match self.phase {
            FocusPhase::Boot if now - self.since >= BOOT_MIN => {
                if !shell.presentation_engine.gpu_canvas_available() {
                    return self.base.fail(
                        shell,
                        "focused capture requires the GPU scene worker".into(),
                    );
                }
                self.base.outcome.borrow_mut().gpu = true;
                self.enter(FocusPhase::Open(0), now);
            }
            FocusPhase::Open(index) if self.near_timer(shell, now) => {
                let trigger = ["glassCalendar", "glassControls"][index];
                let key = self
                    .base
                    .tree(shell, "glass-bar")
                    .and_then(|tree| first_node_with_ref(tree, trigger))
                    .and_then(|node| node.mesh_key())
                    .map(str::to_owned);
                let Some(key) = key else {
                    return self.base.fail(shell, format!("missing {trigger}"));
                };
                let bar = self.bar_index(shell);
                shell.components[bar].component.receive_focus_transfer(
                    &TabFocusTarget::AtKey(key),
                    None,
                    false,
                );
                self.key(shell, self.base.surface("glass-bar"), "Enter");
                self.enter(FocusPhase::Hold(index), now);
            }
            FocusPhase::Hold(index) if now - self.since >= Duration::from_millis(650) => {
                let bar = self.bar_index(shell);
                if shell.components[bar].children.is_empty() {
                    return self
                        .base
                        .fail(shell, "opening did not create a child popup".into());
                }
                let child = shell.components[bar].children[0].target.surface_id.as_str();
                if !self.completed_since(shell, child, self.since) {
                    return ReplayStep {
                        wake_at: Some(now + Duration::from_millis(10)),
                    };
                }
                self.window(["calendar_open", "controls_open"][index], now);
                self.enter(FocusPhase::Close(index), now);
            }
            FocusPhase::Close(index) if self.near_timer(shell, now) => {
                let bar = self.bar_index(shell);
                let child = shell.components[bar].children.iter().find(|child| {
                    child.kind == mesh_core_frontend_shell_adapter::ChildSurfaceKind::Popover
                });
                let Some(child) = child else {
                    return self
                        .base
                        .fail(shell, "popup disappeared before close".into());
                };
                self.key(shell, child.target.surface_id.as_str().into(), "Escape");
                self.enter(FocusPhase::Closed(index), now);
            }
            FocusPhase::Closed(index) if now - self.since >= Duration::from_millis(500) => {
                let bar = self.bar_index(shell);
                if !shell.components[bar].children.is_empty() {
                    return self.base.fail(shell, "popup did not finish closing".into());
                }
                self.window(["calendar_close", "controls_close"][index], now);
                self.enter(
                    if index == 0 {
                        FocusPhase::Open(1)
                    } else {
                        FocusPhase::Idle
                    },
                    now,
                );
            }
            FocusPhase::Idle if now - self.since >= Duration::from_millis(2200) => {
                self.window("clock_idle", now);
                self.base.show(shell, "glass-dashboard");
                self.enter(FocusPhase::Dashboard, now);
            }
            FocusPhase::Dashboard if now - self.since >= SETTLE_AFTER_OPEN => {
                if !self.completed_since(
                    shell,
                    &self.base.surface_ids["glass-dashboard"],
                    self.since,
                ) {
                    if now - self.since > OPEN_LIMIT {
                        return self.base.fail(shell, "dashboard did not commit".into());
                    }
                    return ReplayStep {
                        wake_at: Some(now + Duration::from_millis(10)),
                    };
                }
                self.enter(FocusPhase::Cpu, now);
            }
            FocusPhase::Cpu => {
                if self.counter == 6 {
                    self.window("dashboard_cpu", now);
                    self.enter(FocusPhase::Spectrum, now);
                } else {
                    let value = if self.counter % 2 == 0 { 20 } else { 80 };
                    if !self.base.inject(
                        shell,
                        "mesh.system",
                        serde_json::json!({
                            "available":true,"cpu":value,"cpu_history":vec![value;30],
                            "memory_used":20,"memory_total":100,"disk_used":20,"disk_total":100,
                            "temperature":40,"net_rx":0,"net_tx":0,"uptime":100,
                        }),
                    ) {
                        return ReplayStep { wake_at: None };
                    }
                    self.counter += 1;
                    self.next = now + Duration::from_millis(500);
                }
            }
            FocusPhase::Spectrum => {
                if self.counter == 30 {
                    self.window("dashboard_spectrum", now);
                    self.base.hide(shell, "glass-dashboard");
                    self.enter(FocusPhase::Finish, now);
                } else {
                    if !self.base.inject(shell, "mesh.visualizer", serde_json::json!({"available":true,"active":true,"bars":spectrum_bars(self.counter)})) {
                        return ReplayStep { wake_at: None };
                    }
                    self.counter += 1;
                    self.next = now + SPECTRUM_INTERVAL;
                }
            }
            FocusPhase::Finish if now - self.since >= Duration::from_millis(500) => {
                shell.enqueue_effects(std::iter::once(CoreRequest::Shutdown));
                return ReplayStep { wake_at: None };
            }
            _ => {}
        }
        ReplayStep {
            wake_at: Some(self.next.max(now + Duration::from_millis(10))),
        }
    }
}

#[test]
#[ignore = "live GPU compositor: popup lifecycle, timers, CPU and spectrum updates"]
fn live_gpu_popup_validation() {
    assert!(!cfg!(debug_assertions), "run with --release");
    assert!(cfg!(feature = "gpu"), "run with --features gpu");
    assert!(
        compositor_reachable(),
        "a reachable Wayland compositor is required"
    );
    let _sandbox = Sandbox::new();
    let mut shell = Shell::new();
    assert!(shell.presentation_engine.supports_blocking_dispatch());
    shell
        .presentation_engine
        .set_layer_namespace_suffix(REPLAY_NAMESPACE_SUFFIX);
    let now = Instant::now();
    let outcome = Rc::new(RefCell::new(Outcome::default()));
    let mut replay = FocusReplay {
        base: Replay {
            phase: Phase::Boot,
            phase_started: now,
            next_at: now,
            counter: 0,
            stage: 0,
            tiles: Vec::new(),
            scroll_point: (0.0, 0.0),
            surface_ids: BTreeMap::new(),
            outcome: Rc::clone(&outcome),
        },
        phase: FocusPhase::Boot,
        since: now,
        next: now,
        counter: 0,
    };
    shell.replay = Some(ReplayHarness {
        suppress_backends: true,
        driver: Box::new(move |shell| replay.step(shell)),
        wake_at: None,
    });
    shell.run().expect("focused replay must exit cleanly");
    let outcome = outcome.borrow();
    let records = shell.profiling.capture.as_deref().unwrap_or(&[]);
    let started = outcome.started.unwrap();
    let dir = super::super::root()
        .join("profiles/popup-live")
        .join(std::process::id().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let rows: Vec<_> = records.iter().map(|record| match record {
        CapturedRecord::Stage {at,surface_id,stage,micros,trigger} => serde_json::json!({"kind":"stage","ms":at.duration_since(started).as_secs_f64()*1000.0,"surface":surface_id,"stage":format!("{stage:?}"),"micros":micros,"trigger":trigger}),
        CapturedRecord::Invalidation {at,surface_id,node_visits} => serde_json::json!({"kind":"invalidation","ms":at.duration_since(started).as_secs_f64()*1000.0,"surface":surface_id,"visits":node_visits}),
        CapturedRecord::SceneCompletion {at,surface_id,presented} => serde_json::json!({"kind":"scene_completion","ms":at.duration_since(started).as_secs_f64()*1000.0,"surface":surface_id,"presented":presented}),
    }).collect();
    let windows: Vec<_> = outcome.windows.iter().map(|(name,from,to)| serde_json::json!({"name":name,"from_ms":from.duration_since(started).as_secs_f64()*1000.0,"to_ms":to.duration_since(started).as_secs_f64()*1000.0})).collect();
    std::fs::write(dir.join("capture.json"), serde_json::to_vec(&serde_json::json!({"render_path":shell.presentation_engine.render_path(),"windows":windows,"records":rows,"failure":outcome.failure})).unwrap()).unwrap();
    eprintln!("capture: {}", dir.display());
    assert!(outcome.failure.is_none(), "{:?}", outcome.failure);
    assert!(outcome.gpu);
    for (name, from, to) in &outcome.windows {
        report_window(records, name, *from, *to);
    }
    let completed = records
        .iter()
        .filter(|record| {
            matches!(
                record,
                CapturedRecord::SceneCompletion {
                    presented: true,
                    ..
                }
            )
        })
        .count();
    metric("live_focused_gpu_completed", completed as f64);
    assert!(completed > 0, "the GPU worker must commit frames");
    assert_eq!(outcome.windows.len(), 7);
}
