use super::BackendScriptError;
use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::Value as JsonValue;

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
        return Ok(serde_json::json!({ "ok": true }));
    }

    lua.from_value::<JsonValue>(value).map_err(|err| {
        BackendScriptError::CommandResultConversionFailed {
            module_id: module_id.to_string(),
            message: err.to_string(),
        }
    })
}

pub(super) fn command_error_result(message: impl Into<String>) -> JsonValue {
    serde_json::json!({
        "ok": false,
        "error": message.into(),
    })
}
