/// Backend plugin Luau runtime.
///
/// Backend plugins run inside a real Luau VM via `mlua`. The shell host only
/// injects generic host APIs; all service-specific logic remains in plugin
/// scripts.
use mlua::{Function, Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::process::Command as StdCommand;
use std::sync::{Arc, Mutex};

/// Executes a backend plugin's Luau script.
///
/// Exposes these host APIs to scripts:
/// - `mesh.service.set_poll_interval(ms)` — set polling interval
/// - `mesh.exec("program arg1 arg2 {payload_key}")` — run a system command
/// - `mesh.exec_shell("shell pipeline {payload_key}")` — run a shell command via `sh -lc`
/// - `mesh.service.emit(table)` — emit service state
/// - `mesh.service.emit_json()` — parse `__exec_stdout` as JSON and emit it as state
/// - `mesh.service.emit_unavailable()` — emit unavailable state
/// - `mesh.log.info(msg)` / `mesh.log.warn(msg)`
pub struct BackendScriptContext {
    plugin_id: String,
    lua: Lua,
    runtime: Arc<Mutex<BackendRuntime>>,
}

#[derive(Debug, Default)]
struct BackendRuntime {
    poll_interval_ms: u64,
    pending_emit: Option<JsonValue>,
    current_payload: JsonValue,
}

impl BackendScriptContext {
    pub fn new(plugin_id: impl Into<String>) -> Self {
        let plugin_id = plugin_id.into();
        let lua = Lua::new();
        let runtime = Arc::new(Mutex::new(BackendRuntime {
            poll_interval_ms: 1000,
            pending_emit: None,
            current_payload: JsonValue::Null,
        }));

        let mut ctx = Self {
            plugin_id,
            lua,
            runtime,
        };
        ctx.install_host_api()
            .expect("backend host API setup should succeed");
        ctx
    }

    pub fn poll_interval_ms(&self) -> u64 {
        self.runtime.lock().unwrap().poll_interval_ms
    }

    /// Load and execute a backend Luau script.
    pub fn load_script(&mut self, source: &str) -> Result<(), BackendScriptError> {
        self.lua
            .load(source)
            .set_name(&self.plugin_id)
            .exec()
            .map_err(|err| BackendScriptError::Runtime {
                plugin_id: self.plugin_id.clone(),
                message: err.to_string(),
            })?;
        tracing::info!("loaded backend script for {}", self.plugin_id);
        Ok(())
    }

    /// Call `on_poll()` if it exists. Returns any emitted payload.
    pub fn run_poll(&mut self) -> Option<JsonValue> {
        self.reset_for_call(JsonValue::Null);
        let globals = self.lua.globals();
        let handler = globals.get::<Function>("on_poll").ok()?;
        if let Err(err) = handler.call::<()>(()) {
            tracing::warn!("{} on_poll error: {err}", self.plugin_id);
        }
        self.take_pending_emit()
    }

    /// Call `on_command_<name>()` for the given command. Returns any emitted payload.
    pub fn run_command(&mut self, command: &str, payload: &JsonValue) -> Option<JsonValue> {
        self.reset_for_call(payload.clone());
        self.install_payload_globals(payload);

        let fn_name = format!("on_command_{}", command.replace('-', "_"));
        let globals = self.lua.globals();
        let handler = globals.get::<Function>(fn_name.as_str()).ok()?;
        if let Err(err) = handler.call::<()>(()) {
            tracing::warn!("{} {fn_name} error: {err}", self.plugin_id);
        }
        self.take_pending_emit()
    }

    fn install_host_api(&mut self) -> mlua::Result<()> {
        let globals = self.lua.globals();
        let mesh = self.lua.create_table()?;
        let service = self.lua.create_table()?;
        let log = self.lua.create_table()?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "set_poll_interval",
            self.lua.create_function(move |_lua, ms: u64| {
                runtime.lock().unwrap().poll_interval_ms = ms;
                Ok(())
            })?,
        )?;

        let plugin_id = self.plugin_id.clone();
        let runtime = Arc::clone(&self.runtime);
        service.set(
            "emit",
            self.lua.create_function(move |lua, value: LuaValue| {
                let payload = lua.from_value::<JsonValue>(value)?;
                runtime.lock().unwrap().pending_emit = Some(payload);
                Ok(())
            })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "emit_json",
            self.lua.create_function(move |lua, ()| {
                let stdout = lua
                    .globals()
                    .get::<String>("__exec_stdout")
                    .unwrap_or_default();
                if let Ok(payload) = serde_json::from_str::<JsonValue>(stdout.trim()) {
                    runtime.lock().unwrap().pending_emit = Some(payload);
                }
                Ok(())
            })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "emit_unavailable",
            self.lua.create_function(move |_lua, ()| {
                runtime.lock().unwrap().pending_emit = Some(serde_json::json!({
                    "available": false,
                    "source_plugin": plugin_id,
                }));
                Ok(())
            })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        mesh.set(
            "exec",
            self.lua.create_function(move |lua, cmd: String| {
                run_command(&lua, &runtime, &cmd, false)
            })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        mesh.set(
            "exec_shell",
            self.lua
                .create_function(move |lua, cmd: String| run_command(&lua, &runtime, &cmd, true))?,
        )?;

        let plugin_id = self.plugin_id.clone();
        log.set(
            "info",
            self.lua.create_function(move |_lua, message: String| {
                tracing::info!("{}: {}", plugin_id, message);
                Ok(())
            })?,
        )?;

        let plugin_id = self.plugin_id.clone();
        log.set(
            "warn",
            self.lua.create_function(move |_lua, message: String| {
                tracing::warn!("{}: {}", plugin_id, message);
                Ok(())
            })?,
        )?;

        mesh.set("service", service)?;
        mesh.set("log", log)?;
        globals.set("mesh", mesh)?;
        globals.set("__exec_success", false)?;
        globals.set("__exec_stdout", "")?;
        Ok(())
    }

    fn reset_for_call(&mut self, payload: JsonValue) {
        let mut runtime = self.runtime.lock().unwrap();
        runtime.pending_emit = None;
        runtime.current_payload = payload;
    }

    fn install_payload_globals(&self, payload: &JsonValue) {
        let globals = self.lua.globals();
        if let Some(obj) = payload.as_object() {
            for (key, value) in obj {
                let global_name = format!("__payload_{key}");
                match self.lua.to_value(value) {
                    Ok(lua_value) => {
                        let _ = globals.set(global_name, lua_value);
                    }
                    Err(err) => {
                        tracing::debug!(
                            "{} failed to install payload global {}: {}",
                            self.plugin_id,
                            key,
                            err
                        );
                    }
                }
            }
        }
    }

    fn take_pending_emit(&self) -> Option<JsonValue> {
        self.runtime.lock().unwrap().pending_emit.take()
    }
}

fn run_command(
    lua: &Lua,
    runtime: &Arc<Mutex<BackendRuntime>>,
    raw_cmd: &str,
    shell: bool,
) -> mlua::Result<()> {
    let cmd = substitute_payload_vars(raw_cmd, &runtime.lock().unwrap().current_payload);
    let result = if shell {
        StdCommand::new("sh").arg("-lc").arg(&cmd).output()
    } else {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if let Some((prog, rest)) = parts.split_first() {
            StdCommand::new(prog).args(rest).output()
        } else {
            lua.globals().set("__exec_success", false)?;
            lua.globals().set("__exec_stdout", "")?;
            return Ok(());
        }
    };

    match result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            lua.globals().set("__exec_success", out.status.success())?;
            lua.globals().set("__exec_stdout", stdout)?;
        }
        Err(err) => {
            tracing::debug!("backend exec failed: {}", err);
            lua.globals().set("__exec_success", false)?;
            lua.globals().set("__exec_stdout", "")?;
        }
    }

    Ok(())
}

