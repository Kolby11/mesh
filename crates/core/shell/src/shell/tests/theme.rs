use super::common::*;
use super::*;

#[test]
fn provider_theme_update_is_accepted_through_the_generic_state_path() {
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

    shell.sync_theme_service_state().unwrap();
    let mut payload = shell.latest_service_state["mesh.theme"].state.clone();
    payload["current"] = serde_json::json!("mesh-default-light");
    payload["theme_id"] = serde_json::json!("mesh-default-light");
    payload["is_dark"] = serde_json::json!(false);
    shell
        .broadcast_service_event(service_update("mesh.theme", "@mesh/shell-theme", payload))
        .unwrap();

    assert!(!seen_events.lock().unwrap().is_empty());
    assert_eq!(
        shell.latest_service_state["mesh.theme"].provider_id,
        "@mesh/shell-theme"
    );
}

#[test]
fn theme_snapshot_is_published_by_the_shell() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    shell.sync_theme_service_state().unwrap();

    assert_eq!(
        shell.latest_service_state["mesh.theme"].provider_id,
        "@mesh/shell-theme"
    );
}

#[test]
fn theme_revision_events_mirror_the_rendered_snapshot_and_token_delta() {
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));

    shell.sync_theme_service_state().unwrap();
    shell.theme.update_active(|theme| {
        theme.set_render_metadata("sunset", "light", "high");
        theme.set_token(
            "color.primary",
            mesh_core_theme::TokenValue::String("#123456".into()),
            mesh_core_theme::ThemeProvenance::UserOverride,
        );
    });
    shell.sync_theme_service_state().unwrap();

    let state = shell.latest_service_state["mesh.theme"].state.clone();
    let events = seen_events.lock().unwrap();
    let theme_event = events.iter().find_map(|event| match event {
        ServiceEvent::InterfaceEvent { name, payload, .. } if name == "ThemeChanged" => {
            Some(payload)
        }
        _ => None,
    });
    let theme_event = theme_event.expect("theme changes must publish a named event");
    assert_eq!(theme_event["theme_id"], state["theme_id"]);
    assert_eq!(theme_event["mode"], serde_json::json!("sunset"));
    assert_eq!(theme_event["color_scheme"], serde_json::json!("light"));
    assert_eq!(theme_event["contrast"], serde_json::json!("high"));
    assert_eq!(theme_event["revision"], state["revision"]);
    assert_eq!(
        theme_event["changed_tokens"][0]["name"],
        serde_json::json!("color.primary")
    );
    assert_eq!(
        theme_event["changed_tokens"][0]["provenance"],
        serde_json::json!("UserOverride")
    );

    let token_event = events.iter().find_map(|event| match event {
        ServiceEvent::InterfaceEvent { name, payload, .. }
            if name == "TokenChanged" && payload["name"] == "color.primary" =>
        {
            Some(payload)
        }
        _ => None,
    });
    let token_event = token_event.expect("changed tokens must publish individual events");
    assert_eq!(token_event["revision"], state["revision"]);
    assert_eq!(token_event["value"], serde_json::json!("#123456"));
}

#[test]
fn theme_revision_skips_individual_token_events_without_a_subscriber() {
    let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: Vec::new(),
        cached_update_services: Vec::new(),
        interface_events: vec![ServiceInterfaceEventSubscription {
            service: "theme".to_string(),
            event: "ThemeChanged".to_string(),
        }],
    })));
    let state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/theme-revision-observer",
        summary,
        Arc::clone(&state),
    )));

    shell.sync_theme_service_state().unwrap();
    shell.theme.update_active(|theme| {
        theme.set_token(
            "color.primary",
            mesh_core_theme::TokenValue::String("#123456".into()),
            mesh_core_theme::ThemeProvenance::UserOverride,
        );
    });
    shell.sync_theme_service_state().unwrap();

    let state = state.lock().unwrap();
    assert!(state.handled.iter().any(
        |event| matches!(event, ServiceEvent::InterfaceEvent { name, .. } if name == "ThemeChanged")
    ));
    assert!(!state.handled.iter().any(
        |event| matches!(event, ServiceEvent::InterfaceEvent { name, .. } if name == "TokenChanged")
    ));
}

