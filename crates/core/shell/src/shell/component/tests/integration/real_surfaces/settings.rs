use super::*;

#[test]
fn settings_audio_static_import_keeps_unrelated_page_generations_stable() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../modules/frontend/settings/src/components/audio-page.mesh"
    ));
    let direct = source.replace(
        "local audio_ok, audio = pcall(require, \"mesh.audio@>=1.0\")\nif not audio_ok then audio = nil end",
        "local audio = require(\"mesh.audio@>=1.0\")",
    );
    assert_ne!(direct, source);
    std::sync::Arc::make_mut(&mut settings.frontend_catalog)
        .modules
        .get_mut("@mesh/settings")
        .unwrap()
        .compiled
        .local_components
        .insert("AudioPage".into(), parse_component(&direct).unwrap());
    let extent = SurfaceExtent::unpadded(920, 900);
    let mut buffer = PixelBuffer::new(920, 900);
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    settings
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let update = |settings: &mut FrontendSurfaceComponent, percent| {
        settings
            .handle_service_event(&ServiceEvent::Updated {
                service: "mesh.audio".into(),
                source_module: "@mesh/pipewire-audio".into(),
                payload: serde_json::json!({"percent": percent, "muted": false}),
            })
            .unwrap();
    };
    update(&mut settings, 10);
    settings
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    for _ in 0..2 {
        settings.invalidate_script_state();
        settings
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
    }
    let warm_hits = settings.component_memo_hit_count();
    settings.invalidate_script_state();
    settings
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let baseline_hits = settings.component_memo_hit_count() - warm_hits;
    assert!(baseline_hits > 0);
    let before = {
        let runtimes = settings.runtimes.lock().unwrap();
        assert!(
            runtimes
                .iter()
                .any(|(key, runtime)| key.contains("AudioPage")
                    && runtime.script_ctx.has_tracked_fields_for_service("audio")),
            "audio page must observe the imported service"
        );
        runtimes
            .iter()
            .filter(|(key, _)| key.contains("/local:") && !key.contains("AudioPage"))
            .map(|(key, runtime)| {
                (
                    key.to_string(),
                    runtime.script_ctx.state().mutation_generation(),
                )
            })
            .collect::<std::collections::HashMap<_, _>>()
    };
    assert!(
        before.keys().any(|key| key.contains("AppearancePage")),
        "{before:?}"
    );
    update(&mut settings, 20);
    let runtimes = settings.runtimes.lock().unwrap();
    for (key, generation) in before {
        assert_eq!(
            runtimes
                .get(key.as_str())
                .unwrap()
                .script_ctx
                .state()
                .mutation_generation(),
            generation,
            "{key}"
        );
    }
    drop(runtimes);
    let hits = settings.component_memo_hit_count();
    settings.invalidate_script_state();
    settings
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    assert!(
        settings.component_memo_hit_count() - hits >= baseline_hits.saturating_sub(1),
        "unrelated pages should reuse their memoized trees; baseline {baseline_hits}, after audio {}",
        settings.component_memo_hit_count() - hits
    );
}

#[test]
fn settings_page_title_keeps_its_full_line_box() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);

    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
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
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAudio", &[])
        .unwrap();
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
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
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showBluetooth", &[])
        .unwrap();
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
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
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
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
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
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

fn count_class_token(node: &WidgetNode, token: &str) -> usize {
    usize::from(
        node.attributes
            .get(Prop::Class)
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
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    seed_large_settings_resource_catalog(&mut settings);
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
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
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    seed_large_settings_resource_catalog(&mut settings);
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
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
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
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

#[test]
fn settings_nav_item_answers_with_one_tooltip_for_the_whole_row() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);

    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let tree = settings.last_tree.as_ref().expect("rendered settings tree");
    let nav_item = first_node_with_class_token(tree, "settings-nav-item").expect("nav item");
    let title = first_node_with_class_token(nav_item, "nav-title").expect("nav title");
    let icon = first_node_by_tag(nav_item, "icon").expect("nav icon");

    for (label, node) in [("title", title), ("icon", icon)] {
        let key = node.mesh_key().expect("mesh key").to_owned();
        assert_eq!(
            mesh_core_interaction::find_tooltip_text_by_key(tree, &key).as_deref(),
            Some("Open the Wi-Fi page"),
            "hovering the nav item's {label} should answer with the row's tooltip"
        );
    }
}

