use super::*;
use mesh_core_frontend_host::ShellComponent;

#[test]
fn navigation_bar_keyboard_shortcut_and_theme_activation_work_on_real_surface() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
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
    {
        let tree = component
            .last_tree
            .as_ref()
            .expect("rendered navigation tree");
        let subscribers = component.keybind_subscribers(tree);
        assert!(
            subscribers
                .iter()
                .any(|subscriber| subscriber.keybind_id == "mute"
                    && subscriber.handler.contains("onMuteShortcut")),
            "navigation mute keybind should expose its subscribed handler"
        );
    }
    let shortcut_requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyPressed {
                key: "m".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(matches!(
        shortcut_requests.as_slice(),
        [CoreRequest::ServiceCommand { interface, command, payload, .. }]
            if interface == "mesh.audio"
                && command == "set_muted"
                && payload["device_id"] == serde_json::json!("default")
                && payload["muted"] == serde_json::json!(true)
    ));

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    let theme_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeToggle",
    )
    .expect("rendered theme button");
    let theme_key = theme_button
        .mesh_key()
        .expect("theme button mesh key")
        .to_owned();
    let theme_bounds =
        find_node_bounds_by_key(tree, &theme_key, 0.0, 0.0).expect("theme button bounds");
    component.focused_key = Some(theme_key.clone());
    component.focus_visible_key = Some(theme_key.clone());

    let activation_requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        activation_requests.is_empty(),
        "embedded theme selector should open through component state, not legacy surface requests: {activation_requests:?}"
    );
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let child_requests = component.child_surface_requests();
    assert_eq!(
        child_requests.len(),
        1,
        "keyboard activation should derive one promoted theme selector popup: {child_requests:?}"
    );
    assert_eq!(child_requests[0].content_size, (132, 60));
    assert!(
        rect_matches_bounds(child_requests[0].anchor_rect, theme_bounds),
        "theme selector popup should anchor to the theme trigger bounds {:?}, got {:?}",
        theme_bounds,
        child_requests[0].anchor_rect
    );

    let focused_option_key = component
        .focused_key
        .clone()
        .expect("keyboard activation should focus the first theme option");
    let tree = component
        .last_tree
        .as_ref()
        .expect("navigation tree after opening theme popover");
    assert!(
        find_node_by_key(tree, &focused_option_key).is_some_and(|node| {
            node.attributes
                .get("class")
                .is_some_and(|class| class.split_whitespace().any(|part| part == "bubble-option"))
        }),
        "keyboard activation should move focus into a theme bubble option"
    );

    let selection_requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        selection_requests.iter().any(|request| matches!(
            request,
            CoreRequest::SetTheme { theme_id } if theme_id == "solarized-dark"
        )),
        "Enter on a focused theme option should select it: {selection_requests:?}"
    );

    component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyPressed {
                key: "Escape".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    assert!(
        component.child_surface_requests().is_empty(),
        "Escape from a focused theme option should close the derived popup"
    );
    assert_eq!(
        component.focused_key.as_deref(),
        Some(theme_key.as_str()),
        "Escape should restore focus to the theme trigger"
    );
}

