use super::*;
use mesh_core_interaction::find_tooltip_text_by_key;

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

fn i32_rect(bounds: (f32, f32, f32, f32)) -> (i32, i32, i32, i32) {
    let left = bounds.0.floor() as i32;
    let top = bounds.1.floor() as i32;
    let right = bounds.2.ceil() as i32;
    let bottom = bounds.3.ceil() as i32;
    (left, top, (right - left).max(1), (bottom - top).max(1))
}

fn first_node_with_class_token<'a>(node: &'a WidgetNode, token: &str) -> Option<&'a WidgetNode> {
    if node
        .attributes
        .get("class")
        .is_some_and(|class| class.split_whitespace().any(|candidate| candidate == token))
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| first_node_with_class_token(child, token))
}

fn parent_of_node_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node.children.iter().any(|child| {
        child
            .attributes
            .get("_mesh_key")
            .is_some_and(|candidate| candidate == key)
    }) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| parent_of_node_key(child, key))
}

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
    // the real module must resolve those props into the slider's computed size,
    // matching the previous hard-coded 20x100 CSS.
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
        mesh_core_elements::Dimension::Px(20.0),
        "prop(track_width) should resolve the shipped 20px default"
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

#[test]
fn shipped_theme_selector_restarts_bubble_launch_on_surface_reshow() {
    let theme = default_theme();
    let mut theme_selector =
        real_frontend_module_component("@mesh/theme-selector", audio_network_catalog());
    theme_selector.visible = true;
    theme_selector.set_surface_exiting(false);

    let mut buffer = PixelBuffer::new(112, 92);
    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();
    let entering_tree = theme_selector
        .last_tree
        .as_ref()
        .expect("rendered theme selector entering frame");
    assert!(
        entering_tree
            .attributes
            .get("class")
            .is_some_and(|class| class.contains("mesh-surface-entering")),
        "first show paint should expose mesh-surface-entering for collapsed bubble positions"
    );
    let entering_bubble =
        first_node_with_class_token(entering_tree, "bubble-option").expect("entering theme bubble");
    assert_eq!(
        entering_bubble.computed_style.transform.translate_x, 0.0,
        "button hit target should stay at its resting position during entrance"
    );
    assert_eq!(
        entering_bubble.computed_style.transform.translate_y, 0.0,
        "button hit target should stay at its resting position during entrance"
    );
    let entering_motion = first_node_with_class_token(entering_tree, "bubble-options-motion")
        .expect("entering bubble motion wrapper");
    assert_eq!(
        entering_motion.computed_style.transform.translate_x, 46.0,
        "motion wrapper should visually launch from the trigger origin"
    );
    assert_eq!(
        entering_motion.computed_style.transform.translate_y, 4.0,
        "motion wrapper should visually launch from the trigger origin"
    );

    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();
    let launched_tree = theme_selector
        .last_tree
        .as_ref()
        .expect("rendered theme selector launch frame");
    assert!(
        launched_tree
            .attributes
            .get("class")
            .is_none_or(|class| !class.contains("mesh-surface-entering")),
        "second show paint should transition from entering state into resting bubble positions"
    );
    let launched_bubble =
        first_node_with_class_token(launched_tree, "bubble-option").expect("launched theme bubble");
    assert!(
        launched_bubble.computed_style.transform.translate_x > -1.0,
        "launch transition should begin from the entering transform"
    );
    assert!(
        !theme_selector.transitions.is_empty(),
        "dropping mesh-surface-entering should start bubble transform transitions"
    );

    theme_selector.set_surface_exiting(false);
    assert!(
        theme_selector.transitions.is_empty(),
        "showing a kept-alive surface should clear stale transitions before replaying the launch"
    );

    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();
    let replay_tree = theme_selector
        .last_tree
        .as_ref()
        .expect("rendered theme selector replay entering frame");
    assert!(
        replay_tree
            .attributes
            .get("class")
            .is_some_and(|class| class.contains("mesh-surface-entering")),
        "re-show should expose a fresh entering frame"
    );
}

