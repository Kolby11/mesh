use super::*;

#[test]
fn benchmark_snapshot_exposes_canonical_and_interaction_scenarios() {
    let mut shell = Shell::new();
    let snapshot = shell.build_debug_snapshot();

    assert_eq!(snapshot.benchmarks.scenarios.len(), 12);
    assert_eq!(
        snapshot
            .benchmarks
            .scenarios
            .iter()
            .map(|scenario| scenario.id.id())
            .collect::<Vec<_>>(),
        vec![
            "idle",
            "hover",
            "surface_open_close",
            "pointer_update",
            "text_update",
            "scroll",
            "icon_grid",
            "animation",
            "theme_reload",
            "resize",
            "keyboard_traversal",
            "backend_update",
        ]
    );
    assert_eq!(
        snapshot
            .benchmarks
            .scenarios
            .last()
            .map(|scenario| scenario.label.as_str()),
        Some("Backend-driven update")
    );
}

#[test]
fn benchmark_canonical_profiles_bind_expected_stages_and_surfaces() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_shell_profiling_stage(
        ProfilingStage::SchedulerIdle,
        std::time::Duration::from_micros(500),
        Some("timeout"),
    );
    for (surface, stage, micros) in [
        ("@mesh/settings", ProfilingStage::InputHandling, 11),
        ("@mesh/settings", ProfilingStage::TreeBuild, 22),
        ("@mesh/settings", ProfilingStage::Layout, 33),
        ("@mesh/debug-inspector", ProfilingStage::IconImageRaster, 44),
        ("@mesh/debug-inspector", ProfilingStage::Paint, 55),
        ("@mesh/navigation-bar", ProfilingStage::InputHandling, 60),
        ("@mesh/navigation-bar", ProfilingStage::StyleRestyle, 66),
        ("@mesh/navigation-bar", ProfilingStage::TreeBuild, 77),
        ("@mesh/navigation-bar", ProfilingStage::Layout, 88),
        ("@mesh/navigation-bar", ProfilingStage::Paint, 99),
    ] {
        shell.record_surface_profiling_stage(
            surface,
            Some(surface),
            stage,
            std::time::Duration::from_micros(micros),
            Some("canonical_profile"),
        );
    }

    let snapshot = shell.build_debug_snapshot();
    let scenario = |id: &str| {
        snapshot
            .benchmarks
            .scenarios
            .iter()
            .find(|scenario| scenario.id.id() == id)
            .expect("canonical scenario")
    };
    for id in [
        "idle",
        "pointer_update",
        "text_update",
        "scroll",
        "icon_grid",
        "animation",
        "theme_reload",
        "resize",
    ] {
        assert_eq!(
            scenario(id).status,
            mesh_core_debug::BenchmarkScenarioStatus::Complete,
            "{id} should resolve from its canonical stage samples"
        );
    }
    assert!(
        scenario("idle")
            .primary_metric
            .starts_with("scheduler_idle:")
    );
    assert!(
        scenario("text_update")
            .secondary_metric
            .starts_with("tree_build:")
    );
    assert!(
        scenario("icon_grid")
            .primary_metric
            .starts_with("icon_image_raster:")
    );
    assert!(scenario("resize").primary_metric.starts_with("layout:"));
}

#[test]
fn benchmark_payload_keeps_scenarios_inert_when_profiling_disabled() {
    let mut shell = Shell::new();
    let snapshot = shell.build_debug_snapshot();

    assert!(snapshot.profiling.is_none());
    assert!(
        !shell.debug.profiling_enabled,
        "building debug snapshots must not start profiling"
    );
    assert!(snapshot.benchmarks.scenarios.iter().all(|scenario| {
        scenario.status == mesh_core_debug::BenchmarkScenarioStatus::ProfilingOff
            && scenario.hint == "Start profiling first"
            && scenario.primary_metric == "No benchmark results yet"
            && scenario.secondary_metric == "No benchmark results yet"
    }));

    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug service state should include benchmark rows");
    assert!(latest.state["profiling"].is_null());
    let scenarios = latest.state["benchmarks"]["scenarios"]
        .as_array()
        .expect("benchmarks.scenarios should serialize as an array");
    assert_eq!(scenarios.len(), 12);
    assert!(scenarios.iter().all(|scenario| {
        scenario["status"] == serde_json::json!("Profiling off")
            && scenario["hint"] == serde_json::json!("Start profiling first")
    }));
}

