/// Backend plugin Luau runtime.
///
/// Backend plugins run inside a real Luau VM via `mlua`. The shell host only
/// injects generic host APIs; all service-specific logic remains in plugin
/// scripts.
use mlua::{Function, Lua, LuaSerdeExt, Value as LuaValue};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::process::Command as StdCommand;
use std::sync::{Arc, Mutex};

pub const MIN_POLL_INTERVAL_MS: u64 = 50;

/// Executes a backend plugin's Luau script.
///
/// Exposes these host APIs to scripts:
/// - `init()` — required backend entrypoint called once after script load
/// - `mesh.service.set_poll_interval(ms)` — set polling interval
/// - `mesh.exec("program", {"arg1", "arg2"})` — run a system command
/// - `mesh.config()` — return the full plugin settings Lua table
/// - `mesh.service.emit(table)` — emit service state
/// - `mesh.service.emit_json(value?)` — parse JSON text or emit a Lua table directly
/// - `mesh.service.emit_unavailable()` — emit unavailable state
/// - `mesh.service.payload()` — get the current command payload as a Lua table
/// - `mesh.service.has_capability(name)` — check whether the plugin was granted a capability
/// - `mesh.log(level, msg)` / `mesh.log.debug(msg)` / `mesh.log.info(msg)` / `mesh.log.warn(msg)` / `mesh.log.error(msg)`
pub struct BackendScriptContext {
    plugin_id: String,
    capabilities: HashSet<String>,
    lua: Lua,
    runtime: Arc<Mutex<BackendRuntime>>,
}

#[derive(Debug, Default)]
struct BackendRuntime {
    poll_interval_ms: u64,
    pending_emit: Option<JsonValue>,
    current_payload: JsonValue,
    settings: JsonValue,
}

#[derive(Debug, Clone)]
struct ExecOutcome {
    success: bool,
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

impl BackendScriptContext {
    pub fn new(plugin_id: impl Into<String>) -> Self {
        Self::new_with_settings_and_capabilities(
            plugin_id,
            serde_json::json!({}),
            Vec::<String>::new(),
        )
    }

    pub fn new_with_settings(plugin_id: impl Into<String>, settings: JsonValue) -> Self {
        Self::new_with_settings_and_capabilities(plugin_id, settings, Vec::<String>::new())
    }

    pub fn new_with_capabilities(
        plugin_id: impl Into<String>,
        capabilities: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::new_with_settings_and_capabilities(plugin_id, serde_json::json!({}), capabilities)
    }