#[test]
fn settings_fits_a_narrower_configure_and_keeps_requesting_its_size() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let (width, height) = (695, 700);
    let mut buffer = PixelBuffer::new(width, height);
    settings
        .paint_raster(
            &theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();

    let tree = settings.last_tree.as_ref().expect("rendered settings tree");
    assert_eq!(tree.layout.width, width as f32);
    let pop_out = first_node_with_class_token(tree, "pop-out-button").expect("pop-out button");
    assert!(
        pop_out.layout.x + pop_out.layout.width <= width as f32,
        "the header actions must stay inside a configure narrower than 920px: {:?}",
        pop_out.layout
    );
    assert_eq!(
        settings.requested_layout_size(),
        (920, 700),
        "the surface keeps requesting its declared size so a wider output gets it back"
    );
}

#[test]
fn settings_navigation_classes_and_language_options_derive_from_template_state() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    let active_handler = |settings: &FrontendSurfaceComponent| {
        let tree = settings.last_tree.as_ref().expect("settings tree");
        let selected = selected_nav_tabs(tree);
        assert_eq!(selected.len(), 1);
        selected[0]
            .event_handlers
            .get("activate")
            .cloned()
            .expect("selected nav tab handler")
    };
    assert!(active_handler(&settings).contains("showWifi"));

    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAudio", &[])
        .unwrap();
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    assert!(active_handler(&settings).contains("showAudio"));

    let tree = settings.last_tree.as_ref().expect("settings tree");
    assert_eq!(
        count_class_token(tree, "glass-button-active"),
        1,
        "exactly one of the English/Slovak options is marked active"
    );
}

#[test]
fn settings_pages_render_their_template_conditionals_without_errors() {
    let theme = default_theme();
    let mut settings =
        real_frontend_module_component("@mesh/settings", super::super::debug::settings_catalog());
    let mut buffer = PixelBuffer::new(920, 900);
    for handler in [
        "showWifi",
        "showAudio",
        "showBluetooth",
        "showAppearance",
        "showDevice",
        "showAdvanced",
    ] {
        settings
            .call_namespaced_handler(&format!("__mesh_embed__::@mesh/settings::{handler}"), &[])
            .unwrap();
        settings
            .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
    }
    let diagnostics = settings.diagnostics.as_ref().expect("diagnostics handle");
    let script_errors: Vec<_> = diagnostics
        .active_issues()
        .into_iter()
        .filter(|issue| issue.message.contains("script error"))
        .map(|issue| issue.message)
        .collect();
    assert!(
        script_errors.is_empty(),
        "every page's {{#if}} / class: / style: bindings should evaluate: {script_errors:#?}"
    );
    let tree = settings.last_tree.as_ref().expect("settings tree");
    assert!(
        count_class_token(tree, "settings-empty-state")
            + count_class_token(tree, "settings-prop-card")
            > 0,
        "the advanced page shows either its rows or their empty states"
    );
}

#[test]
fn appearance_theme_cards_stack_their_preview_above_the_label() {
    // Shipped themes lay buttons out as rows; the card must still give its
    // preview the full card width rather than squeezing it beside the label.
    let mut theme = default_theme();
    theme
        .defaults_mut()
        .components
        .entry("button".into())
        .or_default()
        .insert("flex-direction".into(), "row".into());
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let tree = settings
        .last_tree
        .as_ref()
        .expect("rendered appearance tree");
    let card = first_node_with_class_token(tree, "theme-option").expect("theme card");
    let preview = first_node_with_class_token(card, "theme-preview").expect("theme preview");
    let copy = first_node_with_class_token(card, "theme-option-copy").expect("theme copy");

    assert!(
        preview.layout.width + 1.0 >= copy.layout.width && preview.layout.width > 100.0,
        "preview {:?} should span the card like its label {:?}",
        preview.layout,
        copy.layout
    );
    assert!(
        copy.layout.y >= preview.layout.y + preview.layout.height,
        "label {:?} should sit below the preview {:?}",
        copy.layout,
        preview.layout
    );
}