#[test]
fn benchmark_payload_serializes_targets_statuses_and_metrics() {
    let mut shell = Shell::new();
    shell.build_debug_snapshot();

    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug service state should include benchmark payload");
    let scenarios = latest.state["benchmarks"]["scenarios"]
        .as_array()
        .expect("benchmarks.scenarios should serialize as an array");
    assert_eq!(scenarios.len(), 12);
    assert_eq!(
        scenarios
            .iter()
            .map(|scenario| scenario["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "idle",
            "hover",
            "surface_open_close",
            "pointer_update",
            "text_update",
            "scroll",
            "icon_grid",
            "animation",
            "theme_reload",
            "resize",
            "keyboard_traversal",
            "backend_update",
        ]
    );
    assert_eq!(
        scenarios
            .iter()
            .map(|scenario| scenario["target"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "shell scheduler",
            "@mesh/navigation-bar",
            "@mesh/audio-popover",
            "@mesh/navigation-bar audio controls",
            "@mesh/settings text controls",
            "@mesh/settings",
            "@mesh/debug-inspector",
            "@mesh/navigation-bar",
            "active theme + @mesh/navigation-bar",
            "@mesh/navigation-bar",
            "@mesh/navigation-bar focus chain",
            "active backend provider",
        ]
    );
    let backend_update = &scenarios[11];
    assert_eq!(
        backend_update["label"],
        serde_json::json!("Backend-driven update")
    );
    assert_eq!(backend_update["status"], serde_json::json!("Profiling off"));
    assert_eq!(
        backend_update["primary_metric"],
        serde_json::json!("No benchmark results yet")
    );
    assert_eq!(
        backend_update["secondary_metric"],
        serde_json::json!("No benchmark results yet")
    );
}

#[test]
fn benchmark_backend_update_correlates_backend_stage_with_surface_render_cost() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(31),
        Some("broadcast_service_event"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(45),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::Complete
    );
    assert_eq!(backend_update.target, "mesh.audio -> @mesh/pipewire-audio");
    assert!(backend_update.primary_metric.contains("mesh.audio"));
    assert!(
        backend_update
            .primary_metric
            .contains("@mesh/pipewire-audio")
    );
    assert!(
        backend_update
            .primary_metric
            .contains("state_publish_delivery")
    );
    assert!(
        backend_update
            .secondary_metric
            .contains("total_surface_render")
    );
    assert!(backend_update.secondary_metric.contains("45us"));

    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug service state should include benchmark payload");
    let scenarios = latest.state["benchmarks"]["scenarios"]
        .as_array()
        .expect("benchmarks.scenarios should serialize as an array");
    let payload = scenarios
        .iter()
        .find(|scenario| scenario["id"] == serde_json::json!("backend_update"))
        .expect("backend_update payload should serialize");
    assert_eq!(payload["status"], serde_json::json!("Complete"));
    assert_eq!(
        payload["target"],
        serde_json::json!("mesh.audio -> @mesh/pipewire-audio")
    );
    assert!(
        payload["primary_metric"]
            .as_str()
            .unwrap()
            .contains("state_publish_delivery")
    );
    assert!(
        payload["secondary_metric"]
            .as_str()
            .unwrap()
            .contains("frontend total_surface_render")
    );
}

#[test]
fn benchmark_backend_update_waits_when_surface_cost_is_missing() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::CommandHandling,
        std::time::Duration::from_micros(27),
        Some("service_command"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples
    );
    assert_eq!(
        backend_update.primary_metric,
        "Backend provider timing captured"
    );
    assert_eq!(
        backend_update.secondary_metric,
        "No frontend surface render samples yet"
    );
}

#[test]
fn benchmark_backend_update_waits_when_backend_stage_is_missing() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );

    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(53),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples
    );
    assert_eq!(backend_update.target, "mesh.audio -> @mesh/pipewire-audio");
    assert_eq!(
        backend_update.primary_metric,
        "No backend provider samples yet"
    );
    assert_eq!(
        backend_update.secondary_metric,
        "frontend total_surface_render: 53us"
    );
}

