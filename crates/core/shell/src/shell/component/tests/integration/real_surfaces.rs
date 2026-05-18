use super::*;

fn assert_phase44_focused_proof_snapshot(component: &FrontendSurfaceComponent, label: &str) {
    let snapshot = component
        .last_focused_proof_snapshot()
        .unwrap_or_else(|| panic!("{label} should store a focused proof snapshot"));
    assert!(
        !snapshot.nodes.is_empty(),
        "{label} should retain node proof evidence"
    );
    assert!(
        snapshot
            .paint
            .iter()
            .any(|paint| matches!(paint.display_slot, "Text" | "Icon")),
        "{label} should include text or icon paint proof evidence"
    );
    assert!(
        !snapshot.accessibility.is_empty(),
        "{label} should retain accessibility proof evidence"
    );
}

fn assert_layout_contains(parent: &WidgetNode, child: &WidgetNode, label: &str) {
    assert!(
        parent.layout.width > 0.0 && parent.layout.height > 0.0,
        "{label} parent should have non-zero layout"
    );
    assert!(
        child.layout.width > 0.0 && child.layout.height > 0.0,
        "{label} child should have non-zero layout"
    );
    assert!(
        child.layout.x >= parent.layout.x - 0.5
            && child.layout.y >= parent.layout.y - 0.5
            && child.layout.x + child.layout.width <= parent.layout.x + parent.layout.width + 0.5
            && child.layout.y + child.layout.height <= parent.layout.y + parent.layout.height + 0.5,
        "{label} child layout {:?} should stay inside parent layout {:?}",
        child.layout,
        parent.layout
    );
}

#[test]
fn phase47_navigation_and_audio_surfaces_keep_taffy_layout_geometry() {
    let theme = default_theme();

    let mut navigation =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    navigation.set_profiling_enabled(true);
    navigation.visible = true;
    let mut navigation_buffer = PixelBuffer::new(960, 80);
    navigation
        .paint(&theme, 960, 80, &mut navigation_buffer)
        .unwrap();
    let navigation_tree = navigation
        .last_tree
        .as_ref()
        .expect("@mesh/navigation-bar rendered tree");
    let nav_shell =
        first_node_with_attr(navigation_tree, "class", "nav-shell").expect("navigation shell");
    let volume_button = first_node_with_click_handler(
        navigation_tree,
        "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface",
    )
    .expect("volume button");
    assert_layout_contains(
        nav_shell,
        volume_button,
        "@mesh/navigation-bar volume button",
    );
    assert_phase44_focused_proof_snapshot(&navigation, "phase47 navigation bar");
    assert!(
        navigation.take_invalidation_snapshot().is_some(),
        "phase47 navigation repaint should retain invalidation proof"
    );
    assert!(
        navigation.take_present_damage().is_some(),
        "phase47 navigation repaint should retain damage proof"
    );

    let mut audio = real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    audio.set_profiling_enabled(true);
    audio
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();
    let mut audio_buffer = PixelBuffer::new(320, 220);
    audio.paint(&theme, 320, 220, &mut audio_buffer).unwrap();
    let audio_tree = audio
        .last_tree
        .as_ref()
        .expect("@mesh/audio-popover rendered tree");
    let slider = first_node_by_tag(audio_tree, "slider").expect("audio controls slider");
    assert_layout_contains(audio_tree, slider, "@mesh/audio-popover controls");
    assert_phase44_focused_proof_snapshot(&audio, "phase47 audio popover");
    assert!(
        audio.take_invalidation_snapshot().is_some(),
        "phase47 audio repaint should retain invalidation proof"
    );
    assert!(
        audio.take_present_damage().is_some(),
        "phase47 audio repaint should retain damage proof"
    );
}

#[test]
fn phase44_navigation_audio_surface_emits_focused_proof_snapshot() {
    let theme = default_theme();

    let mut navigation =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    navigation.visible = true;
    let mut navigation_buffer = PixelBuffer::new(960, 80);
    navigation
        .paint(&theme, 960, 80, &mut navigation_buffer)
        .unwrap();
    assert_phase44_focused_proof_snapshot(&navigation, "navigation bar");

    let mut audio = real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    audio
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();
    let mut audio_buffer = PixelBuffer::new(320, 220);
    audio.paint(&theme, 320, 220, &mut audio_buffer).unwrap();
    assert_phase44_focused_proof_snapshot(&audio, "audio popover");
}

