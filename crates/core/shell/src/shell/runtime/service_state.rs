#![allow(dead_code)] // Compatibility event routing remains covered by shell tests.

use super::super::*;
#[cfg(test)]
use mesh_core_service::InterfaceArgument;
use mesh_core_service::{InterfaceContract, TypeExpr};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

thread_local! {
    static CONTRACT_TYPE_CACHE: RefCell<HashMap<String, TypeExpr>> =
        RefCell::new(HashMap::new());
}

impl Shell {
    /// Attribute a shell-derived service snapshot to the active provider when
    /// one is running. This keeps host-produced snapshots on the same generic
    /// provider path as the provider's own later updates.
    pub(in crate::shell) fn active_service_provider_or(
        &self,
        interface: &str,
        fallback: &str,
    ) -> String {
        self.backend_runtimes
            .get(interface)
            .map(|slot| slot.provider_id.clone())
            .unwrap_or_else(|| fallback.to_string())
    }

    pub(in crate::shell) fn broadcast_service_event(
        &mut self,
        event: ServiceEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let event = self.normalize_service_event(event);
        let profiling_started = self
            .profiling_enabled()
            .then_some(std::time::Instant::now());
        if !self.record_latest_service_state(&event) {
            return Ok(VecDeque::new());
        }
        let requests = self.deliver_service_event(&event)?;
        if let (
            Some(started),
            ServiceEvent::Updated {
                service,
                source_module,
                ..
            },
        ) = (profiling_started, &event)
        {
            self.record_backend_state_publish_delivery(
                service,
                source_module,
                started.elapsed(),
                Some("broadcast_service_event"),
            );
        }
        Ok(requests)
    }

    pub(in crate::shell) fn normalize_service_event(
        &mut self,
        event: ServiceEvent,
    ) -> ServiceEvent {
        let ServiceEvent::Updated {
            service,
            source_module,
            mut payload,
        } = event
        else {
            return event;
        };
        let interface = canonical_interface_name_owned(service);
        if self
            .backend_runtimes
            .get(&interface)
            .is_some_and(|slot| slot.provider_id != source_module)
            || self
                .backend_runtime_status(&interface, &source_module)
                .is_some_and(|entry| entry.status.rejects_provider_messages())
        {
            return ServiceEvent::Updated {
                service: interface,
                source_module,
                payload,
            };
        }
        // A command-bound field remains shell-authoritative until the provider
        // confirms it. This keeps every observer on one reactive value even if
        // an older provider snapshot arrives while the command is in flight.
        let pending_fields: Vec<String> = self
            .pending_bound_service_state
            .keys()
            .filter(|(pending_interface, _)| pending_interface == &interface)
            .map(|(_, field)| field.clone())
            .collect();
        for field in pending_fields {
            let key = (interface.clone(), field);
            let Some(expected) = self.pending_bound_service_state.get(&key).cloned() else {
                continue;
            };
            if payload
                .get(&key.1)
                .is_some_and(|actual| service_values_equivalent(actual, &expected.optimistic))
            {
                if let Some(confirmed) = self.pending_bound_service_state.remove(&key) {
                    self.forget_bound_service_state_chain(confirmed.call_id);
                }
            } else {
                payload[key.1.as_str()] = expected.optimistic.clone();
            }
        }
        ServiceEvent::Updated {
            service: interface,
            source_module,
            payload,
        }
    }

    pub(in crate::shell) fn record_latest_service_state(&mut self, event: &ServiceEvent) -> bool {
        let interface = match event {
            ServiceEvent::Updated { service, .. } => canonical_interface_name_cow(service),
            _ => return true,
        };
        let identity = self.backend_identity_for_interface(interface.as_ref());
        self.record_latest_service_state_at_identity(event, identity)
    }

    pub(in crate::shell) fn record_latest_service_state_at_identity(
        &mut self,
        event: &ServiceEvent,
        identity: mesh_core_backend::BackendIdentity,
    ) -> bool {
        let ServiceEvent::Updated {
            service,
            source_module,
            payload,
        } = event
        else {
            return true;
        };
        let interface = canonical_interface_name_cow(service);
        if let Some(slot) = self.backend_runtimes.get(interface.as_ref()) {
            if slot.provider_id != *source_module
                || (identity != BackendIdentity::default()
                    && !self.backend_provider_is_active_at_identity(
                        interface.as_ref(),
                        source_module,
                        identity,
                    ))
                || (identity == BackendIdentity::default()
                    && !self.backend_provider_is_active(interface.as_ref(), source_module))
            {
                tracing::debug!(
                    interface = interface.as_ref(),
                    source_module,
                    active_provider = %slot.provider_id,
                    "ignoring stale service update from inactive provider"
                );
                return false;
            }
        } else if identity != BackendIdentity::default()
            || self
                .backend_runtime_status(interface.as_ref(), source_module)
                .is_some_and(|entry| entry.status.rejects_provider_messages())
        {
            tracing::debug!(
                interface = interface.as_ref(),
                source_module,
                "ignoring service update from terminal backend provider"
            );
            return false;
        }
        if let Some(latest) = self.latest_service_state.get(interface.as_ref())
            && latest.provider_id == *source_module
            && latest.state.eq(payload)
        {
            return false;
        }
        if !self.validate_service_state_shape(&interface, source_module, &payload) {
            tracing::warn!(
                interface = %interface,
                source_module,
                "ignored service state snapshot with invalid contract shape"
            );
            return false;
        }
        let interface = interface.into_owned();
        let generation = self
            .latest_service_state
            .get(&interface)
            .map_or(1, |latest| latest.generation.saturating_add(1));
        self.latest_service_state.insert(
            interface.clone(),
            LatestServiceState::new_with_identity(
                interface,
                source_module.clone(),
                generation,
                identity,
                payload.clone(),
            ),
        );
        true
    }

