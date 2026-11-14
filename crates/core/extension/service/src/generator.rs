//! Deterministic authoring artifacts generated from compiled service contracts.
//!
//! These generators intentionally return strings instead of writing files. The
//! CLI, LSP, package tooling, and editor integrations can choose their output
//! location without giving the contract compiler filesystem authority.

use crate::contract::{
    BUILTIN_RESULT_TYPE, BaseType, CompiledContract, CompiledEventSchema, CompiledMethodSchema,
    CompiledStateField, CompiledTypeSchema, TypeExpr,
};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fmt::Write;

mod rust;

pub use rust::generate_rust_bindings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedContractArtifacts {
    pub consumer_types: String,
    pub provider_stub: String,
    pub mock: String,
    pub documentation: String,
}

pub fn generate_contract_artifacts(contract: &CompiledContract) -> GeneratedContractArtifacts {
    GeneratedContractArtifacts {
        consumer_types: generate_luau_consumer_types(contract),
        provider_stub: generate_luau_provider_stub(contract),
        mock: generate_luau_mock(contract),
        documentation: generate_contract_documentation(contract),
    }
}

/// Generate the Luau type declarations (`<interface>.d.luau`) for the proxy a
/// frontend gets from `require("<interface>")`.
///
/// The proxy exposes every state field directly and under `state`, each event
/// as a channel (directly and under `events`), and each method as a call that
/// returns a ticket. `available` is always present: it reads `false` until the
/// service publishes state. Ranges and defaults are documented on the field.
pub fn generate_luau_consumer_types(contract: &CompiledContract) -> String {
    let mut output = generated_header(contract, "consumer types");
    for definition in contract.schemas.types.values() {
        render_type_definition(&mut output, definition);
    }
    output.push_str(CONSUMER_PRELUDE);

    output.push_str("export type State = {\n");
    if !contract
        .schemas
        .state_fields
        .iter()
        .any(|field| field.name == "available")
    {
        output.push_str("    available: boolean,\n");
    }
    for field in contract.schemas.state_fields.iter() {
        if let Some(description) = &field.description {
            writeln!(output, "    -- {description}").unwrap();
        }
        let mut notes = Vec::new();
        if field.minimum.is_some() || field.maximum.is_some() {
            let bound = |value: &Option<serde_json::Number>| {
                value.as_ref().map(ToString::to_string).unwrap_or_default()
            };
            notes.push(format!(
                "range {}..{}",
                bound(&field.minimum),
                bound(&field.maximum)
            ));
        }
        if let Some(default) = &field.default {
            notes.push(format!("default {default}"));
        }
        if field.type_expr.base == BaseType::Int && !field.type_expr.array {
            notes.push("integer".to_string());
        }
        let notes = if notes.is_empty() {
            String::new()
        } else {
            format!(" -- {}", notes.join(", "))
        };
        writeln!(
            output,
            "    {}: {},{notes}",
            field.name,
            luau_type(&field.type_expr)
        )
        .unwrap();
    }
    output.push_str("}\n\n");

    output.push_str("export type Events = {\n");
    for event in contract.schemas.events.iter() {
        writeln!(
            output,
            "    {}: Channel<{}>,",
            event.name,
            luau_event_payload_type(event)
        )
        .unwrap();
    }
    output.push_str("}\n\n");

    output.push_str("export type Service = State & Events & {\n");
    output.push_str("    state: State,\n    events: Events,\n");
    let feature_names = contract
        .features
        .keys()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>();
    let feature_type = if feature_names.is_empty() {
        "string".to_string()
    } else {
        feature_names.join(" | ")
    };
    writeln!(
        output,
        "    -- Whether the bound provider supplies an optional feature.\n    supports: (self: any, feature: {feature_type}) -> boolean,"
    )
    .unwrap();
    for method in contract.schemas.methods.iter() {
        writeln!(output, "    {}: {},", method.name, luau_method_type(method)).unwrap();
    }
    output.push_str("}\n\nreturn {}\n");
    output
}

