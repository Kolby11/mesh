use super::command::{
    BackendCommandOutcome, BackendCommandRegistry, command_error_result, command_result_from_lua,
};
use super::event::BackendEventRegistry;
use super::exec::{
    ExecService, ExecutableCapabilityPolicy, exec_denied_to_lua, missing_exec_capability,
    missing_exec_stream_capability, run_exec,
};
use super::exec_stream::{
    StreamEvent, StreamEventKind, StreamHandle, StreamState, StreamStatus,
    spawn_stream_with_launch_program,
};
use super::logging::log_message;
use super::{BackendScriptError, MIN_POLL_INTERVAL_MS};
use crate::operation::{release_side_effect, reserve_side_effect};
use crate::policy::RuntimePolicy;
use crate::session::RuntimeSession;
use crate::storage::{
    ScopedStorage, StorageManager, StorageScope,
    create_lua_storage_table_with_write_guard_and_charge,
};
use crate::util::{default_runtime_storage_root, is_named_event_channel};
use mesh_core_capability::CapabilitySet;
use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Executes a backend module's Luau script.
///
/// Exposes these host APIs to scripts:
/// - `start(self)` — required backend entrypoint called once after script load
/// - `mesh.service.set_poll_interval(ms)` — set polling interval
/// - `mesh.exec("program", {"arg1", "arg2"})` — run a system command
/// - `mesh.exec_stream("program", {"arg1", "arg2"})` — return a typed
///   stream handle with stable identity and lifecycle status
/// - `on_stream_event(self, event)` — receive typed started/line/eof/failed/
///   exited/overflow records for stream handles
/// - `mesh.config()` — return the full module settings Lua table
/// - `mesh.service.emit(table)` — emit service state
/// - `mesh.service.emit_json(value?)` — parse JSON text or emit a Lua table directly
/// - `mesh.service.emit_unavailable()` — emit unavailable state
/// - `self.EventName:fire(payload)` — publish a declared provider event
/// - `mesh.service.payload()` — get the current command payload as a Lua table
/// - `mesh.service.has_capability(name)` — check whether the module was granted a capability
/// - `mesh.service.can_exec(program, args)` — check the canonical executable policy
/// - `mesh.log(level, msg)` / `mesh.log.debug(msg)` / `mesh.log.info(msg)` / `mesh.log.warn(msg)` / `mesh.log.error(msg)`
pub struct BackendScriptContext {
    module_id: String,
    capabilities: HashSet<String>,
    pub(super) lua: Option<Lua>,
    script_environment: Option<Table>,
    cached_self_table: Option<Table>,
    runtime: Arc<Mutex<BackendRuntime>>,
    builtin_globals: HashSet<String>,
    storage: Arc<Mutex<ScopedStorage>>,
    exec: ExecService,
    exec_policy: ExecutableCapabilityPolicy,
    streams: Arc<StreamState>,
    policy: RuntimePolicy,
    host_side_effects_enabled: Arc<AtomicBool>,
    #[cfg(test)]
    host_setup_failure: Option<String>,
    script_loaded: bool,
    stop_attempted: bool,
    command_registry: Option<BackendCommandRegistry>,
    event_registry: Option<BackendEventRegistry>,
    generation: u64,
    session: RuntimeSession,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackendScriptEvent {
    pub name: String,
    pub payload: JsonValue,
    pub generation: u64,
}

impl BackendScriptEvent {
    pub(crate) fn queued_output_bytes(&self) -> usize {
        serde_json::to_vec(&serde_json::json!({
            "name": &self.name,
            "payload": &self.payload,
        }))
        .map_or(0, |bytes| bytes.len())
    }
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

const BACKEND_STARTUP_HOST_ERROR: &str =
    "backend host side effects are unavailable before start(self)";

fn require_started(host_side_effects_enabled: &AtomicBool) -> mlua::Result<()> {
    if host_side_effects_enabled.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err(mlua::Error::runtime(BACKEND_STARTUP_HOST_ERROR))
    }
}

impl BackendScriptContext {
    #[cfg(test)]
    pub fn new(module_id: impl Into<String>) -> Self {
        Self::new_with_settings_and_capabilities(
            module_id,
            serde_json::json!({}),
            Vec::<String>::new(),
        )
    }

    #[cfg(test)]
    pub fn new_with_settings(module_id: impl Into<String>, settings: JsonValue) -> Self {
        Self::new_with_settings_and_capabilities(module_id, settings, Vec::<String>::new())
    }

    #[cfg(test)]
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

    #[cfg(test)]
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

    #[cfg(test)]
    pub fn fail_host_setup_for_test(&mut self, message: impl Into<String>) {
        self.host_setup_failure = Some(message.into());
    }

