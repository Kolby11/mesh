use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BackendRuntimeStatus {
    NoActiveProvider,
    UnmetBackendRequirement,
    InvalidManifest,
    MissingEntrypoint,
    MissingBinary,
    InitFailed,
    Running,
    PollFailed,
    Failed,
    Stopped,
}

impl BackendRuntimeStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::NoActiveProvider => "no_active_provider",
            Self::UnmetBackendRequirement => "unmet_backend_requirement",
            Self::InvalidManifest => "invalid_manifest",
            Self::MissingEntrypoint => "missing_entrypoint",
            Self::MissingBinary => "missing_binary",
            Self::InitFailed => "init_failed",
            Self::Running => "running",
            Self::PollFailed => "poll_failed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    pub(super) fn from_str(status: &str) -> Self {
        match status {
            "no_active_provider" => Self::NoActiveProvider,
            "unmet_backend_requirement" => Self::UnmetBackendRequirement,
            "invalid_manifest" => Self::InvalidManifest,
            "missing_entrypoint" => Self::MissingEntrypoint,
            "missing_binary" => Self::MissingBinary,
            "init_failed" => Self::InitFailed,
            "running" => Self::Running,
            "poll_failed" => Self::PollFailed,
            "stopped" => Self::Stopped,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BackendRuntimeStatusEntry {
    pub(super) interface: String,
    pub(super) provider_id: String,
    pub(super) status: BackendRuntimeStatus,
    pub(super) message: String,
    /// Cumulative number of failure-category status updates recorded for this entry.
    pub(super) failure_count: u64,
}

#[derive(Debug, Clone)]
pub(super) struct BackendLaunchCandidate {
    pub(super) module_id: String,
    pub(super) interface: String,
    pub(super) service_name: String,
    pub(super) entrypoint_path: PathBuf,
    pub(super) script_source: String,
    pub(super) capabilities: Vec<String>,
    pub(super) settings: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BackendLifecycleStatusRecord {
    pub(super) interface: String,
    pub(super) provider_id: Option<String>,
    pub(super) status: &'static str,
    pub(super) message: String,
}

impl Shell {
    pub(super) fn record_backend_runtime_status(
        &mut self,
        interface: String,
        provider_id: String,
        status: BackendRuntimeStatus,
        message: String,
    ) {
        let is_failure = matches!(
            status,
            BackendRuntimeStatus::InvalidManifest
                | BackendRuntimeStatus::MissingEntrypoint
                | BackendRuntimeStatus::MissingBinary
                | BackendRuntimeStatus::InitFailed
                | BackendRuntimeStatus::PollFailed
                | BackendRuntimeStatus::Failed
        );
        if is_failure {
            self.diagnostics.record_lifecycle_error(
                provider_id.clone(),
                status.as_str(),
                message.clone(),
            );
        }
        let key = (interface.clone(), provider_id.clone());
        let prev_failure_count = self
            .backend_runtime_statuses
            .get(&key)
            .map(|entry| entry.failure_count)
            .unwrap_or(0);
        let failure_count = if is_failure {
            prev_failure_count + 1
        } else {
            prev_failure_count
        };
        self.backend_runtime_statuses.insert(
            key,
            BackendRuntimeStatusEntry {
                interface,
                provider_id,
                status,
                message,
                failure_count,
            },
        );
    }

    pub(super) fn stop_backend_runtime(&mut self, interface: &str) {
        self.service_handlers.remove(interface);
        if let Some(slot) = self.backend_runtimes.remove(interface) {
            slot.task.abort();
            let key = (slot.interface.clone(), slot.provider_id.clone());
            let terminal_failure_already_recorded = self
                .backend_runtime_statuses
                .get(&key)
                .map(|entry| {
                    matches!(
                        entry.status,
                        BackendRuntimeStatus::InitFailed | BackendRuntimeStatus::Failed
                    )
                })
                .unwrap_or(false);
            if !terminal_failure_already_recorded {
                self.record_backend_runtime_status(
                    slot.interface,
                    slot.provider_id,
                    BackendRuntimeStatus::Stopped,
                    "runtime stopped".to_string(),
                );
            }
        }
    }

    pub(super) fn replace_backend_runtime(&mut self, interface: String, slot: BackendRuntimeSlot) {
        self.stop_backend_runtime(&interface);
        self.service_handlers
            .insert(interface.clone(), slot.command_tx.clone());
        self.backend_runtimes.insert(interface, slot);
    }

    pub(super) fn handle_backend_lifecycle(
        &mut self,
        interface: String,
        provider_id: String,
        stage: String,
        status: String,
        message: String,
    ) {
        let runtime_status = BackendRuntimeStatus::from_str(&status);
        self.record_backend_runtime_status(
            interface.clone(),
            provider_id.clone(),
            runtime_status,
            message,
        );
        let event_provider_is_current = self
            .backend_runtimes
            .get(&interface)
            .is_some_and(|slot| slot.provider_id == provider_id);
        if matches!(
            runtime_status,
            BackendRuntimeStatus::InitFailed
                | BackendRuntimeStatus::Failed
                | BackendRuntimeStatus::Stopped
        ) && event_provider_is_current
        {
            tracing::debug!(
                interface = interface,
                stage = stage,
                "cleaning backend runtime slot"
            );
            self.stop_backend_runtime(&interface);
            self.clear_active_provider_service_state(&interface, &provider_id);
        }
    }

    /// Replace `latest_service_state` for the given interface with an unavailable
    /// payload when the active provider is known to be failing.
    pub(super) fn clear_active_provider_service_state(
        &mut self,
        interface: &str,
        provider_id: &str,
    ) {
        let unavailable_payload = if let Some(existing) = self.latest_service_state.get(interface) {
            let mut obj = if existing.state.is_object() {
                existing.state.clone()
            } else {
                serde_json::json!({})
            };
            if let Some(map) = obj.as_object_mut() {
                map.insert("available".to_string(), serde_json::Value::Bool(false));
            }
            obj
        } else {
            serde_json::json!({ "available": false })
        };
        self.latest_service_state.insert(
            interface.to_string(),
            LatestServiceState {
                interface: interface.to_string(),
                provider_id: provider_id.to_string(),
                state: unavailable_payload,
            },
        );
        tracing::debug!(
            interface,
            provider_id,
            "cleared stale public service state after provider failure"
        );
    }

    pub(super) fn spawn_backend_modules(
        &mut self,
        runtime: &Runtime,
        tx: mpsc::UnboundedSender<ShellMessage>,
    ) {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let graph_path = workspace_root.join("config/package.json");
        match load_installed_module_graph(&graph_path) {
            Ok(graph) => {
                let (candidates, statuses) = backend_launch_candidates_from_graph(
                    &graph,
                    &self.modules,
                    &self.config,
                    &self.interfaces,
                );
                for status in statuses {
                    self.record_backend_runtime_status(
                        status.interface.clone(),
                        status
                            .provider_id
                            .clone()
                            .unwrap_or_else(|| "<none>".to_string()),
                        BackendRuntimeStatus::from_str(status.status),
                        status.message.clone(),
                    );
                    tracing::warn!(
                        interface = status.interface,
                        provider_id = status.provider_id.as_deref().unwrap_or("<none>"),
                        status = status.status,
                        "{}",
                        status.message
                    );
                }
                for mut candidate in candidates {
                    self.apply_shell_runtime_settings(&mut candidate);
                    self.spawn_backend_candidate(runtime, tx.clone(), candidate);
                }
            }
            Err(err) => {
                tracing::warn!(
                    "failed to load installed module graph from {}; using legacy backend discovery: {err}",
                    graph_path.display()
                );
                for mut candidate in
                    legacy_backend_candidates_from_discovery(&self.modules, &self.config)
                {
                    self.apply_shell_runtime_settings(&mut candidate);
                    self.spawn_backend_candidate(runtime, tx.clone(), candidate);
                }
            }
        }
    }

    pub(super) fn apply_shell_runtime_settings(&self, candidate: &mut BackendLaunchCandidate) {
        if candidate.interface != "mesh.theme" {
            return;
        }

        let current_theme = self.theme.active().id.clone();
        if let Some(settings) = candidate.settings.as_object_mut() {
            settings.insert(
                "current_theme".to_string(),
                serde_json::Value::String(current_theme),
            );
        } else {
            candidate.settings = serde_json::json!({
                "current_theme": current_theme,
            });
        }
    }

    fn spawn_backend_candidate(
        &mut self,
        runtime: &Runtime,
        tx: mpsc::UnboundedSender<ShellMessage>,
        candidate: BackendLaunchCandidate,
    ) {
        self.stop_backend_runtime(&candidate.interface);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let shell_tx = tx.clone();
        let interface = candidate.interface.clone();
        let provider_id = candidate.module_id.clone();
        let (backend_tx, mut backend_rx) = mpsc::unbounded_channel::<BackendServiceEvent>();
        let bridge_interface = interface.clone();
        let bridge_provider_id = provider_id.clone();
        runtime.spawn(async move {
            while let Some(event) = backend_rx.recv().await {
                match event {
                    BackendServiceEvent::Update(update) => {
                        if shell_tx
                            .send(ShellMessage::Service(ServiceEvent::Updated {
                                service: update.service,
                                source_module: update.source_module,
                                payload: update.payload,
                            }))
                            .is_err()
                        {
                            break;
                        }
                    }
                    BackendServiceEvent::CommandResult(result) => {
                        tracing::debug!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            command = result.command.as_str(),
                            result = %result.result,
                            "backend command result"
                        );
                    }
                    BackendServiceEvent::Started { .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage: "runtime".to_string(),
                            status: "running".to_string(),
                            message: "backend runtime started".to_string(),
                        });
                        tracing::info!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "backend runtime started"
                        );
                    }
                    BackendServiceEvent::InitFailed { message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage: "init".to_string(),
                            status: "init_failed".to_string(),
                            message: message.clone(),
                        });
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::PollFailed { message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage: "poll".to_string(),
                            status: "poll_failed".to_string(),
                            message: message.clone(),
                        });
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::Failed { stage, message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage,
                            status: "failed".to_string(),
                            message: message.clone(),
                        });
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::Stopped { .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            stage: "runtime".to_string(),
                            status: "stopped".to_string(),
                            message: "backend runtime stopped".to_string(),
                        });
                        tracing::info!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "backend runtime stopped"
                        );
                    }
                }
            }
        });
        let task = runtime.spawn(spawn_backend_service(
            candidate.module_id,
            candidate.service_name,
            candidate.capabilities,
            candidate.settings,
            candidate.script_source,
            backend_tx,
            cmd_rx,
        ));
        self.replace_backend_runtime(
            interface.clone(),
            BackendRuntimeSlot {
                interface,
                provider_id,
                command_tx: cmd_tx,
                task: task.abort_handle(),
            },
        );
    }
}

