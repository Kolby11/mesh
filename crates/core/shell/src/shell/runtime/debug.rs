use super::super::*;

impl Shell {
    pub(in crate::shell) fn build_debug_snapshot(&mut self) -> DebugSnapshot {
        let modules = self
            .modules
            .values()
            .map(|inst| ModuleEntry {
                id: inst.manifest.package.id.clone(),
                module_type: format!("{:?}", inst.manifest.package.module_type).to_lowercase(),
                state: inst.state.to_string(),
                error_count: inst.error_count,
                last_error: inst.last_error.clone(),
            })
            .collect();

        let catalog = self.interfaces.catalog();
        let mut interfaces: Vec<InterfaceEntry> = catalog
            .providers
            .iter()
            .map(|(name, providers)| {
                let providers = providers
                    .iter()
                    .map(|p| ProviderEntry {
                        backend_name: p.backend_name.clone(),
                        priority: p.priority,
                    })
                    .collect();
                InterfaceEntry {
                    name: name.clone(),
                    providers,
                }
            })
            .collect();
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));

        let health = self
            .diagnostics
            .snapshot()
            .into_iter()
            .map(|(id, status)| HealthEntry {
                module_id: id,
                status: status.to_string(),
            })
            .collect();

        let mut backend_runtimes: Vec<BackendRuntimeEntry> = self
            .backend_runtime_statuses
            .values()
            .map(|entry| BackendRuntimeEntry {
                interface: entry.interface.clone(),
                provider_id: entry.provider_id.clone(),
                status: entry.status.as_str().to_string(),
                message: entry.message.clone(),
                failure_count: entry.failure_count,
            })
            .collect();
        backend_runtimes.sort_by(|a, b| {
            a.interface
                .cmp(&b.interface)
                .then_with(|| a.provider_id.cmp(&b.provider_id))
        });

        let active_surfaces: Vec<String> = self
            .core
            .surfaces
            .iter()
            .filter(|(_, s)| s.visible)
            .map(|(id, _)| id.clone())
            .collect();

        let profiling = self.debug.profiling_enabled.then(|| {
            let mut profiling = self.profiling.snapshot(self.debug.profiling_session_id);
            profiling
                .surfaces
                .sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
            profiling.backends.sort_by(|a, b| {
                a.interface
                    .cmp(&b.interface)
                    .then_with(|| a.provider_id.cmp(&b.provider_id))
            });
            profiling
        });
        let benchmarks = benchmark_snapshot(
            &self.debug,
            profiling.as_ref(),
            &active_surfaces,
            &backend_runtimes,
        );

        let snapshot = DebugSnapshot {
            modules,
            interfaces,
            backend_runtimes,
            health,
            active_surfaces,
            benchmarks,
            profiling,
        };

        self.latest_service_state.insert(
            mesh_core_debug::DEBUG_INTERFACE.to_string(),
            LatestServiceState {
                interface: mesh_core_debug::DEBUG_INTERFACE.to_string(),
                provider_id: mesh_core_debug::DEBUG_SOURCE_MODULE_ID.to_string(),
                state: debug_service_payload(&self.debug, &snapshot),
            },
        );

        snapshot
    }
}

fn debug_service_payload(
    debug: &mesh_core_debug::DebugOverlayState,
    snapshot: &DebugSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "overlay_enabled": debug.enabled,
        "profiling_enabled": debug.profiling_enabled,
        "profiling_session_id": debug.profiling_session_id,
        "active_view": debug.active_view.label(),
        "modules": snapshot.modules.iter().map(module_entry_json).collect::<Vec<_>>(),
        "interfaces": snapshot.interfaces.iter().map(interface_entry_json).collect::<Vec<_>>(),
        "backend_runtimes": snapshot.backend_runtimes.iter().map(backend_runtime_entry_json).collect::<Vec<_>>(),
        "active_surfaces": snapshot.active_surfaces.clone(),
        "benchmarks": benchmark_snapshot_json(&snapshot.benchmarks),
        "profiling": snapshot.profiling.as_ref().map(profiling_snapshot_json),
    })
}