    fn new_with_settings_capabilities_and_storage_root(
        module_id: impl Into<String>,
        settings: JsonValue,
        capabilities: impl IntoIterator<Item = String>,
        storage_root: impl Into<PathBuf>,
    ) -> Self {
        let module_id = module_id.into();
        let capabilities = capabilities.into_iter().collect::<HashSet<_>>();
        let exec_policy = ExecutableCapabilityPolicy::new(&capabilities);
        let policy = RuntimePolicy::default();
        let session = RuntimeSession::from_policy(
            module_id.clone(),
            CapabilitySet::from_ids(capabilities.iter().cloned()),
            policy.clone(),
        );
        let storage =
            StorageManager::new_with_limit(storage_root.into(), policy.storage_budget()).open(
                StorageScope::backend(module_id.clone(), module_id.clone(), module_id.clone()),
            );
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
            capabilities,
            lua: None,
            script_environment: None,
            cached_self_table: None,
            runtime,
            builtin_globals: HashSet::new(),
            storage: Arc::new(Mutex::new(storage)),
            exec: ExecService::new(policy.budget()),
            exec_policy,
            streams: StreamState::new_with_budget(policy.budget()),
            policy,
            host_side_effects_enabled: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            host_setup_failure: None,
            script_loaded: false,
            stop_attempted: false,
            command_registry: None,
            event_registry: None,
            generation: 0,
            session,
        }
    }

    /// Install the interface-owned command registry before loading provider
    /// code. The registry is immutable for the lifetime of this runtime
    /// generation.
    pub fn set_command_registry(&mut self, registry: BackendCommandRegistry) {
        self.command_registry = Some(registry);
    }

    pub fn command_registry(&self) -> Option<&BackendCommandRegistry> {
        self.command_registry.as_ref()
    }

    /// Install the provider-owned event registry before the Lua host is
    /// initialized. It is immutable for the lifetime of this runtime
    /// generation and is consulted by provider-owned `self.Event:fire` handles.
    pub fn set_event_registry(&mut self, registry: BackendEventRegistry) {
        self.event_registry = Some(registry);
    }

    pub fn event_registry(&self) -> Option<&BackendEventRegistry> {
        self.event_registry.as_ref()
    }

    pub fn set_generation(&mut self, generation: u64) {
        self.generation = generation;
        self.session.set_generation(generation);
    }

    /// The module-owned session that coordinates realm, host identity,
    /// resource accounting, lifecycle, state, and failure supervision.
    pub fn session(&self) -> &RuntimeSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut RuntimeSession {
        &mut self.session
    }

    pub(super) fn ensure_lua(&mut self) -> Result<&Lua, BackendScriptError> {
        if let Some(ref lua) = self.lua {
            return Ok(lua);
        }
        let lua =
            self.session
                .initialize_realm()
                .map_err(|error| BackendScriptError::HostSetup {
                    module_id: self.module_id.clone(),
                    message: error.to_string(),
                })?;
        self.lua = Some(lua);
        #[cfg(test)]
        if let Some(message) = self.host_setup_failure.take() {
            self.lua = None;
            return Err(BackendScriptError::HostSetup {
                module_id: self.module_id.clone(),
                message,
            });
        }
        let globals =
            self.lua
                .as_ref()
                .map(Lua::globals)
                .ok_or_else(|| BackendScriptError::HostSetup {
                    module_id: self.module_id.clone(),
                    message: "Lua runtime disappeared during initialization".to_string(),
                })?;
        if let Err(error) = self.install_host_api(&globals) {
            self.lua = None;
            self.cached_self_table = None;
            return Err(BackendScriptError::HostSetup {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            });
        }
        self.builtin_globals = globals
            .pairs::<String, LuaValue>()
            .filter_map(|result| result.ok().map(|(key, _)| key))
            .collect();
        self.lua
            .as_ref()
            .ok_or_else(|| BackendScriptError::HostSetup {
                module_id: self.module_id.clone(),
                message: "Lua runtime disappeared after host installation".to_string(),
            })
    }

    fn lua_ref(&self) -> mlua::Result<&Lua> {
        self.lua
            .as_ref()
            .ok_or_else(|| mlua::Error::runtime("backend Lua runtime is not initialized"))
    }

    fn backend_lua(&self) -> Result<&Lua, BackendScriptError> {
        self.lua_ref()
            .map_err(|error| BackendScriptError::HostSetup {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            })
    }

    pub fn poll_interval_ms(&self) -> u64 {
        self.runtime.lock().unwrap().poll_interval_ms
    }

