use super::*;

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
    component
        .paint(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&component);
    assert!(text.iter().any(|line| line == "Inspect element"));
    assert!(!text.iter().any(|line| line == "Debug Inspector"));
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

    component
        .paint(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();
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
    component
        .paint(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();
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
    component
        .paint(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();
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
    component
        .paint(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();
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
        "Pointer move",
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
fn debug_inspector_hidden_views_do_not_participate_in_layout() {
    let mut component = real_frontend_module_component("@mesh/debug-inspector", debug_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.debug".into(),
            source_module: "@mesh/core-debug".into(),
            payload: serde_json::json!({
                "overlay_enabled": true,
                "profiling_enabled": false,
                "profiling_session_id": 11,
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
    let mut buffer = PixelBuffer::new(480, 640);
    component
        .paint(&theme, SurfaceExtent::unpadded(480, 640), &mut buffer, 1.0)
        .unwrap();

    let tree = component.last_tree.as_ref().expect("rendered inspector");
    let overview = first_node_with_class_token(tree, "overview-view").expect("overview view");
    let modules = first_node_with_class_token(tree, "modules-view").expect("modules view");
    let surfaces = first_node_with_class_token(tree, "surfaces-view").expect("surfaces view");
    let backend = first_node_with_class_token(tree, "backend-view").expect("backend view");
    let benchmark = first_node_with_class_token(tree, "benchmark-view").expect("benchmark view");

    assert!(overview.layout.height > 0.0);
    for hidden_view in [modules, surfaces, backend, benchmark] {
        assert_eq!(
            hidden_view.computed_style.display,
            mesh_core_elements::style::Display::None
        );
        assert_eq!(hidden_view.layout.height, 0.0);
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
    component
        .paint(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showSurfaces", &[])
        .unwrap();
    component
        .paint(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();

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
    component
        .paint(&theme, SurfaceExtent::unpadded(360, 640), &mut buffer, 1.0)
        .unwrap();

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
