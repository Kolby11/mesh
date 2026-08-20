use semver::{Version, VersionReq};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// A parsed interface contract.
///
/// Contracts are declared inline in `module.json`, or in a module-relative
/// JSON file referenced by an interface declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceContract {
    pub interface: String,
    pub version: Version,
    /// Documented core state fields that providers must emit. These are read
    /// through the service proxy as plain field access (e.g. `audio.percent`)
    /// and are never callable methods.
    pub state_fields: Vec<ContractStateField>,
    /// Mutating command methods callable from frontend scripts. Read-style
    /// accessors are NOT included here — they must use `state_fields` instead.
    pub methods: Vec<InterfaceMethod>,
    pub events: Vec<InterfaceEvent>,
    pub types: HashMap<String, InterfaceTypeDef>,
    pub capabilities: ContractCapabilities,
}

/// A documented core state field that providers must include in emitted payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractStateField {
    pub name: String,
    pub field_type: String,
    #[allow(dead_code)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceMethod {
    pub name: String,
    pub args: Vec<InterfaceArgument>,
    pub returns: Option<String>,
    /// When true, repeated invocations of this method on a backend's command
    /// queue are coalesced — only the most recent payload is executed and
    /// older queued instances are dropped. Right for idempotent setters
    /// (set_volume, set_muted) where intermediate values are stale; wrong for
    /// relative/accumulating commands (volume_up, increment).
    pub coalesce: bool,
    /// Optional reactive state binding. When the command is accepted, the
    /// shell writes the command value into the service's shared state and
    /// publishes it to every observer before the provider confirms it.
    pub state_binding: Option<StateBinding>,
}

