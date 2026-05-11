use super::*;
use std::fs;

#[test]
fn failing_handler_is_reported_once_and_does_not_clear_render_state() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
function onExplode()
    error("boom")
end
</script>
"#,
    );
    component.last_tree = Some(root_with(vec![event_node(
        "button",
        "root/0",
        0.0,
        0.0,
        80.0,
        24.0,
        &[("click", "onExplode")],
    )]));
    component.dirty = false;

    let first = component.call_namespaced_handler("onExplode", &[]);
    let second = component.call_namespaced_handler("onExplode", &[]);

    assert!(first.unwrap().is_empty());
    assert!(second.unwrap().is_empty());
    assert!(
        component.last_tree.is_some(),
        "last successfully rendered tree should remain available"
    );
    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 1);
    assert!(!component.wants_render());
}

#[test]
fn service_update_runs_on_render_before_rebuilding_tree() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box title="{audio_tooltip}" />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end

audio_tooltip = "Volume unavailable"

function onRender()
    if not audio_ok or not audio then
        audio_tooltip = "Audio service unavailable"
        return
    end
    audio_tooltip = string.format("Volume %d%%", math.floor(tonumber(audio.percent) or 0))
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 64, "muted": false }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();

    assert_eq!(
        runtime_value(&component, "audio_tooltip"),
        Some(serde_json::json!("Volume 64%"))
    );
    let tree = component.last_tree.as_ref().unwrap();
    fn first_title(node: &WidgetNode) -> Option<&str> {
        node.attributes
            .get("title")
            .map(String::as_str)
            .or_else(|| node.children.iter().find_map(first_title))
    }
    assert_eq!(first_title(tree), Some("Volume 64%"));
}

#[test]
fn raw_service_state_update_schedules_repaint_without_proxy_tracking() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box title="{last_service_update.name}" />
</template>
<script lang="luau">
</script>
"#,
        InterfaceCatalog::default(),
        &["service.audio.read"],
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    component.dirty = false;

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 64, "muted": false }),
        })
        .unwrap();

    assert!(
        component.wants_render(),
        "raw ScriptState changes should schedule repaint even without proxy tracking"
    );
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    let tree = component.last_tree.as_ref().unwrap();
    fn first_title(node: &WidgetNode) -> Option<&str> {
        node.attributes
            .get("title")
            .map(String::as_str)
            .or_else(|| node.children.iter().find_map(first_title))
    }
    assert_eq!(first_title(tree), Some("audio"));
}

#[test]
fn frontend_proxy_state_update_reaches_render_state() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box title="{volumeLevel}" />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end

volumeLevel = 0

function onRender()
    if not audio_ok or not audio then
        volumeLevel = 0
        return
    end
    volumeLevel = audio.state.percent or 0
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 73, "muted": false }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();

    assert_eq!(
        runtime_value(&component, "volumeLevel"),
        Some(serde_json::json!(73))
    );
    let tree = component.last_tree.as_ref().unwrap();
    fn first_title(node: &WidgetNode) -> Option<&str> {
        node.attributes
            .get("title")
            .map(String::as_str)
            .or_else(|| node.children.iter().find_map(first_title))
    }
    assert_eq!(first_title(tree), Some("73"));
}

#[test]
fn frontend_proxy_state_read_tracks_repaint_invalidation() {
    let mut component = test_frontend_component_with_catalog(
        r#"
<template>
  <box title="{volumeLevel}" />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end

volumeLevel = 0

function onRender()
    if audio_ok and audio then
        volumeLevel = audio.state.percent or 0
    end
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 20, "muted": false }),
        })
        .unwrap();
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    component.clear_runtime_dirty_states();
    component.dirty = false;
    component.render_hooks_pending = false;

    {
        let runtimes = component.runtimes.lock().unwrap();
        let runtime = runtimes.get(component.id()).unwrap();
        assert!(
            runtime
                .script_ctx
                .tracked_fields_for_service("audio")
                .contains("percent"),
            "audio.state.percent should track percent for service invalidation"
        );
    }

    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({ "percent": 35, "muted": false }),
        })
        .unwrap();

    assert!(
        component.wants_render(),
        "changing audio.state.percent should schedule a repaint"
    );
    component.paint(&theme, 240, 40, &mut buffer).unwrap();
    assert_eq!(
        runtime_value(&component, "volumeLevel"),
        Some(serde_json::json!(35))
    );
}

#[test]
fn pcall_service_lookup_diagnostic_reaches_component_diagnostics() {
    let mut component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau">
function onRender()
    pcall(require, "@mesh/missing@>=1.0")
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 40);
    component.paint(&theme, 240, 40, &mut buffer).unwrap();

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 1);
}

#[test]
fn missing_required_icon_degrades_module_without_unloading() {
    let component_source = r#"
<template>
  <box />
</template>
<script lang="luau">
</script>
"#;
    let component =
        test_frontend_component_with_required_icons(component_source, &["missing-proof"]);
    assert_eq!(component.id(), "@test/reactive-surface");
    assert!(component.visible);

    assert!(
        !component
            .record_missing_icon_diagnostic("missing-proof", vec!["material:no-such-icon".into()])
    );
    assert!(
        !component
            .record_missing_icon_diagnostic("missing-proof", vec!["material:not-present".into()])
    );

    let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
    assert_eq!(diagnostics.error_count(), 0);
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Degraded(message)
            if message.contains("missing-proof")
    ));
    assert!(component.visible);
}

