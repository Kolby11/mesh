use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::catalog::FrontendCatalog;
mod cpu_timing;
mod live_replay;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_frontend::compile_frontend_module;
use mesh_core_theme::{Theme, ThemeModule, ThemeModuleLayer, ThemeSourceHandle};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn theme() -> Theme {
    let pack = mesh_core_theme::load_theme_from_source(
        &ThemeSourceHandle::new(root().join("modules/themes/violet-glass"), "theme.css").unwrap(),
        "violet-glass",
        "Violet Glass",
    )
    .unwrap();
    let material: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("modules/libraries/glass-material/module.json"))
            .unwrap(),
    )
    .unwrap();
    let material: ThemeModule = serde_json::from_value(material["mesh"]["theme"].clone()).unwrap();
    let mut layers = vec![ThemeModuleLayer {
        module_id: "@mesh/glass-material".into(),
        module: material,
        families: Vec::new(),
    }];
    for module in ["glass-bar", "glass-widgets", "glass-dock"] {
        layers.push(ThemeModuleLayer {
            module_id: format!("@mesh/{module}"),
            module: ThemeModule::default(),
            families: vec!["@mesh/glass-material".into()],
        });
    }
    Theme::compose_layers(
        &default_theme(),
        &pack,
        "@mesh/violet-glass",
        "dark",
        layers,
        &HashMap::new(),
    )
    .unwrap()
}

const GLASS_MODULES: [&str; 8] = [
    "glass-bar",
    "glass-widgets",
    "glass-dock",
    "glass-dashboard",
    "glass-wallpapers",
    "wallpaper-carousel",
    "profile-switcher",
    "profile-menu",
];

/// Compiled glass modules, contracts and locale, shared by every surface the
/// tests mount. Building it compiles every module, so workloads that time a
/// surface open prepare it once outside the measured region.
struct GlassCatalog {
    entries: HashMap<String, FrontendCatalogEntry>,
    interfaces: InterfaceCatalog,
    locale: mesh_core_locale::LocaleEngine,
}

impl GlassCatalog {
    fn new() -> Self {
        let mut entries = HashMap::new();
        for module in GLASS_MODULES {
            let dir = root().join("modules/frontend").join(module);
            let manifest = mesh_core_module::manifest::load_canonical_manifest(&dir)
                .unwrap()
                .manifest;
            let compiled = compile_frontend_module(&manifest, &dir)
                .unwrap_or_else(|error| panic!("{module}: {error}"));
            entries.insert(
                format!("@mesh/{module}"),
                FrontendCatalogEntry {
                    module_dir: dir,
                    compiled: compiled.into(),
                },
            );
        }
        let mut interfaces = InterfaceCatalog::default();
        for interface in [
            "apps",
            "audio",
            "brightness",
            "media",
            "network",
            "notifications",
            "power",
            "recorder",
            "system",
            "visualizer",
            "wallpapers",
            "weather",
            "wm",
        ] {
            let path = root()
                .join("modules/interfaces")
                .join(interface)
                .join("module.json");
            let json: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let contract = &json["mesh"]["interface"];
            interfaces.register_contract(
                mesh_core_service::contract::parse_interface_contract(
                    contract["name"].as_str().unwrap(),
                    contract["version"].as_str().unwrap(),
                    &contract["contract"],
                )
                .unwrap(),
            );
            interfaces.register_provider(mesh_core_service::InterfaceProvider {
                interface: contract["name"].as_str().unwrap().into(),
                version: Some(contract["version"].as_str().unwrap().into()),
                base_module: None,
                provider_module: "@test/glass-services".into(),
                backend_name: interface.into(),
                priority: 1,
            });
        }
        interfaces.register_contract(
            mesh_core_service::contract::parse_interface_contract(
                "mesh.packages",
                "1.0",
                &serde_json::json!({"state": [
                {"name":"profiles","type":"string[]"}, {"name":"active_profile","type":"string"}
            ], "methods": [{"name":"switch_profile", "args":[{"name":"profile_id","type":"string"}]}]}),
            )
            .unwrap(),
        );
        interfaces.register_contract(
            mesh_core_service::contract::parse_interface_contract(
                "mesh.theme",
                "1.0",
                &serde_json::json!({"state": [{"name":"backdrop_image","type":"string?"}],
                    "methods": [{"name":"set_backdrop_image", "args":[{"name":"path","type":"string"}]}]}),
            )
            .unwrap(),
        );
        let mut locale = mesh_core_locale::LocaleEngine::new("en");
        for module in [
            "glass-bar",
            "glass-dock",
            "glass-dashboard",
            "glass-wallpapers",
            "wallpaper-carousel",
        ] {
            let path = root()
                .join("modules/frontend")
                .join(module)
                .join("config/i18n/en.json");
            let catalog: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            locale.load_module_catalog(
                &format!("@mesh/{module}"),
                mesh_core_locale::compile_catalog("en", &catalog),
            );
        }
        Self {
            entries,
            interfaces,
            locale,
        }
    }

    fn mount(&self, name: &str) -> FrontendSurfaceComponent {
        let id = format!("@mesh/{name}");
        let entry = self.entries[&id].clone();
        let catalog = FrontendCatalog {
            modules: self.entries.clone(),
            ..Default::default()
        };
        let mut component = FrontendSurfaceComponent::new_for_test(
            (*entry.compiled).clone(),
            entry.module_dir,
            catalog,
            self.interfaces.clone(),
            test_settings_store(),
        );
        component.locale = self.locale.snapshot();
        component
            .mount(ComponentContext {
                component_id: id.clone(),
                surface_id: id.clone(),
                diagnostics: Diagnostics::new(id),
            })
            .unwrap();
        component.visible = true;
        component
    }
}

fn host(name: &str) -> FrontendSurfaceComponent {
    GlassCatalog::new().mount(name)
}

fn paint(component: &mut FrontendSurfaceComponent, width: u32, height: u32) -> PixelBuffer {
    let mut pixels = PixelBuffer::new(width, height);
    component
        .paint_raster(
            &theme(),
            SurfaceExtent::unpadded(width, height),
            &mut pixels,
            1.0,
        )
        .unwrap();
    let text = rendered_text(component);
    assert!(
        !text
            .iter()
            .any(|text| text.contains("frontend runtime") || text.contains("script error")),
        "{text:?}"
    );
    pixels
}

