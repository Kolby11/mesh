use super::command::{BackendCommandOutcome, command_error_result, command_result_from_lua};
use super::exec::{exec_denied_to_lua, missing_exec_capability, run_exec};
use super::logging::log_message;
use super::{BackendScriptError, MIN_POLL_INTERVAL_MS};
use mlua::{Function, Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Executes a backend module's Luau script.
///
/// Exposes these host APIs to scripts:
/// - `init()` — required backend entrypoint called once after script load
/// - `mesh.service.set_poll_interval(ms)` — set polling interval
/// - `mesh.exec("program", {"arg1", "arg2"})` — run a system command
/// - `mesh.config()` — return the full module settings Lua table
/// - `mesh.service.emit(table)` — emit service state
/// - `mesh.service.emit_json(value?)` — parse JSON text or emit a Lua table directly
/// - `mesh.service.emit_unavailable()` — emit unavailable state
/// - `mesh.service.payload()` — get the current command payload as a Lua table
/// - `mesh.service.has_capability(name)` — check whether the module was granted a capability
/// - `mesh.log(level, msg)` / `mesh.log.debug(msg)` / `mesh.log.info(msg)` / `mesh.log.warn(msg)` / `mesh.log.error(msg)`
pub struct BackendScriptContext {
    module_id: String,
    capabilities: HashSet<String>,
    pub(super) lua: Lua,
    runtime: Arc<Mutex<BackendRuntime>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackendScriptEvent {
    pub name: String,
    pub payload: JsonValue,
}

#[derive(Debug, Default)]
struct BackendRuntime {
    poll_interval_ms: u64,
    pending_emit: Option<JsonValue>,
    pending_events: Vec<BackendScriptEvent>,
    current_payload: JsonValue,
    settings: JsonValue,
}

impl BackendScriptContext {
    pub fn new(module_id: impl Into<String>) -> Self {
        Self::new_with_settings_and_capabilities(
            module_id,
            serde_json::json!({}),
            Vec::<String>::new(),
        )
    }

    pub fn new_with_settings(module_id: impl Into<String>, settings: JsonValue) -> Self {
        Self::new_with_settings_and_capabilities(module_id, settings, Vec::<String>::new())
    }

    pub fn new_with_capabilities(
        module_id: impl Into<String>,
        capabilities: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::new_with_settings_and_capabilities(module_id, serde_json::json!({}), capabilities)
    }

    pub fn new_with_settings_and_capabilities(
        module_id: impl Into<String>,
        settings: JsonValue,
        capabilities: impl IntoIterator<Item = String>,
    ) -> Self {
        let module_id = module_id.into();
        let lua = Lua::new();
        let runtime = Arc::new(Mutex::new(BackendRuntime {
            poll_interval_ms: 1000,
            pending_emit: None,
            pending_events: Vec::new(),
            current_payload: JsonValue::Null,
            settings,
        }));

        let mut ctx = Self {
            module_id,
            capabilities: capabilities.into_iter().collect(),
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
            .set_name(&self.module_id)
            .exec()
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        tracing::info!("loaded backend script for {}", self.module_id);
        Ok(())
    }

    /// Call the backend script's required `init()` entrypoint once after load.
    pub fn call_init(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        self.reset_for_call(JsonValue::Null);
        let globals = self.lua.globals();
        let init =
            globals
                .get::<Function>("init")
                .map_err(|_| BackendScriptError::MissingEntrypoint {
                    module_id: self.module_id.clone(),
                    name: "init".to_string(),
                })?;
        init.call::<()>(())
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        self.take_service_state_snapshot()
    }

    /// Call `on_poll()` if it exists. Returns any exported service state.
    pub fn run_poll(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        self.reset_for_call(JsonValue::Null);
        let globals = self.lua.globals();
        let handler = match globals.get::<Function>("on_poll") {
            Ok(handler) => handler,
            Err(_) => return Ok(None),
        };
        handler
            .call::<()>(())
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        self.take_service_state_snapshot()
    }

    /// Call `on_command_<name>()` for the given command. Returns any exported service state.
    pub fn run_command(
        &mut self,
        command: &str,
        payload: &JsonValue,
    ) -> Result<Option<JsonValue>, BackendScriptError> {
        self.reset_for_call(payload.clone());
        let normalized = command.replace('-', "_");

        let globals = self.lua.globals();
        let handler_name = format!("on_command_{normalized}");
        let handler = match globals
            .get::<Function>(handler_name.as_str())
            .or_else(|_| globals.get::<Function>(normalized.as_str()))
        {
            Ok(handler) => handler,
            Err(_) => return Ok(None),
        };
        handler
            .call::<()>(())
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        self.take_service_state_snapshot()
    }

    pub fn run_command_with_result(
        &mut self,
        command: &str,
        payload: &JsonValue,
    ) -> Result<BackendCommandOutcome, BackendScriptError> {
        self.reset_for_call(payload.clone());
        let normalized = command.replace('-', "_");

        let globals = self.lua.globals();
        let handler_name = format!("on_command_{normalized}");
        let handler = match globals
            .get::<Function>(handler_name.as_str())
            .or_else(|_| globals.get::<Function>(normalized.as_str()))
        {
            Ok(handler) => handler,
            Err(_) => {
                return Ok(BackendCommandOutcome {
                    state: None,
                    result: command_error_result(format!("unsupported command: {command}")),
                    error: None,
                });
            }
        };

        let returned = match handler.call::<LuaValue>(()) {
            Ok(returned) => returned,
            Err(err) => {
                let message = err.to_string();
                return Ok(BackendCommandOutcome {
                    state: None,
                    result: command_error_result(message.clone()),
                    error: Some(message),
                });
            }
        };

        Ok(BackendCommandOutcome {
            state: self.take_service_state_snapshot()?,
            result: command_result_from_lua(&self.lua, &self.module_id, returned)?,
            error: None,
        })
    }

    pub fn take_service_state_snapshot(&self) -> Result<Option<JsonValue>, BackendScriptError> {
        if let Some(payload) = self.take_pending_emit() {
            return Ok(Some(payload));
        }

        let globals = self.lua.globals();
        let state =
            globals
                .get::<LuaValue>("state")
                .map_err(|err| BackendScriptError::SnapshotFailed {
                    module_id: self.module_id.clone(),
                    message: format!("failed to read state global: {err}"),
                })?;

        if matches!(state, LuaValue::Nil) {
            return Ok(None);
        }

        self.lua
            .from_value::<JsonValue>(state)
            .map(Some)
            .map_err(|err| BackendScriptError::SnapshotFailed {
                module_id: self.module_id.clone(),
                message: format!("failed to convert state to JSON: {err}"),
            })
    }

    pub fn drain_events(&self) -> Vec<BackendScriptEvent> {
        std::mem::take(&mut self.runtime.lock().unwrap().pending_events)
    }

    fn install_host_api(&mut self) -> mlua::Result<()> {
        let globals = self.lua.globals();
        let mesh = self.lua.create_table()?;
        let service = self.lua.create_table()?;
        let log = self.lua.create_table()?;

        let module_id = self.module_id.clone();
        let runtime = Arc::clone(&self.runtime);
        service.set(
            "set_poll_interval",
            self.lua.create_function(move |_lua, ms: u64| {
                let poll_interval_ms = ms.max(MIN_POLL_INTERVAL_MS);
                if poll_interval_ms != ms {
                    tracing::warn!(
                        module_id = module_id,
                        requested_interval_ms = ms,
                        clamped_interval_ms = poll_interval_ms,
                        "backend poll interval below minimum; clamping"
                    );
                }
                runtime.lock().unwrap().poll_interval_ms = poll_interval_ms;
                Ok(())
            })?,
        )?;

        let module_id = self.module_id.clone();
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
            self.lua
                .create_function(move |lua, value: Option<LuaValue>| {
                    let payload = match value {
                        None | Some(LuaValue::Nil) => {
                            runtime.lock().unwrap().current_payload.clone()
                        }
                        Some(LuaValue::String(text)) => {
                            serde_json::from_str::<JsonValue>(text.to_str()?.trim())
                                .map_err(mlua::Error::external)?
                        }
                        Some(other) => lua.from_value::<JsonValue>(other)?,
                    };
                    runtime.lock().unwrap().pending_emit = Some(payload);
                    Ok(())
                })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "emit_unavailable",
            self.lua.create_function(move |_lua, ()| {
                runtime.lock().unwrap().pending_emit = Some(serde_json::json!({
                    "available": false,
                    "source_module": module_id,
                }));
                Ok(())
            })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "emit_event",
            self.lua
                .create_function(move |lua, (name, payload): (String, Option<LuaValue>)| {
                    let payload = match payload {
                        Some(value) => lua.from_value::<JsonValue>(value)?,
                        None => JsonValue::Null,
                    };
                    runtime
                        .lock()
                        .unwrap()
                        .pending_events
                        .push(BackendScriptEvent { name, payload });
                    Ok(())
                })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "payload",
            self.lua.create_function(move |lua, ()| {
                let payload = runtime.lock().unwrap().current_payload.clone();
                lua.to_value(&payload)
            })?,
        )?;

        let capabilities = self.capabilities.clone();
        service.set(
            "has_capability",
            self.lua.create_function(move |_lua, capability: String| {
                Ok(capabilities.contains(capability.as_str()))
            })?,
        )?;

        let capabilities = self.capabilities.clone();
        let module_id = self.module_id.clone();
        mesh.set(
            "exec",
            self.lua
                .create_function(move |lua, (program, args): (String, Vec<String>)| {
                    if let Some(required) = missing_exec_capability(&capabilities, &program) {
                        tracing::warn!(
                            module_id = %module_id,
                            program = %program,
                            required_capability = %required,
                            "denied backend exec"
                        );
                        return exec_denied_to_lua(lua, &program, &required);
                    }

                    run_exec(lua, &program, &args)
                })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        mesh.set(
            "config",
            self.lua.create_function(move |lua, ()| {
                let settings = runtime.lock().unwrap().settings.clone();
                lua.to_value(&settings)
            })?,
        )?;

        let module_id = self.module_id.clone();
        let call_log = self.lua.create_function(
            move |_lua, (_self, level, message): (mlua::Table, String, String)| {
                log_message(&module_id, &level, &message);
                Ok(())
            },
        )?;
        let log_meta = self.lua.create_table()?;
        log_meta.set("__call", call_log)?;
        log.set_metatable(Some(log_meta))?;

        let module_id = self.module_id.clone();
        for (name, level) in [
            ("info", "info"),
            ("warn", "warn"),
            ("warning", "warning"),
            ("error", "error"),
            ("debug", "debug"),
        ] {
            let module_id = module_id.clone();
            log.set(
                name,
                self.lua.create_function(move |_lua, message: String| {
                    log_message(&module_id, level, &message);
                    Ok(())
                })?,
            )?;
        }

        mesh.set("service", service)?;
        mesh.set("log", log)?;
        globals.set("mesh", mesh)?;
        Ok(())
    }

    fn reset_for_call(&mut self, payload: JsonValue) {
        let mut runtime = self.runtime.lock().unwrap();
        runtime.pending_emit = None;
        runtime.pending_events.clear();
        runtime.current_payload = payload;
    }

    fn take_pending_emit(&self) -> Option<JsonValue> {
        self.runtime.lock().unwrap().pending_emit.take()
    }
}
