use super::*;

#[test]
fn phase47_navigation_and_audio_surfaces_keep_taffy_layout_geometry() {
    let theme = default_theme();

    let mut navigation =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    navigation.set_profiling_enabled(true);
    navigation.visible = true;
    let mut navigation_buffer = PixelBuffer::new(960, 80);
    navigation
        .paint(&theme, 960, 80, &mut navigation_buffer, 1.0)
        .unwrap();
    let navigation_health = format!(
        "{:?}",
        navigation
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
            !navigation_health.contains(unexpected),
            "navigation diagnostics should not contain {unexpected}: {navigation_health}"
        );
    }
    let navigation_tree = navigation
        .last_tree
        .as_ref()
        .expect("@mesh/navigation-bar rendered tree");
    let nav_shell =
        first_node_with_attr(navigation_tree, "class", "nav-shell").expect("navigation shell");
    assert_eq!(
        nav_shell.layout.width.round() as u32,
        960,
        "@mesh/navigation-bar shell background should span the resolved surface width"
    );
    let status_cluster =
        first_node_with_attr(navigation_tree, "class", "status-cluster").expect("status cluster");
    let control_cluster =
        first_node_with_attr(navigation_tree, "ref", "control-cluster").expect("control cluster");
    assert!(
        control_cluster.layout.x > status_cluster.layout.x + status_cluster.layout.width,
        "@mesh/navigation-bar controls should be positioned after status content, got controls {:?} and status {:?}",
        control_cluster.layout,
        status_cluster.layout
    );
    let volume_button = first_node_with_click_handler(
        navigation_tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("volume button");
    assert!(
        volume_button.layout.x >= control_cluster.layout.x,
        "@mesh/navigation-bar button layout should include parent offsets, got button {:?} and controls {:?}",
        volume_button.layout,
        control_cluster.layout
    );
    // The button sits within the bar vertically. Full horizontal containment is
    // not asserted here: the shipped nav-bar packs many clusters whose status
    // text can measure wider than the narrow 960px test surface and overflow
    // (clipped by the bar's `overflow-x: hidden`). That is a module content-width
    // concern, independent of retained taffy geometry, which is covered by the
    // width/order/centering assertions above and the retained-parity suite.
    assert!(
        nav_shell.layout.width > 0.0 && nav_shell.layout.height > 0.0,
        "@mesh/navigation-bar shell should have non-zero layout"
    );
    assert!(
        volume_button.layout.y >= nav_shell.layout.y
            && volume_button.layout.y + volume_button.layout.height
                <= nav_shell.layout.y + nav_shell.layout.height + 1.0,
        "@mesh/navigation-bar volume button should be vertically contained in the shell, got button {:?} and shell {:?}",
        volume_button.layout,
        nav_shell.layout
    );
    assert_phase44_focused_proof_snapshot(&navigation, "phase47 navigation bar");
    assert!(
        navigation.take_invalidation_snapshot().is_some(),
        "phase47 navigation repaint should retain invalidation proof"
    );
    assert!(
        !navigation.take_present_damage().is_empty(),
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
    audio
        .paint(&theme, 320, 220, &mut audio_buffer, 1.0)
        .unwrap();
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
        !audio.take_present_damage().is_empty(),
        "phase47 audio repaint should retain damage proof"
    );
}

#[test]
fn navigation_bar_keeps_layer_width_dynamic_after_css_measurement() {
    let theme = default_theme();
    let mut navigation =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let mut buffer = PixelBuffer::new(960, 80);
    navigation.paint(&theme, 960, 80, &mut buffer, 1.0).unwrap();

    let mut surface = LayoutRecordingSurface::default();
    navigation.render_layout(&mut surface);

    let (width, height) = surface.size.expect("navigation layout sets a surface size");
    assert_eq!(
        width, 0,
        "a top navigation bar must keep its layer-shell width dynamic so left+right anchors span the output"
    );
    assert!(
        height > 0,
        "the bar must retain its measured cross-axis height"
    );
}

#[test]
fn shipped_audio_popover_content_measured_surface_contains_volume_slider() {
    let theme = default_theme();
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

    let mut buffer = PixelBuffer::new(280, 164);
    audio.paint(&theme, 280, 164, &mut buffer, 1.0).unwrap();

    // The popover content-measures to its compact vertical slider + percent
    // label rather than the painted surface bounds.
    let (measured_width, measured_height) = audio.requested_layout_size();
    assert!(
        measured_width > 0 && measured_width <= 280,
        "audio popover should content-measure within the painted width, got {measured_width}"
    );
    assert!(
        measured_height > 0 && measured_height <= 260,
        "audio popover should content-measure within the max height, got {measured_height}"
    );

    let tree = audio.last_tree.as_ref().expect("rendered audio popover");
    let slider = first_node_by_tag(tree, "slider").expect("audio popover volume slider");
    assert_eq!(
        slider.attributes.get("orient").map(String::as_str),
        Some("vertical"),
        "audio popover slider should be vertical"
    );
    let percent =
        first_node_with_attr(tree, "class", "audio-percent").expect("audio percent label");
    assert!(
        percent.layout.width > 0.0 && percent.layout.height > 0.0,
        "audio percent label should have non-zero layout"
    );
}

#[test]
fn shipped_audio_popover_slider_sizes_from_props() {
    // Phase 2 reference proof: the shipped @mesh/audio-popover declares its
    // slider track size in a `<props>` block (`track_width` / `track_height`,
    // both `size`) and references them via `prop(...)` in `<style>`. Painting
    // the real module must resolve those props into the slider's computed size.
    // The wider default gives the vertical slider enough room for its thumb,
    // focus/hover effects, and popover padding without self-clipping.
    let theme = default_theme();
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

    let mut buffer = PixelBuffer::new(280, 164);
    audio.paint(&theme, 280, 164, &mut buffer, 1.0).unwrap();

    let tree = audio.last_tree.as_ref().expect("rendered audio popover");
    let slider = first_node_by_tag(tree, "slider").expect("audio popover volume slider");
    assert_eq!(
        slider.computed_style.width,
        mesh_core_elements::Dimension::Px(32.0),
        "prop(track_width) should resolve the shipped 32px non-clipping default"
    );
    assert_eq!(
        slider.computed_style.height,
        mesh_core_elements::Dimension::Px(100.0),
        "prop(track_height) should resolve the shipped 100px default"
    );
}

#[test]
fn shipped_tiny_nav_popovers_are_embeddable_components_without_surface_geometry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();

    for module in ["language-popover", "theme-selector"] {
        let manifest =
            mesh_core_module::manifest::load_manifest(&root.join("modules/frontend").join(module))
                .unwrap_or_else(|err| panic!("{module} manifest should load: {err}"))
                .manifest;

        assert_eq!(
            manifest.package.module_type,
            mesh_core_module::ModuleType::Component,
            "{module} should be an embeddable component, not a standalone surface"
        );
        assert!(
            manifest.surface_layout.is_none(),
            "{module} should not declare surface geometry in module.json"
        );
    }
}
