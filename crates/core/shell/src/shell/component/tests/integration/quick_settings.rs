use super::*;

#[test]
fn quick_settings_wifi_row_empty_id_is_display_only() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
network_id = ""
connection_status = ""

function onConnectWiFi()
    if not network_id or network_id == "" then
        connection_status = "Connection details unavailable"
        return
    end

    local ok, network = pcall(require, "mesh.network@>=1.0")
    if ok and network and network.available ~= false then
        network.connect(network_id)
    end
end
"#,
    )
    .unwrap();

    ctx.call_handler("onConnectWiFi", &[]).unwrap();
    let requests = crate::shell::service::script_events_to_requests(ctx.drain_published_events());

    assert!(
        requests.is_empty(),
        "empty network_id must not publish connect"
    );
    assert_eq!(
        ctx.state.get("connection_status"),
        Some(serde_json::json!("Connection details unavailable"))
    );
}

#[test]
fn quick_settings_wifi_row_publishes_connect_for_wifi_network_ids() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
network_id = "wifi:OfficeNet"

function onConnectWiFi()
    local ok, network = pcall(require, "mesh.network@>=1.0")
    if ok and network and network.available ~= false then
        network.connect(network_id)
    end
end
"#,
    )
    .unwrap();
    ctx.apply_service_payload("network", &serde_json::json!({ "available": true }));

    ctx.call_handler("onConnectWiFi", &[]).unwrap();
    let requests = crate::shell::service::script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.network");
            assert_eq!(command, "connect");
            assert_eq!(
                payload,
                &serde_json::json!({ "connection_id": "wifi:OfficeNet" })
            );
        }
        other => panic!("expected one mesh.network connect command, got {other:?}"),
    }
}

#[test]
fn quick_settings_bluetooth_chip_selects_bluetooth_section() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
active_section = "wifi"
wifi_nav_class = "nav-btn nav-active"
bt_nav_class = "nav-btn"
audio_nav_class = "nav-btn"

function sync_nav_classes()
    wifi_nav_class = active_section == "wifi" and "nav-btn nav-active" or "nav-btn"
    bt_nav_class = active_section == "bluetooth" and "nav-btn nav-active" or "nav-btn"
    audio_nav_class = active_section == "audio" and "nav-btn nav-active" or "nav-btn"
end

function onSelectBluetooth()
    if active_section == "bluetooth" then
        active_section = ""
    else
        active_section = "bluetooth"
    end
    sync_nav_classes()
end

function onToggleBluetooth()
    onSelectBluetooth()
end
"#,
    )
    .unwrap();

    ctx.call_handler("onToggleBluetooth", &[]).unwrap();

    assert_eq!(
        ctx.state.get("active_section"),
        Some(serde_json::json!("bluetooth"))
    );
    assert_eq!(
        ctx.state.get("bt_nav_class"),
        Some(serde_json::json!("nav-btn nav-active"))
    );
    assert_eq!(
        ctx.state.get("wifi_nav_class"),
        Some(serde_json::json!("nav-btn"))
    );
}

#[test]
fn real_core_surfaces_quick_settings_close_publishes_hide_surface() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
function onClose()
    mesh.events.publish("shell.hide-surface", { surface_id = "@mesh/quick-settings" })
end
"#,
    )
    .unwrap();

    ctx.call_handler("onClose", &[]).unwrap();
    let requests = crate::shell::service::script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [CoreRequest::HideSurface { surface_id }] => {
            assert_eq!(surface_id, "@mesh/quick-settings");
        }
        other => panic!("expected quick settings HideSurface request, got {other:?}"),
    }
}

