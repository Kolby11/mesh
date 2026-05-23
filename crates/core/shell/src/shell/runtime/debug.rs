use super::super::*;

impl Shell {
    pub(in crate::shell) fn build_debug_snapshot(&mut self) -> DebugSnapshot {
        let snapshot = self.debug_snapshot();
        self.record_debug_snapshot_state(&snapshot);
        snapshot
    }

    pub(in crate::shell) fn publish_debug_snapshot(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let snapshot = self.debug_snapshot();
        self.broadcast_service_event(self.debug_snapshot_event(&snapshot))
    }

    fn debug_snapshot(&mut self) -> DebugSnapshot {
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

        let module_instances = self.module_object_entries();

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

        let mut keybinds = self
            .components
            .iter()
            .flat_map(|runtime| runtime.component.debug_keybinds())
            .collect::<Vec<_>>();
        keybinds.sort_by(|left, right| {
            left.surface_id
                .cmp(&right.surface_id)
                .then_with(|| left.action_id.cmp(&right.action_id))
        });

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

        DebugSnapshot {
            modules,
            module_instances,
            interfaces,
            backend_runtimes,
            health,
            keybinds,
            active_surfaces,
            benchmarks,
            profiling,
        }
    }

    fn record_debug_snapshot_state(&mut self, snapshot: &DebugSnapshot) {
        self.latest_service_state.insert(
            mesh_core_debug::DEBUG_INTERFACE.to_string(),
            LatestServiceState {
                interface: mesh_core_debug::DEBUG_INTERFACE.to_string(),
                provider_id: mesh_core_debug::DEBUG_SOURCE_MODULE_ID.to_string(),
                state: debug_service_payload(&self.debug, snapshot),
            },
        );
    }

    fn debug_snapshot_event(&self, snapshot: &DebugSnapshot) -> ServiceEvent {
        ServiceEvent::Updated {
            service: mesh_core_debug::DEBUG_INTERFACE.to_string(),
            source_module: mesh_core_debug::DEBUG_SOURCE_MODULE_ID.to_string(),
            payload: debug_service_payload(&self.debug, snapshot),
        }
    }
}

fn debug_service_payload(
    debug: &mesh_core_debug::DebugOverlayState,
    snapshot: &DebugSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "overlay_enabled": debug.enabled,
        "layout_bounds_enabled": debug.show_layout_bounds,
        "profiling_enabled": debug.profiling_enabled,
        "profiling_session_id": debug.profiling_session_id,
        "active_view": debug.active_view.label(),
        "modules": snapshot.modules.iter().map(module_entry_json).collect::<Vec<_>>(),
        "module_instances": snapshot.module_instances.iter().map(module_object_entry_json).collect::<Vec<_>>(),
        "interfaces": snapshot.interfaces.iter().map(interface_entry_json).collect::<Vec<_>>(),
        "backend_runtimes": snapshot.backend_runtimes.iter().map(backend_runtime_entry_json).collect::<Vec<_>>(),
        "health": snapshot.health.iter().map(health_entry_json).collect::<Vec<_>>(),
        "keybinds": snapshot.keybinds.iter().map(keybind_entry_json).collect::<Vec<_>>(),
        "active_surfaces": snapshot.active_surfaces.clone(),
        "benchmarks": benchmark_snapshot_json(&snapshot.benchmarks),
        "profiling": snapshot.profiling.as_ref().map(profiling_snapshot_json),
    })
}

