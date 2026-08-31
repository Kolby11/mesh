use super::common::*;
use super::*;
use crate::shell::backend::BackendSupervisionState;
use mesh_core_backend::BackendIdentity;
use mesh_core_module::{ModuleHealthState, ModuleState};

#[test]
fn backend_supervision_quarantines_provider_after_exhausted_restart_cycles() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (_dir, module) = module_instance("@mesh/pipewire-audio", Some("src/main.luau"));
    shell
        .modules
        .insert("@mesh/pipewire-audio".to_string(), module);

    // Each terminal failure of the current provider schedules a supervised
    // restart; after the restart budget is exhausted the provider is
    // quarantined for the session.
    for cycle in 0..3u32 {
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), slot);
        shell.handle_backend_lifecycle(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "poll".to_string(),
            "failed".to_string(),
            format!("boom {cycle}"),
        );
        let state = shell.backend_supervision.get("mesh.audio").unwrap();
        assert_eq!(state.restart_count, cycle + 1);
        assert!(state.quarantined_providers.is_empty());
    }

    // Fourth consecutive failure exceeds the restart budget: quarantine.
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "boom final".to_string(),
    );
    let state = shell.backend_supervision.get("mesh.audio").unwrap();
    assert!(
        state.quarantined_providers.contains("@mesh/pipewire-audio"),
        "provider should be quarantined after exhausting restart cycles"
    );
    assert_eq!(
        state.restart_count, 0,
        "failover restarts with a fresh budget"
    );
    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/pipewire-audio")
            .map(|entry| entry.status),
        Some(BackendRuntimeStatus::Quarantined)
    );
    let module = shell.module("@mesh/pipewire-audio").unwrap();
    assert_eq!(module.state, ModuleState::Quarantined);
    assert_eq!(module.health().state, ModuleHealthState::Unavailable);
}

#[test]
fn backend_status_updates_authoritative_module_lifecycle_and_health() {
    let mut shell = Shell::new();
    let (_dir, module) = module_instance("@mesh/pipewire-audio", Some("src/main.luau"));
    shell
        .modules
        .insert("@mesh/pipewire-audio".to_string(), module);

    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );
    let module = shell.module("@mesh/pipewire-audio").unwrap();
    assert_eq!(module.state, ModuleState::Running);
    assert_eq!(module.health().state, ModuleHealthState::Healthy);

    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Failed,
        "poll loop stopped".to_string(),
    );
    let module = shell.module("@mesh/pipewire-audio").unwrap();
    assert_eq!(module.state, ModuleState::Errored);
    assert_eq!(module.health().state, ModuleHealthState::Unavailable);

    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "provider recovered".to_string(),
    );
    let module = shell.module("@mesh/pipewire-audio").unwrap();
    assert_eq!(module.state, ModuleState::Running);
    assert_eq!(module.health().state, ModuleHealthState::Healthy);
}

#[test]
fn backend_failure_delivers_unavailable_state_and_health_event() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(Arc::clone(&seen_events))));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    shell.record_latest_service_state(&service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 75 }),
    ));

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "poll loop stopped".to_string(),
    );

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
}

#[test]
fn committed_provider_generation_publishes_recovery_and_rejects_stale_failure() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(Arc::clone(&seen_events))));

    let old_identity = BackendIdentity::new(7, 1);
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/audio");
    *old_slot
        .identity
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = old_identity;
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    shell.record_latest_service_state_at_identity(
        &service_update(
            "mesh.audio",
            "@mesh/audio",
            serde_json::json!({ "available": true, "percent": 75 }),
        ),
        old_identity,
    );

    shell.handle_backend_lifecycle_at_identity(
        "mesh.audio".to_string(),
        "@mesh/audio".to_string(),
        old_identity,
        "poll".to_string(),
        "failed".to_string(),
        "provider stopped".to_string(),
    );
    let failure_events = seen_events.lock().unwrap().clone();
    assert!(failure_events.iter().any(|event| {
        matches!(
            event,
            ServiceEvent::InterfaceEvent { name, payload, .. }
                if name == "health" && payload["state"] == "unavailable"
        )
    }));
    seen_events.lock().unwrap().clear();

    let new_identity = BackendIdentity::new(8, 2);
    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/audio");
    *new_slot
        .identity
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = new_identity;
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);
    shell.record_latest_service_state_at_identity(
        &service_update(
            "mesh.audio",
            "@mesh/audio",
            serde_json::json!({ "percent": 80 }),
        ),
        new_identity,
    );
    shell.publish_backend_health_at_identity(
        "mesh.audio",
        "@mesh/audio",
        new_identity,
        BackendRuntimeStatus::Running,
        "provider recovered",
        true,
    );

    let recovery_events = seen_events.lock().unwrap().clone();
    assert!(recovery_events.iter().any(|event| {
        matches!(
            event,
            ServiceEvent::Updated { source_module, payload, .. }
                if source_module == "@mesh/audio"
                    && payload["available"] == true
                    && payload["percent"] == 80
                    && payload.get("availability_reason").is_none()
        )
    }));
    assert!(recovery_events.iter().any(|event| {
        matches!(
            event,
                ServiceEvent::InterfaceEvent { source_module, name, payload, .. }
                if source_module == "@mesh/audio"
                    && name == "health"
                    && payload["state"] == "healthy"
        )
    }));
    assert_eq!(
        shell.latest_service_health_identities["mesh.audio"],
        new_identity
    );
    let recovery_event_count = recovery_events.len();
    shell.publish_backend_health_at_identity(
        "mesh.audio",
        "@mesh/audio",
        new_identity,
        BackendRuntimeStatus::Running,
        "same generation heartbeat",
        true,
    );
    assert_eq!(
        seen_events.lock().unwrap().len(),
        recovery_event_count,
        "repeated health status from one committed generation is not a transition"
    );

    seen_events.lock().unwrap().clear();
    shell.handle_backend_lifecycle_at_identity(
        "mesh.audio".to_string(),
        "@mesh/audio".to_string(),
        old_identity,
        "poll".to_string(),
        "failed".to_string(),
        "stale provider stopped".to_string(),
    );
    assert!(seen_events.lock().unwrap().is_empty());
    let latest_health = shell.latest_service_health.get("mesh.audio").unwrap();
    assert!(matches!(
        latest_health,
        ServiceEvent::InterfaceEvent { source_module, payload, .. }
            if source_module == "@mesh/audio" && payload["state"] == "healthy"
    ));
}