/// Contract-declared binding between a command and shared service state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateBinding {
    /// Public service-state field controlled by the command.
    pub field: String,
    /// Command argument whose value is written into the shared state field.
    pub from_arg: Option<String>,
    /// Negate the current boolean state instead of copying an argument.
    pub toggle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceArgument {
    pub name: String,
    pub arg_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceEvent {
    pub name: String,
    /// Typed payload fields. Empty means the event carries no declared payload.
    pub payload: Vec<InterfaceArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceTypeDef {
    pub name: String,
    pub fields: Vec<InterfaceArgument>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractCapabilities {
    pub required: Vec<String>,
    pub optional: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("failed to parse interface contract for {interface}: {message}")]
    Parse { interface: String, message: String },

    #[error("invalid interface version '{value}' for {interface}")]
    InvalidVersion { interface: String, value: String },

    #[error("invalid type in contract for {interface}: {message}")]
    InvalidType { interface: String, message: String },
}

/// A parsed type expression from the contract type grammar.
///
/// Grammar: `base`, `base[]`, `base?`, `base[]?` where `base` is a primitive
/// (`string`, `int`, `float`, `boolean`, `object`, `any`) or a named type
/// declared in the contract's `types` map (plus the builtin `Result`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeExpr {
    pub base: BaseType,
    pub array: bool,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseType {
    String,
    Int,
    Float,
    Boolean,
    Object,
    Any,
    Named(String),
}

/// Builtin named type for command results (`{ ok: boolean, error: string? }`).
pub const BUILTIN_RESULT_TYPE: &str = "Result";

impl TypeExpr {
    pub fn parse(expr: &str) -> Result<Self, String> {
        let mut rest = expr.trim();
        if rest.is_empty() {
            return Err("type expression cannot be empty".to_string());
        }
        let mut optional = false;
        if let Some(stripped) = rest.strip_suffix('?') {
            optional = true;
            rest = stripped.trim_end();
        }
        let mut array = false;
        if let Some(stripped) = rest.strip_suffix("[]") {
            array = true;
            rest = stripped.trim_end();
        }
        if rest.is_empty() {
            return Err(format!("type expression '{expr}' has no base type"));
        }
        let base = match rest {
            "string" => BaseType::String,
            "int" => BaseType::Int,
            "float" => BaseType::Float,
            "boolean" => BaseType::Boolean,
            "object" => BaseType::Object,
            "any" => BaseType::Any,
            named => {
                if !named
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
                    || !named.chars().all(|ch| ch.is_ascii_alphanumeric())
                {
                    return Err(format!(
                        "invalid type '{expr}': expected a primitive (string, int, float, boolean, object, any) or a PascalCase named type"
                    ));
                }
                BaseType::Named(named.to_string())
            }
        };
        Ok(Self {
            base,
            array,
            optional,
        })
    }

    /// Check a JSON value against this type expression. Named types match
    /// objects (structural field checks are the caller's concern).
    pub fn matches(&self, value: &JsonValue) -> bool {
        if value.is_null() {
            return self.optional;
        }
        if self.array {
            return value.is_array();
        }
        match &self.base {
            BaseType::String => value.is_string(),
            BaseType::Int => value.as_i64().is_some() || value.as_u64().is_some(),
            BaseType::Float => value.is_number(),
            BaseType::Boolean => value.is_boolean(),
            BaseType::Object => value.is_object(),
            BaseType::Any => true,
            BaseType::Named(_) => value.is_object(),
        }
    }
}

/// Parse and validate an inline or external contract JSON object into an
/// [`InterfaceContract`].
///
/// Every type expression in the contract is validated against the type
/// grammar, and named types must be declared in `types` (or be the builtin
/// `Result`).
pub fn parse_interface_contract(
    interface_name: &str,
    interface_version: &str,
    contract: &JsonValue,
) -> Result<InterfaceContract, ContractError> {
    let contract = normalize_keyed_contract_declarations(contract.clone()).map_err(|message| {
        ContractError::Parse {
            interface: interface_name.to_string(),
            message,
        }
    })?;
    let parsed: ContractJson =
        serde_json::from_value(contract).map_err(|source| ContractError::Parse {
            interface: interface_name.to_string(),
            message: source.to_string(),
        })?;

    let version =
        parse_contract_version(interface_version).ok_or_else(|| ContractError::InvalidVersion {
            interface: interface_name.to_string(),
            value: interface_version.to_string(),
        })?;

    let contract = InterfaceContract {
        interface: interface_name.to_string(),
        version,
        state_fields: parsed
            .state
            .into_iter()
            .map(|field| ContractStateField {
                name: field.name,
                field_type: field.field_type,
                description: field.description,
            })
            .collect(),
        methods: parsed
            .methods
            .into_iter()
            .map(|method| InterfaceMethod {
                name: method.name,
                args: method
                    .args
                    .into_iter()
                    .map(ContractFieldJson::into_argument)
                    .collect(),
                returns: method.returns,
                coalesce: method.coalesce,
                state_binding: method.state_binding.map(|value| StateBinding {
                    field: value.field,
                    from_arg: value.from_arg,
                    toggle: value.toggle,
                }),
            })
            .collect(),
        events: parsed
            .events
            .into_iter()
            .map(|event| InterfaceEvent {
                name: event.name,
                payload: event
                    .payload
                    .into_iter()
                    .map(ContractFieldJson::into_argument)
                    .collect(),
            })
            .collect(),
        types: parsed
            .types
            .into_iter()
            .map(|(name, def)| {
                let fields = def
                    .fields
                    .into_iter()
                    .map(ContractFieldJson::into_argument)
                    .collect();
                (name.clone(), InterfaceTypeDef { name, fields })
            })
            .collect(),
        capabilities: ContractCapabilities {
            required: parsed.capabilities.required,
            optional: parsed.capabilities.optional,
        },
    };

    if let Some(message) = contract_type_errors(&contract).into_iter().next() {
        return Err(ContractError::InvalidType {
            interface: interface_name.to_string(),
            message,
        });
    }

    Ok(contract)
}

/// The compact external `contract.json` format keys state, methods, and
/// events by their public names. Normalize it to the established array form so
/// both authoring shapes compile through exactly the same validation path.
fn normalize_keyed_contract_declarations(mut contract: JsonValue) -> Result<JsonValue, String> {
    let object = contract
        .as_object_mut()
        .ok_or_else(|| "contract must be a JSON object".to_string())?;

    for section in ["state", "methods", "events"] {
        let Some(value) = object.get_mut(section) else {
            continue;
        };
        let Some(entries) = value.as_object() else {
            continue;
        };
        let mut normalized = Vec::with_capacity(entries.len());
        for (name, declaration) in entries {
            let Some(mut declaration) = declaration.as_object().cloned() else {
                return Err(format!("contract {section}.{name} must be a JSON object"));
            };
            match declaration.get("name") {
                Some(JsonValue::String(existing)) if existing == name => {}
                Some(JsonValue::String(existing)) => {
                    return Err(format!(
                        "contract {section}.{name} names itself '{existing}', which does not match its key"
                    ));
                }
                Some(_) => return Err(format!("contract {section}.{name}.name must be a string")),
                None => {
                    declaration.insert("name".into(), JsonValue::String(name.clone()));
                }
            }
            normalized.push(JsonValue::Object(declaration));
        }
        *value = JsonValue::Array(normalized);
    }
    Ok(contract)
}

fn check_type_expr(
    errors: &mut Vec<String>,
    types: &HashMap<String, InterfaceTypeDef>,
    context: String,
    expr: &str,
) {
    match TypeExpr::parse(expr) {
        Ok(parsed) => {
            if let BaseType::Named(name) = &parsed.base
                && name != BUILTIN_RESULT_TYPE
                && !types.contains_key(name)
            {
                errors.push(format!(
                    "{context}: named type '{name}' is not declared in types"
                ));
            }
        }
        Err(message) => errors.push(format!("{context}: {message}")),
    }
}

/// Collect every type-grammar violation in the contract. Empty means valid.
pub fn contract_type_errors(contract: &InterfaceContract) -> Vec<String> {
    let mut errors = Vec::new();
    let check = |errors: &mut Vec<String>, context: String, expr: &str| {
        check_type_expr(errors, &contract.types, context, expr)
    };

    for field in &contract.state_fields {
        check(
            &mut errors,
            format!("state field '{}'", field.name),
            &field.field_type,
        );
    }
    for method in &contract.methods {
        for arg in &method.args {
            check(
                &mut errors,
                format!("method '{}' arg '{}'", method.name, arg.name),
                &arg.arg_type,
            );
        }
        if let Some(returns) = &method.returns {
            check(
                &mut errors,
                format!("method '{}' returns", method.name),
                returns,
            );
        }
        if let Some(binding) = &method.state_binding {
            let bound_field = contract
                .state_fields
                .iter()
                .find(|field| field.name == binding.field);
            if bound_field.is_none() {
                errors.push(format!(
                    "method '{}' stateBinding field '{}' is not a declared state field",
                    method.name, binding.field
                ));
            }
            if binding.from_arg.is_some() == binding.toggle {
                errors.push(format!(
                    "method '{}' stateBinding must declare exactly one of fromArg or toggle=true",
                    method.name
                ));
            } else if let Some(from_arg) = &binding.from_arg {
                let source_arg = method.args.iter().find(|arg| &arg.name == from_arg);
                if source_arg.is_none() {
                    errors.push(format!(
                        "method '{}' stateBinding fromArg '{}' is not a declared argument",
                        method.name, from_arg
                    ));
                } else if let (Some(field), Some(arg)) = (bound_field, source_arg)
                    && field.field_type != arg.arg_type
                {
                    errors.push(format!(
                        "method '{}' stateBinding type mismatch: state field '{}' is '{}' but argument '{}' is '{}'",
                        method.name,
                        field.name,
                        field.field_type,
                        arg.name,
                        arg.arg_type
                    ));
                }
            } else if let Some(field) = bound_field
                && field.field_type != "boolean"
            {
                errors.push(format!(
                    "method '{}' toggle stateBinding field '{}' must be boolean",
                    method.name, field.name
                ));
            }
        }
    }
    for event in &contract.events {
        for field in &event.payload {
            check(
                &mut errors,
                format!("event '{}' payload field '{}'", event.name, field.name),
                &field.arg_type,
            );
        }
    }
    for def in contract.types.values() {
        for field in &def.fields {
            check(
                &mut errors,
                format!("type '{}' field '{}'", def.name, field.name),
                &field.arg_type,
            );
        }
    }
    errors
}

pub fn parse_contract_version(value: &str) -> Option<Version> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    Version::parse(trimmed).ok().or_else(|| {
        let component_count = trimmed.split('.').count();
        let normalized = match component_count {
            1 => format!("{trimmed}.0.0"),
            2 => format!("{trimmed}.0"),
            _ => return None,
        };
        Version::parse(&normalized).ok()
    })
}

pub fn parse_version_req(value: &str) -> Option<VersionReq> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed
        .chars()
        .any(|ch| matches!(ch, '<' | '>' | '=' | '^' | '~' | ',' | '*'))
    {
        return VersionReq::parse(trimmed).ok();
    }

    parse_contract_version(trimmed)
        .and_then(|version| VersionReq::parse(&format!("={version}")).ok())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractJson {
    #[serde(default)]
    state: Vec<ContractStateFieldJson>,
    #[serde(default)]
    methods: Vec<ContractMethodJson>,
    #[serde(default)]
    events: Vec<ContractEventJson>,
    #[serde(default)]
    types: HashMap<String, ContractTypeJson>,
    #[serde(default)]
    capabilities: ContractCapabilitiesJson,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractStateFieldJson {
    name: String,
    #[serde(rename = "type")]
    field_type: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractMethodJson {
    name: String,
    #[serde(default)]
    args: Vec<ContractFieldJson>,
    #[serde(default)]
    returns: Option<String>,
    #[serde(default)]
    coalesce: bool,
    #[serde(default, rename = "stateBinding")]
    state_binding: Option<ContractStateBindingJson>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractStateBindingJson {
    field: String,
    #[serde(default, rename = "fromArg")]
    from_arg: Option<String>,
    #[serde(default)]
    toggle: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractEventJson {
    name: String,
    #[serde(default)]
    payload: Vec<ContractFieldJson>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractTypeJson {
    #[serde(default)]
    fields: Vec<ContractFieldJson>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractFieldJson {
    name: String,
    #[serde(rename = "type")]
    arg_type: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractCapabilitiesJson {
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    optional: Vec<String>,
}

impl ContractFieldJson {
    fn into_argument(self) -> InterfaceArgument {
        InterfaceArgument {
            name: self.name,
            arg_type: self.arg_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_semver_contract_version() {
        let version = parse_contract_version("1.0").unwrap();
        assert_eq!(version.to_string(), "1.0.0");
    }

    #[test]
    fn parses_exact_request_from_short_version() {
        let req = parse_version_req("1.0").unwrap();
        assert!(req.matches(&Version::parse("1.0.0").unwrap()));
        assert!(!req.matches(&Version::parse("1.1.0").unwrap()));
    }

    #[test]
    fn parses_contract_json_shape() {
        let contract = serde_json::json!({
            "methods": [
                { "name": "sensors", "returns": "Sensor[]" },
                {
                    "name": "read",
                    "args": [{ "name": "sensor_id", "type": "string" }],
                    "returns": "float"
                }
            ],
            "events": [
                {
                    "name": "TemperatureChanged",
                    "payload": [
                        { "name": "sensor_id", "type": "string" },
                        { "name": "celsius", "type": "float" }
                    ]
                }
            ],
            "types": {
                "Sensor": {
                    "fields": [
                        { "name": "id", "type": "string" },
                        { "name": "name", "type": "string" }
                    ]
                }
            },
            "capabilities": { "required": ["service.thermal.read"] }
        });

        let contract = parse_interface_contract("alice.thermal", "1.0", &contract).unwrap();

        assert_eq!(contract.interface, "alice.thermal");
        assert_eq!(contract.version.to_string(), "1.0.0");
        assert_eq!(contract.methods.len(), 2);
        assert_eq!(contract.methods[0].returns.as_deref(), Some("Sensor[]"));
        assert_eq!(contract.events[0].name, "TemperatureChanged");
        assert_eq!(contract.events[0].payload.len(), 2);
        assert_eq!(
            contract.capabilities.required,
            vec!["service.thermal.read".to_string()]
        );
        assert!(contract.types.contains_key("Sensor"));
    }

    #[test]
    fn parses_keyed_contract_json_shape() {
        let contract = serde_json::json!({
            "state": { "percent": { "type": "float" } },
            "methods": {
                "set_percent": {
                    "args": [{ "name": "value", "type": "float" }],
                    "stateBinding": { "field": "percent", "fromArg": "value" }
                }
            },
            "events": { "Changed": { "payload": [] } },
            "types": { "Device": { "fields": [] } }
        });
        let parsed = parse_interface_contract("mesh.audio", "1.0", &contract).unwrap();
        assert_eq!(parsed.state_fields[0].name, "percent");
        assert_eq!(parsed.methods[0].name, "set_percent");
        assert_eq!(parsed.events[0].name, "Changed");
        assert!(parsed.types.contains_key("Device"));
    }

    #[test]
    fn parses_command_state_binding() {
        let contract = serde_json::json!({
            "state": [
                {
                    "name": "available",
                    "type": "boolean",
                    "description": "Whether the service is reachable"
                },
                { "name": "muted", "type": "boolean" }
            ],
            "methods": [
                {
                    "name": "set_muted",
                    "args": [
                        { "name": "device_id", "type": "string" },
                        { "name": "muted", "type": "boolean" }
                    ],
                    "returns": "Result",
                    "coalesce": true,
                    "stateBinding": { "field": "muted", "fromArg": "muted" }
                },
                {
                    "name": "toggle_mute",
                    "stateBinding": { "field": "muted", "toggle": true }
                }
            ]
        });

        let contract = parse_interface_contract("mesh.audio", "1.0", &contract).unwrap();

        assert_eq!(contract.state_fields.len(), 2);
        assert_eq!(contract.state_fields[0].name, "available");
        assert_eq!(
            contract.state_fields[0].description.as_deref(),
            Some("Whether the service is reachable")
        );
        let set_muted = &contract.methods[0];
        assert!(set_muted.coalesce);
        let binding = set_muted.state_binding.as_ref().unwrap();
        assert_eq!(binding.field, "muted");
        assert_eq!(binding.from_arg.as_deref(), Some("muted"));
        assert!(!binding.toggle);
        let toggle = contract.methods[1].state_binding.as_ref().unwrap();
        assert_eq!(toggle.field, "muted");
        assert_eq!(toggle.from_arg, None);
        assert!(toggle.toggle);
    }

    #[test]
    fn rejects_legacy_optimistic_annotation() {
        let contract = serde_json::json!({
            "state": [{ "name": "level", "type": "float" }],
            "methods": [{
                "name": "set",
                "args": [{ "name": "level", "type": "float" }],
                "optimistic": { "field": "level", "fromArg": "level" }
            }]
        });

        let err = parse_interface_contract("mesh.brightness", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::Parse { .. }));
        assert!(err.to_string().contains("unknown field `optimistic`"));
    }

    #[test]
    fn rejects_state_binding_type_mismatch() {
        let contract = serde_json::json!({
            "state": [{ "name": "level", "type": "float" }],
            "methods": [{
                "name": "set",
                "args": [{ "name": "level", "type": "string" }],
                "stateBinding": { "field": "level", "fromArg": "level" }
            }]
        });

        let err = parse_interface_contract("mesh.brightness", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
        assert!(err.to_string().contains("stateBinding type mismatch"));
    }

    #[test]
    fn rejects_toggle_binding_for_non_boolean_state() {
        let contract = serde_json::json!({
            "state": [{ "name": "level", "type": "float" }],
            "methods": [{
                "name": "toggle",
                "stateBinding": { "field": "level", "toggle": true }
            }]
        });

        let err = parse_interface_contract("mesh.brightness", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
        assert!(err.to_string().contains("must be boolean"));
    }

    #[test]
    fn rejects_undeclared_named_type() {
        let contract = serde_json::json!({
            "methods": [{ "name": "sensors", "returns": "Sensor[]" }]
        });
        let err = parse_interface_contract("alice.thermal", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
        assert!(err.to_string().contains("Sensor"));
    }

    #[test]
    fn rejects_invalid_type_expression() {
        let contract = serde_json::json!({
            "state": [{ "name": "percent", "type": "[float]" }]
        });
        let err = parse_interface_contract("mesh.audio", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::InvalidType { .. }));
    }

    #[test]
    fn rejects_unknown_contract_keys() {
        let contract = serde_json::json!({ "state_fields": [] });
        let err = parse_interface_contract("mesh.audio", "1.0", &contract).unwrap_err();
        assert!(matches!(err, ContractError::Parse { .. }));
    }

    #[test]
    fn type_expr_grammar_and_matching() {
        let expr = TypeExpr::parse("string").unwrap();
        assert!(expr.matches(&serde_json::json!("hi")));
        assert!(!expr.matches(&serde_json::json!(1)));
        assert!(!expr.matches(&serde_json::Value::Null));

        let expr = TypeExpr::parse("float?").unwrap();
        assert!(expr.matches(&serde_json::json!(1.5)));
        assert!(expr.matches(&serde_json::Value::Null));

        let expr = TypeExpr::parse("Sensor[]").unwrap();
        assert!(expr.array);
        assert!(expr.matches(&serde_json::json!([])));
        assert!(!expr.matches(&serde_json::json!({})));

        let expr = TypeExpr::parse("int").unwrap();
        assert!(expr.matches(&serde_json::json!(3)));
        assert!(!expr.matches(&serde_json::json!(3.5)));

        assert!(TypeExpr::parse("lowercaseNamed").is_err());
        assert!(TypeExpr::parse("").is_err());
        assert!(TypeExpr::parse("[Sensor]").is_err());
    }
}