fn benchmark_snapshot(
    debug: &mesh_core_debug::DebugOverlayState,
    profiling: Option<&mesh_core_debug::ProfilingSnapshot>,
    _active_surfaces: &[String],
    _backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
) -> mesh_core_debug::DebugBenchmarkSnapshot {
    use mesh_core_debug::BenchmarkScenarioId;

    let scenarios = [
        BenchmarkScenarioId::Hover,
        BenchmarkScenarioId::SurfaceOpenClose,
        BenchmarkScenarioId::PointerUpdate,
        BenchmarkScenarioId::KeyboardTraversal,
        BenchmarkScenarioId::BackendUpdate,
    ]
    .into_iter()
    .map(|id| {
        benchmark_scenario_snapshot(
            id,
            debug.profiling_enabled,
            profiling,
            debug.latest_benchmark_run.as_ref(),
        )
    })
    .collect();

    mesh_core_debug::DebugBenchmarkSnapshot { scenarios }
}

fn benchmark_scenario_snapshot(
    id: mesh_core_debug::BenchmarkScenarioId,
    profiling_enabled: bool,
    profiling: Option<&mesh_core_debug::ProfilingSnapshot>,
    latest_run: Option<&mesh_core_debug::DebugBenchmarkRunState>,
) -> mesh_core_debug::BenchmarkScenarioSnapshot {
    let target = benchmark_target(id);
    let (status, primary_metric, secondary_metric, hint) = if !profiling_enabled {
        (
            mesh_core_debug::BenchmarkScenarioStatus::ProfilingOff,
            "No benchmark results yet".to_string(),
            "No benchmark results yet".to_string(),
            "Start profiling first".to_string(),
        )
    } else if let Some(profiling) = profiling {
        let metrics = benchmark_metrics(id, profiling);
        if metrics.0 == mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples {
            benchmark_pending_state(id, latest_run)
        } else {
            metrics
        }
    } else {
        benchmark_pending_state(id, latest_run)
    };

    mesh_core_debug::BenchmarkScenarioSnapshot {
        id,
        label: id.label().to_string(),
        target: target.to_string(),
        status,
        primary_metric,
        secondary_metric,
        hint,
    }
}

fn benchmark_pending_state(
    id: mesh_core_debug::BenchmarkScenarioId,
    latest_run: Option<&mesh_core_debug::DebugBenchmarkRunState>,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    if let Some(latest_run) = latest_run
        && latest_run.scenario_id == id
    {
        return (
            latest_run.status,
            "No benchmark results yet".to_string(),
            "No benchmark results yet".to_string(),
            "Benchmark requested; waiting for profiling samples".to_string(),
        );
    }

    (
        mesh_core_debug::BenchmarkScenarioStatus::Ready,
        "No benchmark results yet".to_string(),
        "No benchmark results yet".to_string(),
        "Run this scenario while profiling is live".to_string(),
    )
}

fn benchmark_target(id: mesh_core_debug::BenchmarkScenarioId) -> &'static str {
    match id {
        mesh_core_debug::BenchmarkScenarioId::Hover => "@mesh/navigation-bar",
        mesh_core_debug::BenchmarkScenarioId::SurfaceOpenClose => "@mesh/audio-popover",
        mesh_core_debug::BenchmarkScenarioId::PointerUpdate => {
            "@mesh/navigation-bar audio controls"
        }
        mesh_core_debug::BenchmarkScenarioId::KeyboardTraversal => {
            "@mesh/navigation-bar focus chain"
        }
        mesh_core_debug::BenchmarkScenarioId::BackendUpdate => "mesh.audio -> @mesh/pipewire-audio",
    }
}

