use super::*;

#[test]
fn settings_page_title_keeps_its_full_line_box() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);

    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let tree = settings.last_tree.as_ref().expect("rendered settings tree");
    let title = first_node_with_class_token(tree, "page-title").expect("settings page title");
    let required_height = title.computed_style.font_size * title.computed_style.line_height;

    assert!(
        title.layout.height + 0.5 >= required_height,
        "settings page title height {} should preserve its {}px line box (flex-shrink {})",
        title.layout.height,
        required_height,
        title.computed_style.flex_shrink
    );
}

#[test]
fn settings_wrapped_descriptions_expand_to_their_content() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);

    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let tree = settings.last_tree.as_ref().expect("rendered settings tree");
    let settings_description = first_node_with_class_token(tree, "sidebar-foot-copy")
        .expect("settings sidebar description");
    assert!(
        settings_description.layout.height > 22.5,
        "wrapped settings description should exceed the theme's fixed 22px text height, got {}",
        settings_description.layout.height
    );

    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAudio", &[])
        .unwrap();
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let tree = settings
        .last_tree
        .as_ref()
        .expect("rendered audio settings tree");
    let audio_description = first_node_with_attr(
        tree,
        "content",
        "Adjust the default output through the active PipeWire or PulseAudio provider.",
    )
    .expect("audio output description");
    assert!(
        (audio_description.layout.height - 22.0).abs() > 0.5,
        "audio description should use intrinsic height instead of the theme's fixed 22px height"
    );
    assert!(
        audio_description.layout.height > 0.0,
        "audio description should retain a visible line box, got {}",
        audio_description.layout.height
    );
}

#[test]
fn settings_tab_switch_resets_scroll_and_replaces_the_visible_page() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);

    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showBluetooth", &[])
        .unwrap();
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let scroll_id = first_node_with_attr(
        settings
            .last_tree
            .as_ref()
            .expect("rendered Bluetooth tree"),
        "ref",
        "settings_scroll",
    )
    .map(|node| node.id)
    .expect("settings scroll node");
    settings.scroll_offsets.entry(scroll_id).or_default().y = 120.0;

    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    assert_eq!(
        settings
            .scroll_offsets
            .get(&scroll_id)
            .map(|offset| offset.y),
        Some(0.0),
        "switching settings pages should reset the shared scroll container"
    );
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let command_text = settings
        .display_list_paint_commands()
        .iter()
        .filter_map(|command| match &command.node.content {
            mesh_core_render::display_list::DisplayPaintContent::Text(text) => {
                Some(text.text.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        command_text.contains(&"Color theme"),
        "Appearance paint commands should be present after the switch"
    );
    assert!(
        !command_text.contains(&"My devices") && !command_text.contains(&"Nearby devices"),
        "Bluetooth page paint commands must be removed after switching to Appearance"
    );
}

#[test]
fn settings_scrollbar_is_conditional_on_overflow() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);

    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let tree = settings.last_tree.as_ref().expect("rendered settings tree");
    let scroll =
        first_node_with_attr(tree, "ref", "settings_scroll").expect("settings scroll container");
    assert_eq!(
        scroll.computed_style.overflow_y,
        mesh_core_elements::style::Overflow::Auto,
        "the settings scrollbar should appear only when its content overflows"
    );
}

fn count_tree_nodes(node: &WidgetNode) -> usize {
    1 + node.children.iter().map(count_tree_nodes).sum::<usize>()
}

fn count_class_token(node: &WidgetNode, token: &str) -> usize {
    usize::from(
        node.attributes
            .get("class")
            .is_some_and(|classes| classes.split_whitespace().any(|class| class == token)),
    ) + node
        .children
        .iter()
        .map(|child| count_class_token(child, token))
        .sum::<usize>()
}

fn seed_large_settings_resource_catalog(settings: &mut FrontendSurfaceComponent) {
    let icon_themes = (0..240)
        .map(|index| {
            serde_json::json!({
                "id": format!("icon-theme-{index:03}"),
                "name": format!("Icon Theme {index:03}"),
                "inherits": ["hicolor"]
            })
        })
        .collect::<Vec<_>>();
    let font_families = (0..240)
        .map(|index| {
            serde_json::json!({
                "name": format!("Font Family {index:03}"),
                "face_count": 4,
                "monospace": index % 5 == 0
            })
        })
        .collect::<Vec<_>>();
    settings
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: "@mesh/core-settings".into(),
            payload: serde_json::json!({
                "current": "mesh-default-dark",
                "theme_id": "mesh-default-dark",
                "is_dark": true,
                "themes": [{ "id": "mesh-default-dark", "label": "MESH Dark" }],
                "available": ["mesh-default-dark"],
                "system_resources": {
                    "active_icon_theme": "icon-theme-000",
                    "active_font_family": "Font Family 000",
                    "icon_themes": icon_themes,
                    "font_families": font_families
                }
            }),
        })
        .unwrap();
    settings.invalidate_script_state();
}

