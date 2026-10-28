use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::catalog::FrontendCatalog;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_frontend::compile_frontend_module;
use mesh_core_interaction::{find_node_bounds_by_key, pointer_hit_test};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn mount_launcher() -> FrontendSurfaceComponent {
    let root = repo_root();
    let dir = root.join("modules/frontend/radial-launcher");
    let manifest = mesh_core_module::manifest::load_canonical_manifest(&dir)
        .unwrap()
        .manifest;
    let compiled = compile_frontend_module(&manifest, &dir).unwrap();
    let mut entries = HashMap::new();
    entries.insert(
        "@mesh/radial-launcher".to_string(),
        FrontendCatalogEntry {
            module_dir: dir.clone(),
            compiled: compiled.clone().into(),
        },
    );
    let mut interfaces = InterfaceCatalog::default();
    for interface in ["apps", "media"] {
        let path = root
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
            provider_module: "@test/launcher-services".into(),
            backend_name: interface.into(),
            priority: 1,
        });
    }
    let mut locale = mesh_core_locale::LocaleEngine::new("en");
    let catalog: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("config/i18n/en.json")).unwrap(),
    )
    .unwrap();
    locale.load_module_catalog(
        "@mesh/radial-launcher",
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
    .with_instance_id("@mesh/radial-launcher");
    component.locale = locale.snapshot();
    component
        .mount(ComponentContext {
            component_id: "@mesh/radial-launcher".into(),
            surface_id: "@mesh/radial-launcher".into(),
            diagnostics: Diagnostics::new("@mesh/radial-launcher".to_string()),
        })
        .unwrap();
    component.visible = true;
    component
}

#[test]
fn media_wedges_hit_only_their_painted_sector() {
    let (width, height) = (900, 700);
    let theme = default_theme();
    let mut component = mount_launcher();
    let mut pixels = PixelBuffer::new(width, height);
    component
        .paint_raster(&theme, SurfaceExtent::unpadded(width, height), &mut pixels, 1.0)
        .unwrap();
    let tree = component.last_tree.as_ref().unwrap();

    let mut wedges = Vec::new();
    collect_class_token_nodes(tree, "media-wedge", &mut wedges);
    assert_eq!(wedges.len(), 3, "previous, play and next wedges");
    let play = wedges
        .iter()
        .find(|wedge| {
            wedge.attributes.get(Prop::Start).map(String::as_str) == Some("163")
        })
        .expect("play wedge");
    let key = play.mesh_key().unwrap();
    let (left, top, right, bottom) = find_node_bounds_by_key(tree, key, 0.0, 0.0).unwrap();
    let (cx, cy) = ((left + right) / 2.0, (top + bottom) / 2.0);

    let inside = pointer_hit_test(tree, cx, cy + 167.0).expect("hit between the radii");
    assert!(inside.path.iter().any(|hit| hit == key), "{:?}", inside.path);

    let corner = pointer_hit_test(tree, cx + 200.0, cy + 200.0);
    assert!(
        corner.is_none_or(|hit| !hit.path.iter().any(|hit| hit == key)),
        "the box corner is outside the sector"
    );
    let side = pointer_hit_test(tree, cx + 167.0, cy);
    assert!(
        side.is_none_or(|hit| !hit.path.iter().any(|hit| hit == key)),
        "the side of the ring belongs to no media wedge"
    );

    let painted = pixels.get_pixel(cx.round() as u32, (cy + 167.0).round() as u32);
    let hole = pixels.get_pixel((cx + 200.0).round() as u32, (cy + 200.0).round() as u32);
    assert_ne!(painted, hole, "the wedge fills where the corner stays clear");
}

fn app(id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "name": name, "icon": "", "running": false, "focused": false,
        "pinned": true, "windows": [], "window_count": 0, "exec": "", "comment": ""
    })
}

fn set_apps(component: &mut FrontendSurfaceComponent, apps: &[(&str, &str)], installed: &[(&str, &str)]) {
    let list = |items: &[(&str, &str)]| -> Vec<_> { items.iter().map(|(id, name)| app(id, name)).collect() };
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.apps".into(),
            source_module: "@test/launcher-services".into(),
            payload: serde_json::json!({
                "available": true, "apps": list(apps), "installed": list(installed)
            }),
        })
        .unwrap();
}

