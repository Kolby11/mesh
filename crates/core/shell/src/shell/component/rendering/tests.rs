use super::layer_surface_request_size;
use mesh_core_wayland::Edge;

#[test]
fn floating_side_surface_keeps_its_measured_height() {
    assert_eq!(
        layer_surface_request_size(Edge::Right, 0, (380, 220)),
        (380, 220)
    );
}

// The navigation bar embeds theme-selector and language-popover through its
// slot contributions, and those import bubble-options. Restyle (hover,
// animation frames) must keep that second-level module's rules, or its nodes
// fall back to theme defaults: grey padded buttons with the hidden icon shown.
#[test]
fn restyle_rules_follow_component_imports_transitively() {
    use super::super::FrontendSurfaceComponent;
    use super::super::catalog::FrontendCatalog;
    use mesh_core_module::lifecycle::ModuleInstance;
    use std::collections::HashMap;
    use std::path::PathBuf;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph =
        mesh_core_module::package::load_authoring_snapshot(&root.join("config/module.json"))
            .expect("shipped graph loads");
    let modules: HashMap<String, ModuleInstance> = std::fs::read_dir(root.join("modules/frontend"))
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let loaded = mesh_core_module::manifest::load_canonical_manifest(&entry.path()).ok()?;
            Some((
                loaded.manifest.package.id.clone(),
                ModuleInstance::new(loaded.manifest, entry.path(), loaded.path, loaded.source),
            ))
        })
        .collect();
    let catalog = FrontendCatalog::from_modules(&modules, Some(&graph)).unwrap();
    let navigation = catalog
        .module("@mesh/navigation-bar")
        .expect("navigation bar compiles")
        .clone();
    let mut component = FrontendSurfaceComponent::new_for_test(
        navigation.compiled,
        navigation.module_dir,
        catalog,
        mesh_core_service::ResolvedServiceCatalog::default(),
        std::sync::Arc::new(mesh_core_config::SettingsStore::default()),
    );

    let has_rule = |component: &mut FrontendSurfaceComponent, class: &str| {
        component
            .module_restyle_rules()
            .iter()
            .any(|rule| format!("{:?}", rule.selector).contains(class))
    };
    assert!(
        has_rule(&mut component, "language-popover-shell"),
        "direct contribution imports are restyled"
    );
    assert!(
        has_rule(&mut component, "bubble-option-hidden"),
        "bubble-options, imported by the popovers, must be restyled too"
    );
}
