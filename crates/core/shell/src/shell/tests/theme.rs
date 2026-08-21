use super::common::*;
use super::*;

#[test]
fn inactive_shell_theme_update_is_ignored_when_theme_provider_is_active() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    shell
        .broadcast_service_event(service_update(
            "mesh.theme",
            "@mesh/shell",
            serde_json::json!({
                "current": "mesh-default-light",
                "theme_id": "mesh-default-light",
                "is_dark": false,
            }),
        ))
        .unwrap();

    assert!(seen_events.lock().unwrap().is_empty());
    assert!(!shell.latest_service_state.contains_key("mesh.theme"));
}

#[test]
fn theme_snapshot_is_published_by_the_active_provider() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    shell.sync_theme_service_state("mesh-default-dark").unwrap();

    assert_eq!(
        shell.latest_service_state["mesh.theme"].provider_id,
        "@mesh/shell-theme"
    );
}

#[test]
fn locale_snapshot_is_published_by_the_active_provider() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.locale", "@mesh/shell-locale");
    shell.replace_backend_runtime("mesh.locale".to_string(), slot);

    shell.sync_locale_service_state().unwrap();

    assert_eq!(
        shell.latest_service_state["mesh.locale"].provider_id,
        "@mesh/shell-locale"
    );
}

#[test]
fn theme_service_state_lists_every_registered_theme() {
    let mut shell = Shell::new();
    for (id, name) in [
        ("mesh-default-dark", "MESH Default Dark"),
        ("mesh-default-light", "MESH Default Light"),
        ("gruvbox-dark", "Gruvbox Dark"),
    ] {
        let mut theme = mesh_core_theme::default_theme();
        theme.id = id.to_string();
        theme.name = name.to_string();
        shell.theme.register_theme(theme);
    }

    shell.sync_theme_service_state("mesh-default-dark").unwrap();

    let state = &shell.latest_service_state["mesh.theme"].state;
    assert_eq!(
        state["available"],
        serde_json::json!(["gruvbox-dark", "mesh-default-dark", "mesh-default-light"])
    );
    assert_eq!(
        state["themes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|theme| {
                (
                    theme["id"].as_str().unwrap().to_string(),
                    theme["label"].as_str().unwrap().to_string(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("gruvbox-dark".to_string(), "Gruvbox Dark".to_string()),
            (
                "mesh-default-dark".to_string(),
                "MESH Default Dark".to_string(),
            ),
            (
                "mesh-default-light".to_string(),
                "MESH Default Light".to_string(),
            ),
        ]
    );
    assert_eq!(
        state["themes"][0]["palette"],
        serde_json::json!({
            "surface": "#1a1b26",
            "surface_container_low": "#1a1b26",
            "surface_container_high": "#2f334d",
            "primary": "#e0af68",
            "outline_variant": "#414868",
            "on_surface": "#c0caf5",
        })
    );
}

#[test]
fn tokyo_night_is_reported_as_a_dark_theme() {
    let mut shell = Shell::new();

    shell.sync_theme_service_state("tokyo-night").unwrap();

    assert_eq!(
        shell.latest_service_state["mesh.theme"].state["is_dark"],
        serde_json::json!(true)
    );
}

#[test]
fn shell_theme_backend_candidate_receives_resolved_active_theme_setting() {
    let mut shell = Shell::new();
    shell.settings.theme.active = "missing-theme".to_string();
    let (theme, theme_watch) = load_active_theme(&shell.settings);
    shell.theme = theme;
    shell.theme_watch = theme_watch;
    let mut candidate = BackendLaunchCandidate {
        module_id: "@mesh/shell-theme".to_string(),
        interface: "mesh.theme".to_string(),
        service_name: "theme".to_string(),
        entrypoint_path: PathBuf::from("src/main.luau"),
        script_source: String::new(),
        capabilities: Vec::new(),
        settings: serde_json::json!({}),
    };

    shell.apply_shell_runtime_settings(&mut candidate);

    assert_eq!(shell.theme.active().id, "tokyo-night");
    assert_eq!(
        candidate
            .settings
            .get("__shell")
            .and_then(|shell| shell.get("theme"))
            .and_then(|value| value.as_str()),
        Some("tokyo-night")
    );
}

#[test]
fn shell_theme_fallback_backend_restart_keeps_latest_state_on_resolved_theme() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell.settings.theme.active = "missing-theme".to_string();
    let (theme, theme_watch) = load_active_theme(&shell.settings);
    shell.theme = theme;
    shell.theme_watch = theme_watch;

    let mut candidate = BackendLaunchCandidate {
        module_id: "@mesh/shell-theme".to_string(),
        interface: "mesh.theme".to_string(),
        service_name: "theme".to_string(),
        entrypoint_path: PathBuf::from("src/main.luau"),
        script_source: String::new(),
        capabilities: Vec::new(),
        settings: serde_json::json!({}),
    };
    shell.apply_shell_runtime_settings(&mut candidate);
    let current_theme = candidate
        .settings
        .get("__shell")
        .and_then(|shell| shell.get("theme"))
        .and_then(|value| value.as_str())
        .unwrap()
        .to_string();

    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.theme",
            "@mesh/shell-theme",
            serde_json::json!({
                "current": current_theme,
                "is_dark": true,
                "available": ["mesh-default-dark", "mesh-default-light"],
            }),
        ))
        .unwrap();

    let (replacement_slot, _replacement_rx) =
        backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), replacement_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.theme",
            "@mesh/shell-theme",
            serde_json::json!({
                "current": "mesh-default-dark",
                "is_dark": true,
                "available": ["mesh-default-dark", "mesh-default-light"],
            }),
        ))
        .unwrap();

    let latest = shell.latest_service_state.get("mesh.theme").unwrap();
    assert_eq!(shell.theme.active().id, "tokyo-night");
    assert_eq!(
        latest.state["current"],
        serde_json::json!("mesh-default-dark")
    );
    assert_eq!(latest.state["is_dark"], serde_json::json!(true));
}

