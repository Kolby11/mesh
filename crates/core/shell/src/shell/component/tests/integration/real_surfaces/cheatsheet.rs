use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::catalog::FrontendCatalog;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_frontend::compile_frontend_module;

fn mount_sheet() -> FrontendSurfaceComponent {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
        .join("modules/frontend/cheatsheet");
    let manifest = mesh_core_module::manifest::load_canonical_manifest(&dir)
        .unwrap()
        .manifest;
    let compiled = compile_frontend_module(&manifest, &dir).unwrap();
    let mut entries = HashMap::new();
    entries.insert(
        "@mesh/cheatsheet".to_string(),
        FrontendCatalogEntry {
            module_dir: dir.clone(),
            compiled: compiled.clone().into(),
        },
    );
    let mut interfaces = InterfaceCatalog::default();
    interfaces.register_contract(
        mesh_core_service::contract::parse_interface_contract(
            "mesh.wm-keybinds",
            "1.0",
            &serde_json::json!({
                "state": { "available": "boolean", "sections": "any" },
                "methods": { "refresh": "() -> Result" }
            }),
        )
        .unwrap(),
    );
    interfaces.register_provider(mesh_core_service::InterfaceProvider {
        interface: "mesh.wm-keybinds".into(),
        version: Some("1.0".into()),
        base_module: None,
        provider_module: "@mesh/hyprland-keybinds".into(),
        backend_name: "Hyprland keybinds".into(),
        priority: 100,
    });
    let mut locale = mesh_core_locale::LocaleEngine::new("en");
    let catalog: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("config/i18n/en.json")).unwrap(),
    )
    .unwrap();
    locale.load_module_catalog(
        "@mesh/cheatsheet",
        mesh_core_locale::compile_catalog("en", &catalog),
    );
    let mut component = FrontendSurfaceComponent::new_for_test(
        compiled,
        dir,
        FrontendCatalog {
            modules: entries,
            ..Default::default()
        },
        interfaces,
        test_settings_store(),
    )
    .with_instance_id("@mesh/cheatsheet");
    component.locale = locale.snapshot();
    component
        .mount(ComponentContext {
            component_id: "@mesh/cheatsheet".into(),
            surface_id: "@mesh/cheatsheet".into(),
            diagnostics: Diagnostics::new("@mesh/cheatsheet".to_string()),
        })
        .unwrap();
    component.visible = true;
    component
}


fn paint(component: &mut FrontendSurfaceComponent) -> Vec<String> {
    let mut pixels = PixelBuffer::new(1400, 700);
    component
        .paint_raster(&default_theme(), SurfaceExtent::unpadded(1400, 700), &mut pixels, 1.0)
        .unwrap();
    rendered_text(component)
}

fn section(name: &str, binds: &[(&[&str], &str, &str)]) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "binds": binds.iter().map(|(mods, key, comment)| serde_json::json!({
            "mods": mods, "key": key, "comment": comment
        })).collect::<Vec<_>>(),
    })
}

#[test]
fn cheatsheet_lists_sections_with_keycaps_and_comments() {
    let mut component = mount_sheet();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.wm-keybinds".into(),
            source_module: "@mesh/hyprland-keybinds".into(),
            payload: serde_json::json!({
                "available": true,
                "sections": [
                    section("Shell", &[(&["Super"], "Slash", "Toggle cheatsheet")]),
                    section("Window", &[(&["Super", "Shift"], "F", "Float/Tile")]),
                    section("Media", &[(&[], "Print", "Screenshot")]),
                ],
            }),
        })
        .unwrap();
    let text = paint(&mut component).join("\n");
    for needle in ["Keybinds", "Shell", "Window", "Media", "Toggle cheatsheet", "Float/Tile", "Screenshot", "Super", "Shift", "Print"] {
        assert!(text.contains(needle), "missing {needle:?} in {text}");
    }
    assert!(!text.contains("No described keybinds"), "{text}");
}

#[test]
fn cheatsheet_says_so_when_no_keybinds_were_found() {
    let mut component = mount_sheet();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.wm-keybinds".into(),
            source_module: "@mesh/hyprland-keybinds".into(),
            payload: serde_json::json!({ "available": false, "sections": [] }),
        })
        .unwrap();
    let text = paint(&mut component).join("\n");
    assert!(text.contains("No described keybinds"), "{text}");
}
