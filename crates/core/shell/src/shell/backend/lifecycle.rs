use super::super::*;
use super::{BackendRuntimeStatus, BackendRuntimeStatusEntry};
use mesh_core_module::manifest::ModuleType;

impl Shell {
    pub(in crate::shell) fn backend_runtime_status(
        &self,
        interface: &str,
        provider_id: &str,
    ) -> Option<&BackendRuntimeStatusEntry> {
        self.backend_runtime_statuses
            .get(interface)
            .and_then(|providers| providers.get(provider_id))
    }

    pub(in crate::shell) fn record_backend_runtime_status(
        &mut self,
        interface: String,
        provider_id: String,
        status: BackendRuntimeStatus,
        message: String,
    ) {
        self.record_backend_runtime_status_with_health(
            interface,
            provider_id,
            status,
            message,
            true,
        );
    }

    fn record_backend_runtime_status_with_health(
        &mut self,
        interface: String,
        provider_id: String,
        status: BackendRuntimeStatus,
        message: String,
        publish_health: bool,
    ) {
        let is_failure = matches!(
            status,
            BackendRuntimeStatus::InvalidManifest
                | BackendRuntimeStatus::MissingEntrypoint
                | BackendRuntimeStatus::MissingBinary
                | BackendRuntimeStatus::InitFailed
                | BackendRuntimeStatus::PollFailed
                | BackendRuntimeStatus::Failed
                | BackendRuntimeStatus::Quarantined
        );
        if is_failure {
            self.diagnostics.record_lifecycle_error(
                provider_id.clone(),
                status.as_str(),
                message.clone(),
            );
        } else if status == BackendRuntimeStatus::Running {
            self.diagnostics.resolve_lifecycle_errors(&provider_id);
        }
        let prev_failure_count = self
            .backend_runtime_status(&interface, &provider_id)
            .map(|entry| entry.failure_count)
            .unwrap_or(0);
        let failure_count = if is_failure {
            prev_failure_count + 1
        } else {
            prev_failure_count
        };
        self.backend_runtime_statuses
            .entry(interface.clone())
            .or_default()
            .insert(
                provider_id.clone(),
                BackendRuntimeStatusEntry {
                    interface: interface.clone(),
                    provider_id: provider_id.clone(),
                    status,
                    message: message.clone(),
                    failure_count,
                },
            );
        self.update_module_runtime_lifecycle(&provider_id, status, &message);
        self.publish_backend_health(&interface, &provider_id, status, &message, publish_health);
    }

    fn update_module_runtime_lifecycle(
        &mut self,
        provider_id: &str,
        status: BackendRuntimeStatus,
        message: &str,
    ) {
        let Some(module) = self.modules.get_mut(provider_id) else {
            return;
        };
        if module.manifest.package.module_type != ModuleType::Backend {
            return;
        }
        let result = match status {
            BackendRuntimeStatus::Running => module.mark_running(),
            BackendRuntimeStatus::PollFailed => {
                module.mark_degraded(message.to_string());
                Ok(())
            }
            BackendRuntimeStatus::Stopped => module.mark_unloaded(),
            BackendRuntimeStatus::Quarantined => module.mark_quarantined(message.to_string()),
            BackendRuntimeStatus::OptionalBackendUnavailable
            | BackendRuntimeStatus::OptionalBackendInactive => {
                module.mark_degraded(message.to_string());
                Ok(())
            }
            BackendRuntimeStatus::InvalidManifest
            | BackendRuntimeStatus::MissingCapability
            | BackendRuntimeStatus::MissingEntrypoint
            | BackendRuntimeStatus::MissingBinary
            | BackendRuntimeStatus::InitFailed
            | BackendRuntimeStatus::Failed
            | BackendRuntimeStatus::NoActiveProvider
            | BackendRuntimeStatus::UnmetBackendRequirement => {
                module.mark_failed(message.to_string())
            }
        };
        if let Err(error) = result {
            tracing::debug!(
                provider_id,
                "module lifecycle state did not accept backend status: {error}"
            );
        }
    }