#[test]
fn navigation_language_button_opens_language_popover_on_real_surface() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "locale": "en",
                "current": "en"
            }),
        })
        .unwrap();
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
        .expect("rendered navigation tree");
    let language_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageToggle",
    )
    .expect("language menu button");
    let language_key = language_button
        .mesh_key()
        .expect("language menu button mesh key")
        .to_owned();
    let language_bounds =
        find_node_bounds_by_key(tree, &language_key, 0.0, 0.0).expect("language button bounds");
    component.focused_key = Some(language_key.clone());
    component.focus_visible_key = Some(language_key.clone());

    let open_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        open_requests.is_empty(),
        "embedded language popover should open through component state, not legacy surface requests: {open_requests:?}"
    );
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let child_requests = component.child_surface_requests();
    assert_eq!(
        child_requests.len(),
        1,
        "keyboard activation should derive one promoted language popup: {child_requests:?}"
    );
    assert_eq!(child_requests[0].content_size, (132, 60));
    assert!(
        rect_matches_bounds(child_requests[0].anchor_rect, language_bounds),
        "language popup should anchor to the language trigger bounds {:?}, got {:?}",
        language_bounds,
        child_requests[0].anchor_rect
    );

    let focused_option_key = component
        .focused_key
        .clone()
        .expect("keyboard activation should focus the first language option");
    let tree = component
        .last_tree
        .as_ref()
        .expect("navigation tree after opening language popover");
    assert!(
        find_node_by_key(tree, &focused_option_key).is_some_and(|node| {
            node.attributes
                .get("class")
                .is_some_and(|class| class.split_whitespace().any(|part| part == "bubble-option"))
        }),
        "keyboard activation should move focus into a language bubble option"
    );

    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyPressed {
                key: "ArrowDown".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert_ne!(
        component.focused_key.as_deref(),
        Some(focused_option_key.as_str()),
        "ArrowDown should move focus to the next language option"
    );

    let select_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    assert!(
        select_requests
            .iter()
            .any(|request| matches!(request, CoreRequest::SetLocale { locale } if locale == "en")),
        "Enter on a focused language option should select it: {select_requests:?}"
    );

    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyPressed {
                key: "Escape".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    assert!(
        component.child_surface_requests().is_empty(),
        "Escape from a focused language option should close the derived popup"
    );
    assert_eq!(
        component.focused_key.as_deref(),
        Some(language_key.as_str()),
        "Escape should restore focus to the language trigger"
    );
}

#[test]
fn navigation_theme_and_language_popovers_close_when_trigger_hover_leaves() {
    let theme = default_theme();
    let width = 960;
    let height = 80;

    for (enter_handler, leave_handler) in [
        (
            "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeEnter",
            "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeLeave",
        ),
        (
            "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageEnter",
            "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageLeave",
        ),
    ] {
        let mut component =
            real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
        let mut buffer = PixelBuffer::new(width, height);
        component
            .handle_service_event(&ServiceEvent::Updated {
                service: "mesh.locale".into(),
                source_module: "@mesh/shell".into(),
                payload: serde_json::json!({ "locale": "en", "current": "en" }),
            })
            .unwrap();
        component
            .paint(
                &theme,
                SurfaceExtent::unpadded(width, height),
                &mut buffer,
                1.0,
            )
            .unwrap();

        component
            .call_namespaced_handler(enter_handler, &[])
            .unwrap();
        component
            .paint(
                &theme,
                SurfaceExtent::unpadded(width, height),
                &mut buffer,
                1.0,
            )
            .unwrap();
        assert_eq!(
            component.child_surface_requests().len(),
            1,
            "{enter_handler} should open one embedded popover"
        );

        component
            .call_namespaced_handler(leave_handler, &[])
            .unwrap();
        component
            .paint(
                &theme,
                SurfaceExtent::unpadded(width, height),
                &mut buffer,
                1.0,
            )
            .unwrap();
        assert_eq!(
            component.child_surface_requests().len(),
            1,
            "{leave_handler} should keep its embedded popover open during the hover bridge"
        );

        std::thread::sleep(Duration::from_millis(220));
        component.tick().unwrap();
        component
            .paint(
                &theme,
                SurfaceExtent::unpadded(width, height),
                &mut buffer,
                1.0,
            )
            .unwrap();
        assert_eq!(
            component.child_surface_requests().len(),
            1,
            "{leave_handler} should retain its popover across a deliberate pointer trip"
        );

        std::thread::sleep(Duration::from_millis(220));
        component.tick().unwrap();
        component
            .paint(
                &theme,
                SurfaceExtent::unpadded(width, height),
                &mut buffer,
                1.0,
            )
            .unwrap();
        assert!(
            component.child_surface_requests().is_empty(),
            "{leave_handler} should close its embedded popover after the hover bridge expires"
        );
    }
}

#[test]
fn navigation_language_popover_closes_when_pointer_leaves_promoted_popup() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({ "locale": "en", "current": "en" }),
        })
        .unwrap();
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
        .expect("rendered navigation tree");
    let language_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageToggle",
    )
    .expect("language menu button");
    let language_key = language_button
        .mesh_key()
        .expect("language menu button mesh key")
        .to_owned();
    component.focused_key = Some(language_key.clone());
    component.focus_visible_key = Some(language_key);

    // Open the popover.
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let child_requests = component.child_surface_requests();
    assert_eq!(
        child_requests.len(),
        1,
        "popover should be open: {child_requests:?}"
    );
    let node_key = child_requests[0].node_key.clone();
    let (cw, ch) = child_requests[0].content_size;
    let (pad_left, pad_top, ..) = child_requests[0].content_padding;
    let content_offset = (pad_left as f32, pad_top as f32);

    // Pointer moves into the promoted popup (cancels the trigger's close bridge).
    component
        .handle_child_surface_input(
            &node_key,
            &theme,
            cw,
            ch,
            content_offset,
            ComponentInput::PointerMove {
                x: content_offset.0 + cw as f32 / 2.0,
                y: content_offset.1 + ch as f32 / 2.0,
            },
        )
        .unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    assert_eq!(
        component.child_surface_requests().len(),
        1,
        "popover must stay open while the pointer is over the promoted popup"
    );

    // Pointer leaves the promoted popup. Keep it alive during the same bridge
    // window used in the trigger-to-popup direction, so crossing a transparent
    // gap or returning to the trigger cannot destroy it underneath the cursor.
    component
        .handle_child_surface_input(
            &node_key,
            &theme,
            cw,
            ch,
            content_offset,
            ComponentInput::PointerLeave,
        )
        .unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    assert_eq!(
        component.child_surface_requests().len(),
        1,
        "language popover must remain open during the popup-to-trigger bridge"
    );

    std::thread::sleep(Duration::from_millis(420));
    component.tick().unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    assert!(
        component.child_surface_requests().is_empty(),
        "language popover must close after the popup hover bridge expires"
    );
}