fn reference(component: &FrontendSurfaceComponent, name: &str) -> String {
    first_node_with_ref(component.last_tree.as_ref().unwrap(), name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .mesh_key()
        .unwrap()
        .to_owned()
}

fn press(component: &mut FrontendSurfaceComponent, name: &str, width: u32, height: u32) {
    component.focused_key = Some(reference(component, name));
    component
        .handle_input(
            &theme(),
            width,
            height,
            ComponentInput::KeyPressed {
                key: "Enter".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    paint(component, width, height);
}

fn update(component: &mut FrontendSurfaceComponent, service: &str, payload: serde_json::Value) {
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: service.into(),
            source_module: "@test/glass-services".into(),
            payload,
        })
        .unwrap();
}

#[test]
fn liquid_glass_popup_anchor_geometry_stays_near_triggers() {
    let catalog = GlassCatalog::new();
    for (name, trigger, height) in [
        ("glass-bar", "glassControls", 58),
        ("glass-dock", "glassLauncher", 114),
    ] {
        let mut component = catalog.mount(name);
        paint(&mut component, 2880, height);
        press(&mut component, trigger, 2880, height);
        let requests = component.child_surface_requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        let trigger_node =
            first_node_with_ref(component.last_tree.as_ref().unwrap(), trigger).unwrap();
        assert_eq!(
            request.anchor_rect,
            (
                trigger_node.layout.x.floor() as i32,
                trigger_node.layout.y.floor() as i32,
                trigger_node.layout.width.ceil() as i32,
                trigger_node.layout.height.ceil() as i32,
            )
        );
        let (left, top, right, bottom) = request.content_padding;
        let config = mesh_core_presentation::PopupConfig {
            parent_surface_id: name.into(),
            placement: mesh_core_presentation::PopupPlacement {
                anchor_rect: request.anchor_rect,
                size: (
                    request.content_size.0 + left + right,
                    request.content_size.1 + top + bottom,
                ),
                ..Default::default()
            },
            padding: mesh_core_presentation::SurfacePadding {
                left,
                top,
                right,
                bottom,
            },
            input_passthrough: false,
            grab: false,
            grab_identity: None,
        };
        let geometry = config.window_geometry();
        assert_eq!(
            (geometry.width, geometry.height),
            request.content_size,
            "{name}: paint reserves must not enlarge constrained popup geometry"
        );
        assert_eq!((geometry.x, geometry.y), (left, top));
    }
}

#[test]
fn liquid_glass_calendar_and_controls_are_promoted_local_popovers() {
    let mut component = host("glass-bar");
    paint(&mut component, 1920, 100);
    assert!(component.child_surface_requests().is_empty());
    press(&mut component, "glassCalendar", 1920, 100);
    let requests = component.child_surface_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].content_size.0 >= 360);
    assert!(requests[0].content_size.1 >= 300);
    component
        .handle_input(
            &theme(),
            1920,
            100,
            ComponentInput::KeyPressed {
                key: "Escape".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    paint(&mut component, 1920, 100);
    assert!(component.child_surface_requests().is_empty());
    update(
        &mut component,
        "mesh.audio",
        serde_json::json!({"available":true,"percent":50,"muted":false}),
    );
    update(
        &mut component,
        "mesh.brightness",
        serde_json::json!({"available":true,"level":70}),
    );
    press(&mut component, "glassControls", 1920, 100);
    assert_eq!(component.child_surface_requests().len(), 1);
    preview(&mut component, 1920, 100, "controls");
    for (reference_name, interface, command, field) in [
        ("glassVolume", "mesh.audio", "set_volume", "percent"),
        ("glassBrightness", "mesh.brightness", "set", "level"),
    ] {
        let key = reference(&component, reference_name);
        let tree = component.last_tree.take().unwrap();
        let requests = component
            .call_node_handler(&tree, &key, "change", &[serde_json::json!(37)])
            .unwrap();
        component.last_tree = Some(tree);
        assert!(
            requests
                .iter()
                .any(|request| service_request_parts(request).is_some_and(
                    |(service, method, args)| service == interface
                        && method == command
                        && args[field] == 37
                )),
            "{requests:?}"
        );
    }
}

#[test]
fn liquid_glass_launcher_keeps_results_viewport_with_many_apps() {
    let mut component = host("glass-dock");
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    component.motion_policy.reduced_motion = true;
    paint(&mut component, 1920, 180);
    let installed: Vec<_> = (0..100)
        .map(|index| {
            serde_json::json!({
                "id": format!("app-{index}"), "name": format!("Application {index}"),
                "icon": "", "pinned": false, "running": false, "focused": false, "windows": 0
            })
        })
        .collect();
    update(
        &mut component,
        "mesh.apps",
        serde_json::json!({"available": true, "installed": installed, "apps": []}),
    );
    paint(&mut component, 1920, 180);
    press(&mut component, "glassLauncher", 1920, 180);
    paint(&mut component, 1920, 180);
    let tree = component.last_tree.as_ref().unwrap();
    let shell = first_node_by_class(tree, "launcher-shell").unwrap();
    let results = first_node_by_class(tree, "results").unwrap();
    let grid = first_node_by_class(tree, "app-grid").unwrap();
    assert_eq!(results.layout.height, 320.0);
    assert!(
        grid.layout.height > results.layout.height * 2.0,
        "{:?}",
        grid.layout
    );
    assert!(shell.layout.height < 600.0, "{:?}", shell.layout);
    let results_layout = results.layout;
    let scroll_x = results.layout.x - shell.layout.x + 40.0;
    let scroll_y = results.layout.y - shell.layout.y + 40.0;
    let request = component
        .child_surface_requests()
        .into_iter()
        .next()
        .unwrap();
    assert!(request.content_size.1 < 600, "{request:?}");
    assert!(
        request.content_padding.1 < request.content_size.1
            && request.content_padding.3 < request.content_size.1,
        "clipped rows must not pad the popup: {request:?}"
    );
    let (pad_left, pad_top, pad_right, pad_bottom) = request.content_padding;
    let popup_size = (
        request.content_size.0 + pad_left + pad_right,
        request.content_size.1 + pad_top + pad_bottom,
    );
    let content_offset = (pad_left, pad_top);
    let input_offset = (pad_left as f32, pad_top as f32);
    let scroll_x = scroll_x + input_offset.0;
    let scroll_y = scroll_y + input_offset.1;
    let viewport = (
        results.layout.x - shell.layout.x + input_offset.0,
        results.layout.y - shell.layout.y + input_offset.1,
        results.layout.width,
        results.layout.height,
    );
    let initial_max = results.resolved_scroll_metrics().max_y;
    component
        .handle_child_surface_input(
            &request.node_key,
            &theme(),
            popup_size.0,
            popup_size.1,
            input_offset,
            ComponentInput::PointerMove {
                x: scroll_x,
                y: scroll_y,
            },
        )
        .unwrap();
    paint(&mut component, 1920, 180);
    let mut popup_pixels = PixelBuffer::new(popup_size.0, popup_size.1);
    component
        .paint_child_surface_raster(
            &request.node_key,
            &mut popup_pixels,
            1.0,
            content_offset,
            false,
        )
        .unwrap();
    let baseline = popup_pixels.data().to_vec();
    let mut previous = 0.0;
    for step in 0..24 {
        let dy = if step < 12 { -28.0 } else { 28.0 };
        component
            .handle_child_surface_input(
                &request.node_key,
                &theme(),
                popup_size.0,
                popup_size.1,
                input_offset,
                ComponentInput::TwoFingerScroll {
                    x: scroll_x,
                    y: scroll_y,
                    dx: 0.0,
                    dy,
                },
            )
            .unwrap();
        paint(&mut component, 1920, 180);
        let tree = component.last_tree.as_ref().unwrap();
        let results = first_node_by_class(tree, "results").unwrap();
        let metrics = results.resolved_scroll_metrics();
        assert_eq!(results.layout, results_layout);
        assert_eq!(metrics.max_y, initial_max, "step {step}");
        let expected = (previous - dy).clamp(0.0, initial_max);
        assert_eq!(metrics.y, expected, "step {step}");
        previous = metrics.y;
        assert_eq!(
            component.child_surface_requests()[0].content_size,
            request.content_size
        );
        assert_eq!(
            component.child_surface_requests()[0].content_padding,
            request.content_padding,
            "scrolling must not reposition the popup buffer, step {step}"
        );
        component
            .paint_child_surface_raster(
                &request.node_key,
                &mut popup_pixels,
                1.0,
                content_offset,
                false,
            )
            .unwrap();
        for y in 0..popup_size.1 {
            for x in 0..popup_size.0 {
                if x as f32 >= viewport.0
                    && (x as f32) < viewport.0 + viewport.2
                    && y as f32 >= viewport.1
                    && (y as f32) < viewport.1 + viewport.3
                {
                    continue;
                }
                let index = ((y * popup_size.0 + x) * 4) as usize;
                assert_eq!(
                    &popup_pixels.data()[index..index + 4],
                    &baseline[index..index + 4],
                    "outside viewport at {x},{y}, step {step}"
                );
            }
        }
    }
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":false}}));
    component.motion_policy.reduced_motion = false;
    for _ in 0..3 {
        component
            .handle_child_surface_input(
                &request.node_key,
                &theme(),
                popup_size.0,
                popup_size.1,
                input_offset,
                ComponentInput::TwoFingerScroll {
                    x: scroll_x,
                    y: scroll_y,
                    dx: 0.0,
                    dy: -24.0,
                },
            )
            .unwrap();
    }
    paint(&mut component, 1920, 180);
    let tree = component.last_tree.as_ref().unwrap();
    let results = first_node_by_class(tree, "results").unwrap();
    let direct = results.resolved_scroll_metrics().y;
    let inertia = component.scroll_inertia[&results.id];
    component.advance_scroll_inertia(inertia.last_input + Duration::from_millis(60));
    paint(&mut component, 1920, 180);
    let results = first_node_by_class(component.last_tree.as_ref().unwrap(), "results").unwrap();
    assert!(
        results.resolved_scroll_metrics().y > direct,
        "momentum should keep moving the popup"
    );
    let mut pixels = PixelBuffer::new(popup_size.0, popup_size.1);
    assert!(
        component
            .paint_child_surface_raster(&request.node_key, &mut pixels, 1.0, content_offset, false)
            .unwrap()
    );
}

#[test]
fn liquid_glass_dock_search_and_neighbor_magnification_work() {
    let mut component = host("glass-dock");
    update(
        &mut component,
        "mesh.apps",
        serde_json::json!({"available":true,"apps":[
            {"id":"browser","name":"Browser","icon":"","pinned":true,"running":true,"focused":true,"windows":1},
            {"id":"terminal","name":"Terminal","icon":"","pinned":true,"running":false,"focused":false,"windows":0}
        ]}),
    );
    paint(&mut component, 1920, 180);
    let browser = reference(&component, "browser");
    let (left, top, right, bottom) =
        find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &browser, 0.0, 0.0).unwrap();
    component
        .handle_input(
            &theme(),
            1920,
            180,
            ComponentInput::PointerMove {
                x: (left + right) / 2.0,
                y: (top + bottom) / 2.0,
            },
        )
        .unwrap();
    paint(&mut component, 1920, 180);
    assert!(first_node_by_class(component.last_tree.as_ref().unwrap(), "tile-neighbor").is_some());
    press(&mut component, "glassLauncher", 1920, 180);
    assert_eq!(component.child_surface_requests().len(), 1);
    preview(&mut component, 1920, 180, "launcher");
    let tree = component.last_tree.as_ref().unwrap();
    let search = first_node_by_class(tree, "search")
        .expect("search input")
        .mesh_key()
        .unwrap()
        .to_owned();
    component.focused_key = Some(search);
    component
        .handle_input(
            &theme(),
            1920,
            180,
            ComponentInput::TextInput {
                text: "term".into(),
            },
        )
        .unwrap();
    paint(&mut component, 1920, 180);
    let texts = rendered_text(&component);
    assert!(texts.iter().any(|text| text == "Terminal"));
    assert!(!texts.iter().any(|text| text == "Browser"));
}