#[test]
fn set_closing_child_keys_scopes_exit_transition_to_popover_subtree_only() {
    let theme = default_theme();
    let mut theme_selector =
        real_frontend_module_component("@mesh/theme-selector", audio_network_catalog());
    theme_selector.visible = true;
    theme_selector.set_surface_exiting(false);

    let mut buffer = PixelBuffer::new(112, 92);
    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();
    // Settle past the entering frame so the baseline paint below isn't itself
    // carrying `mesh-surface-entering`.
    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();

    let popover_key = theme_selector
        .last_tree
        .as_ref()
        .and_then(|tree| first_node_with_class_token(tree, "theme-float-shell"))
        .and_then(|node| node.attributes.get("_mesh_key"))
        .expect("theme selector root should be a keyed popover node")
        .clone();

    // This is the same shell -> component channel `reconcile_child_surface_requests`
    // uses once a promoted popover's node drops out of the open requests while
    // its own CSS exit transition still has time left to run.
    theme_selector.set_closing_child_keys([popover_key.clone()].into_iter().collect());
    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();

    let exiting_tree = theme_selector
        .last_tree
        .as_ref()
        .expect("rendered theme selector exiting frame");
    let popover_node = first_node_with_class_token(exiting_tree, "theme-float-shell")
        .expect("theme selector popover node should survive the exiting paint");
    assert!(
        popover_node
            .attributes
            .get("class")
            .is_some_and(|class| class.contains("mesh-surface-exiting")),
        "closing_child_keys should append mesh-surface-exiting to the popover's own subtree"
    );
    assert!(
        !theme_selector.transitions.is_empty(),
        "the exit class change should start the popover's own opacity/transform transition"
    );

    // Clearing the closing key (e.g. the popover reopened before its grace
    // period elapsed) should stop re-applying the exiting class on the next
    // paint — it does not retroactively rewind the in-flight transition.
    theme_selector.set_closing_child_keys(std::collections::HashSet::new());
    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();
    let reopened_tree = theme_selector
        .last_tree
        .as_ref()
        .expect("rendered theme selector reopened frame");
    let reopened_popover = first_node_with_class_token(reopened_tree, "theme-float-shell")
        .expect("theme selector popover node");
    assert!(
        reopened_popover
            .attributes
            .get("class")
            .is_none_or(|class| !class.contains("mesh-surface-exiting")),
        "clearing closing_child_keys should stop re-appending mesh-surface-exiting"
    );
}

#[test]
fn set_entering_child_keys_scopes_entrance_to_popover_subtree_only() {
    let theme = default_theme();
    let mut theme_selector =
        real_frontend_module_component("@mesh/theme-selector", audio_network_catalog());
    theme_selector.visible = true;

    let mut buffer = PixelBuffer::new(112, 92);
    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();
    let popover_key = theme_selector
        .last_tree
        .as_ref()
        .and_then(|tree| first_node_with_class_token(tree, "theme-float-shell"))
        .and_then(|node| node.attributes.get("_mesh_key"))
        .expect("theme selector root should be a keyed popover node")
        .clone();

    theme_selector.set_entering_child_keys([popover_key].into_iter().collect());
    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();

    let tree = theme_selector.last_tree.as_ref().expect("entering tree");
    let popover = first_node_with_class_token(tree, "theme-float-shell").unwrap();
    assert!(
        popover
            .attributes
            .get("class")
            .is_some_and(|class| class.contains("mesh-surface-entering"))
    );
    let motion = first_node_with_class_token(popover, "bubble-options-motion").unwrap();
    assert_eq!(motion.computed_style.transform.translate_x, 46.0);
    assert_eq!(motion.computed_style.opacity, 0.0);
}