fn benchmark_metrics(
    id: mesh_core_debug::BenchmarkScenarioId,
    profiling: &mesh_core_debug::ProfilingSnapshot,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    match id {
        mesh_core_debug::BenchmarkScenarioId::Hover => surface_benchmark_metrics(
            profiling,
            "@mesh/navigation-bar",
            &[
                mesh_core_debug::ProfilingStage::InputHandling,
                mesh_core_debug::ProfilingStage::StyleRestyle,
            ],
            &[
                mesh_core_debug::ProfilingStage::StyleRestyle,
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
            ],
            "Interact with @mesh/navigation-bar while profiling is live",
        ),
        mesh_core_debug::BenchmarkScenarioId::SurfaceOpenClose => surface_render_benchmark_metrics(
            profiling,
            "@mesh/audio-popover",
            "Open and close @mesh/audio-popover while profiling is live",
        ),
        mesh_core_debug::BenchmarkScenarioId::PointerUpdate => surface_benchmark_metrics(
            profiling,
            "@mesh/navigation-bar",
            &[
                mesh_core_debug::ProfilingStage::InputHandling,
                mesh_core_debug::ProfilingStage::RuntimeUpdateHandling,
            ],
            &[
                mesh_core_debug::ProfilingStage::Layout,
                mesh_core_debug::ProfilingStage::Paint,
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
            ],
            "Adjust the navigation-bar audio controls while profiling is live",
        ),
        mesh_core_debug::BenchmarkScenarioId::KeyboardTraversal => surface_benchmark_metrics(
            profiling,
            "@mesh/navigation-bar",
            &[
                mesh_core_debug::ProfilingStage::InputHandling,
                mesh_core_debug::ProfilingStage::RuntimeUpdateHandling,
            ],
            &[
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
                mesh_core_debug::ProfilingStage::Paint,
            ],
            "Move focus through @mesh/navigation-bar while profiling is live",
        ),
        mesh_core_debug::BenchmarkScenarioId::BackendUpdate => {
            backend_update_benchmark_metrics(profiling)
        }
    }
}

fn surface_benchmark_metrics(
    profiling: &mesh_core_debug::ProfilingSnapshot,
    surface_id: &str,
    primary_stages: &[mesh_core_debug::ProfilingStage],
    secondary_stages: &[mesh_core_debug::ProfilingStage],
    hint: &str,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    let Some(surface) = profiling_surface(profiling, surface_id) else {
        return waiting_for_samples();
    };
    let primary = first_surface_stage(surface, primary_stages);
    let secondary = first_surface_stage(surface, secondary_stages);
    match (primary, secondary) {
        (Some(primary), secondary) => (
            mesh_core_debug::BenchmarkScenarioStatus::Complete,
            profiling_stage_metric(primary),
            secondary
                .map(profiling_stage_metric)
                .unwrap_or_else(|| "No benchmark results yet".to_string()),
            hint.to_string(),
        ),
        _ => waiting_for_samples(),
    }
}

fn surface_render_benchmark_metrics(
    profiling: &mesh_core_debug::ProfilingSnapshot,
    surface_id: &str,
    hint: &str,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    let Some(surface) = profiling_surface(profiling, surface_id) else {
        return waiting_for_samples();
    };
    if surface.total_surface_render_time_micros == 0 && surface.redraw_count == 0 {
        return waiting_for_samples();
    }
    (
        mesh_core_debug::BenchmarkScenarioStatus::Complete,
        format!(
            "total_surface_render: {}us",
            surface.total_surface_render_time_micros
        ),
        format!("redraw_count: {}", surface.redraw_count),
        hint.to_string(),
    )
}

