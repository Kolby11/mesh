use super::*;
use mlua::{Function, Table};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_storage_root(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "mesh-backend-storage-{name}-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&root);
    root
}

fn bundled_backend_script(path: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let script_path = root.join(path);
    let script_path = if script_path.exists() {
        script_path
    } else if let Some(suffix) = path.strip_prefix("../../../../packages/modules/backend/core/") {
        root.join("../../../../modules/backend").join(suffix)
    } else {
        script_path
    };
    std::fs::read_to_string(script_path).unwrap()
}

#[test]
fn loads_poll_interval_from_script() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script("function start()\nmesh.service.set_poll_interval(250)\nend")
        .unwrap();
    ctx.call_init().unwrap();
    assert_eq!(ctx.poll_interval_ms(), 250);
}

#[test]
fn start_receives_backend_self_meta() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "state = {}\n\
         function start(self)\n\
           state = { module_id = self.meta.module_id, provider_id = self.meta.provider_id, kind = self.meta.kind, diagnostics_id = self.meta.diagnostics_id }\n\
         end",
    )
    .unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    assert_eq!(
        payload.get("module_id").and_then(|v| v.as_str()),
        Some("@test/backend")
    );
    assert_eq!(
        payload.get("provider_id").and_then(|v| v.as_str()),
        Some("@test/backend")
    );
    assert_eq!(
        payload.get("kind").and_then(|v| v.as_str()),
        Some("backend")
    );
    assert_eq!(
        payload.get("diagnostics_id").and_then(|v| v.as_str()),
        Some("@test/backend")
    );
}

#[test]
fn start_receives_backend_self_storage() {
    let mut ctx = BackendScriptContext::new("@test/storage-backend");
    ctx.load_script(
        "state = {}\n\
         function start(self)\n\
           self.storage.mode = \"compact\"\n\
           self.storage.settings = { visible = true, order = { \"a\", \"b\" } }\n\
           self.storage.removed = true\n\
           self.storage.removed = nil\n\
           self.storage.invalid = function() return true end\n\
           local snapshot = self.storage:snapshot()\n\
           state = { mode = self.storage.mode, missing = self.storage.removed == nil, visible = snapshot.settings.visible }\n\
         end",
    )
    .unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    assert_eq!(
        payload.get("mode").and_then(|v| v.as_str()),
        Some("compact")
    );
    assert_eq!(payload.get("missing").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(payload.get("visible").and_then(|v| v.as_bool()), Some(true));

    let diagnostics = ctx.drain_storage_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("unsupported storage value"));
}

#[test]
fn backend_storage_flushes_on_stop_and_loads_before_start() {
    let root = temp_storage_root("backend-flush");
    let mut writer = BackendScriptContext::new_with_storage_root("@test/storage-lifecycle", &root);
    writer
        .load_script(
            r#"
state = {}

function start(self)
    self.storage.counter = 1
end

function on_poll(self)
    self.storage.counter = 2
end

function stop(self)
    self.storage.counter = 3
end
"#,
        )
        .unwrap();

    writer.call_init().unwrap();
    writer.run_poll().unwrap();

    let mut before_flush =
        BackendScriptContext::new_with_storage_root("@test/storage-lifecycle", &root);
    before_flush
        .load_script(
            "state = {}\nfunction start(self)\nstate = { loaded = self.storage.counter }\nend",
        )
        .unwrap();
    let before_payload = before_flush.call_init().unwrap().unwrap();
    assert_eq!(before_payload.get("loaded"), None);

    writer.call_stop().unwrap();

    let mut reader = BackendScriptContext::new_with_storage_root("@test/storage-lifecycle", &root);
    reader
        .load_script(
            "state = {}\nfunction start(self)\nstate = { loaded = self.storage.counter }\nend",
        )
        .unwrap();
    let payload = reader.call_init().unwrap().unwrap();
    assert_eq!(
        payload.get("loaded").and_then(|value| value.as_i64()),
        Some(3)
    );
}

#[test]
fn backend_storage_persistence_failure_is_diagnostic_and_keeps_memory_state() {
    let root = temp_storage_root("backend-failure");
    std::fs::write(&root, "not a directory").unwrap();
    let mut ctx = BackendScriptContext::new_with_storage_root("@test/storage-failure", &root);
    ctx.load_script(
        r#"
state = {}

function start(self)
    self.storage.value = "kept"
end

function stop(self)
    state = { latest = self.storage.value }
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.call_stop().unwrap();
    let payload = ctx.take_service_state_snapshot().unwrap().unwrap();

    assert_eq!(
        payload.get("latest").and_then(|value| value.as_str()),
        Some("kept")
    );
    let diagnostics = ctx.drain_storage_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("storage persistence failed"))
    );
}

