use super::command::{BackendCommandOutcome, command_error_result, command_result_from_lua};
use super::exec::{exec_denied_to_lua, missing_exec_capability, run_exec};
use super::exec_stream::{StreamState, spawn_stream};
use super::logging::log_message;
use super::{BackendScriptError, MIN_POLL_INTERVAL_MS};
use crate::storage::{ScopedStorage, StorageManager, StorageScope, create_lua_storage_table};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use crate::util::{default_runtime_storage_root, is_named_event_channel};
use std::path::PathBuf;
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
    pub(super) lua: Option<Lua>,
    runtime: Arc<Mutex<BackendRuntime>>,
    builtin_globals: HashSet<String>,
    storage: Arc<Mutex<ScopedStorage>>,
    streams: Arc<StreamState>,
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
    storage_diagnostics: Vec<String>,
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
        Self::new_with_settings_capabilities_and_storage_root(
            module_id,
            settings,
            capabilities,
            default_runtime_storage_root(),
        )
    }

    pub fn new_with_storage_root(
        module_id: impl Into<String>,
        storage_root: impl Into<PathBuf>,
    ) -> Self {
        Self::new_with_settings_capabilities_and_storage_root(
            module_id,
            serde_json::json!({}),
            Vec::<String>::new(),
            storage_root,
        )
    }

    pub fn new_with_settings_capabilities_and_storage_root(
        module_id: impl Into<String>,
        settings: JsonValue,
        capabilities: impl IntoIterator<Item = String>,
        storage_root: impl Into<PathBuf>,
    ) -> Self {
        let module_id = module_id.into();
        let storage = StorageManager::new(storage_root.into()).open(StorageScope::backend(
            module_id.clone(),
            module_id.clone(),
            module_id.clone(),
        ));
        let storage_diagnostics = storage
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.reason.clone())
            .collect();
        let runtime = Arc::new(Mutex::new(BackendRuntime {
            poll_interval_ms: 1000,
            pending_emit: None,
            pending_events: Vec::new(),
            current_payload: JsonValue::Null,
            settings,
            storage_diagnostics,
        }));

        Self {
            module_id,
            capabilities: capabilities.into_iter().collect(),
            lua: None,
            runtime,
            builtin_globals: HashSet::new(),
            storage: Arc::new(Mutex::new(storage)),
            streams: StreamState::new(),
        }
    }

    pub(super) fn ensure_lua(&mut self) -> &Lua {
        if let Some(ref lua) = self.lua {
            return lua;
        }
        let lua = Lua::new();
        self.lua = Some(lua);
        let globals = self.lua.as_ref().unwrap().globals();
        self.install_host_api(&globals)
            .expect("backend host API setup should succeed");
        self.builtin_globals = self
            .lua
            .as_ref()
            .unwrap()
            .globals()
            .pairs::<String, LuaValue>()
            .filter_map(|result| result.ok().map(|(key, _)| key))
            .collect();
        self.lua.as_ref().unwrap()
    }

    pub fn poll_interval_ms(&self) -> u64 {
        self.runtime.lock().unwrap().poll_interval_ms
    }

    /// Load and execute a backend Luau script.
    pub fn load_script(&mut self, source: &str) -> Result<(), BackendScriptError> {
        self.ensure_lua()
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

    /// Call the backend script's startup entrypoint once after load.
    ///
    /// Canonical v1.14 scripts use `start(self)`. Legacy `init()` remains a
    /// compatibility fallback and receives the same current-provider context.
    pub fn call_init(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        self.reset_for_call(JsonValue::Null);
        let globals = self.ensure_lua().globals();
        let entrypoint_name = if globals.get::<Function>("start").is_ok() {
            "start"
        } else {
            "init"
        };
        let entrypoint = globals.get::<Function>(entrypoint_name).map_err(|_| {
            BackendScriptError::MissingEntrypoint {
                module_id: self.module_id.clone(),
                name: "start".to_string(),
            }
        })?;
        let current_self =
            self.current_self_table()
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
        entrypoint
            .call::<()>(current_self)
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        self.take_service_state_snapshot()
    }

    /// Call the backend script's optional `stop(self)` lifecycle hook.
    pub fn call_stop(&mut self) -> Result<(), BackendScriptError> {
        self.kill_streams();
        self.reset_for_call(JsonValue::Null);
        let globals = self.ensure_lua().globals();
        let stop = match globals.get::<Function>("stop") {
            Ok(stop) => stop,
            Err(_) => {
                self.flush_storage();
                return Ok(());
            }
        };
        let current_self =
            self.current_self_table()
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
        stop.call::<()>(current_self)
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        self.flush_storage();
        Ok(())
    }

    /// Shared subprocess-stream state. The backend service loop awaits on
    /// `stream_state().wait_for_event()` to react to lines from
    /// `mesh.exec_stream` subprocesses.
    pub fn stream_state(&self) -> Arc<StreamState> {
        Arc::clone(&self.streams)
    }

    /// Dispatch one wakeup's worth of subprocess lines to the script.
    ///
    /// If the script defines `on_stream_batch(self, program, lines)`, it is
    /// called once with the full ordered batch — scripts that only need a
    /// "something changed" signal can ignore `lines` entirely. Otherwise the
    /// legacy `on_stream_line(self, program, line)` hook is called once per
    /// line, preserving the documented per-line semantics. Returns the state
    /// snapshot taken after the whole batch is processed.
    pub fn run_stream_batch(
        &mut self,
        program: &str,
        lines: &[String],
    ) -> Result<Option<JsonValue>, BackendScriptError> {
        if lines.is_empty() {
            return Ok(None);
        }
        self.reset_for_call(JsonValue::Null);
        let globals = self.ensure_lua().globals();
        if let Ok(batch_handler) = globals.get::<Function>("on_stream_batch") {
            let current_self =
                self.current_self_table()
                    .map_err(|err| BackendScriptError::Runtime {
                        module_id: self.module_id.clone(),
                        message: err.to_string(),
                    })?;
            let lines_table = self
                .ensure_lua()
                .create_sequence_from(lines.iter().cloned())
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
            batch_handler
                .call::<()>((current_self, program.to_string(), lines_table))
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
            return self.take_service_state_snapshot();
        }
        let line_handler = match globals.get::<Function>("on_stream_line") {
            Ok(handler) => handler,
            Err(_) => return Ok(None),
        };
        for line in lines {
            let current_self =
                self.current_self_table()
                    .map_err(|err| BackendScriptError::Runtime {
                        module_id: self.module_id.clone(),
                        message: err.to_string(),
                    })?;
            line_handler
                .call::<()>((current_self, program.to_string(), line.clone()))
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
        }
        self.take_service_state_snapshot()
    }

    /// Kill every active `mesh.exec_stream` subprocess. Idempotent; safe to
    /// call from `Drop` and from `stop(self)` lifecycle.
    pub fn kill_streams(&self) {
        self.streams.kill_all();
    }

    /// Call `on_poll()` if it exists. Returns any exported service state.
    pub fn run_poll(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        self.reset_for_call(JsonValue::Null);
        let globals = self.ensure_lua().globals();
        let handler = match globals.get::<Function>("on_poll") {
            Ok(handler) => handler,
            Err(_) => return Ok(None),
        };
        let current_self =
            self.current_self_table()
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
        handler
            .call::<()>(current_self)
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

        let globals = self.ensure_lua().globals();
        let handler_name = format!("on_command_{normalized}");
        let handler = match globals
            .get::<Function>(handler_name.as_str())
            .or_else(|_| globals.get::<Function>(normalized.as_str()))
        {
            Ok(handler) => handler,
            Err(_) => return Ok(None),
        };
        let current_self =
            self.current_self_table()
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
        handler
            .call::<()>(current_self)
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

        let globals = self.ensure_lua().globals();
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

        let current_self =
            self.current_self_table()
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
        let returned = match handler.call::<LuaValue>(current_self) {
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

        let state = self.take_service_state_snapshot()?;
        let module_id = self.module_id.clone();
        let lua = self.ensure_lua();
        let result = command_result_from_lua(lua, &module_id, returned)?;
        Ok(BackendCommandOutcome {
            state,
            result,
            error: None,
        })
    }

    pub fn take_service_state_snapshot(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        if let Some(payload) = self.take_pending_emit() {
            return Ok(Some(payload));
        }

        let globals = self.ensure_lua().globals();
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

        self.ensure_lua()
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

    pub fn drain_storage_diagnostics(&self) -> Vec<String> {
        std::mem::take(&mut self.runtime.lock().unwrap().storage_diagnostics)
    }

    pub fn flush_storage(&self) {
        let result = self.storage.lock().unwrap().flush_if_dirty();
        if let Err(error) = result {
            self.runtime
                .lock()
                .unwrap()
                .storage_diagnostics
                .push(format!("storage persistence failed: {error}"));
        }
    }

    pub fn public_function_names(&mut self) -> Vec<String> {
        let globals = self.ensure_lua().globals();
        let mut names = globals
            .pairs::<String, LuaValue>()
            .filter_map(|pair| {
                let (name, value) = pair.ok()?;
                if self.builtin_globals.contains(&name)
                    || is_reserved_backend_hook(&name)
                    || !matches!(value, LuaValue::Function(_))
                {
                    return None;
                }
                Some(name)
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn install_host_api(&mut self, target: &mlua::Table) -> mlua::Result<()> {
        let globals = target;
        globals.set("self", self.current_self_table()?)?;
        let mesh = self.ensure_lua().create_table()?;
        let service = self.ensure_lua().create_table()?;
        let log = self.ensure_lua().create_table()?;

        let module_id = self.module_id.clone();
        let runtime = Arc::clone(&self.runtime);
        service.set(
            "set_poll_interval",
            self.ensure_lua().create_function(move |_lua, ms: u64| {
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
            self.ensure_lua()
                .create_function(move |lua, value: LuaValue| {
                    let payload = lua.from_value::<JsonValue>(value)?;
                    runtime.lock().unwrap().pending_emit = Some(payload);
                    Ok(())
                })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "emit_json",
            self.ensure_lua()
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
            self.ensure_lua().create_function(move |_lua, ()| {
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
            self.ensure_lua().create_function(
                move |lua, (name, payload): (String, Option<LuaValue>)| {
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
                },
            )?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "payload",
            self.ensure_lua().create_function(move |lua, ()| {
                let payload = runtime.lock().unwrap().current_payload.clone();
                lua.to_value(&payload)
            })?,
        )?;

        let capabilities = self.capabilities.clone();
        service.set(
            "has_capability",
            self.ensure_lua()
                .create_function(move |_lua, capability: String| {
                    Ok(capabilities.contains(capability.as_str()))
                })?,
        )?;

        let capabilities = self.capabilities.clone();
        let module_id = self.module_id.clone();
        mesh.set(
            "exec",
            self.ensure_lua().create_function(
                move |lua, (program, args): (String, Vec<String>)| {
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
                },
            )?,
        )?;

        let capabilities = self.capabilities.clone();
        let module_id = self.module_id.clone();
        let streams = Arc::clone(&self.streams);
        mesh.set(
            "exec_stream",
            self.ensure_lua().create_function(
                move |_lua, (program, args): (String, Vec<String>)| {
                    if let Some(required) = missing_exec_capability(&capabilities, &program) {
                        tracing::warn!(
                            module_id = %module_id,
                            program = %program,
                            required_capability = %required,
                            "denied backend exec_stream"
                        );
                        return Ok(false);
                    }
                    match spawn_stream(&streams, program.clone(), args) {
                        Ok(()) => Ok(true),
                        Err(err) => {
                            tracing::warn!(
                                module_id = %module_id,
                                program = %program,
                                "exec_stream failed to spawn: {err}"
                            );
                            Ok(false)
                        }
                    }
                },
            )?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        mesh.set(
            "config",
            self.ensure_lua().create_function(move |lua, ()| {
                let settings = runtime.lock().unwrap().settings.clone();
                lua.to_value(&settings)
            })?,
        )?;

        let module_id = self.module_id.clone();
        let call_log = self.ensure_lua().create_function(
            move |_lua, (_self, level, message): (mlua::Table, String, String)| {
                log_message(&module_id, &level, &message);
                Ok(())
            },
        )?;
        let log_meta = self.ensure_lua().create_table()?;
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
                self.ensure_lua()
                    .create_function(move |_lua, message: String| {
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

    fn current_self_table(&mut self) -> mlua::Result<mlua::Table> {
        let current_self = self.ensure_lua().create_table()?;
        let meta = self.ensure_lua().create_table()?;
        meta.set("module_id", self.module_id.as_str())?;
        meta.set("provider_id", self.module_id.as_str())?;
        meta.set("kind", "backend")?;
        meta.set("instance_id", self.module_id.as_str())?;
        meta.set("diagnostics_id", self.module_id.as_str())?;
        current_self.set("meta", meta)?;
        let runtime_for_storage_diagnostics = Arc::clone(&self.runtime);
        let storage_arc = Arc::clone(&self.storage);
        let storage = create_lua_storage_table(
            self.ensure_lua(),
            storage_arc,
            Arc::new(move |reason| {
                runtime_for_storage_diagnostics
                    .lock()
                    .unwrap()
                    .storage_diagnostics
                    .push(reason);
            }),
            Arc::new(|_key| {}),
            Arc::new(|_key| {}),
        )?;
        current_self.set("storage", storage)?;
        let runtime = Arc::clone(&self.runtime);
        let self_events_meta = self.ensure_lua().create_table()?;
        self_events_meta.set(
            "__index",
            self.ensure_lua()
                .create_function(move |lua, (table, key): (Table, String)| {
                    if key == "meta" {
                        return table.get::<LuaValue>("meta");
                    }
                    if !is_named_event_channel(&key) {
                        return Ok(LuaValue::Nil);
                    }
                    let channel = create_backend_event_channel(lua, &key, Arc::clone(&runtime))?;
                    table.set(key.as_str(), channel.clone())?;
                    Ok(LuaValue::Table(channel))
                })?,
        )?;
        current_self.set_metatable(Some(self_events_meta))?;
        Ok(current_self)
    }
}

fn is_reserved_backend_hook(name: &str) -> bool {
    matches!(name, "init" | "start" | "stop")
}


fn create_backend_event_channel(
    lua: &Lua,
    event_name: &str,
    runtime: Arc<Mutex<BackendRuntime>>,
) -> mlua::Result<Table> {
    let channel = lua.create_table()?;
    let subscribers = lua.create_table()?;
    channel.set("__subscribers", subscribers.clone())?;
    channel.set(
        "subscribe",
        lua.create_function(move |lua, (table, callback): (Table, Function)| {
            let subscribers: Table = table.get("__subscribers")?;
            let id = subscribers.raw_len() + 1;
            subscribers.raw_set(id, callback)?;
            Ok(lua.create_function(move |_lua, ()| subscribers.raw_set(id, LuaValue::Nil))?)
        })?,
    )?;
    channel.set("on", channel.get::<Function>("subscribe")?)?;

    let fire_event_name = event_name.to_string();
    channel.set(
        "fire",
        lua.create_function(move |lua, (table, payload): (Table, Option<LuaValue>)| {
            let payload = match payload {
                Some(value) => lua.from_value::<JsonValue>(value)?,
                None => JsonValue::Null,
            };
            let subscribers: Table = table.get("__subscribers")?;
            for callback in subscribers.sequence_values::<Function>().flatten() {
                callback.call::<()>(lua.to_value(&payload)?)?;
            }
            runtime
                .lock()
                .unwrap()
                .pending_events
                .push(BackendScriptEvent {
                    name: fire_event_name.clone(),
                    payload,
                });
            Ok(())
        })?,
    )?;
    channel.set("emit", channel.get::<Function>("fire")?)?;
    Ok(channel)
}
