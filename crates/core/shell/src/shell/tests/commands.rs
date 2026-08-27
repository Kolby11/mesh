use super::common::*;
use super::*;
use mesh_core_backend::BackendIdentity;

#[test]
fn set_muted_command_broadcasts_bound_audio_state_until_backend_confirms() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": true }),
        ))
        .unwrap();
    events.lock().unwrap().clear();

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_muted",
        &serde_json::json!({ "device_id": "default", "muted": false }),
        "@mesh/audio-popover",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(result["state_bound"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_muted");
    assert_eq!(
        events.lock().unwrap().last().and_then(|event| match event {
            ServiceEvent::Updated { payload, .. } => payload.get("muted").cloned(),
            ServiceEvent::InterfaceEvent { .. } => None,
        }),
        Some(serde_json::json!(false)),
        "bound set_muted(false) should update frontend consumers immediately"
    );

    let delivered_events = events.lock().unwrap().len();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/stale-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": false }),
        ))
        .unwrap();
    assert_eq!(
        events.lock().unwrap().len(),
        delivered_events,
        "inactive providers must not deliver audio state while set_muted is pending"
    );
    assert_eq!(
        shell
            .pending_bound_service_state
            .get(&("mesh.audio".to_string(), "muted".to_string()))
            .map(|pending| pending.optimistic.clone()),
        Some(serde_json::json!(false)),
        "inactive provider updates must not clear pending mute state"
    );

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": true }),
        ))
        .unwrap();
    assert_eq!(
        events.lock().unwrap().last().and_then(|event| match event {
            ServiceEvent::Updated { payload, .. } => payload.get("muted").cloned(),
            ServiceEvent::InterfaceEvent { .. } => None,
        }),
        Some(serde_json::json!(false)),
        "stale backend muted=true must not flip UI while set_muted(false) is pending"
    );

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": false }),
        ))
        .unwrap();
    assert_eq!(
        shell
            .pending_bound_service_state
            .get(&("mesh.audio".to_string(), "muted".to_string())),
        None,
        "matching backend confirmation should clear pending mute state"
    );
}

#[test]
fn set_volume_updates_canonical_audio_percent_until_backend_confirms() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let mut contract = test_contract("mesh.audio");
    contract.methods[0].args = vec![
        InterfaceArgument {
            name: "device_id".to_string(),
            arg_type: "string".to_string(),
        },
        InterfaceArgument {
            name: "percent".to_string(),
            arg_type: "float".to_string(),
        },
    ];
    contract.methods[0].state_binding = Some(mesh_core_service::StateBinding {
        field: "percent".to_string(),
        from_arg: Some("percent".to_string()),
        toggle: false,
    });
    shell.interfaces.register_contract(contract);
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let settings_state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let navigation_state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    for (id, state) in [
        ("@mesh/settings", Arc::clone(&settings_state)),
        ("@mesh/navigation-bar", Arc::clone(&navigation_state)),
    ] {
        shell.register_component(Box::new(IndexedRecordingComponent::new(
            id,
            Arc::new(Mutex::new(Some(ServiceObservationSummary {
                update_services: vec!["audio".to_string()],
                cached_update_services: Vec::new(),
                interface_events: Vec::new(),
            }))),
            state,
        )));
    }
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": false }),
        ))
        .unwrap();
    settings_state.lock().unwrap().handled.clear();
    navigation_state.lock().unwrap().handled.clear();

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "device_id": "default", "percent": 73 }),
        "@mesh/settings",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(result["state_bound"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_volume");
    assert_eq!(
        shell.latest_service_state["mesh.audio"].state["percent"],
        serde_json::json!(73),
        "the shell-owned audio state should change as soon as any surface dispatches set_volume"
    );
    for state in [&settings_state, &navigation_state] {
        assert_eq!(
            state
                .lock()
                .unwrap()
                .handled
                .last()
                .and_then(|event| match event {
                    ServiceEvent::Updated { payload, .. } => payload.get("percent").cloned(),
                    ServiceEvent::InterfaceEvent { .. } => None,
                }),
            Some(serde_json::json!(73)),
            "settings and navigation should receive the shared bound percent"
        );
    }

    let mut pending = VecDeque::new();
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
                    serde_json::json!({ "available": true, "percent": 42.0, "muted": false }),
                ),
            },
        )
        .unwrap();
    assert_eq!(
        shell.latest_service_state["mesh.audio"].state["percent"],
        serde_json::json!(73),
        "a stale provider snapshot must not roll back the shared value"
    );

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
                    serde_json::json!({ "available": true, "percent": 73.0, "muted": false }),
                ),
            },
        )
        .unwrap();
    assert!(
        !shell
            .pending_bound_service_state
            .contains_key(&("mesh.audio".to_string(), "percent".to_string())),
        "matching provider state should confirm and clear the bound value"
    );
}