#[test]
fn clicking_a_theme_card_repaints_settings_in_the_new_theme_in_one_frame() {
    let theme = settings_material_theme(&default_theme());
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);
    let extent = SurfaceExtent::unpadded(920, 900);
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    for _ in 0..8 {
        settings
            .paint_raster(&theme, extent, &mut buffer, 1.0)
            .unwrap();
    }

    let (x, y) = {
        let tree = settings.last_tree.as_ref().unwrap();
        let card = first_node_with_class_token(tree, "theme-option").expect("theme card");
        (
            card.layout.x + card.layout.width / 2.0,
            card.layout.y + card.layout.height / 2.0,
        )
    };
    settings
        .handle_input(&theme, 920, 900, ComponentInput::PointerMove { x, y })
        .unwrap();
    settings
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    for pressed in [true, false] {
        settings
            .handle_input(
                &theme,
                920,
                900,
                ComponentInput::PointerButton {
                    x,
                    y,
                    button: 0x110,
                    pressed,
                },
            )
            .unwrap();
    }

    // The shell applies the picked theme and broadcasts it before the next
    // frame of the clicked surface.
    let mut picked = theme.clone();
    picked.set_token(
        "color.surface-container-high",
        mesh_core_theme::TokenValue::String("#c00000".into()),
        mesh_core_theme::ThemeProvenance::BaseRecovery,
    );
    picked.set_token(
        "color.surface-container-low",
        mesh_core_theme::TokenValue::String("#c00000".into()),
        mesh_core_theme::ThemeProvenance::BaseRecovery,
    );
    let picked = settings_material_theme(&picked);
    settings.theme_changed().unwrap();
    settings
        .paint_raster(&picked, extent, &mut buffer, 1.0)
        .unwrap();

    let tree = settings.last_tree.as_ref().unwrap();
    let shell = first_node_with_class_token(tree, "settings-shell").expect("settings shell");
    let mesh_core_elements::style::BackgroundPaint::LinearGradient(gradient) =
        &shell.computed_style.background_paint
    else {
        panic!("settings surface must keep its theme gradient")
    };
    for color in [gradient.from, gradient.to] {
        assert_eq!((color.r, color.g, color.b), (0xc0, 0, 0));
        assert!(color.a > 0 && color.a < 255);
    }
    // BGRA, at a point of the shell's right edge that no child covers.
    let (px, py) = (
        (shell.layout.x + shell.layout.width - 4.0) as u32,
        (shell.layout.y + shell.layout.height / 2.0) as u32,
    );
    let offset = ((py * buffer.width() + px) * 4) as usize;
    let pixel = &buffer.data()[offset..offset + 4];
    assert!(
        pixel[2] > 0 && pixel[0] == 0 && pixel[1] == 0 && pixel[3] > 0,
        "the frame after the click paints the picked palette: {pixel:?}"
    );
}

pub(super) fn settings_material_theme(pack: &mesh_core_theme::Theme) -> mesh_core_theme::Theme {
    use mesh_core_theme::{Theme, ThemeModule, ThemeModuleLayer};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let material: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("modules/libraries/glass-material/module.json"))
            .unwrap(),
    )
    .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("modules/frontend/settings/module.json")).unwrap(),
    )
    .unwrap();
    let families = manifest["mesh"]["uses"]["modules"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|id| id.as_str() == "@mesh/glass-material")
        .cloned()
        .collect::<Vec<_>>();
    Theme::compose_layers(
        &default_theme(),
        pack,
        &pack.id,
        "dark",
        vec![
            ThemeModuleLayer {
                module_id: "@mesh/glass-material".into(),
                module: serde_json::from_value(material["mesh"]["theme"].clone()).unwrap(),
                families: Vec::new(),
            },
            ThemeModuleLayer {
                module_id: "@mesh/settings".into(),
                module: ThemeModule::default(),
                families: families.clone(),
            },
        ],
        &HashMap::new(),
    )
    .unwrap()
}

#[test]
fn settings_glass_surfaces_follow_the_active_theme_palette() {
    use mesh_core_theme::ThemeSourceHandle;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let mut surfaces = Vec::new();
    for (name, stylesheet, blur) in [
        ("catppuccin-mocha", "theme.css", 26.0),
        ("liquid-glass", "theme.css", 4.0),
        ("frutiger-aero", "light.css", 30.0),
        ("frutiger-aero", "dark.css", 28.0),
    ] {
        let pack = mesh_core_theme::load_theme_from_source(
            &ThemeSourceHandle::new(root.join("modules/themes").join(name), stylesheet).unwrap(),
            name,
            name,
        )
        .unwrap();
        let theme = settings_material_theme(&pack);
        let mut settings =
            real_frontend_module_component("@mesh/settings", audio_network_catalog());
        let mut buffer = PixelBuffer::new(920, 900);
        settings
            .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
        let tree = settings.last_tree.as_ref().unwrap();
        let shell = first_node_with_class_token(tree, "settings-shell").unwrap();
        let sidebar = first_node_with_class_token(tree, "settings-sidebar").unwrap();
        let card = first_node_with_class_token(tree, "settings-empty-state").unwrap();
        if name == "liquid-glass" {
            let mesh_core_elements::style::BackgroundPaint::LinearGradient(gradient) =
                &shell.computed_style.background_paint
            else {
                panic!("liquid-glass settings should have a translucent light panel");
            };
            assert!(gradient.from.r > 200 && gradient.from.a > 200 && gradient.from.a < 255);
            let title = first_node_with_class_token(tree, "sidebar-title").unwrap();
            assert!(title.computed_style.color.r < 100, "settings text must be dark");
        }
        assert_eq!(shell.computed_style.background_color.a, 0);
        assert_ne!(
            shell.computed_style.background_paint,
            mesh_core_elements::style::BackgroundPaint::None
        );
        assert_eq!(shell.computed_style.backdrop_filter.blur_radius, blur);
        assert!(
            sidebar.computed_style.background_color.a > 0
                && sidebar.computed_style.background_color.a < 255
        );
        assert!(
            card.computed_style.background_color.a > 0
                && card.computed_style.background_color.a < 255
        );
        surfaces.push(shell.computed_style.background_paint.clone());
        if let Some(dir) = std::env::var_os("MESH_SETTINGS_PREVIEW_DIR") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(format!("settings-{name}-{stylesheet}-920x900.bgra")),
                buffer.data(),
            )
            .unwrap();
        }
    }
    for pair in surfaces.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "settings must follow the selected palette"
        );
    }
}

