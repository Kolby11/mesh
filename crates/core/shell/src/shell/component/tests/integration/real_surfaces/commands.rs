use super::*;

#[test]
fn real_core_surfaces_quick_settings_commands_publish_service_requests() {
    let mut audio_ctx = make_audio_ctx();
    audio_ctx
        .load_script(
            r#"
local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end

function onVolumeChange(value)
    local percent = math.floor((tonumber(value) or 0) + 0.5)
    if audio_ok and audio and audio.available ~= false then
        audio.set_volume("default", percent)
    end
end
"#,
        )
        .unwrap();
    audio_ctx.apply_service_payload("audio", &serde_json::json!({ "available": true }));
    audio_ctx
        .call_handler("onVolumeChange", &[serde_json::json!(55)])
        .unwrap();
    let audio_requests =
        crate::shell::service::script_events_to_requests(audio_ctx.drain_published_events());

    match audio_requests.as_slice() {
        [
            CoreRequest::ServiceCall {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.audio");
            assert_eq!(command, "set_volume");
            assert_eq!(
                payload,
                &serde_json::json!({ "device_id": "default", "percent": 55 })
            );
        }
        other => panic!("expected one mesh.audio set_volume command, got {other:?}"),
    }

    let mut network_ctx = make_network_ctx();
    network_ctx
        .load_script(
            r#"
local network_ok, network = pcall(require, "mesh.network@>=1.0")
if not network_ok then network = nil end

function onToggleWiFi()
    if network_ok and network and network.available ~= false then
        network.set_wifi_enabled(not (network.wifi_enabled or false))
    end
end
"#,
        )
        .unwrap();
    network_ctx.apply_service_payload(
        "network",
        &serde_json::json!({ "available": true, "wifi_enabled": false }),
    );
    network_ctx.call_handler("onToggleWiFi", &[]).unwrap();
    let network_requests =
        crate::shell::service::script_events_to_requests(network_ctx.drain_published_events());

    match network_requests.as_slice() {
        [
            CoreRequest::ServiceCall {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.network");
            assert_eq!(command, "set_wifi_enabled");
            assert_eq!(payload, &serde_json::json!({ "enabled": true }));
        }
        other => panic!("expected one mesh.network set_wifi_enabled command, got {other:?}"),
    }
}

#[test]
fn shipped_workspace_button_publishes_focus_workspace_request() {
    let mut catalog = InterfaceCatalog::default();
    catalog.register_contract(mesh_core_service::InterfaceContract {
        interface: "mesh.wm".into(),
        version: mesh_core_service::parse_contract_version("1.0").unwrap(),
        state_fields: Vec::new(),
        methods: vec![mesh_core_service::InterfaceMethod {
            name: "focus_workspace".into(),
            args: vec![mesh_core_service::InterfaceArgument {
                name: "id".into(),
                arg_type: "int".into(),
            }],
            returns: Some("Result".into()),
            coalesce: false.into(),
            state_binding: None,
        }],
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: mesh_core_service::ContractCapabilities::default(),
    });
    catalog.register_provider(mesh_core_service::InterfaceProvider {
        interface: "mesh.wm".into(),
        version: Some("1.0".into()),
        base_module: None,
        provider_module: "@mesh/hyprland-wm".into(),
        backend_name: "Hyprland".into(),
        priority: 100,
    });

    let caps = CapabilitySet::from_ids(["service.wm.read", "service.wm.control"]);
    let mut ctx = mesh_core_scripting::ScriptContext::new("@mesh/navigation-bar", caps).unwrap();
    ctx.set_interface_catalog(catalog);
    ctx.load_script(&shipped_component_script(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/navigation-bar/src/components/workspace-list.mesh"
    ))))
    .unwrap();

    ctx.apply_service_payload("wm", &serde_json::json!({
        "available": true, "active_workspace": 99,
        "workspaces": [{"id":99,"name":"first","windows":1}, {"id":42,"name":"second","windows":1}]
    }));
    assert!(ctx.run_reactive_effects().is_empty());
    ctx.call_handler("onSwitch", &[serde_json::json!(42)])
        .unwrap();
    assert_eq!(
        ctx.state().get("active_offset"),
        Some(serde_json::json!(26))
    );
    let requests = crate::shell::service::script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [
            CoreRequest::ServiceCall {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.wm");
            assert_eq!(command, "focus_workspace");
            assert_eq!(payload, &serde_json::json!({ "id": 42 }));
        }
        other => panic!("expected one mesh.wm focus_workspace command, got {other:?}"),
    }
}

