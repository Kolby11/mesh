use super::common::*;
use super::*;

#[test]
fn service_contract_provider_declaration_requires_provider_pair() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.audio": "@mesh/backend" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, module) = module_instance("@mesh/backend", Some("src/main.luau"));
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);
    let interfaces = InterfaceRegistry::new();
    interfaces.register_contract(test_contract("mesh.audio"));

    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_settings(), &interfaces);

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "invalid_manifest"
            && status.provider_id.as_deref() == Some("@mesh/backend")
            && status.message.contains("not registered")
    }));

    register_test_provider(&interfaces, "mesh.audio", "@mesh/backend");
    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_settings(), &interfaces);

    assert_eq!(candidates.len(), 1);
    assert!(
        statuses
            .iter()
            .all(|status| status.provider_id.as_deref() != Some("@mesh/backend"))
    );
}

#[test]
fn backend_lifecycle_accepts_provider_without_consumer_capabilities() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.example": "@mesh/backend" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.example", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, module) = module_instance("@mesh/backend", Some("src/main.luau"));
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);
    let interfaces = InterfaceRegistry::new();
    let mut contract = test_contract("mesh.example");
    contract.capabilities.required = vec!["service.example.read".to_string()];
    interfaces.register_contract(contract);
    register_test_provider(&interfaces, "mesh.example", "@mesh/backend");

    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_settings(), &interfaces);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].interface, "mesh.example");
    assert!(
        statuses
            .iter()
            .all(|status| status.status != "missing_capability")
    );
}

#[test]
fn backend_lifecycle_accepts_valid_provider_with_contract() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.example": "@mesh/backend" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "capabilities": { "required": ["exec.argv:example:*"] },
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.example", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, mut module) = module_instance("@mesh/backend", Some("src/main.luau"));
    module.manifest.capabilities.required = vec!["exec.argv:example:*".to_string()];
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);
    let interfaces = InterfaceRegistry::new();
    let mut contract = test_contract("mesh.example");
    contract.capabilities.required = vec!["service.example.read".to_string()];
    interfaces.register_contract(contract);
    register_test_provider(&interfaces, "mesh.example", "@mesh/backend");

    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_settings(), &interfaces);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].interface, "mesh.example");
    assert_eq!(
        candidates[0].capabilities,
        vec!["exec.argv:example:*".to_string()]
    );
    assert!(
        statuses
            .iter()
            .all(|status| status.status != "missing_capability")
    );
}

#[test]
fn state_shape_mismatch_records_service_contract_warning() {
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0, "muted": false }),
        ))
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": "loud" }),
        ))
        .unwrap();

    let snapshot = shell.diagnostics.snapshot();
    assert!(snapshot.iter().any(|entry| {
        entry.module_id == "@mesh/pipewire-audio"
            && entry
                .health
                .to_string()
                .contains("service_contract_warning")
    }));
    assert_eq!(
        shell.latest_service_state["mesh.audio"].state["percent"],
        serde_json::json!(40.0),
        "invalid state must not replace the last-known-good snapshot"
    );
}

#[test]
fn service_contract_unknown_service_command_returns_failure_result() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "explode",
        &serde_json::json!({}),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(false));
    assert_eq!(
        result["status"],
        serde_json::json!("unsupported_service_command")
    );
    assert!(rx.try_recv().is_err());
    assert!(shell.diagnostics.snapshot().iter().any(|entry| {
        entry.module_id == "@mesh/panel"
            && entry
                .health
                .to_string()
                .contains("unsupported_service_command")
    }));
}

#[test]
fn service_command_dispatch_records_debug_method_call() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 40 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    let command = rx.try_recv().unwrap();
    assert_eq!(command.command, "set_volume");
    assert_eq!(
        command.call_id.raw(),
        result["call_id"]
            .as_u64()
            .expect("dispatch returns call id")
    );
    let snapshot = shell.build_debug_snapshot();
    assert!(snapshot.method_calls.iter().any(|entry| {
        entry.call_id == command.call_id.raw()
            && entry.interface == "mesh.audio"
            && entry.provider_id.as_deref() == Some("@mesh/pipewire-audio")
            && entry.source_module_id == "@mesh/panel"
            && entry.command == "set_volume"
            && entry.status == "queued"
            && entry.queued
    }));
}

#[test]
fn service_command_rejects_invalid_contract_payload_before_queueing() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_muted",
        &serde_json::json!({ "device_id": "default", "muted": "yes" }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(false));
    assert_eq!(
        result["status"],
        serde_json::json!("invalid_service_command_payload")
    );
    assert!(rx.try_recv().is_err());
    assert!(shell.diagnostics.snapshot().iter().any(|entry| {
        entry.module_id == "@mesh/panel"
            && entry
                .health
                .to_string()
                .contains("invalid_service_command_payload")
    }));
}