#[test]
fn stop_receives_backend_self_meta() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "state = {}\n\
         function start(self)\n\
           state = { stopped = false }\n\
         end\n\
         function stop(self)\n\
           state = { stopped = true, stopped_kind = self.meta.kind }\n\
         end",
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.call_stop().unwrap();
    let payload = ctx.take_service_state_snapshot().unwrap().unwrap();

    assert_eq!(payload.get("stopped").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("stopped_kind").and_then(|v| v.as_str()),
        Some("backend")
    );
}

#[test]
fn backend_public_function_inspection_excludes_lifecycle_hooks() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start(self)\nend\n\
         function stop(self)\nend\n\
         function public_method()\nreturn { ok = true }\nend",
    )
    .unwrap();

    assert_eq!(
        ctx.public_function_names(),
        vec!["public_method".to_string()]
    );
}

#[test]
fn poll_interval_below_minimum_is_clamped() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script("function start()\nmesh.service.set_poll_interval(10)\nend")
        .unwrap();
    ctx.call_init().unwrap();
    assert_eq!(ctx.poll_interval_ms(), 50);
}

#[test]
fn poll_interval_at_minimum_is_accepted() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script("function start()\nmesh.service.set_poll_interval(50)\nend")
        .unwrap();
    ctx.call_init().unwrap();
    assert_eq!(ctx.poll_interval_ms(), 50);
}

#[test]
fn registers_handlers_from_script() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\n\
         function on_poll()\nmesh.log.info(\"polling\")\nend\n\
         function on_command_volume_up()\nmesh.log.info(\"up\")\nend",
    )
    .unwrap();
    assert!(
        ctx.ensure_lua()
            .globals()
            .get::<Function>("on_poll")
            .is_ok()
    );
    assert!(
        ctx.ensure_lua()
            .globals()
            .get::<Function>("on_command_volume_up")
            .is_ok()
    );
}

#[test]
fn mesh_service_emit_remains_compatibility_state_setter() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nmesh.service.emit({ available = true, percent = 65 })\nend",
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
fn mesh_service_emit_overrides_exported_state_for_current_callback() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "state = { percent = 10 }\nfunction start()\nend\nfunction on_poll()\nmesh.service.emit({ percent = 65 })\nend",
    )
    .unwrap();
    let payload = ctx.run_poll().unwrap().unwrap();
    assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(65));
}

#[test]
fn emit_unavailable_stores_unavailable_payload() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nmesh.service.emit_unavailable()\nend",
    )
    .unwrap();
    let payload = ctx.run_poll().unwrap().unwrap();
    assert_eq!(
        payload.get("available").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        payload.get("source_module").and_then(|v| v.as_str()),
        Some("@test/backend")
    );
}

#[test]
fn mesh_service_emit_event_buffers_typed_interface_event() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nmesh.service.emit_event(\"VolumeChanged\", { device_id = \"default\", level = 67 })\nend",
    )
    .unwrap();

    ctx.run_poll().unwrap();
    let events = ctx.drain_events();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "VolumeChanged");
    assert_eq!(
        events[0].payload,
        serde_json::json!({ "device_id": "default", "level": 67 })
    );
}

#[test]
fn backend_self_named_event_channel_fires_typed_event() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        r#"
function start(self)
end

function on_poll(self)
    self.VolumeChanged:fire({ device_id = "default", level = 72 })
end
"#,
    )
    .unwrap();

    ctx.run_poll().unwrap();
    let events = ctx.drain_events();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "VolumeChanged");
    assert_eq!(
        events[0].payload,
        serde_json::json!({ "device_id": "default", "level": 72 })
    );
}

#[test]
fn command_handler_reads_payload_via_api() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_command_set_volume()\nlocal p = mesh.service.payload()\nmesh.service.emit({ percent = p.percent })\nend",
    )
    .unwrap();
    let result = ctx.run_command("set-volume", &serde_json::json!({ "percent": 50 }));
    let payload = result.unwrap().unwrap();
    assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(50));
}

#[test]
fn backend_state_snapshot_reads_top_level_state() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "state = { available = false }\n\
         function start()\nstate = { available = true, percent = 65 }\nend",
    )
    .unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    assert_eq!(
        payload.get("available").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(65));
}