#[test]
fn benchmark_service_event_maps_to_run_request() {
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new("service.debug.read"));
    let requests = script_events_to_requests(vec![PublishedEvent {
        channel: "shell.run-debug-benchmark".into(),
        payload: serde_json::json!({ "scenario_id": "hover" }),
        source_module_id: "@mesh/debug-inspector".into(),
        source_capabilities: capabilities,
    }]);

    match requests.as_slice() {
        [CoreRequest::RunDebugBenchmark { scenario_id }] => {
            assert_eq!(scenario_id, "hover");
        }
        other => panic!("expected RunDebugBenchmark request, got {other:?}"),
    }
}

#[test]
fn benchmark_ipc_command_maps_to_run_request() {
    match parse_ipc_command("shell:debug_benchmark:pointer_update") {
        Some(CoreRequest::RunDebugBenchmark { scenario_id }) => {
            assert_eq!(scenario_id, "pointer_update");
        }
        other => panic!("expected RunDebugBenchmark request, got {other:?}"),
    }
}

#[test]
fn benchmark_run_request_does_not_enable_profiling() {
    let mut shell = Shell::new();

    let emitted = shell
        .apply_request(CoreRequest::RunDebugBenchmark {
            scenario_id: "surface_open_close".into(),
        })
        .unwrap();

    assert!(
        !shell.debug.profiling_enabled,
        "benchmark requests must not enable profiling"
    );
    assert_eq!(
        shell
            .debug
            .latest_benchmark_run
            .as_ref()
            .map(|run| run.scenario_id.id()),
        Some("surface_open_close")
    );
    assert_eq!(emitted.len(), 1);
    assert!(matches!(
        &emitted[0],
        CoreRequest::ToggleSurface { surface_id } if surface_id == "@mesh/audio-popover"
    ));

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    assert!(shell.debug.profiling_enabled);
    shell
        .apply_request(CoreRequest::RunDebugBenchmark {
            scenario_id: "keyboard_traversal".into(),
        })
        .unwrap();
    assert!(
        shell.debug.profiling_enabled,
        "benchmark requests must preserve existing profiling state"
    );
    assert_eq!(
        shell
            .debug
            .latest_benchmark_run
            .as_ref()
            .map(|run| run.scenario_id.id()),
        Some("keyboard_traversal")
    );
}

#[test]
fn benchmark_backend_update_uses_the_active_provider_without_interface_branches() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_backend_runtime_status(
        "mesh.network".to_string(),
        "@mesh/networkmanager".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );

    shell.record_backend_profiling_stage(
        "mesh.network",
        "@mesh/networkmanager",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(29),
        Some("broadcast_service_event"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(41),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::Complete
    );
    assert_eq!(
        backend_update.target,
        "mesh.network -> @mesh/networkmanager"
    );
    assert!(
        backend_update
            .primary_metric
            .contains("@mesh/networkmanager")
    );
    assert!(
        backend_update
            .secondary_metric
            .contains("total_surface_render")
    );
}

#[test]
fn benchmark_backend_update_ignores_terminal_runtime_when_running_provider_exists() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Stopped,
        "runtime stopped".to_string(),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pulseaudio-audio".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(99),
        Some("stale_service_update"),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulseaudio-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(29),
        Some("broadcast_service_event"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(41),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.target,
        "mesh.audio -> @mesh/pulseaudio-audio"
    );
    assert!(
        backend_update
            .primary_metric
            .contains("@mesh/pulseaudio-audio")
    );
    assert!(
        !backend_update
            .primary_metric
            .contains("@mesh/pipewire-audio")
    );
}

#[test]
fn benchmark_backend_update_reports_unavailable_for_failed_only_runtime() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Failed,
        "runtime failed".to_string(),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::Unavailable
    );
    assert_eq!(
        backend_update.primary_metric,
        "No backend provider samples yet"
    );
}