#[test]
fn icon_reliability_core_surfaces_proof() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let icon_config_path = root.join("config/icons.toml");
    let icon_config = fs::read_to_string(&icon_config_path).unwrap();
    let config = mesh_core_icon::IconConfig::from_toml_str(&icon_config).unwrap();

    let inventory = [
        "audio-volume-muted",
        "audio-volume-low",
        "audio-volume-medium",
        "audio-volume-high",
        "network-wireless",
        "bluetooth",
        "settings",
        "weather-clear-night",
        "weather-clear",
        "battery-empty",
        "battery-caution",
        "battery-low",
        "battery-good",
        "battery-full",
    ];
    for semantic_name in inventory {
        assert!(
            config.active_profile().icons.contains_key(semantic_name),
            "{semantic_name} must be mapped in config/icons.toml"
        );
    }
    assert!(config.active_profile().icons.contains_key("missing-proof"));

    let surface_manifests = [root.join("modules/frontend/navigation-bar")];
    for module_dir in surface_manifests {
        let loaded = mesh_core_module::manifest::load_manifest(&module_dir).unwrap();
        assert!(
            loaded
                .manifest
                .dependencies
                .icon_packs
                .required
                .contains(&"system".to_string()),
            "{} must declare the system icon pack",
            loaded.manifest.package.id
        );
        for semantic_name in &loaded.manifest.icon_requirements.required {
            assert!(
                config.active_profile().icons.contains_key(semantic_name),
                "{} declares unmapped icon {}",
                loaded.manifest.package.id,
                semantic_name
            );
        }
    }

    for path in [
        "modules/frontend/navigation-bar/src/main.mesh",
        "modules/frontend/navigation-bar/src/components/volume-button.mesh",
        "modules/frontend/navigation-bar/src/components/settings-button.mesh",
        "modules/frontend/navigation-bar/src/components/theme-button.mesh",
        "modules/frontend/navigation-bar/src/components/battery-widget.mesh",
        "modules/frontend/navigation-bar/src/components/battery-button.mesh",
    ] {
        let source = fs::read_to_string(root.join(path)).unwrap();
        for line in source.lines().filter(|line| line.contains("<icon")) {
            assert!(
                !line.contains("material:"),
                "{path} contains pack-specific icon: {line}"
            );
            assert!(
                !line.contains("lucide:"),
                "{path} contains pack-specific icon: {line}"
            );
            assert!(
                !line.contains(".svg"),
                "{path} contains SVG path icon: {line}"
            );
            assert!(
                !line.contains(".png"),
                "{path} contains PNG path icon: {line}"
            );
        }
    }

    let mut svg_node = WidgetNode::new("icon");
    svg_node.attributes.insert("name".into(), "settings".into());
    svg_node.attributes.insert("size".into(), "18".into());
    svg_node.layout = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 18.0,
        height: 18.0,
    };
    let mut svg_buffer = mesh_core_render::PixelBuffer::new(24, 24);
    mesh_core_render::paint_frontend_tree(&svg_node, &mut svg_buffer, 1.0, None);
    assert!(svg_buffer.data.chunks_exact(4).any(|px| px[3] > 0));

    let td = tempfile::tempdir().unwrap();
    let raster_path = td.path().join("raster.bmp");
    fs::write(&raster_path, two_by_two_bmp()).unwrap();
    let mut raster_node = WidgetNode::new("icon");
    raster_node
        .attributes
        .insert("src".into(), raster_path.to_string_lossy().to_string());
    raster_node.layout = LayoutRect {
        x: 1.0,
        y: 1.0,
        width: 12.0,
        height: 10.0,
    };
    let mut raster_buffer = mesh_core_render::PixelBuffer::new(20, 20);
    mesh_core_render::paint_frontend_tree(&raster_node, &mut raster_buffer, 1.0, None);
    assert!(raster_buffer.data.chunks_exact(4).any(|px| px[3] > 0));

    let mut missing_node = WidgetNode::new("icon");
    missing_node
        .attributes
        .insert("name".into(), "missing-proof".into());
    missing_node.attributes.insert("size".into(), "18".into());
    missing_node.layout = LayoutRect {
        x: 2.0,
        y: 2.0,
        width: 18.0,
        height: 18.0,
    };
    let mut missing_buffer = mesh_core_render::PixelBuffer::new(24, 24);
    mesh_core_render::paint_frontend_tree(&missing_node, &mut missing_buffer, 1.0, None);
    assert!(missing_buffer.data.chunks_exact(4).any(|px| px[3] > 0));

    let component = test_frontend_component(
        r#"
<template><box /></template>
<script lang="luau"></script>
"#,
    );
    assert!(
        component
            .record_missing_icon_diagnostic("missing-proof", vec!["material:no-such-icon".into()])
    );
    assert!(
        !component
            .record_missing_icon_diagnostic("missing-proof", vec!["material:no-such-icon".into()])
    );
    let diagnostics = component.diagnostics.as_ref().unwrap();
    assert_eq!(diagnostics.error_count(), 0);
    assert!(matches!(
        diagnostics.health(),
        mesh_core_diagnostics::HealthStatus::Degraded(message)
            if message.contains("missing-proof")
    ));
}

fn two_by_two_bmp() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&70u32.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&2i32.to_le_bytes());
    bytes.extend_from_slice(&(-2i32).to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 16]);
    bytes.extend_from_slice(&[0, 0, 255, 255]);
    bytes.extend_from_slice(&[0, 255, 0, 255]);
    bytes.extend_from_slice(&[255, 0, 0, 255]);
    bytes.extend_from_slice(&[0, 255, 255, 255]);
    bytes
}