#[test]
fn settings_theme_reload_syncs_theme_service_state() {
    let _env_lock = settings_env_lock();
    let runtime = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"mesh-default-dark"}}}"#,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"mesh-default-light"}}}"#,
    )
    .unwrap();
    shell.settings_watch.modified_at = None;
    shell.reload_locale_if_settings_changed().unwrap();

    assert_eq!(shell.settings.theme.active, "mesh-default-light");
    assert_eq!(recorded_updates_for(&seen_events, "mesh.theme"), 1);
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("mesh-default-light"))
    );
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("is_dark")),
        Some(&serde_json::json!(false))
    );
}

#[test]
fn set_theme_forces_full_present_on_existing_components() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("theme.css"), "node { color: #fff; }").unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events),
        )));

    shell.installed_module_graph = Some(graph_with_theme_source(
        dir.path(),
        "test-light-present",
        "theme.css",
    ));

    shell
        .apply_set_theme("@mesh/test-theme:test-light-present")
        .unwrap();

    assert!(
        shell.components[0].parent.force_full_present,
        "theme changes must force a full present for already-painted surfaces"
    );
}

#[test]
fn set_theme_loads_css_package_and_updates_runtime_setting() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("theme.css"),
        ":root { --color-surface: #FFFBFE; }",
    )
    .unwrap();
    let mut shell = Shell::new();
    shell.installed_module_graph = Some(graph_with_theme_source(
        dir.path(),
        "mesh-default-light",
        "theme.css",
    ));

    shell
        .apply_set_theme("@mesh/test-theme:mesh-default-light")
        .unwrap();

    assert_eq!(
        shell.theme.active().id,
        "@mesh/test-theme:mesh-default-light"
    );
    assert_eq!(
        shell.settings.theme.active,
        "@mesh/test-theme:mesh-default-light"
    );
    assert_eq!(
        shell
            .theme
            .active()
            .token("color.surface")
            .map(ToString::to_string),
        Some("#FFFBFE".into())
    );
    assert!(
        shell.theme_watch.path.ends_with("theme.css"),
        "theme watcher should follow the active CSS package"
    );
}