const CONSUMER_PRELUDE: &str = "\
export type Channel<T> = {
    on: (self: any, handler: (payload: T) -> ()) -> any,
}

export type CallSnapshot<R> = {
    call_id: number,
    done: boolean,
    status: string?,
    result: R?,
}

export type Ticket<R> = {
    call_id: number,
    status: string,
    ok: boolean,
    queued: boolean?,
    error: string?,
    poll: (self: any) -> CallSnapshot<R>,
    await: (self: any) -> CallSnapshot<R>,
    cancel: (self: any) -> boolean,
}

";

/// Generate a provider skeleton with explicit `start(self)` setup and one
/// handler per declared command. Implementations fill in host-specific logic.
pub fn generate_luau_provider_stub(contract: &CompiledContract) -> String {
    let mut output = generated_header(contract, "provider stub");
    output.push_str("local Provider = {}\n\n");
    output.push_str("function Provider.start(self)\n");
    output.push_str("    -- Register polling and emit the initial state here.\n");
    output.push_str("    return self\n");
    output.push_str("end\n\n");
    for method in contract.schemas.methods.iter() {
        writeln!(
            output,
            "function Provider.{}({})",
            method.name,
            stub_parameters(method)
        )
        .unwrap();
        writeln!(
            output,
            "    error(\"implement {} for {}\")",
            method.name, contract.interface
        )
        .unwrap();
        output.push_str("end\n\n");
    }
    output.push_str("return Provider\n");
    output
}

/// Generate an in-memory provider mock with state, call recording, and
/// deterministic values that satisfy the contract: declared defaults, numbers
/// inside their declared range, empty arrays, and named records with every
/// required field. Methods return a value of their declared return type. A
/// value that cannot be generated (a required blob handle, or a required
/// recursive field) is listed at the top of the mock; its state field starts
/// nil and its method raises an error naming the reason.
pub fn generate_luau_mock(contract: &CompiledContract) -> String {
    let types = &contract.schemas.types;
    let mut unsupported = Vec::new();
    let mut state = String::new();
    for field in contract.schemas.state_fields.iter() {
        let value = match state_mock_value(field, types) {
            Ok(value) => luau_literal(&value),
            Err(reason) => {
                unsupported.push(format!("state.{}: {reason}", field.name));
                "nil".to_string()
            }
        };
        writeln!(state, "        {} = {value},", field.name).unwrap();
    }
    let mut methods = String::new();
    for method in contract.schemas.methods.iter() {
        writeln!(
            methods,
            "function Mock.{}({})",
            method.name,
            stub_parameters(method)
        )
        .unwrap();
        writeln!(
            methods,
            "    table.insert(self.calls, {{ method = \"{}\" }})",
            method.name
        )
        .unwrap();
        let returned = match &method.returns {
            Some(returns) => mock_value(returns, types, None, &mut Vec::new()),
            None => Ok(serde_json::json!({ "ok": true })),
        };
        match returned {
            Ok(value) => writeln!(methods, "    return {}", luau_literal(&value)).unwrap(),
            Err(reason) => {
                unsupported.push(format!("{}(): {reason}", method.name));
                writeln!(
                    methods,
                    "    error({})",
                    luau_string(&format!("mock cannot produce a {} result: {reason}", method.name))
                )
                .unwrap();
            }
        }
        methods.push_str("end\n\n");
    }

    let mut output = generated_header(contract, "provider mock");
    for entry in &unsupported {
        writeln!(output, "-- unsupported: {entry}").unwrap();
    }
    if !unsupported.is_empty() {
        output.push('\n');
    }
    output.push_str("local Mock = {\n    calls = {},\n    state = {\n");
    output.push_str(&state);
    output.push_str("    },\n}\n\n");
    output.push_str(&methods);
    output.push_str("return Mock\n");
    output
}

