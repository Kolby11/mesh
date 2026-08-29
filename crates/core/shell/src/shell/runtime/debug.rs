#![allow(dead_code)] // Debug snapshots are used by optional profiling integrations.

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
                health: inst.health().state.to_string(),
                static_health: inst.static_health.state.to_string(),
                runtime_health: inst.runtime_health.state.to_string(),
                quarantined: inst.quarantined,
                error_count: inst.error_count,
                last_error: inst.last_error.clone(),
            })
            .collect();

        let module_instances = self.module_object_entries();

        let catalog = self.interfaces.resolved_catalog();
        let mut interfaces: Vec<InterfaceEntry> = catalog
            .list_interfaces()
            .into_iter()
            .map(|name| {
                let providers = catalog.providers_for(&name);
                let providers = providers
                    .iter()
                    .map(|p| ProviderEntry {
                        backend_name: p.backend_name.clone(),
                        priority: p.priority,
                    })
                    .collect();
                InterfaceEntry { name, providers }
            })
            .collect();
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));

        let health = self
            .diagnostics
            .snapshot()
            .into_iter()
            .map(|entry| HealthEntry {
                module_id: entry.module_id,
                status: entry.health.to_string(),
            })
            .collect();

        let mut diagnostics = self
            .diagnostics
            .snapshot()
            .into_iter()
            .flat_map(|module| module.instances)
            .flat_map(|instance| instance.issues)
            .map(debug_diagnostic_from_issue)
            .collect::<Vec<_>>();
        let catalog = self.frontend_catalog.snapshot().catalog;
        diagnostics.extend(catalog.diagnostics.iter().map(|diagnostic| {
            DebugDiagnosticEntry {
                module_id: diagnostic.module_id.clone(),
                instance_id: diagnostic.contribution_id.clone(),
                issue_code: format!(
                    "frontend-source:{}:{}",
                    diagnostic.category.as_str(),
                    diagnostic.source_path.display()
                ),
                category: diagnostic.category,
                severity: mesh_core_diagnostics::IssueSeverity::Error,
                message: diagnostic.message.clone(),
                source_path: diagnostic
                    .source_file
                    .as_ref()
                    .or(Some(&diagnostic.source_path))
                    .map(|path| path.display().to_string()),
                source_span: diagnostic.source_span,
                count: 1,
                active: true,
            }
        }));
        diagnostics.sort_by(|left, right| {
            left.module_id
                .cmp(&right.module_id)
                .then_with(|| left.issue_code.cmp(&right.issue_code))
                .then_with(|| left.message.cmp(&right.message))
        });

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
            .flat_map(|providers| providers.values())
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
            module_graph: self.module_graph_entries(),
            resources: self.resource_explanation_snapshot(),
            module_instances,
            interfaces,
            backend_runtimes,
            method_calls: self.debug.recent_method_calls.clone(),
            health,
            diagnostics,
            keybinds,
            active_surfaces,
            benchmarks,
            profiling,
        }
    }

    fn record_debug_snapshot_state(&mut self, snapshot: &DebugSnapshot) {
        // Synchronous snapshot reads need the latest state before the next
        // render tick, but they must still use the normal service-state path
        // for provider filtering, contract validation, and deduplication.
        let event = self.debug_snapshot_event(snapshot);
        self.record_latest_service_state(&event);
    }

    fn debug_snapshot_event(&self, snapshot: &DebugSnapshot) -> ServiceEvent {
        ServiceEvent::Updated {
            service: mesh_core_debug::DEBUG_INTERFACE.to_string(),
            source_module: self.active_service_provider_or(
                mesh_core_debug::DEBUG_INTERFACE,
                mesh_core_debug::DEBUG_SOURCE_MODULE_ID,
            ),
            payload: debug_service_payload(&self.debug, snapshot),
        }
    }
}

fn debug_diagnostic_from_issue(
    issue: mesh_core_diagnostics::DiagnosticIssue,
) -> DebugDiagnosticEntry {
    DebugDiagnosticEntry {
        module_id: issue.module_id,
        instance_id: (!issue.instance_id.is_empty()).then_some(issue.instance_id),
        issue_code: issue.issue_code,
        category: issue.category,
        severity: issue.severity,
        message: issue.message,
        source_path: issue.source_path,
        source_span: issue.source_span,
        count: issue.count,
        active: issue.active,
    }
}