#[test]
fn wallpaper_picker_keeps_its_size_and_centres_the_focused_card() {
    for (width, height) in [(1920, 1080), (1280, 800)] {
        picker_geometry_is_stable(width, height);
    }
}

fn picker_geometry_is_stable(width: u32, height: u32) {
    let mut component = host("glass-wallpapers");
    let wallpapers: Vec<_> = ["forest", "tree", "lake", "dusk", "night"]
        .iter()
        .map(|name| {
            serde_json::json!({
                "path": format!("/walls/{name}.png"),
                "name": name,
                "thumbnail": "",
                "color": "#334455",
            })
        })
        .collect();
    update(
        &mut component,
        "mesh.wallpapers",
        serde_json::json!({"available": true, "directory": "/walls", "wallpapers": wallpapers}),
    );
    let mut geometry = Vec::new();
    for step in 0..7 {
        paint(&mut component, width, height);
        let tree = component.last_tree.as_ref().unwrap();
        let picker = first_node_by_class(tree, "picker").unwrap().layout;
        let focused = first_node_by_class(tree, "card-focused").unwrap().layout;
        geometry.push((step, picker, focused));
        let next = first_node_by_class(tree, "arrow-next")
            .unwrap()
            .mesh_key()
            .unwrap()
            .to_owned();
        component.focused_key = Some(next);
        component
            .handle_input(
                &theme(),
                width,
                height,
                ComponentInput::KeyPressed {
                    key: "Enter".into(),
                    modifiers: Default::default(),
                },
            )
            .unwrap();
    }
    let (_, picker, focused) = geometry[0];
    let picker_centre = picker.x + picker.width / 2.0;
    assert!(
        (focused.x + focused.width / 2.0 - picker_centre).abs() < 1.0,
        "{width}: focused card is centred: {focused:?} in {picker:?}"
    );
    for &(step, other_picker, other_focused) in &geometry[1..] {
        assert_eq!(
            (
                other_picker.x,
                other_picker.y,
                other_picker.width,
                other_picker.height
            ),
            (picker.x, picker.y, picker.width, picker.height),
            "{width}: picker geometry changed at step {step}"
        );
        assert_eq!(
            (other_focused.x, other_focused.width),
            (focused.x, focused.width),
            "{width}: focused card moved at step {step}"
        );
    }
}

fn carousel_cards(node: &WidgetNode, cards: &mut Vec<WidgetNode>) {
    if first_node_by_class(node, "card").is_some_and(|card| std::ptr::eq(card, node)) {
        cards.push(node.clone());
        return;
    }
    for child in &node.children {
        carousel_cards(child, cards);
    }
}

fn carousel_state(component: &FrontendSurfaceComponent) -> (WidgetNode, Vec<WidgetNode>) {
    let tree = component.last_tree.as_ref().unwrap();
    let stage = first_node_by_class(tree, "stage").unwrap().clone();
    let mut cards = Vec::new();
    carousel_cards(&stage, &mut cards);
    (stage, cards)
}

fn card_label(card: &WidgetNode) -> String {
    card.attributes
        .get(Prop::AriaLabel)
        .cloned()
        .unwrap_or_default()
}

