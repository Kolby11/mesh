use super::super::*;

impl Shell {
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
                &canonical_interface_name(service),
                source_module,
                started.elapsed(),
                Some("broadcast_service_event"),
            );
        }
        Ok(requests)
    }

    fn normalize_service_event(&mut self, event: ServiceEvent) -> ServiceEvent {
        let ServiceEvent::Updated {
            service,
            source_module,
            mut payload,
        } = event
        else {
            return event;
        };
        let interface = canonical_interface_name(&service);
        let shell_authoritative_theme_update =
            interface == "mesh.theme" && source_module == "@mesh/shell";
        if self.backend_runtimes.get(&interface).is_some_and(|slot| {
            slot.provider_id != source_module && !shell_authoritative_theme_update
        }) || self
            .backend_runtime_status(&interface, &source_module)
            .is_some_and(|entry| {
                matches!(
                    entry.status,
                    BackendRuntimeStatus::InitFailed
                        | BackendRuntimeStatus::Failed
                        | BackendRuntimeStatus::Stopped
                )
            })
        {
            return ServiceEvent::Updated {
                service,
                source_module,
                payload,
            };
        }
        if interface == "mesh.audio"
            && let Some(requested_muted) = self.pending_audio_muted
        {
            let backend_muted = payload.get("muted").and_then(|value| value.as_bool());
            if backend_muted == Some(requested_muted) {
                self.pending_audio_muted = None;
            } else {
                payload["muted"] = serde_json::json!(requested_muted);
            }
        }
        ServiceEvent::Updated {
            service,
            source_module,
            payload,
        }
    }

    pub(in crate::shell) fn record_latest_service_state(&mut self, event: &ServiceEvent) -> bool {
        let ServiceEvent::Updated {
            service,
            source_module,
            payload,
        } = event
        else {
            return true;
        };
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
            .backend_runtime_status(&interface, source_module)
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
        if let Some(latest) = self.latest_service_state.get(&interface)
            && latest.provider_id == *source_module
            && latest.state.eq(payload)
        {
            return false;
        }
        self.validate_service_state_shape(&interface, source_module, &payload);
        self.latest_service_state.insert(
            interface.clone(),
            LatestServiceState::new(interface, source_module.clone(), payload.clone()),
        );
        true
    }

    pub(in crate::shell) fn apply_optimistic_audio_muted_state(&mut self, muted: bool) {
        self.pending_audio_muted = Some(muted);
        let interface = "mesh.audio".to_string();
        let provider_id = self
            .backend_runtimes
            .get(&interface)
            .map(|slot| slot.provider_id.clone())
            .or_else(|| {
                self.latest_service_state
                    .get(&interface)
                    .map(|latest| latest.provider_id.clone())
            })
            .unwrap_or_else(|| "@mesh/optimistic-audio".to_string());
        let mut payload = self
            .latest_service_state
            .get(&interface)
            .map(|latest| latest.state.clone())
            .unwrap_or_else(|| serde_json::json!({ "available": true }));
        payload["muted"] = serde_json::json!(muted);
        self.latest_service_state.insert(
            interface.clone(),
            LatestServiceState::new(interface.clone(), provider_id.clone(), payload.clone()),
        );
        let _ = self.deliver_service_event(&ServiceEvent::Updated {
            service: interface,
            source_module: provider_id,
            payload,
        });
    }

    pub(in crate::shell) fn deliver_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            if !runtime.component.observes_service_event(event) {
                continue;
            }
            requests.extend(
                runtime
                    .component
                    .handle_service_event(event)
                    .map_err(ShellRunError::Component)?,
            );
        }
        Ok(requests)
    }

    pub(in crate::shell) fn broadcast_backend_interface_event(
        &mut self,
        interface: String,
        provider_id: String,
        name: String,
        payload: serde_json::Value,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if let Some(slot) = self.backend_runtimes.get(&interface)
            && slot.provider_id != provider_id
        {
            tracing::debug!(
                interface,
                provider_id,
                active_provider = %slot.provider_id,
                event = name,
                "ignoring interface event from inactive provider"
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

    fn service_event_contract_warnings(
        &self,
        interface: &str,
        event_name: &str,
        payload: &serde_json::Value,
    ) -> Vec<String> {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract.as_ref() else {
            return vec![format!(
                "event '{event_name}' emitted for unknown interface {interface}"
            )];
        };
        let Some(event) = contract
            .events
            .iter()
            .find(|event| event.name == event_name)
        else {
            return vec![format!(
                "event '{event_name}' is not declared for {}",
                contract.interface
            )];
        };
        let Some(schema) = event.payload.as_deref() else {
            return Vec::new();
        };
        event_payload_contract_warnings(&contract.interface, event_name, schema, payload)
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
        let latest_service_state = std::mem::take(&mut self.latest_service_state);
        let replay_result: Result<(), ShellRunError> = (|| {
            for latest in latest_service_state.values() {
                let event = ServiceEvent::Updated {
                    service: latest.interface.clone(),
                    source_module: latest.provider_id.clone(),
                    payload: latest.state.clone(),
                };
                requests.extend(self.deliver_service_event(&event)?);
            }
            Ok(())
        })();
        self.latest_service_state = latest_service_state;
        replay_result?;
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

fn event_payload_contract_warnings(
    interface: &str,
    event_name: &str,
    schema: &str,
    payload: &serde_json::Value,
) -> Vec<String> {
    let fields = parse_inline_object_schema(schema);
    if fields.is_empty() {
        return Vec::new();
    }
    let Some(object) = payload.as_object() else {
        return vec![format!(
            "event '{event_name}' for {interface} must be a JSON object, got {}",
            json_type_name(payload)
        )];
    };

    let mut warnings = Vec::new();
    for (field_name, field_type) in fields {
        let Some(value) = object.get(&field_name) else {
            warnings.push(format!(
                "event '{event_name}' for {interface} missing required payload field '{field_name}'"
            ));
            continue;
        };
        if !json_value_matches_contract_type(value, &field_type) {
            warnings.push(format!(
                "event '{event_name}' for {interface} payload field '{field_name}' expected {}, got {}",
                field_type,
                json_type_name(value)
            ));
        }
    }
    warnings
}

fn parse_inline_object_schema(schema: &str) -> Vec<(String, String)> {
    let trimmed = schema.trim();
    let Some(inner) = trimmed
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        return Vec::new();
    };
    inner
        .split(',')
        .filter_map(|part| {
            let (name, field_type) = part.split_once(':')?;
            let name = name.trim();
            let field_type = field_type.trim();
            if name.is_empty() || field_type.is_empty() {
                return None;
            }
            Some((name.to_string(), field_type.to_string()))
        })
        .collect()
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