fn state_mock_value(
    field: &CompiledStateField,
    types: &BTreeMap<String, CompiledTypeSchema>,
) -> Result<JsonValue, String> {
    if let Some(default) = &field.default {
        return Ok(default.clone());
    }
    let range = (
        field.minimum.as_ref().and_then(serde_json::Number::as_f64),
        field.maximum.as_ref().and_then(serde_json::Number::as_f64),
    );
    mock_value(&field.type_expr, types, Some(range), &mut Vec::new())
}

/// The simplest value matching `expr`. `range` bounds a number; `visiting`
/// holds the named types being built, to stop on required recursion.
fn mock_value(
    expr: &TypeExpr,
    types: &BTreeMap<String, CompiledTypeSchema>,
    range: Option<(Option<f64>, Option<f64>)>,
    visiting: &mut Vec<String>,
) -> Result<JsonValue, String> {
    if expr.optional {
        return Ok(JsonValue::Null);
    }
    if expr.array {
        return Ok(JsonValue::Array(Vec::new()));
    }
    let in_range = |integral: bool| {
        let (minimum, maximum) = range.unwrap_or((None, None));
        let mut value = 0.0_f64;
        if let Some(minimum) = minimum.filter(|minimum| value < *minimum) {
            value = if integral { minimum.ceil() } else { minimum };
        }
        if let Some(maximum) = maximum.filter(|maximum| value > *maximum) {
            value = if integral { maximum.floor() } else { maximum };
        }
        value
    };
    Ok(match &expr.base {
        BaseType::String => JsonValue::String(String::new()),
        BaseType::Boolean | BaseType::Any => JsonValue::Bool(false),
        BaseType::Int => JsonValue::from(in_range(true) as i64),
        BaseType::Float => serde_json::Number::from_f64(in_range(false))
            .map_or(JsonValue::from(0), JsonValue::Number),
        BaseType::Object => JsonValue::Object(serde_json::Map::new()),
        BaseType::Blob => return Err("a blob handle cannot be generated".into()),
        BaseType::Named(name) if name == BUILTIN_RESULT_TYPE => serde_json::json!({ "ok": true }),
        BaseType::Named(name) => {
            if visiting.contains(name) {
                return Err(format!("required field recursion through {name}"));
            }
            let schema = types
                .get(name)
                .ok_or_else(|| format!("named type {name} is not declared"))?;
            visiting.push(name.clone());
            let mut record = serde_json::Map::new();
            for field in schema.fields.iter() {
                if field.type_expr.optional {
                    continue;
                }
                let value = mock_value(&field.type_expr, types, None, visiting)
                    .map_err(|reason| format!("{name}.{}: {reason}", field.name))?;
                record.insert(field.name.clone(), value);
            }
            visiting.pop();
            JsonValue::Object(record)
        }
    })
}

/// Render JSON as a Luau literal. `null` becomes `nil`.
fn luau_literal(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "nil".into(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(number) => number.to_string(),
        JsonValue::String(text) => luau_string(text),
        JsonValue::Array(items) if items.is_empty() => "{}".into(),
        JsonValue::Array(items) => {
            let items = items.iter().map(luau_literal).collect::<Vec<_>>();
            format!("{{ {} }}", items.join(", "))
        }
        JsonValue::Object(entries) if entries.is_empty() => "{}".into(),
        JsonValue::Object(entries) => {
            let entries = entries
                .iter()
                .map(|(key, value)| format!("[{}] = {}", luau_string(key), luau_literal(value)))
                .collect::<Vec<_>>();
            format!("{{ {} }}", entries.join(", "))
        }
    }
}