#[test]
fn backend_state_snapshot_updates_after_poll() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "state = { tick = 0 }\n\
         function start()\nend\n\
         function on_poll()\nstate = { tick = state.tick + 1 }\nend",
    )
    .unwrap();

    ctx.call_init().unwrap();
    let payload = ctx.run_poll().unwrap().unwrap();

    assert_eq!(payload.get("tick").and_then(|v| v.as_u64()), Some(1));
}

#[test]
fn backend_state_snapshot_updates_after_command() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "state = { percent = 0 }\n\
         function start()\nend\n\
         function on_command_set_volume()\n\
           local payload = mesh.service.payload()\n\
           state = { percent = payload.percent }\n\
         end",
    )
    .unwrap();

    ctx.call_init().unwrap();
    let payload = ctx
        .run_command("set-volume", &serde_json::json!({ "percent": 72 }))
        .unwrap()
        .unwrap();

    assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(72));
}

#[test]
fn shell_theme_backend_accepts_current_payload() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/shell-theme/src/main.luau",
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
fn shell_theme_backend_initializes_from_configured_current_theme() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/shell-theme/src/main.luau",
    );
    let mut ctx = BackendScriptContext::new_with_settings(
        "@mesh/shell-theme",
        serde_json::json!({ "__shell": { "theme": "mesh-default-light" } }),
    );
    ctx.load_script(&script).unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

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
fn shell_theme_backend_preserves_shell_authored_dark_state() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/shell-theme/src/main.luau",
    );
    let mut ctx = BackendScriptContext::new("@mesh/shell-theme");
    ctx.load_script(&script).unwrap();
    ctx.call_init().unwrap();

    let payload = ctx
        .run_command(
            "set-current",
            &serde_json::json!({
                "current": "custom-dark-theme",
                "theme_id": "custom-dark-theme",
                "is_dark": true,
            }),
        )
        .unwrap()
        .unwrap();

    assert_eq!(
        payload.get("current").and_then(|v| v.as_str()),
        Some("custom-dark-theme")
    );
    assert_eq!(payload.get("is_dark").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn bundled_audio_provider_exports_state() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/pipewire-audio/src/main.luau",
    );
    let mut ctx = BackendScriptContext::new("@mesh/pipewire-audio");
    ctx.load_script(&script).unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    assert_eq!(
        payload.get("available").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(payload.get("percent").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(payload.get("muted").and_then(|v| v.as_bool()), Some(false));
    assert!(
        payload.get("source_module").is_none(),
        "provider-authored source identity must not be public state"
    );
}

#[test]
fn bundled_network_provider_exports_state() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/networkmanager-network/src/main.luau",
    );
    let mut ctx = BackendScriptContext::new("@mesh/networkmanager");
    ctx.load_script(&script).unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    assert_eq!(
        payload.get("available").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        payload.get("wifi_enabled").and_then(|v| v.as_bool()),
        Some(false)
    );
    let exported_state = ctx.ensure_lua().globals().get::<Table>("state").unwrap();
    assert!(exported_state.get::<Table>("connections").is_ok());
    assert!(exported_state.get::<Table>("devices").is_ok());
    assert!(exported_state.get::<Table>("networks").is_ok());
    assert!(
        payload.get("source_module").is_none(),
        "provider-authored source identity must not be public state"
    );
}