pub(super) fn backend_launch_candidates_from_graph(
    graph: &InstalledModuleGraph,
    modules: &HashMap<String, ModuleInstance>,
    config: &ShellConfig,
    interfaces: &InterfaceRegistry,
) -> (
    Vec<BackendLaunchCandidate>,
    Vec<BackendLifecycleStatusRecord>,
) {
    let mut statuses = backend_requirement_statuses(graph);
    let mut interface_names: Vec<String> = graph
        .backend_modules()
        .into_iter()
        .flat_map(|module| {
            module
                .manifest
                .mesh
                .implementations()
                .map(|provided| provided.interface.clone())
                .collect::<Vec<_>>()
        })
        .collect();
    interface_names.sort();
    interface_names.dedup();

    let mut candidates = Vec::new();
    for interface in interface_names {
        let Some(active_provider) = graph.active_provider(&interface) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface,
                provider_id: None,
                status: "no_active_provider",
                message: "no active provider selected".to_string(),
            });
            continue;
        };

        let Some(module) = graph.module(&active_provider.module_id) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} is not installed",
                    active_provider.module_id
                ),
            });
            continue;
        };

        if !module.enabled || module.kind != ModuleKind::Backend {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} is not an enabled backend module",
                    active_provider.module_id
                ),
            });
            continue;
        }

        if !module
            .manifest
            .mesh
            .implementations()
            .any(|provided| provided.interface == interface)
        {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} does not declare interface {interface}",
                    active_provider.module_id
                ),
            });
            continue;
        }

        if let Some(status) =
            validate_backend_provider_contract(&interface, &active_provider.module_id, interfaces)
        {
            statuses.push(status);
            continue;
        }

        let Some(module) = modules.get(&active_provider.module_id) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} has no discovered runtime manifest",
                    active_provider.module_id
                ),
            });
            continue;
        };

        if let Some(binary) = module
            .manifest
            .dependencies
            .binaries
            .iter()
            .find(|binary| !binary.optional && !binary_exists(&binary.name))
        {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_binary",
                message: format!(
                    "backend provider {} requires unavailable binary {}",
                    active_provider.module_id, binary.name
                ),
            });
            continue;
        }

        let entrypoint = module.manifest.entrypoints.main.as_deref();
        let Some(entrypoint) = entrypoint else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_entrypoint",
                message: format!(
                    "backend provider {} has no service entrypoint",
                    active_provider.module_id
                ),
            });
            continue;
        };

        let entrypoint_path = module.path.join(entrypoint);
        let Ok(script_source) = std::fs::read_to_string(&entrypoint_path) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_entrypoint",
                message: format!(
                    "backend provider {} entrypoint is unreadable: {}",
                    active_provider.module_id,
                    entrypoint_path.display()
                ),
            });
            continue;
        };

        let capabilities = module
            .manifest
            .capabilities
            .required
            .iter()
            .chain(module.manifest.capabilities.optional.iter())
            .cloned()
            .collect::<Vec<_>>();
        let settings = backend_module_settings_json(config, &active_provider.module_id);
        candidates.push(BackendLaunchCandidate {
            module_id: active_provider.module_id.clone(),
            interface: interface.clone(),
            service_name: service_name_from_interface(&interface),
            entrypoint_path,
            script_source,
            capabilities,
            settings,
        });
    }

    (candidates, statuses)
}