    pub fn new_with_settings_and_capabilities(
        plugin_id: impl Into<String>,
        settings: JsonValue,
        capabilities: impl IntoIterator<Item = String>,
    ) -> Self {
        let plugin_id = plugin_id.into();
        let lua = Lua::new();
        let runtime = Arc::new(Mutex::new(BackendRuntime {
            poll_interval_ms: 1000,
            pending_emit: None,
            current_payload: JsonValue::Null,
            settings,
        }));

        let mut ctx = Self {
            plugin_id,
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
            .set_name(&self.plugin_id)
            .exec()
            .map_err(|err| BackendScriptError::Runtime {
                plugin_id: self.plugin_id.clone(),
                message: err.to_string(),
            })?;
        tracing::info!("loaded backend script for {}", self.plugin_id);
        Ok(())
    }

    /// Call the backend script's required `init()` entrypoint once after load.
    pub fn call_init(&mut self) -> Result<(), BackendScriptError> {
        let globals = self.lua.globals();
        let init =
            globals
                .get::<Function>("init")
                .map_err(|_| BackendScriptError::MissingEntrypoint {
                    plugin_id: self.plugin_id.clone(),
                    name: "init".to_string(),
                })?;
        init.call::<()>(())
            .map_err(|err| BackendScriptError::Runtime {
                plugin_id: self.plugin_id.clone(),
                message: err.to_string(),
            })?;
        Ok(())
    }

    /// Call `on_poll()` if it exists. Returns any emitted payload.
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
                plugin_id: self.plugin_id.clone(),
                message: err.to_string(),
            })?;
        Ok(self.take_pending_emit())
    }

    /// Call `on_command_<name>()` for the given command. Returns any emitted payload.
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
                plugin_id: self.plugin_id.clone(),
                message: err.to_string(),
            })?;
        Ok(self.take_pending_emit())
    }

    fn install_host_api(&mut self) -> mlua::Result<()> {
        let globals = self.lua.globals();
        let mesh = self.lua.create_table()?;
        let service = self.lua.create_table()?;
        let log = self.lua.create_table()?;

        let plugin_id = self.plugin_id.clone();
        let runtime = Arc::clone(&self.runtime);
        service.set(
            "set_poll_interval",
            self.lua.create_function(move |_lua, ms: u64| {
                let poll_interval_ms = ms.max(MIN_POLL_INTERVAL_MS);
                if poll_interval_ms != ms {
                    tracing::warn!(
                        plugin_id = plugin_id,
                        requested_interval_ms = ms,
                        clamped_interval_ms = poll_interval_ms,
                        "backend poll interval below minimum; clamping"
                    );
                }
                runtime.lock().unwrap().poll_interval_ms = poll_interval_ms;
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
                    "source_plugin": plugin_id,
                }));
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

        mesh.set(
            "exec",
            self.lua
                .create_function(move |lua, (program, args): (String, Vec<String>)| {
                    run_exec(&lua, &program, &args)
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

        let plugin_id = self.plugin_id.clone();
        let call_log = self.lua.create_function(
            move |_lua, (_self, level, message): (mlua::Table, String, String)| {
                log_message(&plugin_id, &level, &message);
                Ok(())
            },
        )?;
        let log_mt = self.lua.create_table()?;
        log_mt.set("__call", call_log)?;
        log.set_metatable(Some(log_mt))?;

        let plugin_id = self.plugin_id.clone();
        for (name, level) in [
            ("info", "info"),
            ("warn", "warn"),
            ("warning", "warning"),
            ("error", "error"),
            ("debug", "debug"),
        ] {
            let plugin_id = plugin_id.clone();
            log.set(
                name,
                self.lua.create_function(move |_lua, message: String| {
                    log_message(&plugin_id, level, &message);
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
        runtime.current_payload = payload;
    }

    fn take_pending_emit(&self) -> Option<JsonValue> {
        self.runtime.lock().unwrap().pending_emit.take()
    }
}

fn run_exec(lua: &Lua, program: &str, args: &[String]) -> mlua::Result<LuaValue> {
    let result = StdCommand::new(program).args(args).output();
    exec_result_to_lua(lua, result)
}

fn exec_result_to_lua(
    lua: &Lua,
    result: std::io::Result<std::process::Output>,
) -> mlua::Result<LuaValue> {
    let outcome = match result {
        Ok(out) => ExecOutcome {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code(),
        },
        Err(err) => {
            tracing::debug!("backend exec failed: {}", err);
            ExecOutcome {
                success: false,
                stdout: String::new(),
                stderr: err.to_string(),
                code: None,
            }
        }
    };

    exec_outcome_to_lua(lua, outcome)
}

fn exec_outcome_to_lua(lua: &Lua, outcome: ExecOutcome) -> mlua::Result<LuaValue> {
    let table = lua.create_table()?;
    table.set("success", outcome.success)?;
    table.set("stdout", outcome.stdout)?;
    table.set("stderr", outcome.stderr)?;
    table.set("code", outcome.code)?;
    Ok(LuaValue::Table(table))
}

fn log_message(plugin_id: &str, level: &str, message: &str) {
    match level.to_ascii_lowercase().as_str() {
        "info" => tracing::info!(plugin_id = plugin_id, "{message}"),
        "warn" | "warning" => tracing::warn!(plugin_id = plugin_id, "{message}"),
        "error" => tracing::error!(plugin_id = plugin_id, "{message}"),
        "debug" => tracing::debug!(plugin_id = plugin_id, "{message}"),
        _ => tracing::warn!(
            plugin_id = plugin_id,
            "unknown log level `{level}`: {message}"
        ),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BackendScriptError {
    #[error("script error in {plugin_id}: {message}")]
    Runtime { plugin_id: String, message: String },

    #[error("backend script {plugin_id} is missing required entrypoint {name}()")]
    MissingEntrypoint { plugin_id: String, name: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Table;

    fn bundled_backend_script(path: &str) -> String {
        let script_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        std::fs::read_to_string(script_path).unwrap()
    }

    #[test]
    fn loads_poll_interval_from_script() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function init()\nmesh.service.set_poll_interval(250)\nend")
            .unwrap();
        ctx.call_init().unwrap();
        assert_eq!(ctx.poll_interval_ms(), 250);
    }

    #[test]
    fn poll_interval_below_minimum_is_clamped() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function init()\nmesh.service.set_poll_interval(10)\nend")
            .unwrap();
        ctx.call_init().unwrap();
        assert_eq!(ctx.poll_interval_ms(), 50);
    }

    #[test]
    fn poll_interval_at_minimum_is_accepted() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function init()\nmesh.service.set_poll_interval(50)\nend")
            .unwrap();
        ctx.call_init().unwrap();
        assert_eq!(ctx.poll_interval_ms(), 50);
    }

    #[test]
    fn registers_handlers_from_script() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\n\
             function on_poll()\nmesh.log.info(\"polling\")\nend\n\
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
            "function init()\nend\nfunction on_poll()\nmesh.service.emit({ available = true, percent = 65 })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(
            payload.get("available").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(65));
    }

    #[test]
    fn emit_unavailable_stores_unavailable_payload() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nmesh.service.emit_unavailable()\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
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
    fn command_handler_reads_payload_via_api() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_command_set_volume()\nlocal p = mesh.service.payload()\nmesh.service.emit({ percent = p.percent })\nend",
        )
        .unwrap();
        let result = ctx.run_command("set-volume", &serde_json::json!({ "percent": 50 }));
        let payload = result.unwrap().unwrap();
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(50));
    }

    #[test]
    fn shell_theme_backend_accepts_current_payload() {
        let script = bundled_backend_script(
            "../../../../packages/plugins/backend/core/shell-theme/src/main.luau",
        );
        let mut ctx = BackendScriptContext::new("@mesh/shell-theme");
        ctx.load_script(&script).unwrap();
        ctx.call_init().unwrap();

        let payload = ctx
            .run_command(
                "set-current",
                &serde_json::json!({ "current": "mesh-default-light", "is_dark": false }),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            payload.get("current").and_then(|v| v.as_str()),
            Some("mesh-default-light")
        );
        assert_eq!(
            payload.get("is_dark").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn bundled_backend_scripts_expose_required_host_api_surface() {
        for (plugin_id, path) in [
            (
                "@mesh/pipewire-audio",
                "../../../../packages/plugins/backend/core/pipewire-audio/src/main.luau",
            ),
            (
                "@mesh/pulseaudio-audio",
                "../../../../packages/plugins/backend/core/pulseaudio-audio/src/main.luau",
            ),
            (
                "@mesh/networkmanager",
                "../../../../packages/plugins/backend/core/networkmanager-network/src/main.luau",
            ),
            (
                "@mesh/upower",
                "../../../../packages/plugins/backend/core/upower-power/src/main.luau",
            ),
            (
                "@mesh/shell-theme",
                "../../../../packages/plugins/backend/core/shell-theme/src/main.luau",
            ),
        ] {
            let script = bundled_backend_script(path);
            let mut ctx = BackendScriptContext::new_with_settings(
                plugin_id,
                serde_json::json!({ "demo": true }),
            );
            ctx.load_script(&script).unwrap();
            ctx.call_init().unwrap();

            let mesh = ctx.lua.globals().get::<Table>("mesh").unwrap();
            let service = mesh.get::<Table>("service").unwrap();
            let log = mesh.get::<Table>("log").unwrap();

            assert!(
                mesh.get::<Function>("exec").is_ok(),
                "{plugin_id} missing mesh.exec"
            );
            assert!(
                mesh.get::<Function>("exec_shell").is_err(),
                "{plugin_id} unexpectedly exposes mesh.exec_shell"
            );
            assert!(
                mesh.get::<Function>("config").is_ok(),
                "{plugin_id} missing mesh.config"
            );
            assert!(
                service.get::<Function>("emit").is_ok(),
                "{plugin_id} missing mesh.service.emit"
            );
            assert!(
                service.get::<Function>("emit_unavailable").is_ok(),
                "{plugin_id} missing mesh.service.emit_unavailable"
            );
            assert!(
                service.get::<Function>("set_poll_interval").is_ok(),
                "{plugin_id} missing mesh.service.set_poll_interval"
            );
            assert!(
                service.get::<Function>("payload").is_ok(),
                "{plugin_id} missing mesh.service.payload"
            );
            assert!(
                log.get::<Function>("debug").is_ok(),
                "{plugin_id} missing mesh.log.debug"
            );
            assert!(
                log.get::<Function>("info").is_ok(),
                "{plugin_id} missing mesh.log.info"
            );
            assert!(
                log.get::<Function>("warn").is_ok(),
                "{plugin_id} missing mesh.log.warn"
            );
            assert!(
                log.get::<Function>("error").is_ok(),
                "{plugin_id} missing mesh.log.error"
            );
        }
    }

    #[test]
    fn emit_json_accepts_explicit_string() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nlocal r = mesh.exec(\"printf\", {\"{\\\"available\\\":true,\\\"percent\\\":65}\"})\nmesh.service.emit_json(r.stdout)\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(
            payload.get("available").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(65));
    }

    #[test]
    fn emit_json_accepts_lua_table() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nmesh.service.emit_json({ available = true, percent = 65 })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(
            payload.get("available").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(65));
    }

    #[test]
    fn emit_json_nil_uses_current_command_payload() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_command_echo_current()\nmesh.service.emit_json(nil)\nend",
        )
        .unwrap();
        let payload = ctx.run_command(
            "echo-current",
            &serde_json::json!({ "percent": 55, "muted": false }),
        );
        let payload = payload.unwrap().unwrap();
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(55));
        assert_eq!(payload.get("muted").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn emit_json_rejects_invalid_json_string() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function init()\nend").unwrap();

        let globals = ctx.lua.globals();
        let mesh = globals.get::<mlua::Table>("mesh").unwrap();
        let service = mesh.get::<mlua::Table>("service").unwrap();
        let emit_json = service.get::<Function>("emit_json").unwrap();

        let err = emit_json.call::<()>("{not-json}").unwrap_err();
        assert!(err.to_string().contains("key must be a string"));
    }

    #[test]
    fn bad_emit_json_does_not_emit_stale_state() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nmesh.service.emit({ ok = true })\nend\nfunction on_command_bad_emit_json()\nmesh.service.emit_json('{not-json}')\nend",
        )
        .unwrap();
        let first_payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(
            first_payload.get("ok").and_then(|v| v.as_bool()),
            Some(true)
        );

        let bad_payload = ctx.run_command("bad-emit-json", &serde_json::json!({}));
        assert!(bad_payload.is_err());
    }

    #[test]
    fn emit_resolves_lua_table_payloads() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nmesh.service.emit({ percent = 42, muted = false })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(payload.get("muted").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn command_handler_can_read_payload_table() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_command_set_volume()\nlocal payload = mesh.service.payload()\nmesh.service.emit({ percent = payload.percent })\nend",
        )
        .unwrap();
        let payload = ctx.run_command("set-volume", &serde_json::json!({ "percent": 55 }));
        assert_eq!(
            payload
                .unwrap()
                .unwrap()
                .get("percent")
                .and_then(|v| v.as_u64()),
            Some(55)
        );
    }

    #[test]
    fn command_handler_can_use_direct_function_name() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction set_volume()\nmesh.service.emit({ ok = true })\nend",
        )
        .unwrap();
        let payload = ctx.run_command("set-volume", &serde_json::json!({}));
        assert_eq!(
            payload
                .unwrap()
                .unwrap()
                .get("ok")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn exec_returns_structured_result() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"printf\", {\"hello\"})\nmesh.service.emit({ ok = result.success, stdout = result.stdout })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            payload.get("stdout").and_then(|v| v.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn exec_accepts_program_and_args() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"printf\", {\"hello\"})\nmesh.service.emit({ success = result.success, stdout = result.stdout, code = result.code })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            payload.get("stdout").and_then(|v| v.as_str()),
            Some("hello")
        );
        assert_eq!(payload.get("code").and_then(|v| v.as_i64()), Some(0));
    }

    #[test]
    fn exec_rejects_single_string_command_form() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nlocal ok = pcall(function()\nmesh.exec(\"printf hello\")\nend)\nmesh.service.emit({ rejected = not ok })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(
            payload.get("rejected").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn exec_missing_program_returns_failure_table() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"__mesh_missing_command_for_test__\", {})\nmesh.service.emit({ success = result.success, stdout = result.stdout, stderr = result.stderr, code = result.code })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(
            payload.get("success").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(payload.get("stdout").and_then(|v| v.as_str()), Some(""));
        assert!(
            payload
                .get("stderr")
                .and_then(|v| v.as_str())
                .is_some_and(|stderr| !stderr.is_empty())
        );
        assert!(payload.get("code").is_none());
    }

    #[test]
    fn exec_nonzero_exit_returns_failure_table() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"sh\", {\"-c\", \"printf err >&2; exit 7\"})\nmesh.service.emit({ success = result.success, stdout = result.stdout, stderr = result.stderr, code = result.code })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(
            payload.get("success").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(payload.get("stdout").and_then(|v| v.as_str()), Some(""));
        assert_eq!(payload.get("stderr").and_then(|v| v.as_str()), Some("err"));
        assert_eq!(payload.get("code").and_then(|v| v.as_i64()), Some(7));
    }

    #[test]
    fn config_returns_backend_settings() {
        let mut ctx = BackendScriptContext::new_with_settings(
            "@test/backend",
            serde_json::json!({
                "enabled": true,
                "nested": {
                    "name": "demo",
                    "features": ["audio", "network"],
                },
            }),
        );
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nlocal cfg = mesh.config()\nmesh.service.emit({ enabled = cfg.enabled, name = cfg.nested.name, first_feature = cfg.nested.features[1] })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(payload.get("enabled").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(payload.get("name").and_then(|v| v.as_str()), Some("demo"));
        assert_eq!(
            payload.get("first_feature").and_then(|v| v.as_str()),
            Some("audio")
        );
    }

    #[test]
    fn log_level_function_and_aliases_are_callable() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nmesh.log(\"debug\", \"polling\")\nmesh.log.debug(\"polling\")\nmesh.log(\"info\", \"polling\")\nmesh.log.info(\"polling\")\nmesh.log(\"warn\", \"polling\")\nmesh.log.warn(\"polling\")\nmesh.log(\"error\", \"polling\")\nmesh.log.error(\"polling\")\nmesh.log(\"warning\", \"polling\")\nmesh.service.emit({ ok = true })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn invalid_log_level_is_non_fatal() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nmesh.log(\"trace\", \"not public\")\nmesh.service.emit({ ok = true })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn bad_emit_payload_does_not_emit_stale_state() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nmesh.service.emit({ ok = true })\nend\nfunction on_command_bad_emit()\nmesh.service.emit(function() end)\nend",
        )
        .unwrap();
        let first_payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(
            first_payload.get("ok").and_then(|v| v.as_bool()),
            Some(true)
        );

        let bad_payload = ctx.run_command("bad-emit", &serde_json::json!({}));
        assert!(bad_payload.is_err());
    }

    #[test]
    fn capabilities_are_visible_to_backend_scripts() {
        let mut ctx = BackendScriptContext::new_with_capabilities(
            "@test/backend",
            vec!["service.network.control".to_string()],
        );
        ctx.load_script(
            "function init()\nend\nfunction on_poll()\nmesh.service.emit({ allowed = mesh.service.has_capability(\"service.network.control\") })\nend",
        )
        .unwrap();
        let payload = ctx.run_poll().unwrap().unwrap();
        assert_eq!(payload.get("allowed").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn rejects_backend_script_without_init_entrypoint() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function on_poll()\nend").unwrap();
        let err = ctx.call_init().unwrap_err();
        assert!(matches!(err, BackendScriptError::MissingEntrypoint { .. }));
    }

    #[test]
    fn backend_poll_handler_error_returns_runtime_error() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script("function init()\nend\nfunction on_poll()\nerror(\"boom\")\nend")
            .unwrap();

        let err = ctx.run_poll().unwrap_err();
        assert!(matches!(err, BackendScriptError::Runtime { .. }));
        assert!(err.to_string().contains("boom"));
    }

    #[test]
    fn backend_command_handler_error_returns_runtime_error() {
        let mut ctx = BackendScriptContext::new("@test/backend");
        ctx.load_script(
            "function init()\nend\nfunction on_command_fail()\nerror(\"command boom\")\nend",
        )
        .unwrap();

        let err = ctx.run_command("fail", &serde_json::json!({})).unwrap_err();
        assert!(matches!(err, BackendScriptError::Runtime { .. }));
        assert!(err.to_string().contains("command boom"));
    }
}