#[test]
fn backend_command_result_records_debug_method_result() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    // Only the interface's *active* provider records a result, so a stale
    // provider's late reply cannot rewrite the debug history.
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut pending = VecDeque::new();

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendCommandResult {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                call_id: mesh_core_backend::CallId::from_raw(0),
                command: "set_volume".to_string(),
                result: serde_json::json!({ "ok": true, "percent": 40 }),
                outcome: mesh_core_backend::BackendCommandOutcome::Completed,
            },
        )
        .unwrap();

    let snapshot = shell.build_debug_snapshot();
    assert!(snapshot.method_calls.iter().any(|entry| {
        entry.interface == "mesh.audio"
            && entry.provider_id.as_deref() == Some("@mesh/pipewire-audio")
            && entry.source_module_id == "<backend>"
            && entry.command == "set_volume"
            && entry.status == "completed"
            && !entry.queued
    }));
}

#[test]
fn backend_command_result_rejects_invalid_contract_output() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut pending = VecDeque::new();

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendCommandResult {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                call_id: mesh_core_backend::CallId::from_raw(0),
                command: "set_volume".to_string(),
                result: serde_json::json!({ "ok": "yes" }),
                outcome: mesh_core_backend::BackendCommandOutcome::Completed,
            },
        )
        .unwrap();

    assert!(shell.diagnostics.snapshot().iter().any(|entry| {
        entry.module_id == "@mesh/pipewire-audio"
            && entry
                .health
                .to_string()
                .contains("invalid_service_command_result")
    }));
    let snapshot = shell.build_debug_snapshot();
    assert!(snapshot.method_calls.iter().any(|entry| {
        entry.command == "set_volume"
            && entry.status == "failed"
            && entry
                .error
                .as_deref()
                .is_some_and(|error| error.contains("expected Result"))
    }));
}

#[test]
fn backend_interface_event_validates_and_delivers_to_components() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));

    let requests = shell
        .broadcast_backend_interface_event(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "VolumeChanged".to_string(),
            serde_json::json!({ "device_id": "default", "level": 42.0 }),
        )
        .unwrap();

    assert!(requests.is_empty());
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    let ServiceEvent::InterfaceEvent {
        service,
        source_module,
        name,
        payload,
    } = events.last().unwrap()
    else {
        panic!("expected interface event");
    };
    assert_eq!(service, "mesh.audio");
    assert_eq!(source_module, "@mesh/pipewire-audio");
    assert_eq!(name, "VolumeChanged");
    assert_eq!(payload["level"], serde_json::json!(42.0));
}

#[test]
fn backend_interface_event_drops_invalid_payload_with_diagnostic() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));

    shell
        .broadcast_backend_interface_event(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "VolumeChanged".to_string(),
            serde_json::json!({ "device_id": "default", "level": "loud" }),
        )
        .unwrap();

    assert!(events.lock().unwrap().is_empty());
    assert!(shell.diagnostics.snapshot().iter().any(|entry| {
        entry.module_id == "@mesh/pipewire-audio"
            && entry
                .health
                .to_string()
                .contains("payload field 'level' expected float")
    }));
}

#[test]
fn terminal_provider_interface_events_are_dropped_after_stop() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));

    shell
        .broadcast_backend_interface_event(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "VolumeChanged".to_string(),
            serde_json::json!({ "device_id": "default", "level": 42.0 }),
        )
        .unwrap();
    shell.stop_backend_runtime("mesh.audio");
    shell
        .broadcast_backend_interface_event(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "VolumeChanged".to_string(),
            serde_json::json!({ "device_id": "default", "level": 5.0 }),
        )
        .unwrap();

    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, ServiceEvent::InterfaceEvent { name, .. } if name == "VolumeChanged"))
            .count(),
        1
    );
    assert!(events.iter().any(|event| {
        matches!(
            event,
            ServiceEvent::Updated { payload, .. }
                if payload.get("available") == Some(&serde_json::Value::Bool(false))
        )
    }));
}

#[test]
fn closed_service_command_channel_returns_unavailable_result() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    drop(rx);
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 40 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(false));
    assert_eq!(result["status"], serde_json::json!("service_unavailable"));
}

#[test]
fn profiling_service_command_attributes_active_provider_dispatch() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 40 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(
        rx.try_recv().unwrap().command,
        "set_volume",
        "the existing command dispatch path must stay intact"
    );
    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("active provider dispatch should be attributed");
    let stage = backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::CommandHandling)
        .expect("command-handling stage should be recorded");
    assert_eq!(stage.sample_count, 1);
}

#[test]
fn profiling_service_command_stays_silent_when_disabled() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 20 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_volume");
    assert!(
        shell.build_debug_snapshot().profiling.is_none(),
        "command attribution must stay inert while profiling is disabled"
    );
}