fn backend_requirement_statuses(graph: &InstalledModuleGraph) -> Vec<BackendLifecycleStatusRecord> {
    let mut statuses = Vec::new();
    for frontend in graph.frontend_modules() {
        let Some(requirements) = graph.requirements_for_frontend(&frontend.id) else {
            continue;
        };
        for interface in requirements.backend.keys() {
            if graph.backend_providers_for_interface(interface).is_empty() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "unmet_backend_requirement",
                    message: format!(
                        "frontend module {} requires {interface}, but no enabled backend provider is installed",
                        frontend.id
                    ),
                });
            } else if graph.active_provider(interface).is_none() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "no_active_provider",
                    message: format!(
                        "frontend module {} requires {interface}, but no active provider is selected",
                        frontend.id
                    ),
                });
            }
        }
    }
    statuses
}

fn validate_backend_provider_contract(
    interface: &str,
    provider_id: &str,
    interfaces: &InterfaceRegistry,
) -> Option<BackendLifecycleStatusRecord> {
    let resolution = interfaces.resolve(interface, None);
    if resolution.contract.is_none() && resolution.provider.is_none() {
        return None;
    }

    if resolution.contract.is_none() {
        return Some(BackendLifecycleStatusRecord {
            interface: canonical_interface_name(interface),
            provider_id: Some(provider_id.to_string()),
            status: "invalid_manifest",
            message: format!("active provider {provider_id} has no interface contract"),
        });
    }

    if !interfaces
        .providers_for(interface)
        .iter()
        .any(|provider| provider.provider_module == provider_id)
    {
        return Some(BackendLifecycleStatusRecord {
            interface: canonical_interface_name(interface),
            provider_id: Some(provider_id.to_string()),
            status: "invalid_manifest",
            message: format!(
                "active provider {provider_id} is not registered for interface {}",
                canonical_interface_name(interface)
            ),
        });
    }

    None
}

