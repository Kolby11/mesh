//! Provider-owned interface event contracts used at the backend queue edge.

use mesh_core_service::{InterfaceContract, InterfaceEvent, InterfaceTypeDef, TypeExpr};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendEventSpec {
    pub name: String,
    pub payload: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackendEventRegistry {
    events: HashMap<String, BackendEventSpec>,
    types: HashMap<String, InterfaceTypeDef>,
}

impl BackendEventRegistry {
    pub fn from_contract(contract: &InterfaceContract) -> Self {
        Self {
            events: contract
                .events
                .iter()
                .map(|event| (event.name.clone(), event_spec(event)))
                .collect(),
            types: contract.types.clone(),
        }
    }

    pub fn event(&self, name: &str) -> Option<&BackendEventSpec> {
        self.events.get(name)
    }

    pub fn validate_payload(&self, name: &str, payload: &JsonValue) -> Result<(), String> {
        let Some(spec) = self.events.get(name) else {
            return Err(format!("unsupported interface event: {name}"));
        };
        let Some(object) = payload.as_object() else {
            return Err(format!("event '{name}' payload must be a JSON object"));
        };
        for (field_name, type_expr) in &spec.payload {
            let value = match object.get(field_name) {
                Some(value) => value,
                None if type_expr.trim().ends_with('?') => continue,
                None => return Err(format!("missing required event field '{field_name}'")),
            };
            let value_type = TypeExpr::parse(type_expr)
                .map_err(|error| format!("invalid event field type for '{field_name}': {error}"))?;
            if !value_type.matches_with_types(value, &self.types) {
                return Err(format!(
                    "event field '{field_name}' expected {type_expr}, got {}",
                    json_type_name(value)
                ));
            }
        }
        if let Some(unknown) = object
            .keys()
            .find(|key| !spec.payload.iter().any(|(name, _)| name == *key))
        {
            return Err(format!("unknown event field '{unknown}'"));
        }
        Ok(())
    }
}

fn event_spec(event: &InterfaceEvent) -> BackendEventSpec {
    BackendEventSpec {
        name: event.name.clone(),
        payload: event
            .payload
            .iter()
            .map(|field| (field.name.clone(), field.arg_type.clone()))
            .collect(),
    }
}

fn json_type_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "boolean",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_service::{ContractCapabilities, InterfaceArgument, parse_contract_version};

    #[test]
    fn registry_rejects_unknown_and_malformed_provider_events() {
        let contract = InterfaceContract {
            interface: "mesh.audio".into(),
            version: parse_contract_version("1.0").unwrap(),
            state_fields: Vec::new(),
            methods: Vec::new(),
            events: vec![InterfaceEvent {
                name: "Changed".into(),
                payload: vec![InterfaceArgument {
                    name: "level".into(),
                    arg_type: "int".into(),
                }],
            }],
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        };
        let registry = BackendEventRegistry::from_contract(&contract);
        assert!(
            registry
                .validate_payload("Missing", &serde_json::json!({}))
                .is_err()
        );
        assert!(
            registry
                .validate_payload("Changed", &serde_json::json!({ "level": "loud" }))
                .is_err()
        );
        assert!(
            registry
                .validate_payload("Changed", &serde_json::json!({ "level": 42 }))
                .is_ok()
        );
    }
}