fn debug_service_payload(
    debug: &mesh_core_debug::DebugOverlayState,
    snapshot: &DebugSnapshot,
) -> serde_json::Value {
    let mut payload = serde_json::to_value(snapshot).expect("debug snapshot DTOs serialize");
    let object = payload
        .as_object_mut()
        .expect("debug snapshot DTO serializes to an object");
    object.insert(
        "schema_version".to_string(),
        serde_json::json!(mesh_core_debug::DEBUG_TELEMETRY_SCHEMA_VERSION),
    );
    object.insert(
        "overlay_enabled".to_string(),
        serde_json::json!(debug.enabled),
    );
    object.insert(
        "layout_bounds_enabled".to_string(),
        serde_json::json!(debug.show_layout_bounds),
    );
    object.insert(
        "element_picker_enabled".to_string(),
        serde_json::json!(debug.element_picker_enabled),
    );
    object.insert(
        "inspected_element".to_string(),
        debug
            .inspected_element
            .clone()
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        "profiling_enabled".to_string(),
        serde_json::json!(debug.profiling_enabled),
    );
    object.insert(
        "profiling_session_id".to_string(),
        serde_json::json!(debug.profiling_session_id),
    );
    object.insert(
        "allocation_profiling_available".to_string(),
        serde_json::json!(mesh_core_debug::allocation::profiling_available()),
    );
    object.insert(
        "active_view".to_string(),
        serde_json::json!(debug.active_view.label()),
    );
    // Module graph entries intentionally keep their established nested wire
    // shape; the DTO owns the complete field set while this compatibility
    // adapter preserves the public `uses`/`provides`/`surface` grouping.
    object.insert(
        "module_graph".to_string(),
        snapshot
            .module_graph
            .iter()
            .map(module_graph_entry_json)
            .collect::<Vec<_>>()
            .into(),
    );
    object.insert(
        "profiling_stream".to_string(),
        snapshot
            .profiling
            .as_ref()
            .map(profiling_stream_json)
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        "chrome_trace".to_string(),
        snapshot
            .profiling
            .as_ref()
            .map(profiling_chrome_trace_json)
            .unwrap_or(serde_json::Value::Null),
    );
    payload
}

