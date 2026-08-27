use super::super::*;
use super::candidates::backend_launch_candidates_from_graph_with_capabilities;
use super::{BackendLaunchCandidate, BackendRuntimeStatus};
use rustix::fd::BorrowedFd;

impl Shell {
    pub(in crate::shell) fn next_backend_identity(
        &mut self,
        interface: &str,
        activation_generation: u64,
    ) -> mesh_core_backend::BackendIdentity {
        let epoch = self
            .backend_provider_epochs
            .entry(interface.to_string())
            .and_modify(|epoch| *epoch = epoch.saturating_add(1).max(1))
            .or_insert(1);
        mesh_core_backend::BackendIdentity::new(activation_generation, *epoch)
    }

    pub(in crate::shell) fn spawn_backend_modules(
        &mut self,
        runtime: &tokio::runtime::Handle,
        tx: mpsc::UnboundedSender<ShellMessage>,
        eventfd_fd: std::os::unix::io::RawFd,
    ) {
        let graph_path = self.installed_module_graph_path();
        match self.load_installed_module_graph_cached() {
            Ok(graph) => {
                let graph = graph.clone();
                let (candidates, statuses) = backend_launch_candidates_from_graph_with_capabilities(
                    &graph,
                    &self.modules,
                    &self.settings_store,
                    &self.interfaces,
                    Some(&self.effective_capabilities),
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
                    if matches!(
                        status.status,
                        "optional_backend_unavailable" | "optional_backend_inactive"
                    ) {
                        tracing::debug!(
                            interface = status.interface,
                            provider_id = status.provider_id.as_deref().unwrap_or("<none>"),
                            status = status.status,
                            "{}",
                            status.message
                        );
                    } else {
                        tracing::warn!(
                            interface = status.interface,
                            provider_id = status.provider_id.as_deref().unwrap_or("<none>"),
                            status = status.status,
                            "{}",
                            status.message
                        );
                    }
                }
                for mut candidate in candidates {
                    self.apply_shell_runtime_settings(&mut candidate);
                    self.spawn_backend_candidate(runtime, tx.clone(), candidate, eventfd_fd);
                }
            }
            Err(err) => {
                let message = format!(
                    "failed to load installed module graph from {}; no backend services started: {err}",
                    graph_path.display()
                );
                tracing::error!("{message}");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/shell".to_string(),
                    "module_graph_load_failed",
                    message,
                );
            }
        }
    }

    /// Inject the generic `__shell` context into every backend's settings:
    /// ambient shell-owned state (active theme, locale) any provider may read
    /// via `mesh.config().__shell`. This is deliberately service-agnostic —
    /// core never injects per-interface values.
    pub(in crate::shell) fn apply_shell_runtime_settings(
        &self,
        candidate: &mut BackendLaunchCandidate,
    ) {
        Self::apply_runtime_settings(candidate, &self.theme.active().id, self.locale.current());
    }

    pub(in crate::shell) fn apply_runtime_settings(
        candidate: &mut BackendLaunchCandidate,
        theme: &str,
        locale: &str,
    ) {
        let shell_context = serde_json::json!({
            "theme": theme,
            "locale": locale,
        });
        if let Some(settings) = candidate.settings.as_object_mut() {
            settings.insert("__shell".to_string(), shell_context);
        } else {
            candidate.settings = serde_json::json!({ "__shell": shell_context });
        }
    }

    pub(in crate::shell) fn spawn_backend_candidate(
        &mut self,
        runtime: &tokio::runtime::Handle,
        tx: mpsc::UnboundedSender<ShellMessage>,
        candidate: BackendLaunchCandidate,
        eventfd_fd: std::os::unix::io::RawFd,
    ) {
        let interface = candidate.interface.clone();
        let slot = self.start_backend_candidate(runtime, tx, candidate, eventfd_fd);
        self.stage_backend_runtime_activation(interface, slot);
    }

    /// Start a backend without publishing it as the active command handler.
    /// Provider switches use this to keep the old runtime serving until the
    /// candidate has completed its script initialization.
    pub(in crate::shell) fn start_backend_candidate(
        &mut self,
        runtime: &tokio::runtime::Handle,
        tx: mpsc::UnboundedSender<ShellMessage>,
        candidate: BackendLaunchCandidate,
        eventfd_fd: std::os::unix::io::RawFd,
    ) -> BackendRuntimeSlot {
        let event_provider_id = candidate.module_id.clone();
        let identity = self.next_backend_identity(&candidate.interface, self.activation_generation);
        self.start_backend_candidate_with_event_id(
            runtime,
            tx,
            candidate,
            eventfd_fd,
            event_provider_id,
            identity,
        )
    }

    pub(in crate::shell) fn start_backend_candidate_with_event_id(
        &mut self,
        runtime: &tokio::runtime::Handle,
        tx: mpsc::UnboundedSender<ShellMessage>,
        candidate: BackendLaunchCandidate,
        eventfd_fd: std::os::unix::io::RawFd,
        initial_event_provider_id: String,
        identity: mesh_core_backend::BackendIdentity,
    ) -> BackendRuntimeSlot {
        if let Some(module) = self.modules.get_mut(&candidate.module_id) {
            module.clear_quarantine();
            if let Err(error) = module.mark_loaded() {
                tracing::debug!(
                    module_id = candidate.module_id.as_str(),
                    "backend candidate did not enter loaded state: {error}"
                );
            }
        }
        let generation = mesh_core_backend::next_runtime_generation();
        let (cmd_tx, cmd_rx) = mpsc::channel(mesh_core_backend::BACKEND_COMMAND_QUEUE_CAPACITY);

        let shell_tx = tx.clone();
        let interface = candidate.interface.clone();
        let provider_id = candidate.module_id.clone();
        let event_provider_id = Arc::new(std::sync::RwLock::new(initial_event_provider_id));
        let bridge_event_provider_id = event_provider_id.clone();
        let identity_handle = Arc::new(RwLock::new(identity));
        let bridge_identity_handle = identity_handle.clone();
        let (backend_tx, mut backend_rx) =
            mpsc::channel::<BackendServiceEvent>(mesh_core_backend::BACKEND_EVENT_QUEUE_CAPACITY);
        let bridge_interface = interface.clone();
        let bridge_provider_id = provider_id.clone();
        let bridge_task = runtime.spawn(async move {
            while let Some(event) = backend_rx.recv().await {
                let current_event_provider_id = bridge_event_provider_id
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                match event {
                    BackendServiceEvent::Update(update) => {
                        if shell_tx
                            .send(ShellMessage::BackendServiceUpdate {
                                interface: bridge_interface.clone(),
                                provider_id: current_event_provider_id.clone(),
                                identity: update.identity,
                                event: ServiceEvent::Updated {
                                    service: update.service.to_string(),
                                    source_module: update.source_module.to_string(),
                                    payload: update.payload,
                                },
                            })
                            .is_err()
                        {
                            break;
                        }
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                    }
                    BackendServiceEvent::CommandResult(result) => {
                        let call_id = result.call_id;
                        let command = result.command;
                        let payload = result.result;
                        let outcome = result.outcome;
                        let generation = result.generation;
                        let identity = result.identity;
                        tracing::debug!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            command = command.as_str(),
                            result = %payload,
                            "backend command result"
                        );
                        let _ = shell_tx.send(ShellMessage::BackendCommandResult {
                            interface: bridge_interface.clone(),
                            provider_id: current_event_provider_id.clone(),
                            identity,
                            generation,
                            call_id,
                            command,
                            result: payload,
                            outcome,
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                    }
                    BackendServiceEvent::InterfaceEvent(event) => {
                        let name = event.name;
                        let payload = event.payload;
                        let generation = event.generation;
                        let identity = event.identity;
                        tracing::debug!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            event = name.as_str(),
                            payload = %payload,
                            "backend interface event"
                        );
                        let _ = shell_tx.send(ShellMessage::BackendInterfaceEvent {
                            interface: bridge_interface.clone(),
                            provider_id: current_event_provider_id.clone(),
                            identity,
                            name,
                            payload,
                            generation,
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                    }
                    BackendServiceEvent::Started { identity, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: current_event_provider_id.clone(),
                            identity,
                            stage: "runtime".to_string(),
                            status: "running".to_string(),
                            message: "backend runtime started".to_string(),
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                        tracing::info!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "backend runtime started"
                        );
                    }
                    BackendServiceEvent::InitFailed {
                        message, identity, ..
                    } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: current_event_provider_id.clone(),
                            identity,
                            stage: "init".to_string(),
                            status: "init_failed".to_string(),
                            message: message.clone(),
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::PollFailed {
                        message, identity, ..
                    } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: current_event_provider_id.clone(),
                            identity,
                            stage: "poll".to_string(),
                            status: "poll_failed".to_string(),
                            message: message.clone(),
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::Failed {
                        stage,
                        message,
                        identity,
                        ..
                    } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: current_event_provider_id.clone(),
                            identity,
                            stage,
                            status: "failed".to_string(),
                            message: message.clone(),
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                        tracing::warn!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "{message}"
                        );
                    }
                    BackendServiceEvent::Stopped { identity, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: current_event_provider_id.clone(),
                            identity,
                            stage: "runtime".to_string(),
                            status: "stopped".to_string(),
                            message: "backend runtime stopped".to_string(),
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                        tracing::info!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            "backend runtime stopped"
                        );
                    }
                }
            }
        });
        let task = runtime.spawn(
            mesh_core_backend::spawn_backend_service_bounded_with_events_and_queue_with_identity(
                candidate.module_id,
                candidate.service_name,
                candidate.capabilities,
                candidate.settings,
                candidate.script_source,
                backend_tx,
                cmd_rx,
                candidate.command_registry,
                candidate.event_registry,
                generation,
                identity,
                identity_handle,
            ),
        );
        BackendRuntimeSlot {
            interface,
            provider_id,
            event_provider_id,
            identity: bridge_identity_handle,
            generation,
            command_tx: cmd_tx,
            task: task.abort_handle(),
            tasks: Some(BackendRuntimeTasks::new(task, bridge_task)),
        }
    }
}