#[test]
fn failed_bound_write_rolls_back_and_older_failure_cannot_override_newer_write() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let mut contract = test_contract("mesh.audio");
    contract.methods[0].args = vec![InterfaceArgument {
        name: "percent".to_string(),
        arg_type: "float".to_string(),
    }];
    contract.methods[0].state_binding = Some(mesh_core_service::StateBinding {
        field: "percent".to_string(),
        from_arg: Some("percent".to_string()),
        toggle: false,
    });
    shell.interfaces.register_contract(contract);
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0 }),
        ))
        .unwrap();
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let first = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 60 }),
        "@mesh/settings",
        &capabilities,
    );
    let first_command = rx.try_recv().unwrap();
    assert_eq!(first["ok"], serde_json::json!(true));

    let second = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 73 }),
        "@mesh/settings",
        &capabilities,
    );
    let second_command = rx.try_recv().unwrap();
    assert_eq!(second["ok"], serde_json::json!(true));
    assert_eq!(
        shell.latest_service_state["mesh.audio"].state["percent"],
        serde_json::json!(73)
    );

    let mut pending = VecDeque::new();
    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendCommandResult {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                identity: BackendIdentity::default(),
                generation: 0,
                call_id: first_command.call_id,
                command: "set_volume".to_string(),
                result: serde_json::json!({
                    "ok": false,
                    "status": "failed",
                    "error": "rejected",
                }),
                outcome: mesh_core_backend::BackendCommandOutcome::Failed,
            },
        )
        .unwrap();
    assert_eq!(
        shell.latest_service_state["mesh.audio"].state["percent"],
        serde_json::json!(73),
        "an older failure must not roll back a newer optimistic write"
    );

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendCommandResult {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                identity: BackendIdentity::default(),
                generation: 0,
                call_id: second_command.call_id,
                command: "set_volume".to_string(),
                result: serde_json::json!({
                    "ok": false,
                    "status": "failed",
                    "error": "rejected",
                }),
                outcome: mesh_core_backend::BackendCommandOutcome::Failed,
            },
        )
        .unwrap();
    assert_eq!(
        shell.latest_service_state["mesh.audio"].state["percent"],
        serde_json::json!(42.0),
        "when both writes fail, rollback must reach the last provider value"
    );
    assert!(
        !shell
            .pending_bound_service_state
            .contains_key(&("mesh.audio".to_string(), "percent".to_string()))
    );
}

