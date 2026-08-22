use super::common::*;
use super::*;

#[test]
fn debug_profiling_request_toggles_independent_session_state() {
    let mut shell = Shell::new();

    assert!(!shell.debug.profiling_enabled);
    assert_eq!(shell.debug.profiling_session_id, 0);
    assert!(!shell.debug.show_layout_bounds);

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    assert!(shell.debug.profiling_enabled);
    assert_eq!(shell.debug.profiling_session_id, 1);

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    assert!(!shell.debug.profiling_enabled);
    assert_eq!(
        shell.debug.profiling_session_id, 1,
        "disabling profiling should not fabricate a new session"
    );

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();
    assert!(
        !shell.debug.profiling_enabled,
        "debug overlay visibility must remain independent from profiling state"
    );
    assert!(
        !shell.debug.show_layout_bounds,
        "profiling changes must remain independent from layout-bounds debugging"
    );
}

#[test]
fn debug_layout_bounds_toggle_remains_independent_from_overlay_visibility() {
    let mut shell = Shell::new();

    shell
        .apply_request(CoreRequest::ToggleDebugLayoutBounds)
        .unwrap();
    assert!(shell.debug.show_layout_bounds);
    assert!(!shell.debug.enabled);
    assert!(!shell.debug.profiling_enabled);

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();
    assert!(shell.debug.show_layout_bounds);

    shell
        .apply_request(CoreRequest::ToggleDebugLayoutBounds)
        .unwrap();
    assert!(!shell.debug.show_layout_bounds);
}

#[test]
fn debug_snapshot_omits_profiling_payload_when_disabled() {
    let mut shell = Shell::new();
    let snapshot = shell.build_debug_snapshot();
    assert!(
        snapshot.profiling.is_none(),
        "profiling payload must be absent while profiling is disabled"
    );
}

#[test]
fn debug_snapshot_exposes_module_object_instances() {
    let mut shell = Shell::new();
    shell.interfaces.register(InterfaceProvider {
        interface: "mesh.example".to_string(),
        version: Some("1.0".to_string()),
        base_module: Some("@mesh/example-interface".to_string()),
        provider_module: "@mesh/example-backend".to_string(),
        backend_name: "Example".to_string(),
        priority: 10,
    });
    shell.record_backend_runtime_status(
        "mesh.example".to_string(),
        "@mesh/example-backend".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );

    let snapshot = shell.build_debug_snapshot();

    assert!(snapshot.module_instances.iter().any(|entry| {
        entry.object_kind == "backend"
            && entry.instance_id == "mesh.example:@mesh/example-backend"
            && entry.module_id == "@mesh/example-backend"
            && entry.interface.as_deref() == Some("mesh.example")
            && entry.version.as_deref() == Some("1.0")
            && entry.lifecycle == "running"
    }));
}