#[test]
fn wallpaper_carousel_centres_the_selection_and_slides_its_cards() {
    let (width, height) = (1920, 1080);
    let mut component = host("wallpaper-carousel");
    let wallpapers: Vec<_> = ["forest", "tree", "lake", "dusk", "night"]
        .iter()
        .map(|name| {
            serde_json::json!({
                "path": format!("/walls/{name}.png"),
                "name": name,
                "thumbnail": "",
                "panorama": "",
                "color": "",
            })
        })
        .collect();
    update(
        &mut component,
        "mesh.wallpapers",
        serde_json::json!({"available": true, "directory": "/walls", "wallpapers": wallpapers}),
    );
    paint(&mut component, width, height);
    let (stage, cards) = carousel_state(&component);
    // Five wallpapers: two visible cards each side plus one transparent buffer.
    assert_eq!(
        cards.len(),
        7,
        "{:?}",
        cards.iter().map(card_label).collect::<Vec<_>>()
    );
    let selected = first_node_by_class(&stage, "card-selected").unwrap();
    let centre = selected.layout.x
        + selected.computed_style.transform.translate_x
        + selected.layout.width / 2.0;
    assert!(
        (centre - (stage.layout.x + stage.layout.width / 2.0)).abs() < 1.0,
        "selected card is centred: {centre} in {:?}",
        stage.layout
    );
    assert!(card_label(selected).contains("forest"));
    let ranks: Vec<f32> = cards
        .iter()
        .map(|card| card.computed_style.transform.scale_x)
        .collect();
    assert!(ranks[3] > ranks[2] && ranks[2] > ranks[1], "{ranks:?}");

    // Clicking the next card selects it; its node slides into the centre
    // rather than being replaced.
    let next = cards[4].mesh_key().unwrap().to_owned();
    let start_x = cards[4].computed_style.transform.translate_x;
    let rest_x = cards[3].computed_style.transform.translate_x;
    component.focused_key = Some(next.clone());
    component
        .handle_input(
            &theme(),
            width,
            height,
            ComponentInput::KeyPressed {
                key: "Space".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    paint(&mut component, width, height);
    let (stage, cards) = carousel_state(&component);
    let selected = first_node_by_class(&stage, "card-selected").unwrap();
    assert_eq!(selected.mesh_key(), Some(next.as_str()));
    assert!(card_label(selected).contains("tree"));
    assert_eq!(cards.len(), 7);
    assert!(!component.transitions.is_empty(), "the strip must animate");
    let mid_x = selected.computed_style.transform.translate_x;
    assert!(
        (mid_x - start_x).abs() < (rest_x - start_x).abs() / 2.0,
        "first frame is near the old position: {start_x} -> {mid_x} (rest {rest_x})"
    );

    // Wrapping: four more steps forward lands back on the first wallpaper.
    for _ in 0..4 {
        let (_, cards) = carousel_state(&component);
        component.focused_key = Some(cards[4].mesh_key().unwrap().to_owned());
        component
            .handle_input(
                &theme(),
                width,
                height,
                ComponentInput::KeyPressed {
                    key: "Space".into(),
                    modifiers: Default::default(),
                },
            )
            .unwrap();
        paint(&mut component, width, height);
    }
    let (stage, _) = carousel_state(&component);
    assert!(card_label(first_node_by_class(&stage, "card-selected").unwrap()).contains("forest"));
}

fn carousel_selection(component: &FrontendSurfaceComponent) -> String {
    let (stage, _) = carousel_state(component);
    card_label(first_node_by_class(&stage, "card-selected").unwrap())
}

#[test]
fn wallpaper_carousel_is_a_top_widget_driven_by_the_keyboard() {
    let (width, height) = (1920, 1080);
    let mut component = host("wallpaper-carousel");
    let wallpapers: Vec<_> = (0..12)
        .map(|index| {
            serde_json::json!({
                "path": format!("/walls/w{index:02}.png"),
                "name": format!("w{index:02}"),
                "thumbnail": "",
                "panorama": "",
                "color": "",
            })
        })
        .collect();
    update(
        &mut component,
        "mesh.wallpapers",
        serde_json::json!({"available": true, "directory": "/walls", "wallpapers": wallpapers}),
    );
    paint(&mut component, width, height);

    // Content-sized rather than a full-screen overlay; layer-shell anchors
    // it to the top edge, so it fits the upper half of a 1080p output.
    let root = component.last_tree.as_ref().unwrap();
    let widget = first_node_by_class(root, "widget").unwrap().layout;
    assert!(
        widget.width < width as f32 && widget.height < height as f32 / 2.0,
        "widget is content-sized: {widget:?}"
    );

    // Surface keybinds reach the strip with nothing focused.
    let key = |component: &mut FrontendSurfaceComponent, name: &str| {
        component
            .handle_input(
                &theme(),
                width,
                height,
                ComponentInput::KeyPressed {
                    key: name.into(),
                    modifiers: Default::default(),
                },
            )
            .unwrap();
        paint(component, width, height);
    };
    key(&mut component, "ArrowRight");
    key(&mut component, "ArrowRight");
    assert!(carousel_selection(&component).contains("w02"));
    key(&mut component, "ArrowLeft");
    key(&mut component, "ArrowLeft");
    key(&mut component, "ArrowLeft");
    assert!(carousel_selection(&component).contains("w11"));
    // A page is the seven visible cards.
    key(&mut component, "PageDown");
    assert!(carousel_selection(&component).contains("w06"));
    key(&mut component, "PageUp");
    assert!(carousel_selection(&component).contains("w11"));
}

#[test]
fn liquid_glass_desktop_cards_fit_laptop_and_large_outputs() {
    for width in [640, 800, 960, 1200, 1440, 1920, 2880] {
        let mut component = host("glass-widgets");
        update(
            &mut component,
            "mesh.weather",
            serde_json::json!({"available":true,"location":"Bratislava","temperature":17,"feels_like":16,"unit":"C","condition":"cloudy","is_day":true,"hourly":[
                {"time":1800000000,"temperature":18,"condition":"clear"},
                {"time":1800003600,"temperature":19,"condition":"cloudy"},
                {"time":1800007200,"temperature":17,"condition":"rain"},
                {"time":1800010800,"temperature":16,"condition":"rain"}
            ]}),
        );
        paint(&mut component, width, 1000);
        if width == 1920 {
            preview(&mut component, width, 1000, "widgets");
        }
        let tree = component.last_tree.as_ref().unwrap();
        let media = first_node_by_class(tree, "slot").expect("media slot");
        let weather = first_node_by_class(tree, "slot-weather").expect("weather slot");
        for card in [media, weather] {
            assert!(card.layout.x >= 0.0);
            assert!(
                card.layout.x + card.layout.width <= width as f32 + 1.0,
                "{width}: {:?}",
                card.layout
            );
        }
        let media_card = first_node_by_class(tree, "media-card").unwrap();
        let weather_card = first_node_by_class(tree, "weather-card").unwrap();
        assert_eq!(
            media_card.layout.width,
            if width <= 800 { 360.0 } else { 480.0 }
        );
        assert_eq!(
            weather_card.layout.width,
            if width <= 800 { 360.0 } else { 384.0 }
        );
        if width <= 960 {
            assert!(weather.layout.y >= media.layout.y + media.layout.height);
        } else {
            assert!(weather.layout.x >= media.layout.x + media.layout.width);
        }
        let texts = rendered_text(&component);
        assert!(texts.iter().any(|text| text == "Bratislava"), "{texts:?}");
    }
}

fn preview(component: &mut FrontendSurfaceComponent, width: u32, height: u32, label: &str) {
    let Some(dir) = std::env::var_os("MESH_GLASS_PREVIEW_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(750));
    let pixels = paint(component, width, height);
    std::fs::write(
        dir.join(format!("{label}-tree.txt")),
        format!("{:#?}", component.last_tree),
    )
    .unwrap();
    std::fs::write(
        dir.join(format!("{label}-{width}x{height}.bgra")),
        pixels.data(),
    )
    .unwrap();
    for request in component.child_surface_requests() {
        let (w, h) = request.content_size;
        let mut pixels = PixelBuffer::new(w, h);
        component
            .paint_child_surface_raster(&request.node_key, &mut pixels, 1.0, (0, 0), false)
            .unwrap();
        std::fs::write(
            dir.join(format!("{label}-popup-{w}x{h}.bgra")),
            pixels.data(),
        )
        .unwrap();
    }
}

// cargo test -p mesh-core-shell --release --lib glass_bar_clock_tick_profile -- --ignored --nocapture
#[test]
#[ignore = "release-only glass-bar clock-tick frame-cost profile"]
fn glass_bar_clock_tick_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 100;
    const TICKS: u32 = 60;
    let theme = theme();
    let mut component = host("glass-bar");
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    // Task sleeps re-queue from the wall clock, so a tick time that runs
    // ahead of it would re-run the clock task until real time caught up.
    let mut tick = |component: &mut FrontendSurfaceComponent, _second: u32| {
        let due = component.next_script_task_deadline().expect("clock task");
        component.run_due_script_tasks(due).unwrap();
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
    };
    for second in 1..=4 {
        tick(&mut component, second);
    }
    for run in 0..3 {
        component.set_profiling_enabled(true);
        let _ = component.take_profiling_records();
        let mut frames = Vec::new();
        for second in 0..TICKS {
            let started = std::time::Instant::now();
            tick(&mut component, 5 + run * TICKS + second);
            frames.push(started.elapsed());
        }
        frames.sort();
        let total: Duration = frames.iter().sum();
        eprintln!(
            "glass-bar clock tick run={run}: mean {:.3}ms p50 {:.3}ms p90 {:.3}ms",
            total.as_secs_f64() * 1000.0 / f64::from(TICKS),
            frames[frames.len() / 2].as_secs_f64() * 1000.0,
            frames[frames.len() * 9 / 10].as_secs_f64() * 1000.0,
        );
        super::navigation_profile::dump_stages(
            "glass-bar clock tick",
            component.take_profiling_records(),
            TICKS,
        );
        component.set_profiling_enabled(false);
    }
}

fn glass_apps_payload(pinned: usize, installed: usize) -> serde_json::Value {
    let app = |index: usize, pinned: bool| {
        serde_json::json!({
            "id": format!("app-{index}"),
            "name": format!("Application {index}"),
            "icon": "",
            "pinned": pinned,
            "running": index % 3 == 0,
            "focused": index == 0,
            "windows": usize::from(index % 3 == 0),
        })
    };
    serde_json::json!({
        "available": true,
        "apps": (0..pinned).map(|index| app(index, true)).collect::<Vec<_>>(),
        "installed": (0..installed).map(|index| app(index, index < pinned)).collect::<Vec<_>>(),
    })
}

fn settle_dock(
    component: &mut FrontendSurfaceComponent,
    theme: &Theme,
    extent: SurfaceExtent,
    pixels: &mut PixelBuffer,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while component.wants_render() {
        assert!(
            std::time::Instant::now() < deadline,
            "the dock never settled"
        );
        component.paint_raster(theme, extent, pixels, 1.0).unwrap();
        std::thread::sleep(Duration::from_millis(8));
    }
}

/// Move the pointer off the dock and let its `:hover` transitions finish, so
/// the next sweep enters from outside like a real pointer.
fn leave_dock(
    component: &mut FrontendSurfaceComponent,
    theme: &Theme,
    extent: SurfaceExtent,
    pixels: &mut PixelBuffer,
) {
    let (width, height) = (pixels.width(), pixels.height());
    component
        .handle_input(theme, width, height, ComponentInput::PointerLeave)
        .unwrap();
    settle_dock(component, theme, extent, pixels);
}

/// Bounds of the ten pinned dock tiles once the shelf's `dock-in` entrance has
/// finished. Mid-entrance the tiles sit lower, so their centers would land on
/// the slot below each button and a hover would never reach its handler.
fn settled_dock_tiles(
    component: &mut FrontendSurfaceComponent,
    theme: &Theme,
    extent: SurfaceExtent,
    pixels: &mut PixelBuffer,
) -> Vec<(f32, f32, f32, f32)> {
    settle_dock(component, theme, extent, pixels);
    (0..10)
        .map(|index| {
            let key = reference(component, &format!("app-{index}"));
            find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &key, 0.0, 0.0).unwrap()
        })
        .collect()
}

fn count_nodes(node: &WidgetNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

// cargo test -p mesh-core-shell --release --lib glass_dock_hover_animation_profile -- --ignored --nocapture
#[test]
#[ignore = "release-only glass-dock hover-animation frame-cost profile"]
fn glass_dock_hover_animation_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 180;
    const FRAMES: u32 = 60;
    const FRAME_INTERVAL: Duration = Duration::from_millis(16);
    let installed = std::env::var("MESH_BENCH_APPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(150);
    let theme = theme();
    let mut component = host("glass-dock");
    update(
        &mut component,
        "mesh.apps",
        glass_apps_payload(10, installed),
    );
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    let tiles = settled_dock_tiles(&mut component, &theme, extent, &mut pixels);
    let nodes = count_nodes(component.last_tree.as_ref().unwrap());
    // Transitions run on the wall clock; back-to-back paints sample them
    // mid-flight, which is the per-frame work an animation pays.
    // Like the shell, paint only frames the component asks for.
    let paint_frame = |component: &mut FrontendSurfaceComponent, pixels: &mut PixelBuffer| {
        if component.wants_render() {
            component.paint_raster(&theme, extent, pixels, 1.0).unwrap();
        }
    };
    eprintln!("glass-dock: {installed} installed apps, {nodes} nodes");
    for run in 0..3 {
        leave_dock(&mut component, &theme, extent, &mut pixels);
        component.set_profiling_enabled(true);
        let _ = component.take_profiling_records();
        let mut times = Vec::new();
        let mut visits = std::collections::BTreeMap::<String, u64>::new();
        let mut measured_frames = 0u64;
        for step in 0..FRAMES {
            // Cross to the next tile every six frames, at display pacing so
            // neighboring tiles' transitions overlap as they do on screen.
            std::thread::sleep(FRAME_INTERVAL);
            let (left, top, right, bottom) = tiles[(step as usize / 6) % tiles.len()];
            let begun = std::time::Instant::now();
            if step % 6 == 0 {
                component
                    .handle_input(
                        &theme,
                        WIDTH,
                        HEIGHT,
                        ComponentInput::PointerMove {
                            x: (left + right) / 2.0,
                            y: (top + bottom) / 2.0,
                        },
                    )
                    .unwrap();
            }
            paint_frame(&mut component, &mut pixels);
            times.push(begun.elapsed());
            if let Some(snapshot) = component.take_invalidation_snapshot() {
                if !snapshot.node_visits.is_empty() {
                    measured_frames += 1;
                    for (pass, count) in snapshot.node_visits {
                        *visits.entry(pass).or_default() += count;
                    }
                }
            }
        }
        times.sort();
        let total: Duration = times.iter().sum();
        eprintln!(
            "glass-dock hover run={run}: mean {:.3}ms p50 {:.3}ms p90 {:.3}ms",
            total.as_secs_f64() * 1000.0 / f64::from(FRAMES),
            times[times.len() / 2].as_secs_f64() * 1000.0,
            times[times.len() * 9 / 10].as_secs_f64() * 1000.0,
        );
        super::navigation_profile::dump_stages(
            "glass-dock hover",
            component.take_profiling_records(),
            FRAMES,
        );
        for (pass, count) in visits {
            eprintln!(
                "node-visits {pass}: total={count} frames={measured_frames} mean={:.1}",
                count as f64 / measured_frames.max(1) as f64
            );
        }
        component.set_profiling_enabled(false);
    }
}

fn d9_report(metric: &str, mut samples: Vec<Duration>) {
    assert!(!samples.is_empty());
    samples.sort();
    let at = |fraction: f64| {
        samples[((samples.len() - 1) as f64 * fraction).round() as usize].as_secs_f64() * 1000.0
    };
    println!("MESH_PERF metric={metric} value={:.6}", at(0.9));
    eprintln!(
        "{metric}: n={} p50={:.3}ms p90={:.3}ms",
        samples.len(),
        at(0.5),
        at(0.9)
    );
}

fn dashboard_payloads(component: &mut FrontendSurfaceComponent) {
    update(
        component,
        "mesh.system",
        serde_json::json!({"available":true,"cpu":37,"memory":52,"uptime":93_784}),
    );
    update(
        component,
        "mesh.media",
        serde_json::json!({"available":true,"playing":true,"title":"Track","artist":"Artist","position":42,"length":215}),
    );
    update(
        component,
        "mesh.weather",
        serde_json::json!({"available":true,"location":"Bratislava","temperature":17,"unit":"C","condition":"cloudy","is_day":true}),
    );
}

// Each sample mounts the dashboard, delivers its services and paints the
// first frame: the work a first open pays before anything is on screen.
#[test]
#[ignore = "D9 release workload budget: glass dashboard mount through first raster"]
fn d9_glass_dashboard_open() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 1920;
    const HEIGHT: u32 = 1080;
    let theme = theme();
    let catalog = GlassCatalog::new();
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut samples = Vec::new();
    for _ in 0..12 {
        let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
        let started = std::time::Instant::now();
        let mut component = catalog.mount("glass-dashboard");
        dashboard_payloads(&mut component);
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        samples.push(started.elapsed());
        assert!(
            pixels.data().chunks_exact(4).any(|pixel| pixel[3] > 0),
            "opened dashboard must paint pixels"
        );
        let texts = rendered_text(&component);
        assert!(texts.iter().any(|text| text == "Bratislava"), "{texts:?}");
    }
    d9_report("d9_glass_dashboard_open_p90_ms", samples);
}

// Only frames the component asks for are timed, like the shell's frame
// callbacks; the first pass covers the shelf's `dock-in` entrance.
#[test]
#[ignore = "D9 release workload budget: glass dock hover animation frames"]
fn d9_glass_dock_animation() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 180;
    let theme = theme();
    let mut component = host("glass-dock");
    update(&mut component, "mesh.apps", glass_apps_payload(10, 150));
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    let tiles: Vec<_> = (0..10)
        .map(|index| {
            let key = reference(&component, &format!("app-{index}"));
            find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &key, 0.0, 0.0).unwrap()
        })
        .collect();
    let mut samples = Vec::new();
    for step in 0..180usize {
        let started = std::time::Instant::now();
        if step % 6 == 0 {
            let (left, top, right, bottom) = tiles[(step / 6) % tiles.len()];
            component
                .handle_input(
                    &theme,
                    WIDTH,
                    HEIGHT,
                    ComponentInput::PointerMove {
                        x: (left + right) / 2.0,
                        y: (top + bottom) / 2.0,
                    },
                )
                .unwrap();
        }
        if component.wants_render() {
            component
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            samples.push(started.elapsed());
        }
    }
    assert!(
        samples.len() >= 30,
        "hover must animate: {} frames",
        samples.len()
    );
    d9_report("d9_glass_dock_frame_p90_ms", samples);
}

