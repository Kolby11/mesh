use super::super::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

thread_local! {
    static INLINE_EVENT_SCHEMA_CACHE: RefCell<HashMap<String, Arc<[CompiledPayloadField]>>> =
        RefCell::new(HashMap::new());
    static CONTRACT_TYPE_CACHE: RefCell<HashMap<String, ContractValueType>> =
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
    schema: &str,
    payload: &serde_json::Value,
) -> Vec<String> {
    let fields = compiled_inline_object_schema(schema);
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
    for field in fields.iter() {
        let Some(value) = object.get(field.name.as_str()) else {
            warnings.push(format!(
                "event '{event_name}' for {interface} missing required payload field '{}'",
                field.name
            ));
            continue;
        };
        if !field.value_type.matches(value) {
            let field_name = field.name.as_str();
            warnings.push(format!(
                "event '{event_name}' for {interface} payload field '{field_name}' expected {}, got {}",
                field.type_label,
                json_type_name(value)
            ));
        }
    }
    warnings
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledPayloadField {
    name: String,
    type_label: String,
    value_type: ContractValueType,
}

fn compiled_inline_object_schema(schema: &str) -> Arc<[CompiledPayloadField]> {
    INLINE_EVENT_SCHEMA_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(fields) = cache.get(schema) {
            return Arc::clone(fields);
        }
        let fields: Arc<[CompiledPayloadField]> = parse_inline_object_schema(schema)
            .into_iter()
            .map(|(name, field_type)| CompiledPayloadField {
                name: name.to_owned(),
                type_label: field_type.to_owned(),
                value_type: cached_contract_value_type(field_type),
            })
            .collect();
        cache.insert(schema.to_owned(), Arc::clone(&fields));
        fields
    })
}

fn parse_inline_object_schema(schema: &str) -> Vec<(&str, &str)> {
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
            Some((name, field_type))
        })
        .collect()
}

fn is_runtime_metadata_state_field(name: &str) -> bool {
    name == "source_module"
}

fn cached_contract_value_type(field_type: &str) -> ContractValueType {
    CONTRACT_TYPE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(value_type) = cache.get(field_type) {
            return *value_type;
        }
        let value_type = ContractValueType::from_contract_type(field_type);
        cache.insert(field_type.to_owned(), value_type);
        value_type
    })
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ContractValueType {
    Bool,
    Number,
    Integer,
    String,
    Object,
    Array,
    Any,
}

impl ContractValueType {
    fn from_contract_type(field_type: &str) -> Self {
        let normalized = field_type.trim();
        if normalized.starts_with('[') && normalized.ends_with(']') {
            return Self::Array;
        }

        if normalized.eq_ignore_ascii_case("bool") || normalized.eq_ignore_ascii_case("boolean") {
            Self::Bool
        } else if normalized.eq_ignore_ascii_case("float")
            || normalized.eq_ignore_ascii_case("double")
            || normalized.eq_ignore_ascii_case("number")
        {
            Self::Number
        } else if normalized.eq_ignore_ascii_case("int")
            || normalized.eq_ignore_ascii_case("integer")
        {
            Self::Integer
        } else if normalized.eq_ignore_ascii_case("string") {
            Self::String
        } else if normalized.eq_ignore_ascii_case("object")
            || normalized.eq_ignore_ascii_case("table")
            || normalized.eq_ignore_ascii_case("map")
        {
            Self::Object
        } else {
            Self::Any
        }
    }

    fn matches(self, value: &serde_json::Value) -> bool {
        match self {
            Self::Bool => value.is_boolean(),
            Self::Number => value.is_number(),
            Self::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
            Self::String => value.is_string(),
            Self::Object => value.is_object(),
            Self::Array => value.is_array(),
            Self::Any => true,
        }
    }
}

fn json_value_matches_contract_type(value: &serde_json::Value, field_type: &str) -> bool {
    cached_contract_value_type(field_type).matches(value)
}

