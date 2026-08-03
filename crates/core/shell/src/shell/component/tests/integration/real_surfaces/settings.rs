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
