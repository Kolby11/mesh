use super::*;
use mesh_core_service::contract::ContractStateField;
use mesh_core_service::{
    ContractCapabilities, InterfaceArgument, InterfaceContract, InterfaceMethod, InterfaceProvider,
    parse_contract_version,
};
use std::collections::HashMap;

#[test]
fn settings_loads_additional_font_pages_once_and_discards_stale_results() {
    let theme = default_theme();
    let mut component = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);
    let extent = SurfaceExtent::unpadded(920, 900);
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "current": "nord", "theme_id": "nord", "is_dark": true,
                "scope": "user", "can_pin": true, "themes": [], "available": [],
                "system_resources": {
                    "font_revision": "catalog-one", "font_total": 72,
                    "font_families": (0..24).map(|index| serde_json::json!({
                        "name": format!("Test Font {index:02}"), "face_count": 1, "monospace": false
                    })).collect::<Vec<_>>()
                }
            }),
        })
        .unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let handler = "__mesh_embed__::@mesh/settings/local:AppearancePage::onShowMoreFonts";
    for (offset, revision) in [(24, "catalog-one"), (48, "stale-catalog")] {
        let mut requests = component.call_namespaced_handler(handler, &[]).unwrap();
        requests.extend(component.tick().unwrap());
        let calls = requests
            .iter()
            .filter_map(|request| match request {
                CoreRequest::ServiceCall {
                    command,
                    payload,
                    call_id,
                    source_instance_id,
                    ..
                } if command == "list_font_families" => {
                    Some((payload, *call_id, source_instance_id))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            calls.len(),
            1,
            "{requests:?}; rendered: {:?}",
            rendered_text(&component)
        );
        let (payload, call_id, instance) = calls[0];
        assert_eq!(payload["offset"], offset);
        let mut duplicate = component.call_namespaced_handler(handler, &[]).unwrap();
        duplicate.extend(component.tick().unwrap());
        assert!(!duplicate.iter().any(|request| matches!(request, CoreRequest::ServiceCall { command, .. } if command == "list_font_families")));
        assert!(component.deliver_service_call_result(
            instance,
            call_id,
            "ok",
            &serde_json::json!({
                "ok": true, "revision": revision, "total": 72,
                "families": (offset..offset + 24).map(|index| serde_json::json!({
                    "name": format!("Test Font {index:02}"), "face_count": 1, "monospace": false
                })).collect::<Vec<_>>()
            })
        ));
        component.tick().unwrap();
        component
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
        let text = rendered_text(&component);
        assert!(text.iter().any(|line| line == "Test Font 47"));
        assert!(!text.iter().any(|line| line == "Test Font 48"));
        assert_eq!(
            text.iter()
                .filter(|line| line.as_str() == "Test Font 24")
                .count(),
            1
        );
    }
}

#[test]
fn debug_inspector_backend_services_view_separates_runtime_health_and_timing_stages() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    // Paint once so the inspector's script reads (and thus tracks) its
    // `mesh.debug` state fields before the first service event; otherwise the
    // runtime does not yet observe the event. The real shell seeds cached
    // service payloads at mount, which this direct-dispatch test bypasses.
    {
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(360, 720);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
            .unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 7,
                "modules": [],
                "interfaces": [],
                "backend_runtimes": [
                    {
                        "interface": "mesh.audio",
                        "provider_id": "@mesh/pipewire-audio",
                        "status": "stopped",
                        "message": "Old provider stopped",
                        "failure_count": 0
                    },
                    {
                        "interface": "mesh.audio",
                        "provider_id": "@mesh/pulseaudio-audio",
                        "status": "running",
                        "message": "Polling steadily",
                        "failure_count": 0
                    }
                ],
                "active_surfaces": [],
                "profiling": {
                    "session_id": 7,
                    "shell": {
                        "stages": [{
                            "stage": "paint",
                            "sample_count": 1,
                            "total_micros": 10,
                            "max_micros": 10,
                            "recent_samples": []
                        }],
                        "redraw_count": 1,
                        "total_surface_render_time_micros": 10
                    },
                    "surfaces": [],
                    "backends": [{
                        "interface": "mesh.audio",
                        "provider_id": "@mesh/pulseaudio-audio",
                        "stages": [
                            {
                                "stage": "poll_update",
                                "sample_count": 3,
                                "total_micros": 90,
                                "max_micros": 40,
                                "recent_samples": []
                            },
                            {
                                "stage": "command_handling",
                                "sample_count": 1,
                                "total_micros": 25,
                                "max_micros": 25,
                                "recent_samples": []
                            },
                            {
                                "stage": "state_publish_delivery",
                                "sample_count": 2,
                                "total_micros": 30,
                                "max_micros": 18,
                                "recent_samples": []
                            }
                        ]
                    }]
                }
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(360, 640);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();
    component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/debug-inspector::showBackendServices",
            &[],
        )
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Backend services"));
    assert!(text.iter().any(|line| line == "Runtime health"));
    assert!(text.iter().any(|line| line == "Timing stages"));
    assert!(
        text.iter()
            .any(|line| line.contains("running: Polling steadily"))
    );
    assert!(
        !text
            .iter()
            .any(|line| line.contains("stopped: Old provider stopped"))
    );
    assert!(text.iter().any(|line| line.contains("poll_update")));
    assert!(
        text.iter()
            .any(|line| line.contains("90us across 3 samples"))
    );
    assert!(text.iter().any(|line| line.contains("command_handling")));
    assert!(
        text.iter()
            .any(|line| line.contains("25us across 1 samples"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("state_publish_delivery"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("30us across 2 samples"))
    );
    assert!(
        runtime_value(&component, "active_view")
            .and_then(|value| value.as_str().map(str::to_string))
            .as_deref()
            == Some("backend_services")
    );
}

#[test]
fn debug_inspector_surfaces_view_renders_retained_paint_filtering_counters() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    // Paint once so the inspector's script reads (and thus tracks) its
    // `mesh.debug` state fields before the first service event; otherwise the
    // runtime does not yet observe the event. The real shell seeds cached
    // service payloads at mount, which this direct-dispatch test bypasses.
    {
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(360, 720);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
            .unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 29,
                "modules": [{ "id": "@mesh/debug-inspector" }],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": ["@mesh/navigation-bar"],
                "benchmarks": {
                    "scenarios": []
                },
                "profiling": {
                    "session_id": 29,
                    "shell": {
                        "stages": [],
                        "redraw_count": 1,
                        "total_surface_render_time_micros": 55
                    },
                    "surfaces": [
                        {
                            "surface_id": "@mesh/navigation-bar",
                            "module_id": "@mesh/navigation-bar",
                            "stages": [{
                                "stage": "paint",
                                "sample_count": 1,
                                "total_micros": 41,
                                "max_micros": 41,
                                "recent_samples": []
                            }],
                            "redraw_count": 2,
                            "total_surface_render_time_micros": 96,
                            "invalidation": {
                                "paint": {
                                    "repaint_policy": "minimal_damage",
                                    "filtered_span_count": 3,
                                    "filtered_command_count": 7,
                                    "filtered_commands_skipped": 12,
                                    "filtered_fallback_count": 0
                                }
                            }
                        },
                        {
                            "surface_id": "@mesh/audio-popover",
                            "module_id": "@mesh/audio-popover",
                            "stages": [],
                            "redraw_count": 1,
                            "total_surface_render_time_micros": 20,
                            "invalidation": {
                                "paint": {
                                    "repaint_policy": "minimal_damage"
                                }
                            }
                        }
                    ],
                    "backends": []
                }
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(360, 640);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showSurfaces", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(
        text.iter().any(|line| line == "Surfaces"),
        "Surfaces view missing; rendered text: {text:?}"
    );
    assert!(
        text.iter()
            .any(|line| line == "Paint policy minimal_damage; fallbacks 0")
    );
    assert!(
        text.iter()
            .any(|line| line == "Filtered 7 commands from 3 spans; skipped 12")
    );
    assert!(text.iter().any(|line| line == "Paint policy unavailable"));
    assert!(
        text.iter()
            .any(|line| line == "Filtered paint counters unavailable")
    );
}

#[test]
fn debug_inspector_modules_view_renders_uses_provides_graph() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    {
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(360, 720);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
            .unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": false,
                "profiling_session_id": 31,
                "modules": [{ "id": "@mesh/navigation-bar" }],
                "module_graph": [
                    {
                        "module_id": "@mesh/navigation-bar",
                        "kind": "ui",
                        "enabled": true,
                        "root_layout": true,
                        "path": "modules/frontend/navigation-bar/module.json",
                        "uses": {
                            "modules": ["@mesh/audio-popover"],
                            "interfaces": ["mesh.audio", "mesh.power"],
                            "optional_interfaces": ["mesh.brightness"],
                            "icon_packs": ["@mesh/icons-default"],
                            "i18n_packs": [],
                            "theme_packs": [],
                            "font_packs": []
                        },
                        "capabilities": ["shell.surface"],
                        "optional_capabilities": [],
                        "provides": {
                            "interfaces": [],
                            "settings": ["@mesh/navigation-bar"],
                            "i18n": ["en:config/i18n/en.json"],
                            "required_icons": ["battery-caution", "audio-volume-high"],
                            "optional_icons": []
                        },
                        "diagnostics": []
                    },
                    {
                        "module_id": "@mesh/pipewire-audio",
                        "kind": "backend",
                        "enabled": true,
                        "root_layout": false,
                        "path": "modules/backend/pipewire-audio/module.json",
                        "uses": {
                            "modules": [],
                            "interfaces": [],
                            "optional_interfaces": [],
                            "icon_packs": [],
                            "i18n_packs": [],
                            "theme_packs": [],
                            "font_packs": []
                        },
                        "capabilities": ["service.audio.read"],
                        "optional_capabilities": [],
                        "provides": {
                            "interfaces": ["mesh.audio"],
                            "settings": [],
                            "i18n": [],
                            "required_icons": [],
                            "optional_icons": []
                        },
                        "diagnostics": ["optional backend mesh.brightness has no active provider"]
                    }
                ],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": ["@mesh/debug-inspector"],
                "benchmarks": {
                    "scenarios": []
                },
                "profiling": serde_json::Value::Null
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(360, 720);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
        .unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showModules", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Modules"));
    assert!(text.iter().any(|line| line == "@mesh/navigation-bar"));
    assert!(
        text.iter()
            .any(|line| line.contains("Interfaces: mesh.audio, mesh.power"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("optional mesh.brightness"))
    );

    component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/debug-inspector::onModuleFilterChange",
            &[serde_json::json!({ "value": "pipewire" })],
        )
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
        .unwrap();
    let filtered_text = rendered_text(&component);
    assert!(
        filtered_text
            .iter()
            .any(|line| line == "@mesh/pipewire-audio")
    );
    assert!(
        !filtered_text
            .iter()
            .any(|line| line == "@mesh/navigation-bar")
    );
    assert!(
        filtered_text
            .iter()
            .any(|line| line == "1 of 2 installed graph entries match.")
    );
    assert!(
        text.iter()
            .any(|line| line.contains("Resources: icons @mesh/icons-default"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("settings @mesh/navigation-bar"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("icons battery-caution"))
    );
    assert!(text.iter().any(|line| line == "Diagnostics: clear"));
    assert!(text.iter().any(|line| line == "@mesh/pipewire-audio"));
    assert!(text.iter().any(|line| line == "Interfaces: mesh.audio"));
    assert!(
        text.iter()
            .any(|line| line.contains("optional backend mesh.brightness"))
    );
    assert!(
        runtime_value(&component, "active_view")
            .and_then(|value| value.as_str().map(str::to_string))
            .as_deref()
            == Some("modules")
    );
}

pub(in crate::shell) fn settings_catalog() -> InterfaceCatalog {
    let mut catalog = debug_catalog();
    let shell = crate::shell::Shell::new();
    let builtins = shell.interfaces.resolved_catalog();
    for interface in [
        "mesh.settings",
        "mesh.packages",
        "mesh.theme",
        "mesh.locale",
        "mesh.keybinds",
    ] {
        for contract in builtins.contracts_for(interface) {
            catalog.register_contract(contract);
        }
        for provider in builtins.providers_for(interface) {
            catalog.register_provider(provider);
        }
    }
    catalog.register_contract(InterfaceContract {
        interface: "mesh.audio".into(),
        version: parse_contract_version("1.0").unwrap(),
        state_fields: vec![
            ContractStateField {
                name: "available".into(),
                field_type: "boolean".into(),
                description: None,
                ..Default::default()
            },
            ContractStateField {
                name: "percent".into(),
                field_type: "float".into(),
                description: None,
                ..Default::default()
            },
            ContractStateField {
                name: "muted".into(),
                field_type: "boolean".into(),
                description: None,
                ..Default::default()
            },
        ],
        methods: vec![
            InterfaceMethod {
                name: "set_volume".into(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "percent".into(),
                        arg_type: "float".into(),
                    },
                ],
                returns: None,
                coalesce: true.into(),
                state_binding: Some(mesh_core_service::StateBinding {
                    field: "percent".into(),
                    from_arg: Some("percent".into()),
                    toggle: false,
                }),
            },
            InterfaceMethod {
                name: "set_muted".into(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "muted".into(),
                        arg_type: "boolean".into(),
                    },
                ],
                returns: None,
                coalesce: true.into(),
                state_binding: None,
            },
        ],
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.audio".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/audio-interface".into()),
        provider_module: "@mesh/pipewire-audio".into(),
        backend_name: "PipeWire".into(),
        priority: 100,
    });
    catalog
}

