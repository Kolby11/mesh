use super::super::*;
use super::{BackendRuntimeStatus, BackendRuntimeStatusEntry};
use mesh_core_module::manifest::ModuleType;
use std::time::Duration;
use tokio::task::JoinHandle;

const BACKEND_STOP_DEADLINE: Duration = Duration::from_secs(2);

impl Shell {
    /// Request every live backend to stop through its command channel and
    /// synchronously join the service and bridge tasks. The command sender is
    /// dropped before waiting so the backend guard can invoke authored `stop`,
    /// flush storage, and publish its terminal lifecycle event.
    pub(in crate::shell) fn shutdown_backend_runtimes(&mut self, runtime: &Runtime) {
        self.service_handlers.clear();

        let active_modules = self
            .backend_runtimes
            .values()
            .map(|slot| {
                let identity = *slot
                    .identity
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                (slot.interface.clone(), slot.provider_id.clone(), identity)
            })
            .collect::<Vec<_>>();
        let mut slots = self
            .backend_runtimes
            .drain()
            .map(|(_, slot)| slot)
            .collect::<Vec<_>>();
        slots.extend(
            self.pending_backend_runtimes
                .drain()
                .map(|(_, pending)| pending.slot),
        );
        if let Some(pending) = self.pending_profile_switch.take() {
            slots.extend(pending.candidate_backends.into_values());
        }
        for (interface, provider_id, identity) in active_modules {
            self.record_backend_runtime_status_with_identity_and_lifecycle(
                interface.clone(),
                provider_id.clone(),
                identity,
                BackendRuntimeStatus::Stopped,
                "shell runtime stopped".to_string(),
                false,
                true,
            );
            self.retire_backend_provider_generation(&interface, &provider_id, identity);
        }
        for slot in slots {
            self.settle_stopped_backend_generation(&slot);
            self.retire_backend_runtime_slot(slot);
        }

        let tasks = std::mem::take(&mut self.retiring_backend_runtimes);
        runtime.block_on(async move {
            for tasks in tasks {
                if let Some(service) = tasks.take_service() {
                    await_backend_task(service, "service").await;
                }
                if let Some(bridge) = tasks.take_bridge() {
                    await_backend_task(bridge, "event bridge").await;
                }
            }
        });
    }

    pub(in crate::shell) fn backend_identity_for_interface(
        &self,
        interface: &str,
    ) -> mesh_core_backend::BackendIdentity {
        self.backend_runtimes
            .get(interface)
            .map(|slot| {
                *slot
                    .identity
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
            })
            .or_else(|| {
                self.pending_backend_runtimes.get(interface).map(|pending| {
                    *pending
                        .slot
                        .identity
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                })
            })
            .unwrap_or_default()
    }

    pub(in crate::shell) fn backend_provider_is_active(
        &self,
        interface: &str,
        provider_id: &str,
    ) -> bool {
        self.backend_runtimes.get(interface).is_some_and(|slot| {
            *slot
                .event_provider_id
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                == provider_id
                && !self
                    .backend_runtime_status(interface, provider_id)
                    .is_some_and(|entry| entry.status.rejects_provider_messages())
        })
    }

    pub(in crate::shell) fn backend_provider_is_active_at_identity(
        &self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
    ) -> bool {
        self.backend_runtimes.get(interface).is_some_and(|slot| {
            *slot
                .event_provider_id
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                == provider_id
                && *slot
                    .identity
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    == identity
                && (identity == mesh_core_backend::BackendIdentity::default()
                    || !self
                        .backend_runtime_status(interface, provider_id)
                        .is_some_and(|entry| {
                            entry.identity == identity && entry.status.rejects_provider_messages()
                        }))
        })
    }

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