fn legacy_backend_candidates_from_discovery(
    modules: &HashMap<String, ModuleInstance>,
    config: &ShellConfig,
) -> Vec<BackendLaunchCandidate> {
    let mut module_ids: Vec<String> = modules.keys().cloned().collect();
    module_ids.sort();
    let mut services: HashMap<String, Vec<(&ModuleInstance, u32)>> = HashMap::new();

    for module_id in module_ids {
        let Some(module) = modules.get(&module_id) else {
            continue;
        };
        if module.manifest.package.module_type != ModuleType::Backend {
            continue;
        }
        let Some(service) = module.manifest.primary_service() else {
            continue;
        };
        let service_name = service_name_from_interface(&service.provides);
        services
            .entry(service_name)
            .or_default()
            .push((module, service.priority));
    }

    let mut candidates = Vec::new();
    for (service_name, mut service_candidates) in services {
        service_candidates.sort_by(|(a, a_priority), (b, b_priority)| {
            b_priority
                .cmp(a_priority)
                .then_with(|| a.manifest.package.id.cmp(&b.manifest.package.id))
        });
        let Some((module, _)) = service_candidates.into_iter().next() else {
            continue;
        };
        let missing_binary = module
            .manifest
            .dependencies
            .binaries
            .iter()
            .find(|binary| !binary.optional && !binary_exists(&binary.name));
        if let Some(binary) = missing_binary {
            tracing::info!(
                "skipping legacy backend '{}' for service '{}' because binary '{}' is unavailable",
                module.manifest.package.id,
                service_name,
                binary.name
            );
            continue;
        }
        let Some(entrypoint) = module.manifest.entrypoints.main.as_deref() else {
            tracing::warn!(
                "legacy backend module {} has no service entrypoint",
                module.manifest.package.id
            );
            continue;
        };
        let entrypoint_path = module.path.join(entrypoint);
        let Ok(script_source) = std::fs::read_to_string(&entrypoint_path) else {
            tracing::warn!(
                "legacy backend module {} has no readable script at {}",
                module.manifest.package.id,
                entrypoint_path.display()
            );
            continue;
        };
        let capabilities = module
            .manifest
            .capabilities
            .required
            .iter()
            .chain(module.manifest.capabilities.optional.iter())
            .cloned()
            .collect::<Vec<_>>();
        candidates.push(BackendLaunchCandidate {
            module_id: module.manifest.package.id.clone(),
            interface: format!("mesh.{service_name}"),
            service_name,
            entrypoint_path,
            script_source,
            capabilities,
            settings: backend_module_settings_json(config, &module.manifest.package.id),
        });
    }

    candidates
}

fn binary_exists(name: &str) -> bool {
    if name.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(name).is_file();
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths).any(|dir| dir.join(name).is_file())
}

fn backend_module_settings_json(config: &ShellConfig, module_id: &str) -> serde_json::Value {
    config
        .modules
        .get(module_id)
        .map(|module| match serde_json::to_value(&module.values) {
            Ok(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
            Ok(_) => serde_json::json!({}),
            Err(err) => {
                tracing::warn!(
                    module_id = module_id,
                    "failed to serialize backend module settings: {err}"
                );
                serde_json::json!({})
            }
        })
        .unwrap_or_else(|| serde_json::json!({}))
}