#[test]
fn uncommitted_provider_failure_does_not_publish_availability() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(Arc::clone(&seen_events))));

    let identity = BackendIdentity::new(11, 1);
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/candidate");
    *slot
        .identity
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = identity;
    shell.stage_backend_runtime_activation("mesh.audio".to_string(), slot);
    shell.handle_backend_lifecycle_at_identity(
        "mesh.audio".to_string(),
        "@mesh/candidate".to_string(),
        identity,
        "init".to_string(),
        "failed".to_string(),
        "candidate failed before commit".to_string(),
    );

    // A status record can outlive the candidate slot during cleanup. Its
    // identity alone must not make that uncommitted generation observable.
    shell.record_backend_runtime_status_at_identity(
        "mesh.audio".to_string(),
        "@mesh/candidate".to_string(),
        identity,
        BackendRuntimeStatus::Failed,
        "late candidate failure".to_string(),
    );

    assert!(seen_events.lock().unwrap().is_empty());
    assert!(!shell.latest_service_state.contains_key("mesh.audio"));
    assert!(!shell.latest_service_health.contains_key("mesh.audio"));
}

#[test]
fn uncommitted_provider_failure_only_marks_a_module_failed_without_a_current_runtime() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (_dir, module) = module_instance("@mesh/candidate", Some("src/main.luau"));
    shell.modules.insert("@mesh/candidate".to_string(), module);

    let identity = BackendIdentity::new(11, 1);
    let (candidate, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/candidate");
    *candidate
        .identity
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = identity;
    shell.stage_backend_runtime_activation("mesh.audio".to_string(), candidate);

    // Starting a candidate is not a module lifecycle transition. The graph
    // and the live runtime remain authoritative until this candidate commits.
    assert_eq!(
        shell.module("@mesh/candidate").unwrap().state,
        ModuleState::Discovered
    );
    shell.handle_backend_lifecycle_at_identity(
        "mesh.audio".to_string(),
        "@mesh/candidate".to_string(),
        identity,
        "init".to_string(),
        "failed".to_string(),
        "candidate failed before commit".to_string(),
    );

    let module = shell.module("@mesh/candidate").unwrap();
    assert_eq!(module.state, ModuleState::Errored);
    assert_eq!(module.health().state, ModuleHealthState::Unavailable);

    // A late event from the retired candidate cannot recover the module.
    shell.record_backend_runtime_status_at_identity(
        "mesh.audio".to_string(),
        "@mesh/candidate".to_string(),
        identity,
        BackendRuntimeStatus::Running,
        "late candidate recovery".to_string(),
    );
    let module = shell.module("@mesh/candidate").unwrap();
    assert_eq!(module.state, ModuleState::Errored);
    assert_eq!(module.health().state, ModuleHealthState::Unavailable);
}