fn luau_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for ch in text.chars() {
        match ch {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            ch if (ch as u32) < 0x20 => write!(quoted, "\\x{:02x}", ch as u32).unwrap(),
            ch => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

/// Generate standalone Markdown documentation from the same schema and policy
/// used by runtime validation and compatibility checks.
pub fn generate_contract_documentation(contract: &CompiledContract) -> String {
    let mut output = String::new();
    writeln!(output, "# `{}`", contract.interface).unwrap();
    writeln!(output).unwrap();
    writeln!(output, "- Version: `{}`", contract.version).unwrap();
    writeln!(
        output,
        "- Schema fingerprint: `{}`",
        contract.schema_fingerprint
    )
    .unwrap();
    writeln!(
        output,
        "- Policy fingerprint: `{}`",
        contract.policy_fingerprint
    )
    .unwrap();
    writeln!(
        output,
        "- Behavior fingerprint: `{}`",
        contract.behavior_fingerprint
    )
    .unwrap();
    if let Some(module) = &contract.provenance.module {
        writeln!(output, "- Declared by module: `{module}`").unwrap();
    }
    if let Some(source) = &contract.provenance.source {
        writeln!(output, "- Declaration source: `{source}`").unwrap();
    }

    output.push_str("\n## State\n\n| Name | Type |\n| --- | --- |\n");
    for field in contract.schemas.state_fields.iter() {
        writeln!(
            output,
            "| `{}` | `{}` |",
            field.name,
            luau_type(&field.type_expr)
        )
        .unwrap();
    }

    output.push_str(
        "\n## Methods\n\n| Name | Arguments | Returns | Coalesced |\n| --- | --- | --- | --- |\n",
    );
    for method in contract.schemas.methods.iter() {
        let args = method
            .args
            .iter()
            .map(|arg| format!("`{}`: `{}`", arg.name, luau_type(&arg.type_expr)))
            .collect::<Vec<_>>()
            .join(", ");
        let returns = method
            .returns
            .as_ref()
            .map(luau_type)
            .unwrap_or_else(|| "none".to_string());
        writeln!(
            output,
            "| `{}` | {} | `{}` | `{}` |",
            method.name, args, returns, method.coalesce
        )
        .unwrap();
    }

    output.push_str("\n## Events\n\n| Name | Payload |\n| --- | --- |\n");
    for event in contract.schemas.events.iter() {
        writeln!(
            output,
            "| `{}` | `{}` |",
            event.name,
            luau_event_payload_type(event)
        )
        .unwrap();
    }

    output.push_str("\n## Operation policy\n\n");
    write_policy_line(
        &mut output,
        "Read",
        contract.operation_policy.read.as_deref(),
    );
    for (name, policy) in contract.operation_policy.events.iter() {
        write_policy_line(&mut output, &format!("Event `{name}`"), Some(policy));
    }
    for (name, policy) in contract.operation_policy.methods.iter() {
        write_policy_line(&mut output, &format!("Method `{name}`"), Some(policy));
    }
    output
}

fn generated_header(contract: &CompiledContract, artifact: &str) -> String {
    format!(
        "-- GENERATED by MESH from {}@{} ({artifact}); do not edit.\n-- schema={} policy={} behavior={}\n\n",
        contract.interface,
        contract.version,
        contract.schema_fingerprint,
        contract.policy_fingerprint,
        contract.behavior_fingerprint,
    )
}

fn render_type_definition(output: &mut String, definition: &CompiledTypeSchema) {
    writeln!(output, "export type {} = {{", definition.name).unwrap();
    for field in definition.fields.iter() {
        writeln!(
            output,
            "    {}: {},",
            field.name,
            luau_type(&field.type_expr)
        )
        .unwrap();
    }
    output.push_str("}\n\n");
}

fn luau_method_type(method: &CompiledMethodSchema) -> String {
    let args = std::iter::once("self: any".to_string())
        .chain(
            method
                .args
                .iter()
                .map(|arg| format!("{}: {}", arg.name, luau_type(&arg.type_expr))),
        )
        .collect::<Vec<_>>()
        .join(", ");
    let returns = method
        .returns
        .as_ref()
        .map(luau_type)
        .unwrap_or_else(|| "nil".to_string());
    format!("({args}) -> Ticket<{returns}>")
}

fn luau_event_payload_type(event: &CompiledEventSchema) -> String {
    if event.payload.is_empty() {
        return "()".to_string();
    }
    let fields = event
        .payload
        .iter()
        .map(|field| format!("{}: {}", field.name, luau_type(&field.type_expr)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ {fields} }}")
}

fn stub_parameters(method: &CompiledMethodSchema) -> String {
    let arguments = method
        .args
        .iter()
        .map(|arg| arg.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if arguments.is_empty() {
        "self".to_string()
    } else {
        format!("self, {arguments}")
    }
}

/// The Luau type the generated `.d.luau` declares for a contract type
/// string. Tooling shows the same spelling the declarations use. A string
/// that does not parse is returned unchanged.
pub fn luau_type_of(contract_type: &str) -> String {
    TypeExpr::parse(contract_type)
        .map_or_else(|_| contract_type.to_string(), |expr| luau_type(&expr))
}

fn luau_type(expr: &TypeExpr) -> String {
    let base = match &expr.base {
        BaseType::String | BaseType::Blob => "string".to_string(),
        BaseType::Int | BaseType::Float => "number".to_string(),
        BaseType::Boolean => "boolean".to_string(),
        BaseType::Object => "{ [string]: unknown }".to_string(),
        BaseType::Any => "unknown".to_string(),
        BaseType::Named(name) if name == "Result" => "{ ok: boolean, error: string? }".to_string(),
        BaseType::Named(name) => name.clone(),
    };
    let base = if expr.array {
        format!("{{ {base} }}")
    } else {
        base
    };
    if expr.optional {
        format!("{base}?")
    } else {
        base
    }
}

fn write_policy_line(output: &mut String, label: &str, policy: Option<&[String]>) {
    let value = policy
        .map(|values| {
            values
                .iter()
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|| "compatibility fallback".to_string());
    writeln!(output, "- {label}: {value}").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        ContractCapabilities, InterfaceArgument, InterfaceContract, InterfaceMethod,
    };
    use semver::Version;
    use std::collections::HashMap;

    fn contract() -> CompiledContract {
        InterfaceContract {
            interface: "mesh.audio".into(),
            version: Version::parse("1.0.0").unwrap(),
            state_fields: vec![crate::contract::ContractStateField {
                name: "percent".into(),
                field_type: "float".into(),
                description: None,
                ..Default::default()
            }],
            methods: vec![InterfaceMethod {
                name: "set_percent".into(),
                args: vec![InterfaceArgument {
                    name: "value".into(),
                    arg_type: "float".into(),
                }],
                returns: Some("Result".into()),
                coalesce: true.into(),
                state_binding: None,
            }],
            events: vec![],
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        }
        .compile(crate::contract::DeclarationProvenance::unknown())
        .unwrap()
    }

    #[test]
    fn generates_all_artifacts_from_one_compiled_contract() {
        let contract = contract();
        let artifacts = generate_contract_artifacts(&contract);
        assert!(artifacts.consumer_types.contains("set_percent"));
        assert!(
            artifacts
                .provider_stub
                .contains("function Provider.start(self)")
        );
        assert!(artifacts.mock.contains("table.insert(self.calls"));
        assert!(artifacts.documentation.contains("Schema fingerprint"));
        assert!(artifacts.consumer_types.starts_with("-- GENERATED by MESH"));
    }

    #[test]
    fn consumer_types_describe_the_proxy_surface() {
        let contract = crate::contract::parse_compiled_contract(
            "mesh.audio",
            "1.0",
            &serde_json::json!({
                "state": {
                    "percent": {
                        "type": "float",
                        "minimum": 0,
                        "maximum": 100,
                        "description": "Output volume"
                    },
                    "muted": { "type": "boolean", "default": false },
                    "extra": "object?"
                },
                "methods": { "set_volume": "(percent: float) -> Result" },
                "events": { "VolumeChanged": { "level": "float" } }
            }),
        )
        .unwrap();
        let types = generate_luau_consumer_types(&contract);
        for expected in [
            "    available: boolean,\n",
            "    -- Output volume\n    percent: number, -- range 0..100\n",
            "    muted: boolean, -- default false\n",
            "    extra: { [string]: unknown }?,\n",
            "    VolumeChanged: Channel<{ level: number }>,\n",
            "    set_volume: (self: any, percent: number) -> Ticket<{ ok: boolean, error: string? }>,\n",
            "export type Service = State & Events & {\n",
        ] {
            assert!(types.contains(expected), "missing {expected:?} in\n{types}");
        }
    }

    #[test]
    fn mock_values_satisfy_the_compiled_contract() {
        let contract = crate::contract::parse_compiled_contract(
            "mesh.audio",
            "1.0",
            &serde_json::json!({
                "state": {
                    "percent": { "type": "float", "minimum": 5, "maximum": 100 },
                    "steps": { "type": "int", "minimum": -10, "maximum": -2 },
                    "muted": { "type": "boolean", "default": true },
                    "sinks": "Sink[]",
                    "route": "Route",
                    "art": "blob",
                    "note": "string?"
                },
                "methods": {
                    "current_route": "() -> Route",
                    "set_volume": "(percent: float) -> Result",
                    "cover": "() -> blob"
                },
                "types": {
                    "Sink": { "name": "string" },
                    "Route": { "target": "Sink", "label": "string?", "tags": "string[]" }
                }
            }),
        )
        .unwrap();
        let types = &contract.schemas.types;
        for field in contract.schemas.state_fields.iter() {
            match state_mock_value(field, types) {
                Ok(value) => {
                    assert!(
                        field.type_expr.matches_compiled(&value, types),
                        "{}: {value}",
                        field.name
                    );
                    if let (Some(number), Some(minimum)) =
                        (value.as_f64(), field.minimum.as_ref().and_then(|n| n.as_f64()))
                    {
                        assert!(number >= minimum, "{}: {number}", field.name);
                    }
                    if let (Some(number), Some(maximum)) =
                        (value.as_f64(), field.maximum.as_ref().and_then(|n| n.as_f64()))
                    {
                        assert!(number <= maximum, "{}: {number}", field.name);
                    }
                }
                Err(reason) => assert_eq!(field.name, "art", "{reason}"),
            }
        }
        for method in contract.schemas.methods.iter() {
            let returns = method.returns.as_ref().unwrap();
            match mock_value(returns, types, None, &mut Vec::new()) {
                Ok(value) => assert!(returns.matches_compiled(&value, types), "{}", method.name),
                Err(_) => assert_eq!(method.name, "cover"),
            }
        }

        let mock = generate_luau_mock(&contract);
        for expected in [
            "-- unsupported: state.art: a blob handle cannot be generated\n",
            "-- unsupported: cover(): a blob handle cannot be generated\n",
            "        percent = 5.0,\n",
            "        steps = -2,\n",
            "        muted = true,\n",
            "        route = { [\"tags\"] = {}, [\"target\"] = { [\"name\"] = \"\" } },\n",
            "        sinks = {},\n",
            "    return { [\"ok\"] = true }\n",
            "    error(\"mock cannot produce a cover result: a blob handle cannot be generated\")\n",
        ] {
            assert!(mock.contains(expected), "missing {expected:?} in\n{mock}");
        }
    }

    #[test]
    fn generators_are_deterministic() {
        let contract = contract();
        assert_eq!(
            generate_luau_consumer_types(&contract),
            generate_luau_consumer_types(&contract)
        );
        assert_eq!(
            generate_contract_documentation(&contract),
            generate_contract_documentation(&contract)
        );
    }
}
