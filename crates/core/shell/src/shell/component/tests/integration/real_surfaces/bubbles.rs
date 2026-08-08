use super::*;

// Applying a theme is a round trip through the shell. Until the service reports
// back, the popover has to mark the picked bubble itself, or the ring keeps
// highlighting the theme the user just moved away from.
#[test]
fn shipped_theme_selector_marks_the_picked_bubble_active_immediately() {
    let (before, after) = pick_bubble_option(
        "__mesh_embed__::@mesh/navigation-bar/slot:end/default-2::onThemeEnter",
        2,
    );
    assert_eq!(
        before,
        Some(1),
        "the ring opens centred on the active theme"
    );
    assert_eq!(
        after,
        Some(2),
        "clicking a theme bubble moves the mark to it"
    );
}

#[test]
fn shipped_language_popover_marks_the_picked_bubble_active_immediately() {
    let (before, after) = pick_bubble_option(
        "__mesh_embed__::@mesh/navigation-bar/slot:end/default-3::onLanguageEnter",
        2,
    );
    assert_eq!(
        before,
        Some(1),
        "the ring opens centred on the active locale"
    );
    assert_eq!(
        after,
        Some(2),
        "clicking a locale bubble moves the mark to it"
    );
}

// The trigger used to keep its own two-entry locale table, so picking anything
// other than Slovak left it reading "EN". It now takes the flag from the
// popover, which owns the locale list.
#[test]
fn shipped_language_trigger_flag_follows_the_picked_locale() {
    let theme = default_theme();
    let width = 1280;
    let height = 80;
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({ "locale": "en", "current": "en" }),
        })
        .unwrap();
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    assert_eq!(
        trigger_flag(&component),
        Some("\u{1f1ec}\u{1f1e7}".to_string()),
        "the trigger starts on the active locale's flag"
    );

    component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/navigation-bar/slot:end/default-3::onLanguageEnter",
            &[],
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
    let requests = component.child_surface_requests();
    let node_key = requests[0].node_key.clone();
    let (cw, ch) = requests[0].content_size;
    let (pad_left, pad_top, ..) = requests[0].content_padding;
    let content_offset = (pad_left as f32, pad_top as f32);

    // Scroll one step so the right bubble is Czech: a locale the trigger's old
    // two-entry table did not know, and therefore used to render as "EN".
    component
        .handle_child_surface_input(
            &node_key,
            &theme,
            cw,
            ch,
            content_offset,
            ComponentInput::Scroll {
                x: content_offset.0 + cw as f32 / 2.0,
                y: content_offset.1 + ch as f32 / 2.0,
                dx: 0.0,
                dy: 1.0,
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

    let option_layout = {
        let tree = component
            .child_surface_debug_tree(&node_key, content_offset)
            .expect("language child tree");
        let mut options = Vec::new();
        collect_class_token_nodes(&tree, "bubble-option", &mut options);
        options[2].layout
    };
    let x = option_layout.x + option_layout.width / 2.0;
    let y = option_layout.y + option_layout.height / 2.0;
    let mut select_requests = Vec::new();
    for pressed in [true, false] {
        select_requests.extend(
            component
                .handle_child_surface_input(
                    &node_key,
                    &theme,
                    cw,
                    ch,
                    content_offset,
                    ComponentInput::PointerButton { x, y, pressed },
                )
                .unwrap(),
        );
    }
    assert!(
        select_requests
            .iter()
            .any(|request| matches!(request, CoreRequest::SetLocale { locale } if locale == "cs")),
        "clicking the bubble should request the locale: {select_requests:?}"
    );
    // Stand in for the shell applying `SetLocale`: `apply_set_locale` broadcasts
    // this, and it is what re-runs the trigger's render hook.
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.locale".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({ "locale": "cs", "current": "cs" }),
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

    assert_eq!(
        trigger_flag(&component),
        Some("\u{1f1e8}\u{1f1ff}".to_string()),
        "picking a locale updates the trigger flag"
    );
}

// With every installed theme on screen at once, turning the ring could only
// permute the same bubbles — "scrolling shows no other options". The ring now
// windows three of the installed themes and scrolling brings in the rest.
#[test]
fn shipped_theme_selector_scroll_reveals_themes_outside_the_window() {
    let theme = default_theme();
    let width = 1280;
    let height = 80;
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    component
        .call_namespaced_handler(
            "__mesh_embed__::@mesh/navigation-bar/slot:end/default-2::onThemeEnter",
            &[],
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

    let requests = component.child_surface_requests();
    let node_key = requests[0].node_key.clone();
    let (cw, ch) = requests[0].content_size;
    let (pad_left, pad_top, ..) = requests[0].content_padding;
    let content_offset = (pad_left as f32, pad_top as f32);

    let swatches = |component: &FrontendSurfaceComponent| -> Vec<String> {
        let tree = component
            .child_surface_debug_tree(&node_key, content_offset)
            .expect("theme child tree");
        let mut cores = Vec::new();
        collect_class_token_nodes(&tree, "bubble-option-core", &mut cores);
        cores
            .iter()
            .filter_map(|node| node.attributes.get("class"))
            .filter_map(|class| {
                class
                    .split_whitespace()
                    .find(|token| {
                        token.starts_with("bubble-option-core-")
                            && !token.ends_with("-active")
                            && !token.ends_with("-blur")
                    })
                    .map(str::to_owned)
            })
            .collect()
    };

    let before = swatches(&component);
    assert_eq!(
        before.len(),
        3,
        "only three themes are on screen: {before:?}"
    );

    let mut seen: Vec<String> = before.clone();
    for _ in 0..4 {
        component
            .handle_child_surface_input(
                &node_key,
                &theme,
                cw,
                ch,
                content_offset,
                ComponentInput::Scroll {
                    x: content_offset.0 + cw as f32 / 2.0,
                    y: content_offset.1 + ch as f32 / 2.0,
                    dx: 0.0,
                    dy: 1.0,
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
        let now = swatches(&component);
        assert_eq!(now.len(), 3, "the window stays three wide: {now:?}");
        for swatch in now {
            if !seen.contains(&swatch) {
                seen.push(swatch);
            }
        }
    }

    assert_eq!(
        seen.len(),
        7,
        "four scroll steps should have brought every installed theme into view: {seen:?}"
    );
}