#[test]
fn navigation_volume_button_second_click_hides_audio_surface_via_parent_handler() {
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
    component.paint(&theme, 220, 80, &mut buffer).unwrap();
    let tree = component.last_tree.as_ref().expect("rendered tree");
    let button = first_node_by_tag(tree, "button").expect("button node");
    let handler = button
        .event_handlers
        .get("click")
        .expect("click handler")
        .clone();

    let click_event = serde_json::json!({
        "current_target": {
            "position": {
                "margin_left": 32,
                "margin_bottom": 40
            }
        }
    });
    component
        .call_namespaced_handler(&handler, std::slice::from_ref(&click_event))
        .unwrap();
    component.paint(&theme, 220, 80, &mut buffer).unwrap();
    let show_requests = component.tick().unwrap();
    assert!(matches!(
        show_requests.as_slice(),
        [CoreRequest::ShowSurface { surface_id }] if surface_id == "@mesh/audio-popover"
    ));

    let requests = component
        .call_namespaced_handler(&handler, &[click_event])
        .unwrap();
    assert!(
        requests.is_empty(),
        "closing toggle should not publish direct shell events"
    );
    assert!(runtime_bool(&component, "audio_surface_hidden"));

    component.paint(&theme, 220, 80, &mut buffer).unwrap();
    let requests = component.tick().unwrap();
    match requests.as_slice() {
        [CoreRequest::HideSurface { surface_id }] => {
            assert_eq!(surface_id, "@mesh/audio-popover");
        }
        other => {
            panic!("expected audio popover hide request from portal visibility, got {other:?}")
        }
    }
}

#[test]
fn shipped_navigation_volume_button_publishes_immediate_audio_popover_show() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(320, 80);
    component.paint(&theme, 320, 80, &mut buffer).unwrap();
    let handler = "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface";
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");
    let button = first_node_with_click_handler(tree, handler).expect("volume button");
    let click_handler = button.event_handlers.get("click").unwrap().clone();

    let requests = component
        .call_namespaced_handler(
            &click_handler,
            &[serde_json::json!({
                "surface": {
                    "id": "@mesh/navigation-bar"
                },
                "current": {
                    "key": button.attributes.get("_mesh_key").cloned().unwrap_or_default()
                },
                "current_target": {
                    "key": button.attributes.get("_mesh_key").cloned().unwrap_or_default(),
                    "position": {
                        "margin_left": 32,
                        "margin_bottom": 40
                    }
                }
            })],
        )
        .unwrap();

    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::PositionSurface {
                surface_id,
                margin_top: 0,
                margin_left: 32
            } if surface_id == "@mesh/audio-popover"
        )),
        "click should position the audio popover before showing it: {requests:?}"
    );
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ActivatePopover { surface_id, .. } if surface_id == "@mesh/audio-popover"
        )),
        "click should register popover activation through the shell request path: {requests:?}"
    );
    assert!(!runtime_bool(&component, "audio_surface_hidden"));
}