#[test]
fn bundled_brightness_provider_reads_and_controls_the_backlight() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/backlight-brightness/src/main.luau",
    );
    let mut ctx = BackendScriptContext::new("@mesh/backlight-brightness");
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .load(
            r#"
exec_calls = {}
mesh.exec = function(program, args)
    exec_calls[#exec_calls + 1] = { program = program, args = args }
    if args[1] == "-m" then
        return {
            success = true,
            stdout = "amdgpu_bl2,backlight,128,50%,255\n",
            stderr = "",
            code = 0,
        }
    end
    return { success = true, stdout = "", stderr = "", code = 0 }
end
"#,
        )
        .exec()
        .unwrap();

    let initial = ctx.call_init().unwrap().unwrap();
    assert_eq!(
        initial.get("available").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(initial.get("level").and_then(|v| v.as_f64()), Some(50.0));

    let outcome = ctx
        .run_command_with_result("increase", &serde_json::json!({ "amount": 5 }))
        .unwrap();
    assert_eq!(
        outcome.result,
        serde_json::json!({ "ok": true, "error": "" })
    );

    let calls = ctx
        .ensure_lua()
        .globals()
        .get::<Table>("exec_calls")
        .unwrap();
    assert_eq!(calls.raw_len(), 3);
    let set_call = calls.get::<Table>(2).unwrap();
    assert_eq!(set_call.get::<String>("program").unwrap(), "brightnessctl");
    let args = set_call.get::<Table>("args").unwrap();
    assert_eq!(args.get::<String>(1).unwrap(), "set");
    assert_eq!(args.get::<String>(2).unwrap(), "5.0%+");
}

#[test]
fn hyprland_workspace_command_waits_for_event_state_instead_of_rereading() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/hyprland-wm/src/main.luau",
    );
    let mut ctx = BackendScriptContext::new("@mesh/hyprland-wm");
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .load(
            r#"
exec_calls = {}
mesh.exec = function(program, args)
    exec_calls[#exec_calls + 1] = { program = program, args = args }
    return { success = true, stdout = "", stderr = "" }
end
"#,
        )
        .exec()
        .unwrap();

    let outcome = ctx
        .run_command_with_result("focus-workspace", &serde_json::json!({ "id": 2 }))
        .unwrap();

    assert_eq!(
        outcome.result,
        serde_json::json!({ "ok": true, "error": "" })
    );
    let calls = ctx
        .ensure_lua()
        .globals()
        .get::<Table>("exec_calls")
        .unwrap();
    assert_eq!(
        calls.raw_len(),
        1,
        "workspace dispatch must not synchronously spawn three state queries"
    );
    let call = calls.get::<Table>(1).unwrap();
    assert_eq!(call.get::<String>("program").unwrap(), "hyprctl");
    let args = call.get::<Table>("args").unwrap();
    assert_eq!(args.get::<String>(1).unwrap(), "dispatch");
    assert_eq!(
        args.get::<String>(2).unwrap(),
        "hl.dsp.focus({ workspace = \"2\" })"
    );
    assert_eq!(args.raw_len(), 2);
    assert_eq!(
        outcome
            .state
            .as_ref()
            .and_then(|state| state.get("active_workspace"))
            .and_then(|workspace| workspace.as_u64()),
        Some(1),
        "the event stream remains responsible for publishing confirmed state"
    );
}

#[test]
fn bundled_backend_scripts_expose_required_host_api_surface() {
    for (module_id, path) in [
        (
            "@mesh/pipewire-audio",
            "../../../../packages/modules/backend/core/pipewire-audio/src/main.luau",
        ),
        (
            "@mesh/pulseaudio-audio",
            "../../../../packages/modules/backend/core/pulseaudio-audio/src/main.luau",
        ),
        (
            "@mesh/networkmanager",
            "../../../../packages/modules/backend/core/networkmanager-network/src/main.luau",
        ),
        (
            concat!("@mesh/", "upo", "wer"),
            concat!(
                "../../../../packages/modules/backend/core/",
                "upo",
                "wer-power/src/main.luau"
            ),
        ),
        (
            "@mesh/backlight-brightness",
            "../../../../packages/modules/backend/core/backlight-brightness/src/main.luau",
        ),
        (
            "@mesh/shell-theme",
            "../../../../packages/modules/backend/core/shell-theme/src/main.luau",
        ),
    ] {
        let script = bundled_backend_script(path);
        let mut ctx =
            BackendScriptContext::new_with_settings(module_id, serde_json::json!({ "demo": true }));
        ctx.load_script(&script).unwrap();
        ctx.call_init().unwrap();

        let mesh = ctx.ensure_lua().globals().get::<Table>("mesh").unwrap();
        let service = mesh.get::<Table>("service").unwrap();
        let log = mesh.get::<Table>("log").unwrap();

        assert!(
            mesh.get::<Function>("exec").is_ok(),
            "{module_id} missing mesh.exec"
        );
        assert!(
            mesh.get::<Function>("exec_shell").is_err(),
            "{module_id} unexpectedly exposes mesh.exec_shell"
        );
        assert!(
            mesh.get::<Function>("config").is_ok(),
            "{module_id} missing mesh.config"
        );
        assert!(
            service.get::<Function>("emit").is_ok(),
            "{module_id} missing mesh.service.emit"
        );
        assert!(
            service.get::<Function>("emit_unavailable").is_ok(),
            "{module_id} missing mesh.service.emit_unavailable"
        );
        assert!(
            service.get::<Function>("set_poll_interval").is_ok(),
            "{module_id} missing mesh.service.set_poll_interval"
        );
        assert!(
            service.get::<Function>("payload").is_ok(),
            "{module_id} missing mesh.service.payload"
        );
        assert!(
            log.get::<Function>("debug").is_ok(),
            "{module_id} missing mesh.log.debug"
        );
        assert!(
            log.get::<Function>("info").is_ok(),
            "{module_id} missing mesh.log.info"
        );
        assert!(
            log.get::<Function>("warn").is_ok(),
            "{module_id} missing mesh.log.warn"
        );
        assert!(
            log.get::<Function>("error").is_ok(),
            "{module_id} missing mesh.log.error"
        );
    }
}