    /// Write a contract-bound command value into the shell's canonical service
    /// state and publish it to every observer. Provider state later confirms
    /// the value through `normalize_service_event`.
    pub(in crate::shell) fn apply_bound_service_state(
        &mut self,
        interface: &str,
        field: &str,
        value: serde_json::Value,
        call_id: Option<mesh_core_backend::CallId>,
    ) {
        let interface = interface.to_string();
        let provider_id = self
            .backend_runtimes
            .get(&interface)
            .map(|slot| slot.provider_id.clone())
            .or_else(|| {
                self.latest_service_state
                    .get(&interface)
                    .map(|latest| latest.provider_id.clone())
            })
            .unwrap_or_else(|| "@mesh/shell".to_string());
        let identity = self
            .backend_runtimes
            .get(&interface)
            .map(|slot| {
                *slot
                    .identity
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
            })
            .or_else(|| {
                self.latest_service_state
                    .get(&interface)
                    .map(|latest| latest.identity)
            })
            .unwrap_or_default();
        let mut payload = self
            .latest_service_state
            .get(&interface)
            .map(|latest| latest.state.clone())
            .unwrap_or_else(|| serde_json::json!({ "available": true }));
        let previous = payload.get(field).cloned();
        if let Some(call_id) = call_id {
            let key = (interface.clone(), field.to_string());
            let previous_call_id = self
                .pending_bound_service_state
                .get(&key)
                .map(|pending| pending.call_id);
            let pending = PendingBoundServiceState {
                call_id,
                interface: interface.clone(),
                field: field.to_string(),
                provider_id: provider_id.clone(),
                identity,
                previous_call_id,
                previous,
                optimistic: value.clone(),
                terminal_status: None,
            };
            self.pending_bound_service_state
                .insert(key, pending.clone());
            self.bound_service_state_transactions
                .insert(call_id, pending);
        }
        payload[field] = value;
        self.publish_bound_service_state(interface, provider_id, identity, payload);
    }

    fn publish_bound_service_state(
        &mut self,
        interface: String,
        provider_id: String,
        identity: mesh_core_backend::BackendIdentity,
        payload: serde_json::Value,
    ) {
        let generation = self
            .latest_service_state
            .get(&interface)
            .map_or(1, |latest| latest.generation.saturating_add(1));
        self.latest_service_state.insert(
            interface.clone(),
            LatestServiceState::new_with_identity(
                interface.clone(),
                provider_id.clone(),
                generation,
                identity,
                payload.clone(),
            ),
        );
        let _ = self.deliver_service_event(&ServiceEvent::Updated {
            service: interface,
            source_module: provider_id,
            payload,
        });
    }

    /// Apply a terminal result to the optimistic write that owns `call_id`.
    /// Success leaves the optimistic overlay in place until the provider
    /// snapshot confirms it. Every non-success outcome restores the value that
    /// was visible before that write, but only when the call still owns the
    /// field; a newer write is never overwritten by an older completion.
    pub(in crate::shell) fn settle_bound_service_state(
        &mut self,
        call_id: mesh_core_backend::CallId,
        status: &str,
    ) {
        let key = self
            .pending_bound_service_state
            .iter()
            .find(|(_, pending)| pending.call_id == call_id)
            .map(|(key, _)| key.clone())
            .or_else(|| {
                self.bound_service_state_transactions
                    .get(&call_id)
                    .map(|pending| (pending.interface.clone(), pending.field.clone()))
            });
        let Some(key) = key else { return };
        if status == "completed" {
            if let Some(pending) = self.bound_service_state_transactions.get_mut(&call_id) {
                pending.terminal_status = Some(status.to_string());
            }
            return;
        }
        if let Some(pending) = self.bound_service_state_transactions.get_mut(&call_id) {
            pending.terminal_status = Some(status.to_string());
        }
        let is_current = self
            .pending_bound_service_state
            .get(&key)
            .is_some_and(|pending| pending.call_id == call_id);
        if !is_current {
            return;
        }
        self.rollback_bound_service_state(call_id);
    }

    /// Provider replacement and stop are terminal failures for writes admitted
    /// to the old provider generation. Roll back only entries owned by that
    /// generation so unrelated interfaces remain untouched.
    pub(in crate::shell) fn rollback_bound_service_states_for_provider(
        &mut self,
        interface: &str,
        provider_id: &str,
    ) {
        loop {
            let call_id = self
                .pending_bound_service_state
                .iter()
                .find(|((pending_interface, _), pending)| {
                    pending_interface == interface && pending.provider_id == provider_id
                })
                .map(|(_, pending)| pending.call_id);
            let Some(call_id) = call_id else { break };
            self.settle_bound_service_state(call_id, "stale_provider");
        }
    }