#[test]
fn failed_provider_candidate_preserves_the_current_module_lifecycle() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (_dir, module) = module_instance("@mesh/audio", Some("src/main.luau"));
    shell.modules.insert("@mesh/audio".to_string(), module);

    let old_identity = BackendIdentity::new(10, 1);
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/audio");
    *old_slot
        .identity
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = old_identity;
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    shell.handle_backend_lifecycle_at_identity(
        "mesh.audio".to_string(),
        "@mesh/audio".to_string(),
        old_identity,
        "runtime".to_string(),
        "running".to_string(),
        "current provider is ready".to_string(),
    );

    let candidate_identity = BackendIdentity::new(11, 2);
    let (candidate, _candidate_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/audio");
    *candidate
        .identity
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = candidate_identity;
    shell.stage_backend_runtime_activation("mesh.audio".to_string(), candidate);
    shell.handle_backend_lifecycle_at_identity(
        "mesh.audio".to_string(),
        "@mesh/audio".to_string(),
        candidate_identity,
        "init".to_string(),
        "failed".to_string(),
        "replacement failed before commit".to_string(),
    );

    let module = shell.module("@mesh/audio").unwrap();
    assert_eq!(module.state, ModuleState::Running);
    assert_eq!(module.health().state, ModuleHealthState::Healthy);
    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/audio")
    );
}

#[test]
fn stale_restart_deadline_cannot_clear_or_run_a_newer_supervision_token() {
    let mut shell = Shell::new();
    shell.backend_supervision.insert(
        "mesh.audio".to_string(),
        BackendSupervisionState {
            restart_count: 1,
            restart_pending: true,
            pending_provider_id: Some("@mesh/old-audio".to_string()),
            pending_identity: BackendIdentity::default(),
            restart_generation: 7,
            running_since: None,
            quarantined_providers: std::collections::HashSet::new(),
        },
    );

    shell.handle_backend_restart_due("mesh.audio", "@mesh/new-audio", 7);
    let state = shell.backend_supervision.get("mesh.audio").unwrap();
    assert!(state.restart_pending);
    assert_eq!(
        state.pending_provider_id.as_deref(),
        Some("@mesh/old-audio")
    );

    shell.handle_backend_restart_due("mesh.audio", "@mesh/old-audio", 7);
    let state = shell.backend_supervision.get("mesh.audio").unwrap();
    assert!(!state.restart_pending);
    assert!(state.pending_provider_id.is_none());
}

#[test]
fn backend_lifecycle_uses_explicit_active_provider_from_package_graph() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph = mesh_core_module::package::load_installed_module_graph(
        &workspace_root.join("config/module.json"),
    )
    .unwrap();
    let (_pipewire_dir, pipewire) = module_instance("@mesh/pipewire-audio", Some("src/main.luau"));
    let (_pulse_dir, pulse) = module_instance("@mesh/pulseaudio-audio", Some("src/main.luau"));
    let (_upower_dir, upower) = module_instance("@mesh/upower-power", Some("src/main.luau"));
    let (_brightness_dir, brightness) =
        module_instance("@mesh/backlight-brightness", Some("src/main.luau"));
    let (_hyprland_dir, hyprland) = module_instance("@mesh/hyprland-wm", Some("src/main.luau"));
    let modules = HashMap::from([
        ("@mesh/pipewire-audio".to_string(), pipewire),
        ("@mesh/pulseaudio-audio".to_string(), pulse),
        ("@mesh/upower-power".to_string(), upower),
        ("@mesh/backlight-brightness".to_string(), brightness),
        ("@mesh/hyprland-wm".to_string(), hyprland),
    ]);

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_settings(),
        &ResolvedServiceCatalogHandle::new(),
    );

    assert!(
        statuses
            .iter()
            .all(|status| status.status != "invalid_manifest")
    );
    assert_eq!(candidates.len(), 4);
    let audio = candidates
        .iter()
        .find(|candidate| candidate.interface == "mesh.audio")
        .unwrap();
    assert_eq!(audio.module_id, "@mesh/pipewire-audio");
    assert_eq!(audio.service_name, "audio");
    assert!(audio.entrypoint_path.ends_with("src/main.luau"));
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.interface == "mesh.power"
                && candidate.module_id == "@mesh/upower-power")
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.interface == "mesh.wm"
                && candidate.module_id == "@mesh/hyprland-wm")
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.interface == "mesh.brightness"
                && candidate.module_id == "@mesh/backlight-brightness")
    );
    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.module_id == "@mesh/pulseaudio-audio")
    );
}

#[test]
fn backend_lifecycle_never_falls_back_to_an_unselected_discovered_provider() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/selected": { "kind": "backend", "path": "@mesh/selected", "enabled": true },
                "@mesh/fallback": { "kind": "backend", "path": "@mesh/fallback", "enabled": true }
              },
              "providers": { "mesh.example": "@mesh/selected" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/selected",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.example", "provider": "selected" }]
                  }
                }"#,
            r#"{
                  "name": "@mesh/fallback",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.example", "provider": "fallback" }]
                  }
                }"#,
        ],
    );
    // Only the unselected provider has a discovered runtime manifest. A
    // discovery-driven compatibility lane would incorrectly launch it.
    let (_fallback_dir, fallback) = module_instance("@mesh/fallback", Some("src/main.luau"));
    let modules = HashMap::from([("@mesh/fallback".to_string(), fallback)]);
    let interfaces = ResolvedServiceCatalogHandle::new();
    interfaces.register_contract(test_contract("mesh.example"));
    register_test_provider(&interfaces, "mesh.example", "@mesh/selected");
    register_test_provider(&interfaces, "mesh.example", "@mesh/fallback");

    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_settings(), &interfaces);

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "invalid_manifest"
            && status.provider_id.as_deref() == Some("@mesh/selected")
            && status.message.contains("no discovered runtime manifest")
    }));
    assert!(
        statuses
            .iter()
            .all(|status| status.provider_id.as_deref() != Some("@mesh/fallback")),
        "the graph-selected provider must never fall back to discovery order"
    );
}

