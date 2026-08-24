use super::BackendScriptError;
use mesh_core_service::{InterfaceContract, InterfaceTypeDef, TypeExpr};
use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The immutable command contract installed by the shell before a backend
/// script is loaded. Runtime dispatch must consult this registry rather than
/// making every top-level Lua function an implicit public command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCommandSpec {
    pub name: String,
    pub arguments: Vec<BackendCommandArgument>,
    pub returns: Option<String>,
    pub coalesce: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCommandArgument {
    pub name: String,
    pub type_expr: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackendCommandRegistry {
    commands: HashMap<String, BackendCommandSpec>,
    types: HashMap<String, InterfaceTypeDef>,
}

impl BackendCommandRegistry {
    pub fn from_specs(
        commands: impl IntoIterator<Item = BackendCommandSpec>,
        types: HashMap<String, InterfaceTypeDef>,
    ) -> Self {
        Self {
            commands: commands
                .into_iter()
                .map(|command| (command.name.clone(), command))
                .collect(),
            types,
        }
    }

    pub fn from_contract(contract: &InterfaceContract) -> Self {
        let commands: HashMap<String, BackendCommandSpec> = contract
            .methods
            .iter()
            .map(|method| {
                (
                    method.name.clone(),
                    BackendCommandSpec {
                        name: method.name.clone(),
                        arguments: method
                            .args
                            .iter()
                            .map(|argument| BackendCommandArgument {
                                name: argument.name.clone(),
                                type_expr: argument.arg_type.clone(),
                            })
                            .collect(),
                        returns: method.returns.clone(),
                        coalesce: method.coalesce,
                    },
                )
            })
            .collect();
        Self::from_specs(commands.into_values(), contract.types.clone())
    }

    pub fn command(&self, name: &str) -> Option<&BackendCommandSpec> {
        self.commands.get(name)
    }

    pub fn coalesces(&self, name: &str) -> bool {
        self.command(name).is_some_and(|command| command.coalesce)
    }

    pub fn validate_payload(&self, command: &str, payload: &JsonValue) -> Result<(), String> {
        let Some(spec) = self.command(command) else {
            return Err(format!("unsupported command: {command}"));
        };
        let Some(object) = payload.as_object() else {
            return Err(format!("command '{command}' payload must be a JSON object"));
        };
        for argument in &spec.arguments {
            let value_type = TypeExpr::parse(&argument.type_expr).map_err(|error| {
                format!("invalid argument type for '{}': {error}", argument.name)
            })?;
            let Some(value) = object.get(&argument.name) else {
                if !argument.type_expr.trim().ends_with('?') {
                    return Err(format!("missing required argument '{}'", argument.name));
                }
                continue;
            };
            if !value_type.matches_with_types(value, &self.types) {
                return Err(format!(
                    "argument '{}' expected {}, got {}",
                    argument.name,
                    argument.type_expr,
                    json_type_name(value)
                ));
            }
        }
        if let Some(unknown) = object.keys().find(|name| {
            !spec
                .arguments
                .iter()
                .any(|argument| argument.name == **name)
        }) {
            return Err(format!("unknown argument '{unknown}'"));
        }
        Ok(())
    }

    pub fn validate_result(&self, command: &str, result: &JsonValue) -> Result<(), String> {
        let Some(returns) = self
            .command(command)
            .and_then(|spec| spec.returns.as_deref())
        else {
            return Ok(());
        };
        let value_type = TypeExpr::parse(returns)
            .map_err(|error| format!("invalid result type for '{command}': {error}"))?;
        if value_type.matches_with_types(result, &self.types) {
            Ok(())
        } else {
            Err(format!(
                "result expected {returns}, got {}",
                json_type_name(result)
            ))
        }
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

fn ok_command_result() -> JsonValue {
    static OK_RESULT: OnceLock<JsonValue> = OnceLock::new();
    OK_RESULT
        .get_or_init(|| serde_json::json!({ "ok": true }))
        .clone()
}

#[derive(Debug, Clone)]
pub struct BackendCommandOutcome {
    pub state: Option<JsonValue>,
    pub result: JsonValue,
    pub error: Option<String>,
}

pub(super) fn command_result_from_lua(
    lua: &Lua,
    module_id: &str,
    value: LuaValue,
) -> Result<JsonValue, BackendScriptError> {
    if matches!(value, LuaValue::Nil) {
        return Ok(ok_command_result());
    }

    lua.from_value::<JsonValue>(value).map_err(|err| {
        BackendScriptError::CommandResultConversionFailed {
            module_id: module_id.to_string(),
            message: err.to_string(),
        }
    })
}

pub(super) fn command_error_result(message: impl Into<String>) -> JsonValue {
    let mut result = serde_json::Map::with_capacity(2);
    result.insert("ok".to_string(), JsonValue::Bool(false));
    result.insert("error".to_string(), JsonValue::String(message.into()));
    JsonValue::Object(result)
}