#[test]
fn bundled_pulseaudio_backend_does_not_restore_high_frequency_exec_polling() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/pulseaudio-audio/src/main.luau",
    );
    assert!(
        script.contains("mesh.exec_stream(\"pactl\", { \"subscribe\" })"),
        "PulseAudio should consume pactl's event stream instead of polling for every change"
    );

    let mut ctx = BackendScriptContext::new("@mesh/pulseaudio-audio");
    ctx.load_script(&script).unwrap();
    ctx.call_init().unwrap();

    assert!(
        ctx.poll_interval_ms() >= 250,
        "PulseAudio fallback polling must stay at or above 250ms to avoid spawning two pactl processes every 100ms"
    );
}

#[test]
fn emit_json_accepts_explicit_string() {
    let mut ctx =
        BackendScriptContext::new_with_capabilities("@test/backend", ["exec.printf".into()]);
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal r = mesh.exec(\"printf\", {\"{\\\"available\\\":true,\\\"percent\\\":65}\"})\nmesh.service.emit_json(r.stdout)\nend",
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
        "function start()\nend\nfunction on_poll()\nmesh.service.emit_json({ available = true, percent = 65 })\nend",
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
        "function start()\nend\nfunction on_command_echo_current()\nmesh.service.emit_json(nil)\nend",
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
    ctx.load_script("function start()\nend").unwrap();

    let globals = ctx.ensure_lua().globals();
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
        "function start()\nend\nfunction on_poll()\nmesh.service.emit({ ok = true })\nend\nfunction on_command_bad_emit_json()\nmesh.service.emit_json('{not-json}')\nend",
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
        "function start()\nend\nfunction on_poll()\nmesh.service.emit({ percent = 42, muted = false })\nend",
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
        "function start()\nend\nfunction on_command_set_volume()\nlocal payload = mesh.service.payload()\nmesh.service.emit({ percent = payload.percent })\nend",
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
        "function start()\nend\nfunction set_volume()\nmesh.service.emit({ ok = true })\nend",
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
fn backend_command_handler_return_table_becomes_result() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "state = { percent = 0 }\n\
         function start()\nend\n\
         function on_command_set_volume()\n\
           local payload = mesh.service.payload()\n\
           state = { percent = payload.percent }\n\
           return { ok = true, applied = payload.percent }\n\
         end",
    )
    .unwrap();

    let outcome = ctx
        .run_command_with_result("set-volume", &serde_json::json!({ "percent": 67 }))
        .unwrap();

    assert_eq!(
        outcome.result.get("ok").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        outcome.result.get("applied").and_then(|v| v.as_u64()),
        Some(67)
    );
    assert_eq!(
        outcome
            .state
            .as_ref()
            .and_then(|state| state.get("percent"))
            .and_then(|v| v.as_u64()),
        Some(67)
    );
    assert!(outcome.error.is_none());
}

#[test]
fn backend_command_handler_nil_defaults_to_ok_result() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script("function start()\nend\nfunction on_command_ping()\nend")
        .unwrap();

    let outcome = ctx
        .run_command_with_result("ping", &serde_json::json!({}))
        .unwrap();

    assert_eq!(outcome.result, serde_json::json!({ "ok": true }));
    assert!(outcome.state.is_none());
    assert!(outcome.error.is_none());
}

#[test]
fn backend_command_handler_error_becomes_failed_result() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_command_fail()\nerror(\"command boom\")\nend",
    )
    .unwrap();

    let outcome = ctx
        .run_command_with_result("fail", &serde_json::json!({}))
        .unwrap();

    assert_eq!(
        outcome.result.get("ok").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(
        outcome
            .result
            .get("error")
            .and_then(|v| v.as_str())
            .is_some_and(|message| message.contains("command boom"))
    );
    assert!(outcome.state.is_none());
    assert!(
        outcome
            .error
            .as_deref()
            .is_some_and(|message| message.contains("command boom"))
    );
}

#[test]
fn backend_command_result_return_table_becomes_result() {
    backend_command_handler_return_table_becomes_result();
}

#[test]
fn backend_command_result_nil_defaults_to_ok_result() {
    backend_command_handler_nil_defaults_to_ok_result();
}

#[test]
fn backend_command_result_error_becomes_failed_result() {
    backend_command_handler_error_becomes_failed_result();
}