#[test]
fn backend_lifecycle_rejects_missing_backend_entrypoint_before_launch() {
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
                    "implements": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, module) = module_instance("@mesh/backend", None);
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_settings(),
        &ResolvedServiceCatalogHandle::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "missing_entrypoint"
            && status.provider_id.as_deref() == Some("@mesh/backend")
    }));
}

#[test]
fn backend_lifecycle_rejects_escaping_backend_entrypoint_before_read() {
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
                    "implements": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, mut module) = module_instance("@mesh/backend", None);
    module.manifest.entrypoints.main = Some("../outside.luau".into());
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_settings(),
        &ResolvedServiceCatalogHandle::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "missing_entrypoint"
            && status.provider_id.as_deref() == Some("@mesh/backend")
            && status.message.contains("contained")
    }));
}

#[cfg(unix)]
#[test]
fn backend_lifecycle_rejects_symlinked_backend_entrypoint_before_read() {
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
                    "implements": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, mut module) = module_instance("@mesh/backend", None);
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("main.luau"), "return 'outside'").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("main.luau"),
        module.path.join("escape.luau"),
    )
    .unwrap();
    module.manifest.entrypoints.main = Some("escape.luau".into());
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_settings(),
        &ResolvedServiceCatalogHandle::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "missing_entrypoint"
            && status.provider_id.as_deref() == Some("@mesh/backend")
            && status.message.contains("symlink")
    }));
}

#[test]
fn backend_lifecycle_excludes_disabled_backend_modules() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true },
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": false }
              },
              "providers": {}
            }"#,
        vec![
            r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.audio": ">=1.0.0" } }
                  }
                }"#,
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

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_settings(),
        &ResolvedServiceCatalogHandle::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "unmet_backend_requirement" && status.interface == "mesh.audio"
    }));
}

#[test]
fn backend_lifecycle_reports_frontend_requirement_without_active_provider() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true },
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": {}
            }"#,
        vec![
            r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.audio": ">=1.0.0" } }
                  }
                }"#,
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

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_settings(),
        &ResolvedServiceCatalogHandle::new(),
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].interface, "mesh.audio");
    assert!(
        statuses
            .iter()
            .all(|status| status.status != "no_active_provider")
    );
}

#[test]
fn backend_lifecycle_reports_frontend_requirement_without_installed_provider() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true }
              },
              "providers": {}
            }"#,
        vec![
            r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.network": ">=1.0.0" } }
                  }
                }"#,
        ],
    );

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &HashMap::new(),
        &test_settings(),
        &ResolvedServiceCatalogHandle::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "unmet_backend_requirement" && status.interface == "mesh.network"
    }));
}

#[test]
fn backend_lifecycle_status_names_match_phase_contract() {
    let statuses = [
        BackendRuntimeStatus::NoActiveProvider,
        BackendRuntimeStatus::UnmetBackendRequirement,
        BackendRuntimeStatus::InvalidManifest,
        BackendRuntimeStatus::MissingCapability,
        BackendRuntimeStatus::MissingEntrypoint,
        BackendRuntimeStatus::MissingBinary,
        BackendRuntimeStatus::InitFailed,
        BackendRuntimeStatus::Running,
        BackendRuntimeStatus::PollFailed,
        BackendRuntimeStatus::Failed,
        BackendRuntimeStatus::Stopped,
    ]
    .map(BackendRuntimeStatus::as_str);

    assert_eq!(
        statuses,
        [
            "no_active_provider",
            "unmet_backend_requirement",
            "invalid_manifest",
            "missing_capability",
            "missing_entrypoint",
            "missing_binary",
            "init_failed",
            "running",
            "poll_failed",
            "failed",
            "stopped",
        ]
    );
}

#[test]
fn backend_lifecycle_replacement_removes_old_command_sender_before_insert() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    let old_sender = old_slot.command_tx.clone();
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    let new_sender = new_slot.command_tx.clone();
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    assert!(!old_sender.is_closed());
    assert!(!new_sender.is_closed());
    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/new-audio")
    );
    assert!(shell.service_handlers.contains_key("mesh.audio"));
}

#[test]
fn backend_lifecycle_replacement_records_stopped_after_transient_poll_failure() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/old-audio".to_string(),
        BackendRuntimeStatus::PollFailed,
        "temporary poll failure".to_string(),
    );

    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/old-audio")
            .map(|entry| entry.status.as_str()),
        Some("stopped")
    );
}

