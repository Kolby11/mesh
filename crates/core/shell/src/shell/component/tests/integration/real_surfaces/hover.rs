use super::*;

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
    let theme_button =
        first_node_with_attr(tree, "aria-label", "Select theme").expect("theme button");
    let theme_button_key = theme_button
        .mesh_key()
        .expect("theme button key")
        .to_owned();
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
        .call_handler_target(
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
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    let requests = component.child_surface_requests();
    assert_eq!(
        requests.len(),
        1,
        "hover-opened theme selector should be promoted to one child popup request: {requests:?}"
    );
    assert_eq!(requests[0].content_size, (132, 60));
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
    assert_ne!(
        embedded_wrapper.computed_style.display,
        mesh_core_elements::style::Display::None,
        "promoted hidden wrapper must stay in layout so popup coordinates remain available"
    );
    assert_eq!(embedded_wrapper.layout.width, 0.0);
    assert_eq!(embedded_wrapper.layout.height, 0.0);
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
fn shipped_navigation_theme_and_language_pointer_hover_promotes_popovers() {
    let theme = default_theme();
    let width = 1280;
    let height = 80;

    for (handler, expected_size, expected_core_class, expected_text) in [
        (
            "__mesh_embed__::@mesh/navigation-bar/local:ThemeButton::onThemeToggle",
            (132, 60),
            "bubble-option-core-dark",
            None,
        ),
        (
            "__mesh_embed__::@mesh/navigation-bar/local:LanguageButton::onLanguageToggle",
            (132, 60),
            "bubble-option-core-language",
            Some("\u{1f1ec}\u{1f1e7}"),
        ),
    ] {
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

        let tree = component
            .last_tree
            .as_ref()
            .expect("rendered navigation bar");
        let nav_before = first_node_by_class(tree, "nav-shell")
            .expect("navigation shell before hover")
            .layout;
        let cluster_before_node =
            first_node_by_class(tree, "right-cluster").expect("navigation controls before hover");
        let cluster_before = cluster_before_node.layout;
        let nav_bounds_before = (
            nav_before.x,
            nav_before.y,
            nav_before.width,
            nav_before.height,
        );
        let cluster_bounds_before = (
            cluster_before.x,
            cluster_before.y,
            cluster_before.width,
            cluster_before.height,
        );
        let controls_before: Vec<_> = cluster_before_node
            .children
            .iter()
            .map(|child| {
                (
                    child.layout.x,
                    child.layout.y,
                    child.layout.width,
                    child.layout.height,
                )
            })
            .collect();
        let button = first_node_with_click_handler(tree, handler).expect("hover trigger button");
        let button_key = button.mesh_key().expect("button mesh key").to_owned();
        let (left, top, right, bottom) =
            find_node_bounds_by_key(tree, &button_key, 0.0, 0.0).expect("button bounds");

        component
            .handle_input(
                &theme,
                width,
                height,
                ComponentInput::PointerMove {
                    x: (left + right) / 2.0,
                    y: (top + bottom) / 2.0,
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
            .expect("rendered navigation bar after hover");
        let hovered_trigger = first_node_with_click_handler(tree, handler)
            .expect("hovered theme or language trigger");
        let nav_after = first_node_by_class(tree, "nav-shell")
            .expect("navigation shell after hover")
            .layout;
        let cluster_after_node =
            first_node_by_class(tree, "right-cluster").expect("navigation controls after hover");
        let cluster_after = cluster_after_node.layout;
        assert_eq!(
            (nav_after.x, nav_after.y, nav_after.width, nav_after.height),
            nav_bounds_before,
            "{handler} must not move or resize the navigation shell"
        );
        assert_eq!(
            (
                cluster_after.x,
                cluster_after.y,
                cluster_after.width,
                cluster_after.height
            ),
            cluster_bounds_before,
            "{handler} must not move or resize the navigation controls"
        );
        assert_eq!(
            cluster_after_node
                .children
                .iter()
                .map(|child| {
                    (
                        child.layout.x,
                        child.layout.y,
                        child.layout.width,
                        child.layout.height,
                    )
                })
                .collect::<Vec<_>>(),
            controls_before,
            "{handler} must not move or resize individual navigation controls"
        );
        assert_eq!(
            hovered_trigger.computed_style.transform.translate_y, 0.0,
            "{handler} must not inherit another navigation control's hover lift"
        );
        assert_eq!(
            hovered_trigger.computed_style.transform.scale_x, 1.0,
            "{handler} must not inherit another navigation control's hover scale"
        );

        let requests = component.child_surface_requests();
        assert_eq!(
            requests.len(),
            1,
            "{handler} should promote one child popup after real pointer hover: {requests:?}"
        );
        assert_eq!(requests[0].content_size, expected_size);
        assert_eq!(
            requests[0].anchor_rect,
            i32_rect((left, top, right, bottom)),
            "{handler} popup should anchor to the hovered trigger"
        );

        let mut child_buffer =
            PixelBuffer::new(requests[0].content_size.0, requests[0].content_size.1);
        assert!(
            component
                .paint_child_surface(&requests[0].node_key, &mut child_buffer, 1.0, (0, 0), false,)
                .unwrap(),
            "{handler} child popup should paint successfully"
        );
        let child_tree = component
            .child_surface_debug_tree(&requests[0].node_key, (0.0, 0.0))
            .expect("debug child tree");
        let mut classes = Vec::new();
        collect_class_attributes(&child_tree, &mut classes);
        let option = first_node_with_class_token(&child_tree, "bubble-option")
            .expect("child popup should contain a bubble hit target");
        assert_eq!(
            option.computed_style.background_color.a, 0,
            "{handler} stationary bubble hit targets must not paint circles at their final positions"
        );
        assert!(
            first_node_with_class_token(&child_tree, expected_core_class).is_some(),
            "{handler} child popup should render option body class {expected_core_class}, got {classes:?}"
        );
        if let Some(expected_text) = expected_text {
            let mut labels = Vec::new();
            collect_text_content(&child_tree, &mut labels);
            assert!(
                labels.iter().any(|label| label == expected_text),
                "{handler} child popup should render option text {expected_text}, got {labels:?}"
            );
        }
        let painted = opaque_pixels_in_bounds(
            &child_buffer,
            0.0,
            0.0,
            requests[0].content_size.0 as f32,
            requests[0].content_size.1 as f32,
        );
        assert!(
            painted > 0,
            "{handler} promoted popup should paint visible option pixels"
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
        let cluster_after_leave = first_node_by_class(
            component
                .last_tree
                .as_ref()
                .expect("tree after pointer leave"),
            "right-cluster",
        )
        .expect("navigation controls after pointer leave");
        assert_eq!(
            cluster_after_leave
                .children
                .iter()
                .map(|child| {
                    (
                        child.layout.x,
                        child.layout.y,
                        child.layout.width,
                        child.layout.height,
                    )
                })
                .collect::<Vec<_>>(),
            controls_before,
            "{handler} must not move controls while crossing into its popover"
        );

        let (popup_width, popup_height) = requests[0].content_size;
        for (x, y) in [
            (0, 0),
            (popup_width - 1, 0),
            (0, popup_height - 1),
            (popup_width - 1, popup_height - 1),
        ] {
            assert_eq!(
                child_buffer.get_pixel(x, y).a,
                0,
                "{handler} popup corner ({x}, {y}) must stay transparent instead of painting a bounding box"
            );
        }
    }
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

    // The audio, theme, and language controls each embed a `<popover>` as the
    // resting (closed) child of their trigger button. A collapsed popover must
    // stay out of flow: if its full-size content leaked into layout it would push
    // the trigger row's siblings into overlap (the audio/theme/language buttons
    // landing on top of each other). Verify the three trigger buttons tile
    // left-to-right without overlapping.
    // Matched by prefix: the audio trigger appends the live level to its label
    // ("Volume", "Volume 64%").
    let mut triggers: Vec<(f32, f32)> = ["Volume", "Select theme", "Choose language"]
        .into_iter()
        .map(|label| {
            let button = first_node_with_attr_prefix(tree, "aria-label", label)
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