#[test]
fn navigation_language_option_cancels_hover_close_and_accepts_mouse_click() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    let theme = default_theme();
    let width = 960;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({ "locale": "en", "current": "en" }),
        })
        .unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    let enter_handler =
        "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageEnter";
    let leave_handler =
        "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageLeave";
    component
        .call_namespaced_handler(enter_handler, &[])
        .unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let requests = component.child_surface_requests();
    assert_eq!(requests.len(), 1, "language popover should open");
    let node_key = requests[0].node_key.clone();
    let (cw, ch) = requests[0].content_size;
    // The popup buffer is padded for descendant overshoot, so the popover
    // subtree starts at `content_padding` inside it and pointer coordinates
    // arrive in that padded space.
    let (pad_left, pad_top, ..) = requests[0].content_padding;
    let content_offset = (pad_left as f32, pad_top as f32);
    assert!(
        pad_left > 0 && pad_top > 0,
        "the bubble popover pads its buffer for blur/transform overshoot, which is \
         exactly the offset child-surface hit testing has to account for"
    );
    let child_tree = component
        .child_surface_debug_tree(&node_key, content_offset)
        .expect("language child tree");
    let option =
        first_node_with_class_token(&child_tree, "bubble-option").expect("language bubble option");
    let option_x = option.layout.x + option.layout.width / 2.0;
    let option_y = option.layout.y + option.layout.height / 2.0;

    // Reproduce the pointer crossing from the trigger into an option: leaving
    // the trigger starts the bridge timer, and entering the actual button must
    // cancel it (not merely entering the promoted surface root).
    component
        .call_namespaced_handler(leave_handler, &[])
        .unwrap();
    component
        .handle_child_surface_input(
            &node_key,
            &theme,
            cw,
            ch,
            content_offset,
            ComponentInput::PointerMove {
                x: option_x,
                y: option_y,
            },
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(420));
    component.tick().unwrap();
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    assert_eq!(
        component.child_surface_requests().len(),
        1,
        "entering a language option must cancel the trigger close timer"
    );

    component
        .handle_child_surface_input(
            &node_key,
            &theme,
            cw,
            ch,
            content_offset,
            ComponentInput::PointerButton {
                x: option_x,
                y: option_y,
                pressed: true,
            },
        )
        .unwrap();
    let click_requests = component
        .handle_child_surface_input(
            &node_key,
            &theme,
            cw,
            ch,
            content_offset,
            ComponentInput::PointerButton {
                x: option_x,
                y: option_y,
                pressed: false,
            },
        )
        .unwrap();
    assert!(
        click_requests
            .iter()
            .any(|request| matches!(request, CoreRequest::SetLocale { .. })),
        "mouse release over a language option should select it: {click_requests:?}"
    );
}