#[test]
fn settings_appearance_resource_lists_are_bounded() {
    let theme = default_theme();
    let mut settings =
        real_frontend_module_component("@mesh/settings", super::super::debug::settings_catalog());
    let mut buffer = PixelBuffer::new(920, 900);
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    seed_large_settings_resource_catalog(&mut settings);
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    let appearance_tree = settings
        .last_tree
        .as_ref()
        .expect("rendered Appearance tree");
    assert_eq!(
        count_class_token(appearance_tree, "resource-option"),
        48,
        "Appearance should instantiate only one 24-row window per large resource catalog"
    );
}

#[test]
fn settings_appearance_resource_scroll_keeps_the_existing_tree() {
    let theme = default_theme();
    let mut settings =
        real_frontend_module_component("@mesh/settings", super::super::debug::settings_catalog());
    let mut buffer = PixelBuffer::new(920, 900);
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    seed_large_settings_resource_catalog(&mut settings);
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let tree = settings
        .last_tree
        .as_ref()
        .expect("rendered Appearance tree");
    let resource_list = first_node_with_class_token(tree, "resource-list")
        .expect("Appearance should contain a scrollable resource list");
    assert!(
        resource_list.resolved_scroll_metrics().max_y > 0.0,
        "the bounded resource list should still have scrollable overflow"
    );
    let scroll_id = resource_list.id;
    settings.scroll_offsets.insert(
        scroll_id,
        mesh_core_interaction::ScrollOffsetState { x: 0.0, y: 120.0 },
    );
    settings.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let scrolled = settings
        .last_tree
        .as_ref()
        .expect("scrolled Appearance tree");
    assert_eq!(
        first_node_with_class_token(scrolled, "resource-list")
            .expect("resource list after scroll")
            .resolved_scroll_metrics()
            .y,
        120.0
    );
    assert!(
        settings
            .display_list_paint_commands()
            .iter()
            .any(|command| {
                matches!(
                    &command.node.content,
                    mesh_core_render::display_list::DisplayPaintContent::Text(text)
                        if text.text.starts_with("Icon Theme")
                )
            }),
        "scrolling Appearance should keep resource content in the retained display list"
    );
}

// cargo test -p mesh-core-shell --release -- settings_active_page_switch_cost --ignored --nocapture
#[test]
#[ignore = "release-only real Settings page-switch benchmark"]
fn settings_active_page_switch_cost() {
    const SWITCHES: usize = 24;
    let theme = default_theme();
    let handlers = [
        "showWifi",
        "showAudio",
        "showBluetooth",
        "showAppearance",
        "showDevice",
        "showAdvanced",
    ];
    let mut samples = Vec::new();
    let mut node_counts = Vec::new();

    for sample in 0..5 {
        let mut settings = real_frontend_module_component(
            "@mesh/settings",
            super::super::debug::settings_catalog(),
        );
        let mut buffer = PixelBuffer::new(920, 900);
        settings
            .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
        seed_large_settings_resource_catalog(&mut settings);
        settings
            .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();

        let started = std::time::Instant::now();
        for index in 0..SWITCHES {
            let handler = handlers[(index + sample) % handlers.len()];
            let namespaced = format!("__mesh_embed__::@mesh/settings::{handler}");
            settings.call_namespaced_handler(&namespaced, &[]).unwrap();
            settings
                .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
                .unwrap();
            std::hint::black_box(settings.display_list_paint_commands().len());
        }
        samples.push(started.elapsed());
        node_counts.push(count_tree_nodes(
            settings.last_tree.as_ref().expect("rendered Settings tree"),
        ));
    }

    samples.sort_unstable();
    node_counts.sort_unstable();
    eprintln!(
        "Settings {SWITCHES}-page switch samples: {:?}..{:?}, median {:?}; final tree nodes {}..{}",
        samples[0],
        samples[samples.len() - 1],
        samples[samples.len() / 2],
        node_counts[0],
        node_counts[node_counts.len() - 1],
    );
    eprintln!(
        "MESH_PERF metric=settings_page_switch_ms value={:.3}",
        samples[samples.len() / 2].as_secs_f64() * 1000.0
    );
}

#[test]
fn settings_nav_item_answers_with_one_tooltip_for_the_whole_row() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);

    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let tree = settings.last_tree.as_ref().expect("rendered settings tree");
    let nav_item = first_node_with_class_token(tree, "settings-nav-item").expect("nav item");
    let title = first_node_with_class_token(nav_item, "nav-title").expect("nav title");
    let subtitle = first_node_with_class_token(nav_item, "nav-subtitle").expect("nav subtitle");
    let icon = first_node_by_tag(nav_item, "icon").expect("nav icon");

    for (label, node) in [("title", title), ("subtitle", subtitle), ("icon", icon)] {
        let key = node.mesh_key().expect("mesh key").to_owned();
        assert_eq!(
            mesh_core_interaction::find_tooltip_text_by_key(tree, &key).as_deref(),
            Some("Open the Wi-Fi page"),
            "hovering the nav item's {label} should answer with the row's tooltip"
        );
    }
}