    /// Load and execute a backend Luau script in the startup staging phase.
    ///
    /// Top-level declarations are installed normally, but every mutating host
    /// handle checks the startup phase and rejects calls until `start(self)` is
    /// entered. This keeps source-load failures from spawning processes,
    /// publishing events, changing polling, or mutating durable storage.
    pub fn load_script(&mut self, source: &str) -> Result<(), BackendScriptError> {
        self.host_side_effects_enabled
            .store(false, Ordering::Release);
        let _budget = self.policy.begin_callback();
        self.ensure_lua()?
            .load(source)
            .set_name(&self.module_id)
            .exec()
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        self.bind_script_environment_to_host_table()
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        self.script_environment = self
            .ensure_lua()?
            .globals()
            .get::<Function>("start")
            .ok()
            .and_then(|function| function.environment());
        self.script_loaded = true;
        self.session
            .mark_loaded()
            .map_err(|error| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            })?;
        tracing::info!("loaded backend script for {}", self.module_id);
        Ok(())
    }

    /// Call the backend script's `start(self)` startup entrypoint once after load.
    pub fn call_init(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        let _budget = self.policy.begin_callback();
        self.session
            .begin_start()
            .map_err(|error| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            })?;
        self.reset_for_call(JsonValue::Null);
        let globals = self.script_environment()?;
        let entrypoint = globals.get::<Function>("start").map_err(|_| {
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
        self.host_side_effects_enabled
            .store(true, Ordering::Release);
        match entrypoint.call::<()>(current_self) {
            Ok(()) => {
                self.session
                    .mark_running()
                    .map_err(|error| BackendScriptError::Runtime {
                        module_id: self.module_id.clone(),
                        message: error.to_string(),
                    })?;
                self.take_service_state_snapshot()
            }
            Err(error) => {
                self.session.record_failure(error.to_string());
                Err(BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: error.to_string(),
                })
            }
        }
    }

    /// Call the backend script's optional `stop(self)` lifecycle hook.
    pub fn call_stop(&mut self) -> Result<(), BackendScriptError> {
        if self.stop_attempted {
            return Ok(());
        }
        self.stop_attempted = true;
        self.session
            .begin_stop()
            .map_err(|error| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            })?;
        self.host_side_effects_enabled
            .store(true, Ordering::Release);
        let _budget = self.policy.begin_callback();
        let result = if !self.script_loaded {
            Ok(())
        } else {
            (|| {
                self.reset_for_call(JsonValue::Null);
                let globals = self.script_environment()?;
                let stop = match globals.get::<Function>("stop") {
                    Ok(stop) => stop,
                    Err(_) => return Ok(()),
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
                    })
            })()
        };
        self.flush_storage();
        self.session
            .finish_stop()
            .map_err(|error| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            })?;
        result
    }

    /// Await termination and child reaping for every stream owned by this
    /// backend generation. This is the normal shutdown path; `Drop` keeps a
    /// synchronous abort fallback for cancellation and panic paths.
    pub async fn shutdown_streams(&self) {
        self.streams.shutdown().await;
    }

    /// Cancel and reap synchronous `mesh.exec` workers before the async
    /// stream supervisor is torn down.
    pub fn shutdown_exec(&self) {
        self.exec.shutdown();
    }

    /// Shared subprocess-stream state. The backend service loop awaits on
    /// `stream_state().wait_for_event()` to react to lines from
    /// `mesh.exec_stream` subprocesses.
    pub fn stream_state(&self) -> Arc<StreamState> {
        Arc::clone(&self.streams)
    }

    /// Dispatch one wakeup's worth of subprocess lines to the script.
    ///
    /// `on_stream_batch(self, program, lines)` receives the full ordered batch;
    /// scripts that only need a "something changed" signal can ignore `lines`.
    /// Otherwise `on_stream_line(self, program, line)` runs once per line.
    /// Returns the state snapshot taken after the whole batch.
    pub fn run_stream_batch(
        &mut self,
        program: &str,
        lines: &[String],
    ) -> Result<Option<JsonValue>, BackendScriptError> {
        self.run_stream_batch_with_program(program, lines)
    }

    /// Dispatch a legacy line batch while retaining the stream identity at
    /// the Rust boundary. The Lua compatibility hook still receives the
    /// program string; new scripts can use `on_stream_event` for the typed
    /// handle and lifecycle records.
    pub fn run_stream_batch_for_stream(
        &mut self,
        stream: &StreamHandle,
        lines: &[String],
    ) -> Result<Option<JsonValue>, BackendScriptError> {
        self.run_stream_batch_with_program(stream.program(), lines)
    }

    fn run_stream_batch_with_program(
        &mut self,
        program: &str,
        lines: &[String],
    ) -> Result<Option<JsonValue>, BackendScriptError> {
        if lines.is_empty() {
            return Ok(None);
        }
        let _budget = self.policy.begin_callback();
        self.host_side_effects_enabled
            .store(true, Ordering::Release);
        self.reset_for_call(JsonValue::Null);
        let globals = self.script_environment()?;
        if let Ok(batch_handler) = globals.get::<Function>("on_stream_batch") {
            let current_self =
                self.current_self_table()
                    .map_err(|err| BackendScriptError::Runtime {
                        module_id: self.module_id.clone(),
                        message: err.to_string(),
                    })?;
            let lines_table = self
                .backend_lua()?
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

    /// Whether this script opted into typed stream lifecycle records.
    pub fn has_stream_event_handler(&mut self) -> bool {
        self.script_environment()
            .is_ok_and(|globals| globals.get::<Function>("on_stream_event").is_ok())
    }

    /// Dispatch one typed stream record to `on_stream_event(self, event)`.
    pub fn run_stream_event(
        &mut self,
        event: &StreamEvent,
    ) -> Result<Option<JsonValue>, BackendScriptError> {
        let _budget = self.policy.begin_callback();
        self.host_side_effects_enabled
            .store(true, Ordering::Release);
        self.reset_for_call(JsonValue::Null);
        let globals = self.script_environment()?;
        let handler = match globals.get::<Function>("on_stream_event") {
            Ok(handler) => handler,
            Err(_) => return Ok(None),
        };
        let current_self =
            self.current_self_table()
                .map_err(|err| BackendScriptError::Runtime {
                    module_id: self.module_id.clone(),
                    message: err.to_string(),
                })?;
        let event_table = stream_event_table(self.backend_lua()?, event).map_err(|err| {
            BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            }
        })?;
        handler
            .call::<()>((current_self, event_table))
            .map_err(|err| BackendScriptError::Runtime {
                module_id: self.module_id.clone(),
                message: err.to_string(),
            })?;
        self.take_service_state_snapshot()
    }

    /// Kill every active `mesh.exec_stream` subprocess. Idempotent; safe to
    /// call from `Drop` and from `stop(self)` lifecycle.
    pub fn kill_streams(&self) {
        self.streams.kill_all();
    }

    /// Call `on_poll()` if it exists. Returns any exported service state.
    pub fn run_poll(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        let _budget = self.policy.begin_callback();
        self.host_side_effects_enabled
            .store(true, Ordering::Release);
        self.reset_for_call(JsonValue::Null);
        let globals = self.script_environment()?;
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
        let _budget = self.policy.begin_callback();
        self.host_side_effects_enabled
            .store(true, Ordering::Release);
        self.reset_for_call(payload.clone());
        if let Some(registry) = &self.command_registry {
            if registry.validate_payload(command, payload).is_err() {
                return Ok(None);
            }
        }
        let normalized = command.replace('-', "_");

        let globals = self.script_environment()?;
        let handler_name = format!("on_command_{normalized}");
        let handler = match globals.get::<Function>(handler_name.as_str()) {
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
        let _budget = self.policy.begin_callback();
        self.host_side_effects_enabled
            .store(true, Ordering::Release);
        self.reset_for_call(payload.clone());
        let normalized = command.replace('-', "_");

        let globals = self.script_environment()?;
        let handler_name = format!("on_command_{normalized}");
        if let Some(registry) = &self.command_registry {
            if let Err(message) = registry.validate_payload(command, payload) {
                return Ok(BackendCommandOutcome {
                    state: None,
                    result: serde_json::json!({
                        "ok": false,
                        "status": "invalid_arguments",
                        "error": message,
                    }),
                    error: None,
                });
            }
        }

        let previous_state = self.capture_state_for_rollback()?;
        let handler = match globals.get::<Function>(handler_name.as_str()) {
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
                self.rollback_command_state(previous_state);
                return Ok(BackendCommandOutcome {
                    state: None,
                    result: command_error_result(message.clone()),
                    error: Some(message),
                });
            }
        };
        let state = match self.take_service_state_snapshot() {
            Ok(state) => state,
            Err(error) => {
                self.rollback_command_state(previous_state);
                return Err(error);
            }
        };
        let module_id = self.module_id.clone();
        let lua = self.backend_lua()?;
        let result = match command_result_from_lua(lua, &module_id, returned) {
            Ok(result) => result,
            Err(error) => {
                self.rollback_command_state(previous_state);
                return Err(error);
            }
        };
        let result_bytes = match serde_json::to_vec(&result) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.rollback_command_state(previous_state);
                return Err(BackendScriptError::CommandResultConversionFailed {
                    module_id: self.module_id.clone(),
                    message: format!("failed to size command result: {error}"),
                });
            }
        };
        if let Err(error) = self
            .policy
            .budget()
            .validate_json(&result, "command result")
        {
            self.rollback_command_state(previous_state);
            return Err(BackendScriptError::CommandResultConversionFailed {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            });
        }
        if let Err(error) = self.policy.budget().reserve_output(result_bytes.len()) {
            self.rollback_command_state(previous_state);
            return Err(BackendScriptError::CommandResultConversionFailed {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            });
        }
        if let Some(registry) = &self.command_registry {
            if let Err(message) = registry.validate_result(command, &result) {
                self.rollback_command_state(previous_state);
                return Err(BackendScriptError::CommandResultConversionFailed {
                    module_id: self.module_id.clone(),
                    message,
                });
            }
        }
        Ok(BackendCommandOutcome {
            state,
            result,
            error: None,
        })
    }

    pub fn take_service_state_snapshot(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        if let Some(payload) = self.take_pending_emit() {
            self.policy
                .budget()
                .validate_json(&payload, "service state snapshot")
                .map_err(|error| BackendScriptError::SnapshotFailed {
                    module_id: self.module_id.clone(),
                    message: error.to_string(),
                })?;
            return Ok(Some(payload));
        }

        let globals = self.script_environment()?;
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

        let payload = self
            .backend_lua()?
            .from_value::<JsonValue>(state)
            .map_err(|err| BackendScriptError::SnapshotFailed {
                module_id: self.module_id.clone(),
                message: format!("failed to convert state to JSON: {err}"),
            })?;
        let output_bytes =
            serde_json::to_vec(&payload).map_err(|error| BackendScriptError::SnapshotFailed {
                module_id: self.module_id.clone(),
                message: format!("failed to size state snapshot: {error}"),
            })?;
        self.policy
            .budget()
            .validate_json(&payload, "service state snapshot")
            .map_err(|error| BackendScriptError::SnapshotFailed {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            })?;
        self.policy
            .budget()
            .reserve_output(output_bytes.len())
            .map_err(|error| BackendScriptError::SnapshotFailed {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            })?;
        Ok(Some(payload))
    }

    pub fn drain_events(&self) -> Vec<BackendScriptEvent> {
        let events = std::mem::take(&mut self.runtime.lock().unwrap().pending_events);
        let bytes = events
            .iter()
            .map(BackendScriptEvent::queued_output_bytes)
            .sum();
        self.policy.budget().release_event(events.len());
        release_side_effect(&self.policy.budget(), events.len(), bytes);
        events
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
        let Ok(globals) = self.script_environment() else {
            return Vec::new();
        };
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
        let mesh = self.lua_ref()?.create_table()?;
        self.install_service_api(&mesh)?;
        self.install_exec_api(&mesh)?;
        self.install_config_api(&mesh)?;
        self.install_log_api(&mesh)?;
        globals.set("mesh", mesh)?;
        Ok(())
    }

    fn script_environment(&mut self) -> Result<Table, BackendScriptError> {
        if let Some(environment) = &self.script_environment {
            return Ok(environment.clone());
        }
        self.lua_ref()
            .map(|lua| lua.globals())
            .map_err(|error| BackendScriptError::HostSetup {
                module_id: self.module_id.clone(),
                message: error.to_string(),
            })
    }

    fn bind_script_environment_to_host_table(&mut self) -> mlua::Result<()> {
        let lua = self.lua_ref()?;
        let globals = lua.globals();
        let mesh = globals.get::<Table>("mesh")?;
        if let Ok(start) = globals.get::<Function>("start")
            && let Some(environment) = start.environment()
        {
            environment.raw_set("mesh", mesh)?;
        }
        Ok(())
    }

    fn install_service_api(&mut self, mesh: &Table) -> mlua::Result<()> {
        let service = self.lua_ref()?.create_table()?;
        let resources = self.policy.budget();
        let module_id = self.module_id.clone();
        let runtime = Arc::clone(&self.runtime);
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        service.set(
            "set_poll_interval",
            self.lua_ref()?.create_function(move |_lua, ms: u64| {
                require_started(&host_side_effects_enabled)?;
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
        let resources_for_emit = resources.clone();
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        service.set(
            "emit",
            self.lua_ref()?
                .create_function(move |lua, value: LuaValue| {
                    require_started(&host_side_effects_enabled)?;
                    let payload = lua.from_value::<JsonValue>(value)?;
                    resources_for_emit
                        .validate_json(&payload, "service payload")
                        .map_err(|error| mlua::Error::external(error.to_string()))?;
                    let output_bytes = serde_json::to_vec(&payload)
                        .map_err(mlua::Error::external)?
                        .len();
                    resources_for_emit
                        .reserve_output(output_bytes)
                        .map_err(|error| mlua::Error::external(error.to_string()))?;
                    runtime.lock().unwrap().pending_emit = Some(payload);
                    Ok(())
                })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        let resources_for_json = resources.clone();
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        service.set(
            "emit_json",
            self.lua_ref()?
                .create_function(move |lua, value: Option<LuaValue>| {
                    require_started(&host_side_effects_enabled)?;
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
                    let output_bytes = serde_json::to_vec(&payload)
                        .map_err(mlua::Error::external)?
                        .len();
                    resources_for_json
                        .validate_json(&payload, "service payload")
                        .map_err(|error| mlua::Error::external(error.to_string()))?;
                    resources_for_json
                        .reserve_output(output_bytes)
                        .map_err(|error| mlua::Error::external(error.to_string()))?;
                    runtime.lock().unwrap().pending_emit = Some(payload);
                    Ok(())
                })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        let resources_for_unavailable = resources.clone();
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        service.set(
            "emit_unavailable",
            self.lua_ref()?.create_function(move |_lua, ()| {
                require_started(&host_side_effects_enabled)?;
                resources_for_unavailable
                    .reserve_output(64)
                    .map_err(|error| mlua::Error::external(error.to_string()))?;
                runtime.lock().unwrap().pending_emit = Some(serde_json::json!({
                    "available": false,
                    "source_module": module_id,
                }));
                Ok(())
            })?,
        )?;

        let runtime = Arc::clone(&self.runtime);
        service.set(
            "payload",
            self.lua_ref()?.create_function(move |lua, ()| {
                let payload = runtime.lock().unwrap().current_payload.clone();
                lua.to_value(&payload)
            })?,
        )?;

        let capabilities = self.capabilities.clone();
        service.set(
            "has_capability",
            self.lua_ref()?
                .create_function(move |_lua, capability: String| {
                    Ok(capabilities.contains(capability.as_str()))
                })?,
        )?;

        let executable_policy = self.exec_policy.clone();
        service.set(
            "can_exec",
            self.lua_ref()?.create_function(
                move |_lua, (program, args): (String, Vec<String>)| {
                    Ok(executable_policy.allows(&program, &args))
                },
            )?,
        )?;

        mesh.set("service", service)?;
        Ok(())
    }

    fn install_exec_api(&mut self, mesh: &Table) -> mlua::Result<()> {
        let executable_policy = self.exec_policy.clone();
        let module_id = self.module_id.clone();
        let resources = self.policy.budget();
        let exec = self.exec.clone();
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        mesh.set(
            "exec",
            self.lua_ref()?.create_function(
                move |lua, (program, args): (String, Vec<String>)| {
                    require_started(&host_side_effects_enabled)?;
                    if let Some(required) =
                        missing_exec_capability(&executable_policy, &program, &args)
                    {
                        tracing::warn!(
                            module_id = %module_id,
                            program = %program,
                            required_capability = %required,
                            "denied backend exec"
                        );
                        return exec_denied_to_lua(lua, &program, &required, &resources);
                    }

                    let launch_program =
                        executable_policy.canonical_launch_program(&program, &args);
                    run_exec(
                        lua,
                        launch_program.as_deref().unwrap_or(&program),
                        &program,
                        &args,
                        &exec,
                    )
                },
            )?,
        )?;

        let executable_policy = self.exec_policy.clone();
        let module_id = self.module_id.clone();
        let streams = Arc::clone(&self.streams);
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        mesh.set(
            "exec_stream",
            self.lua_ref()?.create_function(
                move |_lua, (program, args): (String, Vec<String>)| {
                    require_started(&host_side_effects_enabled)?;
                    if let Some(required) =
                        missing_exec_stream_capability(&executable_policy, &program, &args)
                    {
                        tracing::warn!(
                            module_id = %module_id,
                            program = %program,
                            required_capability = %required,
                            "denied backend exec_stream"
                        );
                        return Ok(LuaValue::Boolean(false));
                    }
                    let launch_program = executable_policy
                        .canonical_launch_program(&program, &args)
                        .unwrap_or_else(|| program.clone());
                    match spawn_stream_with_launch_program(
                        &streams,
                        program.clone(),
                        args,
                        launch_program,
                        program.clone(),
                    ) {
                        Ok(handle) => Ok(LuaValue::Table(stream_handle_table(_lua, &handle)?)),
                        Err(err) => {
                            tracing::warn!(
                                module_id = %module_id,
                                program = %program,
                                "exec_stream failed to spawn: {err}"
                            );
                            Ok(LuaValue::Boolean(false))
                        }
                    }
                },
            )?,
        )?;

        Ok(())
    }

    fn install_config_api(&mut self, mesh: &Table) -> mlua::Result<()> {
        let runtime = Arc::clone(&self.runtime);
        let resources = self.policy.budget();
        mesh.set(
            "config",
            self.lua_ref()?.create_function(move |lua, ()| {
                let settings = runtime.lock().unwrap().settings.clone();
                let output_bytes = serde_json::to_vec(&settings)
                    .map_err(mlua::Error::external)?
                    .len();
                resources
                    .reserve_output(output_bytes)
                    .map_err(|error| mlua::Error::external(error.to_string()))?;
                lua.to_value(&settings)
            })?,
        )?;
        Ok(())
    }

    fn install_log_api(&mut self, mesh: &Table) -> mlua::Result<()> {
        let log = self.lua_ref()?.create_table()?;
        let module_id = self.module_id.clone();
        let resources = self.policy.budget();
        let resources_for_call = resources.clone();
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        let call_log = self.lua_ref()?.create_function(
            move |_lua, (_self, level, message): (mlua::Table, String, String)| {
                require_started(&host_side_effects_enabled)?;
                resources_for_call
                    .reserve_output(message.len())
                    .map_err(|error| mlua::Error::external(error.to_string()))?;
                log_message(&module_id, &level, &message);
                Ok(())
            },
        )?;
        let log_meta = self.lua_ref()?.create_table()?;
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
            let resources_for_level = resources.clone();
            let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
            log.set(
                name,
                self.lua_ref()?
                    .create_function(move |_lua, message: String| {
                        require_started(&host_side_effects_enabled)?;
                        resources_for_level
                            .reserve_output(message.len())
                            .map_err(|error| mlua::Error::external(error.to_string()))?;
                        log_message(&module_id, level, &message);
                        Ok(())
                    })?,
            )?;
        }

        mesh.set("log", log)?;
        Ok(())
    }

    fn reset_for_call(&mut self, payload: JsonValue) {
        let mut runtime = self.runtime.lock().unwrap();
        runtime.pending_emit = None;
        let event_count = runtime.pending_events.len();
        let event_bytes = runtime
            .pending_events
            .iter()
            .map(BackendScriptEvent::queued_output_bytes)
            .sum();
        runtime.pending_events.clear();
        drop(runtime);
        self.policy.budget().release_event(event_count);
        release_side_effect(&self.policy.budget(), event_count, event_bytes);
        let mut runtime = self.runtime.lock().unwrap();
        runtime.current_payload = payload;
    }

    fn capture_state_for_rollback(&mut self) -> Result<Option<JsonValue>, BackendScriptError> {
        let state = self
            .script_environment()?
            .get::<LuaValue>("state")
            .map_err(|err| BackendScriptError::SnapshotFailed {
                module_id: self.module_id.clone(),
                message: format!("failed to read state before command: {err}"),
            })?;
        if matches!(state, LuaValue::Nil) {
            return Ok(None);
        }
        self.backend_lua()?
            .from_value::<JsonValue>(state)
            .map(Some)
            .map_err(|err| BackendScriptError::SnapshotFailed {
                module_id: self.module_id.clone(),
                message: format!("failed to convert state before command: {err}"),
            })
    }

    fn rollback_command_state(&mut self, previous: Option<JsonValue>) {
        let Ok(environment) = self.script_environment() else {
            self.reset_for_call(JsonValue::Null);
            return;
        };
        let _ = environment.set(
            "state",
            match previous {
                Some(value) => self
                    .lua_ref()
                    .ok()
                    .and_then(|lua| lua.to_value(&value).ok())
                    .unwrap_or(LuaValue::Nil),
                None => LuaValue::Nil,
            },
        );
        self.reset_for_call(JsonValue::Null);
    }

    fn take_pending_emit(&self) -> Option<JsonValue> {
        self.runtime.lock().unwrap().pending_emit.take()
    }

    fn current_self_table(&mut self) -> mlua::Result<mlua::Table> {
        if let Some(table) = &self.cached_self_table {
            return Ok(table.clone());
        }
        let current_self = self.lua_ref()?.create_table()?;
        let meta = self.lua_ref()?.create_table()?;
        meta.set("module_id", self.module_id.as_str())?;
        meta.set("provider_id", self.module_id.as_str())?;
        meta.set("kind", "backend")?;
        meta.set("instance_id", self.module_id.as_str())?;
        meta.set("diagnostics_id", self.module_id.as_str())?;
        current_self.set("meta", meta)?;
        let runtime_for_storage_diagnostics = Arc::clone(&self.runtime);
        let storage_arc = Arc::clone(&self.storage);
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        let storage_resources = self.policy.budget();
        let storage = create_lua_storage_table_with_write_guard_and_charge(
            self.lua_ref()?,
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
            Arc::new(move || require_started(&host_side_effects_enabled)),
            Arc::new(move |bytes| {
                storage_resources
                    .reserve_storage(bytes)
                    .map_err(|error| mlua::Error::external(error.to_string()))
            }),
        )?;
        current_self.set("storage", storage)?;
        let runtime = Arc::clone(&self.runtime);
        let resources = self.policy.budget();
        let generation = self.generation;
        let self_events_meta = self.lua_ref()?.create_table()?;
        let event_registry = self.event_registry.clone();
        let host_side_effects_enabled = Arc::clone(&self.host_side_effects_enabled);
        self_events_meta.set(
            "__index",
            self.lua_ref()?
                .create_function(move |lua, (table, key): (Table, String)| {
                    if key == "meta" {
                        return table.get::<LuaValue>("meta");
                    }
                    if !is_named_event_channel(&key) {
                        return Ok(LuaValue::Nil);
                    }
                    let channel = create_backend_event_channel(
                        lua,
                        &key,
                        Arc::clone(&runtime),
                        resources.clone(),
                        event_registry.clone(),
                        generation,
                        Arc::clone(&host_side_effects_enabled),
                    )?;
                    table.set(key.as_str(), channel.clone())?;
                    Ok(LuaValue::Table(channel))
                })?,
        )?;
        current_self.set_metatable(Some(self_events_meta))?;
        self.cached_self_table = Some(current_self.clone());
        Ok(current_self)
    }
}

impl Drop for BackendScriptContext {
    fn drop(&mut self) {
        // A cancelled or panicking backend future cannot await the authored
        // stop hook, but Rust-owned resources must still be reclaimed. The
        // normal lifecycle guard calls call_stop() first; both operations are
        // intentionally idempotent.
        self.kill_streams();
        self.shutdown_exec();
        self.flush_storage();
    }
}

fn is_reserved_backend_hook(name: &str) -> bool {
    matches!(name, "init" | "start" | "stop")
}

fn stream_status_name(status: &StreamStatus) -> &'static str {
    match status {
        StreamStatus::Starting => "starting",
        StreamStatus::Running => "running",
        StreamStatus::Eof => "eof",
        StreamStatus::Stopping => "stopping",
        StreamStatus::Failed { .. } => "failed",
        StreamStatus::Exited(_) => "exited",
    }
}

fn stream_handle_table(lua: &Lua, stream: &StreamHandle) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", stream.id().raw())?;
    table.set("generation", stream.generation())?;
    table.set("program", stream.program())?;
    table.set(
        "args",
        lua.create_sequence_from(stream.args().iter().cloned())?,
    )?;
    table.set("status", stream_status_name(&stream.status()))?;
    Ok(table)
}

fn stream_event_table(lua: &Lua, event: &StreamEvent) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let stream = stream_handle_table(lua, &event.stream)?;
    table.set("stream", stream)?;
    match &event.kind {
        StreamEventKind::Started => table.set("type", "started")?,
        StreamEventKind::Line(line) => {
            table.set("type", "line")?;
            table.set("line", line.as_str())?;
        }
        StreamEventKind::Eof => table.set("type", "eof")?,
        StreamEventKind::Failed(message) => {
            table.set("type", "failed")?;
            table.set("error", message.as_str())?;
        }
        StreamEventKind::Exited(status) => {
            table.set("type", "exited")?;
            table.set("success", status.success)?;
            table.set("code", status.code)?;
            table.set("signal", status.signal)?;
        }
        StreamEventKind::Overflow { dropped } => {
            table.set("type", "overflow")?;
            table.set("dropped", *dropped)?;
        }
    }
    Ok(table)
}