#[test]
fn command_state_binding_updates_non_audio_service() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let interface = "mesh.lighting";
    let provider = "@mesh/test-lighting";
    let mut contract = test_contract(interface);
    contract.state_fields[2].name = "enabled".to_string();
    contract.methods[1].name = "set_enabled".to_string();
    contract.methods[1].args[1].name = "enabled".to_string();
    contract.methods[1].state_binding = Some(mesh_core_service::StateBinding {
        field: "enabled".to_string(),
        from_arg: Some("enabled".to_string()),
        toggle: false,
    });
    shell.interfaces.register_contract(contract);
    register_test_provider(&shell.interfaces, interface, provider);
    let (slot, mut rx) = backend_runtime_slot(&runtime, interface, provider);
    shell.replace_backend_runtime(interface.to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.lighting.control",
    ));

    shell
        .broadcast_service_event(service_update(
            interface,
            provider,
            serde_json::json!({ "available": true, "percent": 0.0, "enabled": false }),
        ))
        .unwrap();
    let result = shell.dispatch_service_command(
        interface,
        "set_enabled",
        &serde_json::json!({ "device_id": "desk", "enabled": true }),
        "@mesh/lighting-controls",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(result["state_bound"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_enabled");
    assert_eq!(
        shell.latest_service_state[interface].state["enabled"],
        serde_json::json!(true)
    );

    shell
        .broadcast_service_event(service_update(
            interface,
            provider,
            serde_json::json!({ "available": true, "percent": 0.0, "enabled": false }),
        ))
        .unwrap();
    assert_eq!(
        shell.latest_service_state[interface].state["enabled"],
        serde_json::json!(true),
        "a stale provider update must retain the generic state binding"
    );

    shell
        .broadcast_service_event(service_update(
            interface,
            provider,
            serde_json::json!({ "available": true, "percent": 0.0, "enabled": true }),
        ))
        .unwrap();
    assert!(
        !shell
            .pending_bound_service_state
            .contains_key(&(interface.to_string(), "enabled".to_string())),
        "matching provider confirmation must clear the generic state binding"
    );
}

/// A shell-provided interface has no backend command queue, so the dispatcher
/// must answer it directly. Before core interfaces were routed, this returned
/// `service_unavailable` because `service_handlers` only knows Luau backends.
/// It later returned `capability_denied` for every caller, because the
/// contract-authorized path asked for a bare `theme.control` grant that no
/// manifest, capability registry entry, or spec line has ever used.
#[test]
fn a_core_provided_command_is_applied_by_the_shell_itself() {
    let dir = tempfile::tempdir().unwrap();
    let _settings = isolated_settings_file(dir.path());
    std::fs::write(
        dir.path().join("theme.css"),
        ":root { --color-surface: #FFF; }",
    )
    .unwrap();
    let mut shell = Shell::new();
    shell.installed_module_graph = Some(graph_with_theme_source(
        dir.path(),
        "mesh-default-light",
        "theme.css",
    ));
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.theme.control",
    ));
    let before = shell.theme.active().id.clone();

    let result = shell.dispatch_service_command(
        "mesh.theme",
        "set_theme",
        &serde_json::json!({ "theme_id": "@mesh/test-theme:mesh-default-light" }),
        "@mesh/any-module-at-all",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(result["status"], serde_json::json!("applied"));
    assert_eq!(
        shell.theme.active().id,
        "@mesh/test-theme:mesh-default-light"
    );
    assert_ne!(
        shell.theme.active().id,
        before,
        "the command must change real shell state, not just report success"
    );
}

#[test]
fn coalescable_service_commands_use_a_backend_cost_budget() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let mut contract = test_contract("mesh.audio");
    contract.methods[0].coalesce = true;
    shell.interfaces.register_contract(contract);
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let first = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 40 }),
        "@mesh/navigation-bar",
        &capabilities,
    );
    assert_eq!(first["ok"], serde_json::json!(true));
    let first_command = rx.try_recv().unwrap();
    assert_eq!(first_command.payload["percent"], serde_json::json!(40));
    assert_eq!(
        first_command.call_id.raw(),
        first["call_id"].as_u64().expect("first call id")
    );

    let second = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "percent": 75 }),
        "@mesh/navigation-bar",
        &capabilities,
    );
    assert_eq!(second["throttled"], serde_json::json!(true));
    assert!(rx.try_recv().is_err());

    std::thread::sleep(std::time::Duration::from_millis(110));
    shell.flush_throttled_commands();
    let second_command = rx.try_recv().unwrap();
    assert_eq!(second_command.payload["percent"], serde_json::json!(75));
    assert_eq!(
        second_command.call_id.raw(),
        second["call_id"].as_u64().expect("second call id")
    );
    assert_ne!(first_command.call_id, second_command.call_id);
}