#[test]
fn debug_snapshot_exposes_installed_module_graph_contracts() {
    let mut shell = Shell::new();
    shell.discover_modules();

    let snapshot = shell.build_debug_snapshot();
    let navigation = snapshot
        .module_graph
        .iter()
        .find(|entry| entry.module_id == "@mesh/navigation-bar")
        .expect("navigation module graph entry");

    assert!(navigation.uses_interfaces.contains(&"mesh.audio".into()));
    assert!(navigation.uses_interfaces.contains(&"mesh.power".into()));
    assert!(
        navigation
            .uses_optional_interfaces
            .contains(&"mesh.brightness".into())
    );
    assert!(
        navigation
            .uses_icon_packs
            .contains(&"@mesh/icons-material-symbols".into())
    );
    assert!(
        navigation
            .provides_settings
            .contains(&"@mesh/navigation-bar".into())
    );
    assert_eq!(
        navigation
            .settings_schema
            .as_ref()
            .and_then(|schema| { schema["properties"]["blur_enabled"]["type"].as_str() }),
        Some("bool")
    );
    assert!(navigation.settings_values.is_object());
    assert!(
        navigation
            .provides_i18n
            .iter()
            .any(|entry| entry == "en:config/i18n/en.json")
    );
    assert!(
        navigation
            .required_icons
            .contains(&"battery-caution".into())
    );
    assert!(navigation.keybind_actions.contains(&"mute".into()));
    assert!(
        navigation
            .active_providers
            .iter()
            .any(|entry| entry.starts_with("mesh.power="))
    );

    let pipewire = snapshot
        .module_graph
        .iter()
        .find(|entry| entry.module_id == "@mesh/pipewire-audio")
        .expect("pipewire module graph entry");
    assert!(pipewire.required_binaries.contains(&"wpctl".into()));
    assert!(pipewire.optional_binaries.contains(&"aplay".into()));
    assert!(
        pipewire
            .native_binaries
            .iter()
            .any(|binary| { binary.name == "wpctl" && !binary.optional })
    );

    let debug_payload = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("debug snapshot should backfill mesh.debug latest state");
    assert!(
        debug_payload.state["module_graph"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry["module_id"] == serde_json::json!("@mesh/navigation-bar")
                    && entry["uses"]["interfaces"]
                        .as_array()
                        .is_some_and(|interfaces| {
                            interfaces.contains(&serde_json::json!("mesh.audio"))
                        })
                    && entry["uses"]["keybinds"]
                        .as_array()
                        .is_some_and(|actions| actions.contains(&serde_json::json!("mute")))
                    && entry["provides"]["settings_schema"]["properties"]["blur_enabled"]["type"]
                        == serde_json::json!("bool")
            }))
    );
    let pipewire_json = debug_payload.state["module_graph"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["module_id"] == serde_json::json!("@mesh/pipewire-audio"))
        })
        .expect("serialized pipewire graph entry");
    assert!(
        pipewire_json["uses"]["native_binaries"]
            .as_array()
            .is_some_and(|binaries| binaries.iter().any(|binary| {
                binary["name"] == serde_json::json!("wpctl")
                    && binary["optional"] == serde_json::json!(false)
                    && binary["available"].is_boolean()
            }))
    );
}

#[test]
fn debug_snapshot_resolves_module_graph_layout_label_with_active_locale() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/panel": { "kind": "frontend", "path": "@mesh/panel", "enabled": true }
              },
              "layout": { "entrypoint": "@mesh/panel:main" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/panel",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "entry": "src/main.mesh",
                    "provides": {
                      "layout": [
                        {
                          "id": "main",
                          "entrypoint": "src/main.mesh",
                          "label": { "t": "layout.main.label", "fallback": "Main panel" }
                        }
                      ]
                    },
                    "surfaceLayout": { "size_policy": "fixed" },
                    "accessibility": { "role": "toolbar" }
                  }
                }"#,
        ],
    );
    let mut shell = Shell::new();
    shell.installed_module_graph = Some(graph);
    shell.locale.load_module_translations(
        "@mesh/panel",
        mesh_core_locale::TranslationSet {
            locale: "sk".into(),
            messages: HashMap::from([("layout.main.label".into(), "Hlavny panel".into())]),
        },
    );
    shell.locale.set_locale("sk");

    let snapshot = shell.build_debug_snapshot();
    let panel = snapshot
        .module_graph
        .iter()
        .find(|entry| entry.module_id == "@mesh/panel")
        .expect("panel module graph entry");

    assert_eq!(panel.surface_layout_label.as_deref(), Some("Hlavny panel"));
    assert_eq!(
        panel.surface_layout_label_key.as_deref(),
        Some("layout.main.label")
    );
    assert_eq!(
        panel.surface_layout_label_fallback.as_deref(),
        Some("Main panel")
    );

    let debug_payload = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("debug snapshot should backfill mesh.debug latest state");
    let panel_json = debug_payload.state["module_graph"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["module_id"] == serde_json::json!("@mesh/panel"))
        })
        .expect("panel module graph JSON entry");
    assert_eq!(
        panel_json["surface"]["layout_label"],
        serde_json::json!("Hlavny panel")
    );
    assert_eq!(
        panel_json["surface"]["layout_label_key"],
        serde_json::json!("layout.main.label")
    );
    assert_eq!(
        panel_json["surface"]["layout_label_fallback"],
        serde_json::json!("Main panel")
    );
}