#[cfg(test)]
fn json_value_matches_contract_type_old(value: &serde_json::Value, field_type: &str) -> bool {
    let normalized = field_type.trim();
    if normalized.starts_with('[') && normalized.ends_with(']') {
        return value.is_array();
    }

    if normalized.eq_ignore_ascii_case("bool") || normalized.eq_ignore_ascii_case("boolean") {
        value.is_boolean()
    } else if normalized.eq_ignore_ascii_case("float")
        || normalized.eq_ignore_ascii_case("double")
        || normalized.eq_ignore_ascii_case("number")
    {
        value.is_number()
    } else if normalized.eq_ignore_ascii_case("int") || normalized.eq_ignore_ascii_case("integer") {
        value.as_i64().is_some() || value.as_u64().is_some()
    } else if normalized.eq_ignore_ascii_case("string") {
        value.is_string()
    } else if normalized.eq_ignore_ascii_case("object")
        || normalized.eq_ignore_ascii_case("table")
        || normalized.eq_ignore_ascii_case("map")
    {
        value.is_object()
    } else {
        true
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

#[cfg(test)]
mod contract_validation_performance_tests {
    use super::*;

    #[test]
    fn borrowed_inline_schema_parser_preserves_trimmed_fields() {
        assert_eq!(
            parse_inline_object_schema("{ device_id: string, level: FLOAT }").as_slice(),
            &[("device_id", "string"), ("level", "FLOAT")]
        );
        assert!(parse_inline_object_schema("string").is_empty());
    }

    #[test]
    fn compiled_inline_schema_cache_reuses_schema_fields() {
        let first = compiled_inline_object_schema("{ device_id: string, level: FLOAT }");
        let second = compiled_inline_object_schema("{ device_id: string, level: FLOAT }");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].name, "device_id");
        assert_eq!(first[0].value_type, ContractValueType::String);
        assert_eq!(first[1].value_type, ContractValueType::Number);
    }

    #[test]
    fn compiled_event_validation_preserves_warnings() {
        let warnings = event_payload_contract_warnings(
            "mesh.audio",
            "VolumeChanged",
            "{ device_id: string, level: float }",
            &serde_json::json!({ "device_id": 7, "other": true }),
        );

        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("payload field 'device_id' expected string, got number"));
        assert!(warnings[1].contains("missing required payload field 'level'"));
    }

    #[test]
    fn allocation_free_type_matching_preserves_supported_aliases() {
        let cases = [
            (serde_json::json!(true), " BOOLEAN ", true),
            (serde_json::json!(1.5), "FLOAT", true),
            (serde_json::json!(1.5), "integer", false),
            (serde_json::json!(1), "INT", true),
            (serde_json::json!("value"), "String", true),
            (serde_json::json!({}), "MAP", true),
            (serde_json::json!([]), "[string]", true),
            (serde_json::json!(null), "custom_type", true),
        ];
        for (value, field_type, expected) in cases {
            assert_eq!(
                json_value_matches_contract_type(&value, field_type),
                expected,
                "type {field_type}"
            );
        }
    }

    #[test]
    #[ignore = "release-only contract validation microbenchmark"]
    fn allocation_free_contract_type_matching_beats_lowercase_allocation() {
        use std::hint::black_box;
        use std::time::Instant;

        fn old_matches(value: &serde_json::Value, field_type: &str) -> bool {
            let normalized = field_type.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "float" | "double" | "number" => value.is_number(),
                _ => true,
            }
        }

        let value = serde_json::json!(42.5);
        let iterations = 1_000_000;
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(old_matches(&value, black_box(" NUMBER ")));
        }
        let allocating = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(json_value_matches_contract_type(
                &value,
                black_box(" NUMBER "),
            ));
        }
        let allocation_free = started.elapsed();

        eprintln!(
            "contract type checks over {iterations} iterations: lowercase allocation {allocating:?}, allocation-free {allocation_free:?}"
        );
    }

    // cargo test -p mesh-core-shell --release -- cached_event_schema_validation_beats_parse_per_event --ignored --nocapture
    #[test]
    #[ignore = "release-only event schema validation microbenchmark"]
    fn cached_event_schema_validation_beats_parse_per_event() {
        use std::hint::black_box;
        use std::time::Instant;

        fn old_event_warnings(schema: &str, payload: &serde_json::Value) -> usize {
            let fields = parse_inline_object_schema(schema);
            let Some(object) = payload.as_object() else {
                return 1;
            };
            let mut warnings = 0usize;
            for (field_name, field_type) in fields {
                let Some(value) = object.get(field_name) else {
                    warnings += 1;
                    continue;
                };
                if !json_value_matches_contract_type_old(value, field_type) {
                    warnings += 1;
                }
            }
            warnings
        }

        let schema = "{ device_id: string, level: float, muted: bool, channels: [string] }";
        let payload = serde_json::json!({
            "device_id": "default",
            "level": 42.0,
            "muted": false,
            "channels": ["front-left", "front-right"]
        });
        let iterations = 300_000usize;

        let started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total += old_event_warnings(black_box(schema), black_box(&payload));
        }
        let parse_each_time = started.elapsed();

        let started = Instant::now();
        let mut cached_total = 0usize;
        for _ in 0..iterations {
            cached_total += event_payload_contract_warnings(
                "mesh.audio",
                "VolumeChanged",
                black_box(schema),
                black_box(&payload),
            )
            .len();
        }
        let cached = started.elapsed();

        eprintln!(
            "event schema validation over {iterations} iterations: parse-per-event {parse_each_time:?}, cached {cached:?}, ratio {:.1}x",
            parse_each_time.as_secs_f64() / cached.as_secs_f64()
        );
        assert_eq!(old_total, cached_total);
        assert!(cached < parse_each_time);
    }
}