    fn rollback_bound_service_state(&mut self, call_id: mesh_core_backend::CallId) {
        let Some(transaction) = self.bound_service_state_transactions.remove(&call_id) else {
            return;
        };
        let key = (transaction.interface.clone(), transaction.field.clone());
        if self
            .pending_bound_service_state
            .get(&key)
            .is_some_and(|pending| pending.call_id == call_id)
        {
            self.pending_bound_service_state.remove(&key);
        }

        if let Some(previous_call_id) = transaction.previous_call_id {
            let previous_failed = self
                .bound_service_state_transactions
                .get(&previous_call_id)
                .and_then(|previous| previous.terminal_status.as_deref())
                .is_some_and(Self::is_failed_bound_service_status);
            if previous_failed {
                self.rollback_bound_service_state(previous_call_id);
                return;
            }
            if let Some(previous) = self
                .bound_service_state_transactions
                .get(&previous_call_id)
                .cloned()
            {
                let provider_id = self
                    .backend_runtimes
                    .get(&key.0)
                    .map(|slot| slot.provider_id.clone())
                    .unwrap_or_else(|| previous.provider_id.clone());
                self.pending_bound_service_state
                    .insert(key.clone(), previous.clone());
                self.restore_bound_service_state_value(
                    &key.0,
                    &key.1,
                    provider_id,
                    previous.optimistic,
                );
                return;
            }
        }
        self.restore_bound_service_state(&key.0, &key.1, transaction.previous);
    }

    fn forget_bound_service_state_chain(&mut self, call_id: mesh_core_backend::CallId) {
        let mut next = Some(call_id);
        while let Some(call_id) = next {
            next = self
                .bound_service_state_transactions
                .remove(&call_id)
                .and_then(|transaction| transaction.previous_call_id);
        }
    }

    fn restore_bound_service_state(
        &mut self,
        interface: &str,
        field: &str,
        previous: Option<serde_json::Value>,
    ) {
        let interface = interface.to_string();
        let provider_id = self
            .backend_runtimes
            .get(&interface)
            .map(|slot| slot.provider_id.clone())
            .or_else(|| {
                self.latest_service_state
                    .get(&interface)
                    .map(|latest| latest.provider_id.clone())
            })
            .unwrap_or_else(|| "@mesh/shell".to_string());
        let mut payload = self
            .latest_service_state
            .get(&interface)
            .map(|latest| latest.state.clone())
            .unwrap_or_else(|| serde_json::json!({ "available": true }));
        if let Some(previous) = previous {
            payload[field] = previous;
        } else if let Some(object) = payload.as_object_mut() {
            object.remove(field);
        }
        let identity = self.backend_identity_for_interface(&interface);
        self.publish_bound_service_state(interface, provider_id, identity, payload);
    }

    fn restore_bound_service_state_value(
        &mut self,
        interface: &str,
        field: &str,
        provider_id: String,
        value: serde_json::Value,
    ) {
        let interface = interface.to_string();
        let mut payload = self
            .latest_service_state
            .get(&interface)
            .map(|latest| latest.state.clone())
            .unwrap_or_else(|| serde_json::json!({ "available": true }));
        payload[field] = value;
        let identity = self.backend_identity_for_interface(&interface);
        self.publish_bound_service_state(interface, provider_id, identity, payload);
    }

    fn is_failed_bound_service_status(status: &str) -> bool {
        // `settle_bound_service_state` is called only for terminal call
        // outcomes. Keep the rollback policy closed over the success state so
        // newly added typed failures (for example invalid results, queue-full,
        // and stale-generation outcomes) cannot leave an older transaction
        // pinned as the visible owner of the field.
        status != "completed"
    }

    /// Resolve a command's state-bound value: either copy the declared
    /// argument or negate the current boolean field for a toggle binding.
    pub(in crate::shell) fn bound_value_for_command(
        &self,
        interface: &str,
        binding: &mesh_core_service::StateBinding,
        payload: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        match &binding.from_arg {
            Some(arg) => payload.get(arg).cloned(),
            None if binding.toggle => {
                let current = self
                    .latest_service_state
                    .get(interface)
                    .and_then(|latest| latest.state.get(&binding.field))
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                Some(serde_json::json!(!current))
            }
            None => None,
        }
    }