    /// Publish one authoritative availability transition to both ordinary
    /// service observers and subscribers of the reserved `health` event. The
    /// provider cache is updated before delivery, so a newly mounted runtime
    /// and an already-mounted runtime see the same terminal state.
    fn publish_backend_health(
        &mut self,
        interface: &str,
        provider_id: &str,
        status: BackendRuntimeStatus,
        message: &str,
        deliver: bool,
    ) {
        let current_provider = self
            .backend_runtimes
            .get(interface)
            .map(|slot| slot.provider_id.as_str())
            .or_else(|| {
                self.pending_backend_runtimes
                    .get(interface)
                    .map(|pending| pending.slot.provider_id.as_str())
            });
        let pending_is_not_active =
            self.pending_backend_runtimes
                .get(interface)
                .is_some_and(|pending| {
                    pending.slot.provider_id == provider_id
                        && !self
                            .backend_runtimes
                            .get(interface)
                            .is_some_and(|slot| slot.provider_id == provider_id)
                });
        if pending_is_not_active {
            tracing::debug!(
                interface,
                provider_id,
                "kept candidate provider health private until activation commit"
            );
            return;
        }
        if current_provider.is_some_and(|current| current != provider_id)
            || current_provider.is_none()
                && provider_id != "<none>"
                && self
                    .latest_service_state
                    .get(interface)
                    .is_some_and(|latest| latest.provider_id != provider_id)
        {
            tracing::debug!(
                interface,
                provider_id,
                "ignored health transition from an inactive provider"
            );
            return;
        }
        let (health_state, recoverable, available) = match status {
            BackendRuntimeStatus::Running => ("healthy", true, Some(true)),
            BackendRuntimeStatus::PollFailed => ("degraded", true, None),
            BackendRuntimeStatus::Quarantined => ("unavailable", false, Some(false)),
            BackendRuntimeStatus::NoActiveProvider
            | BackendRuntimeStatus::UnmetBackendRequirement
            | BackendRuntimeStatus::OptionalBackendUnavailable
            | BackendRuntimeStatus::OptionalBackendInactive
            | BackendRuntimeStatus::InvalidManifest
            | BackendRuntimeStatus::MissingCapability
            | BackendRuntimeStatus::MissingEntrypoint
            | BackendRuntimeStatus::MissingBinary
            | BackendRuntimeStatus::InitFailed
            | BackendRuntimeStatus::Failed
            | BackendRuntimeStatus::Stopped => ("unavailable", true, Some(false)),
        };

        if let Some(available) = available {
            let mut payload = self
                .latest_service_state
                .get(interface)
                .map(|latest| latest.state.clone())
                .unwrap_or_else(|| serde_json::json!({}));
            if !payload.is_object() {
                payload = serde_json::json!({});
            }
            if let Some(object) = payload.as_object_mut() {
                object.insert("available".to_string(), serde_json::Value::Bool(available));
                if !available {
                    object.insert(
                        "availability_reason".to_string(),
                        serde_json::Value::String(message.to_string()),
                    );
                }
            }
            let event = ServiceEvent::Updated {
                service: interface.to_string(),
                source_module: provider_id.to_string(),
                payload: payload.clone(),
            };
            self.latest_service_state.insert(
                interface.to_string(),
                LatestServiceState::new(interface.to_string(), provider_id.to_string(), payload),
            );
            if deliver {
                match self.deliver_service_event(&event) {
                    Ok(requests) => self.deferred_requests.extend(requests),
                    Err(error) => tracing::warn!(
                        interface,
                        provider_id,
                        "failed to deliver backend availability transition: {error}"
                    ),
                }
            }
        }

        let health_event = ServiceEvent::InterfaceEvent {
            service: interface.to_string(),
            source_module: provider_id.to_string(),
            name: "health".to_string(),
            payload: serde_json::json!({
                "interface": interface,
                "provider_id": provider_id,
                "state": health_state,
                "reason": message,
                "recoverable": recoverable,
            }),
        };
        self.latest_service_health
            .insert(interface.to_string(), health_event.clone());
        if deliver {
            match self.deliver_service_event(&health_event) {
                Ok(requests) => self.deferred_requests.extend(requests),
                Err(error) => tracing::warn!(
                    interface,
                    provider_id,
                    "failed to deliver backend health transition: {error}"
                ),
            }
        }
    }

    pub(in crate::shell) fn stop_backend_runtime(&mut self, interface: &str) {
        self.stop_backend_runtime_with_health(interface, false);
    }

    fn stop_backend_runtime_with_health(&mut self, interface: &str, publish_health: bool) {
        self.service_handlers.remove(interface);
        if let Some(slot) = self.backend_runtimes.remove(interface) {
            slot.task.abort();
            self.rollback_bound_service_states_for_provider(&slot.interface, &slot.provider_id);
            let terminal_failure_already_recorded = self
                .backend_runtime_status(&slot.interface, &slot.provider_id)
                .map(|entry| {
                    matches!(
                        entry.status,
                        BackendRuntimeStatus::InitFailed | BackendRuntimeStatus::Failed
                    )
                })
                .unwrap_or(false);
            if !terminal_failure_already_recorded {
                self.record_backend_runtime_status_with_health(
                    slot.interface,
                    slot.provider_id,
                    BackendRuntimeStatus::Stopped,
                    "runtime stopped".to_string(),
                    publish_health,
                );
            }
        }
    }