#[test]
fn navigation_shipped_i18n_covers_all_template_translation_keys() {
    fn collect_keys(source: &str, keys: &mut Vec<String>) {
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
    }

    let mut keys = Vec::new();
    for source in [
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/main.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/battery-button.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/language-button.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/meta-label.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/meta-pill.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/settings-button.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/theme-button.mesh"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
        )),
    ] {
        collect_keys(source, &mut keys);
    }
    keys.sort();
    keys.dedup();

    let en: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/navigation-bar/config/i18n/en.json"
    )))
    .unwrap();
    let sk: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/navigation-bar/config/i18n/sk.json"
    )))
    .unwrap();

    for key in keys {
        assert!(
            en.get(&key).is_some(),
            "missing English nav translation for {key}"
        );
        assert!(
            sk.get(&key).is_some(),
            "missing Slovak nav translation for {key}"
        );
    }
}

#[test]
fn navigation_shipped_keybind_metadata_resolves_from_i18n_catalogs() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.locale.set_locale("sk");
    component.runtimes.lock().unwrap().clear();
    component.init_root_runtime().unwrap();

    let keybinds = component.debug_surface_keybinds();
    let mute = keybinds
        .iter()
        .find(|entry| entry.action_id == "mute")
        .expect("navigation mute debug keybind");

    assert_eq!(mute.label.as_deref(), Some("Stlmit zvuk"));
    assert_eq!(mute.description.as_deref(), Some("Prepnut stlmenie zvuku"));
    assert_eq!(mute.category.as_deref(), Some("Zvuk"));
    assert_eq!(mute.label_key.as_deref(), Some("keybind.mute.label"));
    assert_eq!(
        mute.description_key.as_deref(),
        Some("keybind.mute.description")
    );
    assert_eq!(mute.category_key.as_deref(), Some("keybind.category.audio"));
    assert_eq!(mute.accessibility_shortcut, "m");
}

