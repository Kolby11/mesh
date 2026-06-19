use super::super::*;
use super::candidates::{
    backend_launch_candidates_from_graph, legacy_backend_candidates_from_discovery,
};
use super::{BackendLaunchCandidate, BackendRuntimeStatus};
use rustix::fd::BorrowedFd;

impl Shell {
    pub(in crate::shell) fn spawn_backend_modules(
        &mut self,
        runtime: &Runtime,
        tx: mpsc::UnboundedSender<ShellMessage>,
        eventfd_fd: std::os::unix::io::RawFd,
    ) {
        let graph_path = self.installed_module_graph_path();
        match self.load_installed_module_graph_cached() {
            Ok(graph) => {
                let graph = graph.clone();
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
                    self.spawn_backend_candidate(runtime, tx.clone(), candidate, eventfd_fd);
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
                    self.spawn_backend_candidate(runtime, tx.clone(), candidate, eventfd_fd);
                }
            }
        }
    }

    pub(in crate::shell) fn apply_shell_runtime_settings(
        &self,
        candidate: &mut BackendLaunchCandidate,
    ) {
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
        eventfd_fd: std::os::unix::io::RawFd,
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
                            .send(ShellMessage::BackendServiceUpdate {
                                interface: bridge_interface.clone(),
                                provider_id: bridge_provider_id.clone(),
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
                        let command = result.command;
                        let payload = result.result;
                        tracing::debug!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            command = command.as_str(),
                            result = %payload,
                            "backend command result"
                        );
                        let _ = shell_tx.send(ShellMessage::BackendCommandResult {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            command,
                            result: payload,
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                    }
                    BackendServiceEvent::InterfaceEvent(event) => {
                        let name = event.name;
                        let payload = event.payload;
                        tracing::debug!(
                            interface = bridge_interface.as_str(),
                            provider_id = bridge_provider_id.as_str(),
                            event = name.as_str(),
                            payload = %payload,
                            "backend interface event"
                        );
                        let _ = shell_tx.send(ShellMessage::BackendInterfaceEvent {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
                            name,
                            payload,
                        });
                        let evfd = unsafe { BorrowedFd::borrow_raw(eventfd_fd) };
                        let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
                    }
                    BackendServiceEvent::Started { .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
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
                    BackendServiceEvent::InitFailed { message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
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
                    BackendServiceEvent::PollFailed { message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
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
                    BackendServiceEvent::Failed { stage, message, .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
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
                    BackendServiceEvent::Stopped { .. } => {
                        let _ = shell_tx.send(ShellMessage::BackendLifecycle {
                            interface: bridge_interface.clone(),
                            provider_id: bridge_provider_id.clone(),
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