    pub(in crate::shell) fn replace_backend_runtime(
        &mut self,
        interface: String,
        slot: BackendRuntimeSlot,
    ) {
        if let Some(state) = self.backend_supervision.get_mut(&interface) {
            state.invalidate_pending_restart();
        }
        // A ready replacement takes over without an observable unavailable
        // gap between the old and new provider.
        self.stop_backend_runtime_with_health(&interface, false);
        self.service_handlers
            .insert(interface.clone(), slot.command_tx.clone());
        self.backend_runtimes.insert(interface, slot);
    }

    pub(in crate::shell) fn stage_backend_runtime_switch(
        &mut self,
        interface: String,
        slot: BackendRuntimeSlot,
        graph_path: PathBuf,
    ) {
        if let Some(previous) = self.pending_backend_runtimes.remove(&interface) {
            previous.slot.task.abort();
            self.record_backend_runtime_status(
                previous.slot.interface,
                previous.slot.provider_id,
                BackendRuntimeStatus::Stopped,
                "superseded by a newer provider switch".to_string(),
            );
        }
        self.pending_backend_runtimes
            .insert(interface, PendingBackendRuntime { slot, graph_path });
    }

    fn complete_backend_runtime_switch(&mut self, interface: &str, provider_id: &str) {
        let Some(pending) = self.pending_backend_runtimes.remove(interface) else {
            return;
        };
        if pending.slot.provider_id != provider_id {
            self.pending_backend_runtimes
                .insert(interface.to_string(), pending);
            return;
        }

        if let Err(error) = crate::shell::module_config::write_composed_provider_selection(
            &pending.graph_path,
            interface,
            provider_id,
        ) {
            pending.slot.task.abort();
            let message = format!(
                "provider {provider_id} became ready for {interface}, but its selection could not be saved: {error}"
            );
            self.record_backend_runtime_status(
                interface.to_string(),
                provider_id.to_string(),
                BackendRuntimeStatus::Failed,
                message.clone(),
            );
            self.diagnostics.record_lifecycle_error(
                "@mesh/settings".to_string(),
                "provider_selection_write_failed",
                message.clone(),
            );
            tracing::warn!(interface, provider_id, "{message}");
            return;
        }

        let candidate_graph = match self.load_installed_module_graph_candidate() {
            Ok(graph) => graph,
            Err(error) => {
                pending.slot.task.abort();
                let message = format!(
                    "provider selection was saved but the candidate graph could not be refreshed: {error}"
                );
                tracing::warn!(interface, provider_id, "{message}");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/shell".to_string(),
                    "provider_selection_graph_reload_failed",
                    message,
                );
                return;
            }
        };
        self.commit_installed_module_graph(candidate_graph);
        self.backend_supervision.remove(interface);
        self.replace_backend_runtime(interface.to_string(), pending.slot);
        self.note_backend_running(interface);
        tracing::info!(
            interface,
            provider_id,
            "switched active backend provider live"
        );
    }

    pub(in crate::shell) fn handle_backend_lifecycle(
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
        if self.handle_profile_backend_lifecycle(&interface, &provider_id, runtime_status) {
            return;
        }
        let event_provider_is_pending = self
            .pending_backend_runtimes
            .get(&interface)
            .is_some_and(|pending| pending.slot.provider_id == provider_id);
        if event_provider_is_pending {
            if runtime_status == BackendRuntimeStatus::Running {
                self.complete_backend_runtime_switch(&interface, &provider_id);
            } else if matches!(
                runtime_status,
                BackendRuntimeStatus::InitFailed
                    | BackendRuntimeStatus::Failed
                    | BackendRuntimeStatus::Stopped
            ) && let Some(pending) = self.pending_backend_runtimes.remove(&interface)
            {
                pending.slot.task.abort();
                tracing::warn!(
                    interface,
                    provider_id,
                    stage,
                    "provider switch failed; keeping the current runtime active"
                );
            }
            return;
        }
        let event_provider_is_current = self
            .backend_runtimes
            .get(&interface)
            .is_some_and(|slot| slot.provider_id == provider_id);
        if runtime_status == BackendRuntimeStatus::Running && event_provider_is_current {
            self.note_backend_running(&interface);
        }
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
            self.supervise_backend_failure(&interface, &provider_id);
        }
    }

    /// Replace `latest_service_state` for the given interface with an unavailable
    /// payload when the active provider is known to be failing.
    fn clear_active_provider_service_state(&mut self, interface: &str, provider_id: &str) {
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
            LatestServiceState::new(
                interface.to_string(),
                provider_id.to_string(),
                unavailable_payload,
            ),
        );
        tracing::debug!(
            interface,
            provider_id,
            "cleared stale public service state after provider failure"
        );
    }
}