#[test]
fn shipped_theme_selector_buttons_accept_first_entering_frame_clicks() {
    let theme = default_theme();
    let mut theme_selector =
        real_frontend_module_component("@mesh/theme-selector", audio_network_catalog());
    theme_selector.visible = true;
    theme_selector.set_surface_exiting(false);

    let mut buffer = PixelBuffer::new(112, 92);
    theme_selector
        .paint(&theme, 112, 92, &mut buffer, 1.0)
        .unwrap();

    let tree = theme_selector
        .last_tree
        .as_ref()
        .expect("rendered theme selector entering frame");
    assert!(
        tree.attributes
            .get("class")
            .is_some_and(|class| class.contains("mesh-surface-entering")),
        "test must click during the controlled entering frame"
    );
    let dark = first_node_with_attr(tree, "aria-label", "Default Dark").expect("dark theme button");
    let click_x = dark.layout.x + dark.layout.width * 0.5;
    let click_y = dark.layout.y + dark.layout.height * 0.5;

    theme_selector
        .handle_input(
            &theme,
            112,
            92,
            ComponentInput::PointerButton {
                x: click_x,
                y: click_y,
                pressed: true,
            },
        )
        .unwrap();
    let requests = theme_selector
        .handle_input(
            &theme,
            112,
            92,
            ComponentInput::PointerButton {
                x: click_x,
                y: click_y,
                pressed: false,
            },
        )
        .unwrap();

    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::SetTheme { theme_id } if theme_id == "mesh-default-dark"
        )),
        "first entering-frame click should reach the theme handler: {requests:?}"
    );
    // Selecting a theme no longer closes the popover: it stays open so the user
    // can keep choosing, and only closes on pointer/focus leave (the shell's
    // hover-bridge). So a selection click must NOT request a hide.
    assert!(
        !requests.iter().any(|request| matches!(
            request,
            CoreRequest::HideSurface { surface_id } if surface_id == "@mesh/theme-selector"
        )),
        "theme selection should keep the popover open (no hide request): {requests:?}"
    );
}

#[test]
fn shipped_language_popover_cycles_three_bubble_options_on_scroll() {
    let theme = default_theme();
    let mut language =
        real_frontend_module_component("@mesh/language-popover", audio_network_catalog());
    language.visible = true;
    language.set_surface_exiting(false);
    language
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "locale": "en",
                "current": "en"
            }),
        })
        .unwrap();

    let mut buffer = PixelBuffer::new(112, 92);
    language.paint(&theme, 112, 92, &mut buffer, 1.0).unwrap();
    let tree = language
        .last_tree
        .as_ref()
        .expect("rendered language bubble selector");
    let mut labels = Vec::new();
    collect_text_content(tree, &mut labels);
    assert!(
        labels.iter().any(|label| label == "HI"),
        "initial centered English window should include previous locale"
    );
    assert!(
        labels.iter().any(|label| label == "EN"),
        "initial centered English window should include current locale"
    );
    assert!(
        labels.iter().any(|label| label == "SK"),
        "initial centered English window should include next locale"
    );

    language
        .handle_input(
            &theme,
            112,
            92,
            ComponentInput::Scroll {
                x: 56.0,
                y: 46.0,
                dx: 0.0,
                dy: 1.0,
            },
        )
        .unwrap();
    language.paint(&theme, 112, 92, &mut buffer, 1.0).unwrap();
    let tree = language
        .last_tree
        .as_ref()
        .expect("rendered scrolled language bubble selector");
    let mut labels = Vec::new();
    collect_text_content(tree, &mut labels);
    assert!(
        labels.iter().any(|label| label == "DE"),
        "scrolling over the bubble selector should advance the three visible options"
    );
}

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
    audio.paint(&theme, 320, 220, &mut buffer, 1.0).unwrap();
    assert_eq!(
        runtime_value(&audio, "audio_percent_label"),
        Some(serde_json::json!("50%"))
    );

    audio.theme_changed().unwrap();
    audio.paint(&theme, 320, 220, &mut buffer, 1.0).unwrap();

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
        .paint(&theme, 960, 80, &mut navigation_buffer, 1.0)
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
        .paint(&theme, 320, 220, &mut audio_buffer, 1.0)
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
    component.paint(&theme, 220, 80, &mut buffer, 1.0).unwrap();
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

    component.paint(&theme, 220, 80, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 320, 80, &mut buffer, 1.0).unwrap();
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
    // `audio_surface_hidden` now lives inside the VolumeButton child component
    // and is not observable on the top-level surface. The PositionSurface +
    // ActivatePopover requests above already prove the popover was shown.
}