#[test]
#[ignore = "D9 release workload budget: glass bar clock tick through raster"]
fn d9_glass_bar_clock_tick() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 100;
    let theme = theme();
    let mut component = host("glass-bar");
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    // Task sleeps re-queue from the wall clock, so ticks use the clock
    // task's own deadline rather than a synthetic time that runs ahead.
    let mut samples = Vec::new();
    for second in 1..=64u64 {
        let due = component.next_script_task_deadline().expect("clock task");
        let started = std::time::Instant::now();
        component.run_due_script_tasks(due).unwrap();
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        if second > 4 {
            samples.push(started.elapsed());
        }
    }
    d9_report("d9_glass_clock_tick_p90_ms", samples);
}

/// While a recording runs, the bar's 1 s `elapsed` tick patches its text node
/// in place, and frames with nothing dirty (the pulsing dot keeps the frame
/// loop alive) reuse the retained tree: no rebuild, no structural validation,
/// no whole-bar layout.
#[test]
fn glass_bar_recording_ticks_stay_off_the_rebuild_path() {
    fn assert_sparse(component: &mut FrontendSurfaceComponent, label: &str) -> u64 {
        let snapshot = component.take_invalidation_snapshot().expect("frame");
        let counts = &snapshot.node_visits;
        assert!(!snapshot.full_rebuild, "{label}: {snapshot:?}");
        assert!(snapshot.retained_path, "{label}: {snapshot:?}");
        assert_eq!(counts["validation"], 0, "{label}: {counts:?}");
        assert_eq!(counts["layout_build"], 0, "{label}: {counts:?}");
        snapshot.patched_node_count
    }
    let theme = theme();
    let mut component = host("glass-bar");
    let extent = SurfaceExtent::unpadded(2880, 100);
    let mut pixels = PixelBuffer::new(2880, 100);
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        - 5.0;
    update(
        &mut component,
        "mesh.recorder",
        serde_json::json!({
            "available": true, "recording": true, "started_at": started_at, "output": "",
        }),
    );
    for _ in 0..3 {
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
    }
    component.set_profiling_enabled(true);
    let _ = component.take_invalidation_snapshot();
    let mut patched_frames = 0;
    let finish = std::time::Instant::now() + Duration::from_millis(2200);
    while std::time::Instant::now() < finish {
        std::thread::sleep(Duration::from_millis(16));
        let now = std::time::Instant::now();
        if component
            .next_script_task_deadline()
            .is_some_and(|due| due <= now)
        {
            component.run_due_script_tasks(now).unwrap();
        }
        if component.wants_render() {
            component
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            patched_frames += usize::from(assert_sparse(&mut component, "frame") > 0);
        }
    }
    assert!(
        patched_frames >= 2,
        "the timer patched {patched_frames} frames"
    );
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    assert_eq!(assert_sparse(&mut component, "clean frame"), 0);
    let mut elapsed = Vec::new();
    collect_text_content(
        first_node_by_class(component.last_tree.as_ref().unwrap(), "elapsed").unwrap(),
        &mut elapsed,
    );
    assert!(elapsed.concat().starts_with("00:0"), "{elapsed:?}");
}

fn launcher_wrapper(component: &FrontendSurfaceComponent) -> &WidgetNode {
    fn find(node: &WidgetNode) -> Option<&WidgetNode> {
        if node.is_promoted_popover()
            && node.children.first().is_some_and(|popover| {
                popover
                    .attributes
                    .get(Prop::Class)
                    .is_some_and(|class| class.contains("launcher-shell"))
            })
        {
            return Some(node);
        }
        node.children.iter().find_map(find)
    }
    find(component.last_tree.as_ref().unwrap()).expect("launcher popover wrapper")
}