#[test]
fn settings_theme_reload_publishes_resolved_fallback_theme_state() {
    let _env_lock = settings_env_lock();
    let runtime = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"mesh-default-dark"}}}"#,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"missing-theme"}}}"#,
    )
    .unwrap();
    shell.settings_watch.modified_at = None;
    shell.reload_locale_if_settings_changed().unwrap();

    assert_eq!(shell.settings.theme.active, "missing-theme");
    assert_eq!(shell.theme.active().id, "tokyo-night");
    assert_eq!(recorded_updates_for(&seen_events, "mesh.theme"), 1);
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("tokyo-night"))
    );
}

#[test]
fn malformed_theme_reload_retains_last_known_good_snapshot() {
    let _env_lock = settings_env_lock();
    let dir = tempfile::tempdir().unwrap();
    let theme_dir = dir.path().join("themes");
    fs::create_dir_all(&theme_dir).unwrap();
    let settings_path = dir.path().join("settings.json");
    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"reload-theme"}}}"#,
    )
    .unwrap();
    fs::write(
        theme_dir.join("reload-theme.json"),
        r##"{"id":"reload-theme","name":"Reload","tokens":{"color.surface":"#123456"}}"##,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let _theme_dir = EnvGuard::set("MESH_THEME_DIR", &theme_dir);
    let mut shell = Shell::new();
    shell.theme_watch.modified_at = None;
    shell.next_theme_reload_check = std::time::Instant::now();
    fs::write(theme_dir.join("reload-theme.json"), "{ malformed").unwrap();

    let requests = shell.reload_theme_if_changed().unwrap();

    assert!(requests.is_empty());
    assert_eq!(shell.theme.active().id, "reload-theme");
    assert_eq!(
        shell
            .diagnostics
            .snapshot()
            .iter()
            .flat_map(|module| module.instances.iter())
            .flat_map(|instance| instance.active_issues.iter())
            .filter(|issue| issue.issue_code == "theme_reload_rejected")
            .count(),
        1,
    );
}

#[test]
fn theme_file_recovery_syncs_mesh_theme_latest_state_and_components() {
    let _env_lock = settings_env_lock();
    let runtime = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let theme_dir = dir.path().join("themes");
    fs::create_dir_all(&theme_dir).unwrap();
    let settings_path = dir.path().join("settings.json");
    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"mesh-recovered-light"}}}"#,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let _theme_dir = EnvGuard::set("MESH_THEME_DIR", &theme_dir);
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    assert_eq!(shell.settings.theme.active, "mesh-recovered-light");
    assert_eq!(shell.theme.active().id, "tokyo-night");
    let fallback_theme_id = shell.theme.active().id.clone();
    shell.sync_theme_service_state(&fallback_theme_id).unwrap();
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("tokyo-night"))
    );
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("is_dark")),
        Some(&serde_json::json!(true))
    );

    fs::write(
        theme_dir.join("mesh-recovered-light.json"),
        r#"{"id":"mesh-recovered-light","name":"Recovered Light","tokens":{}}"#,
    )
    .unwrap();
    let requests = shell.reload_theme_if_changed().unwrap();

    assert!(requests.is_empty());
    assert_eq!(shell.theme.active().id, "mesh-recovered-light");
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("mesh-recovered-light"))
    );
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("is_dark")),
        Some(&serde_json::json!(false))
    );

    let events = seen_events.lock().unwrap();
    assert_eq!(events.len(), 2);
    let ServiceEvent::Updated { payload, .. } = events.last().unwrap() else {
        panic!("expected theme service update");
    };
    assert_eq!(
        payload["current"],
        serde_json::json!("mesh-recovered-light")
    );
    assert_eq!(payload["is_dark"], serde_json::json!(false));
}