fn paint_launcher(component: &mut FrontendSurfaceComponent, width: u32, height: u32) {
    let mut pixels = PixelBuffer::new(width, height);
    component
        .paint_raster(&default_theme(), SurfaceExtent::unpadded(width, height), &mut pixels, 1.0)
        .unwrap();
}

fn app_wedges(component: &FrontendSurfaceComponent) -> Vec<&WidgetNode> {
    let mut wedges = Vec::new();
    collect_class_token_nodes(component.last_tree.as_ref().unwrap(), "wedge", &mut wedges);
    wedges
        .into_iter()
        .filter(|wedge| {
            wedge.attributes.get(Prop::Class).is_some_and(|class| !class.contains("media-wedge"))
        })
        .collect()
}

#[test]
fn app_wedges_highlight_under_the_pointer() {
    let (width, height) = (900, 700);
    let theme = default_theme();
    let mut component = mount_launcher();
    set_apps(&mut component, &[("a", "Alpha"), ("b", "Beta"), ("c", "Gamma")], &[]);
    let mut paint_pixels = |component: &mut FrontendSurfaceComponent| {
        let mut pixels = PixelBuffer::new(width, height);
        component
            .paint_raster(&theme, SurfaceExtent::unpadded(width, height), &mut pixels, 1.0)
            .unwrap();
        pixels
    };
    let before = paint_pixels(&mut component);
    let wedges = app_wedges(&component);
    assert_eq!(wedges.len(), 3);
    let key = wedges[0].mesh_key().unwrap().to_owned();
    let tree = component.last_tree.as_ref().unwrap();
    let (left, top, right, bottom) = find_node_bounds_by_key(tree, &key, 0.0, 0.0).unwrap();
    let (cx, cy) = ((left + right) / 2.0, (top + bottom) / 2.0);
    // Off the icon, inside the first wedge: 237 + 246/8/2 degrees clockwise from the top.
    let radians = (237.0f32 + 15.375).to_radians();
    let (x, y) = (cx + 140.0 * radians.sin(), cy - 140.0 * radians.cos());
    let rest = before.get_pixel(x as u32, y as u32);
    component
        .handle_input(&theme, width, height, ComponentInput::PointerMove { x, y })
        .unwrap();
    let after = paint_pixels(&mut component);
    let hovered = find_node_by_key(component.last_tree.as_ref().unwrap(), &key).unwrap();
    assert!(hovered.state.contains(mesh_core_elements::NodeState::HOVERED));
    assert_ne!(after.get_pixel(x as u32, y as u32), rest, "the hovered wedge changes fill");
}

#[test]
fn search_filters_the_ring_to_matching_installed_apps() {
    let (width, height) = (900, 700);
    let mut component = mount_launcher();
    set_apps(
        &mut component,
        &[("a", "Alpha")],
        &[("a", "Alpha"), ("f", "Firefox"), ("fm", "Files"), ("t", "Terminal")],
    );
    paint_launcher(&mut component, width, height);
    assert_eq!(app_wedges(&component).len(), 1, "the ring starts with pinned and running apps");

    component
        .handle_input(
            &default_theme(),
            width,
            height,
            ComponentInput::KeyPressed { key: "f".into(), modifiers: Default::default() },
        )
        .unwrap();
    paint_launcher(&mut component, width, height);
    let due = component.next_script_task_deadline().expect("focus task is scheduled");
    component.run_due_script_tasks(due).unwrap();
    paint_launcher(&mut component, width, height);
    component
        .handle_input(
            &default_theme(),
            width,
            height,
            ComponentInput::TextInput { text: "fi".into() },
        )
        .unwrap();
    paint_launcher(&mut component, width, height);
    let names: Vec<_> = app_wedges(&component)
        .iter()
        .filter_map(|wedge| wedge.attributes.get(Prop::AriaLabel).cloned())
        .collect();
    assert_eq!(names, ["Firefox", "Files"]);
}