#[test]
fn exec_returns_structured_result() {
    let mut ctx =
        BackendScriptContext::new_with_capabilities("@test/backend", ["exec.printf".into()]);
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"printf\", {\"hello\"})\nmesh.service.emit({ ok = result.success, stdout = result.stdout })\nend",
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
    let mut ctx =
        BackendScriptContext::new_with_capabilities("@test/backend", ["exec.printf".into()]);
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"printf\", {\"hello\"})\nmesh.service.emit({ success = result.success, stdout = result.stdout, code = result.code })\nend",
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
    let mut ctx =
        BackendScriptContext::new_with_capabilities("@test/backend", ["exec.printf hello".into()]);
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal ok = pcall(function()\nmesh.exec(\"printf hello\")\nend)\nmesh.service.emit({ rejected = not ok })\nend",
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
    let mut ctx =
        BackendScriptContext::new_with_capabilities("@test/backend", ["exec.command".into()]);
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"__mesh_missing_command_for_test__\", {})\nmesh.service.emit({ success = result.success, stdout = result.stdout, stderr = result.stderr, code = result.code })\nend",
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
    let mut ctx = BackendScriptContext::new_with_capabilities("@test/backend", ["exec.sh".into()]);
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"sh\", {\"-c\", \"printf err >&2; exit 7\"})\nmesh.service.emit({ success = result.success, stdout = result.stdout, stderr = result.stderr, code = result.code })\nend",
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
fn exec_without_program_capability_returns_denied_result() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"printf\", {\"hello\"})\nmesh.service.emit({ success = result.success, stderr = result.stderr, code = result.code })\nend",
    )
    .unwrap();
    let payload = ctx.run_poll().unwrap().unwrap();
    assert_eq!(
        payload.get("success").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(
        payload
            .get("stderr")
            .and_then(|v| v.as_str())
            .is_some_and(|stderr| stderr.contains("exec.printf"))
    );
    assert!(payload.get("code").is_none());
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
        "function start()\nend\nfunction on_poll()\nlocal cfg = mesh.config()\nmesh.service.emit({ enabled = cfg.enabled, name = cfg.nested.name, first_feature = cfg.nested.features[1] })\nend",
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
        "function start()\nend\nfunction on_poll()\nmesh.log(\"debug\", \"polling\")\nmesh.log.debug(\"polling\")\nmesh.log(\"info\", \"polling\")\nmesh.log.info(\"polling\")\nmesh.log(\"warn\", \"polling\")\nmesh.log.warn(\"polling\")\nmesh.log(\"error\", \"polling\")\nmesh.log.error(\"polling\")\nmesh.log(\"warning\", \"polling\")\nmesh.service.emit({ ok = true })\nend",
    )
    .unwrap();
    let payload = ctx.run_poll().unwrap().unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn invalid_log_level_is_non_fatal() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nmesh.log(\"trace\", \"not public\")\nmesh.service.emit({ ok = true })\nend",
    )
    .unwrap();
    let payload = ctx.run_poll().unwrap().unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn bad_emit_payload_does_not_emit_stale_state() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nmesh.service.emit({ ok = true })\nend\nfunction on_command_bad_emit()\nmesh.service.emit(function() end)\nend",
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
        "function start()\nend\nfunction on_poll()\nmesh.service.emit({ allowed = mesh.service.has_capability(\"service.network.control\") })\nend",
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
    ctx.load_script("function start()\nend\nfunction on_poll()\nerror(\"boom\")\nend")
        .unwrap();

    let err = ctx.run_poll().unwrap_err();
    assert!(matches!(err, BackendScriptError::Runtime { .. }));
    assert!(err.to_string().contains("boom"));
}

#[test]
fn backend_command_handler_error_returns_runtime_error() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_command_fail()\nerror(\"command boom\")\nend",
    )
    .unwrap();

    let err = ctx.run_command("fail", &serde_json::json!({})).unwrap_err();
    assert!(matches!(err, BackendScriptError::Runtime { .. }));
    assert!(err.to_string().contains("command boom"));
}

#[test]
fn backend_missing_start_entrypoint_is_reported() {
    // A script without a start() function must produce MissingEntrypoint — not a generic
    // Runtime error — so the shell can emit an InitFailed event with a clear message.
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script("function on_poll()\nend").unwrap();
    let err = ctx.call_init().unwrap_err();
    assert!(
        matches!(err, BackendScriptError::MissingEntrypoint { .. }),
        "expected MissingEntrypoint, got {err:?}"
    );
    assert!(
        err.to_string().contains("start"),
        "error message should name the missing entrypoint: {err}"
    );
}