// cargo test -p mesh-core-shell --release -- unsubscribed_theme_token_event_gate_beats_fanout --ignored --nocapture
#[test]
#[ignore = "release-only unsubscribed theme-token event benchmark"]
fn unsubscribed_theme_token_event_gate_beats_fanout() {
    let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: Vec::new(),
        cached_update_services: Vec::new(),
        interface_events: vec![ServiceInterfaceEventSubscription {
            service: "theme".to_string(),
            event: "ThemeChanged".to_string(),
        }],
    })));
    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/theme-benchmark-observer",
        summary,
        Arc::new(Mutex::new(IndexedRecordingState::default())),
    )));
    assert!(!shell.has_interface_event_observers("mesh.theme", "TokenChanged"));

    const BATCHES: usize = 100;
    const TOKENS: usize = 215;
    let started = std::time::Instant::now();
    for batch in 0..BATCHES {
        for token in 0..TOKENS {
            shell
                .broadcast_shell_interface_event(
                    "mesh.theme",
                    "TokenChanged",
                    serde_json::json!({
                        "theme_id": "@mesh/benchmark",
                        "mode": "dark",
                        "name": format!("color.token-{token}"),
                        "value": format!("#{:06x}", token),
                        "provenance": "ThemePack",
                        "revision": batch.to_string(),
                    }),
                )
                .unwrap();
        }
    }
    let forced_fanout = started.elapsed();

    let started = std::time::Instant::now();
    for _ in 0..BATCHES {
        if shell.has_interface_event_observers("mesh.theme", "TokenChanged") {
            std::hint::black_box(TOKENS);
        }
    }
    let subscriber_gate = started.elapsed();

    eprintln!(
        "unsubscribed theme token events: forced={forced_fanout:?} gated={subscriber_gate:?}"
    );
    assert!(subscriber_gate < forced_fanout);
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
fn manual_locale_change_commits_the_shared_settings_revision_before_broadcast() {
    let _env_lock = settings_env_lock();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    fs::write(
        &settings_path,
        r#"{"shell":{"i18n":{"locale":"en","fallback_locale":"en"}}}"#,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();

    shell.apply_set_locale("sk-SK").unwrap();

    assert_eq!(shell.locale.current(), "sk-SK");
    assert_eq!(shell.settings_store.revision(), 1);
    let persisted = mesh_core_config::SettingsStore::load_from(&settings_path).unwrap();
    assert_eq!(persisted.revision(), 1);
    assert_eq!(persisted.shell().i18n.locale, "sk-SK");
    assert_eq!(persisted.shell().i18n.fallback_locale, "en");
    assert_eq!(
        shell.latest_service_state["mesh.settings"].state["durable_revision"],
        serde_json::json!("shared:1")
    );
    assert_eq!(
        shell.latest_service_state["mesh.locale"].state["durable_revision"],
        serde_json::json!("shared:1")
    );
}

#[test]
fn manual_locale_change_commits_the_active_profile_revision_before_broadcast() {
    let _env_lock = settings_env_lock();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let graph_path = dir.path().join("module.json");
    let profiles_dir = dir.path().join("profiles");
    fs::create_dir_all(&profiles_dir).unwrap();
    fs::write(&settings_path, "{}").unwrap();
    fs::write(&graph_path, "{}").unwrap();
    fs::write(
        profiles_dir.join("default.json"),
        r#"{"schemaVersion":3,"roots":{},"settings":{}}"#,
    )
    .unwrap();
    fs::write(dir.path().join("active-profile"), "default\n").unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let _graph_path = EnvGuard::set("MESH_MODULE_GRAPH_PATH", &graph_path);
    let mut shell = Shell::new();

    shell.apply_set_locale("sk-SK").unwrap();

    let shared = mesh_core_config::SettingsStore::load_from(&settings_path).unwrap();
    let profile = mesh_core_module::package::ProfilePaths::from_root_graph(&graph_path)
        .unwrap()
        .load("default")
        .unwrap();
    assert_eq!(shared.revision(), 0);
    assert_eq!(profile.revision, 1);
    assert_eq!(
        profile.settings["shell"]["i18n"]["locale"],
        serde_json::json!("sk-SK")
    );
    assert_eq!(
        shell.latest_service_state["mesh.settings"].state["durable_revision"],
        serde_json::json!("shared:0;profile:1")
    );
    assert_eq!(
        shell.latest_service_state["mesh.locale"].state["durable_revision"],
        serde_json::json!("shared:0;profile:1")
    );
}

#[test]
fn manual_locale_change_reuses_the_active_graph_catalog_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let _settings = isolated_settings_file(dir.path());
    let mut shell = Shell::new();
    shell.installed_module_graph = Some(graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {}
            }"#,
        Vec::new(),
    ));
    let catalogs = shell.locale.catalog_snapshot();

    shell.apply_set_locale("sk-SK").unwrap();

    assert!(Arc::ptr_eq(&catalogs, &shell.locale.catalog_snapshot()));
    assert_eq!(shell.locale.current(), "sk-SK");
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
        shell.theme.register_theme(theme).unwrap();
    }

    shell.sync_theme_service_state().unwrap();

    let state = &shell.latest_service_state["mesh.theme"].state;
    assert_eq!(
        state["available"],
        serde_json::json!([
            "gruvbox-dark",
            "mesh-default-dark",
            "mesh-default-light",
            "tokyo-night"
        ])
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
            ("tokyo-night".to_string(), "Tokyo Night".to_string()),
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
fn explicit_color_scheme_is_reported_without_id_heuristics() {
    let mut shell = Shell::new();

    shell.sync_theme_service_state().unwrap();

    assert_eq!(
        shell.latest_service_state["mesh.theme"].state["is_dark"],
        serde_json::json!(true)
    );
    assert_eq!(
        shell.latest_service_state["mesh.theme"].state["color_scheme"],
        serde_json::json!("dark")
    );
}

#[test]
fn shell_theme_backend_candidate_receives_resolved_active_theme_setting() {
    let mut shell = Shell::new();
    shell.settings.theme.active = "missing-theme".to_string();
    let mut candidate = BackendLaunchCandidate {
        module_id: "@mesh/shell-theme".to_string(),
        interface: "mesh.theme".to_string(),
        service_name: "theme".to_string(),
        entrypoint_path: PathBuf::from("src/main.luau"),
        script_source: String::new(),
        capabilities: Vec::new(),
        settings: serde_json::json!({}),
        command_registry: None,
        event_registry: None,
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

    let mut candidate = BackendLaunchCandidate {
        module_id: "@mesh/shell-theme".to_string(),
        interface: "mesh.theme".to_string(),
        service_name: "theme".to_string(),
        entrypoint_path: PathBuf::from("src/main.luau"),
        script_source: String::new(),
        capabilities: Vec::new(),
        settings: serde_json::json!({}),
        command_registry: None,
        event_registry: None,
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
    shell.sync_theme_service_state().unwrap();
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
    assert_eq!(latest.state["current"], serde_json::json!("tokyo-night"));
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
    fs::write(
        dir.path().join("theme.css"),
        ":root { --color-surface: #FFFBFE; }",
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();
    shell.installed_module_graph = Some(graph_with_theme_source(
        dir.path(),
        "mesh-default-light",
        "theme.css",
    ));
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
        Some(&serde_json::json!("@mesh/test-theme:mesh-default-light"))
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
    let _settings = isolated_settings_file(dir.path());
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
    let _settings = isolated_settings_file(dir.path());
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
    let persisted =
        mesh_core_config::SettingsStore::load_from(&dir.path().join("settings.json")).unwrap();
    assert_eq!(persisted.revision(), 1);
    assert_eq!(
        persisted.shell().theme.active,
        "@mesh/test-theme:mesh-default-light"
    );
    assert_eq!(
        shell.latest_service_state["mesh.settings"].state["durable_revision"],
        serde_json::json!("shared:1")
    );
    assert_eq!(
        shell.latest_service_state["mesh.theme"].state["durable_revision"],
        serde_json::json!("shared:1")
    );
}

#[test]
fn settings_reload_delivers_one_ordered_control_plane_batch() {
    let _env_lock = settings_env_lock();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    fs::write(&settings_path, "{}").unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    fs::write(
        dir.path().join("theme.css"),
        ":root { --color-surface: #123456; }",
    )
    .unwrap();
    shell.installed_module_graph = Some(graph_with_theme_source(
        dir.path(),
        "control-plane-theme",
        "theme.css",
    ));
    fs::write(
        &settings_path,
        r#"{"revision":1,"shell":{"theme":{"active":"@mesh/test-theme:control-plane-theme"},"i18n":{"locale":"sk-SK","fallback_locale":"en"}}}"#,
    )
    .unwrap();
    shell.settings_watch.modified_at = None;
    shell.next_shell_settings_reload_check = std::time::Instant::now();

    shell.reload_locale_if_settings_changed().unwrap();

    let events = seen_events.lock().unwrap();
    let updates = events
        .iter()
        .filter_map(|event| match event {
            ServiceEvent::Updated { service, .. } => Some(service.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(updates, ["mesh.settings", "mesh.theme", "mesh.locale"]);
    assert_eq!(
        shell.latest_service_state["mesh.settings"].state["durable_revision"],
        serde_json::json!("shared:1")
    );
    assert_eq!(
        shell.latest_service_state["mesh.theme"].state["durable_revision"],
        serde_json::json!("shared:1")
    );
    assert_eq!(
        shell.latest_service_state["mesh.locale"].state["durable_revision"],
        serde_json::json!("shared:1")
    );
}

#[test]
fn settings_theme_reload_rejects_unconfigured_theme_without_fallback() {
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

    assert_eq!(shell.settings.theme.active, "mesh-default-dark");
    assert_eq!(shell.theme.active().id, "tokyo-night");
    assert_eq!(recorded_updates_for(&seen_events, "mesh.theme"), 0);
}

#[test]
fn untracked_theme_files_cannot_bootstrap_the_shell_theme() {
    let _env_lock = settings_env_lock();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let untracked_theme_dir = dir.path().join("themes");
    fs::create_dir_all(&untracked_theme_dir).unwrap();
    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"untracked"}}}"#,
    )
    .unwrap();
    fs::write(
        untracked_theme_dir.join("untracked.json"),
        r##"{"id":"untracked","name":"Untracked","tokens":{"color.surface":"#123456"}}"##,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);

    let shell = Shell::new();

    assert_eq!(shell.theme.active().id, "tokyo-night");
    assert!(shell.theme_watch.path.as_os_str().is_empty());
}

#[test]
fn malformed_theme_reload_retains_last_known_good_snapshot() {
    let _env_lock = settings_env_lock();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"reload-theme"}}}"#,
    )
    .unwrap();
    fs::write(dir.path().join("theme.css"), "node { color: #123456; }").unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();
    shell.installed_module_graph = Some(graph_with_theme_source(
        dir.path(),
        "reload-theme",
        "theme.css",
    ));
    shell
        .apply_set_theme("@mesh/test-theme:reload-theme")
        .unwrap();
    shell.theme_watch.modified_at = None;
    shell.next_theme_reload_check = std::time::Instant::now();
    fs::write(dir.path().join("theme.css"), "{ malformed").unwrap();

    let requests = shell.reload_theme_if_changed().unwrap();

    assert!(requests.is_empty());
    assert_eq!(shell.theme.active().id, "@mesh/test-theme:reload-theme");
}

