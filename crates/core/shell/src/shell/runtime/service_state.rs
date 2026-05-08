use super::super::*;

impl Shell {
    pub(in crate::shell) fn broadcast_service_event(
        &mut self,
        event: ServiceEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
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
                &canonical_interface_name(service),
                source_module,
                started.elapsed(),
                Some("broadcast_service_event"),
            );
        }
        Ok(requests)
    }

    pub(in crate::shell) fn record_latest_service_state(&mut self, event: &ServiceEvent) -> bool {
        let ServiceEvent::Updated {
            service,
            source_module,
            payload,
        } = event;
        let interface = canonical_interface_name(service);
        let shell_authoritative_theme_update =
            interface == "mesh.theme" && source_module == "@mesh/shell";
        if let Some(slot) = self.backend_runtimes.get(&interface) {
            if slot.provider_id != *source_module && !shell_authoritative_theme_update {
                tracing::debug!(
                    interface,
                    source_module,
                    active_provider = %slot.provider_id,
                    "ignoring stale service update from inactive provider"
                );
                return false;
            }
        } else if self
            .backend_runtime_statuses
            .get(&(interface.clone(), source_module.clone()))
            .is_some_and(|entry| {
                matches!(
                    entry.status,
                    BackendRuntimeStatus::InitFailed
                        | BackendRuntimeStatus::Failed
                        | BackendRuntimeStatus::Stopped
                )
            })
        {
            tracing::debug!(
                interface,
                source_module,
                "ignoring service update from terminal backend provider"
            );
            return false;
        }
        self.validate_service_state_shape(&interface, source_module, payload);
        self.latest_service_state.insert(
            interface.clone(),
            LatestServiceState {
                interface,
                provider_id: source_module.clone(),
                state: payload.clone(),
            },
        );
        true
    }

    pub(in crate::shell) fn deliver_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            requests.extend(
                runtime
                    .component
                    .handle_service_event(event)
                    .map_err(ShellRunError::Component)?,
            );
        }
        Ok(requests)
    }

    fn validate_service_state_shape(
        &mut self,
        interface: &str,
        provider_id: &str,
        payload: &serde_json::Value,
    ) {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract.as_ref() else {
            return;
        };
        for warning in service_state_contract_warnings(contract, payload) {
            self.record_service_contract_warning(interface, provider_id, warning);
        }
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
        let events = self
            .latest_service_state
            .values()
            .map(|latest| ServiceEvent::Updated {
                service: latest.interface.clone(),
                source_module: latest.provider_id.clone(),
                payload: latest.state.clone(),
            })
            .collect::<Vec<_>>();
        for event in events {
            requests.extend(self.broadcast_service_event(event)?);
        }
        Ok(requests)
    }
}

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
            warnings.push(format!(
                "missing required state field '{}' for {}",
                field.name, contract.interface
            ));
            continue;
        };
        if !json_value_matches_contract_type(value, &field.field_type) {
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

fn is_runtime_metadata_state_field(name: &str) -> bool {
    name == "source_module"
}

fn json_value_matches_contract_type(value: &serde_json::Value, field_type: &str) -> bool {
    let normalized = field_type.trim().to_ascii_lowercase();
    if normalized.starts_with('[') && normalized.ends_with(']') {
        return value.is_array();
    }

    match normalized.as_str() {
        "bool" | "boolean" => value.is_boolean(),
        "float" | "double" | "number" => value.is_number(),
        "int" | "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "string" => value.is_string(),
        "object" | "table" | "map" => value.is_object(),
        _ => true,
    }
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