impl Shell {
    fn module_graph_entries(&self) -> Vec<mesh_core_debug::ModuleGraphEntry> {
        let Some(graph) = self.installed_module_graph.as_ref() else {
            return Vec::new();
        };

        let mut entries = graph
            .modules()
            .into_iter()
            .map(|module| {
                let requirements = graph.requirements_for_frontend(&module.id);
                let mut diagnostics = graph
                    .diagnostics()
                    .iter()
                    .filter(|diagnostic| diagnostic.module_id == module.id)
                    .map(|diagnostic| diagnostic.status.clone())
                    .collect::<Vec<_>>();
                diagnostics.sort();
                diagnostics.dedup();

                let mut health = graph
                    .health()
                    .iter()
                    .filter(|record| record.module_id == module.id)
                    .map(|record| record.status.clone())
                    .collect::<Vec<_>>();
                health.sort();
                health.dedup();

                let mut provides_interfaces = Vec::new();
                let mut provides_interface_labels = Vec::new();
                if module.kind == ModuleKind::Backend {
                    let providers: Vec<_> = graph
                        .backend_provider_contributions()
                        .into_iter()
                        .filter(|provider| provider.module_id == module.id)
                        .collect();
                    for provider in &providers {
                        provides_interfaces.push(provider.interface.clone());
                        provides_interface_labels.push(provider.label.as_ref().map(|label| {
                            resolve_debug_manifest_text(&self.locale, &module.id, label).text
                        }));
                    }
                }
                if let Some(interface) = graph
                    .declared_interfaces()
                    .into_iter()
                    .find(|interface| interface.module_id == module.id)
                {
                    provides_interfaces.push(interface.name.clone());
                    provides_interface_labels.push(None);
                }
                {
                    let mut pairs: Vec<(String, Option<String>)> = provides_interfaces
                        .into_iter()
                        .zip(provides_interface_labels)
                        .collect();
                    pairs.sort_by(|a, b| a.0.cmp(&b.0));
                    pairs.dedup_by(|a, b| a.0 == b.0);
                    provides_interfaces = pairs.iter().map(|(i, _)| i.clone()).collect();
                    provides_interface_labels = pairs.into_iter().map(|(_, l)| l).collect();
                }

                let (provides_themes, provides_theme_labels): (Vec<String>, Vec<Option<String>>) =
                    graph
                        .contributed_themes()
                        .iter()
                        .filter(|t| t.module_id == module.id)
                        .map(|t| {
                            let label = t.label.as_ref().map(|label| {
                                resolve_debug_manifest_text(&self.locale, &module.id, label).text
                            });
                            (t.id.clone(), label)
                        })
                        .unzip();

                let module_settings = graph
                    .settings_schemas()
                    .iter()
                    .find(|settings| settings.module_id == module.id);
                let provides_settings = graph
                    .settings_schemas()
                    .iter()
                    .filter(|settings| settings.module_id == module.id)
                    .map(|settings| settings.namespace.clone())
                    .collect::<Vec<_>>();
                let settings_values = self
                    .settings_store
                    .namespace(&module.id)
                    .pointer("/props/global")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let mut settings_instances = self
                    .components
                    .iter()
                    .filter(|runtime| runtime.component.id() == module.id)
                    .map(|runtime| runtime.surface_id.clone())
                    .collect::<Vec<_>>();
                settings_instances.sort();
                settings_instances.dedup();
                let settings_instance_values = self
                    .settings_store
                    .namespace(&module.id)
                    .pointer("/props/instances")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let provides_i18n = graph
                    .contributed_i18n()
                    .iter()
                    .filter(|i18n| i18n.module_id == module.id)
                    .map(|i18n| {
                        let source = format!("{}:{}", i18n.locale, i18n.path);
                        if i18n.target_module_id == i18n.module_id {
                            source
                        } else {
                            format!("{source}->{}", i18n.target_module_id)
                        }
                    })
                    .collect::<Vec<_>>();
                let required_icons = graph
                    .icon_requirements()
                    .iter()
                    .filter(|icon| icon.module_id == module.id && icon.required)
                    .map(|icon| icon.name.clone())
                    .collect::<Vec<_>>();
                let optional_icons = graph
                    .icon_requirements()
                    .iter()
                    .filter(|icon| icon.module_id == module.id && !icon.required)
                    .map(|icon| icon.name.clone())
                    .collect::<Vec<_>>();
                let mut required_binaries = module
                    .manifest
                    .mesh
                    .dependencies
                    .binaries
                    .iter()
                    .filter(|binary| !binary.optional)
                    .map(|binary| binary.name.clone())
                    .collect::<Vec<_>>();
                required_binaries.sort();
                required_binaries.dedup();
                let mut optional_binaries = module
                    .manifest
                    .mesh
                    .dependencies
                    .binaries
                    .iter()
                    .filter(|binary| binary.optional)
                    .map(|binary| binary.name.clone())
                    .collect::<Vec<_>>();
                optional_binaries.sort();
                optional_binaries.dedup();
                let mut native_binaries = module
                    .manifest
                    .mesh
                    .dependencies
                    .binaries
                    .iter()
                    .map(|binary| mesh_core_debug::ModuleBinaryHealthEntry {
                        name: binary.name.clone(),
                        optional: binary.optional,
                        available: mesh_core_module::package::binary_available(&binary.name),
                    })
                    .collect::<Vec<_>>();
                native_binaries.sort_by(|left, right| left.name.cmp(&right.name));
                let mut keybind_actions = graph
                    .keybind_actions()
                    .iter()
                    .filter(|action| action.module_id == module.id)
                    .map(|action| action.action_id.clone())
                    .collect::<Vec<_>>();
                keybind_actions.sort();
                keybind_actions.dedup();
                let mut active_providers = requirements
                    .into_iter()
                    .flat_map(|requirements| {
                        requirements
                            .backend
                            .keys()
                            .chain(requirements.optional_backend.keys())
                    })
                    .filter_map(|interface| {
                        graph
                            .active_provider(interface)
                            .map(|provider| format!("{interface}={}", provider.module_id))
                    })
                    .collect::<Vec<_>>();
                active_providers.sort();
                active_providers.dedup();
                let frontend_surface = graph
                    .frontend_surfaces()
                    .iter()
                    .find(|surface| surface.module_id == module.id);
                let surface_layout_label = frontend_surface.and_then(|surface| {
                    graph
                        .contributed_layouts()
                        .iter()
                        .find(|layout| layout.module_id == module.id && layout.path == surface.path)
                        .and_then(|layout| layout.label.as_ref())
                        .map(|label| resolve_debug_manifest_text(&self.locale, &module.id, label))
                });

                mesh_core_debug::ModuleGraphEntry {
                    module_id: module.id.clone(),
                    kind: format!("{:?}", module.kind).to_lowercase(),
                    enabled: module.enabled,
                    root_layout: graph
                        .layout_entrypoint()
                        .is_some_and(|layout| layout.module_id == module.id),
                    path: module.path.clone(),
                    uses_modules: requirements
                        .map(|requirements| sorted_keys(&requirements.modules))
                        .unwrap_or_default(),
                    uses_interfaces: requirements
                        .map(|requirements| sorted_keys(&requirements.backend))
                        .unwrap_or_default(),
                    uses_optional_interfaces: requirements
                        .map(|requirements| sorted_keys(&requirements.optional_backend))
                        .unwrap_or_default(),
                    uses_icon_packs: requirements
                        .map(|requirements| sorted_keys(&requirements.icons))
                        .unwrap_or_default(),
                    uses_i18n_packs: requirements
                        .map(|requirements| sorted_keys(&requirements.i18n))
                        .unwrap_or_default(),
                    uses_theme_packs: requirements
                        .map(|requirements| sorted_keys(&requirements.themes))
                        .unwrap_or_default(),
                    uses_font_packs: requirements
                        .map(|requirements| sorted_keys(&requirements.fonts))
                        .unwrap_or_default(),
                    required_binaries,
                    optional_binaries,
                    keybind_actions,
                    active_providers,
                    native_binaries,
                    capabilities: module.manifest.mesh.capabilities.required.clone(),
                    optional_capabilities: module.manifest.mesh.capabilities.optional.clone(),
                    surface_entrypoint: frontend_surface.map(|surface| surface.path.clone()),
                    surface_settings_namespace: frontend_surface
                        .and_then(|surface| surface.settings_namespace.clone()),
                    surface_accessibility_role: frontend_surface
                        .and_then(|surface| surface.accessibility.as_ref())
                        .and_then(|accessibility| accessibility.role.clone()),
                    surface_accessibility_label: frontend_surface
                        .and_then(|surface| surface.accessibility.as_ref())
                        .and_then(|accessibility| accessibility.label.clone()),
                    // Surface sizing is CSS content-measured for every surface
                    // now; there is no manifest size-policy to report.
                    surface_size_policy: None,
                    surface_layout_label: surface_layout_label
                        .as_ref()
                        .map(|resolved| resolved.text.clone()),
                    surface_layout_label_key: surface_layout_label
                        .as_ref()
                        .and_then(|resolved| resolved.key.clone()),
                    surface_layout_label_fallback: surface_layout_label
                        .as_ref()
                        .and_then(|resolved| resolved.fallback.clone()),
                    provides_interfaces,
                    provides_interface_labels,
                    provides_settings,
                    settings_schema: module_settings.map(|settings| settings.schema.clone()),
                    settings_values,
                    settings_instances,
                    settings_instance_values,
                    // Report the page that will actually render: a composition
                    // may have suppressed or replaced the module's own, and the
                    // generated fallback keys off this being absent.
                    settings_page: graph
                        .resolved_contribution_entry(
                            &module.id,
                            mesh_core_module::package::SETTINGS_PAGE_POINT,
                        )
                        .map(str::to_string),
                    provides_i18n,
                    provides_themes,
                    provides_theme_labels,
                    required_icons,
                    optional_icons,
                    diagnostics,
                    health,
                }
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.module_id.cmp(&right.module_id));
        entries
    }

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

        let catalog = self.interfaces.resolved_catalog();
        for interface in catalog.list_interfaces() {
            let providers = catalog.providers_for(&interface);
            for provider in providers {
                let active = self
                    .backend_runtimes
                    .get(&interface)
                    .is_some_and(|slot| slot.provider_id == provider.provider_module);
                let lifecycle = self
                    .backend_runtime_status(&interface, &provider.provider_module)
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

    pub(in crate::shell) fn record_method_call(&mut self, entry: mesh_core_debug::MethodCallEntry) {
        const MAX_METHOD_CALLS: usize = 50;
        self.debug.recent_method_calls.push(entry);
        if self.debug.recent_method_calls.len() > MAX_METHOD_CALLS {
            let overflow = self.debug.recent_method_calls.len() - MAX_METHOD_CALLS;
            self.debug.recent_method_calls.drain(0..overflow);
        }
    }

    pub(in crate::shell) fn record_backend_method_result(
        &mut self,
        interface: String,
        provider_id: String,
        call_id: mesh_core_backend::CallId,
        command: String,
        result: serde_json::Value,
        outcome: mesh_core_backend::BackendCommandOutcome,
    ) {
        let error = result
            .get("error")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        let terminal_status = outcome.as_str().to_string();
        if let Some(entry) = self
            .debug
            .recent_method_calls
            .iter_mut()
            .rev()
            .find(|entry| entry.call_id == call_id.raw())
        {
            entry.provider_id = Some(provider_id);
            entry.status = terminal_status;
            entry.queued = false;
            entry.result = Some(result);
            entry.error = error;
            return;
        }
        self.record_method_call(mesh_core_debug::MethodCallEntry {
            call_id: call_id.raw(),
            interface,
            provider_id: Some(provider_id),
            source_module_id: "<backend>".to_string(),
            command,
            status: terminal_status,
            queued: false,
            result: Some(result),
            error,
        });
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
        BenchmarkScenarioId::Idle,
        BenchmarkScenarioId::Hover,
        BenchmarkScenarioId::SurfaceOpenClose,
        BenchmarkScenarioId::PointerUpdate,
        BenchmarkScenarioId::TextUpdate,
        BenchmarkScenarioId::Scroll,
        BenchmarkScenarioId::IconGrid,
        BenchmarkScenarioId::Animation,
        BenchmarkScenarioId::ThemeReload,
        BenchmarkScenarioId::Resize,
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
    shell: &'a mesh_core_debug::ProfilingScopeSnapshot,
    navigation_bar: Option<&'a mesh_core_debug::ProfilingSurfaceSnapshot>,
    audio_popover: Option<&'a mesh_core_debug::ProfilingSurfaceSnapshot>,
    settings: Option<&'a mesh_core_debug::ProfilingSurfaceSnapshot>,
    debug_inspector: Option<&'a mesh_core_debug::ProfilingSurfaceSnapshot>,
    backend_update_backend: Option<&'a mesh_core_debug::ProfilingBackendSnapshot>,
    backend_update_surface: Option<&'a mesh_core_debug::ProfilingSurfaceSnapshot>,
    backend_runtime_available: bool,
}

impl<'a> BenchmarkProfilingView<'a> {
    fn new(
        profiling: &'a mesh_core_debug::ProfilingSnapshot,
        backend_runtimes: &'a [mesh_core_debug::BackendRuntimeEntry],
    ) -> Self {
        let navigation_bar = profiling_surface(profiling, "@mesh/navigation-bar");
        let audio_popover = profiling_surface(profiling, "@mesh/audio-popover");
        let settings = profiling_surface(profiling, "@mesh/settings");
        let debug_inspector = profiling_surface(profiling, "@mesh/debug-inspector");
        let backend_update_backend = backend_update_backend(profiling, backend_runtimes);
        let backend_update_surface = profiling
            .surfaces
            .iter()
            .filter(|surface| surface.total_surface_render_time_micros > 0)
            .max_by_key(|surface| surface.total_surface_render_time_micros);
        let backend_runtime_available = backend_update_runtime_available(backend_runtimes);

        Self {
            shell: &profiling.shell,
            navigation_bar,
            audio_popover,
            settings,
            debug_inspector,
            backend_update_backend,
            backend_update_surface,
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
        mesh_core_debug::BenchmarkScenarioId::Idle => "shell scheduler".to_string(),
        mesh_core_debug::BenchmarkScenarioId::Hover => "@mesh/navigation-bar".to_string(),
        mesh_core_debug::BenchmarkScenarioId::SurfaceOpenClose => "@mesh/audio-popover".to_string(),
        mesh_core_debug::BenchmarkScenarioId::PointerUpdate => {
            "@mesh/navigation-bar audio controls".to_string()
        }
        mesh_core_debug::BenchmarkScenarioId::TextUpdate => {
            "@mesh/settings text controls".to_string()
        }
        mesh_core_debug::BenchmarkScenarioId::Scroll => "@mesh/settings".to_string(),
        mesh_core_debug::BenchmarkScenarioId::IconGrid => "@mesh/debug-inspector".to_string(),
        mesh_core_debug::BenchmarkScenarioId::Animation => "@mesh/navigation-bar".to_string(),
        mesh_core_debug::BenchmarkScenarioId::ThemeReload => {
            "active theme + @mesh/navigation-bar".to_string()
        }
        mesh_core_debug::BenchmarkScenarioId::Resize => "@mesh/navigation-bar".to_string(),
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
        mesh_core_debug::BenchmarkScenarioId::Idle => shell_benchmark_metrics(
            profiling_view.shell,
            mesh_core_debug::ProfilingStage::SchedulerIdle,
            "Leave the shell idle after starting a fresh profiling session",
        ),
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
        mesh_core_debug::BenchmarkScenarioId::TextUpdate => surface_benchmark_metrics(
            profiling_view.settings,
            &[
                mesh_core_debug::ProfilingStage::InputHandling,
                mesh_core_debug::ProfilingStage::RuntimeUpdateHandling,
            ],
            &[
                mesh_core_debug::ProfilingStage::TreeBuild,
                mesh_core_debug::ProfilingStage::TextShaping,
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
            ],
            "Edit a text control on @mesh/settings in a fresh profiling session",
        ),
        mesh_core_debug::BenchmarkScenarioId::Scroll => surface_benchmark_metrics(
            profiling_view.settings,
            &[mesh_core_debug::ProfilingStage::InputHandling],
            &[
                mesh_core_debug::ProfilingStage::Layout,
                mesh_core_debug::ProfilingStage::Paint,
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
            ],
            "Scroll @mesh/settings in a fresh profiling session",
        ),
        mesh_core_debug::BenchmarkScenarioId::IconGrid => surface_benchmark_metrics(
            profiling_view.debug_inspector,
            &[
                mesh_core_debug::ProfilingStage::IconImageRaster,
                mesh_core_debug::ProfilingStage::PaintTraversal,
            ],
            &[
                mesh_core_debug::ProfilingStage::Paint,
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
            ],
            "Open the debug inspector icon-heavy view in a fresh profiling session",
        ),
        mesh_core_debug::BenchmarkScenarioId::Animation => surface_benchmark_metrics(
            profiling_view.navigation_bar,
            &[
                mesh_core_debug::ProfilingStage::StyleRestyle,
                mesh_core_debug::ProfilingStage::RuntimeUpdateHandling,
            ],
            &[
                mesh_core_debug::ProfilingStage::Paint,
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
            ],
            "Run navigation-bar transitions in a fresh profiling session",
        ),
        mesh_core_debug::BenchmarkScenarioId::ThemeReload => surface_benchmark_metrics(
            profiling_view.navigation_bar,
            &[
                mesh_core_debug::ProfilingStage::TreeBuild,
                mesh_core_debug::ProfilingStage::StyleRestyle,
            ],
            &[
                mesh_core_debug::ProfilingStage::Layout,
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
            ],
            "Reload the active theme in a fresh profiling session",
        ),
        mesh_core_debug::BenchmarkScenarioId::Resize => surface_benchmark_metrics(
            profiling_view.navigation_bar,
            &[mesh_core_debug::ProfilingStage::Layout],
            &[
                mesh_core_debug::ProfilingStage::Paint,
                mesh_core_debug::ProfilingStage::PresentCommit,
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
            ],
            "Resize the navigation-bar output in a fresh profiling session",
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

fn shell_benchmark_metrics(
    shell: &mesh_core_debug::ProfilingScopeSnapshot,
    stage: mesh_core_debug::ProfilingStage,
    hint: &str,
) -> (
    mesh_core_debug::BenchmarkScenarioStatus,
    String,
    String,
    String,
) {
    let Some(summary) = shell.stages.iter().find(|summary| summary.stage == stage) else {
        return waiting_for_samples();
    };
    (
        mesh_core_debug::BenchmarkScenarioStatus::Complete,
        profiling_stage_metric(summary),
        format!("redraw_count: {}", shell.redraw_count),
        hint.to_string(),
    )
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
    let frontend = profiling_view.backend_update_surface;
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
                format!("Update {} while profiling is live", backend.interface),
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
    if let Some(runtime) = running_backend_runtime(backend_runtimes) {
        return format!("{} -> {}", runtime.interface, runtime.provider_id);
    }
    "active backend provider".to_string()
}

fn backend_update_backend<'a>(
    profiling: &'a mesh_core_debug::ProfilingSnapshot,
    backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
) -> Option<&'a mesh_core_debug::ProfilingBackendSnapshot> {
    let Some(runtime) = running_backend_runtime(backend_runtimes) else {
        return profiling.backends.first();
    };
    profiling.backends.iter().find(|backend| {
        backend.interface == runtime.interface && backend.provider_id == runtime.provider_id
    })
}

fn backend_update_runtime_available(
    backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
) -> bool {
    running_backend_runtime(backend_runtimes).is_some()
}

fn running_backend_runtime(
    backend_runtimes: &[mesh_core_debug::BackendRuntimeEntry],
) -> Option<&mesh_core_debug::BackendRuntimeEntry> {
    backend_runtimes
        .iter()
        .find(|entry| entry.status == BackendRuntimeStatus::Running.as_str())
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

fn module_graph_entry_json(entry: &mesh_core_debug::ModuleGraphEntry) -> serde_json::Value {
    serde_json::json!({
        "module_id": entry.module_id,
        "kind": entry.kind,
        "enabled": entry.enabled,
        "root_layout": entry.root_layout,
        "path": entry.path,
        "uses": {
            "modules": entry.uses_modules,
            "interfaces": entry.uses_interfaces,
            "optional_interfaces": entry.uses_optional_interfaces,
            "icon_packs": entry.uses_icon_packs,
            "i18n_packs": entry.uses_i18n_packs,
            "theme_packs": entry.uses_theme_packs,
            "font_packs": entry.uses_font_packs,
            "required_binaries": entry.required_binaries,
            "optional_binaries": entry.optional_binaries,
            "keybinds": entry.keybind_actions,
            "active_providers": entry.active_providers,
            "native_binaries": entry.native_binaries.iter().map(|binary| serde_json::json!({
                "name": binary.name,
                "optional": binary.optional,
                "available": binary.available,
            })).collect::<Vec<_>>(),
            "capabilities": entry.capabilities,
            "optional_capabilities": entry.optional_capabilities,
        },
        "provides": {
            "interfaces": entry.provides_interfaces.iter().zip(entry.provides_interface_labels.iter())
                .map(|(iface, label)| serde_json::json!({ "interface": iface, "label": label }))
                .collect::<Vec<_>>(),
            "themes": entry.provides_themes.iter().zip(entry.provides_theme_labels.iter())
                .map(|(id, label)| serde_json::json!({ "id": id, "label": label }))
                .collect::<Vec<_>>(),
            "settings": entry.provides_settings,
            "settings_schema": entry.settings_schema,
            "settings_values": entry.settings_values,
            "settings_instances": entry.settings_instances,
            "settings_instance_values": entry.settings_instance_values,
            "settings_page": entry.settings_page,
            "i18n": entry.provides_i18n,
            "required_icons": entry.required_icons,
            "optional_icons": entry.optional_icons,
        },
        "surface": entry.surface_entrypoint.as_ref().map(|entrypoint| serde_json::json!({
            "entrypoint": entrypoint,
            "settings_namespace": entry.surface_settings_namespace.as_ref(),
            "accessibility_role": entry.surface_accessibility_role.as_ref(),
            "accessibility_label": entry.surface_accessibility_label.as_ref(),
            "size_policy": entry.surface_size_policy.as_ref(),
            "layout_label": entry.surface_layout_label.as_ref(),
            "layout_label_key": entry.surface_layout_label_key.as_ref(),
            "layout_label_fallback": entry.surface_layout_label_fallback.as_ref(),
        })),
        "diagnostics": entry.diagnostics,
        "health": entry.health,
    })
}

fn sorted_keys<T>(map: &std::collections::HashMap<String, T>) -> Vec<String> {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

fn resolve_debug_manifest_text(
    locale: &mesh_core_locale::LocaleEngine,
    module_id: &str,
    text: &mesh_core_module::LocalizedText,
) -> mesh_core_locale::LocalizedTextResolution {
    match text {
        mesh_core_module::LocalizedText::Literal(value) => {
            mesh_core_locale::LocalizedTextResolution::literal(
                module_id,
                value,
                locale.catalog_snapshot().revision(),
            )
        }
        mesh_core_module::LocalizedText::Translation { key, fallback } => {
            let translator = locale.module_translator(module_id);
            translator.resolve(key, Some(fallback))
        }
    }
}

/// Bounded, order-stable samples for consumers that need an event stream rather
/// than aggregate stage summaries. Surface samples are sourced from their
/// per-surface buckets so records duplicated into the shell roll-up appear once.
fn profiling_stream_json(snapshot: &mesh_core_debug::ProfilingSnapshot) -> serde_json::Value {
    let mut samples = Vec::new();
    for summary in &snapshot.shell.stages {
        samples.extend(
            summary
                .recent_samples
                .iter()
                .filter(|sample| sample.surface_id.is_none())
                .map(|sample| serde_json::to_value(sample).expect("profiling sample serializes")),
        );
    }
    for surface in &snapshot.surfaces {
        for summary in &surface.stages {
            samples.extend(
                summary.recent_samples.iter().map(|sample| {
                    serde_json::to_value(sample).expect("profiling sample serializes")
                }),
            );
        }
    }
    samples.sort_by_key(|sample| sample["order"].as_u64().unwrap_or(u64::MAX));
    serde_json::Value::Array(samples)
}

/// Chrome trace / Perfetto-compatible complete events. Timestamps are monotonic
/// microseconds from the active profiling session, captured alongside each sample.
fn profiling_chrome_trace_json(snapshot: &mesh_core_debug::ProfilingSnapshot) -> serde_json::Value {
    let stream = profiling_stream_json(snapshot);
    let mut events = stream
        .as_array()
        .into_iter()
        .flatten()
        .map(|sample| {
            let surface_id = sample["surface_id"].as_str().unwrap_or("shell");
            serde_json::json!({
                "name": sample["stage"],
                "cat": "mesh",
                "ph": "X",
                "pid": "mesh-shell",
                "tid": surface_id,
                "ts": sample["timestamp_micros"],
                "dur": sample["duration_micros"],
                "args": { "trigger": sample["trigger_kind"], "module_id": sample["module_id"] },
            })
        })
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event["ts"].as_u64().unwrap_or(u64::MAX));
    serde_json::json!({ "traceEvents": events, "displayTimeUnit": "ms" })
}

#[cfg(test)]
mod allocation_profiling_json_tests {
    #[test]
    fn allocation_summary_json_keeps_cumulative_and_recent_pass_counts() {
        let summary = mesh_core_debug::ProfilingAllocationSummary {
            sample_count: 2,
            allocation_count: 7,
            allocated_bytes: 4_096,
            deallocation_count: 5,
            deallocated_bytes: 2_048,
            reallocation_count: 1,
            max_allocated_bytes_per_pass: 3_072,
            recent_samples: vec![mesh_core_debug::ProfilingAllocationSample {
                order: 4,
                timestamp_micros: 90,
                allocation_count: 3,
                allocated_bytes: 3_072,
                deallocation_count: 2,
                deallocated_bytes: 1_024,
                reallocation_count: 1,
            }],
        };

        let json = serde_json::to_value(&summary).expect("allocation DTO serializes");
        assert_eq!(json["sample_count"], serde_json::json!(2));
        assert_eq!(json["allocated_bytes"], serde_json::json!(4_096));
        assert_eq!(
            json["max_allocated_bytes_per_pass"],
            serde_json::json!(3_072)
        );
        assert_eq!(
            json["recent_samples"][0]["allocation_count"],
            serde_json::json!(3)
        );
    }

    #[test]
    fn profiling_snapshot_json_advertises_allocator_mode() {
        let json = serde_json::to_value(mesh_core_debug::ProfilingSnapshot {
            allocation_profiling_available: true,
            ..mesh_core_debug::ProfilingSnapshot::default()
        })
        .expect("profiling DTOs serialize");

        assert_eq!(
            json["allocation_profiling_available"],
            serde_json::json!(true)
        );
        assert!(json["shell"]["allocations"].is_null());
    }
}