#[test]
fn debug_snapshot_backfills_mesh_debug_service_state() {
    let mut shell = Shell::new();
    shell.debug.enabled = true;

    let snapshot = shell.build_debug_snapshot();
    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug service state should be backfilled from debug snapshots");

    assert_eq!(latest.provider_id, mesh_core_debug::DEBUG_SOURCE_MODULE_ID);
    assert_eq!(latest.state["overlay_enabled"], serde_json::json!(true));
    assert_eq!(
        latest.state["layout_bounds_enabled"],
        serde_json::json!(false)
    );
    assert_eq!(latest.state["profiling_enabled"], serde_json::json!(false));
    assert_eq!(latest.state["profiling_session_id"], serde_json::json!(0));
    assert_eq!(latest.state["active_view"], serde_json::json!("overview"));
    assert_eq!(
        latest.state["active_surfaces"],
        serde_json::json!(snapshot.active_surfaces)
    );
    assert!(latest.state["profiling"].is_null());
    assert!(latest.state["profiling_stream"].is_null());
}

#[test]
fn debug_snapshot_is_attributed_to_the_active_provider() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(
        &runtime,
        mesh_core_debug::DEBUG_INTERFACE,
        "@mesh/custom-debug",
    );
    shell.replace_backend_runtime(mesh_core_debug::DEBUG_INTERFACE.to_string(), slot);

    shell.build_debug_snapshot();

    assert_eq!(
        shell.latest_service_state[mesh_core_debug::DEBUG_INTERFACE].provider_id,
        "@mesh/custom-debug"
    );
}

#[test]
fn debug_snapshot_exposes_deduplicated_ordered_profiling_stream() {
    let mut shell = Shell::new();
    shell.debug.profiling_enabled = true;
    shell.debug.profiling_session_id = 1;
    shell.profiling.record_shell_stage(
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(7),
        Some("input"),
    );
    shell.profiling.record_surface_stage(
        "@test/surface",
        Some("@test/module"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(11),
        Some("paint"),
    );

    shell.build_debug_snapshot();
    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("debug snapshot should publish state");
    let stream = latest.state["profiling_stream"]
        .as_array()
        .expect("profiling stream should be an array");

    assert_eq!(
        stream.len(),
        2,
        "surface samples must not be duplicated from the shell roll-up"
    );
    assert!(
        stream.windows(2).all(|pair| {
            pair[0]["order"].as_u64().unwrap() < pair[1]["order"].as_u64().unwrap()
        })
    );
    assert!(
        stream
            .iter()
            .all(|sample| sample["timestamp_micros"].is_u64())
    );
    assert_eq!(stream[1]["surface_id"], serde_json::json!("@test/surface"));
    let trace = latest.state["chrome_trace"]["traceEvents"]
        .as_array()
        .expect("chrome trace should contain events");
    assert_eq!(trace.len(), 2);
    assert_eq!(trace[1]["ph"], serde_json::json!("X"));
    assert_eq!(trace[1]["tid"], serde_json::json!("@test/surface"));
}

#[test]
fn shell_registers_debug_provider_for_builtin_inspector_imports() {
    let shell = Shell::new();
    let resolution = shell
        .interfaces
        .resolve(mesh_core_debug::DEBUG_INTERFACE, Some(">=1.0"));

    assert_eq!(
        resolution
            .provider
            .as_ref()
            .map(|provider| provider.provider_module.as_str()),
        Some(mesh_core_debug::DEBUG_SOURCE_MODULE_ID)
    );
}

#[test]
fn shell_registers_theme_provider_for_frontend_theme_proxy() {
    let shell = Shell::new();
    let resolution = shell.interfaces.resolve("mesh.theme", None);

    assert_eq!(
        resolution
            .provider
            .as_ref()
            .map(|provider| provider.provider_module.as_str()),
        Some("@mesh/shell"),
        "frontend modules with theme.read must be able to resolve require(\"mesh.theme\")"
    );
    let fields = resolution
        .contract
        .as_ref()
        .expect("built-in mesh.theme contract")
        .state_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        fields,
        [
            "current",
            "theme_id",
            "mode",
            "mode_policy",
            "color_scheme",
            "contrast",
            "tokens",
            "provenance",
            "revision",
            "fingerprint",
            "is_dark",
            "themes",
            "available",
            "system_resources",
        ],
        "theme registry fields must be declared so frontend reads are reactive"
    );
}