#[test]
fn backend_state_snapshot_failure_is_reported() {
    // A state global that cannot be serialized to JSON (a Lua function) triggers
    // SnapshotFailed, not a generic Runtime error, so the shell can bucket it correctly.
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\n\
           state = function() end\n\
         end",
    )
    .unwrap();
    ctx.call_init().unwrap_err(); // init sets state, snapshot taken after init call fails
    // Directly set up the state as non-serializable after init loads
    let mut ctx2 = BackendScriptContext::new("@test/backend");
    ctx2.load_script("function start()\nend").unwrap();
    ctx2.call_init().unwrap();
    // Inject a non-serializable value into state
    ctx2.ensure_lua()
        .globals()
        .set(
            "state",
            ctx2.ensure_lua().create_function(|_, ()| Ok(())).unwrap(),
        )
        .unwrap();
    let err = ctx2.take_service_state_snapshot().unwrap_err();
    assert!(
        matches!(err, BackendScriptError::SnapshotFailed { .. }),
        "expected SnapshotFailed, got {err:?}"
    );
    assert!(
        err.to_string().contains("failed to export state snapshot"),
        "error message should identify snapshot stage: {err}"
    );
}

#[test]
fn backend_command_result_conversion_failure_is_reported() {
    // A handler that returns a non-serializable Lua value (a function) triggers
    // CommandResultConversionFailed so the shell can distinguish it from handler errors.
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\n\
         function on_command_bad_result()\n\
           return function() end\n\
         end",
    )
    .unwrap();
    ctx.call_init().unwrap();
    let err = ctx
        .run_command_with_result("bad-result", &serde_json::json!({}))
        .unwrap_err();
    assert!(
        matches!(
            err,
            BackendScriptError::CommandResultConversionFailed { .. }
        ),
        "expected CommandResultConversionFailed, got {err:?}"
    );
    assert!(
        err.to_string().contains("failed to convert command result"),
        "error message should identify command-result conversion stage: {err}"
    );
}

#[test]
fn backend_command_runtime_error_becomes_failed_result() {
    // A handler that calls error() at runtime produces a CommandOutcome with ok=false
    // and a populated error field — it does NOT return Err from run_command_with_result.
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\n\
         function on_command_fail()\n\
           error(\"handler exploded\")\n\
         end",
    )
    .unwrap();
    ctx.call_init().unwrap();
    let outcome = ctx
        .run_command_with_result("fail", &serde_json::json!({}))
        .unwrap();
    assert_eq!(
        outcome.result.get("ok").and_then(|v| v.as_bool()),
        Some(false),
        "runtime error should produce a failed result"
    );
    assert!(
        outcome
            .result
            .get("error")
            .and_then(|v| v.as_str())
            .is_some_and(|message| message.contains("handler exploded")),
        "result error field should carry the handler error message"
    );
    assert!(
        outcome
            .error
            .as_deref()
            .is_some_and(|message| message.contains("handler exploded")),
        "outcome.error should carry the message for lifecycle visibility"
    );
    assert!(
        outcome.state.is_none(),
        "no state should be published on handler error"
    );
}

#[test]
fn reference_media_provider_reads_config_and_exports_state() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/reference-media/src/main.luau",
    );
    let mut ctx = BackendScriptContext::new_with_settings(
        "@mesh/reference-media",
        serde_json::json!({
            "seed_title": "Config Track",
            "seed_artist": "Config Artist",
            "seed_album": "Config Album"
        }),
    );
    ctx.load_script(&script).unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    // State must include all required media interface fields
    assert_eq!(
        payload.get("available").and_then(|v| v.as_bool()),
        Some(true),
        "reference-media must export available=true on init"
    );
    assert_eq!(
        payload.get("title").and_then(|v| v.as_str()),
        Some("Config Track"),
        "reference-media must seed title from config"
    );
    assert_eq!(
        payload.get("artist").and_then(|v| v.as_str()),
        Some("Config Artist"),
        "reference-media must seed artist from config"
    );
    assert_eq!(
        payload.get("album").and_then(|v| v.as_str()),
        Some("Config Album"),
        "reference-media must seed album from config"
    );
    assert!(
        payload.get("state").and_then(|v| v.as_str()).is_some(),
        "reference-media must export a playback state field"
    );
}

