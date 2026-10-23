//! Deterministic authoring artifacts generated from compiled service contracts.
//!
//! These generators intentionally return strings instead of writing files. The
//! CLI, LSP, package tooling, and editor integrations can choose their output
//! location without giving the contract compiler filesystem authority.

use crate::contract::{
    BaseType, CompiledContract, CompiledEventSchema, CompiledMethodSchema, CompiledTypeSchema,
    TypeExpr,
};
use std::fmt::Write;

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
/// deterministic default values for every state field.
pub fn generate_luau_mock(contract: &CompiledContract) -> String {
    let mut output = generated_header(contract, "provider mock");
    output.push_str("local Mock = {\n    calls = {},\n    state = {\n");
    for field in contract.schemas.state_fields.iter() {
        writeln!(
            output,
            "        {} = {},",
            field.name,
            luau_default_value(&field.type_expr)
        )
        .unwrap();
    }
    output.push_str("    },\n}\n\n");
    for method in contract.schemas.methods.iter() {
        writeln!(
            output,
            "function Mock.{}({})",
            method.name,
            stub_parameters(method)
        )
        .unwrap();
        writeln!(
            output,
            "    table.insert(self.calls, {{ method = \"{}\" }})",
            method.name
        )
        .unwrap();
        output.push_str("    return { ok = true }\nend\n\n");
    }
    output.push_str("return Mock\n");
    output
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
        BaseType::String => "string".to_string(),
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

fn luau_default_value(expr: &TypeExpr) -> &'static str {
    if expr.optional {
        return "nil";
    }
    match &expr.base {
        BaseType::String => "\"\"",
        BaseType::Int | BaseType::Float => "0",
        BaseType::Boolean => "false",
        BaseType::Object | BaseType::Named(_) => "{}",
        BaseType::Any => "nil",
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
                "state": [
                    { "name": "percent", "type": "float", "minimum": 0, "maximum": 100,
                      "description": "Output volume" },
                    { "name": "muted", "type": "boolean", "default": false },
                    { "name": "extra", "type": "object?" }
                ],
                "methods": [{ "name": "set_volume",
                    "args": [{ "name": "percent", "type": "float" }], "returns": "Result" }],
                "events": [{ "name": "VolumeChanged",
                    "payload": [{ "name": "level", "type": "float" }] }]
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