    pub(in crate::shell) fn record_backend_runtime_status_at_identity(
        &mut self,
        interface: String,
        provider_id: String,
        identity: mesh_core_backend::BackendIdentity,
        status: BackendRuntimeStatus,
        message: String,
    ) {
        self.record_backend_runtime_status_with_identity(
            interface,
            provider_id,
            identity,
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
        let identity = self.backend_identity_for_interface(&interface);
        self.record_backend_runtime_status_with_identity(
            interface,
            provider_id,
            identity,
            status,
            message,
            publish_health,
        );
    }

    fn record_backend_runtime_status_with_identity(
        &mut self,
        interface: String,
        provider_id: String,
        identity: mesh_core_backend::BackendIdentity,
        status: BackendRuntimeStatus,
        message: String,
        publish_health: bool,
    ) {
        self.record_backend_runtime_status_with_identity_and_lifecycle(
            interface,
            provider_id,
            identity,
            status,
            message,
            publish_health,
            true,
        );
    }

    fn record_backend_runtime_status_with_identity_and_lifecycle(
        &mut self,
        interface: String,
        provider_id: String,
        identity: mesh_core_backend::BackendIdentity,
        status: BackendRuntimeStatus,
        message: String,
        publish_health: bool,
        update_module_lifecycle: bool,
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
                    identity,
                    failure_count,
                },
            );
        if update_module_lifecycle
            && self.backend_runtime_status_is_authoritative(
                &interface,
                &provider_id,
                identity,
                status,
            )
        {
            self.update_module_runtime_lifecycle(&provider_id, status, &message);
        }
        self.publish_backend_health_at_identity(
            &interface,
            &provider_id,
            identity,
            status,
            &message,
            publish_health,
        );
    }

    pub(in crate::shell) fn update_module_runtime_lifecycle(
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

    fn backend_runtime_status_is_authoritative(
        &self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
        status: BackendRuntimeStatus,
    ) -> bool {
        let pending_candidate =
            self.pending_backend_runtimes
                .get(interface)
                .is_some_and(|pending| {
                    pending.slot.provider_id == provider_id
                        && (identity == mesh_core_backend::BackendIdentity::default()
                            || *pending
                                .slot
                                .identity
                                .read()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                == identity)
                })
                || self.profile_candidate_is_pending_at_identity(interface, provider_id, identity);
        if pending_candidate {
            return false;
        }

        // The legacy, identity-less lifecycle channel is also used for
        // startup validation failures before a runtime slot exists. Preserve
        // that authoritative path; once an identity is present, require the
        // live or committed generation to own the transition.
        identity == mesh_core_backend::BackendIdentity::default()
            || self.backend_runtimes.get(interface).is_some_and(|slot| {
                slot.provider_id == provider_id
                    && *slot
                        .identity
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        == identity
            })
            || self.committed_provider_generation_matches(
                interface,
                provider_id,
                identity,
                matches!(
                    status,
                    BackendRuntimeStatus::Stopped | BackendRuntimeStatus::Quarantined
                ),
            )
    }

    fn committed_provider_generation_matches(
        &self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
        include_retired: bool,
    ) -> bool {
        self.committed_provider_generations
            .get(interface)
            .is_some_and(|generation| {
                generation.provider_id == provider_id
                    && generation.identity == identity
                    && (include_retired || !generation.retired)
            })
    }

    fn commit_backend_provider_generation(&mut self, slot: &BackendRuntimeSlot) {
        let identity = *slot
            .identity
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.committed_provider_generations.insert(
            slot.interface.clone(),
            CommittedProviderGeneration {
                provider_id: slot.provider_id.clone(),
                identity,
                retired: false,
            },
        );
    }

    fn retire_backend_provider_generation(
        &mut self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
    ) {
        if let Some(generation) = self.committed_provider_generations.get_mut(interface)
            && generation.provider_id == provider_id
            && generation.identity == identity
        {
            generation.retired = true;
        }
    }

    /// Publish one authoritative availability transition to both ordinary
    /// service observers and subscribers of the reserved `health` event. The
    /// provider cache is updated before delivery, so a newly mounted runtime
    /// and an already-mounted runtime see the same terminal state.
    pub(in crate::shell) fn publish_backend_health(
        &mut self,
        interface: &str,
        provider_id: &str,
        status: BackendRuntimeStatus,
        message: &str,
        deliver: bool,
    ) {
        let identity = self.backend_identity_for_interface(interface);
        self.publish_backend_health_at_identity(
            interface,
            provider_id,
            identity,
            status,
            message,
            deliver,
        );
    }

    pub(in crate::shell) fn publish_backend_health_at_identity(
        &mut self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
        status: BackendRuntimeStatus,
        message: &str,
        deliver: bool,
    ) {
        let active_identity_matches = self.backend_runtimes.get(interface).is_some_and(|slot| {
            slot.provider_id == provider_id
                && *slot
                    .identity
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    == identity
        });
        let pending_identity_matches =
            self.pending_backend_runtimes
                .get(interface)
                .is_some_and(|pending| {
                    pending.slot.provider_id == provider_id
                        && *pending
                            .slot
                            .identity
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            == identity
                });
        let profile_candidate_is_pending =
            self.profile_candidate_is_pending_at_identity(interface, provider_id, identity);
        if pending_identity_matches || profile_candidate_is_pending {
            tracing::debug!(
                interface,
                provider_id,
                "kept candidate provider health private until activation commit"
            );
            return;
        }
        let retired_stop_matches = status == BackendRuntimeStatus::Stopped
            && self.committed_provider_generation_matches(interface, provider_id, identity, true);
        let committed_identity_matches =
            self.committed_provider_generation_matches(interface, provider_id, identity, false);
        if identity != mesh_core_backend::BackendIdentity::default()
            && !active_identity_matches
            && !committed_identity_matches
            && !retired_stop_matches
        {
            tracing::debug!(
                interface,
                provider_id,
                "ignored health transition from an inactive backend identity"
            );
            return;
        }
        let current_provider = self
            .backend_runtimes
            .get(interface)
            .map(|slot| slot.provider_id.as_str())
            .or_else(|| {
                self.pending_backend_runtimes
                    .get(interface)
                    .map(|pending| pending.slot.provider_id.as_str())
            });
        if current_provider.is_some_and(|current| current != provider_id)
            || current_provider.is_none() && provider_id != "<none>" && !retired_stop_matches
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
                } else {
                    object.remove("availability_reason");
                }
            }
            let state_changed = self
                .latest_service_state
                .get(interface)
                .is_none_or(|latest| latest.provider_id != provider_id || latest.state != payload);
            let event = ServiceEvent::Updated {
                service: interface.to_string(),
                source_module: provider_id.to_string(),
                payload: payload.clone(),
            };
            let generation = self
                .latest_service_state
                .get(interface)
                .map_or(1, |latest| latest.generation.saturating_add(1));
            self.latest_service_state.insert(
                interface.to_string(),
                LatestServiceState::new_with_identity(
                    interface.to_string(),
                    provider_id.to_string(),
                    generation,
                    identity,
                    payload,
                ),
            );
            if deliver && state_changed {
                match self.deliver_service_event(&event) {
                    Ok(requests) => self.enqueue_effects(requests),
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
        let health_transition_changed =
            self.latest_service_health
                .get(interface)
                .is_none_or(|previous| {
                    self.latest_service_health_identities
                        .get(interface)
                        .is_none_or(|previous_identity| *previous_identity != identity)
                        || backend_health_transition_changed(previous, &health_event)
                });
        self.latest_service_health
            .insert(interface.to_string(), health_event.clone());
        self.latest_service_health_identities
            .insert(interface.to_string(), identity);
        if deliver && health_transition_changed {
            match self.deliver_service_event(&health_event) {
                Ok(requests) => self.enqueue_effects(requests),
                Err(error) => tracing::warn!(
                    interface,
                    provider_id,
                    "failed to deliver backend health transition: {error}"
                ),
            }
        }
    }

    pub(in crate::shell) fn stop_backend_runtime(&mut self, interface: &str) {
        self.stop_backend_runtime_with_health(interface, true);
    }

    /// Close a backend's command ingress and retain both runtime task handles
    /// for the shutdown join. Closing the command channel lets the backend
    /// lifecycle guard run the authored stop hook and flush durable storage;
    /// abort is reserved for the bounded final cleanup path.
    pub(in crate::shell) fn retire_backend_runtime_slot(&mut self, slot: BackendRuntimeSlot) {
        if slot.tasks.is_none() {
            // Test and legacy slots may not carry retained join handles; do
            // not leave their synthetic task detached after the sender closes.
            slot.task.abort();
        }
        let tasks = slot.tasks;
        drop(slot.command_tx);
        if let Some(tasks) = tasks {
            self.retiring_backend_runtimes.push(tasks);
        }
    }

    fn stop_backend_runtime_with_health(&mut self, interface: &str, publish_health: bool) {
        self.service_handlers.remove(interface);
        if let Some(slot) = self.backend_runtimes.remove(interface) {
            let identity = *slot
                .identity
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            self.settle_stopped_backend_generation(&slot);
            let interface_name = slot.interface.clone();
            let provider_id = slot.provider_id.clone();
            self.retire_backend_runtime_slot(slot);
            self.rollback_bound_service_states_for_provider(&interface_name, &provider_id);
            let terminal_failure_already_recorded = self
                .backend_runtime_status(&interface_name, &provider_id)
                .map(|entry| {
                    matches!(
                        entry.status,
                        BackendRuntimeStatus::InitFailed | BackendRuntimeStatus::Failed
                    )
                })
                .unwrap_or(false);
            if !terminal_failure_already_recorded {
                self.record_backend_runtime_status_with_identity(
                    interface_name.clone(),
                    provider_id.clone(),
                    identity,
                    BackendRuntimeStatus::Stopped,
                    "runtime stopped".to_string(),
                    publish_health,
                );
            }
            self.retire_backend_provider_generation(&interface_name, &provider_id, identity);
        }
    }

    fn settle_stopped_backend_generation(&mut self, slot: &BackendRuntimeSlot) {
        let stale_calls = self
            .pending_service_call_routes
            .iter()
            .filter(|(_, route)| {
                route.interface == slot.interface && route.generation == slot.generation
            })
            .map(|(call_id, _)| *call_id)
            .collect::<Vec<_>>();

        for call_id in stale_calls {
            for state in self.command_throttle.values_mut() {
                if state
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.call_id.raw() == call_id)
                {
                    state.pending = None;
                }
            }
            let call = mesh_core_backend::CallId::from_raw(call_id);
            mesh_core_backend::finish_call(call);
            self.complete_service_call_route(
                call,
                "stale_generation",
                &serde_json::json!({
                    "ok": false,
                    "status": "stale_generation",
                    "error": "backend runtime stopped before command settlement",
                }),
            );
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
        self.commit_backend_provider_generation(&slot);
        self.backend_runtimes.insert(interface, slot);
    }

    pub(in crate::shell) fn retag_backend_runtimes_for_activation(
        &mut self,
        activation_generation: u64,
    ) {
        for state in self.backend_supervision.values_mut() {
            if state.restart_pending
                && state.pending_identity.activation_generation != activation_generation
            {
                state.invalidate_pending_restart();
            }
        }
        let identities = self
            .backend_runtimes
            .iter()
            .map(|(interface, slot)| {
                let mut identity = slot
                    .identity
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                identity.activation_generation = activation_generation;
                (interface.clone(), *identity, slot.provider_id.clone())
            })
            .collect::<Vec<_>>();
        for (interface, identity, provider_id) in identities {
            if let Some(entry) = self
                .backend_runtime_statuses
                .get_mut(&interface)
                .and_then(|providers| providers.get_mut(&provider_id))
            {
                entry.identity = identity;
            }
            if let Some(latest) = self.latest_service_state.get_mut(&interface)
                && latest.provider_id == provider_id
            {
                latest.identity = identity;
            }
            if let Some(ServiceEvent::InterfaceEvent { source_module, .. }) =
                self.latest_service_health.get(&interface)
                && source_module == &provider_id
            {
                self.latest_service_health_identities
                    .insert(interface.clone(), identity);
            }
            if let Some(generation) = self.committed_provider_generations.get_mut(&interface)
                && generation.provider_id == provider_id
            {
                generation.identity = identity;
                generation.retired = false;
            }
        }
    }

    /// Keep a newly spawned runtime private until it has reported a lifecycle
    /// start and, for stateful contracts, a valid initial snapshot.
    pub(in crate::shell) fn stage_backend_runtime_activation(
        &mut self,
        interface: String,
        slot: BackendRuntimeSlot,
    ) {
        self.stage_pending_backend_runtime(
            interface,
            PendingBackendRuntime {
                slot,
                graph_path: None,
                started: false,
                initial_state: None,
            },
        );
    }

    pub(in crate::shell) fn stage_backend_runtime_switch(
        &mut self,
        interface: String,
        slot: BackendRuntimeSlot,
        graph_path: PathBuf,
    ) {
        self.stage_pending_backend_runtime(
            interface,
            PendingBackendRuntime {
                slot,
                graph_path: Some(graph_path),
                started: false,
                initial_state: None,
            },
        );
    }

    fn stage_pending_backend_runtime(&mut self, interface: String, pending: PendingBackendRuntime) {
        if let Some(previous) = self.pending_backend_runtimes.remove(&interface) {
            let identity = *previous
                .slot
                .identity
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous_interface = previous.slot.interface.clone();
            let previous_provider = previous.slot.provider_id.clone();
            self.retire_backend_runtime_slot(previous.slot);
            self.record_backend_runtime_status_with_identity_and_lifecycle(
                previous_interface,
                previous_provider,
                identity,
                BackendRuntimeStatus::Stopped,
                "superseded by a newer provider switch".to_string(),
                true,
                false,
            );
        }
        self.pending_backend_runtimes.insert(interface, pending);
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

        if let Some(graph_path) = pending.graph_path.as_ref() {
            let identity = *pending
                .slot
                .identity
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Err(error) = crate::shell::module_config::write_composed_provider_selection(
                graph_path,
                interface,
                provider_id,
            ) {
                self.retire_backend_runtime_slot(pending.slot);
                let message = format!(
                    "provider {provider_id} became ready for {interface}, but its selection could not be saved: {error}"
                );
                self.record_backend_runtime_status_with_identity_and_lifecycle(
                    interface.to_string(),
                    provider_id.to_string(),
                    identity,
                    BackendRuntimeStatus::Failed,
                    message.clone(),
                    true,
                    false,
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
                    self.retire_backend_runtime_slot(pending.slot);
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
            if let Err(error) = self.commit_installed_module_graph(candidate_graph) {
                self.retire_backend_runtime_slot(pending.slot);
                let message = format!(
                    "provider selection was saved but locale catalogs could not be committed: {error}"
                );
                self.diagnostics.record_lifecycle_error(
                    "@mesh/shell",
                    "provider_selection_locale_commit_failed",
                    message,
                );
                return;
            }
        }
        self.backend_supervision.remove(interface);
        let identity = *pending
            .slot
            .identity
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let initial_state = pending.initial_state;
        self.replace_backend_runtime(interface.to_string(), pending.slot);
        self.note_backend_running(interface);
        if let Some(payload) = initial_state {
            self.publish_prepared_backend_state(interface, provider_id, payload);
        }
        if let Some(module) = self.modules.get_mut(provider_id) {
            module.clear_quarantine();
        }
        self.record_backend_runtime_status_at_identity(
            interface.to_string(),
            provider_id.to_string(),
            identity,
            BackendRuntimeStatus::Running,
            "backend runtime ready".to_string(),
        );
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
        let identity = self.backend_identity_for_interface(&interface);
        self.handle_backend_lifecycle_at_identity(
            interface,
            provider_id,
            identity,
            stage,
            status,
            message,
        );
    }

    pub(in crate::shell) fn handle_backend_lifecycle_at_identity(
        &mut self,
        interface: String,
        provider_id: String,
        identity: mesh_core_backend::BackendIdentity,
        stage: String,
        status: String,
        message: String,
    ) {
        let runtime_status = BackendRuntimeStatus::from_str(&status);
        let is_prepared_provider =
            self.pending_backend_runtimes
                .get(&interface)
                .is_some_and(|pending| {
                    pending.slot.provider_id == provider_id
                        && (identity == mesh_core_backend::BackendIdentity::default()
                            || *pending
                                .slot
                                .identity
                                .read()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                == identity)
                })
                || self.profile_candidate_is_pending_at_identity(
                    &interface,
                    &provider_id,
                    identity,
                );
        let is_active_provider =
            self.backend_provider_is_active_at_identity(&interface, &provider_id, identity);
        if identity != mesh_core_backend::BackendIdentity::default()
            && !is_prepared_provider
            && !is_active_provider
        {
            tracing::debug!(
                interface,
                provider_id,
                status = runtime_status.as_str(),
                "ignored lifecycle transition from an inactive backend identity"
            );
            return;
        }
        self.record_backend_runtime_status_with_identity_and_lifecycle(
            interface.clone(),
            provider_id.clone(),
            identity,
            runtime_status,
            message.clone(),
            true,
            !is_prepared_provider,
        );
        if self.handle_profile_backend_lifecycle_at_identity(
            &interface,
            &provider_id,
            identity,
            runtime_status,
        ) {
            return;
        }
        let event_provider_is_pending =
            self.pending_backend_runtimes
                .get(&interface)
                .is_some_and(|pending| {
                    pending.slot.provider_id == provider_id
                        && (identity == mesh_core_backend::BackendIdentity::default()
                            || *pending
                                .slot
                                .identity
                                .read()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                == identity)
                });
        if event_provider_is_pending {
            if runtime_status == BackendRuntimeStatus::Running {
                let needs_initial_state = self.service_requires_initial_state(&interface)
                    && self
                        .pending_backend_runtimes
                        .get(&interface)
                        .is_some_and(|pending| pending.initial_state.is_none());
                if needs_initial_state {
                    if let Some(pending) = self.pending_backend_runtimes.get_mut(&interface) {
                        pending.started = true;
                    }
                    tracing::info!(
                        interface,
                        provider_id,
                        "provider started; waiting for a valid initial service snapshot"
                    );
                } else {
                    self.complete_backend_runtime_switch(&interface, &provider_id);
                }
            } else if matches!(
                runtime_status,
                BackendRuntimeStatus::InitFailed
                    | BackendRuntimeStatus::Failed
                    | BackendRuntimeStatus::Stopped
            ) && let Some(pending) = self.pending_backend_runtimes.remove(&interface)
            {
                let has_current_runtime = self.backend_runtimes.contains_key(&interface);
                let module_id = pending.slot.provider_id.clone();
                self.retire_backend_runtime_slot(pending.slot);
                if !has_current_runtime {
                    self.update_module_runtime_lifecycle(&module_id, runtime_status, &message);
                }
                tracing::warn!(
                    interface,
                    provider_id,
                    stage,
                    "provider switch failed; keeping the current runtime active"
                );
            }
            return;
        }
        let event_provider_is_current = self.backend_runtimes.get(&interface).is_some_and(|slot| {
            slot.provider_id == provider_id
                && (identity == mesh_core_backend::BackendIdentity::default()
                    || *slot
                        .identity
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        == identity)
        });
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
            self.stop_backend_runtime_with_health(&interface, false);
            self.supervise_backend_failure_at_identity(&interface, &provider_id, identity);
        }
    }

    pub(in crate::shell) fn capture_pending_backend_update(
        &mut self,
        interface: &str,
        provider_id: &str,
        event: ServiceEvent,
    ) -> bool {
        self.capture_pending_backend_update_at_identity(
            interface,
            provider_id,
            mesh_core_backend::BackendIdentity::default(),
            event,
        )
    }

    pub(in crate::shell) fn capture_pending_backend_update_at_identity(
        &mut self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
        event: ServiceEvent,
    ) -> bool {
        let matches_pending = self
            .pending_backend_runtimes
            .get(interface)
            .is_some_and(|pending| {
                pending.slot.provider_id == provider_id
                    && (identity == mesh_core_backend::BackendIdentity::default()
                        || *pending
                            .slot
                            .identity
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            == identity)
            });
        if !matches_pending {
            return false;
        }
        let ServiceEvent::Updated {
            service,
            source_module: _,
            payload,
        } = self.normalize_service_event(event)
        else {
            return true;
        };
        if !self.validate_service_state_shape(interface, provider_id, &payload) {
            self.abort_pending_backend_runtime(
                interface,
                provider_id,
                "provider emitted an invalid initial service snapshot",
            );
            return true;
        }
        let should_commit = if let Some(pending) = self.pending_backend_runtimes.get_mut(interface)
        {
            pending.initial_state = Some(payload);
            pending.started
        } else {
            false
        };
        if should_commit {
            self.complete_backend_runtime_switch(interface, provider_id);
        }
        tracing::debug!(
            interface,
            provider_id,
            service,
            "buffered prepared provider snapshot"
        );
        true
    }

    fn abort_pending_backend_runtime(&mut self, interface: &str, provider_id: &str, message: &str) {
        let Some(pending) = self.pending_backend_runtimes.remove(interface) else {
            return;
        };
        let has_current_runtime = self.backend_runtimes.contains_key(interface);
        let module_id = pending.slot.provider_id.clone();
        let identity = *pending
            .slot
            .identity
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.retire_backend_runtime_slot(pending.slot);
        self.record_backend_runtime_status_with_identity_and_lifecycle(
            interface.to_string(),
            provider_id.to_string(),
            identity,
            BackendRuntimeStatus::Failed,
            message.to_string(),
            true,
            false,
        );
        if !has_current_runtime {
            self.update_module_runtime_lifecycle(&module_id, BackendRuntimeStatus::Failed, message);
        }
        tracing::warn!(interface, provider_id, "{message}");
    }

    fn publish_prepared_backend_state(
        &mut self,
        interface: &str,
        provider_id: &str,
        payload: serde_json::Value,
    ) {
        let event = ServiceEvent::Updated {
            service: interface.to_string(),
            source_module: provider_id.to_string(),
            payload,
        };
        if self.record_latest_service_state(&event) {
            match self.deliver_service_event(&event) {
                Ok(requests) => self.enqueue_effects(requests),
                Err(error) => tracing::warn!(
                    interface,
                    provider_id,
                    "failed to deliver prepared provider snapshot: {error}"
                ),
            }
        }
    }
}

fn backend_health_transition_changed(previous: &ServiceEvent, next: &ServiceEvent) -> bool {
    let (
        ServiceEvent::InterfaceEvent {
            service: previous_service,
            source_module: previous_source,
            name: previous_name,
            payload: previous_payload,
        },
        ServiceEvent::InterfaceEvent {
            service: next_service,
            source_module: next_source,
            name: next_name,
            payload: next_payload,
        },
    ) = (previous, next)
    else {
        return true;
    };

    previous_service != next_service
        || previous_source != next_source
        || previous_name != next_name
        || previous_payload.get("state") != next_payload.get("state")
        || previous_payload.get("recoverable") != next_payload.get("recoverable")
}

async fn await_backend_task(task: JoinHandle<()>, role: &str) {
    let mut task = task;
    tokio::select! {
        result = &mut task => {
            if let Err(error) = result {
                tracing::debug!(role, "backend {role} task ended during shutdown: {error}");
            }
        }
        _ = tokio::time::sleep(BACKEND_STOP_DEADLINE) => {
            tracing::warn!(role, "backend {role} task exceeded shutdown deadline; aborting");
            task.abort();
            let _ = task.await;
        }
    }
}
