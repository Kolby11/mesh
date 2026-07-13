use super::super::*;
use mesh_core_service::{InterfaceArgument, TypeExpr};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CONTRACT_TYPE_CACHE: RefCell<HashMap<String, TypeExpr>> =
        RefCell::new(HashMap::new());
}

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
                service,
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
        let interface = canonical_interface_name_owned(service);
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
                service: interface,
                source_module,
                payload,
            };
        }
        // Re-apply pending optimistic patches until the provider confirms the
        // expected value; a confirming update clears the pending entry.
        let pending_fields: Vec<String> = self
            .pending_optimistic_state
            .keys()
            .filter(|(pending_interface, _)| pending_interface == &interface)
            .map(|(_, field)| field.clone())
            .collect();
        for field in pending_fields {
            let key = (interface.clone(), field);
            let Some(expected) = self.pending_optimistic_state.get(&key) else {
                continue;
            };
            if payload.get(&key.1) == Some(expected) {
                self.pending_optimistic_state.remove(&key);
            } else {
                payload[key.1.as_str()] = expected.clone();
            }
        }
        ServiceEvent::Updated {
            service: interface,
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
        let interface = canonical_interface_name_cow(service);
        let shell_authoritative_theme_update =
            interface == "mesh.theme" && source_module == "@mesh/shell";
        if let Some(slot) = self.backend_runtimes.get(interface.as_ref()) {
            if slot.provider_id != *source_module && !shell_authoritative_theme_update {
                tracing::debug!(
                    interface = interface.as_ref(),
                    source_module,
                    active_provider = %slot.provider_id,
                    "ignoring stale service update from inactive provider"
                );
                return false;
            }
        } else if self
            .backend_runtime_status(interface.as_ref(), source_module)
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
        self.validate_service_state_shape(&interface, source_module, &payload);
        let interface = interface.into_owned();
        self.latest_service_state.insert(
            interface.clone(),
            LatestServiceState::new(interface, source_module.clone(), payload.clone()),
        );
        true
    }

    /// Apply a contract-declared optimistic state patch: set the public state
    /// field to the expected value immediately so UI reacts before the
    /// provider confirms. `normalize_service_event` keeps re-applying the
    /// patch until a provider update carries the expected value.
    pub(in crate::shell) fn apply_optimistic_service_state(
        &mut self,
        interface: &str,
        field: &str,
        value: serde_json::Value,
    ) {
        self.pending_optimistic_state
            .insert((interface.to_string(), field.to_string()), value.clone());
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
            .unwrap_or_else(|| "@mesh/optimistic".to_string());
        let mut payload = self
            .latest_service_state
            .get(&interface)
            .map(|latest| latest.state.clone())
            .unwrap_or_else(|| serde_json::json!({ "available": true }));
        payload[field] = value;
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

    /// Compute the optimistic value for a dispatched command from its
    /// contract annotation: either the named argument's payload value, or the
    /// negation of the current boolean state field for toggles.
    pub(in crate::shell) fn optimistic_value_for_command(
        &self,
        interface: &str,
        optimistic: &mesh_core_service::OptimisticUpdate,
        payload: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        match &optimistic.from_arg {
            Some(arg) => payload.get(arg).cloned(),
            None => {
                let current = self
                    .latest_service_state
                    .get(interface)
                    .and_then(|latest| latest.state.get(&optimistic.field))
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                Some(serde_json::json!(!current))
            }
        }
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
        if event.payload.is_empty() {
            return Vec::new();
        }
        event_payload_contract_warnings(&contract.interface, event_name, &event.payload, payload)
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
        let compiled_type = cached_contract_value_type(&field.field_type);
        if !compiled_type.matches(value) {
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
            warnings.push(format!(
                "event '{event_name}' for {interface} missing required payload field '{}'",
                field.name
            ));
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

    fn field(name: &str, arg_type: &str) -> InterfaceArgument {
        InterfaceArgument {
            name: name.to_string(),
            arg_type: arg_type.to_string(),
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
}
