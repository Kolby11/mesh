use super::*;

#[test]
fn shipped_theme_selector_restarts_bubble_launch_on_surface_reshow() {
    let theme = default_theme();
    let mut theme_selector =
        real_frontend_module_component("@mesh/theme-selector", audio_network_catalog());
    theme_selector.popup_promoted = true;
    theme_selector.visible = true;
    theme_selector.set_surface_exiting(false);

    let mut buffer = PixelBuffer::new(112, 92);
    theme_selector
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
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
        entering_tree.computed_style.opacity, 1.0,
        "entering popover root must stay visible while launching"
    );
    assert_eq!(
        entering_motion.computed_style.opacity, 1.0,
        "entering bubble content must stay visible while launching"
    );
    assert_eq!(
        entering_motion.computed_style.transform.translate_x, 46.0,
        "motion wrapper should visually launch from the trigger origin"
    );
    assert_eq!(
        entering_motion.computed_style.transform.translate_y, 4.0,
        "motion wrapper should visually launch from the trigger origin"
    );

    theme_selector
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
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
    theme_selector.popup_promoted = true;
    theme_selector.visible = true;
    theme_selector.set_surface_exiting(false);

    let mut buffer = PixelBuffer::new(112, 92);
    theme_selector
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
        .unwrap();
    // Settle past the entering frame so the baseline paint below isn't itself
    // carrying `mesh-surface-entering`.
    theme_selector
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
        .unwrap();

    let popover_key = theme_selector
        .last_tree
        .as_ref()
        .and_then(|tree| first_node_with_class_token(tree, "theme-float-shell"))
        .and_then(|node| node.mesh_key())
        .expect("theme selector root should be a keyed popover node")
        .to_owned();

    // This is the same shell -> component channel `reconcile_child_surface_requests`
    // uses once a promoted popover's node drops out of the open requests while
    // its own CSS exit transition still has time left to run.
    theme_selector.set_closing_child_keys([popover_key.clone()].into_iter().collect());
    theme_selector
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
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
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
        .unwrap();
    let popover_key = theme_selector
        .last_tree
        .as_ref()
        .and_then(|tree| first_node_with_class_token(tree, "theme-float-shell"))
        .and_then(|node| node.mesh_key())
        .expect("theme selector root should be a keyed popover node")
        .to_owned();

    theme_selector.set_entering_child_keys([popover_key].into_iter().collect());
    theme_selector
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
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
    assert_eq!(motion.computed_style.transform.scale_x, 0.12);
    // The entering pose keeps full opacity — only the exiting rule fades out.
    assert_eq!(motion.computed_style.opacity, 1.0);
}

