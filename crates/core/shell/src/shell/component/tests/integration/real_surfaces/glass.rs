use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::catalog::FrontendCatalog;
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

fn host(name: &str) -> FrontendSurfaceComponent {
    let mut entries = HashMap::new();
    for module in [
        "glass-bar",
        "glass-widgets",
        "glass-dock",
        "profile-switcher",
        "profile-menu",
    ] {
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
    let id = format!("@mesh/{name}");
    let entry = entries[&id].clone();
    let mut interfaces = InterfaceCatalog::default();
    for interface in [
        "apps",
        "audio",
        "brightness",
        "media",
        "network",
        "power",
        "recorder",
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
    let catalog = FrontendCatalog {
        modules: entries,
        ..Default::default()
    };
    let mut locale = mesh_core_locale::LocaleEngine::new("en");
    for module in ["glass-bar", "glass-dock"] {
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
    let mut component = FrontendSurfaceComponent::new_for_test(
        (*entry.compiled).clone(),
        entry.module_dir,
        catalog,
        interfaces,
        test_settings_store(),
    );
    component.locale = locale.snapshot();
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