#[test]
fn navigation_settings_button_drops_its_tooltip_while_quick_settings_is_open() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
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
        .expect("rendered navigation tree");
    let settings_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:SettingsButton::onOpenSettings",
    )
    .expect("rendered settings button");
    let settings_key = settings_button
        .mesh_key()
        .expect("settings button mesh key")
        .to_owned();
    assert!(
        settings_button
            .attributes
            .get("title")
            .is_some_and(|title| !title.is_empty()),
        "the resting settings trigger should carry a tooltip"
    );

    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &settings_key, 0.0, 0.0).expect("settings bounds");
    component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::PointerMove {
                x: (left + right) * 0.5,
                y: (top + bottom) * 0.5,
            },
        )
        .unwrap();
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
        .expect("navigation tree after hovering settings");
    let settings_button =
        find_node_by_key(tree, &settings_key).expect("settings button after hover");
    // The promoted quick-settings popup covers the tooltip's placement, leaving
    // only a sliver of it visible below the bar. The trigger must drop the
    // tooltip while the popup is open.
    assert_eq!(
        settings_button.attributes.get("title").map(String::as_str),
        Some(""),
        "hovering should clear the settings tooltip text"
    );
    assert_eq!(
        settings_button
            .attributes
            .get("data-tooltip-disabled")
            .map(String::as_str),
        Some("true"),
        "hovering should disable the settings tooltip"
    );

    component
        .handle_input(&theme, width, height, ComponentInput::PointerLeave)
        .unwrap();
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
        .expect("navigation tree after leaving settings");
    let settings_button =
        find_node_by_key(tree, &settings_key).expect("settings button after leave");
    assert!(
        settings_button
            .attributes
            .get("title")
            .is_some_and(|title| !title.is_empty()),
        "closing the popup should restore the settings tooltip"
    );
    assert_ne!(
        settings_button
            .attributes
            .get("data-tooltip-disabled")
            .map(String::as_str),
        Some("true"),
        "closing the popup should re-enable the settings tooltip"
    );
}

#[test]
fn navigation_bar_pointer_click_opens_settings_and_updates_focus_diagnostic() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
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
        .expect("rendered navigation tree");
    let settings_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:SettingsButton::onOpenSettings",
    )
    .expect("rendered settings button");
    let settings_key = settings_button
        .mesh_key()
        .expect("settings button mesh key")
        .to_owned();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &settings_key, 0.0, 0.0).expect("settings bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;
    component
        .handle_input(
            &theme,
            320,
            80,
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

    assert!(matches!(
        requests.as_slice(),
        [CoreRequest::ShowSurface { surface_id }] if surface_id == "@mesh/settings"
    ));
    assert_eq!(
        component.focused_key.as_deref(),
        Some(settings_key.as_str())
    );
}

#[test]
fn navigation_bar_real_surface_keeps_status_copy_non_selectable() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 42,
                "muted": false
            }),
        })
        .unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(420, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(420, 80), &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation tree");
    assert_eq!(
        count_selectable_text_nodes(tree),
        0,
        "the shipped nav bar should not expose selectable passive text nodes"
    );
}

#[test]
fn navigation_bar_keyboard_activation_toggles_volume_mute_on_real_surface() {
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
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();

    component.focused_key = Some(volume_key.clone());
    component.focus_visible_key = Some(volume_key);

    let requests = component
        .handle_input(
            &theme,
            320,
            80,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: KeyModifiers::default(),
            },
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
        "Enter on the focused volume button should toggle mute: {requests:?}"
    );
}

#[test]
fn navigation_bar_pointer_activation_toggles_volume_mute() {
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
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;

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
            width,
            height,
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
            CoreRequest::ServiceCommand { interface, command, payload, .. }
                if interface == "mesh.audio"
                    && command == "set_muted"
                    && payload == &serde_json::json!({
                        "device_id": "default",
                        "muted": true
                    })
        )),
        "pointer click should toggle mute: {requests:?}"
    );
}

#[test]
fn navigation_bar_volume_scroll_changes_level_immediately() {
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
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;

    let down_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::Scroll {
                x,
                y,
                dx: 0.0,
                dy: -1.0,
            },
        )
        .unwrap();
    assert!(
        down_requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { interface, command, payload, .. }
                if interface == "mesh.audio"
                    && command == "set_volume"
                    && payload["device_id"] == serde_json::json!("default")
                    && payload["percent"] == serde_json::json!(45)
        )),
        "scrolling down should lower volume by the default step: {down_requests:?}"
    );

    let up_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::TwoFingerScroll {
                x,
                y,
                dx: 0.0,
                dy: 1.0,
            },
        )
        .unwrap();
    assert!(
        up_requests.iter().any(|request| matches!(
            request,
            CoreRequest::ServiceCommand { interface, command, payload, .. }
                if interface == "mesh.audio"
                    && command == "set_volume"
                    && payload["device_id"] == serde_json::json!("default")
                    && payload["percent"] == serde_json::json!(50)
        )),
        "two-finger scroll up should raise volume by the default step: {up_requests:?}"
    );
}