#[test]
fn shipped_theme_selector_buttons_accept_first_entering_frame_clicks() {
    let theme = default_theme();
    let mut theme_selector =
        real_frontend_module_component("@mesh/theme-selector", audio_network_catalog());
    theme_selector.popup_promoted = true;
    theme_selector.visible = true;
    theme_selector.set_surface_exiting(false);

    let mut buffer = PixelBuffer::new(112, 92);
    theme_selector
        .paint(&theme, SurfaceExtent::unpadded(112, 92), &mut buffer, 1.0)
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
            CoreRequest::ServiceCommand { interface, command, payload, .. }
                if interface == "mesh.theme"
                    && command == "set_theme"
                    && payload.get("theme_id")
                        == Some(&serde_json::json!("mesh-default-dark"))
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
    assert_eq!(
        requests.len(),
        1,
        "language popover should open: {requests:?}"
    );
    let node_key = requests[0].node_key.clone();
    let (cw, ch) = requests[0].content_size;
    let (pad_left, pad_top, ..) = requests[0].content_padding;
    let content_offset = (pad_left as f32, pad_top as f32);

    let labels = bubble_option_labels(&component, &node_key, content_offset);
    assert_eq!(
        labels,
        vec![
            "\u{1f1f0}\u{1f1f7}".to_string(),
            "\u{1f1ec}\u{1f1e7}".to_string(),
            "\u{1f1f8}\u{1f1f0}".to_string()
        ],
        "the ring opens centred on the current locale, showing locale flags"
    );

    // Scrolling over the bubbles advances the window *and* keeps it advanced:
    // re-centring on every render used to revert the step before it painted.
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

    let labels = bubble_option_labels(&component, &node_key, content_offset);
    assert_eq!(
        labels,
        vec![
            "\u{1f1ec}\u{1f1e7}".to_string(),
            "\u{1f1f8}\u{1f1f0}".to_string(),
            "\u{1f1e8}\u{1f1ff}".to_string()
        ],
        "one scroll step turns the ring by exactly one slot"
    );

    let rotating = component
        .child_surface_debug_tree(&node_key, content_offset)
        .expect("language child tree");
    // Every bubble travels, and the middle slot travels along a different arc
    // leg than the outer two — that height difference is the rotation.
    let mut motions = Vec::new();
    collect_class_token_nodes(&rotating, "bubble-options-motion", &mut motions);
    let arc_variants: Vec<String> = motions
        .iter()
        .filter_map(|node| node.attributes.get("class"))
        .filter_map(|class| {
            class
                .split_whitespace()
                .find(|token| token.starts_with("bubble-options-rotate-"))
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(
        arc_variants.len(),
        3,
        "a scroll step should arm the arc travel on every bubble: {arc_variants:?}"
    );
    assert!(
        arc_variants[1].contains("-center-"),
        "the middle slot travels its own arc leg: {arc_variants:?}"
    );
    assert!(
        arc_variants[0].contains("-outer-") && arc_variants[2].contains("-outer-"),
        "the outer slots share one arc leg: {arc_variants:?}"
    );
    // The classes have to reach real keyframes, not just be present in markup.
    let resolved: Vec<String> = motions
        .iter()
        .filter_map(|node| node.computed_style.animations.first())
        .filter_map(|animation| animation.name.clone())
        .collect();
    assert_eq!(
        resolved,
        vec![
            "bubble-options-arc-next-outer-b".to_string(),
            "bubble-options-arc-next-center-b".to_string(),
            "bubble-options-arc-next-outer-b".to_string(),
        ],
        "each travel class should resolve to its arc keyframes"
    );

    // The bubble that appeared at the trailing edge fades in.
    let mut buttons = Vec::new();
    collect_class_token_nodes(&rotating, "bubble-option", &mut buttons);
    let arriving: Vec<usize> = buttons
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.attributes.get("class").is_some_and(|class| {
                class
                    .split_whitespace()
                    .any(|token| token.starts_with("bubble-option-arriving-"))
            })
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        arriving,
        vec![2],
        "turning the ring forward makes exactly the trailing bubble appear"
    );
}

// A trackpad reports continuous pixel deltas through `ontwofingerscroll`, a
// different event from the wheel's `onscroll`. The stage has to bind both, and
// accumulate the continuous stream into discrete ring steps.
#[test]
fn shipped_language_popover_rotates_on_two_finger_trackpad_scroll() {
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
    let x = content_offset.0 + cw as f32 / 2.0;
    let y = content_offset.1 + ch as f32 / 2.0;

    let before = bubble_option_labels(&component, &node_key, content_offset);

    // A few small deltas stay inside one notch: the ring must not spin.
    for _ in 0..3 {
        component
            .handle_child_surface_input(
                &node_key,
                &theme,
                cw,
                ch,
                content_offset,
                ComponentInput::TwoFingerScroll {
                    x,
                    y,
                    dx: 0.0,
                    dy: 9.0,
                },
            )
            .unwrap();
    }
    component
        .paint(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    assert_eq!(
        bubble_option_labels(&component, &node_key, content_offset),
        before,
        "27px of trackpad travel is less than one notch and must not step the ring"
    );

    // Crossing the notch spends exactly one step.
    component
        .handle_child_surface_input(
            &node_key,
            &theme,
            cw,
            ch,
            content_offset,
            ComponentInput::TwoFingerScroll {
                x,
                y,
                dx: 0.0,
                dy: 20.0,
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
    let after = bubble_option_labels(&component, &node_key, content_offset);
    assert_ne!(
        after, before,
        "crossing the notch should turn the ring one slot"
    );
    assert_eq!(
        after[0], before[1],
        "one notch turns the ring by exactly one slot: {before:?} -> {after:?}"
    );
}