#[test]
fn navigation_volume_button_opens_audio_surface_via_parent_handler() {
    let button_component = parse_component(
        r#"
<template>
  <button onclick={onActivate}>Volume</button>
</template>

<script lang="luau">
function onActivate()
end
</script>
"#,
    )
    .unwrap();
    let root_component = parse_component(
        r#"
<template>
  <row>
    <VolumeButton onActivate={onToggleAudioSurface} />
    <AudioPopover hidden={audio_surface_hidden} />
  </row>
</template>

<script lang="luau">
import AudioPopover from "@mesh/audio-popover"
import VolumeButton from "./components/volume-button.mesh"

audio_surface_id = "@mesh/audio-popover"
audio_surface_hidden = true

function onToggleAudioSurface(event)
    local position = event.current_target.position or {}
    local margin_left = tonumber(position.margin_left) or 0
    local margin_top = 0

    if audio_surface_hidden then
        mesh.events.publish("shell.position-surface", {
            surface_id = audio_surface_id,
            margin_top = margin_top,
            margin_left = margin_left
        })
    end

    audio_surface_hidden = not audio_surface_hidden
end
</script>
"#,
    )
    .unwrap();
    let popover_component = parse_component("<template><box /></template>").unwrap();

    let mut root_manifest = minimal_test_manifest("@mesh/navigation-bar");
    root_manifest.dependencies.modules.insert(
        "@mesh/audio-popover".into(),
        mesh_core_module::manifest::DependencySpec::Simple(">=0.1.0".into()),
    );
    let popover_manifest = minimal_test_manifest("@mesh/audio-popover");

    let root_compiled = CompiledFrontendModule {
        manifest: root_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: root_component,
        local_components: HashMap::from([("VolumeButton".into(), button_component)]),
        module_component_imports: HashMap::from([(
            "AudioPopover".into(),
            "@mesh/audio-popover".into(),
        )]),
        watched_paths: Vec::new(),
    };
    let popover_compiled = CompiledFrontendModule {
        manifest: popover_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: popover_component,
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                "@mesh/navigation-bar".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: root_compiled.clone(),
                },
            ),
            (
                "@mesh/audio-popover".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: popover_compiled,
                },
            ),
        ]),
        slot_contributions: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        root_compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
    );
    component
        .mount(ComponentContext {
            component_id: "@mesh/navigation-bar".into(),
            surface_id: "@mesh/navigation-bar".into(),
            diagnostics: Diagnostics::new("@mesh/navigation-bar"),
        })
        .unwrap();
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(220, 80);
    component.paint(&theme, 220, 80, &mut buffer, 1.0).unwrap();
    let tree = component.last_tree.as_ref().expect("rendered tree");
    let button = first_node_by_tag(tree, "button").expect("button node");
    let handler = button
        .event_handlers
        .get("click")
        .expect("click handler")
        .clone();

    assert_eq!(
        handler,
        "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface"
    );

    let requests = component
        .call_namespaced_handler(
            &handler,
            &[serde_json::json!({
                "current_target": {
                    "position": {
                        "margin_left": 32,
                        "margin_bottom": 40
                    }
                }
            })],
        )
        .unwrap();

    match requests.as_slice() {
        [
            CoreRequest::PositionSurface {
                surface_id,
                margin_top,
                margin_left,
            },
        ] => {
            assert_eq!(surface_id, "@mesh/audio-popover");
            assert_eq!(*margin_left, 32);
            assert_eq!(*margin_top, 0);
        }
        other => panic!("expected audio popover position request, got {other:?}"),
    }

    assert!(!runtime_bool(&component, "audio_surface_hidden"));

    component.paint(&theme, 220, 80, &mut buffer, 1.0).unwrap();
    let visibility_requests = component.tick().unwrap();
    match visibility_requests.as_slice() {
        [CoreRequest::ShowSurface { surface_id }] => {
            assert_eq!(surface_id, "@mesh/audio-popover");
        }
        other => {
            panic!("expected audio popover show request from portal visibility, got {other:?}")
        }
    }

    component
        .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
            surface_id: "@mesh/audio-popover".into(),
            visible: false,
        })
        .unwrap();
    assert!(
        runtime_bool(&component, "audio_surface_hidden"),
        "keyboard-driven popover hide must sync the portal owner script state"
    );

    component.paint(&theme, 220, 80, &mut buffer, 1.0).unwrap();
    let requests = component.tick().unwrap();
    assert!(
        requests.is_empty(),
        "synced portal state must not immediately re-show the keyboard-hidden popover"
    );
}