    pub(in crate::shell) fn deliver_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        self.rebuild_service_delivery_index_if_needed();
        let mut requests = VecDeque::new();
        let mut component_failures = Vec::new();
        match event {
            ServiceEvent::Updated { service, .. } => {
                let service_name = crate::shell::service::service_name_from_interface_cow(service);
                let interface = canonical_interface_name_cow(service);
                let generation = self
                    .latest_service_state
                    .get(interface.as_ref())
                    .map_or(0, |latest| latest.generation);
                let epoch = self
                    .service_delivery_index
                    .begin_delivery_epoch(self.components.len());
                let (index, components) = (&mut self.service_delivery_index, &mut self.components);
                let (
                    fallback_components,
                    update_services,
                    cached_update_services,
                    component_epochs,
                ) = (
                    &index.fallback_components,
                    &index.update_services,
                    &index.cached_update_services,
                    &mut index.component_epochs,
                );

                // Summaries with no safe snapshot retain the legacy gate.
                // Indexed members were recorded from a current summary, so
                // they can dispatch without re-locking every runtime.
                for &component_index in fallback_components {
                    let Some(runtime) = components.get_mut(component_index) else {
                        continue;
                    };
                    if runtime.quarantined {
                        continue;
                    }
                    if component_epochs[component_index] == epoch {
                        continue;
                    }
                    component_epochs[component_index] = epoch;
                    if runtime.component.observes_service_event(event) {
                        match runtime
                            .component
                            .handle_service_event_with_generation(event, generation)
                        {
                            Ok(emitted) => requests.extend(emitted),
                            Err(error) => component_failures.push((component_index, error)),
                        };
                    } else {
                        runtime
                            .component
                            .cache_service_payload_with_generation(event, generation);
                    }
                }
                if let Some(subscribers) = update_services.get(service_name.as_ref()) {
                    for &component_index in subscribers {
                        let Some(runtime) = components.get_mut(component_index) else {
                            continue;
                        };
                        if runtime.quarantined {
                            continue;
                        }
                        if component_epochs[component_index] != epoch {
                            component_epochs[component_index] = epoch;
                            match runtime
                                .component
                                .handle_service_event_with_generation(event, generation)
                            {
                                Ok(emitted) => requests.extend(emitted),
                                Err(error) => component_failures.push((component_index, error)),
                            };
                        }
                    }
                }
                // Keep declared-service caches warm without visiting unrelated
                // components. The epoch marker avoids a duplicate cache write
                // for a component that already handled this update.
                if let Some(cached) = cached_update_services.get(service_name.as_ref()) {
                    for &component_index in cached {
                        let Some(runtime) = components.get_mut(component_index) else {
                            continue;
                        };
                        if runtime.quarantined {
                            continue;
                        }
                        if component_epochs[component_index] != epoch {
                            component_epochs[component_index] = epoch;
                            runtime
                                .component
                                .cache_service_payload_with_generation(event, generation);
                        }
                    }
                }
            }
            ServiceEvent::InterfaceEvent { service, name, .. } => {
                let service_name = crate::shell::service::service_name_from_interface_cow(service);
                let (index, components) = (&self.service_delivery_index, &mut self.components);
                for &component_index in &index.fallback_components {
                    let Some(runtime) = components.get_mut(component_index) else {
                        continue;
                    };
                    if runtime.quarantined {
                        continue;
                    }
                    if runtime.component.observes_service_event(event) {
                        match runtime.component.handle_service_event(event) {
                            Ok(emitted) => requests.extend(emitted),
                            Err(error) => component_failures.push((component_index, error)),
                        }
                    }
                }
                if let Some(subscribers) = index
                    .interface_events
                    .get(service_name.as_ref())
                    .and_then(|events| events.get(name))
                {
                    for &component_index in subscribers {
                        let Some(runtime) = components.get_mut(component_index) else {
                            continue;
                        };
                        if runtime.quarantined {
                            continue;
                        }
                        match runtime.component.handle_service_event(event) {
                            Ok(emitted) => requests.extend(emitted),
                            Err(error) => component_failures.push((component_index, error)),
                        }
                    }
                }
            }
        }
        for (component_index, error) in component_failures {
            self.contain_component_failure(component_index, "service_event_delivery", error);
        }
        Ok(requests)
    }

    pub(in crate::shell) fn rebuild_service_delivery_index_if_needed(&mut self) {
        if !self.service_delivery_index.dirty {
            return;
        }

        let mut index = ServiceDeliveryIndex::default();
        for (component_index, runtime) in self.components.iter().enumerate() {
            let summary = runtime.component.service_observation_summary();
            index.component_summaries.push(summary.clone());
            let Some(summary) = summary else {
                index.fallback_components.push(component_index);
                continue;
            };
            for service in summary.update_services {
                index
                    .update_services
                    .entry(service)
                    .or_default()
                    .push(component_index);
            }
            for service in summary.cached_update_services {
                index
                    .cached_update_services
                    .entry(service)
                    .or_default()
                    .push(component_index);
            }
            for ServiceInterfaceEventSubscription { service, event } in summary.interface_events {
                index
                    .interface_events
                    .entry(service)
                    .or_default()
                    .entry(event)
                    .or_default()
                    .push(component_index);
            }
        }
        // Component indices are appended in ascending order, so duplicate
        // declarations are adjacent. Normalize once when the index changes
        // instead of cloning/sorting/deduplicating targets for every event.
        for subscribers in index.update_services.values_mut() {
            subscribers.dedup();
        }
        for subscribers in index.cached_update_services.values_mut() {
            subscribers.dedup();
        }
        for events in index.interface_events.values_mut() {
            for subscribers in events.values_mut() {
                subscribers.dedup();
            }
        }
        index.dirty = false;
        self.service_delivery_index = index;
    }

    /// Whether an interface event has any possible observer in the current
    /// component generation. Components without an authoritative observation
    /// summary remain conservative fallbacks, so this gate never suppresses
    /// an event that the legacy delivery path could observe.
    pub(in crate::shell) fn has_interface_event_observers(
        &mut self,
        interface: &str,
        event: &str,
    ) -> bool {
        self.rebuild_service_delivery_index_if_needed();
        if !self.service_delivery_index.fallback_components.is_empty() {
            return true;
        }
        let service_name = crate::shell::service::service_name_from_interface_cow(interface);
        self.service_delivery_index
            .interface_events
            .get(service_name.as_ref())
            .and_then(|events| events.get(event))
            .is_some_and(|subscribers| !subscribers.is_empty())
    }

    /// Deliver an interface event produced by the shell itself. Shell-owned
    /// snapshots do not have to impersonate a backend provider, but they still
    /// use the same contract validation and capability-filtered delivery path.
    pub(in crate::shell) fn broadcast_shell_interface_event(
        &mut self,
        interface: &str,
        name: &str,
        payload: serde_json::Value,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let warnings = self.service_event_contract_warnings(interface, name, &payload);
        if !warnings.is_empty() {
            for warning in warnings {
                self.record_service_contract_warning(interface, "@mesh/shell", warning);
            }
            return Ok(VecDeque::new());
        }

        self.deliver_service_event(&ServiceEvent::InterfaceEvent {
            service: interface.to_string(),
            source_module: "@mesh/shell".to_string(),
            name: name.to_string(),
            payload,
        })
    }

    pub(in crate::shell) fn broadcast_backend_interface_event(
        &mut self,
        interface: String,
        provider_id: String,
        name: String,
        payload: serde_json::Value,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        self.broadcast_backend_interface_event_at_generation(
            interface,
            provider_id,
            name,
            payload,
            0,
        )
    }

    pub(in crate::shell) fn broadcast_backend_interface_event_at_generation(
        &mut self,
        interface: String,
        provider_id: String,
        name: String,
        payload: serde_json::Value,
        generation: u64,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let identity = self.backend_identity_for_interface(&interface);
        self.broadcast_backend_interface_event_at_identity(
            interface,
            provider_id,
            identity,
            name,
            payload,
            generation,
        )
    }

    pub(in crate::shell) fn broadcast_backend_interface_event_at_identity(
        &mut self,
        interface: String,
        provider_id: String,
        identity: mesh_core_backend::BackendIdentity,
        name: String,
        payload: serde_json::Value,
        generation: u64,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if generation != 0
            && !self
                .backend_runtimes
                .get(&interface)
                .is_some_and(|slot| slot.generation == generation)
        {
            tracing::debug!(
                interface,
                provider_id,
                event = name,
                generation,
                "ignoring interface event from a stale backend generation"
            );
            return Ok(VecDeque::new());
        }
        let provider_is_active = if identity == BackendIdentity::default() {
            self.backend_provider_is_active(&interface, &provider_id)
        } else {
            self.backend_provider_is_active_at_identity(&interface, &provider_id, identity)
        };
        if !provider_is_active {
            tracing::debug!(
                interface,
                provider_id,
                event = name,
                "ignoring interface event from inactive or terminal provider"
            );
            return Ok(VecDeque::new());
        }

        let warnings = self.service_event_contract_warnings(&interface, &name, &payload);
        if !warnings.is_empty() {
            for warning in warnings {
                self.record_service_contract_warning(&interface, &provider_id, warning);
            }
            return Ok(VecDeque::new());
        }

        self.deliver_service_event(&ServiceEvent::InterfaceEvent {
            service: interface,
            source_module: provider_id,
            name,
            payload,
        })
    }

    pub(in crate::shell) fn validate_service_state_shape(
        &mut self,
        interface: &str,
        provider_id: &str,
        payload: &serde_json::Value,
    ) -> bool {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract else {
            return true;
        };
        let warnings = {
            let cache = self.validation_cache_for_contract(contract);
            service_state_contract_warnings_cached(cache, payload)
        };
        let valid = warnings.is_empty();
        for warning in warnings {
            self.record_service_contract_warning(interface, provider_id, warning);
        }
        valid
    }

    pub(in crate::shell) fn service_requires_initial_state(&self, interface: &str) -> bool {
        self.interfaces
            .resolve(interface, None)
            .contract
            .is_some_and(|contract| {
                contract
                    .state_fields
                    .iter()
                    .any(|field| !is_runtime_metadata_state_field(&field.name))
            })
    }

    fn service_event_contract_warnings(
        &mut self,
        interface: &str,
        event_name: &str,
        payload: &serde_json::Value,
    ) -> Vec<String> {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract else {
            return vec![format!(
                "event '{event_name}' emitted for unknown interface {interface}"
            )];
        };
        let cache = self.validation_cache_for_contract(contract);
        event_payload_contract_warnings_cached(cache, event_name, payload)
    }

    fn record_service_contract_warning(
        &mut self,
        interface: &str,
        provider_id: &str,
        message: String,
    ) {
        let message = format!("service_contract_warning: {interface}: {message}");
        tracing::warn!(interface, provider_id, "{message}");
        self.diagnostics.record_lifecycle_error(
            provider_id.to_string(),
            "service_contract_warning",
            message,
        );
    }

    pub(in crate::shell) fn replay_cached_service_events(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        let latest_service_state = self.latest_service_state.clone();
        let replay_result: Result<(), ShellRunError> = (|| {
            for latest in latest_service_state.values() {
                let event = ServiceEvent::Updated {
                    service: latest.interface.clone(),
                    source_module: latest.provider_id.clone(),
                    payload: latest.state.clone(),
                };
                requests.extend(self.deliver_service_event(&event)?);
            }
            let latest_service_health = self
                .latest_service_health
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for event in latest_service_health {
                requests.extend(self.deliver_service_event(&event)?);
            }
            Ok(())
        })();
        replay_result?;
        Ok(requests)
    }

    fn validation_cache_for_contract(
        &mut self,
        contract: Arc<InterfaceContract>,
    ) -> &ContractValidationCache {
        let interface = contract.interface.as_str();
        let cached = self
            .service_contract_validation
            .get(interface)
            .is_some_and(|cache| Arc::ptr_eq(&cache.contract, &contract));
        if !cached {
            self.service_contract_validation.insert(
                contract.interface.clone(),
                build_contract_validation_cache(Arc::clone(&contract)),
            );
        }
        self.service_contract_validation
            .get(interface)
            .expect("contract validation cache inserted")
    }
}