fn create_backend_event_channel(
    lua: &Lua,
    event_name: &str,
    runtime: Arc<Mutex<BackendRuntime>>,
    resources: crate::policy::ResourceBudget,
    event_registry: Option<BackendEventRegistry>,
    generation: u64,
    host_side_effects_enabled: Arc<AtomicBool>,
) -> mlua::Result<Table> {
    let channel = lua.create_table()?;
    let subscribers = lua.create_table()?;
    let next_subscription_id = Arc::new(AtomicU64::new(1));
    let subscribe_host_side_effects_enabled = Arc::clone(&host_side_effects_enabled);
    channel.set("__subscribers", subscribers.clone())?;
    channel.set(
        "subscribe",
        lua.create_function(move |lua, (table, callback): (Table, Function)| {
            require_started(&subscribe_host_side_effects_enabled)?;
            let subscribers: Table = table.get("__subscribers")?;
            let id = next_subscription_id.fetch_add(1, Ordering::Relaxed);
            subscribers.raw_set(id, callback)?;
            Ok(lua.create_function(move |_lua, ()| subscribers.raw_set(id, LuaValue::Nil))?)
        })?,
    )?;
    channel.set("on", channel.get::<Function>("subscribe")?)?;

    let fire_event_name = event_name.to_string();
    let fire_registry = event_registry;
    let host_side_effects_enabled = Arc::clone(&host_side_effects_enabled);
    channel.set(
        "fire",
        lua.create_function(move |lua, (table, payload): (Table, Option<LuaValue>)| {
            require_started(&host_side_effects_enabled)?;
            let payload = match payload {
                Some(value) => lua.from_value::<JsonValue>(value)?,
                None => JsonValue::Object(serde_json::Map::new()),
            };
            if let Some(registry) = &fire_registry {
                registry
                    .validate_payload(&fire_event_name, &payload)
                    .map_err(mlua::Error::runtime)?;
            }
            resources
                .reserve_event()
                .map_err(|error| mlua::Error::external(error.to_string()))?;
            let output_bytes = serde_json::to_vec(&serde_json::json!({
                "name": &fire_event_name,
                "payload": &payload,
            }))
            .map_err(mlua::Error::external)?
            .len();
            if let Err(error) = reserve_side_effect(&resources, output_bytes) {
                resources.release_event(1);
                return Err(mlua::Error::external(error));
            }
            let subscribers: Table = table.get("__subscribers")?;
            dispatch_backend_event_subscribers(
                &subscribers,
                lua.to_value(&payload)?,
                &fire_event_name,
            );
            runtime
                .lock()
                .unwrap()
                .pending_events
                .push(BackendScriptEvent {
                    name: fire_event_name.clone(),
                    payload,
                    generation,
                });
            Ok(())
        })?,
    )?;
    channel.set("emit", channel.get::<Function>("fire")?)?;
    Ok(channel)
}