/// The capability is the whole gate. No module id is consulted, so a
/// third-party settings frontend has exactly the same reach as `@mesh/settings`.
#[test]
fn a_core_provided_command_without_its_capability_is_denied() {
    let mut shell = Shell::new();
    let before = shell.theme.active().id.clone();

    let result = shell.dispatch_service_command(
        "mesh.theme",
        "set_theme",
        &serde_json::json!({ "theme_id": "mesh-default-light" }),
        "@mesh/settings",
        &mesh_core_capability::CapabilitySet::new(),
    );

    assert_eq!(result["ok"], serde_json::json!(false));
    assert_eq!(result["error"], serde_json::json!("capability_denied"));
    assert_eq!(
        shell.theme.active().id,
        before,
        "a denied command must not change shell state"
    );
}

/// Argument extraction is strict: a payload that does not match the declared
/// contract is an unsupported command, not a command applied with a default.
#[test]
fn a_core_provided_command_with_a_malformed_payload_changes_nothing() {
    let mut shell = Shell::new();
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.theme.control",
    ));
    let before = shell.theme.active().id.clone();

    let result = shell.dispatch_service_command(
        "mesh.theme",
        "set_theme",
        &serde_json::json!({ "theme_id": "   " }),
        "@mesh/settings",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(false));
    assert_eq!(shell.theme.active().id, before);
}

#[test]
fn file_watcher_stopped_message_clears_active_flag_and_forces_immediate_reload_checks() {
    let mut shell = Shell::new();
    shell.file_watcher_active = true;
    let far_future = std::time::Instant::now() + std::time::Duration::from_secs(3600);
    shell.next_theme_reload_check = far_future;
    shell.next_shell_settings_reload_check = far_future;
    shell.next_frontend_reload_check = far_future;

    let mut pending = VecDeque::new();
    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::FileWatcherStopped { generation: 0 },
        )
        .unwrap();

    assert!(
        !shell.file_watcher_active,
        "a dead watch thread must not be trusted to keep reporting changes"
    );
    let now = std::time::Instant::now();
    assert!(shell.next_theme_reload_check <= now);
    assert!(shell.next_shell_settings_reload_check <= now);
    assert!(shell.next_frontend_reload_check <= now);
}

#[test]
fn file_watcher_stopped_message_is_a_noop_when_the_watcher_was_never_active() {
    let mut shell = Shell::new();
    assert!(!shell.file_watcher_active);
    let far_future = std::time::Instant::now() + std::time::Duration::from_secs(3600);
    shell.next_theme_reload_check = far_future;

    let mut pending = VecDeque::new();
    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::FileWatcherStopped { generation: 0 },
        )
        .unwrap();

    assert_eq!(
        shell.next_theme_reload_check, far_future,
        "an already-inactive watcher must not disturb existing poll scheduling"
    );
}

#[test]
fn retired_file_watch_generation_cannot_trigger_a_reload() {
    let mut shell = Shell::new();
    shell.file_watch_set = crate::shell::file_watch::WatchSet::new(2, Vec::new());
    let far_future = std::time::Instant::now() + std::time::Duration::from_secs(3600);
    shell.next_theme_reload_check = far_future;
    shell.next_shell_settings_reload_check = far_future;
    shell.next_frontend_reload_check = far_future;

    let mut pending = VecDeque::new();
    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::FilesystemChanged { generation: 1 },
        )
        .unwrap();

    assert_eq!(shell.next_theme_reload_check, far_future);
    assert_eq!(shell.next_shell_settings_reload_check, far_future);
    assert_eq!(shell.next_frontend_reload_check, far_future);
    assert!(pending.is_empty());
}

#[test]
fn file_watcher_status_is_generation_scoped() {
    let mut shell = Shell::new();
    shell.file_watch_set = crate::shell::file_watch::WatchSet::new(2, Vec::new());
    shell.file_watcher_active = true;

    let mut pending = VecDeque::new();
    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::FileWatcherStatus {
                generation: 1,
                active: false,
                watched_paths: 0,
            },
        )
        .unwrap();
    assert!(shell.file_watcher_active);

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::FileWatcherStatus {
                generation: 2,
                active: false,
                watched_paths: 0,
            },
        )
        .unwrap();
    assert!(!shell.file_watcher_active);
}