fn service_values_equivalent(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    actual == expected
        || actual
            .as_f64()
            .zip(expected.as_f64())
            .is_some_and(|(actual, expected)| (actual - expected).abs() < f64::EPSILON)
}

fn build_contract_validation_cache(contract: Arc<InterfaceContract>) -> ContractValidationCache {
    let state_fields = contract
        .state_fields
        .iter()
        .filter(|field| !is_runtime_metadata_state_field(&field.name))
        .map(|field| CompiledContractField {
            name: field.name.clone(),
            field_type: field.field_type.clone(),
            value_type: cached_contract_value_type(&field.field_type),
        })
        .collect();
    let events = contract
        .events
        .iter()
        .map(|event| {
            (
                event.name.clone(),
                event
                    .payload
                    .iter()
                    .map(|field| CompiledContractField {
                        name: field.name.clone(),
                        field_type: field.arg_type.clone(),
                        value_type: cached_contract_value_type(&field.arg_type),
                    })
                    .collect(),
            )
        })
        .collect();
    ContractValidationCache {
        types: contract.types.clone(),
        contract,
        state_fields,
        events,
    }
}

#[cfg(test)]
fn service_state_contract_warnings(
    contract: &InterfaceContract,
    payload: &serde_json::Value,
) -> Vec<String> {
    let Some(object) = payload.as_object() else {
        return vec![format!(
            "state for {} must be a JSON object, got {}",
            contract.interface,
            json_type_name(payload)
        )];
    };

    let mut warnings = Vec::new();
    for field in &contract.state_fields {
        if is_runtime_metadata_state_field(&field.name) {
            continue;
        }
        let Some(value) = object.get(&field.name) else {
            if !cached_contract_value_type(&field.field_type).optional {
                warnings.push(format!(
                    "missing required state field '{}' for {}",
                    field.name, contract.interface
                ));
            }
            continue;
        };
        let compiled_type = cached_contract_value_type(&field.field_type);
        if !compiled_type.matches_with_types(value, &contract.types) {
            warnings.push(format!(
                "state field '{}' for {} expected {}, got {}",
                field.name,
                contract.interface,
                field.field_type,
                json_type_name(value)
            ));
        }
    }
    warnings
}

