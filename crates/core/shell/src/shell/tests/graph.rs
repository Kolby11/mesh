use super::common::*;
use super::*;
use std::fs;

#[test]
fn installed_module_graph_exposes_shell_package_choices() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph = mesh_core_module::package::load_installed_module_graph(
        &workspace_root.join("config/module.json"),
    )
    .unwrap();

    assert_eq!(
        graph.declared_interface("mesh.audio").unwrap().module_id,
        "@mesh/audio-interface"
    );
    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
    assert!(
        graph
            .backend_providers_for_interface("mesh.audio")
            .iter()
            .any(|provider| provider.module_id == "@mesh/pulseaudio-audio")
    );
    assert!(
        graph
            .icon_pack_contributions()
            .iter()
            .any(|icon_pack| icon_pack.module_id == "@mesh/icons-default")
    );
    assert!(graph.active_provider("mesh.network").is_none());
    assert_eq!(
        graph.active_provider("mesh.power").unwrap().module_id,
        "@mesh/upower-power"
    );
    assert_eq!(
        graph.active_provider("mesh.brightness").unwrap().module_id,
        "@mesh/backlight-brightness"
    );
    assert_eq!(
        graph.backend_providers_for_interface("mesh.network").len(),
        0
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.power").len(), 1);
    assert_eq!(
        graph
            .backend_providers_for_interface("mesh.brightness")
            .len(),
        1
    );
    assert_eq!(
        graph.declared_interface("mesh.device").unwrap().module_id,
        "@mesh/device-interface"
    );
    assert_eq!(
        graph.active_provider("mesh.device").unwrap().module_id,
        "@mesh/device-info"
    );
    assert_eq!(
        graph.backend_providers_for_interface("mesh.device").len(),
        1
    );
    assert!(
        graph
            .frontend_modules()
            .iter()
            .any(|module| module.id == "@mesh/navigation-bar")
    );
    assert!(
        graph
            .frontend_modules()
            .iter()
            .all(|module| module.id != "@mesh/text-selection-proof")
    );
    assert_eq!(
        graph.module("@mesh/text-selection-proof").unwrap().enabled,
        false
    );

    let layout = graph.layout_entrypoint().unwrap();
    assert_eq!(layout.module_id, "@mesh/navigation-bar");
    assert_eq!(layout.entrypoint_id, "main");

    let mut shell = Shell::new();
    shell.register_interfaces_from_graph(&graph);
    let contracts = shell.interfaces.contracts_for("mesh.audio");
    assert!(contracts.iter().any(|contract| {
        contract.interface == "mesh.audio"
            && contract
                .state_fields
                .iter()
                .any(|field| field.name == "available")
    }));
    let providers = shell.interfaces.providers_for("mesh.audio");
    assert_eq!(providers.len(), 2);
    assert!(providers.iter().any(|provider| {
        provider.provider_module == "@mesh/pipewire-audio"
            && provider.backend_name == "pipewire"
            && provider.base_module.as_deref() == Some("@mesh/audio-interface")
    }));
    assert!(providers.iter().any(|provider| {
        provider.provider_module == "@mesh/pulseaudio-audio"
            && provider.backend_name == "pulseaudio"
            && provider.base_module.as_deref() == Some("@mesh/audio-interface")
    }));
}

#[test]
fn installed_graph_registers_each_module_settings_owner_before_consumption() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph = mesh_core_module::package::load_installed_module_graph(
        &workspace_root.join("config/module.json"),
    )
    .unwrap();
    let mut shell = Shell::new();

    shell.register_interfaces_from_graph(&graph);

    for module in graph.modules() {
        assert_eq!(
            shell
                .settings_store
                .schema_registry()
                .get(&module.id)
                .map(|schema| schema.owner.as_str()),
            Some(module.id.as_str()),
            "module {} must register its own settings namespace",
            module.id
        );
    }
}

#[test]
fn load_frontend_components_keeps_shell_shipped_debug_inspector_even_when_not_in_package_graph() {
    let mut shell = Shell::new();
    shell.discover_modules();
    shell.resolve_modules().unwrap();
    shell.load_frontend_components().unwrap();

    assert!(
        shell
            .components
            .iter()
            .any(|runtime| runtime.surface_id == "@mesh/debug-inspector"),
        "built-in debug inspector should load as a shell surface even when absent from config/module.json"
    );
}

#[test]
fn missing_active_profile_is_explicit_legacy_composition() {
    let root = tempfile::tempdir().unwrap();
    let graph_path = root.path().join("module.json");
    let (mode, profile) = startup_composition(&graph_path);
    assert_eq!(mode, ShellCompositionMode::LegacyNoProfile);
    assert_eq!(mode.service_name(), "legacy_no_profile");
    assert!(profile.is_none());
}