#[test]
fn closed_popover_subtree_is_skipped_until_it_opens_or_exits() {
    let mut component = host("glass-dock");
    update(&mut component, "mesh.apps", glass_apps_payload(4, 24));
    paint(&mut component, 1920, 180);
    paint(&mut component, 1920, 180);
    let wrapper = launcher_wrapper(&component);
    assert!(component.is_dormant_popover_wrapper(wrapper));
    let popover = &wrapper.children[0];
    let popover_id = popover.id;
    let popover_key = popover.mesh_key().unwrap().to_owned();

    press(&mut component, "glassLauncher", 1920, 180);
    assert_eq!(component.child_surface_requests().len(), 1);
    assert!(!component.is_dormant_popover_wrapper(launcher_wrapper(&component)));
    paint(&mut component, 1920, 180);

    component
        .handle_input(
            &theme(),
            1920,
            180,
            ComponentInput::KeyPressed {
                key: "Escape".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    paint(&mut component, 1920, 180);
    assert!(component.child_surface_requests().is_empty());
    assert!(component.is_dormant_popover_wrapper(launcher_wrapper(&component)));

    // The shell keeps the popup mapped for its exit: the subtree must sample
    // again, transitioning from the resting style it showed while closed.
    component.set_closing_child_keys([popover_key].into_iter().collect());
    assert!(!component.is_dormant_popover_wrapper(launcher_wrapper(&component)));
    paint(&mut component, 1920, 180);
    assert!(
        component.transitions.contains_key(popover_id),
        "the exit class must start the launcher's own transition"
    );
}

// cargo test -p mesh-core-shell --release --lib glass_dashboard_spectrum_profile -- --ignored --nocapture
#[test]
#[ignore = "release-only glass-dashboard spectrum frame-cost profile"]
fn glass_dashboard_spectrum_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 1920;
    const HEIGHT: u32 = 1080;
    const FRAMES: u32 = 120;
    let theme = theme();
    let mut component = host("glass-dashboard");
    component.force_full_template_build = std::env::var_os("MESH_BENCH_FORCE_TEMPLATE").is_some();
    dashboard_payloads(&mut component);
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    let mut frame = |component: &mut FrontendSurfaceComponent, index: u32| {
        let bars: Vec<f64> = (0..32)
            .map(|bar| (f64::from((index * 7 + bar * 13) % 100)) / 100.0)
            .collect();
        update(
            component,
            "mesh.visualizer",
            serde_json::json!({"available":true,"active":true,"bars":bars}),
        );
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
    };
    for index in 0..30 {
        frame(&mut component, index);
    }
    eprintln!(
        "glass-dashboard: {} nodes",
        count_nodes(component.last_tree.as_ref().unwrap())
    );
    for run in 0..3 {
        component.set_profiling_enabled(true);
        let _ = component.take_profiling_records();
        let mut times = Vec::new();
        let mut visits = std::collections::BTreeMap::<String, u64>::new();
        let mut measured_frames = 0u64;
        for index in 0..FRAMES {
            let begun = std::time::Instant::now();
            frame(&mut component, 30 + run * FRAMES + index);
            times.push(begun.elapsed());
            if let Some(snapshot) = component.take_invalidation_snapshot() {
                if !snapshot.node_visits.is_empty() {
                    measured_frames += 1;
                    for (pass, count) in snapshot.node_visits {
                        *visits.entry(pass).or_default() += count;
                    }
                }
            }
        }
        times.sort();
        let total: Duration = times.iter().sum();
        eprintln!(
            "glass-dashboard spectrum run={run}: mean {:.3}ms p50 {:.3}ms p90 {:.3}ms",
            total.as_secs_f64() * 1000.0 / f64::from(FRAMES),
            times[times.len() / 2].as_secs_f64() * 1000.0,
            times[times.len() * 9 / 10].as_secs_f64() * 1000.0,
        );
        super::navigation_profile::dump_stages(
            "glass-dashboard spectrum",
            component.take_profiling_records(),
            FRAMES,
        );
        for (pass, count) in visits {
            eprintln!(
                "node-visits {pass}: total={count} frames={measured_frames} mean={:.1}",
                count as f64 / measured_frames.max(1) as f64
            );
        }
        component.set_profiling_enabled(false);
    }
}

#[test]
fn glass_settings_trigger_keeps_its_icon_centered_when_popup_opens() {
    let mut component = host("glass-bar");
    Arc::make_mut(&mut component.settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    for open in [false, true] {
        paint(&mut component, 2880, 100);
        if open {
            press(&mut component, "glassControls", 2880, 100);
            paint(&mut component, 2880, 100);
        }
        let trigger =
            first_node_with_ref(component.last_tree.as_ref().unwrap(), "glassControls").unwrap();
        let icon = first_node_by_class(trigger, "status-glyph").unwrap();
        assert!(
            (icon.layout.x + icon.layout.width / 2.0
                - trigger.layout.x
                - trigger.layout.width / 2.0)
                .abs()
                < 0.5,
            "open={open}, trigger={:?}, icon={:?}",
            trigger.layout,
            icon.layout
        );
        assert!(
            (icon.layout.y + icon.layout.height / 2.0
                - trigger.layout.y
                - trigger.layout.height / 2.0)
                .abs()
                < 0.5,
            "open={open}, trigger={:?}, icon={:?}",
            trigger.layout,
            icon.layout
        );
    }
}

fn workspace_pill(node: &WidgetNode) -> Option<&WidgetNode> {
    let is = |n: &WidgetNode, class: &str| {
        n.attributes
            .get(Prop::Class)
            .is_some_and(|v| v.split_whitespace().any(|t| t == class))
    };
    if is(node, "pill") && node.children.iter().any(|c| is(c, "ws")) {
        return Some(node);
    }
    node.children.iter().find_map(workspace_pill)
}

/// The glass theme lifts hovered pills by 3px. A pointer resting on the bottom
/// edge must keep the pill hovered instead of un-hovering as it rises, then
/// re-hovering as it drops back, every few frames.
#[test]
fn glass_bar_pill_hover_is_stable_at_its_lifted_edge() {
    const WIDTH: u32 = 1440;
    const HEIGHT: u32 = 100;
    let theme = theme();
    let mut component = host("glass-bar");
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    update(
        &mut component,
        "mesh.wm",
        serde_json::json!({"available":true,"active_workspace":1,"active_window":null,
            "workspaces":[{"id":1,"name":"1","windows":2}]}),
    );
    settle_dock(&mut component, &theme, extent, &mut pixels);
    let tree = component.last_tree.as_ref().unwrap();
    let key = workspace_pill(tree).unwrap().mesh_key().unwrap().to_owned();
    let (left, _, right, bottom) = find_node_bounds_by_key(tree, &key, 0.0, 0.0).unwrap();
    let hovered = |component: &FrontendSurfaceComponent| {
        workspace_pill(component.last_tree.as_ref().unwrap())
            .unwrap()
            .state
            .contains(mesh_core_elements::NodeState::HOVERED)
    };
    let mut move_to = |component: &mut FrontendSurfaceComponent, x: f32, y: f32| {
        component
            .handle_input(&theme, WIDTH, HEIGHT, ComponentInput::PointerMove { x, y })
            .unwrap();
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
    };
    let x = (left + right) / 2.0 + 30.0;
    let mut states = Vec::new();
    for _ in 0..30 {
        move_to(&mut component, x, bottom - 1.0);
        states.push(hovered(&component));
        std::thread::sleep(Duration::from_millis(16));
    }
    assert!(states.iter().all(|&state| state), "{states:?}");

    move_to(&mut component, x, bottom + 4.0);
    assert!(
        !hovered(&component),
        "leaving the resting box still un-hovers"
    );
}

fn first_open_report(label: &str, samples: &mut [Duration]) {
    samples.sort();
    eprintln!(
        "{label}: n={} min {:.3}ms p50 {:.3}ms max {:.3}ms",
        samples.len(),
        samples[0].as_secs_f64() * 1000.0,
        samples[samples.len() / 2].as_secs_f64() * 1000.0,
        samples[samples.len() - 1].as_secs_f64() * 1000.0,
    );
}

// Synchronous work between the user's open and the first painted frame of
// the launcher popover and the wallpaper picker. Resource decoding stays on
// the broker's workers and is not included.
// cargo test -p mesh-core-shell --release --lib glass_first_open_profile -- --ignored --nocapture
#[test]
#[ignore = "release-only glass launcher and wallpaper-picker first-open profile"]
fn glass_first_open_profile() {
    const ROUNDS: usize = 5;
    let installed = std::env::var("MESH_BENCH_APPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(150);
    let theme = theme();
    let catalog = GlassCatalog::new();

    let (width, height) = (2880, 180);
    let extent = SurfaceExtent::unpadded(width, height);
    let mut pixels = PixelBuffer::new(width, height);
    let mut component = catalog.mount("glass-dock");
    update(
        &mut component,
        "mesh.apps",
        glass_apps_payload(10, installed),
    );
    settle_dock(&mut component, &theme, extent, &mut pixels);
    eprintln!(
        "glass-dock: {installed} installed apps, {} nodes",
        count_nodes(component.last_tree.as_ref().unwrap())
    );
    let mut samples = Vec::new();
    for round in 0..ROUNDS {
        component.set_profiling_enabled(round == ROUNDS - 1);
        let _ = component.take_profiling_records();
        let started = std::time::Instant::now();
        component.focused_key = Some(reference(&component, "glassLauncher"));
        component
            .handle_input(
                &theme,
                width,
                height,
                ComponentInput::KeyPressed {
                    key: "Enter".into(),
                    modifiers: Default::default(),
                },
            )
            .unwrap();
        let input_done = started.elapsed();
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        let dock_done = started.elapsed();
        let request = component
            .child_surface_requests()
            .into_iter()
            .next()
            .expect("launcher popup");
        let (w, h) = request.content_size;
        let mut popup = PixelBuffer::new(w, h);
        component
            .paint_child_surface_raster(&request.node_key, &mut popup, 1.0, (0, 0), false)
            .unwrap();
        samples.push(started.elapsed());
        eprintln!(
            "launcher round {round}: input {:.3}ms, dock paint {:.3}ms, popup paint {:.3}ms",
            input_done.as_secs_f64() * 1000.0,
            (dock_done - input_done).as_secs_f64() * 1000.0,
            (started.elapsed() - dock_done).as_secs_f64() * 1000.0,
        );
        if round == ROUNDS - 1 {
            super::navigation_profile::dump_stages(
                "launcher open",
                component.take_profiling_records(),
                1,
            );
        }
        component
            .handle_input(
                &theme,
                width,
                height,
                ComponentInput::KeyPressed {
                    key: "Escape".into(),
                    modifiers: Default::default(),
                },
            )
            .unwrap();
        settle_dock(&mut component, &theme, extent, &mut pixels);
    }
    first_open_report("launcher open", &mut samples);

    let (width, height) = (2880, 1800);
    let extent = SurfaceExtent::unpadded(width, height);
    let mut pixels = PixelBuffer::new(width, height);
    let mut component = catalog.mount("glass-wallpapers");
    let wallpapers: Vec<_> = (0..6)
        .map(|index| {
            serde_json::json!({
                "path": format!("/walls/{index}.png"),
                "name": format!("wall {index}"),
                "thumbnail": "",
                "color": "#334455",
            })
        })
        .collect();
    update(
        &mut component,
        "mesh.wallpapers",
        serde_json::json!({"available": true, "directory": "/walls", "wallpapers": wallpapers}),
    );
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    let id = "@mesh/glass-wallpapers".to_owned();
    let mut samples = Vec::new();
    for round in 0..ROUNDS {
        component
            .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
                surface_id: id.clone(),
                visible: false,
            })
            .unwrap();
        component.set_profiling_enabled(round == ROUNDS - 1);
        let _ = component.take_profiling_records();
        let started = std::time::Instant::now();
        component
            .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
                surface_id: id.clone(),
                visible: true,
            })
            .unwrap();
        // Until the first frame in resting styles, without the entrance pose.
        loop {
            component
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            let entering = component.last_tree.as_ref().is_some_and(|tree| {
                tree.state
                    .contains(mesh_core_elements::NodeState::SURFACE_ENTERING)
            });
            if !entering {
                break;
            }
        }
        samples.push(started.elapsed());
        if round == ROUNDS - 1 {
            super::navigation_profile::dump_stages(
                "wallpaper picker open",
                component.take_profiling_records(),
                1,
            );
        }
    }
    first_open_report("wallpaper picker open", &mut samples);
}