#[test]
fn shell_registers_interface_contracts_and_providers_from_installed_graph() {
    let interface_dir = tempfile::tempdir().unwrap();
    let root = RootModuleGraphManifest::from_json_str(
        r#"{
              "name": "@mesh/test-config",
              "version": "0.1.0",
              "mesh": {
                "schemaVersion": 1,
                "modulesDir": "modules",
                "modules": {
                  "@mesh/example-interface": { "kind": "interface", "path": "@mesh/example-interface", "enabled": true },
                  "@mesh/example-backend": { "kind": "backend", "path": "@mesh/example-backend", "enabled": true }
                },
                "providers": { "mesh.example": "@mesh/example-backend" }
              }
            }"#,
    )
    .unwrap();
    let interface = LoadedModuleManifest {
        manifest: ModuleManifest::from_json_str(
            r#"{
                  "name": "@mesh/example-interface",
                  "version": "1.0.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "interface",
                    "interface": {
                      "name": "mesh.example",
                      "version": "1.0",
                      "domain": "example",
                      "relationship": "base",
                      "contract": {
                        "methods": [{ "name": "read", "returns": "boolean" }],
                        "capabilities": { "required": ["service.example.read"] }
                      }
                    }
                  }
                }"#,
        )
        .unwrap(),
        path: interface_dir.path().join("module.json"),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    };
    let backend = LoadedModuleManifest {
        manifest: ModuleManifest::from_json_str(
            r#"{
                  "name": "@mesh/example-backend",
                  "version": "1.0.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [
                      {
                        "interface": "mesh.example",
                        "version": "1.0",
                        "baseModule": "@mesh/example-interface",
                        "provider": "example",
                        "priority": 10
                      }
                    ]
                  }
                }"#,
        )
        .unwrap(),
        path: PathBuf::from("<test>/backend/module.json"),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    };
    let graph = InstalledModuleGraph::from_parts(root, vec![interface, backend]).unwrap();
    let mut shell = Shell::new();

    shell.register_interfaces_from_graph(&graph);

    let contracts = shell.interfaces.contracts_for("mesh.example");
    assert_eq!(contracts.len(), 1);
    assert_eq!(
        contracts[0].capabilities.required,
        vec!["service.example.read".to_string()]
    );
    let providers = shell.interfaces.providers_for("mesh.example");
    assert!(providers.iter().any(|provider| {
        provider.provider_module == "@mesh/example-backend"
            && provider.backend_name == "example"
            && provider.base_module.as_deref() == Some("@mesh/example-interface")
    }));
}

