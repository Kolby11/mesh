use super::*;

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
        component.paint(&theme, 360, 720, &mut buffer, 1.0).unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 7,
                "active_view": "overview",
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
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();
    component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/debug-inspector::showBackendServices",
            &[],
        )
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();

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
        component.paint(&theme, 360, 720, &mut buffer, 1.0).unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 29,
                "active_view": "surfaces",
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
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showSurfaces", &[])
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Surfaces"));
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
        component.paint(&theme, 360, 720, &mut buffer, 1.0).unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": false,
                "profiling_session_id": 31,
                "active_view": "modules",
                "modules": [{ "id": "@mesh/navigation-bar" }],
                "module_graph": [
                    {
                        "module_id": "@mesh/navigation-bar",
                        "kind": "frontend",
                        "enabled": true,
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
    component.paint(&theme, 360, 720, &mut buffer, 1.0).unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showModules", &[])
        .unwrap();
    component.paint(&theme, 360, 720, &mut buffer, 1.0).unwrap();

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

#[test]
fn debug_inspector_benchmark_view_renders_five_rows_when_profiling_off() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": false,
                "profiling_session_id": 11,
                "active_view": "benchmark",
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
    component.paint(&theme, 320, 640, &mut buffer, 1.0).unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showBenchmark", &[])
        .unwrap();
    component.paint(&theme, 320, 640, &mut buffer, 1.0).unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Benchmark / Interaction"));
    for title in [
        "Hover",
        "Surface open/close",
        "Pointer-driven update",
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
        component.paint(&theme, 360, 720, &mut buffer, 1.0).unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 12,
                "active_view": "benchmark",
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
    component.paint(&theme, 320, 640, &mut buffer, 1.0).unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showBenchmark", &[])
        .unwrap();
    component.paint(&theme, 320, 640, &mut buffer, 1.0).unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Benchmark / Interaction"));
    assert!(text.iter().any(|line| line == "Waiting for samples"));
    assert!(text.iter().any(|line| line == "Run scenario"));
    assert!(
        text.iter()
            .any(|line| line.contains("Run a scenario while profiling is live"))
    );
    for title in [
        "Hover",
        "Surface open/close",
        "Pointer-driven update",
        "Keyboard traversal",
        "Backend-driven update",
    ] {
        assert!(text.iter().any(|line| line == title));
    }
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
        component.paint(&theme, 360, 720, &mut buffer, 1.0).unwrap();
    }
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": true,
                "profiling_session_id": 13,
                "active_view": "benchmark",
                "modules": [],
                "interfaces": [],
                "backend_runtimes": [{
                    "interface": "mesh.audio",
                    "provider_id": "@mesh/pipewire-audio",
                    "status": "running",
                    "message": "Polling steadily",
                    "failure_count": 0
                }],
                "active_surfaces": ["@mesh/navigation-bar", "@mesh/audio-popover"],
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
                            "target": "@mesh/audio-popover",
                            "status": "Complete",
                            "primary_metric": "total_surface_render: 140us",
                            "secondary_metric": "redraw_count: 2",
                            "hint": "Open and close @mesh/audio-popover while profiling is live"
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
    component.paint(&theme, 320, 720, &mut buffer, 1.0).unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showBenchmark", &[])
        .unwrap();
    component.paint(&theme, 320, 720, &mut buffer, 1.0).unwrap();

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