impl Shell {
    fn module_object_entries(&self) -> Vec<mesh_core_debug::ModuleObjectEntry> {
        let mut entries = Vec::new();

        for module in self.modules.values() {
            let module_id = module.manifest.package.id.clone();
            let kind = format!("{:?}", module.manifest.package.module_type).to_lowercase();
            entries.push(mesh_core_debug::ModuleObjectEntry {
                instance_id: module_id.clone(),
                module_id,
                object_kind: kind,
                interface: None,
                version: Some(module.manifest.package.version.clone()),
                lifecycle: module.state.to_string(),
                active: true,
                capabilities: module.manifest.capabilities.required.clone(),
            });
        }

        for runtime in &self.components {
            let module_id = runtime.component.id().to_string();
            entries.push(mesh_core_debug::ModuleObjectEntry {
                instance_id: runtime.surface_id.clone(),
                module_id,
                object_kind: "frontend".to_string(),
                interface: None,
                version: self
                    .modules
                    .get(runtime.component.id())
                    .map(|module| module.manifest.package.version.clone()),
                lifecycle: self
                    .modules
                    .get(runtime.component.id())
                    .map(|module| module.state.to_string())
                    .unwrap_or_else(|| "mounted".to_string()),
                active: self
                    .core
                    .surfaces
                    .get(&runtime.surface_id)
                    .map(|surface| surface.visible)
                    .unwrap_or(true),
                capabilities: self
                    .modules
                    .get(runtime.component.id())
                    .map(|module| module.manifest.capabilities.required.clone())
                    .unwrap_or_default(),
            });
        }

        for (interface, providers) in self.interfaces.catalog().providers {
            for provider in providers {
                let active = self
                    .backend_runtimes
                    .get(&interface)
                    .is_some_and(|slot| slot.provider_id == provider.provider_module);
                let lifecycle = self
                    .backend_runtime_statuses
                    .get(&(interface.clone(), provider.provider_module.clone()))
                    .map(|status| status.status.as_str().to_string())
                    .unwrap_or_else(|| "registered".to_string());
                entries.push(mesh_core_debug::ModuleObjectEntry {
                    instance_id: format!("{}:{}", interface, provider.provider_module),
                    module_id: provider.provider_module,
                    object_kind: "backend".to_string(),
                    interface: Some(interface.clone()),
                    version: provider.version,
                    lifecycle,
                    active,
                    capabilities: Vec::new(),
                });
            }
        }

        entries.sort_by(|left, right| {
            left.object_kind
                .cmp(&right.object_kind)
                .then_with(|| left.instance_id.cmp(&right.instance_id))
        });
        entries
    }
}

fn benchmark_snapshot(
    debug: &mesh_core_debug::DebugOverlayState,
    profiling: Option<&mesh_core_debug::ProfilingSnapshot>,
    _active_surfaces: &[String],
    backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
) -> mesh_core_debug::DebugBenchmarkSnapshot {
    use mesh_core_debug::BenchmarkScenarioId;

    let profiling_view =
        profiling.map(|profiling| BenchmarkProfilingView::new(profiling, backend_runtimes));
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
            profiling_view.as_ref(),
            backend_runtimes,
            debug.latest_benchmark_run.as_ref(),
        )
    })
    .collect();

    mesh_core_debug::DebugBenchmarkSnapshot { scenarios }
}