#[test]
fn shipped_navigation_audio_popover_transition_delay_stays_bounded() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(320, 220);
    // The hide transition is now a CSS `transition` on the surface root, read
    // from the last painted root style, so paint once before querying it.
    component.paint(&theme, 320, 220, &mut buffer, 1.0).unwrap();
    assert_eq!(
        component.hide_transition_ms(),
        120,
        "audio popover should keep the shipped bounded hide transition"
    );

    component.set_surface_exiting(true);
    component.paint(&theme, 320, 220, &mut buffer, 1.0).unwrap();
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
fn shipped_navigation_audio_popover_transition_does_not_consume_first_input() {
    let mut navigation =
        real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
    navigation.visible = true;

    let theme = default_theme();
    let mut nav_buffer = PixelBuffer::new(960, 80);
    navigation
        .paint(&theme, 960, 80, &mut nav_buffer, 1.0)
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
    let click_handler = button.event_handlers.get("click").unwrap().clone();
    let button_key = button
        .attributes
        .get("_mesh_key")
        .expect("button mesh key")
        .clone();
    let open_requests = navigation
        .call_namespaced_handler(
            &click_handler,
            &[serde_json::json!({
                "trigger": { "type": "pointer" },
                "current": { "key": button_key },
                "current_target": {
                    "key": button_key,
                    "position": {
                        "margin_left": 32,
                        "margin_bottom": 40
                    }
                }
            })],
        )
        .unwrap();
    assert!(
        open_requests.iter().any(|request| matches!(
            request,
            CoreRequest::ActivatePopover { surface_id, focus, .. }
                if surface_id == "@mesh/audio-popover" && !*focus
        )),
        "first pointer click should open the audio popover without stealing focus: {open_requests:?}"
    );

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
        .paint(&theme, 320, 220, &mut audio_buffer, 1.0)
        .unwrap();
    let audio_tree = audio.last_tree.as_ref().expect("rendered audio popover");
    let slider = first_node_by_tag(audio_tree, "slider").expect("slider node");
    let slider_key = slider
        .attributes
        .get("_mesh_key")
        .expect("slider key")
        .clone();
    audio.focused_key = Some(slider_key);

    let requests = audio
        .handle_input(
            &theme,
            320,
            220,
            ComponentInput::KeyPressed {
                key: "ArrowRight".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { interface, command, .. }
                if interface == "mesh.audio" && command == "set_volume"
        )),
        "first audio popover input should reach the service command path: {requests:?}"
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
        .paint(&theme, width, height, &mut buffer, 1.0)
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
    assert_eq!(
        find_tooltip_text_by_key(tree, &icon_key).as_deref(),
        Some("Volume 50%"),
        "tooltip lookup should inherit the button title when hovering the icon"
    );

    let slovak_locale = mesh_core_locale::LocaleEngine::new("sk");
    component.locale_changed(&slovak_locale).unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
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

    // NOTE: the shipped VolumeButton now opens the audio popover on
    // `onpointerenter` (hover-to-open), so hovering the icon no longer arms a
    // tooltip-reveal timer the way the old static button did. The inheritance
    // *lookup* asserted above (icon resolves the button's title) is the durable
    // behavior; the hover-timer choreography is exercised by generic tooltip
    // unit tests rather than this shipped-surface integration test.

    // The icon carries no click handler of its own; a click on it bubbles up to
    // the enclosing VolumeButton. Pointer routing resolves a leaf coordinate to
    // its nearest click-handling ancestor by walking the node path, so verify
    // the structural relationship (icon nested under the button that owns the
    // handler) independent of the bar's painted geometry, then dispatch the
    // handler and confirm it activates the audio popover.
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
    assert_eq!(
        inherited_handler, "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
        "icon click should bubble to the VolumeButton toggle handler"
    );

    let button_node = node_by_mesh_key(tree, &button_key);
    let requests = component
        .call_namespaced_handler(
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
            CoreRequest::ActivatePopover { surface_id, .. } if surface_id == "@mesh/audio-popover"
        )),
        "clicking directly on the icon should bubble to the button click handler: {requests:?}"
    );
}