fn substitute_payload_vars(s: &str, payload: &JsonValue) -> String {
    let mut result = s.to_string();
    if let Some(obj) = payload.as_object() {
        for (key, value) in obj {
            let token = format!("{{{key}}}");
            let replacement = match value {
                JsonValue::String(v) => v.clone(),
                JsonValue::Number(v) => v.to_string(),
                JsonValue::Bool(v) => v.to_string(),
                _ => continue,
            };
            result = result.replace(&token, &replacement);
        }
    }
    result
}

#[derive(Debug, thiserror::Error)]
pub enum BackendScriptError {
    #[error("script error in {plugin_id}: {message}")]
    Runtime { plugin_id: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_poll_interval_from_script() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("mesh.service.set_poll_interval(250)")
            .unwrap();
        assert_eq!(ctx.poll_interval_ms(), 250);
    }

    #[test]
    fn registers_handlers_from_script() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function on_poll()\nmesh.log.info(\"polling\")\nend\n\
             function on_command_volume_up()\nmesh.log.info(\"up\")\nend",
        )
        .unwrap();
        assert!(ctx.lua.globals().get::<Function>("on_poll").is_ok());
        assert!(
            ctx.lua
                .globals()
                .get::<Function>("on_command_volume_up")
                .is_ok()
        );
    }

    #[test]
    fn emit_stores_pending_payload() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function on_poll()\nmesh.service.emit({ available = true, percent = 65 })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap();
        assert_eq!(
            payload.get("available").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(65));
    }

    #[test]
    fn emit_unavailable_stores_unavailable_payload() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function on_poll()\nmesh.service.emit_unavailable()\nend")
            .unwrap();
        let payload = ctx.run_poll().unwrap();
        assert_eq!(
            payload.get("available").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            payload.get("source_plugin").and_then(|v| v.as_str()),
            Some("@test/backend")
        );
    }

    #[test]
    fn command_handler_substitutes_payload_vars() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function on_command_set_volume()\nmesh.exec_shell(\"printf '{\\\"percent\\\":{percent}}'\")\nmesh.service.emit_json()\nend",
        )
        .unwrap();
        let result = ctx.run_command("set-volume", &serde_json::json!({ "percent": 50 }));
        let payload = result.unwrap();
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(50));
    }

    #[test]
    fn exec_shell_captures_stdout() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function on_poll()\nmesh.exec_shell(\"printf '{\\\"ok\\\":true}'\")\nend")
            .unwrap();
        ctx.run_poll();
        assert_eq!(
            ctx.lua.globals().get::<bool>("__exec_success").ok(),
            Some(true)
        );
        assert_eq!(
            ctx.lua.globals().get::<String>("__exec_stdout").ok(),
            Some("{\"ok\":true}".to_string())
        );
    }

    #[test]
    fn emit_json_uses_exec_stdout() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function on_poll()\nmesh.exec_shell(\"printf '{\\\"available\\\":true,\\\"percent\\\":65}'\")\nmesh.service.emit_json()\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap();
        assert_eq!(
            payload.get("available").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(65));
    }

    #[test]
    fn emit_resolves_lua_table_payloads() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function on_poll()\nmesh.service.emit({ percent = 42, muted = false })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap();
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(payload.get("muted").and_then(|v| v.as_bool()), Some(false));
    }
}