#[test]
fn settings_surface_renders_backend_pages_and_advanced_controls() {
    let mut component = real_frontend_module_component("@mesh/settings", settings_catalog());
    {
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(920, 700);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
            .unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": false,
                "profiling_enabled": false,
                "profiling_session_id": 0,
                "modules": [],
                "module_graph": [
                    {
                        "module_id": "@mesh/navigation-bar",
                        "kind": "ui",
                        "enabled": true,
                        "path": "modules/frontend/navigation-bar/module.json",
                        "uses": {
                            "modules": ["@mesh/quick-settings"],
                            "interfaces": ["mesh.audio", "mesh.power"],
                            "optional_interfaces": [],
                            "icon_packs": [],
                            "i18n_packs": [],
                            "theme_packs": [],
                            "font_packs": [],
                            "keybinds": ["mute"],
                            "active_providers": [
                                "mesh.audio=@mesh/pipewire-audio"
                            ],
                            "native_binaries": [
                                { "name": "pactl", "optional": false, "available": true }
                            ],
                            "capabilities": ["shell.surface"],
                            "optional_capabilities": ["service.power.read"]
                        },
                        "provides": {
                            "interfaces": [],
                            "settings": ["@mesh/navigation-bar"],
                            "settings_schema": {
                                "type": "object",
                                "properties": {
                                    "blur_enabled": {
                                        "type": "bool",
                                        "default": true,
                                        "label": "Backdrop blur"
                                    }
                                }
                            },
                            "settings_values": { "blur_enabled": false },
                            "settings_instances": ["@mesh/navigation-bar#bottom"],
                            "settings_instance_values": {
                                "@mesh/navigation-bar#bottom": { "blur_enabled": true }
                            },
                            "i18n": ["en:config/i18n/en.json"],
                            "required_icons": ["audio-volume-muted"],
                            "optional_icons": []
                        },
                        "diagnostics": [],
                        "health": []
                    },
                    {
                        "module_id": "@mesh/pipewire-audio",
                        "kind": "backend",
                        "enabled": true,
                        "path": "modules/backend/pipewire-audio/module.json",
                        "uses": {
                            "modules": [],
                            "interfaces": [],
                            "optional_interfaces": [],
                            "icon_packs": [],
                            "i18n_packs": [],
                            "theme_packs": [],
                            "font_packs": [],
                            "active_providers": []
                        },
                        "capabilities": ["service.audio.read"],
                        "optional_capabilities": [],
                        "provides": {
                            "interfaces": [{ "interface": "mesh.audio", "label": "Audio" }],
                            "settings": [],
                            "i18n": [],
                            "required_icons": [],
                            "optional_icons": []
                        },
                        "diagnostics": [],
                        "health": []
                    },
                    {
                        "module_id": "@mesh/pulseaudio-audio",
                        "kind": "backend",
                        "enabled": true,
                        "root_layout": false,
                        "path": "modules/backend/pulseaudio-audio/module.json",
                        "uses": {
                            "modules": [],
                            "interfaces": [],
                            "optional_interfaces": [],
                            "icon_packs": [],
                            "i18n_packs": [],
                            "theme_packs": [],
                            "font_packs": [],
                            "active_providers": []
                        },
                        "capabilities": ["service.audio.read"],
                        "optional_capabilities": [],
                        "provides": {
                            "interfaces": [{ "interface": "mesh.audio", "label": "Audio" }],
                            "settings": [],
                            "i18n": [],
                            "required_icons": [],
                            "optional_icons": []
                        },
                        "diagnostics": [],
                        "health": []
                    },
                    {
                        "module_id": "@mesh/mesh-default-dark",
                        "kind": "theme",
                        "enabled": true,
                        "uses": {},
                        "provides": {
                            "themes": [{ "id": "mesh-default-dark", "label": "MESH Default Dark" }]
                        },
                        "diagnostics": []
                    },
                    {
                        "module_id": "@mesh/mesh-default-light",
                        "kind": "theme",
                        "enabled": true,
                        "uses": {},
                        "provides": {
                            "themes": [{ "id": "mesh-default-light", "label": "MESH Default Light" }]
                        },
                        "diagnostics": []
                    },
                    {
                        "module_id": "@mesh/gruvbox-dark",
                        "kind": "theme",
                        "enabled": true,
                        "uses": {},
                        "provides": {
                            "themes": [{ "id": "gruvbox-dark", "label": "Gruvbox Dark" }]
                        },
                        "diagnostics": []
                    }
                ],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": ["@mesh/settings"],
                "benchmarks": { "scenarios": [] },
                "profiling": serde_json::Value::Null
            }),
        })
        .unwrap();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: "@mesh/core-settings".into(),
            payload: serde_json::json!({
                "current": "gruvbox-dark",
                "theme_id": "gruvbox-dark",
                "is_dark": true,
                "themes": [
                    { "id": "gruvbox-dark", "label": "Gruvbox Dark" }
                ],
                "available": [
                    "gruvbox-dark"
                ],
                "system_resources": {
                    "active_icon_theme": "Papirus-Dark",
                    "active_font_family": "Noto Sans",
                    "icon_themes": [{
                        "id": "Papirus-Dark",
                        "name": "Papirus Dark",
                        "inherits": ["Papirus", "hicolor"]
                    }],
                    "font_families": [{
                        "name": "Noto Sans",
                        "face_count": 4,
                        "monospace": false
                    }]
                }
            }),
        })
        .unwrap();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 64,
                "muted": false
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(920, 700);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Settings"));
    assert!(text.iter().any(|line| line == "Appearance"));
    assert!(text.iter().any(|line| line == "Wi-Fi"));
    assert!(text.iter().any(|line| line == "Bluetooth"));
    assert!(
        text.iter()
            .any(|line| line == "Network service unavailable"),
        "settings Wi-Fi fallback should resolve from the English catalog: {text:?}"
    );

    component
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAudio", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "64%"));
    assert!(text.iter().any(|line| line == "Active audio provider"));

    let requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AudioPage::onVolumeChange",
            &[serde_json::json!(38)],
        )
        .unwrap();
    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::ServiceCall {
            interface,
            command,
            payload,
            ..
        }] if interface == "mesh.audio"
            && command == "set_volume"
            && payload == &serde_json::json!({ "device_id": "default", "percent": 38 })
    ));

    component
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAdvanced", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Active providers"));
    assert!(text.iter().any(|line| line == "mesh.audio"));
    assert!(text.iter().any(|line| line == "@mesh/pipewire-audio"));
    assert!(text.iter().any(|line| line == "Installed modules"));
    assert!(text.iter().any(|line| line == "@mesh/navigation-bar"));
    assert!(text.iter().any(|line| line == "Module preferences"));
    assert!(text.iter().any(|line| line == "Backdrop blur"));

    component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AdvancedPage::onModuleDetails",
            &[serde_json::json!("@mesh/navigation-bar")],
        )
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    let text = rendered_text(&component);
    assert!(
        text.iter().any(|line| {
            line == "Required interfaces: mesh.audio → @mesh/pipewire-audio, mesh.power → UNAVAILABLE"
        }),
        "expanded module details should render required provider bindings: {text:?}"
    );
    assert!(
        text.iter()
            .any(|line| line
                == "Capabilities: Required: shell.surface · Optional: service.power.read")
    );
    assert!(
        text.iter()
            .any(|line| line == "Native binaries: pactl (Available)")
    );
    assert!(
        text.iter()
            .any(|line| line == "I18n catalogs: en:config/i18n/en.json")
    );
    assert!(text.iter().any(|line| line == "Keybinds: mute"));
    assert!(text.iter().any(|line| line == "Health: Healthy"));

    let requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AdvancedPage::onPropToggle",
            &[
                serde_json::json!("@mesh/navigation-bar"),
                serde_json::json!("blur_enabled"),
                serde_json::json!(false),
            ],
        )
        .unwrap();
    assert!(
        matches!(
            requests.as_slice(),
            [CoreRequest::ServiceCall { interface, command, payload, .. }
                | CoreRequest::ServiceCall { interface, command, payload, .. }]
                if interface == "mesh.settings"
                    && command == "set_prop"
                    && payload.get("module_id")
                        == Some(&serde_json::json!("@mesh/navigation-bar"))
                    && payload.get("prop") == Some(&serde_json::json!("blur_enabled"))
                    && payload.get("value") == Some(&serde_json::json!(true))
                    && payload.get("instance_id").is_none_or(serde_json::Value::is_null)
        ),
        "{requests:?}"
    );

    component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AdvancedPage::onPropScopeNext",
            &[
                serde_json::json!("@mesh/navigation-bar"),
                serde_json::json!(""),
                serde_json::json!(["@mesh/navigation-bar#bottom"]),
            ],
        )
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    let scoped_text = rendered_text(&component);
    assert!(
        scoped_text
            .iter()
            .any(|line| line == "@mesh/navigation-bar#bottom"),
        "instance scope should be rendered after selection: {scoped_text:?}"
    );
    let requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AdvancedPage::onPropToggle",
            &[
                serde_json::json!("@mesh/navigation-bar"),
                serde_json::json!("blur_enabled"),
                serde_json::json!(true),
                serde_json::json!("@mesh/navigation-bar#bottom"),
            ],
        )
        .unwrap();
    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::ServiceCall { interface, command, payload, .. }
            | CoreRequest::ServiceCall { interface, command, payload, .. }]
            if interface == "mesh.settings"
                && command == "set_prop"
                && payload.get("module_id")
                    == Some(&serde_json::json!("@mesh/navigation-bar"))
                && payload.get("instance_id")
                    == Some(&serde_json::json!("@mesh/navigation-bar#bottom"))
                && payload.get("prop") == Some(&serde_json::json!("blur_enabled"))
                && payload.get("value") == Some(&serde_json::json!(false))
    ));

    let reset_requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AdvancedPage::onPropReset",
            &[
                serde_json::json!("@mesh/navigation-bar"),
                serde_json::json!("blur_enabled"),
                serde_json::json!(true),
                serde_json::json!("@mesh/navigation-bar#bottom"),
            ],
        )
        .unwrap();
    assert!(
        matches!(reset_requests.as_slice(),
            [CoreRequest::ServiceCall { interface, command, payload, .. }]
                if interface == "mesh.settings" && command == "unset_prop"
                    && payload["module_id"] == "@mesh/navigation-bar"
                    && payload["instance_id"] == "@mesh/navigation-bar#bottom"
                    && payload["prop"] == "blur_enabled"
        ),
        "{reset_requests:?}"
    );

    component
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Color theme"));
    assert!(text.iter().any(|line| line == "Language"));
    assert!(text.iter().any(|line| line == "MESH Default Dark"));
    assert!(text.iter().any(|line| line == "MESH Default Light"));
    assert!(text.iter().any(|line| line == "Gruvbox Dark"));
    assert!(text.iter().any(|line| line == "Papirus Dark"));
    assert!(text.iter().any(|line| line == "Noto Sans"));
    first_node_with_attr(
        component
            .last_tree
            .as_ref()
            .expect("rendered settings tree"),
        "content",
        "Slovenčina",
    )
    .expect("Slovak language label");

    fn collect_theme_options(
        node: &mesh_core_elements::WidgetNode,
        options: &mut Vec<(String, String)>,
    ) {
        if let Some(class) = node.attributes.get(Prop::Class)
            && class
                .split_whitespace()
                .any(|token| token == "theme-option")
        {
            assert!(node.event_handlers.contains_key("click"));
            let id = first_node_by_class(node, "settings-card-meta")
                .and_then(|meta| meta.attributes.get(Prop::Content))
                .expect("theme card id");
            options.push((id.clone(), class.clone()));
        }
        for child in &node.children {
            collect_theme_options(child, options);
        }
    }

    let mut theme_options = Vec::new();
    collect_theme_options(
        component
            .last_tree
            .as_ref()
            .expect("rendered settings tree"),
        &mut theme_options,
    );
    assert_eq!(theme_options.len(), 3);
    assert!(
        theme_options.iter().any(|(id, class)| {
            id == "gruvbox-dark"
                && class
                    .split_whitespace()
                    .any(|token| token == "theme-option-active")
        }),
        "the exact current theme should be highlighted: {theme_options:?}"
    );
    assert!(
        theme_options.iter().all(|(id, class)| {
            id == "gruvbox-dark"
                || !class
                    .split_whitespace()
                    .any(|token| token == "theme-option-active")
        }),
        "only the exact current theme should be highlighted: {theme_options:?}"
    );

    let mut slovak_locale = mesh_core_locale::LocaleEngine::from_snapshot(component.locale.clone());
    slovak_locale.set_locale("sk");
    component.locale_changed(&slovak_locale).unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    let text = rendered_text(&component);
    assert!(
        text.iter().any(|line| line == "Farebná téma"),
        "changing locale must translate and preserve the active Appearance settings page"
    );
    assert!(text.iter().any(|line| line == "Nastavenia"));
    assert!(text.iter().any(|line| line == "Slovenčina"));

    component
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAudio", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    let text = rendered_text(&component);
    assert!(
        text.iter().any(|line| line == "Aktívny poskytovateľ zvuku"),
        "live backend fallback copy should also use the Slovak catalog"
    );

    let requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AppearancePage::onThemeSelect",
            &[serde_json::json!("mesh-default-light")],
        )
        .unwrap();
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
            assert_eq!(command, "set_theme");
            assert_eq!(payload["theme_id"], serde_json::json!("mesh-default-light"));
        }
        other => panic!("expected theme change request, got {other:?}"),
    }

    let requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AppearancePage::onIconThemeSelect",
            &[serde_json::json!("Papirus-Dark")],
        )
        .unwrap();
    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::ServiceCall { interface, command, payload, .. }
            | CoreRequest::ServiceCall { interface, command, payload, .. }]
            if interface == "mesh.theme"
                && command == "set_icon_theme"
                && payload.get("theme_id") == Some(&serde_json::json!("Papirus-Dark"))
    ));

    let requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AppearancePage::onFontSelect",
            &[serde_json::json!("Noto Sans")],
        )
        .unwrap();
    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::ServiceCall { interface, command, payload, .. }
            | CoreRequest::ServiceCall { interface, command, payload, .. }]
            if interface == "mesh.theme"
                && command == "set_font_family"
                && payload.get("family") == Some(&serde_json::json!("Noto Sans"))
    ));

    let requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AdvancedPage::onProviderNext",
            &[
                serde_json::json!("mesh.audio"),
                serde_json::json!("@mesh/pipewire-audio"),
            ],
        )
        .unwrap();
    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::ServiceCall { interface, command, payload, .. }
            | CoreRequest::ServiceCall { interface, command, payload, .. }]
            if interface == "mesh.packages"
                && command == "set_provider"
                && payload.get("interface") == Some(&serde_json::json!("mesh.audio"))
                && payload.get("provider_id")
                    == Some(&serde_json::json!("@mesh/pulseaudio-audio"))
    ));

    let requests = component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/settings/local:AdvancedPage::onModuleToggle",
            &[
                serde_json::json!("@mesh/pulseaudio-audio"),
                serde_json::json!(true),
            ],
        )
        .unwrap();
    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::ServiceCall { interface, command, payload, .. }
            | CoreRequest::ServiceCall { interface, command, payload, .. }]
            if interface == "mesh.packages"
                && command == "set_module_enabled"
                && payload.get("module_id")
                    == Some(&serde_json::json!("@mesh/pulseaudio-audio"))
                && payload.get("enabled") == Some(&serde_json::json!(false))
    ));

    let requests = component
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::onClose", &[])
        .unwrap();
    match requests.as_slice() {
        [CoreRequest::HideSurface { surface_id }] => assert_eq!(surface_id, "@mesh/settings"),
        other => panic!("expected settings hide request, got {other:?}"),
    }
}