#[test]
fn pending_provider_init_failure_keeps_current_runtime_active() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (candidate, _candidate_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.stage_backend_runtime_switch(
        "mesh.audio".to_string(),
        candidate,
        PathBuf::from("unused-module.json"),
    );

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/new-audio".to_string(),
        "init".to_string(),
        "init_failed".to_string(),
        "candidate initialization failed".to_string(),
    );

    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/old-audio")
    );
    assert!(!shell.pending_backend_runtimes.contains_key("mesh.audio"));
    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/new-audio")
            .map(|entry| entry.status),
        Some(BackendRuntimeStatus::InitFailed)
    );
}

#[test]
fn stateful_provider_waits_for_valid_initial_snapshot_before_activation() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (candidate, _candidate_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.stage_backend_runtime_activation("mesh.audio".to_string(), candidate);

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/new-audio".to_string(),
        "runtime".to_string(),
        "running".to_string(),
        "backend runtime started".to_string(),
    );
    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/old-audio")
    );

    shell
        .handle_shell_message(
            &mut VecDeque::new(),
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/new-audio".to_string(),
                identity: BackendIdentity::default(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/new-audio",
                    serde_json::json!({
                        "available": true,
                        "percent": 55.0,
                        "muted": false,
                    }),
                ),
            },
        )
        .unwrap();

    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/new-audio")
    );
    assert_eq!(
        shell.latest_service_state["mesh.audio"].state["percent"],
        serde_json::json!(55.0)
    );
}

#[test]
fn invalid_prepared_snapshot_keeps_current_provider_active() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (candidate, _candidate_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.stage_backend_runtime_activation("mesh.audio".to_string(), candidate);

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/new-audio".to_string(),
        "runtime".to_string(),
        "running".to_string(),
        "backend runtime started".to_string(),
    );
    shell
        .handle_shell_message(
            &mut VecDeque::new(),
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/new-audio".to_string(),
                identity: BackendIdentity::default(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/new-audio",
                    serde_json::json!({ "available": true, "percent": "unknown" }),
                ),
            },
        )
        .unwrap();

    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/old-audio")
    );
    assert!(!shell.pending_backend_runtimes.contains_key("mesh.audio"));
    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/new-audio")
            .map(|entry| entry.status),
        Some(BackendRuntimeStatus::Failed)
    );
}

#[test]
fn ready_provider_is_persisted_before_live_runtime_handoff() {
    let runtime = Runtime::new().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let graph_path = directory.path().join("module.json");
    fs::write(
        &graph_path,
        r#"{
  "name": "@mesh/test-config",
  "version": "0.1.0",
  "mesh": {
    "schemaVersion": 1,
    "modulesDir": "modules",
    "providers": {"mesh.audio": "@mesh/old-audio"}
  }
}"#,
    )
    .unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (candidate, _candidate_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.stage_backend_runtime_switch("mesh.audio".to_string(), candidate, graph_path.clone());

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/new-audio".to_string(),
        "runtime".to_string(),
        "running".to_string(),
        "backend runtime started".to_string(),
    );

    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(graph_path).unwrap()).unwrap();
    assert_eq!(saved["mesh"]["providers"]["mesh.audio"], "@mesh/new-audio");
    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/new-audio")
    );
    assert!(!shell.pending_backend_runtimes.contains_key("mesh.audio"));
}

#[test]
fn ready_provider_write_failure_keeps_current_runtime_active() {
    let runtime = Runtime::new().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (candidate, _candidate_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.stage_backend_runtime_switch(
        "mesh.audio".to_string(),
        candidate,
        directory.path().join("missing-module.json"),
    );

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/new-audio".to_string(),
        "runtime".to_string(),
        "running".to_string(),
        "backend runtime started".to_string(),
    );

    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/old-audio")
    );
    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/new-audio")
            .map(|entry| entry.status),
        Some(BackendRuntimeStatus::Failed)
    );
}

#[test]
fn frontend_module_deactivation_removes_runtime_and_destroys_surface() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    shell.register_component(Box::new(RecordingComponent::new(Arc::new(Mutex::new(
        Vec::new(),
    )))));

    let requests = shell
        .deactivate_frontend_module("@test/recording", None)
        .unwrap();

    assert!(requests.is_empty());
    assert!(shell.components.is_empty());
    assert!(!shell.component_by_surface.contains_key("@test/recording"));
    assert_eq!(
        shell.presentation_engine.testing_destroyed_surfaces(),
        &["@test/recording".to_string()]
    );
}

