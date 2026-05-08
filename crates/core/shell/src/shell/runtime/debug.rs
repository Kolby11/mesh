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

        let active_surfaces = self
            .core
            .surfaces
            .iter()
            .filter(|(_, s)| s.visible)
            .map(|(id, _)| id.clone())
            .collect();

        let snapshot = DebugSnapshot {
            modules,
            interfaces,
            backend_runtimes,
            health,
            active_surfaces,
            profiling: self.debug.profiling_enabled.then(|| {
                let mut profiling = self.profiling.snapshot(self.debug.profiling_session_id);
                profiling.surfaces.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
                profiling.backends.sort_by(|a, b| {
                    a.interface
                        .cmp(&b.interface)
                        .then_with(|| a.provider_id.cmp(&b.provider_id))
                });
                profiling
            }),
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
        "profiling": snapshot.profiling.as_ref().map(profiling_snapshot_json),
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
