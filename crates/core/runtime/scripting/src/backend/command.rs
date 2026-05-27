use super::BackendScriptError;
use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::sync::OnceLock;

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