#[test]
fn legacy_graph_delta_uses_the_activation_coordinator_commit_boundary() {
    let mut shell = Shell::new();
    shell.active_profile_id = None;
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {},
              "providers": {}
            }"#,
        Vec::new(),
    );

    shell.activate_graph_candidate(graph.clone());
    for _ in 0..500 {
        if shell.pending_resource_preparation.is_none() && shell.pending_profile_switch.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        shell.poll_pending_resource_preparation();
    }

    assert!(shell.pending_resource_preparation.is_none());
    assert!(shell.pending_profile_switch.is_none());
    assert_eq!(
        shell.installed_module_graph.as_ref().unwrap().diff(&graph),
        Default::default()
    );
    assert_eq!(
        shell
            .active_snapshot()
            .as_ref()
            .map(|snapshot| snapshot.generation()),
        Some(1)
    );
}

#[test]
fn newly_active_backend_interfaces_spawns_only_the_active_unrunning_provider() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/selected": { "kind": "backend", "path": "@mesh/selected", "enabled": true },
                "@mesh/fallback": { "kind": "backend", "path": "@mesh/fallback", "enabled": true },
                "@mesh/other": { "kind": "backend", "path": "@mesh/other", "enabled": true }
              },
              "providers": {
                "mesh.example": "@mesh/selected",
                "mesh.other": "@mesh/other"
              }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/selected",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.example", "provider": "selected" }]
                  }
                }"#,
            r#"{
                  "name": "@mesh/fallback",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.example", "provider": "fallback" }]
                  }
                }"#,
            r#"{
                  "name": "@mesh/other",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.other", "provider": "other" }]
                  }
                }"#,
        ],
    );

    // `@mesh/fallback` implements `mesh.example` but is not the graph's
    // selected provider: enabling it must never spawn anything.
    assert!(
        shell
            .newly_active_backend_interfaces(&graph, "@mesh/fallback")
            .is_empty()
    );

    // `@mesh/other` is the active provider for `mesh.other`, but a runtime is
    // already live for it: enabling it again must not respawn it.
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.other", "@mesh/other");
    shell.replace_backend_runtime("mesh.other".to_string(), slot);
    assert!(
        shell
            .newly_active_backend_interfaces(&graph, "@mesh/other")
            .is_empty()
    );

    // `@mesh/selected` is the active provider for `mesh.example` and has no
    // live runtime: enabling it must report that interface to spawn.
    assert_eq!(
        shell.newly_active_backend_interfaces(&graph, "@mesh/selected"),
        vec!["mesh.example".to_string()]
    );
}

#[test]
fn frontend_module_activation_mounts_shipped_surface_live() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    shell.discover_modules();
    shell.resolve_modules().unwrap();
    let graph = shell.load_installed_module_graph_cached().unwrap().clone();

    let mut requests = shell
        .activate_frontend_module("@mesh/settings", &graph)
        .unwrap();

    assert!(!requests.is_empty());
    shell.drain_requests(&mut requests).unwrap();
    assert!(
        shell
            .components
            .iter()
            .any(|runtime| runtime.component.id() == "@mesh/settings")
    );
    assert!(shell.component_by_surface.contains_key("@mesh/settings"));

    let surface = shell.surfaces.get_mut("@mesh/settings").unwrap();
    surface.width = 420;
    surface.height = 1200;
    let mut requests = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@mesh/settings".into(),
        })
        .unwrap();
    shell.drain_requests(&mut requests).unwrap();
    shell.render_components().unwrap();
    let mut requests = shell.publish_debug_snapshot().unwrap();
    shell.drain_requests(&mut requests).unwrap();
    shell.render_components().unwrap();

    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/settings")
        .unwrap();
    let buffer = runtime
        .parent
        .paint_buffer
        .as_ref()
        .expect("settings surface should have a full-shell paint buffer");
    let opaque_pixels = buffer
        .data()
        .chunks_exact(4)
        .filter(|pixel| pixel[3] != 0)
        .count();
    assert!(
        opaque_pixels > 20_000,
        "settings surface should paint substantial visible content, got {opaque_pixels} opaque pixels"
    );
    assert!(
        shell
            .presentation_engine
            .testing_presented_surfaces()
            .iter()
            .any(|surface| surface == "@mesh/settings")
    );
    let config = runtime
        .parent
        .last_surface_config
        .as_ref()
        .expect("settings surface should be configured through the shell renderer");
    assert_eq!(config.surface_size().0, 920);
    // `@mesh/settings` is `promotable` and *starts* as chrome, so this is the
    // layer-surface shape: the 700px CSS-measured root plus the 200px tooltip
    // overlay reserve. Popping it out into a window drops that reserve, because
    // a toplevel's size is its content size — the compositor pins, decorates,
    // and tiles by it, so a padded buffer would make the window measurably
    // larger than the UI inside it.
    assert_eq!(config.surface_size().1, 900);
    assert_eq!(config.role, mesh_core_wayland::SurfaceRole::Layer);
}

