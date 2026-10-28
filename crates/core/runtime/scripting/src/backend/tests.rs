use super::*;
use crate::RuntimeLifecycleState;
use mesh_core_service::{
    ContractCapabilities, InterfaceArgument, InterfaceContract, InterfaceEvent,
    parse_contract_version,
};
use mlua::{Function, Table};
use std::collections::HashMap;
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
    std::fs::read_to_string(script_path).unwrap()
}

fn audio_event_registry() -> BackendEventRegistry {
    BackendEventRegistry::from_contract(&InterfaceContract {
        interface: "mesh.audio".into(),
        version: parse_contract_version("1.0").unwrap(),
        state_fields: Vec::new(),
        methods: Vec::new(),
        events: vec![InterfaceEvent {
            name: "VolumeChanged".into(),
            payload: vec![
                InterfaceArgument {
                    name: "device_id".into(),
                    arg_type: "string".into(),
                },
                InterfaceArgument {
                    name: "level".into(),
                    arg_type: "int".into(),
                },
            ],
        }],
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    })
}

#[test]
fn loads_poll_interval_from_script() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    assert_eq!(
        ctx.session().lifecycle().state(),
        RuntimeLifecycleState::Created
    );
    ctx.load_script("function start()\nmesh.service.set_poll_interval(250)\nend")
        .unwrap();
    assert_eq!(
        ctx.session().lifecycle().state(),
        RuntimeLifecycleState::Loaded
    );
    ctx.call_init().unwrap();
    assert_eq!(
        ctx.session().lifecycle().state(),
        RuntimeLifecycleState::Running
    );
    assert_eq!(ctx.poll_interval_ms(), 250);
    ctx.call_stop().unwrap();
    assert_eq!(
        ctx.session().lifecycle().state(),
        RuntimeLifecycleState::Stopped
    );
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
fn backend_lifecycle_reuses_self_storage_and_event_handles() {
    let mut ctx = BackendScriptContext::new_with_storage_root(
        "@test/stable-backend-self",
        temp_storage_root("stable-self"),
    );
    ctx.load_script(
        r#"
state = { events = 0 }

function start(self)
    state.self_id = tostring(self)
    state.storage_id = tostring(self.storage)
    self.storage.value = "persisted"
    self.Changed:on(function()
        state.events = state.events + 1
    end)
end

function on_poll(self)
    state.same_self = state.self_id == tostring(self)
    state.same_storage = state.storage_id == tostring(self.storage)
    state.storage_value = self.storage.value
    self.Changed:fire({})
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    let payload = ctx.run_poll().unwrap().unwrap();

    assert_eq!(payload["same_self"], serde_json::json!(true));
    assert_eq!(payload["same_storage"], serde_json::json!(true));
    assert_eq!(payload["storage_value"], serde_json::json!("persisted"));
    assert_eq!(payload["events"], serde_json::json!(1));
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
fn stop_hook_failure_still_flushes_storage_and_is_idempotent() {
    let root = temp_storage_root("stop-hook-failure");
    let mut ctx = BackendScriptContext::new_with_storage_root("@test/stop-hook-failure", &root);
    ctx.load_script(
        r#"
function start(self)
    self.storage.stop_count = 0
end

function stop(self)
    self.storage.stop_count = self.storage.stop_count + 1
    error("stop boom")
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();

    let error = ctx.call_stop().unwrap_err();
    assert!(error.to_string().contains("stop boom"));
    // A second caller must not invoke authored cleanup again.
    ctx.call_stop().unwrap();
    drop(ctx);

    let mut reader = BackendScriptContext::new_with_storage_root("@test/stop-hook-failure", &root);
    reader
        .load_script("function start(self)\nstate = { stop_count = self.storage.stop_count }\nend")
        .unwrap();
    let payload = reader.call_init().unwrap().unwrap();
    assert_eq!(
        payload.get("stop_count").and_then(|value| value.as_u64()),
        Some(1)
    );
}

#[test]
fn top_level_exec_stream_is_rejected_before_start() {
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/load-failure",
        ["exec.argv:sh:[\"-c\",\"sleep 60\"]".to_string()],
    );
    let streams = ctx.stream_state();
    let error = ctx
        .load_script(
            "mesh.exec_stream(\"sh\", { \"-c\", \"sleep 60\" })\n\
             function start()\n\
             end",
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("backend host side effects are unavailable before start(self)")
    );
    assert_eq!(streams.active_stream_count(), 0);
}

#[test]
fn top_level_service_emit_is_rejected_before_start() {
    let mut ctx = BackendScriptContext::new("@test/load-failure");
    let error = ctx
        .load_script(
            "mesh.service.emit({ available = true })\n\
             function start()\n\
             end",
        )
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("backend host side effects are unavailable before start(self)")
    );
    assert!(ctx.take_service_state_snapshot().unwrap().is_none());
}

#[test]
fn top_level_provider_event_is_rejected_before_start() {
    let mut ctx = BackendScriptContext::new("@test/load-failure");
    ctx.set_event_registry(audio_event_registry());
    let error = ctx
        .load_script(
            "self.VolumeChanged:fire({ device_id = \"default\", level = 67 })\n\
             function start()\n\
             end",
        )
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("backend host side effects are unavailable before start(self)")
    );
    assert!(ctx.drain_events().is_empty());
}

#[test]
fn top_level_storage_write_is_rejected_before_start() {
    let root = temp_storage_root("top-level-write");
    let mut ctx = BackendScriptContext::new_with_storage_root("@test/load-failure", &root);
    let error = ctx
        .load_script(
            "self.storage.ready = true\n\
             function start(self)\n\
               state = { ready = self.storage.ready }\n\
             end",
        )
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("backend host side effects are unavailable before start(self)")
    );
    drop(ctx);

    let mut reader = BackendScriptContext::new_with_storage_root("@test/load-failure", &root);
    reader
        .load_script(
            "function start(self)\n\
               state = { ready = self.storage.ready }\n\
             end",
        )
        .unwrap();
    let state = reader.call_init().unwrap().unwrap();
    assert!(state.get("ready").is_none());
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
            .unwrap()
            .globals()
            .get::<Function>("on_poll")
            .is_ok()
    );
    assert!(
        ctx.ensure_lua()
            .unwrap()
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
fn provider_self_event_handle_buffers_typed_interface_event() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.set_event_registry(audio_event_registry());
    ctx.load_script(
        "function start()\nend\nfunction on_poll(self)\nself.VolumeChanged:fire({ device_id = \"default\", level = 67 })\nend",
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
fn backend_event_boundary_rejects_unknown_and_malformed_events_before_queueing() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.set_event_registry(audio_event_registry());
    ctx.load_script("function start()\nend\nfunction on_poll(self)\nself.Unknown:fire({})\nend")
        .unwrap();
    assert!(ctx.run_poll().is_err());
    assert!(ctx.drain_events().is_empty());

    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.set_event_registry(audio_event_registry());
    ctx.load_script(
        "function start()\nend\nfunction on_poll(self)\nself.VolumeChanged:fire({ level = \"loud\", device_id = \"default\" })\nend",
    )
    .unwrap();
    assert!(ctx.run_poll().is_err());
    assert!(ctx.drain_events().is_empty());
}

#[test]
fn string_based_provider_event_publication_is_not_exposed() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nmesh.service.emit_event(\"Changed\", {})\nend",
    )
    .unwrap();

    assert!(ctx.run_poll().is_err());
    assert!(ctx.drain_events().is_empty());
}

#[test]
fn backend_self_named_event_channel_fires_typed_event() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.set_event_registry(audio_event_registry());
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
fn backend_self_event_unsubscribing_first_keeps_later_subscriber() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        r#"
state = { first = 0, second = 0 }

function start()
end

function on_poll(self)
    local unsubscribe = self.Changed:on(function()
        state.first = state.first + 1
    end)
    self.Changed:on(function()
        state.second = state.second + 1
    end)
    unsubscribe()
    self.Changed:fire({})
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    let state = ctx.run_poll().unwrap().unwrap();

    assert_eq!(state["first"], serde_json::json!(0));
    assert_eq!(state["second"], serde_json::json!(1));
}

#[test]
fn backend_self_event_callback_failure_does_not_suppress_later_subscriber() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        r#"
state = { second = 0 }

function start()
end