#[test]
fn debug_inspector_benchmark_view_renders_canonical_rows_when_profiling_off() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": false,
                "profiling_session_id": 11,
                "modules": [{ "id": "@mesh/debug-inspector" }],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": ["@mesh/debug-inspector"],
                "benchmarks": {
                    "scenarios": []
                },
                "profiling": serde_json::Value::Null
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(320, 640);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(320, 640), &mut buffer, 1.0)
        .unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showBenchmark", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(320, 640), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Benchmark / Interaction"));
    for title in [
        "Idle scheduler",
        "Hover",
        "Audio popover open/close",
        "Pointer move",
        "Text update",
        "Scroll",
        "Icon grid",
        "Animation tick",
        "Theme reload",
        "Resize",
        "Keyboard traversal",
        "Backend-driven update",
    ] {
        assert!(
            text.iter().any(|line| line == title),
            "benchmark row should render {title}"
        );
    }
    assert!(text.iter().any(|line| line == "Profiling off"));
    assert!(text.iter().any(|line| line == "Start profiling first"));
    assert!(
        text.iter()
            .any(|line| line.contains("@mesh/navigation-bar"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("mesh.audio -> @mesh/pipewire-audio"))
    );
}

#[test]
fn debug_inspector_benchmark_view_renders_waiting_rows_when_profiling_live_without_results() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    // Paint once so the inspector's script reads (and thus tracks) its
    // `mesh.debug` state fields before the first service event; otherwise the
    // runtime does not yet observe the event. The real shell seeds cached
    // service payloads at mount, which this direct-dispatch test bypasses.
    {
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(360, 720);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
            .unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 12,
                "modules": [],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": [],
                "benchmarks": {
                    "scenarios": []
                },
                "profiling": {
                    "session_id": 12,
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

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(320, 640);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(320, 640), &mut buffer, 1.0)
        .unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showBenchmark", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(320, 640), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Benchmark / Interaction"));
    assert!(text.iter().any(|line| line == "Waiting for samples"));
    assert!(text.iter().any(|line| line == "Run scenario"));
    assert!(
        text.iter()
            .any(|line| line.contains("Run a scenario while profiling is live"))
    );
    for title in [
        "Idle scheduler",
        "Hover",
        "Audio popover open/close",
        "Pointer move",
        "Text update",
        "Scroll",
        "Icon grid",
        "Animation tick",
        "Theme reload",
        "Resize",
        "Keyboard traversal",
        "Backend-driven update",
    ] {
        assert!(text.iter().any(|line| line == title));
    }
}