#[test]
fn changing_ui_font_repaints_the_mounted_settings_surface() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    shell.discover_modules();
    shell.resolve_modules().unwrap();
    let graph = shell.load_installed_module_graph_cached().unwrap().clone();

    let mut requests = shell
        .activate_frontend_module("@mesh/settings", &graph)
        .unwrap();
    shell.drain_requests(&mut requests).unwrap();

    let surface = shell.surfaces.get_mut("@mesh/settings").unwrap();
    surface.width = 420;
    surface.height = 1200;
    let mut requests = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@mesh/settings".into(),
        })
        .unwrap();
    shell.drain_requests(&mut requests).unwrap();
    shell.render_components().unwrap();

    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/settings")
        .unwrap();
    let frame = runtime
        .component
        .frontend_frame()
        .expect("settings should publish a frontend frame");
    let tree = frame.tree().expect("settings should publish a frame tree");
    let normal_text_nodes = tree
        .nodes()
        .iter()
        .filter(|node| node.tag() == "text" && !node.style().explicit_properties.font_family)
        .map(|node| (node.identity().clone(), node.style().font_family.clone()))
        .collect::<Vec<_>>();
    assert!(
        !normal_text_nodes.is_empty(),
        "settings should contain inherited text nodes to exercise global typography"
    );
    let initial_family = normal_text_nodes[0].1.to_string();
    let selected_family = shell
        .resource_snapshot
        .host_catalog
        .font_families
        .iter()
        .map(|family| family.name.as_str())
        .find(|family| *family != initial_family)
        .expect("host catalog should expose a second font family")
        .to_owned();

    let mut requests = shell
        .apply_request(CoreRequest::SetFontFamily {
            family: selected_family.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut requests).unwrap();
    shell.render_components().unwrap();

    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/settings")
        .unwrap();
    let frame = runtime
        .component
        .frontend_frame()
        .expect("settings should publish its updated frontend frame");
    let tree = frame
        .tree()
        .expect("settings should publish its updated frame tree");
    let updated_node = tree
        .node(&normal_text_nodes[0].0)
        .expect("the normal text node should survive a font-only update");
    let updated_family = updated_node.style().font_family.to_string();
    assert_eq!(
        updated_family, selected_family,
        "initial={initial_family}, selected={selected_family}"
    );
}

#[test]
fn backend_lifecycle_init_failure_removes_command_handler() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "init".to_string(),
        "init_failed".to_string(),
        "init boom".to_string(),
    );

    assert!(!shell.service_handlers.contains_key("mesh.audio"));
    assert!(!shell.backend_runtimes.contains_key("mesh.audio"));
    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/pipewire-audio")
            .map(|entry| entry.status.as_str()),
        Some("init_failed")
    );
}

#[test]
fn stale_backend_lifecycle_event_does_not_stop_current_provider() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/old-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "old provider failed after replacement".to_string(),
    );

    assert!(shell.service_handlers.contains_key("mesh.audio"));
    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/new-audio")
    );
    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/old-audio")
            .map(|entry| entry.status.as_str()),
        Some("failed")
    );
}

#[test]
fn stale_provider_epoch_update_is_rejected_for_same_provider_id() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let old_identity = BackendIdentity::new(3, 1);
    let current_identity = BackendIdentity::new(3, 2);

    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/audio");
    *old_slot
        .identity
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = old_identity;
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

    let (current_slot, _current_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/audio");
    *current_slot
        .identity
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = current_identity;
    shell.replace_backend_runtime("mesh.audio".to_string(), current_slot);

    shell
        .handle_shell_message(
            &mut VecDeque::new(),
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/audio".to_string(),
                identity: current_identity,
                event: service_update(
                    "mesh.audio",
                    "@mesh/audio",
                    serde_json::json!({ "available": true, "percent": 90.0 }),
                ),
            },
        )
        .unwrap();
    shell
        .handle_shell_message(
            &mut VecDeque::new(),
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/audio".to_string(),
                identity: old_identity,
                event: service_update(
                    "mesh.audio",
                    "@mesh/audio",
                    serde_json::json!({ "available": true, "percent": 10.0 }),
                ),
            },
        )
        .unwrap();

    assert_eq!(
        shell.latest_service_state["mesh.audio"].state["percent"],
        serde_json::json!(90.0)
    );
}

#[test]
fn stale_provider_epoch_restart_deadline_cannot_restart_same_provider() {
    let mut shell = Shell::new();
    shell.activation_generation = 5;
    shell.backend_supervision.insert(
        "mesh.audio".to_string(),
        BackendSupervisionState {
            restart_count: 1,
            restart_pending: true,
            pending_provider_id: Some("@mesh/audio".to_string()),
            pending_identity: BackendIdentity::new(5, 2),
            restart_generation: 7,
            running_since: None,
            quarantined_providers: std::collections::HashSet::new(),
        },
    );

    shell.handle_backend_restart_due_at_identity(
        "mesh.audio",
        "@mesh/audio",
        BackendIdentity::new(5, 1),
        7,
    );

    let state = shell.backend_supervision.get("mesh.audio").unwrap();
    assert!(state.restart_pending);
    assert_eq!(state.pending_provider_id.as_deref(), Some("@mesh/audio"));
    assert_eq!(state.pending_identity, BackendIdentity::new(5, 2));
}