#[test]
fn valid_active_profile_is_explicit_configured_composition() {
    let root = tempfile::tempdir().unwrap();
    let graph_path = root.path().join("module.json");
    let paths = mesh_core_module::package::ProfilePaths::from_root_graph(&graph_path).unwrap();
    fs::create_dir_all(paths.profiles_dir()).unwrap();
    fs::write(
        paths.profile_path("default").unwrap(),
        serde_json::to_vec(&mesh_core_module::package::ShellProfile::new()).unwrap(),
    )
    .unwrap();
    fs::write(root.path().join("active-profile"), "default\n").unwrap();

    let (mode, profile) = startup_composition(&graph_path);
    assert_eq!(
        mode,
        ShellCompositionMode::ConfiguredProfile {
            id: "default".into()
        }
    );
    assert_eq!(profile.map(|(id, _)| id).as_deref(), Some("default"));
}

#[test]
fn invalid_configured_profile_enters_recovery_instead_of_legacy_mode() {
    let root = tempfile::tempdir().unwrap();
    let graph_path = root.path().join("module.json");
    fs::write(root.path().join("active-profile"), "missing\n").unwrap();
    let (mode, profile) = startup_composition(&graph_path);
    assert!(matches!(&mode, ShellCompositionMode::Recovery { .. }));
    assert!(profile.is_none());
    assert!(
        mode.recovery_reason()
            .is_some_and(|reason| reason.contains("configured shell profile"))
    );
}

#[test]
fn invalid_initial_graph_enters_explicit_recovery_without_discovering_modules() {
    let root = tempfile::tempdir().unwrap();
    let graph_path = root.path().join("module.json");
    fs::write(&graph_path, "{\"mesh\":").unwrap();

    let mut shell = Shell::new();
    shell.discover_modules_at(&graph_path);

    assert!(matches!(
        shell.composition_mode,
        ShellCompositionMode::Recovery { .. }
    ));
    assert!(shell.installed_module_graph.is_none());
    assert_eq!(shell.modules.len(), 0);
    assert!(
        shell
            .diagnostics
            .snapshot()
            .into_iter()
            .flat_map(|module| module.instances)
            .flat_map(|instance| instance.active_issues)
            .any(|issue| {
                issue.issue_code.contains("configured_composition_recovery")
                    && issue.message.contains("configured shell graph/profile")
            })
    );
}

#[test]
fn invalid_live_graph_keeps_last_known_good_composition_active() {
    let mut shell = Shell::new();
    let last_known_good = graph_from_json(
        r#"{
            "modulesDir": "modules",
            "modules": {
                "@test/old": {
                    "kind": "frontend",
                    "path": "old",
                    "enabled": true
                }
            }
        }"#,
        vec![
            r#"{
                "name": "@test/old",
                "version": "0.1.0",
                "mesh": { "apiVersion": "0.1", "kind": "frontend" }
            }"#,
        ],
    );
    shell.installed_module_graph = Some(last_known_good);

    let root = tempfile::tempdir().unwrap();
    let invalid_graph = root.path().join("module.json");
    fs::write(&invalid_graph, "{\"mesh\":").unwrap();
    shell.discover_modules_at(&invalid_graph);
    assert!(
        shell
            .installed_module_graph
            .as_ref()
            .is_some_and(|graph| graph.module("@test/old").is_some())
    );
}

#[test]
fn invalid_graph_reload_keeps_the_last_known_good_graph() {
    let mut shell = Shell::new();
    let last_known_good = graph_from_json(
        r#"{
            "modulesDir": "modules",
            "modules": {
                "@test/old": {
                    "kind": "frontend",
                    "path": "old",
                    "enabled": true
                }
            }
        }"#,
        vec![
            r#"{
                "name": "@test/old",
                "version": "0.1.0",
                "mesh": { "apiVersion": "0.1", "kind": "frontend" }
            }"#,
        ],
    );
    shell.installed_module_graph = Some(last_known_good);

    let root = tempfile::tempdir().unwrap();
    let invalid_graph = root.path().join("module.json");
    fs::write(&invalid_graph, "{\"mesh\":").unwrap();

    assert!(
        shell
            .reload_installed_module_graph_at(&invalid_graph)
            .is_err()
    );
    assert!(
        shell
            .installed_module_graph
            .as_ref()
            .is_some_and(|graph| graph.module("@test/old").is_some()),
        "a rejected graph candidate must not replace the active graph"
    );
}