fn backend_update_benchmark_metrics(
    profiling: &mesh_core_debug::ProfilingSnapshot,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    let backend = profiling.backends.iter().find(|backend| {
        backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
    });
    let frontend = profiling_surface(profiling, "@mesh/navigation-bar")
        .or_else(|| profiling_surface(profiling, "@mesh/audio-popover"));
    let primary = backend.and_then(|backend| {
        first_backend_stage(
            backend,
            &[
                mesh_core_debug::ProfilingBackendStage::PollUpdate,
                mesh_core_debug::ProfilingBackendStage::StatePublishDelivery,
                mesh_core_debug::ProfilingBackendStage::CommandHandling,
            ],
        )
    });

    match primary {
        Some(primary) => (
            mesh_core_debug::BenchmarkScenarioStatus::Complete,
            profiling_backend_stage_metric(primary),
            frontend
                .map(|surface| {
                    format!(
                        "frontend total_surface_render: {}us",
                        surface.total_surface_render_time_micros
                    )
                })
                .unwrap_or_else(|| "No benchmark results yet".to_string()),
            "Update mesh.audio while profiling is live".to_string(),
        ),
        None => waiting_for_samples(),
    }
}

fn waiting_for_samples() -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    (
        mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples,
        "No benchmark results yet".to_string(),
        "No benchmark results yet".to_string(),
        "Run or interact with this scenario while profiling is live".to_string(),
    )
}

fn profiling_surface<'a>(
    profiling: &'a mesh_core_debug::ProfilingSnapshot,
    surface_id: &str,
) -> Option<&'a mesh_core_debug::ProfilingSurfaceSnapshot> {
    profiling.surfaces.iter().find(|surface| {
        surface.surface_id == surface_id || surface.module_id.as_deref() == Some(surface_id)
    })
}

fn first_surface_stage<'a>(
    surface: &'a mesh_core_debug::ProfilingSurfaceSnapshot,
    stages: &[mesh_core_debug::ProfilingStage],
) -> Option<&'a mesh_core_debug::ProfilingStageSummary> {
    stages.iter().find_map(|stage| {
        surface
            .stages
            .iter()
            .find(|summary| summary.stage == *stage && summary.sample_count > 0)
    })
}

fn first_backend_stage<'a>(
    backend: &'a mesh_core_debug::ProfilingBackendSnapshot,
    stages: &[mesh_core_debug::ProfilingBackendStage],
) -> Option<&'a mesh_core_debug::ProfilingBackendStageSummary> {
    stages.iter().find_map(|stage| {
        backend
            .stages
            .iter()
            .find(|summary| summary.stage == *stage && summary.sample_count > 0)
    })
}

fn profiling_stage_metric(summary: &mesh_core_debug::ProfilingStageSummary) -> String {
    format!(
        "{}: {} samples, max {}us",
        summary.stage.label(),
        summary.sample_count,
        summary.max_micros
    )
}

fn profiling_backend_stage_metric(
    summary: &mesh_core_debug::ProfilingBackendStageSummary,
) -> String {
    format!(
        "{}: {} samples, max {}us",
        summary.stage.label(),
        summary.sample_count,
        summary.max_micros
    )
}

fn benchmark_snapshot_json(
    snapshot: &mesh_core_debug::DebugBenchmarkSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "scenarios": snapshot.scenarios.iter().map(benchmark_scenario_json).collect::<Vec<_>>(),
    })
}

fn benchmark_scenario_json(
    scenario: &mesh_core_debug::BenchmarkScenarioSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "id": scenario.id.id(),
        "label": scenario.label,
        "target": scenario.target,
        "status": scenario.status.label(),
        "primary_metric": scenario.primary_metric,
        "secondary_metric": scenario.secondary_metric,
        "hint": scenario.hint,
    })
}

fn module_entry_json(entry: &ModuleEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "module_type": entry.module_type,
        "state": entry.state,
        "error_count": entry.error_count,
        "last_error": entry.last_error,
    })
}

fn interface_entry_json(entry: &InterfaceEntry) -> serde_json::Value {
    serde_json::json!({
        "name": entry.name,
        "providers": entry.providers.iter().map(provider_entry_json).collect::<Vec<_>>(),
    })
}