#[test]
fn debug_inspector_renders_allocator_mode_and_per_surface_counts() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(360, 720);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
        .unwrap();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "layout_bounds_enabled": false,
                "allocation_profiling_available": true,
                "profiling_enabled": true,
                "profiling_session_id": 14,
                "modules": [],
                "module_graph": [],
                "interfaces": [],
                "backend_runtimes": [],
                "active_surfaces": ["@mesh/navigation-bar"],
                "benchmarks": { "scenarios": [] },
                "profiling": {
                    "session_id": 14,
                    "allocation_profiling_available": true,
                    "shell": {
                        "stages": [],
                        "redraw_count": 1,
                        "total_surface_render_time_micros": 20,
                        "allocations": {
                            "sample_count": 1,
                            "allocation_count": 12,
                            "allocated_bytes": 4096
                        }
                    },
                    "surfaces": [{
                        "surface_id": "@mesh/navigation-bar",
                        "module_id": "@mesh/navigation-bar",
                        "stages": [],
                        "redraw_count": 1,
                        "total_surface_render_time_micros": 20,
                        "allocations": {
                            "sample_count": 1,
                            "allocation_count": 12,
                            "allocated_bytes": 4096
                        }
                    }],
                    "backends": []
                }
            }),
        })
        .unwrap();

    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
        .unwrap();
    let overview_text = rendered_text(&component);
    assert!(overview_text.iter().any(|line| line == "4.0 KiB / 12 ops"));

    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showSurfaces", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
        .unwrap();
    let surface_text = rendered_text(&component);
    assert!(
        surface_text
            .iter()
            .any(|line| line == "Allocations 4.0 KiB in 12 ops across 1 passes")
    );
}