#[test]
fn exiting_glass_surface_stops_exporting_compositor_blur() {
    let theme = theme();
    let mut component = host("glass-dashboard");
    let extent = SurfaceExtent::unpadded(1920, 1080);
    let mut pixels = PixelBuffer::new(1920, 1080);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    assert!(!component.display_list_blur_regions().is_empty());

    component.set_surface_exiting(true);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    assert!(
        component.display_list_blur_regions().is_empty(),
        "compositor blur cannot fade with the content"
    );

    component.set_surface_exiting(false);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    assert!(!component.display_list_blur_regions().is_empty());
}

/// Drives one real glass surface through the shell runtime against the
/// testing presentation backend.
struct ShownSurfaceHarness {
    shell: crate::shell::Shell,
    id: String,
}

impl ShownSurfaceHarness {
    fn wallpapers() -> Self {
        let mut component = GlassCatalog::new().mount("glass-wallpapers");
        let wallpapers: Vec<_> = (0..6)
            .map(|index| {
                serde_json::json!({
                    "path": format!("/walls/{index}.png"),
                    "name": format!("wall {index}"),
                    "thumbnail": "",
                    "color": "#334455",
                })
            })
            .collect();
        update(
            &mut component,
            "mesh.wallpapers",
            serde_json::json!({"available": true, "directory": "/walls", "wallpapers": wallpapers}),
        );
        let id = component.surface_id().to_owned();
        let mut shell = crate::shell::Shell::new();
        shell.presentation_engine =
            mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
        shell.register_component(Box::new(component));
        shell.debug.profiling_enabled = true;
        let mut harness = Self { shell, id };
        harness.set_visible(false);
        harness
    }

    fn set_visible(&mut self, visible: bool) {
        let surface_id = self.id.clone();
        let request = if visible {
            CoreRequest::ShowSurface { surface_id }
        } else {
            CoreRequest::HideSurface { surface_id }
        };
        let mut emitted = self.shell.apply_request(request).unwrap();
        self.shell.drain_requests(&mut emitted).unwrap();
        if let Some(state) = self.shell.core.surfaces.get_mut(&self.id)
            && state.closing_until.is_some()
        {
            state.closing_until = Some(std::time::Instant::now());
            let mut emitted = self.shell.complete_due_surface_transitions().unwrap();
            self.shell.drain_requests(&mut emitted).unwrap();
        }
        self.shell.render_components().unwrap();
        if !visible {
            // The hiding present unmaps the surface; it must be configured
            // again before it can take another buffer.
            self.shell
                .presentation_engine
                .testing_set_surface_configured(&self.id, false);
        }
    }

    fn configures(&self) -> usize {
        self.shell
            .presentation_engine
            .testing_surface_config_history()
            .iter()
            .filter(|(surface, _)| *surface == self.id)
            .count()
    }

    /// Show the surface, answer its configure, and count what that took.
    fn show(&mut self) -> ShowCost {
        use crate::shell::runtime::profiling::CapturedRecord;
        use mesh_core_debug::ProfilingStage;

        self.shell.profiling.capture = Some(Vec::new());
        let configures_before = self.configures();
        self.set_visible(true);
        let configures = self.configures() - configures_before;
        self.shell
            .presentation_engine
            .testing_set_surface_configured(&self.id, true);
        self.shell.render_components().unwrap();
        let records = self.shell.profiling.capture.take().unwrap();
        let count = |stage: ProfilingStage, trigger: Option<&str>| {
            records
                .iter()
                .filter(|record| {
                    matches!(record, CapturedRecord::Stage { surface_id, stage: s, trigger: t, .. }
                        if *surface_id == self.id && *s == stage
                            && trigger.is_none_or(|trigger| t.as_deref() == Some(trigger)))
                })
                .count()
        };
        ShowCost {
            configures,
            rebuilds: count(ProfilingStage::TreeBuild, Some("rebuild")),
            renders: count(ProfilingStage::TotalSurfaceRender, None),
            paints: count(ProfilingStage::Paint, None),
            presents: count(ProfilingStage::PresentCommit, None),
        }
    }
}

#[derive(Debug, PartialEq)]
struct ShowCost {
    configures: usize,
    rebuilds: usize,
    renders: usize,
    paints: usize,
    presents: usize,
}

/// One configure, then one paint at the size the compositor grants, from the
/// retained tree.
const RETAINED_SHOW: ShowCost = ShowCost {
    configures: 1,
    rebuilds: 0,
    renders: 1,
    paints: 1,
    presents: 1,
};

/// Re-showing a hidden surface reuses its retained tree and paints once, at
/// the size the compositor configures, instead of painting an unconfigured
/// pass and rebuilding the tree for each.
#[test]
fn reshown_glass_surface_reuses_its_tree_and_paints_once() {
    let mut harness = ShownSurfaceHarness::wallpapers();
    harness.show();
    harness.set_visible(false);
    assert_eq!(harness.show(), RETAINED_SHOW);
}