fn provider_entry_json(entry: &ProviderEntry) -> serde_json::Value {
    serde_json::json!({
        "backend_name": entry.backend_name,
        "priority": entry.priority,
    })
}

fn backend_runtime_entry_json(entry: &BackendRuntimeEntry) -> serde_json::Value {
    serde_json::json!({
        "interface": entry.interface,
        "provider_id": entry.provider_id,
        "status": entry.status,
        "message": entry.message,
        "failure_count": entry.failure_count,
    })
}

fn profiling_snapshot_json(snapshot: &mesh_core_debug::ProfilingSnapshot) -> serde_json::Value {
    serde_json::json!({
        "session_id": snapshot.session_id,
        "shell": profiling_scope_snapshot_json(&snapshot.shell),
        "surfaces": snapshot.surfaces.iter().map(profiling_surface_snapshot_json).collect::<Vec<_>>(),
        "backends": snapshot.backends.iter().map(profiling_backend_snapshot_json).collect::<Vec<_>>(),
    })
}

fn profiling_scope_snapshot_json(
    snapshot: &mesh_core_debug::ProfilingScopeSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "stages": snapshot.stages.iter().map(profiling_stage_summary_json).collect::<Vec<_>>(),
        "redraw_count": snapshot.redraw_count,
        "total_surface_render_time_micros": snapshot.total_surface_render_time_micros,
    })
}

fn profiling_surface_snapshot_json(
    snapshot: &mesh_core_debug::ProfilingSurfaceSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "surface_id": snapshot.surface_id,
        "module_id": snapshot.module_id,
        "stages": snapshot.stages.iter().map(profiling_stage_summary_json).collect::<Vec<_>>(),
        "redraw_count": snapshot.redraw_count,
        "total_surface_render_time_micros": snapshot.total_surface_render_time_micros,
    })
}

fn profiling_stage_summary_json(
    summary: &mesh_core_debug::ProfilingStageSummary,
) -> serde_json::Value {
    serde_json::json!({
        "stage": summary.stage.label(),
        "sample_count": summary.sample_count,
        "total_micros": summary.total_micros,
        "max_micros": summary.max_micros,
        "recent_samples": summary.recent_samples.iter().map(profiling_sample_json).collect::<Vec<_>>(),
    })
}

fn profiling_sample_json(sample: &mesh_core_debug::ProfilingSample) -> serde_json::Value {
    serde_json::json!({
        "stage": sample.stage.label(),
        "order": sample.order,
        "duration_micros": sample.duration_micros,
        "surface_id": sample.surface_id,
        "module_id": sample.module_id,
        "redraw_count": sample.redraw_count,
        "trigger_kind": sample.trigger_kind,
    })
}

fn profiling_backend_snapshot_json(
    snapshot: &mesh_core_debug::ProfilingBackendSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "interface": snapshot.interface,
        "provider_id": snapshot.provider_id,
        "stages": snapshot.stages.iter().map(profiling_backend_stage_summary_json).collect::<Vec<_>>(),
    })
}

fn profiling_backend_stage_summary_json(
    summary: &mesh_core_debug::ProfilingBackendStageSummary,
) -> serde_json::Value {
    serde_json::json!({
        "stage": summary.stage.label(),
        "sample_count": summary.sample_count,
        "total_micros": summary.total_micros,
        "max_micros": summary.max_micros,
        "recent_samples": summary.recent_samples.iter().map(profiling_backend_sample_json).collect::<Vec<_>>(),
    })
}

fn profiling_backend_sample_json(
    sample: &mesh_core_debug::ProfilingBackendSample,
) -> serde_json::Value {
    serde_json::json!({
        "stage": sample.stage.label(),
        "order": sample.order,
        "duration_micros": sample.duration_micros,
        "trigger_kind": sample.trigger_kind,
    })
}