#[test]
fn debug_inspector_benchmark_view_renders_populated_benchmark_result_rows() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    // Paint once so the inspector's script reads (and thus tracks) its
    // `mesh.debug` state fields before the first service event; otherwise the
    // runtime does not yet observe the event. The real shell seeds cached
    // service payloads at mount, which this direct-dispatch test bypasses.
    {
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(360, 720);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(360, 720), &mut buffer, 1.0)
            .unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 13,
                "modules": [],
                "interfaces": [],
                "backend_runtimes": [{
                    "interface": "mesh.audio",
                    "provider_id": "@mesh/pipewire-audio",
                    "status": "running",
                    "message": "Polling steadily",
                    "failure_count": 0
                }],
                "active_surfaces": ["@mesh/navigation-bar"],
                "benchmarks": {
                    "scenarios": [
                        {
                            "id": "hover",
                            "label": "Hover",
                            "target": "@mesh/navigation-bar",
                            "status": "Complete",
                            "primary_metric": "input_handling: 2 samples, max 18us",
                            "secondary_metric": "style_restyle: 2 samples, max 12us",
                            "hint": "Interact with @mesh/navigation-bar while profiling is live"
                        },
                        {
                            "id": "surface_open_close",
                            "label": "Surface open/close",
                            "target": "@mesh/navigation-bar audio controls",
                            "status": "Complete",
                            "primary_metric": "total_surface_render: 140us",
                            "secondary_metric": "redraw_count: 2",
                            "hint": "Open and close the navigation-bar audio popover while profiling is live"
                        },
                        {
                            "id": "pointer_update",
                            "label": "Pointer-driven update",
                            "target": "@mesh/navigation-bar audio controls",
                            "status": "Complete",
                            "primary_metric": "runtime_update_handling: 1 samples, max 22us",
                            "secondary_metric": "paint: 1 samples, max 30us",
                            "hint": "Adjust the navigation-bar audio controls while profiling is live"
                        },
                        {
                            "id": "keyboard_traversal",
                            "label": "Keyboard traversal",
                            "target": "@mesh/navigation-bar focus chain",
                            "status": "Complete",
                            "primary_metric": "input_handling: 1 samples, max 8us",
                            "secondary_metric": "total_surface_render: 1 samples, max 60us",
                            "hint": "Move focus through @mesh/navigation-bar while profiling is live"
                        },
                        {
                            "id": "backend_update",
                            "label": "Backend-driven update",
                            "target": "mesh.audio -> @mesh/pipewire-audio",
                            "status": "Complete",
                            "primary_metric": "state_publish_delivery: 3 samples, max 45us",
                            "secondary_metric": "frontend total_surface_render: 160us",
                            "hint": "Update mesh.audio while profiling is live"
                        }
                    ]
                },
                "profiling": {
                    "session_id": 13,
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

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(320, 720);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(320, 720), &mut buffer, 1.0)
        .unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showBenchmark", &[])
        .unwrap();
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(320, 720), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Benchmark / Interaction"));
    assert!(text.iter().any(|line| line == "Complete"));
    assert!(text.iter().any(|line| line == "Run scenario"));
    assert!(
        text.iter()
            .any(|line| line.contains("input_handling: 2 samples"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("total_surface_render: 140us"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("state_publish_delivery"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("@mesh/navigation-bar"))
    );
    assert!(
        text.iter()
            .any(|line| line.contains("mesh.audio -> @mesh/pipewire-audio"))
    );
}