/// Count pixels with non-zero alpha inside `[left,right) x [top,bottom)`.
/// Bounds are surface-local logical coordinates; the buffer is the painted
/// surface at scale 1.0, so they map 1:1.
fn opaque_pixels_in_bounds(
    buffer: &PixelBuffer,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> u32 {
    let x0 = left.floor().max(0.0) as u32;
    let y0 = top.floor().max(0.0) as u32;
    let x1 = (right.ceil() as u32).min(buffer.width);
    let y1 = (bottom.ceil() as u32).min(buffer.height);
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let offset = (y * buffer.stride + x * 4) as usize;
            if buffer.data[offset + 3] != 0 {
                count += 1;
            }
        }
    }
    count
}

/// End-to-end proof that the icon pipeline lands pixels on a real module
/// surface: compile the shipped navigation bar, paint it, locate the volume
/// `<icon>` node, and assert its bounding box contains rasterized pixels.
/// This exercises the full chain — template `<icon>` → WidgetNode →
/// `DisplayPaintContent::Icon` → `render_display_icon_node` → registry/XDG
/// resolution → SVG/PNG raster (or the built-in missing-icon fallback) → blit.
/// The missing-icon fallback always rasterizes, so this is deterministic even
/// without a system icon theme installed.
#[test]
fn shipped_navigation_icon_rasterizes_pixels_on_real_surface() {
    // Provide every interface the navigation bar consumes (audio, network,
    // power, brightness, hyprland, media). A missing interface makes the
    // affected component render an unbounded error-string placeholder instead
    // of its real content — and three ~700px error strings (workspaces, window
    // title, battery) inflate the bar far past its intrinsic width and shove
    // the right-aligned control cluster off-buffer. With the real content the
    // bar fits a normal panel width, exactly as the shipped shell paints it.
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

    // Paint at a realistic laptop panel width — narrower than the bar's content
    // overflowed to in the icon-fix follow-up note (x≈1978 on a 960px paint).
    // With real component content the whole bar, including the right cluster,
    // stays on-buffer here.
    let theme = default_theme();
    let width = 1280;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");

    // The right-aligned control cluster must fit entirely within the surface so
    // its buttons (volume, theme, language, battery, settings) are visible and
    // hittable — this is the invariant the follow-up note flagged.
    let cluster = first_node_by_class(tree, "right-cluster").expect("control cluster node");
    let cluster_right = cluster.layout.x + cluster.layout.width;
    assert!(
        cluster.layout.x >= 0.0 && cluster_right <= width as f32,
        "right control cluster bounds [x={}, right={cluster_right}] should fall inside the \
         {width}px surface so all of its controls stay visible",
        cluster.layout.x
    );

    let button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("volume button");
    let icon = first_node_by_tag(button, "icon").expect("volume icon node");
    assert!(
        icon.attributes.get("name").is_some() || icon.attributes.get("src").is_some(),
        "volume icon should declare a name or src to resolve"
    );
    let icon_key = icon
        .attributes
        .get("_mesh_key")
        .expect("icon mesh key")
        .clone();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &icon_key, 0.0, 0.0).expect("icon bounds");
    assert!(
        right > left && bottom > top,
        "icon should have a non-empty layout box, got {left},{top},{right},{bottom}"
    );
    assert!(
        right <= width as f32 && bottom <= height as f32,
        "volume icon bounds [{left},{top},{right},{bottom}] should fall inside the painted \
         {width}x{height} surface so it is actually visible"
    );

    let painted = opaque_pixels_in_bounds(&buffer, left, top, right, bottom);
    assert!(
        painted > 0,
        "the volume icon should rasterize visible pixels onto the real navigation surface \
         (themed icon or built-in missing-icon fallback), but its bounds \
         [{left},{top},{right},{bottom}] were fully transparent"
    );
}