fn service_state_contract_warnings_cached(
    cache: &ContractValidationCache,
    payload: &serde_json::Value,
) -> Vec<String> {
    let Some(object) = payload.as_object() else {
        return vec![format!(
            "state for {} must be a JSON object, got {}",
            cache.contract.interface,
            json_type_name(payload)
        )];
    };

    let mut warnings = Vec::new();
    for field in &cache.state_fields {
        let Some(value) = object.get(&field.name) else {
            if !field.value_type.optional {
                warnings.push(format!(
                    "missing required state field '{}' for {}",
                    field.name, cache.contract.interface
                ));
            }
            continue;
        };
        if !field.value_type.matches_with_types(value, &cache.types) {
            warnings.push(format!(
                "state field '{}' for {} expected {}, got {}",
                field.name,
                cache.contract.interface,
                field.field_type,
                json_type_name(value)
            ));
        }
    }
    warnings
}

#[cfg(test)]
fn event_payload_contract_warnings(
    interface: &str,
    event_name: &str,
    fields: &[InterfaceArgument],
    payload: &serde_json::Value,
) -> Vec<String> {
    let Some(object) = payload.as_object() else {
        return vec![format!(
            "event '{event_name}' for {interface} must be a JSON object, got {}",
            json_type_name(payload)
        )];
    };

    let mut warnings = Vec::new();
    for field in fields {
        let Some(value) = object.get(field.name.as_str()) else {
            if !cached_contract_value_type(&field.arg_type).optional {
                warnings.push(format!(
                    "event '{event_name}' for {interface} missing required payload field '{}'",
                    field.name
                ));
            }
            continue;
        };
        if !cached_contract_value_type(&field.arg_type).matches(value) {
            let field_name = field.name.as_str();
            warnings.push(format!(
                "event '{event_name}' for {interface} payload field '{field_name}' expected {}, got {}",
                field.arg_type,
                json_type_name(value)
            ));
        }
    }
    warnings
}

fn event_payload_contract_warnings_cached(
    cache: &ContractValidationCache,
    event_name: &str,
    payload: &serde_json::Value,
) -> Vec<String> {
    let Some(fields) = cache.events.get(event_name) else {
        return vec![format!(
            "event '{event_name}' is not declared for {}",
            cache.contract.interface
        )];
    };
    if fields.is_empty() {
        return match payload.as_object() {
            Some(object) if object.is_empty() => Vec::new(),
            Some(_) => vec![format!(
                "event '{event_name}' for {} expected an empty object payload",
                cache.contract.interface
            )],
            None => vec![format!(
                "event '{event_name}' for {} must be a JSON object, got {}",
                cache.contract.interface,
                json_type_name(payload)
            )],
        };
    }
    compiled_event_payload_contract_warnings(
        &cache.contract.interface,
        event_name,
        fields,
        &cache.types,
        payload,
    )
}