/// An idle prewarm gives a never-shown surface the retained tree a re-shown
/// one has, so its first show costs the same.
#[test]
fn prewarmed_glass_surface_first_show_paints_once() {
    let mut harness = ShownSurfaceHarness::wallpapers();
    assert!(harness.shell.prewarm_hidden_surface().unwrap());
    assert!(
        !harness.shell.prewarm_hidden_surface().unwrap(),
        "each hidden surface is prewarmed once"
    );
    assert_eq!(harness.configures(), 0, "a prewarm configures nothing");
    assert!(
        harness
            .shell
            .presentation_engine
            .testing_presented_surfaces()
            .is_empty(),
        "a prewarm presents nothing"
    );
    assert_eq!(harness.show(), RETAINED_SHOW);
}

#[test]
fn glass_popup_animation_preserves_parent_scene() {
    let theme = theme();
    let extent = SurfaceExtent::unpadded(1920, 100);
    let scale = mesh_core_render::FractionalScale::identity();
    let mut total_parent_recordings = 0;
    for trigger in ["glassCalendar", "glassControls"] {
        let mut component = host("glass-bar");
        component.set_profiling_enabled(true);
        let mut canvas = SurfaceCanvas::Scene(
            mesh_core_render::prepared_scene::SceneCanvas::new(1920, 100).unwrap(),
        );
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        // Finish the bar's own startup keyframes before opening a popup.
        for animation in component.keyframe_animations.values_mut() {
            animation.started_at = Instant::now() - Duration::from_secs(2);
        }
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        component.focused_key = Some(reference(&component, trigger));
        component
            .handle_input(
                &theme,
                1920,
                100,
                ComponentInput::KeyPressed {
                    key: "Enter".into(),
                    modifiers: Default::default(),
                },
            )
            .unwrap();
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        let request = component.child_surface_requests().remove(0);
        component.set_entering_child_keys([request.node_key.clone()].into_iter().collect());
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        component.set_entering_child_keys(Default::default());
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        // Trigger feedback lasts 180 ms; popup motion lasts 420 ms.
        std::thread::sleep(Duration::from_millis(200));
        component.request_paint();
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        component.take_present_damage();
        let SurfaceCanvas::Scene(recording) = &mut canvas else {
            unreachable!()
        };
        let mut previous = recording.snapshot(scale).unwrap();
        let mut child_pixels =
            PixelBuffer::new(request.content_size.0 + 40, request.content_size.1 + 40);
        let mut previous_child = component.child_surface_paint_generation(&request.node_key);
        let mut changed_child_frames = 0;
        let mut parent_recordings = 0;
        component.take_profiling_records();
        for _ in 0..21 {
            component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
            assert!(
                component.take_present_damage().is_empty(),
                "{trigger}: parent damage"
            );
            let SurfaceCanvas::Scene(recording) = &mut canvas else {
                unreachable!()
            };
            let current = recording.snapshot(scale).unwrap();
            parent_recordings +=
                usize::from(!std::ptr::eq(previous.resources(), current.resources()));
            previous = current;
            let child = component.child_surface_paint_generation(&request.node_key);
            changed_child_frames += usize::from(child != previous_child);
            previous_child = child;
        }
        let paint_records = component
            .take_profiling_records()
            .into_iter()
            .filter(|record| record.stage == mesh_core_debug::ProfilingStage::Paint)
            .count();
        assert_eq!(paint_records, parent_recordings);
        component
            .paint_child_surface_raster(&request.node_key, &mut child_pixels, 1.0, (20, 20), false)
            .unwrap();
        assert!(child_pixels.data().iter().any(|byte| *byte != 0));
        assert!(
            changed_child_frames > 0,
            "{trigger}: popup must still animate"
        );
        eprintln!(
            "MESH_PERF trigger={trigger} frames=21 parent_recordings={parent_recordings} changed_child_frames={changed_child_frames}"
        );
        total_parent_recordings += parent_recordings;
        component.request_paint();
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        assert!(!component.take_present_damage().is_empty());
        let SurfaceCanvas::Scene(recording) = &mut canvas else {
            unreachable!()
        };
        assert!(!std::ptr::eq(
            previous.resources(),
            recording.snapshot(scale).unwrap().resources()
        ));
    }
    assert_eq!(
        total_parent_recordings, 0,
        "child-only animation must preserve the parent's complete scene"
    );
}

#[test]
fn dashboard_meter_animation_preserves_layout() {
    let theme = theme();
    let set_motion = |component: &mut FrontendSurfaceComponent, reduced: bool| {
        let mut settings = component.settings.clone();
        Arc::make_mut(&mut settings)
            .set_namespace("shell", serde_json::json!({"motion":{"reduced":reduced}}));
        component.apply_settings(&settings).unwrap();
    };
    let mut component = host("glass-dashboard");
    let extent = SurfaceExtent::unpadded(1920, 1080);
    let mut canvas = SurfaceCanvas::Scene(
        mesh_core_render::prepared_scene::SceneCanvas::new(1920, 1080).unwrap(),
    );
    set_motion(&mut component, true);
    let payload = |value: u32| {
        serde_json::json!({
            "available":true, "cpu":value, "cpu_history":vec![value; 30],
            "memory_used":20, "memory_total":100, "disk_used":20, "disk_total":100,
            "temperature":40, "net_rx":0, "net_tx":0, "uptime":100,
        })
    };
    update(&mut component, "mesh.system", payload(20));
    component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
    set_motion(&mut component, false);
    component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
    update(&mut component, "mesh.system", payload(80));
    component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
    assert!(!component.transitions.is_empty(), "meter must animate");
    component.set_profiling_enabled(true);
    let mut layout_frames = 0;
    for _ in 0..21 {
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        let snapshot = component.take_invalidation_snapshot().unwrap();
        let tree = component.last_tree.as_ref().unwrap();
        assert_eq!(
            first_node_by_class(tree, "meter-cpu")
                .unwrap()
                .layout
                .height,
            92.0
        );
        assert_eq!(
            first_node_by_class(tree, "history-bar")
                .unwrap()
                .layout
                .height,
            54.0
        );
        layout_frames += usize::from(
            snapshot.node_visits["layout_sync"] > 0 || snapshot.node_visits["layout_write"] > 0,
        );
    }
    eprintln!("MESH_PERF dashboard_meter frames=21 layout_frames={layout_frames}");
    assert_eq!(
        layout_frames, 0,
        "meter animation must only change paint transforms"
    );
    set_motion(&mut component, true);
    component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
    let tree = component.last_tree.as_ref().unwrap();
    for (class, expected) in [("meter-cpu", 74.0 / 92.0), ("history-bar", 43.0 / 54.0)] {
        let node = first_node_by_class(tree, class).unwrap();
        assert!(
            (node.computed_style.transform.scale_y - expected).abs() < 0.00001,
            "{class}: {} != {expected}",
            node.computed_style.transform.scale_y
        );
        assert_eq!(
            node.computed_style.transform_origin.y,
            mesh_core_elements::style::TransformOriginValue::Percent(100.0)
        );
    }
}

#[test]
fn dashboard_spectrum_updates_preserve_layout() {
    let theme = theme();
    let mut component = host("glass-dashboard");
    let mut settings = component.settings.clone();
    Arc::make_mut(&mut settings)
        .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
    component.apply_settings(&settings).unwrap();
    let extent = SurfaceExtent::unpadded(1920, 1080);
    let mut canvas = SurfaceCanvas::Scene(
        mesh_core_render::prepared_scene::SceneCanvas::new(1920, 1080).unwrap(),
    );
    update(
        &mut component,
        "mesh.visualizer",
        serde_json::json!({"available":true,"active":true,"bars":vec![0.2;32]}),
    );
    component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
    component.set_profiling_enabled(true);
    let mut layout_frames = 0;
    for index in 0..21 {
        let value = if index % 2 == 0 { 0.8 } else { 0.2 };
        update(
            &mut component,
            "mesh.visualizer",
            serde_json::json!({"available":true,"active":true,"bars":vec![value;32]}),
        );
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        let snapshot = component.take_invalidation_snapshot().unwrap();

        layout_frames += usize::from(
            snapshot.node_visits["layout_sync"] > 0 || snapshot.node_visits["layout_write"] > 0,
        );
    }
    eprintln!("MESH_PERF spectrum updates=21 layout_frames={layout_frames}");
    assert_eq!(layout_frames, 0);
    let bar = first_node_by_class(component.last_tree.as_ref().unwrap(), "spectrum-bar").unwrap();
    assert_eq!(bar.layout.height, 44.0);
    assert!((bar.computed_style.transform.scale_y - 35.0 / 44.0).abs() < 0.00001);
    assert_eq!(
        bar.computed_style.transform_origin.y,
        mesh_core_elements::style::TransformOriginValue::Percent(100.0)
    );
}