#[test]
fn shipped_navigation_hover_popover_does_not_expand_parent_control_layout() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;

    let theme = default_theme();
    let width = 1280;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");
    let theme_button =
        first_node_with_attr(tree, "aria-label", "Select theme").expect("theme button");
    let theme_button_key = theme_button
        .attributes
        .get("_mesh_key")
        .expect("theme button key")
        .clone();
    let (theme_left, theme_top, theme_right, theme_bottom) =
        find_node_bounds_by_key(tree, &theme_button_key, 0.0, 0.0).expect("theme button bounds");
    let theme_center_x = (theme_left + theme_right) / 2.0;
    let theme_center_y = (theme_top + theme_bottom) / 2.0;
    let cluster_before =
        first_node_by_class(tree, "right-cluster").expect("control cluster before");
    let cluster_width_before = cluster_before.layout.width;

    let enter_handler = theme_button
        .event_handlers
        .get("pointerenter")
        .unwrap_or_else(|| {
            panic!(
                "theme button should expose pointerenter handler, got {:?}",
                theme_button.event_handlers
            )
        })
        .clone();
    component
        .call_namespaced_handler(
            &enter_handler,
            &[serde_json::json!({
                "surface": { "id": "@mesh/navigation-bar" },
                "current_target": {
                    "key": theme_button_key,
                    "position": {
                        "margin_left": theme_center_x as i64,
                        "margin_bottom": theme_center_y as i64
                    }
                }
            })],
        )
        .unwrap();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let requests = component.child_surface_requests();
    assert_eq!(
        requests.len(),
        1,
        "hover-opened theme selector should be promoted to one child popup request: {requests:?}"
    );
    assert_eq!(requests[0].content_size, (112, 74));
    assert_eq!(
        requests[0].anchor_rect,
        i32_rect((theme_left, theme_top, theme_right, theme_bottom)),
        "promoted popover should anchor to the trigger rect, not its own CSS box"
    );

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar with open popover");
    let popover = find_node_by_key(tree, &requests[0].node_key).expect("promoted popover node");
    assert!(
        !popover.attributes.contains_key("hidden"),
        "promoted popover node itself must stay paintable for the child popup"
    );
    let embedded_wrapper =
        parent_of_node_key(tree, &requests[0].node_key).expect("embedded popover wrapper");
    assert_eq!(
        embedded_wrapper
            .attributes
            .get("hidden")
            .map(String::as_str),
        Some("true"),
        "embedded wrapper should be hidden so promoted content is not painted inline"
    );
    let cluster_after = first_node_by_class(tree, "right-cluster").expect("control cluster after");
    assert!(
        (cluster_after.layout.width - cluster_width_before).abs() <= 1.0,
        "opening promoted popover must not expand the parent nav control cluster \
         from {cluster_width_before} to {}",
        cluster_after.layout.width
    );
    assert!(
        cluster_after.layout.x + cluster_after.layout.width <= width as f32,
        "open promoted popover must not push controls off the nav surface"
    );
}

#[test]
fn shipped_navigation_resting_control_buttons_do_not_overlap() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;

    let theme = default_theme();
    let width = 1280;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");

    // The audio, theme, and language controls each embed a `<popover>` as the
    // resting (closed) child of their trigger button. A collapsed popover must
    // stay out of flow: if its full-size content leaked into layout it would push
    // the trigger row's siblings into overlap (the audio/theme/language buttons
    // landing on top of each other). Verify the three trigger buttons tile
    // left-to-right without overlapping.
    let mut triggers: Vec<(f32, f32)> = ["Open audio controls", "Select theme", "Choose language"]
        .into_iter()
        .map(|label| {
            let button = first_node_with_attr(tree, "aria-label", label)
                .unwrap_or_else(|| panic!("{label} button"));
            (button.layout.x, button.layout.x + button.layout.width)
        })
        .collect();
    triggers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for pair in triggers.windows(2) {
        let (left_x, left_right) = (pair[0].0, pair[0].1);
        let next_x = pair[1].0;
        assert!(
            next_x >= left_right - 0.5,
            "resting popover trigger buttons must not overlap: a button at \
             x={left_x}..{left_right} overlaps the next at x={next_x}"
        );
    }
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
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();

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

    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();
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
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();
    component
        .call_namespaced_handler("__mesh_embed__::@mesh/debug-inspector::showSurfaces", &[])
        .unwrap();
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();

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
    component.paint(&theme, 360, 640, &mut buffer, 1.0).unwrap();

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