fn benchmark_scenario_snapshot(
    id: mesh_core_debug::BenchmarkScenarioId,
    profiling_enabled: bool,
    profiling_view: Option<&BenchmarkProfilingView<'_>>,
    backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
    latest_run: Option<&mesh_core_debug::DebugBenchmarkRunState>,
) -> mesh_core_debug::BenchmarkScenarioSnapshot {
    let target = benchmark_target(id, profiling_view, backend_runtimes);
    let (status, primary_metric, secondary_metric, hint) = if !profiling_enabled {
        (
            mesh_core_debug::BenchmarkScenarioStatus::ProfilingOff,
            "No benchmark results yet".to_string(),
            "No benchmark results yet".to_string(),
            "Start profiling first".to_string(),
        )
    } else if let Some(profiling_view) = profiling_view {
        let metrics = benchmark_metrics(id, profiling_view);
        if metrics.0 == mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples
            && id != mesh_core_debug::BenchmarkScenarioId::BackendUpdate
        {
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

struct BenchmarkProfilingView<'a> {
    navigation_bar: Option<&'a mesh_core_debug::ProfilingSurfaceSnapshot>,
    audio_popover: Option<&'a mesh_core_debug::ProfilingSurfaceSnapshot>,
    backend_update_backend: Option<&'a mesh_core_debug::ProfilingBackendSnapshot>,
    backend_runtime_available: bool,
}

impl<'a> BenchmarkProfilingView<'a> {
    fn new(
        profiling: &'a mesh_core_debug::ProfilingSnapshot,
        backend_runtimes: &'a [mesh_core_debug::BackendRuntimeEntry],
    ) -> Self {
        let navigation_bar = profiling_surface(profiling, "@mesh/navigation-bar");
        let audio_popover = profiling_surface(profiling, "@mesh/audio-popover");
        let backend_update_provider_id =
            backend_update_provider_id(Some(profiling), backend_runtimes);
        let backend_update_backend = backend_update_backend(profiling, backend_update_provider_id);
        let backend_runtime_available = backend_update_runtime_available(backend_runtimes);

        Self {
            navigation_bar,
            audio_popover,
            backend_update_backend,
            backend_runtime_available,
        }
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

fn benchmark_target(
    id: mesh_core_debug::BenchmarkScenarioId,
    profiling_view: Option<&BenchmarkProfilingView<'_>>,
    backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
) -> String {
    match id {
        mesh_core_debug::BenchmarkScenarioId::Hover => "@mesh/navigation-bar".to_string(),
        mesh_core_debug::BenchmarkScenarioId::SurfaceOpenClose => "@mesh/audio-popover".to_string(),
        mesh_core_debug::BenchmarkScenarioId::PointerUpdate => {
            "@mesh/navigation-bar audio controls".to_string()
        }
        mesh_core_debug::BenchmarkScenarioId::KeyboardTraversal => {
            "@mesh/navigation-bar focus chain".to_string()
        }
        mesh_core_debug::BenchmarkScenarioId::BackendUpdate => {
            backend_update_target(profiling_view, backend_runtimes)
        }
    }
}

fn benchmark_metrics(
    id: mesh_core_debug::BenchmarkScenarioId,
    profiling_view: &BenchmarkProfilingView<'_>,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    match id {
        mesh_core_debug::BenchmarkScenarioId::Hover => surface_benchmark_metrics(
            profiling_view.navigation_bar,
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
            profiling_view.audio_popover,
            "Open and close @mesh/audio-popover while profiling is live",
        ),
        mesh_core_debug::BenchmarkScenarioId::PointerUpdate => surface_benchmark_metrics(
            profiling_view.navigation_bar,
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
            profiling_view.navigation_bar,
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
            backend_update_benchmark_metrics(profiling_view)
        }
    }
}

fn surface_benchmark_metrics(
    surface: Option<&mesh_core_debug::ProfilingSurfaceSnapshot>,
    primary_stages: &[mesh_core_debug::ProfilingStage],
    secondary_stages: &[mesh_core_debug::ProfilingStage],
    hint: &str,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    let Some(surface) = surface else {
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
    surface: Option<&mesh_core_debug::ProfilingSurfaceSnapshot>,
    hint: &str,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    let Some(surface) = surface else {
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
    profiling_view: &BenchmarkProfilingView<'_>,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    let backend = profiling_view.backend_update_backend;
    let frontend = profiling_view
        .navigation_bar
        .or(profiling_view.audio_popover);
    let primary = backend.and_then(|backend| {
        first_backend_stage(
            backend,
            &[
                mesh_core_debug::ProfilingBackendStage::CommandHandling,
                mesh_core_debug::ProfilingBackendStage::PollUpdate,
                mesh_core_debug::ProfilingBackendStage::StatePublishDelivery,
            ],
        )
    });

    match (backend, primary, frontend) {
        (Some(backend), Some(primary), Some(frontend))
            if frontend.total_surface_render_time_micros > 0 =>
        {
            (
                mesh_core_debug::BenchmarkScenarioStatus::Complete,
                format!(
                    "{} -> {} {}",
                    backend.interface,
                    backend.provider_id,
                    profiling_backend_stage_metric(primary)
                ),
                format!(
                    "frontend total_surface_render: {}us",
                    frontend.total_surface_render_time_micros
                ),
                "Update mesh.audio while profiling is live".to_string(),
            )
        }
        (None, _, _) if !profiling_view.backend_runtime_available => (
            mesh_core_debug::BenchmarkScenarioStatus::Unavailable,
            "No backend provider samples yet".to_string(),
            "No frontend surface render samples yet".to_string(),
            "Start the backend provider and run this scenario while profiling is live".to_string(),
        ),
        (Some(_), None, frontend) => waiting_for_backend_samples(frontend),
        (None, _, frontend) => waiting_for_backend_samples(frontend),
        (_, _, Some(_)) | (_, _, None) => waiting_for_surface_samples(),
    }
}

fn backend_update_target(
    profiling_view: Option<&BenchmarkProfilingView<'_>>,
    backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
) -> String {
    if let Some(backend) = profiling_view.and_then(|view| view.backend_update_backend) {
        return format!("{} -> {}", backend.interface, backend.provider_id);
    }
    if let Some(runtime) = backend_runtimes
        .iter()
        .find(|entry| is_running_audio_runtime(entry))
    {
        return format!("{} -> {}", runtime.interface, runtime.provider_id);
    }
    "mesh.audio -> @mesh/pipewire-audio".to_string()
}

fn backend_update_backend<'a>(
    profiling: &'a mesh_core_debug::ProfilingSnapshot,
    provider_id: Option<&str>,
) -> Option<&'a mesh_core_debug::ProfilingBackendSnapshot> {
    profiling.backends.iter().find(|backend| {
        backend.interface == "mesh.audio"
            && provider_id
                .map(|provider_id| backend.provider_id == provider_id)
                .unwrap_or(true)
    })
}

fn backend_update_runtime_available(
    backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
) -> bool {
    backend_runtimes.iter().any(is_running_audio_runtime)
}

fn backend_update_provider_id<'a>(
    profiling: Option<&'a mesh_core_debug::ProfilingSnapshot>,
    backend_runtimes: &'a [mesh_core_debug::BackendRuntimeEntry],
) -> Option<&'a str> {
    backend_runtimes
        .iter()
        .find(|entry| is_running_audio_runtime(entry))
        .map(|entry| entry.provider_id.as_str())
        .or_else(|| {
            profiling.and_then(|profiling| {
                profiling
                    .backends
                    .iter()
                    .find(|backend| backend.interface == "mesh.audio")
                    .map(|backend| backend.provider_id.as_str())
            })
        })
}

fn is_running_audio_runtime(entry: &mesh_core_debug::BackendRuntimeEntry) -> bool {
    entry.interface == "mesh.audio" && entry.status == BackendRuntimeStatus::Running.as_str()
}

fn waiting_for_backend_samples(
    frontend: Option<&mesh_core_debug::ProfilingSurfaceSnapshot>,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    (
        mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples,
        "No backend provider samples yet".to_string(),
        frontend
            .filter(|surface| surface.total_surface_render_time_micros > 0)
            .map(|surface| {
                format!(
                    "frontend total_surface_render: {}us",
                    surface.total_surface_render_time_micros
                )
            })
            .unwrap_or_else(|| "No frontend surface render samples yet".to_string()),
        "Run the backend-driven scenario while profiling is live".to_string(),
    )
}

fn waiting_for_surface_samples() -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    (
        mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples,
        "Backend provider timing captured".to_string(),
        "No frontend surface render samples yet".to_string(),
        "Render @mesh/navigation-bar or @mesh/audio-popover after the backend update".to_string(),
    )
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

fn module_object_entry_json(entry: &mesh_core_debug::ModuleObjectEntry) -> serde_json::Value {
    serde_json::json!({
        "instance_id": entry.instance_id,
        "module_id": entry.module_id,
        "object_kind": entry.object_kind,
        "interface": entry.interface,
        "version": entry.version,
        "lifecycle": entry.lifecycle,
        "active": entry.active,
        "capabilities": entry.capabilities,
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

fn health_entry_json(entry: &HealthEntry) -> serde_json::Value {
    serde_json::json!({
        "module_id": entry.module_id,
        "status": entry.status,
    })
}

fn keybind_entry_json(entry: &mesh_core_debug::DebugKeybindEntry) -> serde_json::Value {
    serde_json::json!({
        "surface_id": entry.surface_id,
        "module_id": entry.module_id,
        "action_id": entry.action_id,
        "key": entry.key,
        "modifiers": entry.modifiers,
        "trigger_kind": entry.trigger_kind,
        "source": entry.source,
        "accessibility_shortcut": entry.accessibility_shortcut,
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
        "invalidation": snapshot.invalidation.as_ref().map(profiling_invalidation_json),
    })
}

fn profiling_invalidation_json(
    snapshot: &mesh_core_debug::ProfilingInvalidationSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "full_rebuild": snapshot.full_rebuild,
        "retained_path": snapshot.retained_path,
        "retained_generation": snapshot.retained_generation,
        "component": {
            "script": snapshot.component.script,
            "state": snapshot.component.state,
            "style": snapshot.component.style,
            "layout": snapshot.component.layout,
            "paint": snapshot.component.paint,
            "text": snapshot.component.text,
            "accessibility": snapshot.component.accessibility,
            "metrics": snapshot.component.metrics,
            "surface_config": snapshot.component.surface_config,
        },
        "retained": {
            "inserted": snapshot.retained.inserted,
            "removed": snapshot.retained.removed,
            "layout": snapshot.retained.layout,
            "style": snapshot.retained.style,
            "attributes": snapshot.retained.attributes,
            "children": snapshot.retained.children,
            "state": snapshot.retained.state,
        },
        "paint": profiling_paint_snapshot_json(&snapshot.paint),
        "text": {
            "layout_hits": snapshot.text.layout_hits,
            "layout_misses": snapshot.text.layout_misses,
            "layout_invalidations": snapshot.text.layout_invalidations,
            "shaped_entries": snapshot.text.shaped_entries,
            "glyph_cache_active": snapshot.text.glyph_cache_active,
            "shaping_micros": snapshot.text.shaping_micros,
        },
    })
}

fn profiling_paint_snapshot_json(
    paint: &mesh_core_debug::RetainedPaintSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "retained_generation": paint.retained_generation,
        "entries_total": paint.entries_total,
        "entries_reused": paint.entries_reused,
        "entries_rebuilt": paint.entries_rebuilt,
        "entries_removed": paint.entries_removed,
        "subtree_segments_reused": paint.subtree_segments_reused,
        "subtree_segments_rebuilt": paint.subtree_segments_rebuilt,
        "subtree_commands_rebuilt": paint.subtree_commands_rebuilt,
        "changed_layout_count": paint.changed_layout_count,
        "changed_paint_count": paint.changed_paint_count,
        "effect_overflow_count": paint.effect_overflow_count,
        "fallback_promotion_count": paint.fallback_promotion_count,
        "full_fallback_count": paint.full_fallback_count,
        "broad_dirty_fallback_count": paint.broad_dirty_fallback_count,
        "damage_rect_count": paint.damage_rect_count,
        "damage_area": paint.damage_area,
        "surface_area": paint.surface_area,
        "full_surface_damage": paint.full_surface_damage,
        "partial_present_supported": paint.partial_present_supported,
        "skipped_paint_pixels": paint.skipped_paint_pixels,
        "omitted_subtrees": paint.omitted_subtrees,
        "omitted_nodes": paint.omitted_nodes,
        "omitted_commands": paint.omitted_commands,
        "preclipped_descendants": paint.preclipped_descendants,
        "repaint_policy": paint.repaint_policy.as_str(),
        "filtered_span_count": paint.filtered_span_count,
        "filtered_command_count": paint.filtered_command_count,
        "filtered_commands_skipped": paint.filtered_commands_skipped,
        "filtered_fallback_count": paint.filtered_fallback_count,
        "batch_count": paint.batch_count,
        "batched_primitives": paint.batched_primitives,
        "barrier_count": paint.barrier_count,
        "barriers": profiling_paint_barriers_json(&paint.barriers),
        "raster_cache_hits": paint.raster_cache_hits,
        "raster_cache_misses": paint.raster_cache_misses,
        "raster_cache_bypasses": paint.raster_cache_bypasses,
        "raster_cache_opaque_hits": paint.raster_cache_opaque_hits,
        "raster_cache_translucent_hits": paint.raster_cache_translucent_hits,
    })
}

fn profiling_paint_barriers_json(
    barriers: &mesh_core_debug::DisplayBatchBarrierSnapshot,
) -> serde_json::Value {
    serde_json::json!({
        "text": barriers.text,
        "icon": barriers.icon,
        "opacity": barriers.opacity,
        "clip": barriers.clip,
        "translucency": barriers.translucency,
        "material_change": barriers.material_change,
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