/// Dispatch a stable snapshot of subscription IDs. A callback can close its
/// own or another subscription without creating a sequence hole that skips
/// later subscribers. Failures are reported one at a time and never prevent
/// the provider event from being queued below.
fn dispatch_backend_event_subscribers(subscribers: &Table, payload: LuaValue, event_name: &str) {
    let mut subscription_ids = match subscribers
        .pairs::<u64, Function>()
        .map(|pair| pair.map(|(id, _)| id))
        .collect::<mlua::Result<Vec<_>>>()
    {
        Ok(subscription_ids) => subscription_ids,
        Err(error) => {
            tracing::warn!(
                event = event_name,
                error = %error,
                "event subscriber iteration failed"
            );
            return;
        }
    };
    subscription_ids.sort_unstable();

    for subscription_id in subscription_ids {
        let callback = match subscribers.raw_get::<LuaValue>(subscription_id) {
            Ok(LuaValue::Function(callback)) => callback,
            Ok(_) => continue,
            Err(error) => {
                tracing::warn!(
                    event = event_name,
                    subscription_id,
                    error = %error,
                    "event subscriber lookup failed; continuing dispatch"
                );
                continue;
            }
        };
        if let Err(error) = callback.call::<()>(payload.clone()) {
            tracing::warn!(
                event = event_name,
                subscription_id,
                error = %error,
                "event subscriber callback failed; continuing dispatch"
            );
        }
    }
}
