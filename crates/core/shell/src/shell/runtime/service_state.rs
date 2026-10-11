#![allow(dead_code)] // Compatibility event routing remains covered by shell tests.

use super::super::*;
use mesh_core_service::RUNTIME_METADATA_STATE_FIELDS;

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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
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
        if interface == "mesh.theme" {
            // Theme state is a rendered control-plane fact. A provider may
            // implement commands or mirror the state, but it cannot replace
            // the snapshot that the shell actually committed and painted.
            payload = self.authoritative_theme_service_payload();
        } else if let Some(contract) = self.interfaces.resolve(&interface, None).compiled_contract {
            contract.repair_state(&mut payload);
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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
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
                            Ok(emitted) => {
                                requests.extend(runtime.effect_origin().attribute(emitted))
                            }
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
                                Ok(emitted) => {
                                    requests.extend(runtime.effect_origin().attribute(emitted))
                                }
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
                            Ok(emitted) => {
                                requests.extend(runtime.effect_origin().attribute(emitted))
                            }
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
                            Ok(emitted) => {
                                requests.extend(runtime.effect_origin().attribute(emitted))
                            }
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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
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

        if interface == "mesh.theme" && matches!(name.as_str(), "ThemeChanged" | "TokenChanged") {
            // Theme events carry rendered facts and are emitted only by the
            // shell after its revisioned snapshot commit. A provider mirror
            // cannot publish a competing event with a different identity,
            // palette, provenance, or revision.
            tracing::debug!(
                interface,
                provider_id,
                event = name,
                "ignoring provider-authored rendered theme event"
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
        let Some(contract) = self.interfaces.resolve(interface, None).compiled_contract else {
            return true;
        };
        let warnings = contract.state_warnings(payload);
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
                    .any(|field| !RUNTIME_METADATA_STATE_FIELDS.contains(&field.name.as_str()))
            })
    }

    fn service_event_contract_warnings(
        &self,
        interface: &str,
        event_name: &str,
        payload: &serde_json::Value,
    ) -> Vec<String> {
        let Some(contract) = self.interfaces.resolve(interface, None).compiled_contract else {
            return vec![format!(
                "event '{event_name}' emitted for unknown interface {interface}"
            )];
        };
        contract.event_warnings(event_name, payload)
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
    ) -> Result<VecDeque<SourcedRequest>, ShellRunError> {
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
}

fn service_values_equivalent(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    actual == expected
        || actual
            .as_f64()
            .zip(expected.as_f64())
            .is_some_and(|(actual, expected)| (actual - expected).abs() < f64::EPSILON)
}
