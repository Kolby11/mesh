use super::*;

#[test]
fn shipped_navigation_brightness_uses_one_level_icon_and_scrolls_both_input_kinds() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.brightness".into(),
            source_module: "@mesh/backlight-brightness".into(),
            payload: serde_json::json!({ "level": 50 }),
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
    let button = first_node_by_class(tree, "brightness-button").expect("brightness button");
    let button_x = button.layout.x + button.layout.width / 2.0;
    let button_y = button.layout.y + button.layout.height / 2.0;
    let icons: Vec<_> = button
        .children
        .iter()
        .filter(|child| child.tag == "icon")
        .collect();
    assert_eq!(icons.len(), 1, "brightness button must contain one icon");
    assert_eq!(
        icons[0].attributes.get("name").map(String::as_str),
        Some("display-brightness-medium")
    );
    let icon_key = icons[0].mesh_key().expect("brightness icon key");
    assert_eq!(
        find_tooltip_text_by_key(tree, icon_key).as_deref(),
        Some("Brightness 50%")
    );

    let wheel_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::Scroll {
                x: button_x,
                y: button_y,
                dx: 0.0,
                dy: -1.0,
            },
        )
        .unwrap();
    assert!(wheel_requests.iter().any(|request| {
        service_request_parts(request).is_some_and(|(interface, command, payload)| {
            interface == "mesh.brightness"
                && command == "set"
                && payload == &serde_json::json!({ "level": 45 })
        })
    }));

    let touchpad_down_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::TwoFingerScroll {
                x: button_x,
                y: button_y,
                dx: 0.0,
                dy: -1.0,
            },
        )
        .unwrap();
    assert!(touchpad_down_requests.iter().any(|request| {
        service_request_parts(request).is_some_and(|(interface, command, payload)| {
            interface == "mesh.brightness"
                && command == "set"
                && payload == &serde_json::json!({ "level": 40 })
        })
    }));

    let touchpad_up_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::TwoFingerScroll {
                x: button_x,
                y: button_y,
                dx: 0.0,
                dy: 1.0,
            },
        )
        .unwrap();
    assert!(touchpad_up_requests.iter().any(|request| {
        service_request_parts(request).is_some_and(|(interface, command, payload)| {
            interface == "mesh.brightness"
                && command == "set"
                && payload == &serde_json::json!({ "level": 45 })
        })
    }));
}

#[test]
fn shipped_navigation_brightness_uses_configured_scroll_sensitivity() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.settings_json = serde_json::json!({
        "props": {
            "instances": {
                "@mesh/navigation-bar/slot:end/default-0": {
                    "scroll_sensitivity": 12
                }
            }
        }
    });
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.brightness".into(),
            source_module: "@mesh/backlight-brightness".into(),
            payload: serde_json::json!({ "level": 50 }),
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
    let button = first_node_by_class(
        component.last_tree.as_ref().expect("navigation tree"),
        "brightness-button",
    )
    .expect("brightness button");
    let button_x = button.layout.x + button.layout.width / 2.0;
    let button_y = button.layout.y + button.layout.height / 2.0;

    let wheel_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::Scroll {
                x: button_x,
                y: button_y,
                dx: 0.0,
                dy: -1.0,
            },
        )
        .unwrap();
    assert!(wheel_requests.iter().any(|request| {
        service_request_parts(request).is_some_and(|(interface, command, payload)| {
            interface == "mesh.brightness"
                && command == "set"
                && payload == &serde_json::json!({ "level": 38 })
        })
    }));

    let touchpad_requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::TwoFingerScroll {
                x: button_x,
                y: button_y,
                dx: 0.0,
                dy: 1.0,
            },
        )
        .unwrap();
    assert!(touchpad_requests.iter().any(|request| {
        service_request_parts(request).is_some_and(|(interface, command, payload)| {
            interface == "mesh.brightness"
                && command == "set"
                && payload == &serde_json::json!({ "level": 50 })
        })
    }));
}

#[test]
fn shipped_navigation_brightness_falls_back_for_invalid_scroll_sensitivity() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.settings_json = serde_json::json!({
        "props": {
            "instances": {
                "@mesh/navigation-bar/slot:end/default-0": {
                    "scroll_sensitivity": 0
                }
            }
        }
    });
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.brightness".into(),
            source_module: "@mesh/backlight-brightness".into(),
            payload: serde_json::json!({ "level": 50 }),
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
    let button = first_node_by_class(
        component.last_tree.as_ref().expect("navigation tree"),
        "brightness-button",
    )
    .expect("brightness button");
    let button_x = button.layout.x + button.layout.width / 2.0;
    let button_y = button.layout.y + button.layout.height / 2.0;

    let requests = component
        .handle_input(
            &theme,
            width,
            height,
            ComponentInput::Scroll {
                x: button_x,
                y: button_y,
                dx: 0.0,
                dy: 1.0,
            },
        )
        .unwrap();
    assert!(requests.iter().any(|request| {
        service_request_parts(request).is_some_and(|(interface, command, payload)| {
            interface == "mesh.brightness"
                && command == "set"
                && payload == &serde_json::json!({ "level": 55 })
        })
    }));
}