#[test]
fn debug_snapshot_publish_delivers_mesh_debug_service_event() {
    let mut shell = Shell::new();
    shell.debug.enabled = true;
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));

    let emitted = shell.publish_debug_snapshot().unwrap();

    assert!(emitted.is_empty());
    let events = events.lock().unwrap();
    let ServiceEvent::Updated {
        service,
        source_module,
        payload,
    } = events
        .last()
        .expect("debug snapshot should be delivered as a service update")
    else {
        panic!("expected debug snapshot service update");
    };
    assert_eq!(service, mesh_core_debug::DEBUG_INTERFACE);
    assert_eq!(source_module, mesh_core_debug::DEBUG_SOURCE_MODULE_ID);
    assert_eq!(payload["overlay_enabled"], serde_json::json!(true));
    assert_eq!(payload["layout_bounds_enabled"], serde_json::json!(false));
    assert!(payload["benchmarks"]["scenarios"].is_array());
}

#[test]
fn debug_snapshot_payload_includes_resolved_keybind_metadata() {
    let mut shell = Shell::new();
    shell.debug.enabled = true;
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::with_keybinds(
        events.clone(),
        vec![mesh_core_debug::DebugKeybindEntry {
            surface_id: "@mesh/navigation-bar".into(),
            module_id: "@mesh/navigation-bar".into(),
            action_id: "mute".into(),
            label: Some("Mute".into()),
            description: Some("Toggle audio output".into()),
            category: Some("Audio".into()),
            label_key: Some("keybind.mute.label".into()),
            description_key: Some("keybind.mute.description".into()),
            category_key: Some("keybind.category.audio".into()),
            key: "m".into(),
            modifiers: vec!["ctrl".into()],
            trigger_kind: "shortcut".into(),
            source: "module_default".into(),
            accessibility_shortcut: "Control+m".into(),
        }],
    )));

    shell.publish_debug_snapshot().unwrap();

    let events = events.lock().unwrap();
    let ServiceEvent::Updated { payload, .. } = events
        .last()
        .expect("debug snapshot should be delivered as a service update")
    else {
        panic!("expected debug snapshot service update");
    };
    assert_eq!(
        payload["keybinds"][0],
        serde_json::json!({
            "surface_id": "@mesh/navigation-bar",
            "module_id": "@mesh/navigation-bar",
            "action_id": "mute",
            "label": "Mute",
            "description": "Toggle audio output",
            "category": "Audio",
            "label_key": "keybind.mute.label",
            "description_key": "keybind.mute.description",
            "category_key": "keybind.category.audio",
            "key": "m",
            "modifiers": ["ctrl"],
            "trigger_kind": "shortcut",
            "source": "module_default",
            "accessibility_shortcut": "Control+m",
        })
    );
    assert!(
        payload["health"].is_array(),
        "debug payload should keep diagnostics health visible"
    );
}

#[test]
fn debug_overlay_toggle_does_not_enable_profiling_in_mesh_debug_payload() {
    let mut shell = Shell::new();

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();
    shell.build_debug_snapshot();
    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug state should exist after snapshot generation");

    assert_eq!(latest.state["overlay_enabled"], serde_json::json!(true));
    assert_eq!(
        latest.state["layout_bounds_enabled"],
        serde_json::json!(false)
    );
    assert_eq!(latest.state["profiling_enabled"], serde_json::json!(false));
    assert_eq!(latest.state["profiling_session_id"], serde_json::json!(0));
    assert!(latest.state["profiling"].is_null());
}

#[test]
fn debug_overlay_toggle_controls_mesh_debug_inspector_visibility_without_enabling_profiling() {
    let mut shell = Shell::new();

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();

    let inspector = shell
        .core
        .surfaces
        .get("@mesh/debug-inspector")
        .expect("debug inspector surface should be tracked when overlay toggles on");
    assert!(shell.debug.enabled);
    assert!(inspector.visible);
    assert!(!shell.debug.profiling_enabled);

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();

    let inspector = shell
        .core
        .surfaces
        .get("@mesh/debug-inspector")
        .expect("debug inspector surface state should remain addressable");
    assert!(!shell.debug.enabled);
    assert!(!inspector.visible);
    assert!(!shell.debug.profiling_enabled);
}