fn locale_graph_fixture(root: &std::path::Path) -> InstalledModuleGraph {
    let module_root = root.join("locale-module");
    fs::create_dir_all(module_root.join("config/i18n")).unwrap();
    let manifest = ModuleManifest::from_json_str(
        r#"{
            "name": "@test/locale",
            "version": "0.1.0",
            "mesh": {
                "apiVersion": "0.1",
                "kind": "library",
                "provides": {
                    "i18n": [
                        { "id": "en", "locale": "en", "path": "config/i18n/en.json" }
                    ]
                }
            }
        }"#,
    )
    .unwrap();
    let loaded = LoadedModuleManifest {
        manifest,
        path: module_root.join("module.json"),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    };
    let root_manifest = RootModuleGraphManifest::from_json_str(
        r#"{
            "name": "@mesh/test-config",
            "version": "0.1.0",
            "mesh": {
                "schemaVersion": 1,
                "modulesDir": "modules",
                "modules": {
                    "@test/locale": {
                        "kind": "library",
                        "path": "locale-module",
                        "enabled": true
                    }
                }
            }
        }"#,
    )
    .unwrap();
    InstalledModuleGraph::from_parts(root_manifest, vec![loaded]).unwrap()
}

#[test]
fn graph_locale_commit_replaces_catalog_and_retains_last_known_good_on_failure() {
    let root = tempfile::tempdir().unwrap();
    let catalog_path = root.path().join("locale-module/config/i18n/en.json");
    fs::write(&catalog_path, r#"{ "hello": "Hello" }"#).unwrap();
    let graph = locale_graph_fixture(root.path());

    let mut shell = Shell::new();
    shell.commit_installed_module_graph(graph.clone()).unwrap();
    let revision = shell.locale.catalog_snapshot().revision();
    assert_eq!(
        shell
            .locale
            .module_translator("@test/locale")
            .translate("hello"),
        Some("Hello")
    );

    fs::write(&catalog_path, "{ malformed").unwrap();
    let replacement = locale_graph_fixture(root.path());
    assert!(shell.commit_installed_module_graph(replacement).is_err());
    assert_eq!(shell.locale.catalog_snapshot().revision(), revision);
    assert_eq!(
        shell
            .locale
            .module_translator("@test/locale")
            .translate("hello"),
        Some("Hello")
    );
    assert!(
        shell
            .installed_module_graph
            .as_ref()
            .is_some_and(|active| active.module("@test/locale").is_some())
    );
}

#[test]
fn core_crate_boundaries_do_not_regress() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");

    let frontend = manifest_dependencies(&root.join("crates/core/frontend/compiler/Cargo.toml"));
    assert!(!frontend.contains("mesh-core-shell"));
    assert!(!frontend.contains("mesh-core-render"));
    assert!(!frontend.contains("mesh-core-presentation"));

    let frontend_host = manifest_dependencies(&root.join("crates/core/frontend/host/Cargo.toml"));
    assert!(frontend_host.contains("mesh-core-render"));
    assert!(frontend_host.contains("mesh-core-wayland"));
    assert!(!frontend_host.contains("mesh-core-shell"));
    assert!(!frontend_host.contains("mesh-core-frontend"));
    assert!(!frontend_host.contains("mesh-core-presentation"));

    let animation = manifest_dependencies(&root.join("crates/core/ui/animation/Cargo.toml"));
    assert!(animation.contains("mesh-core-elements"));
    assert!(!animation.contains("mesh-core-shell"));
    assert!(!animation.contains("mesh-core-frontend"));
    assert!(!animation.contains("mesh-core-render"));

    let interaction = manifest_dependencies(&root.join("crates/core/ui/interaction/Cargo.toml"));
    assert!(interaction.contains("mesh-core-elements"));
    assert!(!interaction.contains("mesh-core-shell"));
    assert!(!interaction.contains("mesh-core-render"));
    assert!(!interaction.contains("mesh-core-presentation"));

    let render = manifest_dependencies(&root.join("crates/core/frontend/render/Cargo.toml"));
    assert!(render.contains("mesh-core-elements"));
    assert!(render.contains("mesh-core-icon"));
    assert!(!render.contains("mesh-core-shell"));
    assert!(!render.contains("mesh-core-frontend"));
    assert!(!render.contains("mesh-core-presentation"));

    let presentation = manifest_dependencies(&root.join("crates/core/presentation/Cargo.toml"));
    assert!(presentation.contains("mesh-core-render"));
    assert!(presentation.contains("mesh-core-wayland"));
    assert!(!presentation.contains("mesh-core-shell"));
    assert!(!presentation.contains("mesh-core-frontend"));

    let surface_config = manifest_dependencies(&root.join("crates/core/surface-config/Cargo.toml"));
    assert!(surface_config.contains("mesh-core-module"));
    assert!(surface_config.contains("mesh-core-wayland"));
    assert!(!surface_config.contains("mesh-core-shell"));
    assert!(!surface_config.contains("mesh-core-render"));
}
