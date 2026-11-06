use super::*;
use mesh_core_theme::{ThemeModule, ThemeModuleLayer, ThemeSourceHandle};
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .unwrap()
}

pub fn theme() -> Theme {
    let pack = mesh_core_theme::load_theme_from_source(
        &ThemeSourceHandle::new(root().join("modules/themes/liquid-glass"), "theme.css").unwrap(),
        "liquid-glass",
        "Liquid Glass",
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
    for module in [
        "glass-bar",
        "glass-widgets",
        "glass-dock",
        "glass-dashboard",
        "glass-notifications",
        "glass-wallpapers",
    ] {
        layers.push(ThemeModuleLayer {
            module_id: format!("@mesh/{module}"),
            module: ThemeModule::default(),
            families: vec!["@mesh/glass-material".into()],
        });
    }
    Theme::compose_layers(
        &default_theme(),
        &pack,
        "@mesh/liquid-glass",
        "dark",
        layers,
        &HashMap::new(),
    )
    .unwrap()
}

const GLASS_MODULES: [&str; 10] = [
    "glass-bar",
    "glass-widgets",
    "glass-dock",
    "glass-dashboard",
    "glass-wallpapers",
    "wallpaper-carousel",
    "wallpaper-parallax",
    "profile-switcher",
    "profile-menu",
    "material-workspace",
];

/// Compiled glass modules, contracts and locale, shared by every surface the
/// tests mount. Building it compiles every module, so workloads that time a
/// surface open prepare it once outside the measured region.
pub struct GlassCatalog {
    entries: HashMap<String, FrontendCatalogEntry>,
    interfaces: InterfaceCatalog,
    locale: mesh_core_locale::LocaleEngine,
}

impl GlassCatalog {
    pub fn new() -> Self {
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
                &serde_json::json!({
                    "state": { "profiles": "string[]", "active_profile": "string" },
                    "methods": { "switch_profile": "(profile_id: string)" }
                }),
            )
            .unwrap(),
        );
        interfaces.register_contract(
            mesh_core_service::contract::parse_interface_contract(
                "mesh.theme",
                "1.0",
                &serde_json::json!({
                    "state": { "backdrop_image": "string?" },
                    "methods": { "set_backdrop_image": "(path: string)" }
                }),
            )
            .unwrap(),
        );
        let mut locale = mesh_core_locale::LocaleEngine::new("en");
        for module in [
            "glass-bar",
            "glass-widgets",
            "glass-dock",
            "glass-dashboard",
            "glass-wallpapers",
            "wallpaper-carousel",
            "wallpaper-parallax",
            "material-workspace",
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

    pub fn mount(&self, name: &str) -> FrontendSurfaceComponent {
        self.mount_instance(name, &format!("@mesh/{name}"))
    }

    pub fn mount_instance(&self, name: &str, instance: &str) -> FrontendSurfaceComponent {
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
        component = component.with_instance_id(instance);
        component.locale = self.locale.snapshot();
        component
            .mount(ComponentContext {
                component_id: instance.into(),
                surface_id: instance.into(),
                diagnostics: Diagnostics::new(id),
            })
            .unwrap();
        component.visible = true;
        component
    }
}

pub fn host(name: &str) -> FrontendSurfaceComponent {
    GlassCatalog::new().mount(name)
}

pub fn paint(component: &mut FrontendSurfaceComponent, width: u32, height: u32) -> PixelBuffer {
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

pub fn update(component: &mut FrontendSurfaceComponent, service: &str, payload: serde_json::Value) {
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: service.into(),
            source_module: "@test/glass-services".into(),
            payload,
        })
        .unwrap();
}

pub fn glass_apps_payload(pinned: usize, installed: usize) -> serde_json::Value {
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

pub fn settle_dock(
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

pub fn leave_dock(
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

pub fn settled_dock_tiles(
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

pub fn count_nodes(node: &WidgetNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

pub fn reference(component: &FrontendSurfaceComponent, name: &str) -> String {
    first_node_with_ref(component.last_tree.as_ref().unwrap(), name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .mesh_key()
        .unwrap()
        .to_owned()
}
