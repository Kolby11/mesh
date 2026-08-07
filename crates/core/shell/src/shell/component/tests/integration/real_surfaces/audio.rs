use super::*;

#[test]
fn audio_popover_theme_repaint_keeps_audio_state_without_available_flag() {
    let theme = default_theme();
    let mut audio = real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    audio
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();

    let mut buffer = PixelBuffer::new(320, 220);
    audio
        .paint(&theme, SurfaceExtent::unpadded(320, 220), &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        runtime_value(&audio, "audio_percent_label"),
        Some(serde_json::json!("50%"))
    );

    audio.theme_changed().unwrap();
    audio
        .paint(&theme, SurfaceExtent::unpadded(320, 220), &mut buffer, 1.0)
        .unwrap();

    let text = rendered_text(&audio);
    assert!(
        text.iter().any(|line| line == "50%"),
        "theme repaint should preserve audio percent, got {text:?}"
    );
    assert!(
        !text.iter().any(|line| line == "Audio unavailable"),
        "theme repaint should not fall back to unavailable copy, got {text:?}"
    );
}

#[test]
fn audio_popover_shipped_i18n_covers_template_translation_keys() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/audio-popover/src/main.mesh"
    ));
    let mut keys = Vec::new();
    for (index, _) in source.match_indices("t(") {
        if index > 0 {
            let previous = source[..index].chars().next_back().unwrap_or(' ');
            if previous.is_ascii_alphanumeric() || previous == '_' {
                continue;
            }
        }
        let fragment = &source[index + 2..];
        let Some(end) = fragment.find(')') else {
            continue;
        };
        let raw = fragment[..end].trim();
        let quoted = raw
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| {
                raw.strip_prefix('\'')
                    .and_then(|value| value.strip_suffix('\''))
            });
        if let Some(key) = quoted {
            keys.push(key.to_string());
        }
    }
    for quote in ['"', '\''] {
        for fragment in source.split(quote).skip(1).step_by(2) {
            if fragment.starts_with("audio.") {
                keys.push(fragment.to_string());
            }
        }
    }
    keys.sort();
    keys.dedup();

    let en: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/audio-popover/config/i18n/en.json"
    )))
    .unwrap();
    let sk: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/audio-popover/config/i18n/sk.json"
    )))
    .unwrap();

    for key in keys {
        assert!(
            en.get(&key).is_some(),
            "missing English audio translation for {key}"
        );
        assert!(
            sk.get(&key).is_some(),
            "missing Slovak audio translation for {key}"
        );
    }
}

#[test]
fn phase44_navigation_audio_surface_emits_focused_proof_snapshot() {
    let theme = default_theme();

    let mut navigation =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    navigation.visible = true;
    let mut navigation_buffer = PixelBuffer::new(960, 80);
    navigation
        .paint(
            &theme,
            SurfaceExtent::unpadded(960, 80),
            &mut navigation_buffer,
            1.0,
        )
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
    audio
        .paint(
            &theme,
            SurfaceExtent::unpadded(320, 220),
            &mut audio_buffer,
            1.0,
        )
        .unwrap();
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
                    compiled: root_compiled.clone().into(),
                },
            ),
            (
                "@mesh/audio-popover".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: popover_compiled.into(),
                },
            ),
        ]),
        extension_point_contributions: HashMap::new(),
        extension_point_entries: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        root_compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
        test_settings_store(),
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
    component
        .paint(&theme, SurfaceExtent::unpadded(220, 80), &mut buffer, 1.0)
        .unwrap();
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
        .call_handler_target(&handler, std::slice::from_ref(&click_event))
        .unwrap();
    component
        .paint(&theme, SurfaceExtent::unpadded(220, 80), &mut buffer, 1.0)
        .unwrap();
    let show_requests = component.tick().unwrap();
    assert!(matches!(
        show_requests.as_slice(),
        [CoreRequest::ShowSurface { surface_id }] if surface_id == "@mesh/audio-popover"
    ));

    let requests = component
        .call_handler_target(&handler, &[click_event])
        .unwrap();
    assert!(
        requests.is_empty(),
        "closing toggle should not publish direct shell events"
    );
    assert!(runtime_bool(&component, "audio_surface_hidden"));

    component
        .paint(&theme, SurfaceExtent::unpadded(220, 80), &mut buffer, 1.0)
        .unwrap();
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
fn shipped_navigation_volume_button_click_toggles_mute() {
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
    let mut buffer = PixelBuffer::new(320, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(320, 80), &mut buffer, 1.0)
        .unwrap();
    let health = format!(
        "{:?}",
        component
            .diagnostics
            .as_ref()
            .expect("navigation diagnostics")
            .health()
    );
    for unexpected in [
        "missing image asset",
        "unsupported background-image",
        "excessive blur",
    ] {
        assert!(
            !health.contains(unexpected),
            "navigation diagnostics should not contain {unexpected}: {health}"
        );
    }
    let handler = "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle";
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");
    let button = first_node_with_click_handler(tree, handler).expect("volume button");
    let click_handler = button.event_handlers.get("click").unwrap().clone();

    let requests = component
        .call_handler_target(
            &click_handler,
            &[serde_json::json!({
                "surface": {
                    "id": "@mesh/navigation-bar"
                },
                "current": {
                    "key": button.mesh_key().unwrap_or_default()
                },
                "current_target": {
                    "key": button.mesh_key().unwrap_or_default(),
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
            CoreRequest::ServiceCommand { interface, command, payload, .. }
                if interface == "mesh.audio"
                    && command == "set_muted"
                    && payload == &serde_json::json!({
                        "device_id": "default",
                        "muted": true
                    })
        )),
        "click should toggle mute through the audio service path: {requests:?}"
    );
}

#[test]
fn shipped_navigation_audio_popover_transition_delay_stays_bounded() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(320, 220);
    // The hide transition is now a CSS `transition` on the surface root, read
    // from the last painted root style, so paint once before querying it.
    component
        .paint(&theme, SurfaceExtent::unpadded(320, 220), &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        component.hide_transition_ms(),
        120,
        "audio popover should keep the shipped bounded hide transition"
    );

    component.set_surface_exiting(true);
    component
        .paint(&theme, SurfaceExtent::unpadded(320, 220), &mut buffer, 1.0)
        .unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered audio popover");
    assert!(
        tree.attributes
            .get("class")
            .is_some_and(|class| class.contains("mesh-surface-exiting")),
        "closing transition should expose mesh-surface-exiting state to styles"
    );
}

#[test]
fn shipped_navigation_volume_scroll_reaches_audio_service_on_first_input() {
    let mut navigation =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    navigation
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
    navigation.visible = true;

    let theme = default_theme();
    let mut nav_buffer = PixelBuffer::new(960, 80);
    navigation
        .paint(
            &theme,
            SurfaceExtent::unpadded(960, 80),
            &mut nav_buffer,
            1.0,
        )
        .unwrap();
    let tree = navigation
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");
    let button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("volume button");
    let button_x = button.layout.x + button.layout.width / 2.0;
    let button_y = button.layout.y + button.layout.height / 2.0;
    let requests = navigation
        .handle_input(
            &theme,
            960,
            80,
            ComponentInput::Scroll {
                x: button_x,
                y: button_y,
                dx: 0.0,
                dy: 1.0,
            },
        )
        .unwrap();
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { interface, command, payload, .. }
                if interface == "mesh.audio"
                    && command == "set_volume"
                    && payload["device_id"] == serde_json::json!("default")
                    && payload["percent"] == serde_json::json!(55)
        )),
        "first scroll input should reach the volume service command path: {requests:?}"
    );
}