#[test]
fn shipped_navigation_volume_icon_inherits_button_click_and_tooltip() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();
    component.visible = true;

    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component.paint(&theme, width, height, &mut buffer).unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");
    let button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar::onToggleAudioSurface",
    )
    .expect("volume button");
    let button_key = button
        .attributes
        .get("_mesh_key")
        .expect("button mesh key")
        .clone();
    let icon = first_node_by_tag(button, "icon").expect("volume icon");
    let icon_key = icon
        .attributes
        .get("_mesh_key")
        .expect("icon mesh key")
        .clone();
    let (button_left, button_top, _button_right, _button_bottom) =
        find_node_bounds_by_key(tree, &button_key, 0.0, 0.0).expect("button bounds");
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &icon_key, 0.0, 0.0).expect("icon bounds");
    let button_x = button_left + 1.0;
    let button_y = button_top + 1.0;
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;

    assert_eq!(
        find_tooltip_text_by_key(tree, &icon_key).as_deref(),
        Some("Volume unavailable"),
        "tooltip lookup should inherit the button title when hovering the icon"
    );

    component
        .handle_input(&theme, width, height, ComponentInput::PointerMove { x, y })
        .unwrap();
    assert!(
        component.hover_start.is_some(),
        "hovering the icon should start the inherited button tooltip timer"
    );
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerMove {
                x: button_x,
                y: button_y,
            },
        )
        .unwrap();
    let preserved_hover_start = std::time::Instant::now() - std::time::Duration::from_secs(1);
    component.hover_start = Some(preserved_hover_start);
    component
        .handle_input(&theme, width, height, ComponentInput::PointerMove { x, y })
        .unwrap();
    assert_eq!(
        component.hover_start,
        Some(preserved_hover_start),
        "moving from a tooltip owner to a descendant inheriting the same tooltip should not restart the tooltip"
    );

    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: true,
            },
        )
        .unwrap();
    let requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::PointerButton {
                x,
                y,
                pressed: false,
            },
        )
        .unwrap();

    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ActivatePopover { surface_id, .. } if surface_id == "@mesh/audio-popover"
        )),
        "clicking directly on the icon should bubble to the button click handler: {requests:?}"
    );
}

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
        audio.set_volume("default", percent / 100)
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
            },
        ] => {
            assert_eq!(interface, "mesh.audio");
            assert_eq!(command, "set_volume");
            assert_eq!(
                payload,
                &serde_json::json!({ "device_id": "default", "volume": 0.55 })
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
fn debug_inspector_overview_renders_profiling_off_state_on_real_surface() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": false,
                "profiling_session_id": 3,
                "active_view": "overview",
                "modules": [{ "id": "@mesh/debug-inspector" }],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": ["@mesh/debug-inspector"],
                "profiling": serde_json::Value::Null
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(360, 640);
    component.paint(&theme, 360, 640, &mut buffer).unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Debug Inspector"));
    assert!(text.iter().any(|line| line == "Profiling is off"));
    assert!(text.iter().any(|line| line.contains("Enable profiling")));
    assert!(text.iter().any(|line| line == "Start profiling"));
    assert!(
        runtime_value(&component, "active_view")
            .and_then(|value| value.as_str().map(str::to_string))
            .as_deref()
            == Some("overview")
    );
}

#[test]
fn debug_inspector_all_four_views_keep_stable_empty_or_pending_states_on_real_surface() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(360, 640);

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 9,
                "active_view": "overview",
                "modules": [{ "id": "@mesh/debug-inspector" }],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": [],
                "profiling": {
                    "session_id": 9,
                    "shell": {
                        "stages": [],
                        "redraw_count": 0,
                        "total_surface_render_time_micros": 0
                    },
                    "surfaces": [],
                    "backends": []
                }
            }),
        })
        .unwrap();

    component.paint(&theme, 360, 640, &mut buffer).unwrap();
    let overview_text = rendered_text(&component);
    assert!(overview_text.iter().any(|line| line == "Overview"));
    assert!(
        overview_text
            .iter()
            .any(|line| line == "No recent samples yet")
    );

    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showSurfaces", &[])
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer).unwrap();
    let surfaces_text = rendered_text(&component);
    assert!(surfaces_text.iter().any(|line| line == "Surfaces"));
    assert!(
        surfaces_text
            .iter()
            .any(|line| line == "No recent surface activity")
    );

    component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/debug-inspector::showBackendServices",
            &[],
        )
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer).unwrap();
    let backend_text = rendered_text(&component);
    assert!(backend_text.iter().any(|line| line == "Backend services"));
    assert!(
        backend_text
            .iter()
            .any(|line| line == "No backend samples yet")
    );

    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showBenchmark", &[])
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer).unwrap();
    let benchmark_text = rendered_text(&component);
    assert!(
        benchmark_text
            .iter()
            .any(|line| line == "Benchmark / Interaction")
    );
    assert!(
        benchmark_text
            .iter()
            .any(|line| line.contains("Run fixed shell interactions"))
    );
    for label in [
        "Hover",
        "Surface open/close",
        "Pointer-driven update",
        "Keyboard traversal",
        "Backend-driven update",
    ] {
        assert!(
            benchmark_text.iter().any(|line| line == label),
            "benchmark scaffold should render {label}"
        );
    }
}

#[test]
fn debug_inspector_surfaces_view_renders_empty_and_live_rows_on_real_surface() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(360, 640);

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 4,
                "active_view": "overview",
                "modules": [],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": [],
                "profiling": {
                    "session_id": 4,
                    "shell": {
                        "stages": [],
                        "redraw_count": 0,
                        "total_surface_render_time_micros": 0
                    },
                    "surfaces": [],
                    "backends": []
                }
            }),
        })
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer).unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showSurfaces", &[])
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer).unwrap();

    let empty_text = rendered_text(&component);
    assert!(empty_text.iter().any(|line| line == "Surfaces"));
    assert!(
        empty_text
            .iter()
            .any(|line| line == "No recent surface activity")
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 4,
                "active_view": "overview",
                "modules": [],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": ["@mesh/navigation-bar"],
                "profiling": {
                    "session_id": 4,
                    "shell": {
                        "stages": [{
                            "stage": "paint",
                            "sample_count": 2,
                            "total_micros": 42,
                            "max_micros": 24,
                            "recent_samples": []
                        }],
                        "redraw_count": 2,
                        "total_surface_render_time_micros": 128
                    },
                    "surfaces": [{
                        "surface_id": "@mesh/navigation-bar",
                        "module_id": "@mesh/navigation-bar",
                        "stages": [{
                            "stage": "paint",
                            "sample_count": 2,
                            "total_micros": 42,
                            "max_micros": 24,
                            "recent_samples": []
                        }],
                        "redraw_count": 2,
                        "total_surface_render_time_micros": 128
                    }],
                    "backends": []
                }
            }),
        })
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer).unwrap();

    let live_text = rendered_text(&component);
    assert!(live_text.iter().any(|line| line == "@mesh/navigation-bar"));
    assert!(
        live_text
            .iter()
            .any(|line| line.contains("paint: 42us across 2 samples"))
    );
    assert!(
        live_text
            .iter()
            .any(|line| line.contains("Total render 128us"))
    );
}
