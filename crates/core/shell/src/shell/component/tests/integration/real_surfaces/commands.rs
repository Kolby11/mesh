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
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            }
            | CoreRequest::ServiceCall {
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
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            }
            | CoreRequest::ServiceCall {
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
            coalesce: false,
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

    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.wm.read"));
    caps.grant(Capability::new("service.wm.control"));
    let mut ctx = mesh_core_scripting::ScriptContext::new("@mesh/navigation-bar", caps).unwrap();
    ctx.set_interface_catalog(catalog);
    ctx.load_script(&shipped_component_script(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/navigation-bar/src/components/workspace-list.mesh"
    ))))
    .unwrap();

    ctx.call_handler("onSwitch2", &[]).unwrap();
    assert_eq!(
        ctx.state().get("indicator_style"),
        Some(serde_json::json!("transform: translateX(26px);"))
    );
    let requests = crate::shell::service::script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            }
            | CoreRequest::ServiceCall {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.wm");
            assert_eq!(command, "focus_workspace");
            assert_eq!(payload, &serde_json::json!({ "id": 2 }));
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