fn compiled_event_payload_contract_warnings(
    interface: &str,
    event_name: &str,
    fields: &[CompiledContractField],
    types: &HashMap<String, mesh_core_service::InterfaceTypeDef>,
    payload: &serde_json::Value,
) -> Vec<String> {
    let Some(object) = payload.as_object() else {
        return vec![format!(
            "event '{event_name}' for {interface} must be a JSON object, got {}",
            json_type_name(payload)
        )];
    };

    let mut warnings = Vec::new();
    for field in fields {
        let Some(value) = object.get(field.name.as_str()) else {
            if !field.value_type.optional {
                warnings.push(format!(
                    "event '{event_name}' for {interface} missing required payload field '{}'",
                    field.name
                ));
            }
            continue;
        };
        if !field.value_type.matches_with_types(value, types) {
            let field_name = field.name.as_str();
            warnings.push(format!(
                "event '{event_name}' for {interface} payload field '{field_name}' expected {}, got {}",
                field.field_type,
                json_type_name(value)
            ));
        }
    }
    warnings
}

/// Validate the JSON object sent to a declared interface method before it is
/// placed on a backend queue. This is the shell-side safety net for direct
/// callers that bypass the Luau proxy.
pub(in crate::shell) fn service_method_input_contract_warnings(
    contract: &InterfaceContract,
    command: &str,
    payload: &serde_json::Value,
) -> Vec<String> {
    let Some(method) = contract
        .methods
        .iter()
        .find(|method| method.name == command)
    else {
        return vec![format!(
            "method '{command}' is not declared for {}",
            contract.interface
        )];
    };
    let Some(object) = payload.as_object() else {
        return vec![format!(
            "method '{command}' for {} must receive a JSON object, got {}",
            contract.interface,
            json_type_name(payload)
        )];
    };

    let mut warnings = Vec::new();
    for argument in &method.args {
        let value_type = cached_contract_value_type(&argument.arg_type);
        let Some(value) = object.get(&argument.name) else {
            if !value_type.optional {
                warnings.push(format!(
                    "method '{command}' for {} missing required argument '{}'",
                    contract.interface, argument.name
                ));
            }
            continue;
        };
        if !value_type.matches_with_types(value, &contract.types) {
            warnings.push(format!(
                "method '{command}' for {} argument '{}' expected {}, got {}",
                contract.interface,
                argument.name,
                argument.arg_type,
                json_type_name(value)
            ));
        }
    }
    warnings
}

/// Validate a provider's result before completing a frontend service ticket.
pub(in crate::shell) fn service_method_result_contract_warnings(
    contract: &InterfaceContract,
    command: &str,
    result: &serde_json::Value,
) -> Vec<String> {
    let Some(method) = contract
        .methods
        .iter()
        .find(|method| method.name == command)
    else {
        return vec![format!(
            "method '{command}' is not declared for {}",
            contract.interface
        )];
    };
    let Some(returns) = method.returns.as_deref() else {
        return Vec::new();
    };
    let value_type = cached_contract_value_type(returns);
    if value_type.matches_with_types(result, &contract.types) {
        Vec::new()
    } else {
        vec![format!(
            "method '{command}' for {} returned {}, got {}",
            contract.interface,
            returns,
            json_type_name(result)
        )]
    }
}

fn is_runtime_metadata_state_field(name: &str) -> bool {
    name == "source_module"
}

/// Parse a contract type expression through the shared grammar, cached per
/// expression string. Unparseable expressions never reach here for graph-built
/// contracts (they are rejected at graph build), but fall back to a permissive
/// `any?` so runtime validation degrades gracefully.
fn cached_contract_value_type(field_type: &str) -> TypeExpr {
    CONTRACT_TYPE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(value_type) = cache.get(field_type) {
            return value_type.clone();
        }
        let value_type = TypeExpr::parse(field_type).unwrap_or(TypeExpr {
            base: mesh_core_service::BaseType::Any,
            array: false,
            optional: true,
        });
        cache.insert(field_type.to_owned(), value_type.clone());
        value_type
    })
}

fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod contract_validation_tests {
    use super::*;
    use mesh_core_service::{ContractCapabilities, InterfaceContract, InterfaceEvent};
    use std::sync::Arc;

    fn field(name: &str, arg_type: &str) -> InterfaceArgument {
        InterfaceArgument {
            name: name.to_string(),
            arg_type: arg_type.to_string(),
        }
    }

    fn validation_contract(event_count: usize, fields_per_event: usize) -> InterfaceContract {
        InterfaceContract {
            interface: "mesh.audio".to_string(),
            version: mesh_core_service::parse_contract_version("1.0").unwrap(),
            state_fields: vec![
                mesh_core_service::contract::ContractStateField {
                    name: "available".to_string(),
                    field_type: "boolean".to_string(),
                    description: None,
                },
                mesh_core_service::contract::ContractStateField {
                    name: "percent".to_string(),
                    field_type: "float".to_string(),
                    description: None,
                },
                mesh_core_service::contract::ContractStateField {
                    name: "source_module".to_string(),
                    field_type: "string".to_string(),
                    description: None,
                },
            ],
            methods: Vec::new(),
            events: (0..event_count)
                .map(|event_index| InterfaceEvent {
                    name: format!("Event{event_index}"),
                    payload: (0..fields_per_event)
                        .map(|field_index| field(&format!("field_{field_index}"), "float"))
                        .collect(),
                })
                .collect(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        }
    }

    #[test]
    fn structured_event_validation_preserves_warnings() {
        let warnings = event_payload_contract_warnings(
            "mesh.audio",
            "VolumeChanged",
            &[field("device_id", "string"), field("level", "float")],
            &serde_json::json!({ "device_id": 7, "other": true }),
        );

        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("payload field 'device_id' expected string, got number"));
        assert!(warnings[1].contains("missing required payload field 'level'"));
    }

    #[test]
    fn event_validation_rejects_non_object_payload() {
        let warnings = event_payload_contract_warnings(
            "mesh.audio",
            "VolumeChanged",
            &[field("level", "float")],
            &serde_json::json!(42),
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("must be a JSON object"));
    }

    #[test]
    fn cached_contract_validation_preserves_warning_text() {
        let contract = Arc::new(validation_contract(2, 2));
        let cache = build_contract_validation_cache(Arc::clone(&contract));
        let state_warnings = service_state_contract_warnings_cached(
            &cache,
            &serde_json::json!({ "available": true, "percent": "loud" }),
        );
        assert_eq!(
            state_warnings,
            service_state_contract_warnings(
                &contract,
                &serde_json::json!({ "available": true, "percent": "loud" }),
            )
        );
        assert_eq!(state_warnings.len(), 1);
        assert!(state_warnings[0].contains("state field 'percent'"));

        let event_warnings = event_payload_contract_warnings_cached(
            &cache,
            "Event1",
            &serde_json::json!({ "field_0": "bad" }),
        );
        assert_eq!(event_warnings.len(), 2);
        assert!(event_warnings[0].contains("payload field 'field_0' expected float"));
        assert!(event_warnings[1].contains("missing required payload field 'field_1'"));
    }

    #[test]
    fn cached_type_matching_follows_shared_grammar() {
        let cases = [
            (serde_json::json!(true), "boolean", true),
            (serde_json::json!(1.5), "float", true),
            (serde_json::json!(1.5), "int", false),
            (serde_json::json!(1), "int", true),
            (serde_json::json!("value"), "string", true),
            (serde_json::json!({}), "object", true),
            (serde_json::json!([]), "Device[]", true),
            (serde_json::json!(null), "string?", true),
            (serde_json::json!(null), "string", false),
        ];
        for (value, field_type, expected) in cases {
            assert_eq!(
                cached_contract_value_type(field_type).matches(&value),
                expected,
                "type {field_type}"
            );
        }
    }

    #[test]
    fn invalid_type_expressions_degrade_to_permissive_matching() {
        assert!(cached_contract_value_type("[string]").matches(&serde_json::json!(1)));
    }

    // cargo test -p mesh-core-shell --release -- contract_validation_cache_beats_event_schema_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only contract validation cache microbenchmark"]
    fn contract_validation_cache_beats_event_schema_scan() {
        let contract = Arc::new(validation_contract(64, 8));
        let cache = build_contract_validation_cache(Arc::clone(&contract));
        let event_name = "Event63";
        let payload = serde_json::Value::Object(
            (0..8)
                .map(|index| (format!("field_{index}"), serde_json::json!(index as f64)))
                .collect(),
        );
        let iterations = 100_000usize;

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let event = contract
                .events
                .iter()
                .find(|event| event.name == event_name)
                .unwrap();
            old_total += event_payload_contract_warnings(
                &contract.interface,
                std::hint::black_box(event_name),
                &event.payload,
                std::hint::black_box(&payload),
            )
            .len();
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_total += event_payload_contract_warnings_cached(
                std::hint::black_box(&cache),
                std::hint::black_box(event_name),
                std::hint::black_box(&payload),
            )
            .len();
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "contract event validation: scan+parse {old_time:?}; cached {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, 0);
        assert_eq!(new_total, 0);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- contract_validation_lookup_borrows_interface_name --ignored --nocapture
    #[test]
    #[ignore = "release-only contract validation lookup microbenchmark"]
    fn contract_validation_lookup_borrows_interface_name() {
        let contract = Arc::new(validation_contract(4, 4));
        let mut cache = HashMap::new();
        cache.insert(
            contract.interface.clone(),
            build_contract_validation_cache(Arc::clone(&contract)),
        );
        let iterations = 2_000_000usize;

        let owned_started = std::time::Instant::now();
        let mut owned_total = 0usize;
        for _ in 0..iterations {
            let interface = contract.interface.clone();
            owned_total = owned_total.wrapping_add(std::hint::black_box(
                cache.get(&interface).expect("cached contract").events.len(),
            ));
        }
        let owned_time = owned_started.elapsed();

        let borrowed_started = std::time::Instant::now();
        let mut borrowed_total = 0usize;
        for _ in 0..iterations {
            borrowed_total = borrowed_total.wrapping_add(std::hint::black_box(
                cache
                    .get(contract.interface.as_str())
                    .expect("cached contract")
                    .events
                    .len(),
            ));
        }
        let borrowed_time = borrowed_started.elapsed();

        assert_eq!(owned_total, borrowed_total);
        let speedup = owned_time.as_secs_f64() / borrowed_time.as_secs_f64();
        eprintln!(
            "contract validation cache lookup over {iterations} hits: owned interface key {owned_time:?}; borrowed key {borrowed_time:?}; ratio {speedup:.2}x"
        );
        eprintln!("MESH_PERF metric=contract_validation_lookup_speedup value={speedup:.6}");
        assert!(borrowed_time < owned_time);
    }
}