#[test]
fn real_core_surfaces_reject_legacy_service_callback_api_in_shipped_surfaces() {
    let sources = [
        (
            "navigation-bar root",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../modules/frontend/navigation-bar/src/main.mesh"
            )),
        ),
        (
            "navigation-bar volume button",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
            )),
        ),
        (
            "navigation-bar settings button",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../modules/frontend/navigation-bar/src/components/settings-button.mesh"
            )),
        ),
    ];

    for (name, source) in sources {
        assert_no_legacy_service_callbacks(name, source);
    }
}

#[test]
fn real_quick_settings_theme_chips_switch_the_active_theme_mode() {
    let caps = CapabilitySet::from_ids([
        "service.audio.read",
        "service.audio.control",
        "service.brightness.read",
        "service.brightness.control",
        "theme.read",
        "locale.read",
        "service.theme.control",
    ]);
    let mut ctx = mesh_core_scripting::ScriptContext::new("@mesh/quick-settings", caps).unwrap();
    let mut catalog = audio_network_catalog();
    catalog.register_contract(mesh_core_service::InterfaceContract {
        interface: "mesh.brightness".into(),
        version: mesh_core_service::parse_contract_version("1.0").unwrap(),
        state_fields: Vec::new(),
        methods: Vec::new(),
        events: Vec::new(),
        types: Default::default(),
        capabilities: Default::default(),
    });
    catalog.register_provider(mesh_core_service::InterfaceProvider {
        interface: "mesh.brightness".into(),
        version: Some("1.0".into()),
        base_module: None,
        provider_module: "@mesh/backlight-brightness".into(),
        backend_name: "Backlight".into(),
        priority: 100,
    });
    ctx.set_interface_catalog(catalog);
    ctx.load_script(&shipped_component_script(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/quick-settings/src/main.mesh"
    ))))
    .unwrap();

    ctx.apply_service_payload(
        "theme",
        &serde_json::json!({
            "current": "mesh-default", "theme_id": "mesh-default", "mode": "dark", "is_dark": true,
            "modes": [
                { "name": "dark", "color_scheme": "dark", "contrast": "normal" },
                { "name": "light", "color_scheme": "light", "contrast": "normal" }
            ]
        }),
    );
    ctx.run_reactive_effects();
    assert_eq!(
        ctx.state().get("light_mode"),
        Some(serde_json::json!("light"))
    );
    ctx.call_handler("onThemeLight", &[]).unwrap();
    let requests = crate::shell::service::script_events_to_requests(ctx.drain_published_events());
    match requests.as_slice() {
        [
            CoreRequest::ServiceCall {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.theme");
            assert_eq!(command, "set_mode");
            assert_eq!(payload, &serde_json::json!({ "mode": "light" }));
        }
        other => panic!("expected one mesh.theme set_mode command, got {other:?}"),
    }

    // A dark-only theme keeps its identity: Light is unavailable, not a theme switch.
    ctx.apply_service_payload(
        "theme",
        &serde_json::json!({
            "current": "nord", "theme_id": "nord", "mode": "dark", "is_dark": true,
            "modes": [{ "name": "dark", "color_scheme": "dark", "contrast": "normal" }]
        }),
    );
    ctx.run_reactive_effects();
    assert_eq!(ctx.state().get("light_mode"), None);
    ctx.call_handler("onThemeLight", &[]).unwrap();
    assert!(
        crate::shell::service::script_events_to_requests(ctx.drain_published_events()).is_empty()
    );
}
