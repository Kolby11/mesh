use super::super::*;
use super::{BackendRuntimeStatus, BackendRuntimeStatusEntry};

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
                    interface,
                    provider_id,
                    status,
                    message,
                    failure_count,
                },
            );
    }

    pub(in crate::shell) fn stop_backend_runtime(&mut self, interface: &str) {
        self.service_handlers.remove(interface);
        if let Some(slot) = self.backend_runtimes.remove(interface) {
            slot.task.abort();
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
                self.record_backend_runtime_status(
                    slot.interface,
                    slot.provider_id,
                    BackendRuntimeStatus::Stopped,
                    "runtime stopped".to_string(),
                );
            }
        }
    }

    pub(in crate::shell) fn replace_backend_runtime(
        &mut self,
        interface: String,
        slot: BackendRuntimeSlot,
    ) {
        self.stop_backend_runtime(&interface);
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

        if let Err(error) = crate::shell::module_config::write_active_provider_selection(
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

        self.installed_module_graph = None;
        if let Err(error) = self.load_installed_module_graph_cached() {
            tracing::warn!(
                interface,
                provider_id,
                "provider selection was saved but the installed graph cache could not be refreshed: {error}"
            );
        }
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