function on_poll(self)
    self.Changed:on(function()
        error("first subscriber failed")
    end)
    self.Changed:on(function()
        state.second = state.second + 1
    end)
    self.Changed:fire({})
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    let state = ctx.run_poll().unwrap().unwrap();
    let events = ctx.drain_events();

    assert_eq!(state["second"], serde_json::json!(1));
    assert_eq!(events.len(), 1);
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
fn backend_output_accounting_survives_repeated_snapshot_exports() {
    let mut ctx = BackendScriptContext::new("@test/snapshot-accounting");
    ctx.load_script(
        r#"
state = { data = string.rep("x", 60000) }
function start() end
function on_poll() end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();

    for iteration in 0..160 {
        let state = ctx.take_service_state_snapshot().unwrap().unwrap();
        assert_eq!(state["data"].as_str().unwrap().len(), 60000);
        ctx.run_poll()
            .unwrap_or_else(|error| panic!("poll {iteration}: {error}"));
    }
}

#[test]
fn backend_output_accounting_survives_repeated_task_completions() {
    let mut ctx = BackendScriptContext::new("@test/task-output-accounting");
    ctx.load_script(
        r#"
state = { data = string.rep("x", 60000) }
function start() end
function on_command_read()
    task.sleep(0)
    return { data = state.data }
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();

    for iteration in 0..160 {
        assert!(matches!(
            ctx.run_command_with_result("read", &serde_json::json!({}))
                .unwrap(),
            BackendCommandRun::Suspended { .. }
        ));
        let run = ctx.run_due_tasks(std::time::Instant::now());
        assert!(
            run.errors.is_empty(),
            "iteration {iteration}: {:?}",
            run.errors
        );
        assert_eq!(run.commands.len(), 1);
        let outcome = run.commands.into_iter().next().unwrap().1.unwrap();
        assert!(outcome.error.is_none());
        assert_eq!(outcome.result["data"].as_str().unwrap().len(), 60000);
        assert!(run.state.is_some());
    }
}

#[test]
fn backend_output_accounting_includes_resumed_command_results() {
    let mut ctx = BackendScriptContext::new("@test/task-output-limit");
    ctx.load_script(
        r#"
function start() end
function on_command_read()
    task.sleep(0)
    charge_output()
    return { ok = true }
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    let resources = ctx.session().resources().clone();
    let output_limit = resources.config().output_budget as usize;
    let lua = ctx.ensure_lua().unwrap();
    lua.globals()
        .set(
            "charge_output",
            lua.create_function(move |_, ()| {
                resources
                    .reserve_output(output_limit - 1)
                    .map_err(|error| mlua::Error::external(error.to_string()))
            })
            .unwrap(),
        )
        .unwrap();
    ctx.run_command_with_result("read", &serde_json::json!({}))
        .unwrap();
    let run = ctx.run_due_tasks(std::time::Instant::now());
    assert!(run.errors.is_empty(), "{:?}", run.errors);
    let error = run.commands.into_iter().next().unwrap().1.unwrap_err();
    assert!(matches!(
        error,
        BackendScriptError::CommandResultConversionFailed { .. }
    ));
    assert!(error.to_string().contains("output budget exceeded"));
    assert!(run.state.is_none());
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
fn blob_handles_in_state_must_be_registered_and_live_with_the_provider() {
    use mesh_core_resources::blob::lookup_blob;
    let mut ctx = BackendScriptContext::new("@test/blobs");
    ctx.load_script(
        r#"
        function start()
            local icon = mesh.blob.put({ 255, 0, 0, 255 }, "image/x-mesh-rgba8", { width = 1, height = 1 })
            state = { icon = icon, again = mesh.blob.put(buffer.fromstring("\255\0\0\255"), "image/x-mesh-rgba8", { width = 1, height = 1 }) }
        end
        function on_poll()
            state = { icon = "mesh-blob:sha256-" .. string.rep("0", 64) }
        end
        "#,
    )
    .unwrap();
    let state = ctx.call_init().unwrap().unwrap();
    let handle = state["icon"].as_str().unwrap().to_owned();
    assert_eq!(state["again"], state["icon"], "content-addressed");
    assert_eq!(lookup_blob(&handle).unwrap().bytes().as_ref(), [255, 0, 0, 255]);

    let error = ctx.run_poll().unwrap_err().to_string();
    assert!(error.contains("has not registered"), "{error}");
    assert!(lookup_blob(&handle).is_some(), "a rejected snapshot changes nothing");

    drop(ctx);
    assert!(lookup_blob(&handle).is_none(), "provider teardown releases its blobs");
}

#[test]
fn blob_put_retypes_host_registered_bytes_without_reading_them() {
    use mesh_core_resources::blob::{BlobFormat, RawPixelLayout, lookup_blob};
    let mut ctx = BackendScriptContext::new("@test/blob-records");
    ctx.load_script(
        r#"
        function start() end
        function on_command_show()
            local record = mesh.service.payload().record
            state = { image = mesh.blob.put(record, "image/x-mesh-rgb8", { width = 2, height = 2, stride = 8 }) }
        end
        "#,
    )
    .unwrap();
    let bytes = ctx
        .blob_owner()
        .put(BlobFormat::Bytes, (0..14).collect())
        .unwrap();
    let state = ctx
        .run_command(
            "show",
            &serde_json::json!({ "record": { "blob": &*bytes, "len": 14 } }),
        )
        .unwrap()
        .unwrap();
    let image = lookup_blob(state["image"].as_str().unwrap()).unwrap();
    assert_eq!(
        image.format(),
        BlobFormat::Raw {
            layout: RawPixelLayout::Rgb8,
            width: 2,
            height: 2,
            stride: 8,
        }
    );
    assert_eq!(image.bytes().len(), 14);
}

#[test]
fn blob_put_rejects_bad_bytes_and_formats() {
    let mut ctx = BackendScriptContext::new("@test/blob-errors");
    ctx.load_script(
        r#"
        function start()
            local results = {}
            for _, case in {
                { { 256 }, "image/png" },
                { "x", "image/gif" },
                { "abc", "image/x-mesh-rgb8" },
                { "ab", "image/x-mesh-rgb8", { width = 1, height = 1 } },
                { 42, "image/png" },
            } do
                local ok, message = pcall(mesh.blob.put, case[1], case[2], case[3])
                results[#results + 1] = if ok then "ok" else tostring(message)
            end
            state = { results = results }
        end
        "#,
    )
    .unwrap();
    let state = ctx.call_init().unwrap().unwrap();
    let results: Vec<_> = state["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    for (result, expected) in results.iter().zip([
        "must be an integer in 0..=255",
        "unsupported blob MIME type",
        "requires width and height",
        "needs at least 3",
        "must be a string, buffer or byte array",
    ]) {
        assert!(result.contains(expected), "{result} lacks {expected}");
    }
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
    let script = bundled_backend_script("tests/fixtures/shell-theme/src/main.luau");
    let mut ctx = BackendScriptContext::new("@mesh/shell-theme");
    ctx.load_script(&script).unwrap();
    ctx.call_init().unwrap();

    let payload = ctx
        .run_command(
            "set-current",
            &serde_json::json!({
                "current": "mesh-default-light",
                "is_dark": false,
                "themes": [
                    { "id": "mesh-default-dark", "label": "MESH Default Dark" },
                    { "id": "mesh-default-light", "label": "MESH Default Light" },
                    { "id": "gruvbox-dark", "label": "Gruvbox Dark" }
                ],
                "available": [
                    "mesh-default-dark",
                    "mesh-default-light",
                    "gruvbox-dark"
                ]
            }),
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
    assert_eq!(
        payload
            .get("themes")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(3)
    );
    assert_eq!(
        payload
            .get("available")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(3)
    );
}

#[test]
fn shell_theme_backend_initializes_from_configured_current_theme() {
    let script = bundled_backend_script("tests/fixtures/shell-theme/src/main.luau");
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
fn shell_theme_backend_treats_tokyo_night_as_dark() {
    let script = bundled_backend_script("tests/fixtures/shell-theme/src/main.luau");
    let mut ctx = BackendScriptContext::new_with_settings(
        "@mesh/shell-theme",
        serde_json::json!({
            "__shell": {
                "theme": "tokyo-night",
                "theme_color_scheme": "dark"
            }
        }),
    );
    ctx.load_script(&script).unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    assert_eq!(
        payload.get("current").and_then(|v| v.as_str()),
        Some("tokyo-night")
    );
    assert_eq!(payload.get("is_dark").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn shell_theme_backend_does_not_infer_darkness_from_theme_id() {
    let script = bundled_backend_script("tests/fixtures/shell-theme/src/main.luau");
    let mut ctx = BackendScriptContext::new_with_settings(
        "@mesh/shell-theme",
        serde_json::json!({
            "__shell": {
                "theme": "custom-dark-looking-name",
                "theme_color_scheme": "light"
            }
        }),
    );
    ctx.load_script(&script).unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    assert_eq!(
        payload.get("current").and_then(|v| v.as_str()),
        Some("custom-dark-looking-name")
    );
    assert_eq!(
        payload.get("is_dark").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn shell_theme_backend_preserves_shell_authored_dark_state() {
    let script = bundled_backend_script("tests/fixtures/shell-theme/src/main.luau");
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
    let script = bundled_backend_script("../../../../modules/backend/pipewire-audio/src/main.luau");
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
    let script = bundled_backend_script("tests/fixtures/networkmanager-network/src/main.luau");
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
    let exported_state = ctx
        .ensure_lua()
        .unwrap()
        .globals()
        .get::<Table>("state")
        .unwrap();
    assert!(exported_state.get::<Table>("connections").is_ok());
    assert!(exported_state.get::<Table>("devices").is_ok());
    assert!(exported_state.get::<Table>("networks").is_ok());
    assert!(
        payload.get("source_module").is_none(),
        "provider-authored source identity must not be public state"
    );
}

#[test]
fn bundled_device_provider_normalizes_linux_system_information() {
    let script = bundled_backend_script("../../../../modules/backend/device-info/src/main.luau");
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@mesh/device-info",
        [
            "exec.argv:printenv:[\"XDG_CURRENT_DESKTOP\"]".to_string(),
            "exec.argv:printenv:[\"DESKTOP_SESSION\"]".to_string(),
            "exec.argv:lspci:[]".to_string(),
        ],
    );
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .unwrap()
        .load(
            r#"
mesh.exec = function(program, args)
    if program == "uname" and args[1] == "-r" then
        return { success = true, stdout = "6.8.0-mesh\n" }
    elseif program == "uname" and args[1] == "-m" then
        return { success = true, stdout = "x86_64\n" }
    elseif program == "uname" and args[1] == "-s" then
        return { success = true, stdout = "Linux\n" }
    elseif program == "hostname" then
        return { success = true, stdout = "mesh-host\n" }
    elseif program == "printenv" then
        return { success = true, stdout = args[1] == "XDG_CURRENT_DESKTOP" and "GNOME\n" or "" }
    elseif program == "cat" and args[1] == "/etc/os-release" then
        return { success = true, stdout = "NAME=Ubuntu\nVERSION_ID=\"24.04\"\n" }
    elseif program == "cat" and args[1] == "/proc/cpuinfo" then
        return { success = true, stdout = "model name : MESH CPU 9000\n" }
    elseif program == "cat" and args[1] == "/proc/meminfo" then
        return { success = true, stdout = "MemTotal:       8388608 kB\n" }
    elseif program == "cat" and args[1] == "/proc/uptime" then
        return { success = true, stdout = "12345.67 67890.12\n" }
    elseif program == "lspci" then
        return { success = true, stdout = "00:02.0 VGA compatible controller: Advanced GPU\n" }
    end
    return { success = false, stdout = "" }
end
"#,
        )
        .exec()
        .unwrap();

    let payload = ctx.call_init().unwrap().unwrap();

    assert_eq!(
        payload.get("available").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        payload.get("hostname").and_then(|v| v.as_str()),
        Some("mesh-host")
    );
    assert_eq!(
        payload.get("os_name").and_then(|v| v.as_str()),
        Some("Ubuntu")
    );
    assert_eq!(
        payload.get("os_version").and_then(|v| v.as_str()),
        Some("24.04")
    );
    assert_eq!(
        payload.get("kernel").and_then(|v| v.as_str()),
        Some("6.8.0-mesh")
    );
    assert_eq!(
        payload.get("architecture").and_then(|v| v.as_str()),
        Some("x86_64")
    );
    assert_eq!(
        payload.get("desktop").and_then(|v| v.as_str()),
        Some("GNOME")
    );
    assert_eq!(
        payload.get("processor").and_then(|v| v.as_str()),
        Some("MESH CPU 9000")
    );
    assert_eq!(
        payload.get("memory_total_bytes").and_then(|v| v.as_u64()),
        Some(8 * 1024 * 1024 * 1024)
    );
    assert_eq!(
        payload.get("graphics").and_then(|v| v.as_str()),
        Some("Advanced GPU")
    );
    assert_eq!(
        payload.get("uptime_seconds").and_then(|v| v.as_u64()),
        Some(12345)
    );
    assert_eq!(ctx.poll_interval_ms(), 30000);
}

#[test]
fn bundled_brightness_provider_reads_and_controls_the_backlight() {
    let script =
        bundled_backend_script("../../../../modules/backend/backlight-brightness/src/main.luau");
    let mut ctx = BackendScriptContext::new("@mesh/backlight-brightness");
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .unwrap()
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
        .run_command_finished("increase", &serde_json::json!({ "amount": 5 }))
        .unwrap();
    assert_eq!(
        outcome.result,
        serde_json::json!({ "ok": true, "error": "" })
    );

    let calls = ctx
        .ensure_lua()
        .unwrap()
        .globals()
        .get::<Table>("exec_calls")
        .unwrap();
    assert_eq!(calls.raw_len(), 2);
    let set_call = calls.get::<Table>(2).unwrap();
    assert_eq!(set_call.get::<String>("program").unwrap(), "brightnessctl");
    let args = set_call.get::<Table>("args").unwrap();
    assert_eq!(args.get::<String>(1).unwrap(), "set");
    assert_eq!(args.get::<String>(2).unwrap(), "5.0%+");
    assert_eq!(
        outcome
            .state
            .as_ref()
            .and_then(|state| state.get("level"))
            .and_then(|level| level.as_f64()),
        Some(55.0),
        "a successful brightness write should publish optimistic state"
    );
}

#[test]
fn hyprland_workspace_command_waits_for_event_state_instead_of_rereading() {
    let script = bundled_backend_script("../../../../modules/backend/hyprland-wm/src/main.luau");
    let mut ctx = BackendScriptContext::new("@mesh/hyprland-wm");
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .unwrap()
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
        .run_command_finished("focus-workspace", &serde_json::json!({ "id": 2 }))
        .unwrap();

    assert_eq!(
        outcome.result,
        serde_json::json!({ "ok": true, "error": "" })
    );
    let calls = ctx
        .ensure_lua()
        .unwrap()
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
fn hyprland_stream_events_query_only_missing_fields_and_emit_selectively() {
    let script = bundled_backend_script("../../../../modules/backend/hyprland-wm/src/main.luau");
    let mut ctx = BackendScriptContext::new("@mesh/hyprland-wm");
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua().unwrap()
        .load(
            r#"
exec_calls = {}
mesh.service.can_exec = function(program, _args)
    return program == "nc"
end
mesh.exec_stream = function(_program, _args)
    return true
end
mesh.wayland_subscribe = function(_kind)
    return { program = "wayland:workspace" }
end
mesh.exec = function(program, args)
    exec_calls[#exec_calls + 1] = { program = program, args = args }
    if program == "sh" then
        local command = args[2] or ""
        if string.find(command, "HYPRLAND_INSTANCE_SIGNATURE", 1, true) then
            return { success = true, stdout = "test-instance", stderr = "", code = 0 }
        end
        if string.find(command, "XDG_RUNTIME_DIR", 1, true) then
            return { success = true, stdout = "/run/user/1000", stderr = "", code = 0 }
        end
        return { success = true, stdout = "ok", stderr = "", code = 0 }
    end
    if args[1] == "activeworkspace" then
        return { success = true, stdout = '{"id":3}', stderr = "", code = 0 }
    end
    if args[1] == "activewindow" then
        return { success = true, stdout = '{"class":"kitty","title":"before"}', stderr = "", code = 0 }
    end
    if args[1] == "workspaces" then
        return { success = true, stdout = '[{"id":3,"name":"3","windows":1}]', stderr = "", code = 0 }
    end
    return { success = false, stdout = "", stderr = "unexpected call", code = 1 }
end
"#,
        )
        .exec()
        .unwrap();

    ctx.call_init().unwrap();
    ctx.run_stream_batch(
        "wayland:workspace",
        &[
            r#"{"event":"added","id":7}"#,
            r#"{"event":"name","id":7,"value":"3"}"#,
            r#"{"event":"coordinates","id":7,"value":[3]}"#,
            r#"{"event":"state","id":7,"value":1}"#,
            r#"{"event":"done"}"#,
        ]
        .map(String::from),
    )
    .unwrap();
    ctx.drain_events();
    ctx.ensure_lua()
        .unwrap()
        .load("exec_calls = {}")
        .exec()
        .unwrap();

    let title_state = ctx
        .run_stream_batch("nc", &["activewindow>>kitty,⠸ mesh, build".to_string()])
        .unwrap()
        .unwrap();
    let calls = ctx
        .ensure_lua()
        .unwrap()
        .globals()
        .get::<Table>("exec_calls")
        .unwrap();
    assert_eq!(
        calls.raw_len(),
        0,
        "activewindow payload must not spawn state queries"
    );
    assert_eq!(
        title_state["active_window"]["title"],
        serde_json::json!("⠸ mesh, build")
    );
    let events = ctx.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "WindowChanged");

    let workspace_state = ctx
        .run_stream_batch("nc", &["workspacev2>>4,code".to_string()])
        .unwrap()
        .unwrap();
    assert_eq!(workspace_state["active_workspace"], serde_json::json!(4));
    assert_eq!(calls.raw_len(), 0);
    let events = ctx.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "WorkspaceChanged");

    let rapid_state = ctx
        .run_stream_batch(
            "nc",
            &[
                "workspacev2>>5,work".to_string(),
                "workspacev2>>2,code".to_string(),
                "workspacev2>>3,chat".to_string(),
            ],
        )
        .unwrap()
        .unwrap();
    assert_eq!(rapid_state["active_workspace"], serde_json::json!(3));
    let rapid_events = ctx.drain_events();
    assert_eq!(rapid_events.len(), 3);
    assert_eq!(
        rapid_events
            .iter()
            .map(|event| event.payload["id"].clone())
            .collect::<Vec<_>>(),
        vec![
            serde_json::json!(5),
            serde_json::json!(2),
            serde_json::json!(3)
        ]
    );

    let catalog_state = ctx
        .run_stream_batch(
            "nc",
            &["openwindow>>abc,4,kitty,another window".to_string()],
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        calls.raw_len(),
        0,
        "workspace lists come from ext-workspace or the request socket, never hyprctl"
    );
    assert_eq!(catalog_state["active_workspace"], serde_json::json!(3));
    assert!(ctx.drain_events().is_empty());
}

#[test]
fn hyprland_reads_workspaces_from_ext_workspace_records() {
    let script = bundled_backend_script("../../../../modules/backend/hyprland-wm/src/main.luau");
    let mut ctx = BackendScriptContext::new("@mesh/hyprland-wm");
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .unwrap()
        .load(
            r#"
exec_calls = {}
mesh.wayland_subscribe = function(kind)
    assert(kind == "workspace")
    return { program = "wayland:workspace" }
end
mesh.exec = function(program, args)
    exec_calls[#exec_calls + 1] = { program = program, args = args }
    return { success = false, stdout = "", stderr = "", code = 1 }
end
"#,
        )
        .exec()
        .unwrap();
    ctx.call_init().unwrap();
    ctx.drain_events();

    let records = [
        r#"{"event":"group-added","id":10}"#,
        r#"{"event":"group-added","id":11}"#,
        r#"{"event":"added","id":1}"#,
        r#"{"event":"name","id":1,"value":"1"}"#,
        r#"{"event":"coordinates","id":1,"value":[1]}"#,
        r#"{"event":"state","id":1,"value":1}"#,
        r#"{"event":"added","id":2}"#,
        r#"{"event":"name","id":2,"value":"notes"}"#,
        r#"{"event":"coordinates","id":2,"value":[4294965959]}"#,
        r#"{"event":"state","id":2,"value":2}"#,
        r#"{"event":"added","id":3}"#,
        r#"{"event":"name","id":3,"value":"special:chat"}"#,
        r#"{"event":"coordinates","id":3,"value":[4294967197]}"#,
        r#"{"event":"state","id":3,"value":4}"#,
        r#"{"event":"added","id":4}"#,
        r#"{"event":"name","id":4,"value":"2"}"#,
        r#"{"event":"coordinates","id":4,"value":[2]}"#,
        r#"{"event":"state","id":4,"value":1}"#,
        r#"{"event":"group-workspace-enter","id":10,"workspace":1}"#,
        r#"{"event":"done"}"#,
    ]
    .map(String::from);
    let state = ctx
        .run_stream_batch("wayland:workspace", &records)
        .unwrap()
        .unwrap();
    assert_eq!(state["available"], serde_json::json!(true));
    assert_eq!(state["active_workspace"], serde_json::json!(1));
    let workspaces = state["workspaces"].as_array().unwrap();
    let ids: Vec<_> = workspaces.iter().map(|w| w["id"].clone()).collect();
    assert_eq!(ids, [serde_json::json!(-1337), serde_json::json!(1), serde_json::json!(2)]);
    assert_eq!(workspaces[0]["name"], serde_json::json!("notes"));
    assert_eq!(workspaces[0]["urgent"], serde_json::json!(true));
    assert!(workspaces[0].get("index").is_none_or(serde_json::Value::is_null));
    assert_eq!(workspaces[1]["focused"], serde_json::json!(true));
    assert_eq!(workspaces[1]["visible"], serde_json::json!(true));
    assert_eq!(workspaces[2]["focused"], serde_json::json!(false));
    assert_eq!(workspaces[2]["visible"], serde_json::json!(true));
    assert_eq!(workspaces[2]["windows"], serde_json::json!(0));

    ctx.drain_events();
    let switched = ctx
        .run_stream_batch(
            "wayland:workspace",
            &[
                r#"{"event":"state","id":1,"value":0}"#,
                r#"{"event":"state","id":2,"value":1}"#,
                r#"{"event":"done"}"#,
            ]
            .map(String::from),
        )
        .unwrap()
        .unwrap();
    assert_eq!(switched["active_workspace"], serde_json::json!(-1337));
    let events = ctx.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "WorkspaceChanged");
    assert_eq!(events[0].payload["id"], serde_json::json!(-1337));

    let calls = ctx
        .ensure_lua()
        .unwrap()
        .globals()
        .get::<Table>("exec_calls")
        .unwrap();
    for index in 1..=calls.raw_len() {
        let args = calls.get::<Table>(index).unwrap().get::<Table>("args").unwrap();
        let first: String = args.get(1).unwrap_or_default();
        assert!(
            first != "workspaces" && first != "activeworkspace",
            "workspace state must not be read through hyprctl"
        );
    }
}

#[test]
fn bundled_backend_scripts_expose_required_host_api_surface() {
    for (module_id, path) in [
        (
            "@mesh/pipewire-audio",
            "../../../../modules/backend/pipewire-audio/src/main.luau",
        ),
        (
            "@mesh/pulseaudio-audio",
            "../../../../modules/backend/pulseaudio-audio/src/main.luau",
        ),
        (
            "@mesh/networkmanager",
            "tests/fixtures/networkmanager-network/src/main.luau",
        ),
        (
            concat!("@mesh/", "upo", "wer"),
            concat!(
                "../../../../modules/backend/",
                "upo",
                "wer-power/src/main.luau"
            ),
        ),
        (
            "@mesh/backlight-brightness",
            "../../../../modules/backend/backlight-brightness/src/main.luau",
        ),
        (
            "@mesh/shell-theme",
            "tests/fixtures/shell-theme/src/main.luau",
        ),
        (
            "@mesh/device-info",
            "../../../../modules/backend/device-info/src/main.luau",
        ),
    ] {
        let script = bundled_backend_script(path);
        let mut ctx =
            BackendScriptContext::new_with_settings(module_id, serde_json::json!({ "demo": true }));
        ctx.load_script(&script).unwrap();
        ctx.call_init().unwrap();

        let mesh = ctx
            .ensure_lua()
            .unwrap()
            .globals()
            .get::<Table>("mesh")
            .unwrap();
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
    let script =
        bundled_backend_script("../../../../modules/backend/pulseaudio-audio/src/main.luau");
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
fn bundled_pipewire_backend_skips_successful_write_readback_when_streaming() {
    let script = bundled_backend_script("../../../../modules/backend/pipewire-audio/src/main.luau");
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@mesh/pipewire-audio",
        [
            "exec.argv:pw-mon:[\"--hide-props\",\"--hide-params\",\"--print-separator\"]"
                .to_string(),
        ],
    );
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .unwrap()
        .load(
            r#"
exec_calls = {}
mesh.exec_stream = function(program, _args)
    return program == "pw-mon" and { program = program } or false
end
mesh.exec = function(program, args)
    exec_calls[#exec_calls + 1] = { program = program, args = args }
    return { success = true, stdout = "", stderr = "", code = 0 }
end
"#,
        )
        .exec()
        .unwrap();

    ctx.call_init().unwrap();
    let outcome = ctx
        .run_command_finished("set-volume", &serde_json::json!({ "percent": 72 }))
        .unwrap();
    assert_eq!(outcome.result["ok"], serde_json::json!(true));

    let calls = ctx
        .ensure_lua()
        .unwrap()
        .globals()
        .get::<Table>("exec_calls")
        .unwrap();
    assert_eq!(
        calls.raw_len(),
        1,
        "streaming writes must not reread volume"
    );
    let call = calls.get::<Table>(1).unwrap();
    assert_eq!(call.get::<String>("program").unwrap(), "wpctl");
    assert_eq!(
        call.get::<Table>("args").unwrap().get::<String>(1).unwrap(),
        "set-volume"
    );
}

#[test]
fn bundled_pulseaudio_backend_skips_successful_write_readback_when_streaming() {
    let script =
        bundled_backend_script("../../../../modules/backend/pulseaudio-audio/src/main.luau");
    let mut ctx = BackendScriptContext::new("@mesh/pulseaudio-audio");
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .unwrap()
        .load(
            r#"
exec_calls = {}
mesh.exec_stream = function(program, _args)
    return program == "pactl"
end
mesh.exec = function(program, args)
    exec_calls[#exec_calls + 1] = { program = program, args = args }
    return { success = true, stdout = "", stderr = "", code = 0 }
end
"#,
        )
        .exec()
        .unwrap();

    ctx.call_init().unwrap();
    let outcome = ctx
        .run_command_finished("set-volume", &serde_json::json!({ "percent": 72 }))
        .unwrap();
    assert_eq!(outcome.result["ok"], serde_json::json!(true));

    let calls = ctx
        .ensure_lua()
        .unwrap()
        .globals()
        .get::<Table>("exec_calls")
        .unwrap();
    assert_eq!(
        calls.raw_len(),
        1,
        "streaming writes must not reread volume"
    );
    let call = calls.get::<Table>(1).unwrap();
    assert_eq!(call.get::<String>("program").unwrap(), "pactl");
    assert_eq!(
        call.get::<Table>("args").unwrap().get::<String>(1).unwrap(),
        "set-sink-volume"
    );
}

#[test]
fn emit_json_accepts_explicit_string() {
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/backend",
        ["exec.argv:printf:[\"*\"]".into()],
    );
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal r = mesh.exec(\"printf\", {\"{\\\"available\\\":true,\\\"percent\\\":65}\"})\nmesh.service.emit_json(r.stdout)\nend",
    )
    .unwrap();
    let payload = settled_poll(&mut ctx);
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
    ctx.call_init().unwrap();

    let globals = ctx.ensure_lua().unwrap().globals();
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
fn command_helper_name_is_not_a_public_command() {
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.load_script(
        "function start()\nend\nfunction set_volume()\nmesh.service.emit({ ok = true })\nend",
    )
    .unwrap();
    let payload = ctx.run_command("set-volume", &serde_json::json!({}));
    assert!(payload.unwrap().is_none());
}

#[test]
fn command_registry_validates_arguments_and_blocks_implicit_helpers() {
    let registry = BackendCommandRegistry::from_specs(
        vec![BackendCommandSpec {
            name: "set-volume".to_string(),
            arguments: vec![BackendCommandArgument {
                name: "percent".to_string(),
                type_expr: "int".to_string(),
            }],
            returns: None,
            coalesce: true.into(),
        }],
        std::collections::HashMap::new(),
    );
    let mut ctx = BackendScriptContext::new("@test/backend");
    ctx.set_command_registry(registry);
    ctx.load_script(
        "function start()\nend\nfunction set_volume()\nerror('helper must remain private')\nend",
    )
    .unwrap();

    let invalid = ctx
        .run_command_finished("set-volume", &serde_json::json!({ "percent": "loud" }))
        .unwrap();
    assert_eq!(
        invalid
            .result
            .get("status")
            .and_then(|value| value.as_str()),
        Some("invalid_arguments")
    );
    assert!(
        ctx.run_command_finished("set-volume", &serde_json::json!({ "percent": 20 }))
            .unwrap()
            .result
            .get("ok")
            .and_then(|value| value.as_bool())
            .is_some_and(|ok| !ok)
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
        .run_command_finished("set-volume", &serde_json::json!({ "percent": 67 }))
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
        .run_command_finished("ping", &serde_json::json!({}))
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
        .run_command_finished("fail", &serde_json::json!({}))
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
fn exec_returns_structured_result() {
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/backend",
        ["exec.argv:printf:[\"*\"]".into()],
    );
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"printf\", {\"hello\"})\nmesh.service.emit({ ok = result.success, stdout = result.stdout })\nend",
    )
    .unwrap();
    let payload = settled_poll(&mut ctx);
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("stdout").and_then(|v| v.as_str()),
        Some("hello")
    );
}

#[test]
fn exec_locale_option_needs_its_capability() {
    let script = "function start()\nend\nfunction on_poll()\nlocal ok, result = pcall(mesh.exec, \"printenv\", {\"LC_ALL\"}, { locale = \"sk_SK.UTF-8\" })\nmesh.service.emit({ ok = ok, stdout = ok and result.stdout or \"\" })\nend";

    let mut denied = BackendScriptContext::new_with_capabilities(
        "@test/backend",
        ["exec.argv:printenv:[\"LC_ALL\"]".into()],
    );
    denied.load_script(script).unwrap();
    let payload = denied.run_poll().unwrap().unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(false));

    let mut granted = BackendScriptContext::new_with_capabilities(
        "@test/backend",
        [
            "exec.argv:printenv:[\"LC_ALL\"]".into(),
            "exec.locale".into(),
        ],
    );
    granted.load_script(script).unwrap();
    let payload = settled_poll(&mut granted);
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("stdout").and_then(|v| v.as_str()),
        Some("sk_SK.UTF-8\n")
    );
}

#[test]
fn exec_accepts_program_and_args() {
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/backend",
        ["exec.argv:printf:[\"*\"]".into()],
    );
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"printf\", {\"hello\"})\nmesh.service.emit({ success = result.success, stdout = result.stdout, code = result.code })\nend",
    )
    .unwrap();
    let payload = settled_poll(&mut ctx);
    assert_eq!(payload.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("stdout").and_then(|v| v.as_str()),
        Some("hello")
    );
    assert_eq!(payload.get("code").and_then(|v| v.as_i64()), Some(0));
}

#[test]
fn exec_rejects_single_string_command_form() {
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/backend",
        ["exec.argv:printf:[\"*\"]".into()],
    );
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
    let payload = settled_poll(&mut ctx);
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
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/backend",
        ["exec.argv:sh:[\"-c\",\"printf err >&2; exit 7\"]".into()],
    );
    ctx.load_script(
        "function start()\nend\nfunction on_poll()\nlocal result = mesh.exec(\"sh\", {\"-c\", \"printf err >&2; exit 7\"})\nmesh.service.emit({ success = result.success, stdout = result.stdout, stderr = result.stderr, code = result.code })\nend",
    )
    .unwrap();
    let payload = settled_poll(&mut ctx);
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
            .is_some_and(|stderr| stderr.contains("exec.argv:"))
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
fn backend_host_setup_failure_is_recoverable() {
    let mut ctx = BackendScriptContext::new("@test/backend-host-setup");
    ctx.fail_host_setup_for_test("injected host registration failure");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.load_script("function start()\nend")
    }));
    let error = result
        .expect("host setup failure must not panic")
        .expect_err("injected host setup failure should be returned");
    assert!(matches!(error, BackendScriptError::HostSetup { .. }));
    assert!(
        error
            .to_string()
            .contains("injected host registration failure")
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
        .unwrap()
        .globals()
        .set(
            "state",
            ctx2.ensure_lua()
                .unwrap()
                .create_function(|_, ()| Ok(()))
                .unwrap(),
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
        "function start()\nstate = { value = 'old' }\nend\n\
         function on_command_bad_result()\n\
           state = { value = 'new' }\n\
           return function() end\n\
         end",
    )
    .unwrap();
    ctx.call_init().unwrap();
    let err = ctx
        .run_command_finished("bad-result", &serde_json::json!({}))
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
    assert_eq!(
        ctx.take_service_state_snapshot()
            .unwrap()
            .map(super::BackendState::into_payload),
        Some(serde_json::json!({ "value": "new" })),
        "failed result conversion leaves private state intact"
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
        .run_command_finished("fail", &serde_json::json!({}))
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
    let script = bundled_backend_script("tests/fixtures/reference-media/src/main.luau");
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
    let script = bundled_backend_script("tests/fixtures/reference-media/src/main.luau");
    let mut ctx = BackendScriptContext::new("@mesh/reference-media");
    ctx.load_script(&script).unwrap();
    ctx.call_init().unwrap();

    // Issue play command — state should transition to playing
    let outcome = ctx
        .run_command_finished("play", &serde_json::json!({ "player_id": "default" }))
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
        .run_command_finished("next", &serde_json::json!({ "player_id": "default" }))
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

#[tokio::test]
async fn typed_stream_events_expose_handle_identity_and_exit_status() {
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/typed-stream",
        ["exec.argv:sh:[\"-c\",\"printf 'hello\\\\n'\"]".to_string()],
    );
    ctx.load_script(
        "state = {}\n\
         function start()\n\
           local stream = mesh.exec_stream(\"sh\", { \"-c\", \"printf 'hello\\\\n'\" })\n\
           state = { id = stream.id, generation = stream.generation, program = stream.program }\n\
         end\n\
         function on_stream_event(self, event)\n\
           state = { event = event.type, id = event.stream.id, status = event.stream.status, line = event.line }\n\
         end",
    )
    .unwrap();
    let initial = ctx.call_init().unwrap().unwrap();
    let stream_id = initial.get("id").and_then(|value| value.as_u64()).unwrap();
    assert_eq!(
        initial.get("program").and_then(|value| value.as_str()),
        Some("sh")
    );
    assert!(
        initial
            .get("generation")
            .and_then(|value| value.as_u64())
            .unwrap_or_default()
            > 0
    );

    let streams = ctx.stream_state();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut exited = false;
    while tokio::time::Instant::now() < deadline && !exited {
        streams.wait_for_event().await;
        for event in streams.drain_events() {
            let snapshot = ctx.run_stream_event(&event).unwrap();
            assert_eq!(event.stream.id().raw(), stream_id);
            if matches!(event.kind, StreamEventKind::Exited(_)) {
                let snapshot = snapshot.expect("exit callback should publish state");
                assert_eq!(
                    snapshot.get("event").and_then(|value| value.as_str()),
                    Some("exited")
                );
                assert_eq!(
                    snapshot.get("id").and_then(|value| value.as_u64()),
                    Some(stream_id)
                );
                exited = true;
            }
        }
    }
    assert!(exited, "typed stream exit event should arrive");
    ctx.shutdown_streams().await;
}

#[test]
fn backend_tasks_resume_without_poll_and_isolate_failures() {
    let mut ctx = BackendScriptContext::new("@test/backend-tasks");
    ctx.load_script(
        r#"
state = { value = 0 }
function start(self)
    task.spawn(function()
        task.sleep(0)
        state.value = 7
    end)
    task.delay(0, function() error("independent task failure") end)
    task.delay(0, function() state.ready = true end)
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    assert_eq!(ctx.pending_task_count(), 3);
    let BackendTaskRun {
        state: payload,
        errors,
        ..
    } = ctx.run_due_tasks(std::time::Instant::now());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].to_string().contains("independent task failure"));
    assert_eq!(
        payload.unwrap(),
        serde_json::json!({"value": 7, "ready": true})
    );
    assert_eq!(ctx.pending_task_count(), 0);
    assert!(ctx.next_task_deadline().is_none());
}

#[test]
fn backend_task_handles_cancel_and_stop_clears_pending_work() {
    let mut ctx = BackendScriptContext::new("@test/backend-task-lifetime");
    ctx.load_script(
        r#"
state = { value = 0 }
function start(self)
    local handle = task.delay(0, function() state.value = 1 end)
    state.cancelled = handle:cancel()
    task.spawn(function() task.sleep(10000); state.value = 2 end)
end
"#,
    )
    .unwrap();
    let state = ctx.call_init().unwrap().unwrap();
    assert_eq!(state["cancelled"], true);
    assert_eq!(ctx.pending_task_count(), 1);
    ctx.call_stop().unwrap();
    assert_eq!(ctx.pending_task_count(), 0);
    assert!(ctx.next_task_deadline().is_none());
}

#[test]
fn backend_task_registration_requires_startup() {
    for source in [
        "task.spawn(function() end); function start(self) end",
        "task.delay(0, function() end); function start(self) end",
    ] {
        let mut ctx = BackendScriptContext::new("@test/backend-task-staging");
        assert!(
            ctx.load_script(source)
                .unwrap_err()
                .to_string()
                .contains("before start(self)")
        );
        assert_eq!(ctx.pending_task_count(), 0);
    }
}

fn synchronous_exec_from_sort_returns_its_result() {
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/sync-exec-sort",
        ["exec.argv:printf:[\"*\"]".into()],
    );
    ctx.load_script(
        r#"
state = {}
function start(self)
    table.sort({2, 1}, function(a, b)
        local result = mesh.exec("printf", {"done"})
        state.success = result.success
        state.output = result.stdout
        return a < b
    end)
end
"#,
    )
    .unwrap();
    let state = ctx.call_init().unwrap().unwrap();
    assert_eq!(state["success"], true);
    assert_eq!(state["output"], "done");
}

#[test]
fn synchronous_exec_without_tokio_returns_its_result() {
    synchronous_exec_from_sort_returns_its_result();
}

#[tokio::test]
async fn synchronous_exec_on_current_thread_runtime_returns_its_result() {
    synchronous_exec_from_sort_returns_its_result();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn synchronous_exec_releases_the_tokio_worker() {
    let root = temp_storage_root("sync-exec-worker");
    std::fs::create_dir_all(&root).unwrap();
    let release = root.join("release");
    let ready = std::sync::Arc::new(tokio::sync::Notify::new());
    let release_ready = ready.clone();
    let release_path = release.clone();
    let independent = tokio::spawn(async move {
        release_ready.notified().await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        std::fs::write(release_path, "ready").unwrap();
    });
    let backend = tokio::spawn(async move {
        let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
            "@test/sync-exec-worker",
            serde_json::json!({"release": release}),
            mesh_core_capability::CapabilitySet::from_ids(["exec.command"]),
        );
        ctx.load_script(r#"
state = {}
function start(self)
    table.sort({2, 1}, function(a, b)
        local result = mesh.exec("sh", {"-c", 'while [ ! -e "$1" ]; do sleep 0.01; done; printf done', "mesh-test", mesh.config().release})
        state.success = result.success
        state.output = result.stdout
        state.error = result.stderr
        return a < b
    end)
end
"#).unwrap();
        ready.notify_one();
        ctx.call_init().unwrap().unwrap()
    });
    let state = backend.await.unwrap();
    independent.await.unwrap();
    let _ = std::fs::remove_dir_all(root);
    assert_eq!(state["success"], true, "{state:?}");
    assert_eq!(state["output"], "done");
}

#[tokio::test]
async fn backend_exec_task_yields_while_independent_tasks_run() {
    let root = temp_storage_root("async-exec");
    std::fs::create_dir_all(&root).unwrap();
    let release = root.join("release");
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        "@test/async-exec",
        serde_json::json!({"release": release}),
        mesh_core_capability::CapabilitySet::from_ids(["exec.command"]),
    );
    ctx.load_script(r#"
state = { done = false }
function start(self)
    task.spawn(function()
        local result = mesh.exec("sh", {"-c", 'while [ ! -e "$1" ]; do sleep 0.01; done; printf done', "mesh-test", mesh.config().release})
        state.done = result.success
        state.output = result.stdout
        state.error = result.stderr
    end)
    task.delay(0, function() state.independent = true end)
end
"#).unwrap();
    ctx.call_init().unwrap();
    assert_eq!(ctx.pending_task_count(), 2);
    let BackendTaskRun { state, errors, .. } = ctx.run_due_tasks(std::time::Instant::now());
    assert!(errors.is_empty(), "{errors:?}");
    let state = state.unwrap();
    assert_eq!(state["independent"], true);
    assert_eq!(state["done"], false);
    std::fs::write(&release, "ready").unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        ctx.task_wakeup().notified(),
    )
    .await
    .unwrap();
    let BackendTaskRun { state, errors, .. } = ctx.run_due_tasks(std::time::Instant::now());
    assert!(errors.is_empty(), "{errors:?}");
    let state = state.unwrap();
    assert_eq!(state["done"], true, "{state:?}");
    assert_eq!(state["output"], "done");
    assert_eq!(ctx.pending_task_count(), 0);
    drop(ctx);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn backend_socket_task_yields_until_the_peer_replies() {
    use std::io::{Read, Write};
    let root = temp_storage_root("async-socket");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("service.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let (release, released) = std::sync::mpsc::channel();
    let peer = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = String::new();
        socket.read_to_string(&mut request).unwrap();
        assert_eq!(request, "request");
        released.recv().unwrap();
        socket.write_all(b"reply").unwrap();
    });
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        "@test/async-socket",
        serde_json::json!({"socket": path}),
        mesh_core_capability::CapabilitySet::from_ids(["net.socket"]),
    );
    ctx.load_script(
        r#"
state = { done = false }
function start(self)
    task.spawn(function()
        local reply = mesh.socket_request(mesh.config().socket, "request")
        state.done = reply.success
        state.response = reply.response
    end)
    task.delay(0, function() state.independent = true end)
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    assert_eq!(ctx.pending_task_count(), 2);
    let BackendTaskRun { state, errors, .. } = ctx.run_due_tasks(std::time::Instant::now());
    assert!(errors.is_empty(), "{errors:?}");
    let state = state.unwrap();
    assert_eq!(state["independent"], true);
    assert_eq!(state["done"], false);
    release.send(()).unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        ctx.task_wakeup().notified(),
    )
    .await
    .unwrap();
    let BackendTaskRun { state, errors, .. } = ctx.run_due_tasks(std::time::Instant::now());
    assert!(errors.is_empty(), "{errors:?}");
    let state = state.unwrap();
    assert_eq!(state["done"], true, "{state:?}");
    assert_eq!(state["response"], "reply");
    peer.join().unwrap();
    drop(ctx);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn backend_file_task_reads_regular_files_with_an_explicit_grant() {
    let root = temp_storage_root("async-file");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("contents");
    std::fs::write(&path, "hello from a file").unwrap();
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        "@test/async-file",
        serde_json::json!({"path": path}),
        mesh_core_capability::CapabilitySet::from_ids([format!("fs.read:{}", path.display())]),
    );
    ctx.load_script(
        r#"
state = {}
function start(self)
    task.spawn(function()
        local reply = mesh.read_file(mesh.config().path)
        state.done = reply.success
        state.contents = reply.contents
    end)
end
"#,
    )
    .unwrap();
    let initial = ctx.call_init().unwrap();
    let state = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        // A fast worker can finish task.spawn before start returns.
        if let Some(state) = initial
            && state["done"] == true
        {
            return state;
        }
        loop {
            let BackendTaskRun { state, errors, .. } = ctx.run_due_tasks(std::time::Instant::now());
            assert!(errors.is_empty(), "{errors:?}");
            if let Some(state) = state {
                assert_eq!(state["done"], true);
                return state;
            }
            ctx.task_wakeup().notified().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state["contents"], "hello from a file");
    drop(ctx);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn backend_file_reads_require_a_grant_and_an_absolute_regular_path() {
    let mut denied = BackendScriptContext::new("@test/denied-file");
    denied
        .load_script("function start(self) mesh.read_file('/etc/passwd') end")
        .unwrap();
    assert!(
        denied
            .call_init()
            .unwrap_err()
            .to_string()
            .contains("fs.read")
    );
    let mut granted =
        BackendScriptContext::new_with_capabilities("@test/file-path", ["fs.read:/**".into()]);
    granted
        .load_script("function start(self) mesh.read_file('relative') end")
        .unwrap();
    assert!(
        granted
            .call_init()
            .unwrap_err()
            .to_string()
            .contains("absolute path")
    );
}

#[test]
fn denied_exec_inside_a_task_preserves_the_reply_contract() {
    let mut ctx = BackendScriptContext::new("@test/denied-async-exec");
    ctx.load_script(
        r#"
state = {}
function start(self)
    task.spawn(function()
        local result = mesh.exec("sh", {"-c", "printf forbidden"})
        state.success = result.success
        state.no_code = result.code == nil
        state.error = result.stderr
    end)
end
"#,
    )
    .unwrap();
    let state = ctx.call_init().unwrap().unwrap();
    assert_eq!(state["success"], false);
    assert_eq!(state["no_code"], true);
    assert!(
        state["error"]
            .as_str()
            .unwrap()
            .contains("denied mesh.exec")
    );
    assert_eq!(ctx.pending_task_count(), 0);
}

trait FinishedCommand {
    /// The state a finished command left, or its handler error.
    fn run_command(
        &mut self,
        command: &str,
        payload: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, BackendScriptError>;

    fn run_command_finished(
        &mut self,
        command: &str,
        payload: &serde_json::Value,
    ) -> Result<BackendCommandOutcome, BackendScriptError>;
}

impl FinishedCommand for BackendScriptContext {
    fn run_command(
        &mut self,
        command: &str,
        payload: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, BackendScriptError> {
        let outcome = self.run_command_finished(command, payload)?;
        match outcome.error {
            Some(message) => Err(BackendScriptError::Runtime {
                module_id: "@test/backend".to_string(),
                message,
            }),
            None => Ok(outcome.state.map(super::BackendState::into_payload)),
        }
    }

    fn run_command_finished(
        &mut self,
        command: &str,
        payload: &serde_json::Value,
    ) -> Result<BackendCommandOutcome, BackendScriptError> {
        match self.run_command_with_result(command, payload)? {
            BackendCommandRun::Finished(outcome) => Ok(outcome),
            BackendCommandRun::Suspended { task, .. } => {
                panic!("command {command} suspended as task {task}")
            }
        }
    }
}

const ASYNC_CALLBACKS: &str = r#"
state = { started = false, polls = 0 }
local function wait_for(path)
    return mesh.exec("sh", {"-c", 'while [ ! -e "$1" ]; do sleep 0.01; done; printf ok', "mesh-test", path})
end
function start(self)
    state.started = wait_for(mesh.config().start).stdout == "ok"
end
function on_poll(self)
    state.polls += 1
    state.polled = wait_for(mesh.config().poll).stdout
end
function on_command_fetch(self)
    local amount = mesh.service.payload().amount
    local result = wait_for(mesh.config().command)
    return { ok = result.success, amount = amount }
end
function on_command_fail(self)
    wait_for(mesh.config().command)
    error("failed after suspending")
end
"#;

fn async_callbacks_context(root: &std::path::Path) -> BackendScriptContext {
    std::fs::create_dir_all(root).unwrap();
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        "@test/async-callbacks",
        serde_json::json!({
            "start": root.join("start"),
            "poll": root.join("poll"),
            "command": root.join("command"),
        }),
        mesh_core_capability::CapabilitySet::from_ids(["exec.command"]),
    );
    ctx.load_script(ASYNC_CALLBACKS).unwrap();
    ctx
}

async fn settle(ctx: &mut BackendScriptContext) -> BackendTaskRun {
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        ctx.task_wakeup().notified(),
    )
    .await
    .expect("host operation should complete");
    ctx.run_due_tasks(std::time::Instant::now())
}

#[tokio::test]
async fn suspended_start_finishes_before_the_session_runs() {
    let root = temp_storage_root("async-start");
    let mut ctx = async_callbacks_context(&root);
    ctx.call_init().unwrap();
    assert!(ctx.start_pending());
    assert_ne!(
        ctx.session().lifecycle().state(),
        RuntimeLifecycleState::Running
    );
    std::fs::write(root.join("start"), "").unwrap();
    let run = settle(&mut ctx).await;
    assert!(matches!(run.start, Some(Ok(()))), "{run:?}");
    assert!(!ctx.start_pending());
    assert_eq!(
        ctx.session().lifecycle().state(),
        RuntimeLifecycleState::Running
    );
    assert_eq!(run.state.unwrap()["started"], true);
    drop(ctx);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn suspended_poll_stays_in_flight_until_its_host_call_returns() {
    let root = temp_storage_root("async-poll");
    let mut ctx = async_callbacks_context(&root);
    std::fs::write(root.join("start"), "").unwrap();
    ctx.call_init().unwrap();
    let _ = settle(&mut ctx).await;
    assert!(ctx.run_poll().unwrap().is_none());
    assert!(ctx.poll_in_flight());
    std::fs::write(root.join("poll"), "").unwrap();
    let run = settle(&mut ctx).await;
    assert_eq!(run.polls.len(), 1);
    assert!(run.polls[0].is_ok());
    assert!(!ctx.poll_in_flight());
    assert_eq!(run.state.unwrap()["polled"], "ok");
    drop(ctx);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn suspended_command_settles_with_its_own_payload_and_result() {
    let root = temp_storage_root("async-command");
    let mut ctx = async_callbacks_context(&root);
    std::fs::write(root.join("start"), "").unwrap();
    ctx.call_init().unwrap();
    let _ = settle(&mut ctx).await;
    let BackendCommandRun::Suspended { task: fetch, .. } = ctx
        .run_command_with_result("fetch", &serde_json::json!({ "amount": 3 }))
        .unwrap()
    else {
        panic!("fetch should suspend on mesh.exec");
    };
    let BackendCommandRun::Suspended { task: fail, .. } = ctx
        .run_command_with_result("fail", &serde_json::json!({}))
        .unwrap()
    else {
        panic!("fail should suspend on mesh.exec");
    };
    std::fs::write(root.join("command"), "").unwrap();
    let mut settled = HashMap::new();
    while settled.len() < 2 {
        for (task, outcome) in settle(&mut ctx).await.commands {
            settled.insert(task, outcome.unwrap());
        }
    }
    assert_eq!(
        settled[&fetch].result,
        serde_json::json!({ "ok": true, "amount": 3 })
    );
    assert!(
        settled[&fail]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("failed after suspending"))
    );
    assert_eq!(settled[&fail].result["ok"], false);
    drop(ctx);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn cancelling_a_suspended_command_drops_its_task() {
    let root = temp_storage_root("async-cancel");
    let mut ctx = async_callbacks_context(&root);
    std::fs::write(root.join("start"), "").unwrap();
    ctx.call_init().unwrap();
    let _ = settle(&mut ctx).await;
    let BackendCommandRun::Suspended { task, .. } = ctx
        .run_command_with_result("fetch", &serde_json::json!({ "amount": 1 }))
        .unwrap()
    else {
        panic!("fetch should suspend on mesh.exec");
    };
    assert!(ctx.cancel_task(task));
    assert_eq!(ctx.pending_task_count(), 0);
    std::fs::write(root.join("command"), "").unwrap();
    let run = settle(&mut ctx).await;
    assert!(run.commands.is_empty());
    drop(ctx);
    let _ = std::fs::remove_dir_all(root);
}

/// Run `on_poll` and, when it suspends on a host operation, wait for it to
/// finish. Returns the state snapshot after the poll.
fn settled_poll(ctx: &mut BackendScriptContext) -> serde_json::Value {
    let mut state = ctx.run_poll().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while ctx.poll_in_flight() {
        assert!(std::time::Instant::now() < deadline, "poll did not settle");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let run = ctx.run_due_tasks(std::time::Instant::now());
        for poll in run.polls {
            poll.unwrap();
        }
        state = run.state.or(state);
    }
    state.unwrap().into_payload()
}

#[test]
fn hyprland_focuses_named_workspaces_by_name_with_lua_quoting() {
    let script = bundled_backend_script("../../../../modules/backend/hyprland-wm/src/main.luau");
    let mut context = BackendScriptContext::new("@mesh/hyprland-wm");
    context.load_script(&script).unwrap();
    context
        .ensure_lua()
        .unwrap()
        .load(
            r#"
state.workspaces = { { id = -1337, name = 'notes "quoted"' } }
exec_calls = {}
mesh.exec = function(program, args)
    exec_calls[#exec_calls + 1] = args
    return { success = true, stdout = "", stderr = "" }
end
"#,
        )
        .exec()
        .unwrap();
    context
        .run_command_finished("focus_workspace", &serde_json::json!({"id":-1337}))
        .unwrap();
    let lua = context.ensure_lua().unwrap();
    let calls: Table = lua.globals().get("exec_calls").unwrap();
    assert_eq!(calls.raw_len(), 1);
    let args: Table = calls.get(1).unwrap();
    let dispatcher: String = args.get(2).unwrap();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let captured_clone = captured.clone();
    let focus = lua
        .create_function(move |_, args: Table| {
            *captured_clone.lock().unwrap() = args.get::<String>("workspace")?;
            Ok(())
        })
        .unwrap();
    let dsp = lua.create_table().unwrap();
    dsp.set("focus", focus).unwrap();
    let hl = lua.create_table().unwrap();
    hl.set("dsp", dsp).unwrap();
    let env = lua.create_table().unwrap();
    env.set("hl", hl).unwrap();
    lua.load(&dispatcher).set_environment(env).exec().unwrap();
    assert_eq!(*captured.lock().unwrap(), "name:notes \"quoted\"");
}

#[test]
fn hyprland_workspace_snapshot_preserves_named_ids_and_output_metadata() {
    let script = bundled_backend_script("../../../../modules/backend/hyprland-wm/src/main.luau");
    let mut context = BackendScriptContext::new("@mesh/hyprland-wm");
    context.load_script(&script).unwrap();
    context.ensure_lua().unwrap().load(r#"
mesh.socket_stream = function(_path) return { program = "socket" } end
mesh.socket_request = function(_path, request)
    if request == "j/activeworkspace" then return { success = true, response = '{"id":-1337}' } end
    if request == "j/activewindow" then return { success = true, response = '{}' } end
    if request == "j/workspaces" then return { success = true, response = [=[[{"id":-1337,"name":"code \"quoted\" {pane}","windows":1,"lastwindow":{"title":"{nested}"},"monitor":"DP-1"},{"id":2,"name":"2","windows":0,"monitor":"DP-2"},{"id":-99,"name":"special:chat","windows":1}]]=] } end
    return { success = false }
end
mesh.exec = function(program, args)
    if program == "sh" and string.find(args[2], "HYPRLAND_INSTANCE_SIGNATURE", 1, true) then
        return { success = true, stdout = "test-instance" }
    end
    if program == "sh" then return { success = true, stdout = "/run/user/1000" } end
    return { success = false }
end
"#).exec().unwrap();
    context.call_init().unwrap();
    let state = settled_poll(&mut context);
    assert_eq!(state["active_workspace"], serde_json::json!(-1337));
    let workspaces = state["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 2);
    assert_eq!(
        workspaces[0]["name"],
        serde_json::json!("code \"quoted\" {pane}")
    );
    assert_eq!(workspaces[0]["output"], serde_json::json!("DP-1"));
    assert_eq!(workspaces[0]["focused"], serde_json::json!(true));
    assert!(
        workspaces[0]
            .get("index")
            .is_none_or(serde_json::Value::is_null)
    );
    assert_eq!(workspaces[1]["index"], serde_json::json!(2));
    assert_eq!(workspaces[1]["focused"], serde_json::json!(false));
}

#[test]
fn failed_command_preserves_table_identity_without_publishing() {
    let mut ctx = BackendScriptContext::new("@test/command-publication");
    ctx.load_script(
        r#"
state = { value = 0 }
local captured = state
function start(self) end
function on_command_fail(self)
    state.value = 1
    mesh.service.emit({ value = 99 })
    error("failed")
end
function on_command_check(self)
    assert(state == captured)
    state.value += 1
    return { ok = true }
end
"#,
    )
    .unwrap();
    assert_eq!(ctx.call_init().unwrap().unwrap()["value"], 0);
    let failed = ctx
        .run_command_finished("fail", &serde_json::json!({}))
        .unwrap();
    assert!(failed.error.is_some());
    assert!(failed.state.is_none());
    let success = ctx
        .run_command_finished("check", &serde_json::json!({}))
        .unwrap();
    assert_eq!(success.state.unwrap()["value"], 2);
}

#[test]
fn task_publication_keeps_the_last_success_before_a_failure() {
    let mut ctx = BackendScriptContext::new("@test/task-publication");
    ctx.load_script(
        r#"
state = { value = 0 }
function start(self)
    task.delay(0, function() state.value = 1 end)
    task.delay(0, function()
        state.value = 2
        mesh.service.emit({ value = 99 })
        error("failed task")
    end)
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    let run = ctx.run_due_tasks(std::time::Instant::now());
    assert_eq!(run.errors.len(), 1);
    assert_eq!(run.state.unwrap()["value"], 1);
    let run = ctx.run_due_tasks(std::time::Instant::now());
    assert!(run.state.is_none());
}

#[test]
fn suspended_command_failure_does_not_publish_partial_state() {
    let mut ctx = BackendScriptContext::new("@test/suspended-publication");
    ctx.load_script(
        r#"
state = { value = 0 }
function start(self) end
function on_command_fail(self)
    state.value = 1
    mesh.service.emit({ value = 99 })
    task.sleep(0)
    state.value = 2
    error("failed after yield")
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    let run = ctx
        .run_command_with_result("fail", &serde_json::json!({}))
        .unwrap();
    assert!(matches!(
        run,
        BackendCommandRun::Suspended { state: None, .. }
    ));
    let run = ctx.run_due_tasks(std::time::Instant::now());
    assert!(run.state.is_none());
    assert_eq!(run.commands.len(), 1);
    assert!(run.commands[0].1.as_ref().unwrap().error.is_some());
}

#[test]
fn native_event_sources_require_grants_and_startup() {
    let calls = [
        "mesh.dbus_subscribe('session', { sender = 'org.mesh.Test', path_namespace = '/org/mesh/Test' })",
        "mesh.dbus_subscribe('system', { sender = 'org.mesh.Test', path_namespace = '/org/mesh/Test' })",
        "mesh.pipewire_subscribe()",
        "mesh.netlink_subscribe({'link'})",
        "mesh.wayland_subscribe('foreign-toplevel')",
    ];
    for call in calls {
        let mut ctx = BackendScriptContext::new("@test/native-denied");
        ctx.load_script(&format!(
            "state = {{}}; function start(self) state.denied = ({call}) == false end"
        ))
        .unwrap();
        assert_eq!(ctx.call_init().unwrap().unwrap()["denied"], true);
        assert_eq!(ctx.stream_state().active_stream_count(), 0);
        let mut ctx = BackendScriptContext::new("@test/native-staging");
        assert!(
            ctx.load_script(&format!("{call}; function start(self) end"))
                .unwrap_err()
                .to_string()
                .contains("before start(self)")
        );
    }
}

#[test]
fn dbus_call_requires_a_matching_grant_and_startup() {
    let call = "mesh.dbus_call('session', { destination = 'org.mesh.Test', path = '/org/mesh/Test', interface = 'org.mesh.Test', member = 'Echo' })";
    for grants in [
        vec![],
        vec!["dbus.subscribe:session:org.mesh.Test".to_string()],
        vec!["dbus.read:session:org.mesh.Test:org.mesh.Test".to_string()],
        vec!["dbus.call:system:*:org.mesh.Test".to_string()],
        vec!["dbus.call:session:org.mesh.Other:org.mesh.Test".to_string()],
        vec!["dbus.call:session:*:org.mesh.Other".to_string()],
    ] {
        let mut ctx =
            BackendScriptContext::new_with_capabilities("@test/dbus-call", grants.clone());
        ctx.load_script(&format!(
            "state = {{}}; function start(self) local reply = {call}; state.success = reply.success; state.error = reply.error end"
        ))
        .unwrap();
        let state = ctx.call_init().unwrap().unwrap();
        assert_eq!(state["success"], false, "{grants:?}");
        assert_eq!(
            state["error"], "missing capability dbus.call:session:org.mesh.Test:org.mesh.Test",
            "{grants:?}"
        );
    }
    let mut ctx = BackendScriptContext::new("@test/dbus-call-staging");
    assert!(
        ctx.load_script(&format!("{call}; function start(self) end"))
            .unwrap_err()
            .to_string()
            .contains("before start(self)")
    );
    let mut ctx = BackendScriptContext::new("@test/dbus-call-fields");
    ctx.load_script("function start(self) mesh.dbus_call('session', { destination = 'a.b', path = '/', interface = 'a.b', member = 'C', extra = 1 }) end")
        .unwrap();
    assert!(ctx.call_init().is_err());
}

#[test]
fn dbus_serve_requires_an_ownership_grant_for_that_name_and_bus() {
    let serve = "mesh.dbus_serve('session', { name = 'org.mesh.Served', path = '/org/mesh/Served', interface = 'org.mesh.Served' })";
    for grants in [
        vec![],
        vec!["dbus.subscribe:session:org.mesh.Served".to_string()],
        vec!["dbus.call:session:*:org.mesh.Served".to_string()],
        vec!["dbus.own:system:org.mesh.Served".to_string()],
        vec!["dbus.own:session:org.mesh.Other".to_string()],
    ] {
        let mut ctx =
            BackendScriptContext::new_with_capabilities("@test/dbus-serve", grants.clone());
        ctx.load_script(&format!(
            "state = {{}}; function start(self) state.served = {serve} end"
        ))
        .unwrap();
        let state = ctx.call_init().unwrap().unwrap();
        assert_eq!(state["served"], false, "{grants:?}");
    }
    let mut ctx = BackendScriptContext::new("@test/dbus-serve-staging");
    assert!(
        ctx.load_script(&format!("{serve}; function start(self) end"))
            .unwrap_err()
            .to_string()
            .contains("before start(self)")
    );
    let mut ctx = BackendScriptContext::new("@test/dbus-reply-foreign");
    ctx.load_script(
        "state = {}; function start(self) state.replied = mesh.dbus_reply({ id = 1, generation = 999 }, 1, {}) end",
    )
    .unwrap();
    let state = ctx.call_init().unwrap().unwrap();
    assert_eq!(
        state["replied"], false,
        "a handle from another generation is ignored"
    );
}

#[test]
fn stream_callbacks_suspend_and_report_completion_and_failure() {
    for hook in ["on_stream_batch", "on_stream_line"] {
        let mut ctx = BackendScriptContext::new("@test/async-stream");
        ctx.load_script(&format!(
            r#"
state = {{ count = 0 }}
function start(self) mesh.service.set_poll_interval(0) end
function {hook}(self, program, data)
    task.sleep(0)
    state.count += 1
    if program == "fail" then error("stream failed") end
end
"#
        ))
        .unwrap();
        ctx.call_init().unwrap();
        assert_eq!(ctx.poll_interval_ms(), 0);
        assert!(
            ctx.run_stream_batch("test", &["a".into(), "b".into()])
                .unwrap()
                .is_none()
        );
        assert!(ctx.stream_in_flight());
        let mut completed = 0;
        for _ in 0..3 {
            let run = ctx.run_due_tasks(std::time::Instant::now());
            assert!(run.errors.is_empty());
            completed += run.streams.len();
        }
        assert_eq!(completed, 1);
        assert!(!ctx.stream_in_flight());
        ctx.run_stream_batch("fail", &["a".into()]).unwrap();
        let run = ctx.run_due_tasks(std::time::Instant::now());
        assert!(run.state.is_none());
        assert_eq!(run.streams.len(), 1);
        assert!(
            run.streams[0]
                .as_ref()
                .unwrap_err()
                .to_string()
                .contains("stream failed")
        );
    }
}

#[tokio::test]
async fn stop_hook_suspends_cancels_old_tasks_and_runs_once() {
    let mut ctx = BackendScriptContext::new("@test/async-stop");
    ctx.load_script(
        r#"
state = { stopped = 0 }
function start(self) task.delay(0, function() error("old task ran") end) end
function stop(self)
    task.sleep(0)
    state.stopped += 1
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    ctx.call_stop_async().await.unwrap();
    ctx.call_stop_async().await.unwrap();
    assert_eq!(ctx.pending_task_count(), 0);
    assert_eq!(
        ctx.take_service_state_snapshot().unwrap().unwrap()["stopped"],
        1
    );
}

#[tokio::test]
async fn stop_hook_has_a_bounded_shutdown_deadline() {
    let mut ctx = BackendScriptContext::new("@test/stop-deadline");
    ctx.load_script("function start(self) end; function stop(self) task.sleep(10000) end")
        .unwrap();
    ctx.call_init().unwrap();
    let error = ctx.call_stop_async().await.unwrap_err();
    assert!(error.to_string().contains("shutdown deadline"));
    assert_eq!(ctx.pending_task_count(), 0);
}

#[test]
fn shipped_providers_prefer_native_sources_and_fall_back_when_the_source_closes() {
    for (module, native, program, fallback) in [
        (
            "upower-power",
            "dbus_subscribe",
            "dbus:system:org.freedesktop.UPower",
            0,
        ),
        (
            "networkmanager-network",
            "dbus_subscribe",
            "dbus:system:org.freedesktop.NetworkManager",
            0,
        ),
        ("pipewire-audio", "pipewire_subscribe", "pipewire", 1),
    ] {
        let mut ctx = BackendScriptContext::new(format!("@mesh/{module}"));
        let source = bundled_backend_script(&format!(
            "../../../../modules/backend/{module}/src/main.luau"
        ));
        ctx.load_script(&source).unwrap();
        ctx.ensure_lua()
            .unwrap()
            .load(format!(
                r#"
monitor_count = 0
native_count = 0
mesh.{native} = function(...)
    native_count += 1
    return {{ program = "{program}" }}
end
mesh.service.can_exec = function(...) return true end
mesh.exec_stream = function(program, args)
    monitor_count += 1
    return {{ program = program }}
end
mesh.exec = function(...) return {{ success = true, stdout = "", stderr = "" }} end
"#
            ))
            .exec()
            .unwrap();
        ctx.call_init().unwrap();
        let lua = ctx.ensure_lua().unwrap().clone();
        assert_eq!(
            lua.globals().get::<u32>("native_count").unwrap(),
            1,
            "{module}"
        );
        assert_eq!(
            lua.globals().get::<u32>("monitor_count").unwrap(),
            0,
            "{module}"
        );
        lua.globals()
            .get::<mlua::Function>("on_stream_closed")
            .unwrap()
            .call::<()>((mlua::Value::Nil, program, false))
            .unwrap();
        // UPower and NetworkManager read over D-Bus only; no CLI monitor remains.
        assert_eq!(
            lua.globals().get::<u32>("monitor_count").unwrap(),
            fallback,
            "{module}"
        );
    }
}

#[tokio::test]
#[ignore = "requires live UPower, NetworkManager and PipeWire; reads provider state only"]
async fn live_provider_event_profile() {
    use std::time::{Duration, Instant};
    for native in [false, true] {
        let mut contexts = Vec::new();
        for module in ["upower-power", "networkmanager-network", "pipewire-audio"] {
            let mut ctx = BackendScriptContext::new_with_capabilities(
                format!("@mesh/{module}"),
                [
                    "exec.command",
                    "dbus.subscribe:system:org.freedesktop.UPower",
                    "dbus.subscribe:system:org.freedesktop.NetworkManager",
                    "dbus.read:system:org.freedesktop.UPower:org.freedesktop.UPower.Device",
                    "dbus.read:system:org.freedesktop.NetworkManager:org.freedesktop.NetworkManager",
                    "dbus.read:system:org.freedesktop.NetworkManager:org.freedesktop.NetworkManager.Device",
                    "dbus.read:system:org.freedesktop.NetworkManager:org.freedesktop.NetworkManager.Device.Wireless",
                    "dbus.read:system:org.freedesktop.NetworkManager:org.freedesktop.NetworkManager.AccessPoint",
                    "pipewire.observe",
                ]
                .map(str::to_owned),
            );
            ctx.load_script(&bundled_backend_script(&format!(
                "../../../../modules/backend/{module}/src/main.luau"
            )))
            .unwrap();
            ctx.ensure_lua()
                .unwrap()
                .load(
                    r#"
exec_count = 0
monitor_count = 0
local exec = mesh.exec
mesh.exec = function(...)
    exec_count += 1
    return exec(...)
end
local stream = mesh.exec_stream
mesh.exec_stream = function(...)
    monitor_count += 1
    return stream(...)
end
"#,
                )
                .exec()
                .unwrap();
            if native && module == "networkmanager-network" {
                ctx.ensure_lua().unwrap().load(r#"
signal_counts = {}
local previous = on_stream_line
function on_stream_line(self, program, line)
    if program:sub(1, 5) == "dbus:" then
        local signal = mesh.json.decode(line)
        local keys = {}
        local fields = signal.body or {}
        local changed = fields[2] and fields[2].value
        if type(changed) == "table" then
            for key in pairs(changed) do keys[#keys + 1] = tostring(key) end
            table.sort(keys)
        end
        local label = (signal.interface or "") .. ":" .. (signal.member or "") .. ":" .. table.concat(keys, ",")
        signal_counts[label] = (signal_counts[label] or 0) + 1
    end
    return previous(self, program, line)
end
"#).exec().unwrap();
            }
            if !native {
                ctx.ensure_lua().unwrap().load("mesh.dbus_subscribe = function(...) return false end; mesh.pipewire_subscribe = function(...) return false end").exec().unwrap();
            }
            ctx.call_init().unwrap();
            contexts.push(ctx);
        }
        let mut next_poll = vec![Instant::now(); contexts.len()];
        let mut poll_started = vec![false; contexts.len()];
        let mut intervals = vec![0; contexts.len()];
        let started = Instant::now();
        let mut initial_execs = None;
        let mut maximum_workers = 0;
        while started.elapsed() < Duration::from_secs(5) {
            for (index, ctx) in contexts.iter_mut().enumerate() {
                let run = ctx.run_due_tasks(Instant::now());
                assert!(run.errors.is_empty(), "{:?}", run.errors);
                assert!(run.streams.iter().all(Result::is_ok), "{:?}", run.streams);
                if !ctx.start_pending() && !ctx.stream_in_flight() {
                    let events = ctx.stream_state().drain_dispatch_batch(false);
                    let mut lines = Vec::new();
                    let mut program = String::new();
                    for event in events {
                        match event.kind {
                            StreamEventKind::Line(line) => {
                                program = event.stream.program().into();
                                lines.push(line);
                            }
                            StreamEventKind::Exited(status) => {
                                ctx.run_stream_closed(&event.stream, status.success)
                                    .unwrap();
                            }
                            StreamEventKind::Failed(message) => {
                                panic!("live source failed: {message}")
                            }
                            _ => {}
                        }
                    }
                    if !lines.is_empty() {
                        ctx.run_stream_batch(&program, &lines).unwrap();
                    }
                }
                if poll_started[index] && intervals[index] != ctx.poll_interval_ms() {
                    intervals[index] = ctx.poll_interval_ms();
                    next_poll[index] = Instant::now() + Duration::from_millis(intervals[index]);
                }
                if !ctx.start_pending()
                    && !ctx.poll_in_flight()
                    && Instant::now() >= next_poll[index]
                {
                    ctx.run_poll().unwrap();
                    poll_started[index] = true;
                    intervals[index] = ctx.poll_interval_ms();
                    next_poll[index] = Instant::now() + Duration::from_millis(intervals[index]);
                }
            }
            maximum_workers = maximum_workers.max(super::task_io::active_workers());
            if started.elapsed() >= Duration::from_secs(2) && initial_execs.is_none() {
                initial_execs = Some(
                    contexts
                        .iter_mut()
                        .map(|ctx| {
                            ctx.ensure_lua()
                                .unwrap()
                                .globals()
                                .get::<u64>("exec_count")
                                .unwrap()
                        })
                        .sum::<u64>(),
                );
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let execs = contexts
            .iter_mut()
            .map(|ctx| {
                ctx.ensure_lua()
                    .unwrap()
                    .globals()
                    .get::<u64>("exec_count")
                    .unwrap()
            })
            .sum::<u64>();
        let monitors = contexts
            .iter_mut()
            .map(|ctx| {
                ctx.ensure_lua()
                    .unwrap()
                    .globals()
                    .get::<u64>("monitor_count")
                    .unwrap()
            })
            .sum::<u64>();
        println!(
            "live_provider_event_profile native={native} providers=3 seconds=5 settled_seconds=3 exec_calls={} monitor_starts={monitors} peak_host_workers={maximum_workers}",
            execs - initial_execs.unwrap()
        );
        if native {
            assert_eq!(monitors, 0);
        }
        if native {
            let lua = contexts[1].ensure_lua().unwrap().clone();
            let counts: std::collections::HashMap<String, u64> =
                mlua::LuaSerdeExt::from_value(&lua, lua.globals().get("signal_counts").unwrap())
                    .unwrap();
            println!("NetworkManager signal counts: {counts:?}");
        }
        for ctx in &mut contexts {
            ctx.call_stop_async().await.unwrap();
            ctx.shutdown_exec();
            ctx.shutdown_streams().await;
        }
    }
}

#[tokio::test]
async fn native_stream_handles_cancel_only_their_own_generation() {
    let root = temp_storage_root("watch-close");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("watched");
    std::fs::write(&path, "initial").unwrap();
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        "@test/watch-close",
        serde_json::json!({ "path": path }),
        mesh_core_capability::CapabilitySet::from_ids([format!("fs.watch:{}", path.display())]),
    );
    ctx.load_script(
        r#"
state = {}
function start(self)
    local source = mesh.watch_path(mesh.config().path)
    state.foreign = mesh.close_stream({ id = source.id, generation = source.generation + 1 })
    state.closed = mesh.close_stream(source)
    state.again = mesh.close_stream(source)
end
"#,
    )
    .unwrap();
    let state = ctx.call_init().unwrap().unwrap();
    assert_eq!(state["foreign"], false);
    assert_eq!(state["closed"], true);
    assert_eq!(state["again"], false);
    ctx.shutdown_streams().await;
    assert_eq!(ctx.stream_state().active_stream_count(), 0);
    drop(ctx);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn suspended_stream_publications_are_private_until_success_and_discarded_on_failure() {
    for fails in [false, true] {
        let mut ctx = BackendScriptContext::new("@test/stream-publications");
        ctx.load_script(&format!(
            r#"
function start(self) end
function on_stream_batch(self, program, lines)
    self.Changed:fire({{ source = "stream" }})
    mesh.service.emit({{ source = "stream" }})
    task.sleep(0)
    if {fails} then error("stream failure") end
end
function on_command_ping(self)
    self.Changed:fire({{ source = "command" }})
    return {{ ok = true }}
end
"#
        ))
        .unwrap();
        ctx.call_init().unwrap();
        ctx.run_stream_batch("test", &["record".into()]).unwrap();
        assert!(ctx.drain_events().is_empty());
        let outcome = ctx
            .run_command_finished("ping", &serde_json::json!({}))
            .unwrap();
        assert!(outcome.state.is_none());
        let events = ctx.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["source"], "command");
        let run = ctx.run_due_tasks(std::time::Instant::now());
        assert_eq!(run.streams.len(), 1);
        let events = ctx.drain_events();
        if fails {
            assert!(run.streams[0].is_err());
            assert!(run.state.is_none());
            assert!(events.is_empty());
        } else {
            assert!(run.streams[0].is_ok());
            assert_eq!(run.state.unwrap()["source"], "stream");
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].payload["source"], "stream");
        }
    }
}

#[test]
fn suspended_publication_bytes_are_bounded_and_cancelled_tasks_release_them() {
    let mut ctx = BackendScriptContext::new("@test/suspended-output-budget");
    ctx.load_script(
        r#"
local handles = {}
local payload = string.rep("x", 32000)
function start(self) end
function on_command_fill(self)
    for index = 1, 64 do
        handles[index] = task.delay(0, function()
            mesh.service.emit({ value = payload })
            task.sleep(10000)
        end)
    end
    return { ok = true }
end
function on_command_clear(self)
    for _, handle in ipairs(handles) do handle:cancel() end
    handles = {}
    return { ok = true }
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    for iteration in 0..2 {
        ctx.run_command_finished("fill", &serde_json::json!({}))
            .unwrap();
        let run = ctx.run_due_tasks(std::time::Instant::now());
        assert!(
            !run.errors.is_empty(),
            "suspended snapshots must share the byte budget: iteration={iteration}, pending={}",
            ctx.pending_task_count()
        );
        assert!(ctx.pending_task_count() > 0);
        ctx.run_command_finished("clear", &serde_json::json!({}))
            .unwrap();
        ctx.run_due_tasks(std::time::Instant::now());
        assert_eq!(ctx.pending_task_count(), 0);
    }
}

#[test]
fn native_network_events_ignore_unexposed_properties_and_keep_state_changes() {
    let mut ctx = BackendScriptContext::new("@test/network-signals");
    ctx.load_script(&bundled_backend_script(
        "../../../../modules/backend/networkmanager-network/src/main.luau",
    ))
    .unwrap();
    ctx.ensure_lua()
        .unwrap()
        .load(
            r#"
mesh.dbus_subscribe = function(...) return { program = "dbus:network" } end
mesh.exec = function(...) return { success = true, stdout = "", stderr = "" } end
"#,
        )
        .exec()
        .unwrap();
    ctx.call_init().unwrap();
    let signal = |fields: serde_json::Value, invalidated: serde_json::Value| {
        serde_json::json!({
            "member": "PropertiesChanged", "body": [
                { "signature": "s", "value": "org.freedesktop.NetworkManager.AccessPoint" },
                { "signature": "a{sv}", "value": fields },
                { "signature": "as", "value": invalidated }
            ]
        })
        .to_string()
    };
    ctx.run_stream_batch(
        "dbus:network",
        &[signal(
            serde_json::json!({"Bitrate": 100, "LastSeen": 42}),
            serde_json::json!([]),
        )],
    )
    .unwrap();
    assert_eq!(ctx.poll_interval_ms(), 60000);
    ctx.run_stream_batch(
        "dbus:network",
        &[signal(
            serde_json::json!({"Strength": 70}),
            serde_json::json!([]),
        )],
    )
    .unwrap();
    assert_eq!(ctx.poll_interval_ms(), 400);
    ctx.run_poll().unwrap();
    assert_eq!(ctx.poll_interval_ms(), 60000);
    ctx.run_stream_batch(
        "dbus:network",
        &[signal(
            serde_json::json!({}),
            serde_json::json!(["Connectivity"]),
        )],
    )
    .unwrap();
    assert_eq!(ctx.poll_interval_ms(), 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn scoped_grants_gate_file_reads_signals_and_property_reads() {
    let root = temp_storage_root("scoped-grants");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("allowed"), "yes").unwrap();
    std::fs::write(root.join("other"), "no").unwrap();
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        "@test/scoped-grants",
        serde_json::json!({ "root": root }),
        mesh_core_capability::CapabilitySet::from_ids([
            format!("fs.read:{}/allowed", root.display()),
            "dbus.subscribe:session:org.mesh.Allowed".to_string(),
            "dbus.read:session:org.mesh.Allowed:org.mesh.Allowed.Device".to_string(),
        ]),
    );
    ctx.load_script(
        r#"
state = {}
local function properties(interface, member)
    return mesh.dbus_call("session", {
        destination = "org.mesh.Allowed", path = "/org/mesh/Allowed",
        interface = "org.freedesktop.DBus.Properties", member = member,
        signature = "s", args = { interface },
    })
end
function start(self)
    local root = mesh.config().root
    state.allowed = mesh.read_file(root .. "/allowed").contents
    state.other = pcall(mesh.read_file, root .. "/other")
    state.escape = pcall(mesh.read_file, root .. "/other/../allowed")
    state.foreign_signals = mesh.dbus_subscribe("session", {
        sender = "org.mesh.Other", path_namespace = "/org/mesh/Other",
    })
    state.read_error = properties("org.mesh.Allowed.Device", "GetAll").error
    state.other_interface_error = properties("org.mesh.Allowed.Secret", "GetAll").error
    state.set_error = properties("org.mesh.Allowed.Device", "Set").error
    state.done = true
end
"#,
    )
    .unwrap();
    let initial = ctx.call_init().unwrap();
    let state = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        if let Some(state) = initial
            && state["done"] == true
        {
            return state;
        }
        loop {
            let BackendTaskRun { state, errors, .. } = ctx.run_due_tasks(std::time::Instant::now());
            assert!(errors.is_empty(), "{errors:?}");
            if let Some(state) = state
                && state["done"] == true
            {
                return state;
            }
            ctx.task_wakeup().notified().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state["allowed"], "yes");
    assert_eq!(state["other"], false);
    assert_eq!(state["escape"], false);
    assert_eq!(state["foreign_signals"], false);
    let denied = |field: &str| {
        state[field]
            .as_str()
            .unwrap_or("")
            .starts_with("missing capability")
    };
    assert!(!denied("read_error"), "{state:?}");
    assert!(denied("other_interface_error"), "{state:?}");
    assert!(denied("set_error"), "{state:?}");
    let _ = std::fs::remove_dir_all(root);
}

// cargo test -p mesh-core-scripting live_dbus_providers_read_state -- --ignored --nocapture
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live UPower and NetworkManager on the system bus"]
async fn live_dbus_providers_read_state() {
    for module in ["upower-power", "networkmanager-network"] {
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(format!(
                "{}/../../../../modules/backend/{module}/module.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .unwrap();
        let grants = manifest["mesh"]["uses"]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|grant| grant.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        let mut ctx = BackendScriptContext::new_with_capabilities(format!("@mesh/{module}"), grants);
        ctx.load_script(&bundled_backend_script(&format!(
            "../../../../modules/backend/{module}/src/main.luau"
        )))
        .unwrap();
        let mut state = ctx.call_init().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.as_ref().is_none_or(|state| state["available"] != true)
            && std::time::Instant::now() < deadline
        {
            let run = ctx.run_due_tasks(std::time::Instant::now());
            assert!(run.errors.is_empty(), "{:?}", run.errors);
            state = run.state.or(state);
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(200),
                ctx.task_wakeup().notified(),
            )
            .await;
        }
        println!("{module}: {state:?}");
        assert!(state.is_some_and(|state| state["available"] == true), "{module}");
    }
}

// cargo test -p mesh-core-scripting live_ext_workspace_records -- --ignored --nocapture
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a Wayland compositor advertising ext-workspace"]
async fn live_ext_workspace_records() {
    let mut ctx = BackendScriptContext::new_with_capabilities(
        "@test/workspaces",
        ["wayland.observe".to_string()],
    );
    ctx.load_script("function start() mesh.wayland_subscribe(\"workspace\") end")
        .unwrap();
    ctx.call_init().unwrap();
    let streams = ctx.stream_state();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut records = Vec::new();
    while tokio::time::Instant::now() < deadline
        && !records.iter().any(|line: &String| line.contains("\"done\""))
    {
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            streams.wait_for_event(),
        )
        .await;
        for event in streams.drain_events() {
            match event.kind {
                StreamEventKind::Line(line) => records.push(line),
                other => println!("{other:?}"),
            }
        }
    }
    for record in &records {
        println!("{record}");
    }
    assert!(records.iter().any(|line| line.contains("\"added\"")));
    ctx.shutdown_streams().await;
}

// cargo test -p mesh-core-scripting live_hyprland_wm_reads_ext_workspaces -- --ignored --nocapture
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a running Hyprland session"]
async fn live_hyprland_wm_reads_ext_workspaces() {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}/../../../../modules/backend/hyprland-wm/module.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap();
    let grants = |key: &str| {
        manifest["mesh"]["uses"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|grant| grant.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let required = grants("capabilities");
    let all = [required.clone(), grants("optionalCapabilities")].concat();
    for (label, granted) in [("required", required), ("required+optional", all)] {
        let mut ctx = BackendScriptContext::new_with_capabilities("@mesh/hyprland-wm", granted);
        ctx.load_script(&bundled_backend_script(
            "../../../../modules/backend/hyprland-wm/src/main.luau",
        ))
        .unwrap();
        let mut state = ctx.call_init().unwrap();
        let socket_granted = label != "required";
        // Every Hyprland workspace has a monitor, which only the request socket reports.
        let has_workspaces = |state: &Option<BackendState>| {
            state.as_ref().is_some_and(|state| {
                state["available"] == true
                    && state["workspaces"].as_array().is_some_and(|list| {
                        !list.is_empty()
                            && list.iter().all(|w| !socket_granted || w["output"].is_string())
                    })
            })
        };
        let mut started = state.is_some();
        let streams = ctx.stream_state();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !(started && has_workspaces(&state)) && std::time::Instant::now() < deadline {
            for event in streams.drain_events() {
                let result = match event.kind {
                    StreamEventKind::Line(line) => {
                        ctx.run_stream_batch_for_stream(&event.stream, &[line])
                    }
                    StreamEventKind::Exited(status) => {
                        ctx.run_stream_closed(&event.stream, status.success)
                    }
                    _ => Ok(None),
                };
                state = result.unwrap().or(state);
            }
            let run = ctx.run_due_tasks(std::time::Instant::now());
            assert!(run.errors.is_empty(), "{:?}", run.errors);
            if let Some(start) = run.start {
                start.unwrap();
                started = true;
            }
            state = run.state.or(state);
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                streams.wait_for_event(),
            )
            .await;
        }
        println!("{label}: {state:?}");
        assert!(started && has_workspaces(&state), "{label}");
        ctx.shutdown_streams().await;
    }
}

#[test]
fn launch_app_exists_only_with_its_capability() {
    let script = "function start()\nend\nfunction on_poll()\nif mesh.launch_app == nil then\nmesh.service.emit({ present = false })\nreturn\nend\nlocal result = mesh.launch_app(\"../escape\")\nmesh.service.emit({ present = true, ok = result.ok, error = result.error })\nend";

    let mut denied = BackendScriptContext::new_with_capabilities("@test/backend", []);
    denied.load_script(script).unwrap();
    let payload = denied.run_poll().unwrap().unwrap();
    assert_eq!(payload.get("present").and_then(|v| v.as_bool()), Some(false));

    let mut granted =
        BackendScriptContext::new_with_capabilities("@test/backend", ["exec.launch-app".into()]);
    granted.load_script(script).unwrap();
    let payload = granted.run_poll().unwrap().unwrap();
    assert_eq!(payload.get("present").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(false));
    assert!(
        payload
            .get("error")
            .and_then(|v| v.as_str())
            .is_some_and(|error| error.contains("invalid desktop file id")),
        "{payload:?}"
    );
}

#[test]
fn http_requires_its_capability_and_backend_start() {
    let script = "function start(self) mesh.http_request({ url = 'http://127.0.0.1:1/' }) end";
    let mut denied = BackendScriptContext::new("@test/http-denied");
    denied.load_script(script).unwrap();
    assert!(
        denied
            .call_init()
            .unwrap_err()
            .to_string()
            .contains("net.http")
    );
    let mut staged =
        BackendScriptContext::new_with_capabilities("@test/http-staged", ["net.http".into()]);
    assert!(
        staged
            .load_script(
                "mesh.http_request({ url = 'http://127.0.0.1:1/' }); function start(self) end"
            )
            .unwrap_err()
            .to_string()
            .contains("before start(self)")
    );
    let mut invalid =
        BackendScriptContext::new_with_capabilities("@test/http-input", ["net.http".into()]);
    invalid
        .load_script("function start(self) mesh.http_request({url = 'file:///etc/passwd'}) end")
        .unwrap();
    assert!(
        invalid
            .call_init()
            .unwrap_err()
            .to_string()
            .contains("HTTP")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn http_yields_the_backend_until_the_bounded_worker_completes() {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let (release, wait) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        stream.read(&mut bytes).unwrap();
        let released = wait.recv_timeout(Duration::from_secs(2)).is_ok();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello")
            .unwrap();
        released
    });
    let mut ctx =
        BackendScriptContext::new_with_capabilities("@test/http-worker", ["net.http".into()]);
    ctx.load_script(&format!("state = {{}}; function start(self) local reply = mesh.http_request({{url = {url:?}}}); state = {{success = reply.success, body = reply.body}} end")).unwrap();
    assert!(
        ctx.call_init().unwrap().is_none(),
        "HTTP startup should suspend"
    );
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let state = loop {
        let run = ctx.run_due_tasks(Instant::now());
        assert!(run.errors.is_empty(), "{:?}", run.errors);
        if let Some(start) = run.start {
            start.unwrap();
            break run.state.unwrap();
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(2)).await;
    };
    assert!(server.join().unwrap(), "HTTP blocked the backend thread");
    assert_eq!(state["success"], true);
    assert_eq!(state["body"], "hello");
}

#[test]
fn bundled_mpris_publishes_playback_before_art_and_keeps_latest_track() {
    let script = bundled_backend_script("../../../../modules/backend/mpris-media/src/main.luau");
    let mut ctx =
        BackendScriptContext::new_with_capabilities("@mesh/mpris-media", ["net.http".into()]);
    let contract: serde_json::Value = serde_json::from_str(&bundled_backend_script(
        "../../../../modules/interfaces/media/module.json",
    ))
    .unwrap();
    let contract = mesh_core_service::contract::parse_interface_contract(
        "mesh.media",
        "1.0",
        &contract["mesh"]["interface"]["contract"],
    )
    .unwrap();
    ctx.set_event_registry(BackendEventRegistry::from_contract(&contract));
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua().unwrap().load(r#"
        sample_line = "Playing\tplayer\t200000000\t42000000\thttps://example.test/first.png\tArtist\tAlbum\tFirst"
        downloads = 0
        streams = {}
        mesh.exec_stream = function(program, args)
            streams[#streams + 1] = args[2]
            return { id = #streams, program = program }
        end
        last_args = {}
        mesh.exec = function(program, args)
            last_args = args
            return { success = true, stdout = sample_line }
        end
        mesh.http_request = function(spec)
            downloads += 1
            return { success = true, path = "/cache/" .. spec.url:match("([^/]+)$") }
        end
    "#).exec().unwrap();
    let initial = ctx.call_init().unwrap().unwrap();
    assert_eq!(initial["title"], "First");
    assert_eq!(initial["state"], "playing");
    assert_eq!(initial["position"], 42.0);
    assert_eq!(initial["art_path"], "");
    assert_eq!(
        ctx.ensure_lua()
            .unwrap()
            .globals()
            .get::<u32>("downloads")
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.ensure_lua()
            .unwrap()
            .globals()
            .get::<Table>("streams")
            .unwrap()
            .raw_len(),
        2
    );
    let latest = ctx.run_stream_batch("playerctl", &[
        "Playing\tplayer\t200000000\t1000000\thttps://example.test/old.png\tArtist\tAlbum\tOld".into(),
        "Paused\tplayer\t200000000\t9000000\thttps://example.test/latest.png\tArtist\tAlbum\tLatest".into(),
    ]).unwrap().unwrap();
    assert_eq!(latest["title"], "Latest");
    assert_eq!(latest["state"], "paused");
    assert_eq!(latest["position"], 9.0);
    let run = ctx.run_due_tasks(std::time::Instant::now() + std::time::Duration::from_millis(10));
    assert!(run.errors.is_empty(), "{:?}", run.errors);
    let art = run.state.unwrap();
    assert_eq!(art["title"], "Latest");
    assert_eq!(art["state"], "paused");
    assert_eq!(art["art_path"], "/cache/latest.png");
    assert_eq!(
        ctx.ensure_lua()
            .unwrap()
            .globals()
            .get::<u32>("downloads")
            .unwrap(),
        1
    );
    ctx.ensure_lua().unwrap().load(r#"
        sample_line = "Playing\tplayer\t200000000\t10000000\tfile:///art/local%20cover.png\tArtist\tAlbum\tLatest"
    "#).exec().unwrap();
    let resumed = ctx
        .run_stream_batch("playerctl", &["Playing".into()])
        .unwrap()
        .unwrap();
    assert_eq!(resumed["state"], "playing");
    let args = ctx.ensure_lua().unwrap().globals().get::<Table>("last_args").unwrap();
    assert_eq!(args.get::<String>(1).unwrap(), "--player");
    assert_eq!(args.get::<String>(2).unwrap(), "player");
    let command = ctx.run_command_finished("play-pause", &serde_json::json!({})).unwrap();
    assert_eq!(command.result["ok"], true);

    assert_eq!(resumed["art_path"], "/art/local cover.png");
    let gone = ctx
        .run_stream_batch("playerctl", &["".into()])
        .unwrap()
        .unwrap();
    assert_eq!(gone["available"], false);
}

#[test]
fn bundled_cava_discards_superseded_frames_and_clears_on_exit() {
    let script =
        bundled_backend_script("../../../../modules/backend/cava-visualizer/src/main.luau");
    let mut ctx = BackendScriptContext::new("@mesh/cava-visualizer");
    ctx.load_script(&script).unwrap();
    ctx.ensure_lua()
        .unwrap()
        .load(
            r#"
        mesh.exec_stream = function(program) return { id = 1, program = program } end
    "#,
        )
        .exec()
        .unwrap();
    ctx.call_init().unwrap();
    let frame = |level: u32| {
        (0..24)
            .map(|_| level.to_string())
            .collect::<Vec<_>>()
            .join(";")
    };
    let idle = ctx.run_stream_batch("sh", &[frame(0)]).unwrap().unwrap();
    assert_eq!(idle["available"], true);
    assert_eq!(idle["active"], false);
    let latest = ctx
        .run_stream_batch("sh", &[frame(1000), frame(250)])
        .unwrap()
        .unwrap();
    assert_eq!(latest["active"], true);
    assert_eq!(latest["bars"][0], 0.25);
    let silent = ctx.run_stream_batch("sh", &[frame(0)]).unwrap().unwrap();
    assert_eq!(silent["active"], false);
    assert_eq!(silent["bars"][0], 0.0);
    let handle = ctx.stream_state().register("sh".into(), vec![]).unwrap();
    let closed = ctx.run_stream_closed(&handle, false).unwrap().unwrap();
    assert_eq!(closed["available"], false);
    assert_eq!(ctx.poll_interval_ms(), 5000);
}

#[tokio::test]
#[ignore = "requires a live MPRIS player and playerctl"]
async fn live_mpris_publishes_playerctl_position() {
    let manifest: serde_json::Value = serde_json::from_str(&bundled_backend_script(
        "../../../../modules/backend/mpris-media/module.json",
    ))
    .unwrap();
    let grants = manifest["mesh"]["uses"]["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let mut ctx = BackendScriptContext::new_with_capabilities("@mesh/mpris-media", grants);
    ctx.load_script(&bundled_backend_script(
        "../../../../modules/backend/mpris-media/src/main.luau",
    ))
    .unwrap();
    let mut initial = ctx.call_init().unwrap();
    while ctx.start_pending() {
        let run = settle(&mut ctx).await;
        assert!(run.errors.is_empty(), "{:?}", run.errors);
        initial = run.state.or(initial);
    }
    let initial = initial.unwrap();
    println!("live MPRIS initial: {initial:?}");
    assert_eq!(initial["available"], true);
    let mut state = ctx.run_poll().unwrap();
    while ctx.poll_in_flight() {
        let run = settle(&mut ctx).await;
        assert!(run.errors.is_empty(), "{:?}", run.errors);
        state = run.state.or(state);
    }
    let state = state.unwrap();
    println!("live MPRIS poll: {state:?}");
    let player = state["player"].as_str().unwrap();
    let sampled = std::process::Command::new("playerctl")
        .args(["--player", player, "position"])
        .output()
        .unwrap();
    assert!(sampled.status.success());
    let actual = String::from_utf8(sampled.stdout)
        .unwrap()
        .trim()
        .parse::<f64>()
        .unwrap();
    let reported = state["position"].as_f64().unwrap();
    assert!(
        (actual - reported).abs() < 2.0,
        "playerctl={actual}, provider={reported}"
    );
    ctx.call_stop_async().await.unwrap();
    ctx.shutdown_streams().await;
}