#[test]
fn navigation_bar_volume_scroll_respects_instance_sensitivity() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.settings_json = serde_json::json!({
        "props": {
            "instances": {
                "@mesh/navigation-bar/local:VolumeButton": {
                    "scroll_sensitivity": 12
                }
            }
        }
    });
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
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = (left + right) * 0.5;
    let y = (top + bottom) * 0.5;

    let requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::Scroll {
                x,
                y,
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
                    && payload["percent"] == serde_json::json!(62)
        )),
        "configured volume scroll sensitivity should apply to wheel input: {requests:?}"
    );
}

#[test]
fn navigation_bar_volume_trigger_keeps_click_capture_during_press_animation() {
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
        .expect("rendered navigation tree");
    let volume_button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("rendered volume button");
    let volume_key = volume_button
        .mesh_key()
        .expect("volume button mesh key")
        .to_owned();
    let (_left, top, right, bottom) =
        find_node_bounds_by_key(tree, &volume_key, 0.0, 0.0).expect("volume bounds");
    let x = right - 0.5;
    let y = (top + bottom) * 0.5;

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
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let requests = component
        .handle_input(
            &theme,
            width,
            height,
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
            CoreRequest::ServiceCommand { interface, command, payload, .. }
                if interface == "mesh.audio"
                    && command == "set_muted"
                    && payload == &serde_json::json!({
                        "device_id": "default",
                        "muted": true
                    })
        )),
        "release at the original press point should still click while the active animation changes visual bounds: {requests:?}"
    );
}

#[test]
fn navigation_bar_keyboard_audio_popover_slider_responds_to_arrow_keys() {
    let mut component =
        real_frontend_module_component("@mesh/audio-popover", audio_network_catalog());
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

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(320, 220);
    component
        .paint(&theme, SurfaceExtent::unpadded(320, 220), &mut buffer, 1.0)
        .unwrap();
    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered audio popover");
    let slider = first_node_by_tag(tree, "slider").expect("slider node");
    let slider_key = slider.mesh_key().expect("slider key").to_owned();
    component.focused_key = Some(slider_key);

    let requests = component
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
    match requests.as_slice() {
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
            assert_eq!(payload["device_id"], serde_json::json!("default"));
            let percent = payload["percent"]
                .as_f64()
                .expect("numeric percent payload");
            assert!(
                (percent - 55.0).abs() < 0.001,
                "expected slider keyboard step near 55%, got {percent}"
            );
        }
        other => panic!("expected one audio set_volume request, got {other:?}"),
    }
}

#[test]
fn phase44_navigation_behavior_survives_focused_proof_path() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(960, 80);
    component
        .paint(&theme, SurfaceExtent::unpadded(960, 80), &mut buffer, 1.0)
        .unwrap();
    assert!(
        component.last_focused_proof_snapshot().is_some(),
        "initial navigation paint should store focused proof evidence"
    );

    component
        .handle_input(
            &theme,
            960,
            80,
            ComponentInput::KeyPressed {
                key: "Tab".into(),
                modifiers: KeyModifiers::default(),
            },
        )
        .unwrap();
    component
        .paint(&theme, SurfaceExtent::unpadded(960, 80), &mut buffer, 1.0)
        .unwrap();

    assert!(
        component.last_focused_proof_snapshot().is_some(),
        "keyboard navigation repaint should keep focused proof evidence"
    );
    assert!(
        component.focused_key.is_some(),
        "Tab navigation should focus a shipped navigation control"
    );
    assert_eq!(
        component.focused_key, component.focus_visible_key,
        "keyboard focus should remain visibly tracked after focused proof paint"
    );
}