#[test]
fn reference_media_provider_command_updates_state() {
    let script = bundled_backend_script(
        "../../../../packages/modules/backend/core/reference-media/src/main.luau",
    );
    let mut ctx = BackendScriptContext::new("@mesh/reference-media");
    ctx.load_script(&script).unwrap();
    ctx.call_init().unwrap();

    // Issue play command — state should transition to playing
    let outcome = ctx
        .run_command_with_result("play", &serde_json::json!({ "player_id": "default" }))
        .unwrap();
    assert_eq!(
        outcome.result.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "play command must return ok=true"
    );
    assert!(
        outcome.error.is_none(),
        "play command must not carry an error"
    );
    let state_after_play = outcome.state.as_ref().expect("play must update state");
    assert_eq!(
        state_after_play.get("state").and_then(|v| v.as_str()),
        Some("playing"),
        "playback state must change to 'playing' after play command"
    );

    // Issue next command — track index should advance
    let next_outcome = ctx
        .run_command_with_result("next", &serde_json::json!({ "player_id": "default" }))
        .unwrap();
    assert_eq!(
        next_outcome.result.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "next command must return ok=true"
    );
    let state_after_next = next_outcome.state.as_ref().expect("next must update state");
    // The second track has a different title than the default first track
    assert_ne!(
        state_after_next.get("title").and_then(|v| v.as_str()),
        Some("Reference Track"),
        "next command must advance to a different track"
    );
}

#[test]
fn run_stream_batch_invokes_on_stream_batch_once_with_full_ordered_batch() {
    let mut ctx = BackendScriptContext::new("@test/stream-batch");
    ctx.load_script(
        "state = { calls = 0, last_program = nil, last_lines = {} }\n\
         function start() end\n\
         function on_stream_batch(self, program, lines)\n\
           state = { calls = state.calls + 1, last_program = program, last_lines = lines }\n\
         end",
    )
    .unwrap();
    ctx.call_init().unwrap();

    let lines = vec![
        "changed:".to_string(),
        "\tid: 47".to_string(),
        "\tobject.serial = \"3\"".to_string(),
    ];
    let snapshot = ctx
        .run_stream_batch("pw-mon", &lines)
        .unwrap()
        .expect("batch should produce a state snapshot");
    assert_eq!(snapshot.get("calls").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(
        snapshot.get("last_program").and_then(|v| v.as_str()),
        Some("pw-mon")
    );
    let received: Vec<String> = snapshot
        .get("last_lines")
        .and_then(|v| v.as_array())
        .expect("lines must be exported as an array")
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(received, lines);
}

#[test]
fn run_stream_batch_falls_back_to_on_stream_line_per_line() {
    let mut ctx = BackendScriptContext::new("@test/stream-line-fallback");
    ctx.load_script(
        "state = { call_count = 0, lines = {} }\n\
         function start() end\n\
         function on_stream_line(self, program, line)\n\
           state = { call_count = state.call_count + 1, lines = state.lines }\n\
           table.insert(state.lines, line)\n\
         end",
    )
    .unwrap();
    ctx.call_init().unwrap();

    let lines = vec!["changed:".to_string(), "added:".to_string()];
    let snapshot = ctx
        .run_stream_batch("pw-mon", &lines)
        .unwrap()
        .expect("legacy hook should still produce a snapshot");
    assert_eq!(snapshot.get("call_count").and_then(|v| v.as_u64()), Some(2));
    let received: Vec<String> = snapshot
        .get("lines")
        .and_then(|v| v.as_array())
        .expect("lines should be exported as an array")
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(received, lines);
}

#[test]
fn run_stream_batch_with_empty_batch_does_not_invoke_handler() {
    let mut ctx = BackendScriptContext::new("@test/stream-empty");
    ctx.load_script(
        "state = { calls = 0 }\n\
         function start() end\n\
         function on_stream_batch(self, program, lines)\n\
           state = { calls = state.calls + 1 }\n\
         end",
    )
    .unwrap();
    ctx.call_init().unwrap();

    let snapshot = ctx.run_stream_batch("pw-mon", &[]).unwrap();
    assert!(
        snapshot.is_none(),
        "empty batch should short-circuit before snapshot"
    );
}

#[test]
fn run_stream_batch_without_any_hook_is_a_noop() {
    let mut ctx = BackendScriptContext::new("@test/stream-no-hook");
    ctx.load_script(
        "state = { value = 0 }\n\
         function start() end",
    )
    .unwrap();
    ctx.call_init().unwrap();

    let snapshot = ctx
        .run_stream_batch("pw-mon", &["changed:".to_string()])
        .unwrap();
    assert!(
        snapshot.is_none(),
        "missing batch and line hooks should produce no snapshot"
    );
}