#[test]
fn phase18_baseline_ranks_hotspots_by_absolute_latency() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(142),
        Some("phase18_fresh_baseline"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(77),
        Some("phase18_fresh_baseline"),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "phase18 backend baseline".to_string(),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(109),
        Some("phase18_fresh_baseline"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(88),
        Some("phase18_backend_visible_frontend"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot
        .profiling
        .as_ref()
        .expect("profiling should be enabled for phase 18 baseline");
    let navigation_bar = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("navigation bar surface sample should be recorded");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("backend sample should be recorded");
    let backend_visible_frontend = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/audio-popover")
        .expect("backend-driven frontend surface sample should be recorded");

    let nav_render = navigation_bar
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingStage::TotalSurfaceRender)
        .expect("navigation bar render stage should be recorded");
    let nav_paint = navigation_bar
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingStage::Paint)
        .expect("navigation bar paint stage should be recorded");
    let backend_publish = backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::StatePublishDelivery)
        .expect("backend publish stage should be recorded");

    let mut candidates = [
        (
            "surface_render:@mesh/navigation-bar",
            nav_render.max_micros,
            true,
        ),
        ("paint:@mesh/navigation-bar", nav_paint.max_micros, true),
        (
            "backend_publish:@mesh/pipewire-audio",
            backend_publish.max_micros,
            backend_visible_frontend.total_surface_render_time_micros > 0,
        ),
    ];
    candidates.sort_by(|left, right| right.1.cmp(&left.1));

    assert_eq!(candidates[0].0, "surface_render:@mesh/navigation-bar");
    assert_eq!(candidates[0].1, 142);
    assert!(
        candidates.iter().any(
            |(name, value, eligible)| name.starts_with("backend_publish")
                && *value == 109
                && *eligible
        ),
        "backend candidate should remain eligible only when frontend impact is visible"
    );
}

#[test]
fn phase18_benchmark_payload_preserves_render_visible_contract_after_lookup_cache() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(21),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::StyleRestyle,
        std::time::Duration::from_micros(34),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(55),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(67),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(142),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(88),
        Some("phase18_contract"),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "phase18 backend contract".to_string(),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(109),
        Some("phase18_contract"),
    );

    let snapshot = shell.build_debug_snapshot();
    let scenario_ids: Vec<_> = snapshot
        .benchmarks
        .scenarios
        .iter()
        .map(|scenario| scenario.id.id())
        .collect();

    assert_eq!(
        scenario_ids,
        [
            "idle",
            "hover",
            "surface_open_close",
            "pointer_update",
            "text_update",
            "scroll",
            "icon_grid",
            "animation",
            "theme_reload",
            "resize",
            "keyboard_traversal",
            "backend_update"
        ]
    );

    let navigation_rows: Vec<_> = snapshot
        .benchmarks
        .scenarios
        .iter()
        .filter(|scenario| scenario.target.contains("@mesh/navigation-bar"))
        .collect();
    assert_eq!(navigation_rows.len(), 6);
    assert!(
        navigation_rows.iter().all(|scenario| {
            scenario.status == mesh_core_debug::BenchmarkScenarioStatus::Complete
        })
    );
    assert!(navigation_rows.iter().any(|scenario| {
        scenario.id.id() == "keyboard_traversal" && scenario.secondary_metric.contains("142us")
    }));

    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");
    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::Complete
    );
    assert_eq!(backend_update.target, "mesh.audio -> @mesh/pipewire-audio");
    assert!(
        backend_update
            .primary_metric
            .contains("@mesh/pipewire-audio")
    );
    assert!(backend_update.secondary_metric.contains("142us"));
}

#[test]
fn benchmark_run_request_rejects_unknown_scenario() {
    let mut shell = Shell::new();

    let emitted = shell
        .apply_request(CoreRequest::RunDebugBenchmark {
            scenario_id: "not_a_scenario".into(),
        })
        .unwrap();

    assert!(
        !shell.debug.profiling_enabled,
        "rejected benchmark requests must not enable profiling"
    );
    assert!(shell.debug.latest_benchmark_run.is_none());
    match emitted.as_slices().0 {
        [CoreRequest::PublishDiagnostics { message }] => {
            assert!(message.contains("unknown debug benchmark scenario"));
            assert!(message.contains("not_a_scenario"));
        }
        other => panic!("expected diagnostic for unknown benchmark scenario, got {other:?}"),
    }
}
