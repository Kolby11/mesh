use super::common::*;
use super::*;
use mesh_core_backend::BackendIdentity;

#[test]
fn latest_service_state_tracks_provider_metadata_separately() {
    let mut shell = Shell::new();

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 65.0, "muted": false }),
        ))
        .unwrap();

    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(latest.provider_id, "@mesh/pipewire-audio");
    assert_eq!(latest.generation, 1);
    assert_eq!(latest.state["available"], serde_json::json!(true));
    assert!(latest.state.get("source_module").is_none());
}

#[test]
fn identical_service_state_is_deduplicated_before_delivery() {
    let mut shell = Shell::new();
    let event = service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 65.0, "muted": false }),
    );

    assert!(shell.record_latest_service_state(&event));
    assert!(!shell.record_latest_service_state(&event));

    let changed = service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 66.0, "muted": false }),
    );
    assert!(shell.record_latest_service_state(&changed));
}

#[test]
fn provider_swap_replaces_interface_latest_state() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (pipewire_slot, _pipewire_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), pipewire_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    let (pulse_slot, _pulse_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), pulse_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pulseaudio-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();

    assert_eq!(shell.latest_service_state.len(), 1);
    assert!(shell.latest_service_state.contains_key("mesh.audio"));
    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(latest.interface, "mesh.audio");
    assert_eq!(latest.provider_id, "@mesh/pulseaudio-audio");
    assert!(latest.generation >= 2);
    assert_eq!(latest.state["percent"], serde_json::json!(55.0));
    assert!(
        !shell
            .latest_service_state
            .values()
            .any(|latest| latest.provider_id == "@mesh/pipewire-audio")
    );
}

#[test]
fn stale_provider_update_does_not_replace_current_latest_state() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (pipewire_slot, _pipewire_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), pipewire_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    let (pulse_slot, _pulse_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), pulse_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pulseaudio-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 5.0 }),
        ))
        .unwrap();

    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(latest.provider_id, "@mesh/pulseaudio-audio");
    assert_eq!(latest.state["percent"], serde_json::json!(55.0));
}

#[test]
fn stale_provider_update_does_not_reach_components() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    let (new_slot, _new_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pulseaudio-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 5.0 }),
        ))
        .unwrap();

    let events = seen_events.lock().unwrap();
    assert_eq!(events.len(), 2);
    let ServiceEvent::Updated {
        source_module,
        payload,
        ..
    } = &events[0]
    else {
        panic!("expected first service update");
    };
    assert_eq!(source_module, "@mesh/pipewire-audio");
    assert_eq!(payload["percent"], serde_json::json!(40.0));
    let ServiceEvent::Updated {
        source_module,
        payload,
        ..
    } = events.last().unwrap()
    else {
        panic!("expected last service update");
    };
    assert_eq!(source_module, "@mesh/pulseaudio-audio");
    assert_eq!(payload["percent"], serde_json::json!(55.0));
}

#[test]
fn profiling_backend_poll_update_attributes_accepted_backend_messages() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut pending = std::collections::VecDeque::new();

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                identity: BackendIdentity::default(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/pipewire-audio",
                    serde_json::json!({ "available": true, "percent": 44.0 }),
                ),
            },
        )
        .unwrap();

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("accepted backend updates should record provider-attributed profiling");
    let stage = backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::PollUpdate)
        .expect("poll/update stage should be recorded for accepted backend work");
    assert_eq!(stage.sample_count, 1);
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.audio")
            .unwrap()
            .provider_id,
        "@mesh/pipewire-audio"
    );
}

#[test]
fn profiling_backend_poll_update_ignores_stale_backend_messages() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);
    let mut pending = std::collections::VecDeque::new();

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/old-audio".to_string(),
                identity: BackendIdentity::default(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/old-audio",
                    serde_json::json!({ "available": true, "percent": 12.0 }),
                ),
            },
        )
        .unwrap();

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert!(
        profiling.backends.is_empty(),
        "stale backend updates must not create poll/update samples"
    );
}

#[test]
fn profiling_state_publish_delivery_attributes_accepted_service_updates() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    assert_eq!(seen_events.lock().unwrap().len(), 1);
    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("accepted service updates should record backend publish/delivery profiling");
    let stage = backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::StatePublishDelivery)
        .expect("publish/delivery stage should be recorded for accepted service updates");
    assert_eq!(stage.sample_count, 1);
    assert!(
        stage
            .recent_samples
            .iter()
            .all(|sample| sample.trigger_kind.as_deref() == Some("broadcast_service_event"))
    );
}

#[test]
fn profiling_state_publish_delivery_ignores_stale_service_updates() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/old-audio",
            serde_json::json!({ "available": true, "percent": 12.0 }),
        ))
        .unwrap();

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert!(
        profiling.backends.is_empty(),
        "stale service updates must not create publish/delivery samples"
    );
}

#[test]
fn terminal_provider_update_does_not_replace_latest_state_or_reach_components() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    shell.stop_backend_runtime("mesh.audio");
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 5.0 }),
        ))
        .unwrap();

    let events = seen_events.lock().unwrap();
    assert!(events.iter().any(|event| {
        matches!(
            event,
            ServiceEvent::Updated { payload, .. }
                if payload.get("available") == Some(&serde_json::Value::Bool(false))
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            ServiceEvent::InterfaceEvent { name, payload, .. }
                if name == "health" && payload["state"] == "unavailable"
        )
    }));
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.audio")
            .and_then(|state| state.state.get("percent")),
        Some(&serde_json::json!(40.0))
    );
}

#[test]
fn service_update_populates_frontend_state() {
    let mut state = ScriptState::new();
    seed_service_state(&mut state);
    apply_service_update(
        &mut state,
        true,
        "audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 65, "label": "65%" }),
    );

    let audio = state.get("audio").expect("audio state should exist");
    assert_eq!(audio.get("label").and_then(|v| v.as_str()), Some("65%"));
    assert_eq!(audio.get("percent").and_then(|v| v.as_u64()), Some(65));
}

#[test]
fn service_update_gated_by_capability() {
    let mut state = ScriptState::new();
    seed_service_state(&mut state);
    apply_service_update(
        &mut state,
        false, // no audio.read capability
        "audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 99 }),
    );
    assert!(state.get("audio").is_none());
}

#[test]
fn service_update_accepts_canonical_interface_name() {
    let mut state = ScriptState::new();
    seed_service_state(&mut state);
    apply_service_update(
        &mut state,
        true,
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 42 }),
    );
    assert_eq!(
        state
            .get("last_service_update")
            .and_then(|v| v.get("name").cloned())
            .and_then(|v| v.as_str().map(str::to_string)),
        Some("audio".to_string())
    );
}

#[test]
fn normalizes_service_names_from_interfaces() {
    assert_eq!(service_name_from_interface("mesh.audio"), "audio");
    assert_eq!(service_name_from_interface("audio"), "audio");
}