#[test]
fn backend_lifecycle_failed_runtime_does_not_start_fallback_provider() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "poll boom".to_string(),
    );

    assert!(!shell.service_handlers.contains_key("mesh.audio"));
    assert!(
        !shell
            .backend_runtimes
            .values()
            .any(|slot| slot.provider_id == "@mesh/pulseaudio-audio")
    );
    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/pipewire-audio")
            .map(|entry| entry.status.as_str()),
        Some("failed")
    );
}

#[test]
fn debug_snapshot_includes_backend_lifecycle_status() {
    let mut shell = Shell::new();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );

    let snapshot = shell.build_debug_snapshot();
    assert!(snapshot.backend_runtimes.iter().any(|entry| {
        entry.interface == "mesh.audio"
            && entry.provider_id == "@mesh/pipewire-audio"
            && entry.status == "running"
    }));
}

#[test]
fn backend_lifecycle_debug_snapshot_includes_failure_counts() {
    let mut shell = Shell::new();
    // Record three poll failures for the same provider.
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::PollFailed,
        "poll failure 1".to_string(),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::PollFailed,
        "poll failure 2".to_string(),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::PollFailed,
        "poll failure 3".to_string(),
    );

    let snapshot = shell.build_debug_snapshot();
    let entry = snapshot
        .backend_runtimes
        .iter()
        .find(|e| e.interface == "mesh.audio" && e.provider_id == "@mesh/pipewire-audio")
        .expect("backend runtime entry must be present in debug snapshot");

    assert_eq!(
        entry.failure_count, 3,
        "debug snapshot must include cumulative failure count for the provider"
    );
    assert_eq!(entry.status, "poll_failed");
    assert!(
        !entry.provider_id.is_empty(),
        "debug snapshot must include provider identity"
    );
}

#[test]
fn backend_runtime_status_records_provider_identity_for_failures() {
    let mut shell = Shell::new();
    shell.record_backend_runtime_status(
        "mesh.network".to_string(),
        "@mesh/networkmanager-network".to_string(),
        BackendRuntimeStatus::InitFailed,
        "dbus connection refused".to_string(),
    );

    // The runtime status map must record both provider identity and status.
    let entry = shell
        .backend_runtime_status("mesh.network", "@mesh/networkmanager-network")
        .expect("runtime status must be recorded for the failed provider");
    assert_eq!(
        entry.provider_id, "@mesh/networkmanager-network",
        "runtime status must identify the failed provider"
    );
    assert_eq!(
        entry.interface, "mesh.network",
        "runtime status must identify the interface"
    );
    assert_eq!(
        entry.status.as_str(),
        "init_failed",
        "runtime status must record the lifecycle stage"
    );
    assert_eq!(
        entry.failure_count, 1,
        "first failure must set failure_count to 1"
    );
    assert!(
        entry.message.contains("dbus connection refused"),
        "runtime status must preserve the failure message"
    );

    // Additional failure increments the count.
    shell.record_backend_runtime_status(
        "mesh.network".to_string(),
        "@mesh/networkmanager-network".to_string(),
        BackendRuntimeStatus::InitFailed,
        "still failing".to_string(),
    );
    let entry = shell
        .backend_runtime_status("mesh.network", "@mesh/networkmanager-network")
        .unwrap();
    assert_eq!(
        entry.failure_count, 2,
        "repeated failure must increment failure_count"
    );
}

#[test]
fn active_provider_failure_clears_latest_service_state() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);

    // Inject a healthy service state for the active provider.
    let healthy_event = service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 75, "muted": false }),
    );
    shell.record_latest_service_state(&healthy_event);
    {
        let latest = shell.latest_service_state.get("mesh.audio").unwrap();
        assert_eq!(latest.state["available"], true);
    }

    // Provider fails — should replace stale state with unavailable payload.
    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "poll boom".to_string(),
    );

    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(
        latest.state["available"], false,
        "active provider failure must set available=false in latest_service_state"
    );
    assert_eq!(latest.provider_id, "@mesh/pipewire-audio");
}

#[test]
fn stale_provider_failure_does_not_clear_new_provider_state() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    // New provider emits healthy state.
    let healthy_event = service_update(
        "mesh.audio",
        "@mesh/new-audio",
        serde_json::json!({ "available": true, "percent": 50 }),
    );
    shell.record_latest_service_state(&healthy_event);

    // Old (stale) provider reports failure — must NOT clear new provider's state.
    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/old-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "old provider late failure".to_string(),
    );

    // New provider's state must remain intact.
    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(
        latest.provider_id, "@mesh/new-audio",
        "stale provider failure must not replace new provider state"
    );
    assert_eq!(latest.state["available"], true);
}