#[test]
fn shipped_navigation_volume_icon_inherits_button_click_and_tooltip() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
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
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");
    let button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("volume button");
    let button_key = button.mesh_key().expect("button mesh key").to_owned();
    let icon = first_node_by_tag(button, "icon").expect("volume icon");
    let icon_key = icon.mesh_key().expect("icon mesh key").to_owned();
    assert_eq!(
        find_tooltip_text_by_key(tree, &icon_key).as_deref(),
        Some("Volume 50%"),
        "tooltip lookup should inherit the button title when hovering the icon"
    );

    let slovak_locale = mesh_core_locale::LocaleEngine::new("sk");
    component.locale_changed(&slovak_locale).unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered localized navigation bar");
    assert_eq!(
        find_tooltip_text_by_key(tree, &icon_key).as_deref(),
        Some("Hlasitost 50%"),
        "volume tooltip should update when the shell locale changes"
    );

    // The icon carries no click handler of its own; a click on it bubbles up to
    // the enclosing VolumeButton. Pointer routing resolves a leaf coordinate to
    // its nearest click-handling ancestor by walking the node path, so verify
    // the structural relationship (icon nested under the button that owns the
    // handler) independent of the bar's painted geometry, then dispatch the
    // handler and confirm it toggles mute.
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered localized navigation bar");
    assert!(
        icon_key.starts_with(&format!("{button_key}/")),
        "volume icon {icon_key} should be nested inside the button {button_key} it inherits clicks from"
    );
    let inherited_handler =
        find_click_handler(tree, &button_key).expect("button should own a click handler");
    assert_eq!(inherited_handler.handler(), "onAudioToggle");
    assert_eq!(
        inherited_handler.instance_key(),
        Some("@mesh/navigation-bar/local:VolumeButton"),
        "icon click should bubble to the VolumeButton toggle handler"
    );

    let button_node = node_by_mesh_key(tree, &button_key);
    let requests = component
        .call_handler_target(
            &inherited_handler,
            &[serde_json::json!({
                "surface": { "id": "@mesh/navigation-bar" },
                "current_target": {
                    "key": button_key,
                    "position": {
                        "margin_left": button_node.layout.x as i64,
                        "margin_bottom": 40
                    }
                }
            })],
        )
        .unwrap();

    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { interface, command, payload, .. }
                if interface == "mesh.audio"
                    && command == "set_muted"
                    && payload == &serde_json::json!({
                        "device_id": "default",
                        "muted": true
                    })
        )),
        "clicking directly on the icon should bubble to the button click handler: {requests:?}"
    );
}