fn is_nav_tab(node: &WidgetNode) -> bool {
    node.attributes.get(Prop::Class).is_some_and(|class| {
        class
            .split_whitespace()
            .any(|part| part == "settings-nav-item")
    })
}

fn selected_nav_tabs(tree: &WidgetNode) -> Vec<&WidgetNode> {
    fn collect<'a>(node: &'a WidgetNode, out: &mut Vec<&'a WidgetNode>) {
        if is_nav_tab(node) && node.accessibility.state.selected {
            out.push(node);
        }
        for child in &node.children {
            collect(child, out);
        }
    }
    let mut out = Vec::new();
    collect(tree, &mut out);
    out
}

/// The navigation is one Tab stop: arrows move between its tabs and show
/// each page, Tab leaves it for the page, and Shift+Tab comes back to the
/// page's own tab.
#[test]
fn settings_navigation_is_one_tab_stop_with_arrow_keys_inside() {
    let theme = default_theme();
    let mut settings = real_frontend_module_component("@mesh/settings", audio_network_catalog());
    let mut buffer = PixelBuffer::new(920, 900);
    let mut paint = |settings: &mut FrontendSurfaceComponent| {
        settings
            .paint_raster(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
    };
    let key_theme = default_theme();
    let key = |settings: &mut FrontendSurfaceComponent, key: &str, shift: bool| {
        settings
            .handle_input(
                &key_theme,
                920,
                900,
                ComponentInput::KeyPressed {
                    key: key.into(),
                    modifiers: KeyModifiers {
                        shift,
                        ..Default::default()
                    },
                },
            )
            .unwrap();
    };
    let focused_class = |settings: &FrontendSurfaceComponent| {
        let tree = settings.last_tree.as_ref().unwrap();
        let key = settings.focused_key.as_deref().expect("focus");
        let node = mesh_core_interaction::find_node_by_key(tree, key).unwrap();
        (
            node.attributes
                .get(Prop::Class)
                .cloned()
                .unwrap_or_default(),
            node.tag.clone(),
        )
    };
    let active_page = |settings: &FrontendSurfaceComponent| {
        let tree = settings.last_tree.as_ref().unwrap();
        let selected = selected_nav_tabs(tree);
        assert_eq!(selected.len(), 1);
        selected[0].event_handlers["activate"].clone()
    };
    paint(&mut settings);
    let tree = settings.last_tree.as_ref().unwrap();
    let traversal = mesh_core_interaction::collect_focus_traversal(tree);
    let nav_stops = traversal
        .iter()
        .filter(|key| {
            mesh_core_interaction::find_node_by_key(tree, key).is_some_and(|node| is_nav_tab(node))
        })
        .count();
    assert_eq!(
        nav_stops, 1,
        "the navigation is one Tab stop: {traversal:?}"
    );

    let wifi = selected_nav_tabs(tree)[0].mesh_key().unwrap().to_owned();
    settings.focused_key = Some(wifi);
    key(&mut settings, "ArrowDown", false);
    paint(&mut settings);
    assert!(active_page(&settings).contains("showAudio"));
    assert!(focused_class(&settings).0.contains("settings-nav-item"));
    let audio_tab = settings.focused_key.clone();

    key(&mut settings, "Tab", false);
    paint(&mut settings);
    let (class, tag) = focused_class(&settings);
    assert!(
        !class.contains("settings-nav-item"),
        "Tab leaves the navigation for the page, got {tag} {class:?}"
    );

    key(&mut settings, "Tab", true);
    paint(&mut settings);
    assert_eq!(
        settings.focused_key, audio_tab,
        "Shift+Tab returns to the shown page's tab"
    );

    key(&mut settings, "End", false);
    paint(&mut settings);
    assert!(active_page(&settings).contains("showAdvanced"));
    key(&mut settings, "ArrowDown", false);
    paint(&mut settings);
    assert!(active_page(&settings).contains("showWifi"), "arrows wrap");
}