#[test]
fn theme_file_recovery_syncs_mesh_theme_latest_state_and_components() {
    let _env_lock = settings_env_lock();
    let runtime = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    fs::write(
        &settings_path,
        r#"{"shell":{"theme":{"active":"mesh-recovered-light"}}}"#,
    )
    .unwrap();
    fs::write(dir.path().join("theme.css"), "{ malformed").unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();
    shell.installed_module_graph = Some(graph_with_theme_source(
        dir.path(),
        "mesh-recovered-light",
        "theme.css",
    ));
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
    shell.sync_theme_service_state().unwrap();
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

    shell.theme_watch.modified_at = None;
    shell.next_theme_reload_check = std::time::Instant::now();
    let requests = shell.reload_theme_if_changed().unwrap();

    assert!(requests.is_empty());
    assert_eq!(shell.theme.active().id, "tokyo-night");
    assert_eq!(recorded_updates_for(&seen_events, "mesh.theme"), 1);

    fs::write(
        dir.path().join("theme.css"),
        ":root { --color-surface: #FFFBFE; }",
    )
    .unwrap();
    shell.next_theme_reload_check = std::time::Instant::now();
    let requests = shell.reload_theme_if_changed().unwrap();

    assert!(requests.is_empty());
    assert_eq!(
        shell.theme.active().id,
        "@mesh/test-theme:mesh-recovered-light"
    );
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("@mesh/test-theme:mesh-recovered-light"))
    );

    let events = seen_events.lock().unwrap();
    let updates = events
        .iter()
        .filter_map(|event| match event {
            ServiceEvent::Updated { payload, .. } => Some(payload),
            ServiceEvent::InterfaceEvent { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 2);
    let payload = updates.last().unwrap();
    assert_eq!(
        payload["current"],
        serde_json::json!("@mesh/test-theme:mesh-recovered-light")
    );
}

/// `register_contract` discards compilation failures, so one bad field type in
/// a built-in contract removes the entire interface from the catalog without a
/// word. `mesh.theme` declared `fingerprint` as `integer?` rather than `int?`,
/// which left every theme method — `set_theme` among them — rejecting as an
/// unknown channel, so no surface could change the theme.
#[test]
fn built_in_interfaces_compile_and_expose_their_methods() {
    let shell = Shell::new();
    let catalog = shell.interfaces.resolved_catalog();

    for interface in [
        "mesh.theme",
        "mesh.locale",
        "mesh.settings",
        "mesh.packages",
        "mesh.composition",
    ] {
        assert!(
            catalog.resolve(interface, None).contract.is_some(),
            "built-in interface '{interface}' is missing from the catalog, so every \
             method on it rejects as an unknown channel"
        );
    }

    let theme = catalog
        .resolve("mesh.theme", None)
        .contract
        .expect("mesh.theme contract");
    let methods = theme
        .methods
        .iter()
        .map(|method| method.name.as_str())
        .collect::<Vec<_>>();
    for method in ["set_theme", "set_icon_theme", "set_font_family"] {
        assert!(
            methods.contains(&method),
            "mesh.theme must expose {method}, found {methods:?}"
        );
    }
}

/// Live component dispatch authorizes published events against the catalog,
/// not the shell operation registry, so this is the path that rejected every
/// theme change with "Unknown shell channel 'mesh.theme.set_theme'".
#[test]
fn a_capable_module_may_publish_the_theme_service_methods() {
    let shell = Shell::new();
    let catalog = shell.interfaces.resolved_catalog();
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.theme.control",
    ));

    let outcome = mesh_core_scripting::OperationRegistry::builtin().authorize_event_with_catalog(
        "mesh.theme.set_theme",
        &serde_json::json!({ "theme_id": "nord" }),
        "@mesh/settings",
        &capabilities,
        &catalog,
    );

    assert!(
        outcome.is_ok(),
        "a module holding service.theme.control must be able to set the theme: {:?}",
        outcome.unwrap_err()
    );
}
