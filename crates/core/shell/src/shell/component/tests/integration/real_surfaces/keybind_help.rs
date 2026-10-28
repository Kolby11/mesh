use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::catalog::FrontendCatalog;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_frontend::compile_frontend_module;

fn mount_help() -> FrontendSurfaceComponent {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
        .join("modules/frontend/keybind-help");
    let manifest = mesh_core_module::manifest::load_canonical_manifest(&dir)
        .unwrap()
        .manifest;
    let compiled = compile_frontend_module(&manifest, &dir).unwrap();
    let mut entries = HashMap::new();
    entries.insert(
        "@mesh/keybind-help".to_string(),
        FrontendCatalogEntry {
            module_dir: dir.clone(),
            compiled: compiled.clone().into(),
        },
    );
    let mut interfaces = InterfaceCatalog::default();
    interfaces.register_contract(
        mesh_core_service::contract::parse_interface_contract(
            "mesh.keybinds",
            "1.0",
            &serde_json::json!({
                "state": { "entries": "any", "revision": "string" },
                "methods": { "remap": "(surface: string, action: string, key: string)" }
            }),
        )
        .unwrap(),
    );
    interfaces.register_provider(mesh_core_service::InterfaceProvider {
        interface: "mesh.keybinds".into(),
        version: Some("1.0".into()),
        base_module: None,
        provider_module: "@mesh/shell".into(),
        backend_name: "Shell Keybinds".into(),
        priority: 200,
    });
    let mut locale = mesh_core_locale::LocaleEngine::new("en");
    let catalog: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("config/i18n/en.json")).unwrap(),
    )
    .unwrap();
    locale.load_module_catalog(
        "@mesh/keybind-help",
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
    .with_instance_id("@mesh/keybind-help");
    component.locale = locale.snapshot();
    component
        .mount(ComponentContext {
            component_id: "@mesh/keybind-help".into(),
            surface_id: "@mesh/keybind-help".into(),
            diagnostics: Diagnostics::new("@mesh/keybind-help".to_string()),
        })
        .unwrap();
    component.visible = true;
    component
}

fn entry(surface: &str, action: &str, label: &str, key: &str, scope: &str, focused: bool) -> serde_json::Value {
    serde_json::json!({
        "surface_id": surface, "module_id": surface, "action_id": action,
        "label": label, "description": format!("{label} description"),
        "key": key, "modifiers": [], "trigger_kind": "shortcut", "source": "module_default",
        "accessibility_shortcut": key, "scope": scope, "active": true, "focused": focused
    })
}

fn paint(component: &mut FrontendSurfaceComponent) -> Vec<String> {
    let mut pixels = PixelBuffer::new(900, 700);
    component
        .paint_raster(&default_theme(), SurfaceExtent::unpadded(900, 700), &mut pixels, 1.0)
        .unwrap();
    rendered_text(component)
}

#[test]
fn help_lists_the_focused_surface_first_then_shell_then_the_rest() {
    let mut component = mount_help();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.keybinds".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "revision": "1",
                "entries": [
                    entry("@mesh/glass-bar", "dash", "Open dashboard", "d", "surface", false),
                    entry("@mesh/glass-dock", "find", "Find an app", "f", "surface", true),
                    entry("shell", "help", "Open key help", "F1", "shell", false),
                ],
            }),
        })
        .unwrap();
    let text = paint(&mut component).join("\n");
    let position = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("missing {needle:?} in {text}"));
    assert!(position("Here · glass-dock") < position("Shell"));
    assert!(position("Shell") < position("glass-bar"));
    assert!(position("Find an app") < position("Open key help"));
    assert!(text.contains("Open dashboard description"));
}

#[test]
fn help_search_filters_rows_by_name_and_key() {
    let mut component = mount_help();
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.keybinds".into(),
            source_module: "@mesh/shell".into(),
            payload: serde_json::json!({
                "revision": "1",
                "entries": [
                    entry("@mesh/glass-bar", "dash", "Open dashboard", "d", "surface", false),
                    entry("@mesh/glass-dock", "find", "Find an app", "f", "surface", false),
                ],
            }),
        })
        .unwrap();
    paint(&mut component);
    component.focused_key = Some(
        first_node_with_ref(component.last_tree.as_ref().unwrap(), "search")
            .expect("search input")
            .mesh_key()
            .unwrap()
            .to_owned(),
    );
    component
        .handle_input(&default_theme(), 900, 700, ComponentInput::TextInput { text: "dash".into() })
        .unwrap();
    let text = paint(&mut component).join("\n");
    assert!(text.contains("Open dashboard"), "{text}");
    assert!(!text.contains("Find an app"), "{text}");
}
