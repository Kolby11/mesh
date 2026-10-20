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
    assert_eq!(
        shell
            .interfaces
            .resolve("mesh.audio", None)
            .provider
            .as_ref()
            .map(|provider| provider.provider_module.as_str()),
        Some("@mesh/pipewire-audio"),
        "resolution must use the graph-selected provider rather than registry priority"
    );
}

#[test]
fn graph_settings_reject_the_same_surface_enum_values_as_surface_config() {
    let graph = graph_from_json(
        r#"{
            "modulesDir": "modules",
            "modules": {
                "@test/frontend": {
                    "kind": "ui",
                    "path": "frontend",
                    "enabled": true
                }
            }
        }"#,
        vec![
            r#"{
                "name": "@test/frontend",
                "version": "0.1.0",
                "mesh": {
                    "apiVersion": "0.1",
                    "kind": "ui",
                    "entry": "src/main.mesh"
                }
            }"#,
        ],
    );
    let mut store = mesh_core_config::SettingsStore::from_value(
        "/tmp/mesh-graph-surface-schema-parity.json",
        serde_json::json!({
            "@test/frontend": {
                "surface": {
                    "role": "popup",
                    "anchor": "botom",
                    "layer": "overly",
                    "keyboard_mode": "exclusive-ish"
                }
            }
        }),
    )
    .unwrap();

    crate::shell::discovery::register_graph_settings_schemas(&mut store, &graph).unwrap();

    assert_eq!(
        store.namespace("@test/frontend")["surface"],
        serde_json::json!({})
    );
    for key in ["role", "anchor", "layer", "keyboard_mode"] {
        assert!(
            store
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.key_path == format!("surface.{key}")),
            "graph schema should diagnose invalid surface.{key}"
        );
    }
}

#[test]
fn graph_interface_catalog_replaces_stale_providers_and_excludes_disabled_candidates() {
    let first = graph_from_json(
        r#"{
            "modulesDir": "modules",
            "modules": {
                "@test/example-interface": {
                    "kind": "interface",
                    "path": "example-interface",
                    "enabled": true
                },
                "@test/old-backend": {
                    "kind": "backend",
                    "path": "old-backend",
                    "enabled": true
                }
            },
            "providers": { "test.example": "@test/old-backend" }
        }"#,
        vec![
            r#"{
                "name": "@test/example-interface",
                "version": "1.0.0",
                "mesh": {
                    "apiVersion": "0.1",
                    "kind": "interface",
                    "interface": {
                        "name": "test.example",
                        "version": "1.0",
                        "domain": "example",
                        "contract": { "methods": [{ "name": "read", "returns": "boolean" }] }
                    }
                }
            }"#,
            r#"{
                "name": "@test/old-backend",
                "version": "1.0.0",
                "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{
                        "interface": "test.example",
                        "version": "1.0",
                        "provider": "old",
                        "priority": 10
                    }]
                }
            }"#,
        ],
    );
    let second = graph_from_json(
        r#"{
            "modulesDir": "modules",
            "modules": {
                "@test/example-interface": {
                    "kind": "interface",
                    "path": "example-interface",
                    "enabled": true
                },
                "@test/old-backend": {
                    "kind": "backend",
                    "path": "old-backend",
                    "enabled": false
                }
            },
            "providers": { "test.example": "@test/old-backend" }
        }"#,
        vec![
            r#"{
                "name": "@test/example-interface",
                "version": "1.0.0",
                "mesh": {
                    "apiVersion": "0.1",
                    "kind": "interface",
                    "interface": {
                        "name": "test.example",
                        "version": "1.0",
                        "domain": "example",
                        "contract": { "methods": [{ "name": "read", "returns": "boolean" }] }
                    }
                }
            }"#,
            r#"{
                "name": "@test/old-backend",
                "version": "1.0.0",
                "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{
                        "interface": "test.example",
                        "version": "1.0",
                        "provider": "old",
                        "priority": 10
                    }]
                }
            }"#,
        ],
    );

    let mut shell = Shell::new();
    shell.commit_installed_module_graph(first).unwrap();
    assert_eq!(
        shell
            .interfaces
            .providers_for("test.example")
            .iter()
            .map(|provider| provider.provider_module.as_str())
            .collect::<Vec<_>>(),
        vec!["@test/old-backend"]
    );

    shell.commit_installed_module_graph(second).unwrap();
    assert!(
        shell.interfaces.providers_for("test.example").is_empty(),
        "a disabled provider from the replacement graph must not inherit the previous registry entry"
    );
    assert!(
        shell
            .interfaces
            .providers_for("mesh.theme")
            .iter()
            .any(|provider| { provider.provider_module == "@mesh/shell" }),
        "core-owned providers must survive graph catalog replacement"
    );
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
                    "kind": "ui",
                    "path": "old",
                    "enabled": true
                }
            }
        }"#,
        vec![
            r#"{
                "name": "@test/old",
                "version": "0.1.0",
                "mesh": { "apiVersion": "0.1", "kind": "ui" }
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
                    "kind": "ui",
                    "path": "old",
                    "enabled": true
                }
            }
        }"#,
        vec![
            r#"{
                "name": "@test/old",
                "version": "0.1.0",
                "mesh": { "apiVersion": "0.1", "kind": "ui" }
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
    fs::create_dir_all(catalog_path.parent().unwrap()).unwrap();
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
fn graph_theme_commit_rejects_bad_css_before_swapping_graph_or_watch() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("theme.css"),
        ":root { --color-surface: #123456; }",
    )
    .unwrap();
    let graph = graph_with_theme_source(dir.path(), "graph-theme", "theme.css");
    let theme_id = "@mesh/test-theme:graph-theme";

    let mut shell = Shell::new();
    shell.settings.theme.active = theme_id.to_string();
    shell.commit_installed_module_graph(graph.clone()).unwrap();
    let previous_watch = shell.theme_watch.clone();
    let previous_watch_set = shell.file_watch_set.clone();
    assert!(
        shell
            .file_watch_set
            .paths
            .iter()
            .any(|path| path.ends_with("theme.css"))
    );

    fs::write(dir.path().join("theme.css"), "{ malformed").unwrap();
    let replacement = graph_with_theme_source(dir.path(), "graph-theme", "theme.css");

    assert!(shell.commit_installed_module_graph(replacement).is_err());
    assert_eq!(shell.theme.active().id, theme_id);
    assert_eq!(
        shell
            .theme
            .active()
            .token("color.surface")
            .map(ToString::to_string),
        Some("#123456".into())
    );
    assert_eq!(shell.theme_watch.path, previous_watch.path);
    assert_eq!(shell.theme_watch.fingerprint, previous_watch.fingerprint);
    assert_eq!(shell.file_watch_set, previous_watch_set);
    assert!(
        shell
            .installed_module_graph
            .as_ref()
            .is_some_and(|active| active.theme_catalog().get(theme_id).is_some())
    );
}

#[test]
fn graph_theme_descriptor_changes_enter_the_activation_diff() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("theme.css"), ":root {} ").unwrap();
    let before = graph_with_theme_source(dir.path(), "before", "theme.css");
    let after = graph_with_theme_source(dir.path(), "after", "theme.css");

    let diff = before.diff(&after);

    assert!(
        diff.profile_effects
            .iter()
            .any(|effect| effect == "themes changed"),
        "theme descriptor changes must not be treated as an empty graph diff: {diff:?}"
    );
}

#[test]
fn core_crate_boundaries_do_not_regress() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");

    let frontend = manifest_dependencies(&root.join("crates/core/frontend/compiler/Cargo.toml"));
    assert!(!frontend.contains("mesh-core-shell"));
    assert!(!frontend.contains("mesh-core-render"));
    assert!(!frontend.contains("mesh-core-presentation"));

    let frontend_abi = manifest_dependencies(&root.join("crates/core/frontend/abi/Cargo.toml"));
    assert!(frontend_abi.contains("mesh-core-capability"));
    assert!(!frontend_abi.contains("mesh-core-render"));
    assert!(!frontend_abi.contains("mesh-core-wayland"));
    assert!(!frontend_abi.contains("mesh-core-debug"));
    assert!(!frontend_abi.contains("mesh-core-config"));
    assert!(!frontend_abi.contains("mesh-core-shell"));
    assert!(!frontend_abi.contains("mesh-core-presentation"));

    let frontend_adapter =
        manifest_dependencies(&root.join("crates/core/frontend/shell-adapter/Cargo.toml"));
    assert!(frontend_adapter.contains("mesh-core-frontend-abi"));
    assert!(frontend_adapter.contains("mesh-core-render"));
    assert!(frontend_adapter.contains("mesh-core-wayland"));
    assert!(frontend_adapter.contains("mesh-core-debug"));
    assert!(frontend_adapter.contains("mesh-core-config"));
    assert!(!frontend_adapter.contains("mesh-core-presentation"));

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

#[test]
fn one_module_with_unresolvable_capabilities_does_not_withhold_grants_from_the_rest() {
    let graph = graph_from_json(
        r#"{
            "modulesDir": "modules",
            "modules": {
                "@test/good": { "kind": "backend", "path": "@test/good", "enabled": true },
                "@test/bad": { "kind": "backend", "path": "@test/bad", "enabled": true }
            }
        }"#,
        vec![
            r#"{ "name": "@test/good", "version": "0.1.0",
                 "mesh": { "apiVersion": "0.1", "kind": "backend", "entry": "main.luau" } }"#,
            r#"{ "name": "@test/bad", "version": "0.1.0",
                 "mesh": { "apiVersion": "0.1", "kind": "backend", "entry": "main.luau" } }"#,
        ],
    );
    let (_good_dir, mut good) = module_instance("@test/good", None);
    good.manifest.capabilities.optional = vec!["net.http".into()];
    let (_bad_dir, mut bad) = module_instance("@test/bad", None);
    bad.manifest.capabilities.required = vec!["service.nonexistent.read".into()];
    let modules = HashMap::from([
        ("@test/good".to_string(), good),
        ("@test/bad".to_string(), bad),
    ]);

    let (granted, failures) = super::super::profile::candidate_capabilities(
        &mesh_core_capability::CapabilityPolicy::default(),
        &graph,
        &modules,
    );

    assert!(granted.contains_key("@test/good"));
    assert!(!granted.contains_key("@test/bad"));
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "@test/bad");
    assert!(
        failures[0]
            .1
            .to_string()
            .contains("service.nonexistent.read")
    );
}

#[test]
fn live_reload_keeps_a_module_whose_manifest_is_mid_edit() {
    let root = tempfile::tempdir().unwrap();
    let module_dir = root.path().join("modules/widget");
    fs::create_dir_all(module_dir.join("src")).unwrap();
    fs::write(
        module_dir.join("src/main.mesh"),
        "<template><box /></template>",
    )
    .unwrap();
    let manifest = module_dir.join("module.json");
    fs::write(
        &manifest,
        r#"{"name":"@test/widget","version":"0.1.0",
            "mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh"}}"#,
    )
    .unwrap();
    let graph_path = root.path().join("module.json");
    fs::write(
        &graph_path,
        r#"{"name":"@test/config","version":"0.1.0",
            "mesh":{"schemaVersion":1,"modulesDir":"modules","modules":{}}}"#,
    )
    .unwrap();

    let mut shell = Shell::new();
    shell.discover_modules_at(&graph_path);
    assert!(shell.modules.contains_key("@test/widget"));

    // Saving a half-typed manifest must not deactivate the module.
    fs::write(&manifest, r#"{"name":"@test/widget","version":"#).unwrap();
    let reloaded = shell
        .reload_installed_module_graph_at(&graph_path)
        .expect("a module mid-edit must not reject the whole graph");
    assert!(reloaded.module("@test/widget").is_some());
    assert!(reloaded.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@test/widget" && diagnostic.status == "module_manifest_retained"
    }));
}

fn composition_switch_fixture(dir: &Path) {
    let modules = dir.join("modules");
    let theme = modules.join("theme");
    fs::create_dir_all(&theme).unwrap();
    fs::write(
        theme.join("module.json"),
        serde_json::json!({
            "name": "@test/theme", "version": "1.0.0",
            "mesh": {"apiVersion": "0.1", "kind": "theme", "provides": {"themes": [{
                "id": "fixture", "label": "Fixture", "modes": {"default": "theme.css"},
                "default_mode": "default"
            }]}}
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        theme.join("theme.css"),
        ":root { --color-background: #123456; }",
    )
    .unwrap();
    let base = modules.join("base");
    fs::create_dir_all(&base).unwrap();
    fs::write(
        base.join("module.json"),
        serde_json::json!({
            "name": "@test/base", "version": "1.0.0",
            "mesh": {"apiVersion": "0.1", "kind": "composition", "compose": {
                "resources": {"theme": "@test/theme"},
                "settings": {"shell": {"render": {"blur": {"passes": 3}}}}
            }}
        })
        .to_string(),
    )
    .unwrap();
    for name in ["first", "second"] {
        let frontend = modules.join(name);
        fs::create_dir_all(&frontend).unwrap();
        fs::write(
            frontend.join("module.json"),
            serde_json::json!({
                "name": format!("@test/{name}"), "version": "1.0.0",
                "mesh": {"apiVersion": "0.1", "kind": "ui", "entry": "main.mesh",
                    "surface": {"role": "layer", "width": 200, "height": 40}}
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            frontend.join("main.mesh"),
            "<template><box><text>Shell</text></box></template>",
        )
        .unwrap();
        let composition = modules.join(format!("{name}-shell"));
        fs::create_dir_all(&composition).unwrap();
        fs::write(
            composition.join("module.json"),
            serde_json::json!({
                "name": format!("@test/{name}-shell"), "version": "1.0.0",
                "mesh": {"apiVersion": "0.1", "kind": "composition", "extends": "@test/base", "compose": {
                    "roots": {format!("@test/{name}#default"): {"module": format!("@test/{name}")}},
                    "settings": {"shell": {"fonts": {"ui_family": name}}}
                }}
            })
            .to_string(),
        )
        .unwrap();
    }
    fs::write(
        dir.join("module.json"),
        serde_json::json!({"mesh": {
            "schemaVersion": 1, "modulesDir": "modules", "modules": {
                "@test/theme": {"kind": "theme", "path": "theme"},
                "@test/base": {"kind": "composition", "path": "base"},
                "@test/first": {"kind": "ui", "path": "first"},
                "@test/second": {"kind": "ui", "path": "second"},
                "@test/first-shell": {"kind": "composition", "path": "first-shell"},
                "@test/second-shell": {"kind": "composition", "path": "second-shell"}
            }
        }})
        .to_string(),
    )
    .unwrap();
    fs::create_dir_all(dir.join("profiles")).unwrap();
    for name in ["first", "second"] {
        fs::write(
            dir.join(format!("profiles/{name}.json")),
            serde_json::json!({
                "schemaVersion": 3, "revision": 7,
                "from": {"module": format!("@test/{name}-shell")}
            })
            .to_string(),
        )
        .unwrap();
    }
    fs::write(dir.join("active-profile"), "first\n").unwrap();
}

#[test]
fn composition_startup_and_live_switch_have_identical_roots_and_settings() {
    let dir = tempfile::tempdir().unwrap();
    let (_settings, _graph, _lock) = isolated_settings_file(dir.path());
    composition_switch_fixture(dir.path());
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    shell.discover_modules();
    shell.resolve_modules().unwrap();
    shell.load_frontend_components().unwrap();
    let mut requests = shell.mount_components().unwrap();
    shell.drain_requests(&mut requests).unwrap();
    assert_eq!(shell.components.len(), 1);
    assert_eq!(shell.components[0].surface_id, "@test/first#default");
    assert_eq!(shell.settings.fonts.ui_family.as_deref(), Some("first"));
    assert_eq!(
        shell.settings_store.to_value()["shell"]["theme"]["active"],
        "@test/theme"
    );
    assert_eq!(
        shell.settings_store.to_value()["shell"]["render"]["blur"]["passes"],
        3
    );
    let initial_settings = shell.settings_store.to_value();
    for (generation, name, previous) in [(1, "second", "first"), (2, "first", "second")] {
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let mut requests = shell.apply_switch_profile_with_ack(name, Some(sender));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            shell.drain_requests(&mut requests).unwrap();
            if shell.pending_resource_preparation.is_none()
                && shell.pending_profile_switch.is_none()
            {
                break;
            }
            assert!(Instant::now() < deadline, "profile switch did not finish");
            std::thread::sleep(Duration::from_millis(1));
            requests.extend(shell.poll_pending_resource_preparation());
        }
        let acknowledgement = receiver.try_recv().unwrap();
        assert!(
            matches!(
                acknowledgement,
                super::super::types::IpcProfileSwitchResponse::Committed { .. }
            ),
            "{acknowledgement:?}"
        );
        assert_eq!(shell.components.len(), 1);
        assert_eq!(
            shell.components[0].surface_id,
            format!("@test/{name}#default")
        );
        assert!(
            shell
                .presentation_engine
                .testing_destroyed_surfaces()
                .contains(&format!("@test/{previous}#default"))
        );
        let snapshot = shell.active_snapshot().unwrap();
        assert_eq!(snapshot.generation(), generation);
        assert_eq!(snapshot.profile_revision(), Some(7));
        assert_eq!(
            snapshot.roots().get(&format!("@test/{name}#default")),
            Some(&format!("@test/{name}"))
        );
        assert_eq!(shell.settings.fonts.ui_family.as_deref(), Some(name));
        let mut restarted = Shell::new();
        restarted.discover_modules();
        restarted.resolve_modules().unwrap();
        restarted.load_frontend_components().unwrap();
        assert_eq!(restarted.components.len(), 1);
        assert_eq!(
            restarted.components[0].surface_id,
            shell.components[0].surface_id
        );
        assert_eq!(
            restarted.settings_store.to_value(),
            shell.settings_store.to_value()
        );
    }
    shell.settings_watch.modified_at = None;
    shell.next_shell_settings_reload_check = Instant::now();
    let mut requests = shell.reload_locale_if_settings_changed().unwrap();
    shell.drain_requests(&mut requests).unwrap();
    assert_eq!(shell.settings_store.to_value(), initial_settings);
    fs::write(
        dir.path().join("profiles/broken.json"),
        serde_json::json!({
            "schemaVersion": 3, "from": {"module": "@test/missing"}
        })
        .to_string(),
    )
    .unwrap();
    let (sender, mut receiver) = tokio::sync::oneshot::channel();
    shell.apply_switch_profile_with_ack("broken", Some(sender));
    assert!(matches!(
        receiver.try_recv().unwrap(),
        super::super::types::IpcProfileSwitchResponse::Rejected { .. }
    ));
    assert_eq!(shell.active_profile_id.as_deref(), Some("first"));
    assert_eq!(shell.components[0].surface_id, "@test/first#default");
    assert_eq!(shell.settings_store.to_value(), initial_settings);
    assert_eq!(
        fs::read_to_string(dir.path().join("active-profile"))
            .unwrap()
            .trim(),
        "first"
    );
    let paths =
        mesh_core_module::package::ProfilePaths::from_root_graph(&dir.path().join("module.json"))
            .unwrap();
    assert!(paths.load("first").unwrap().roots.is_empty());
    assert!(paths.load("second").unwrap().settings.is_empty());
}

#[test]
fn composition_startup_preserves_user_deltas_and_theme_precedence() {
    let dir = tempfile::tempdir().unwrap();
    composition_switch_fixture(dir.path());
    let graph_path = dir.path().join("module.json");
    let paths = mesh_core_module::package::ProfilePaths::from_root_graph(&graph_path).unwrap();
    let profile_path = paths.profile_path("first").unwrap();
    fs::write(
        &profile_path,
        serde_json::json!({
            "schemaVersion": 3, "revision": 9,
            "from": {"module": "@test/first-shell"},
            "roots": {"@test/first#default": {"active": false}},
            "settings": {"shell": {"fonts": {"ui_family": "user"}}}
        })
        .to_string(),
    )
    .unwrap();
    let (mode, profile) = startup_composition(&graph_path);
    assert!(matches!(
        mode,
        ShellCompositionMode::ConfiguredProfile { .. }
    ));
    let (_, mut profile) = profile.unwrap();
    assert_eq!(profile.revision, 9);
    assert!(!profile.roots["@test/first#default"].is_active());
    let shared = mesh_core_config::SettingsStore::from_value(
        dir.path().join("settings.json"),
        serde_json::json!({"shell": {"theme": {"active": "user-theme"}}}),
    )
    .unwrap();
    let effective =
        super::super::discovery::effective_profile_settings(shared.clone(), Some(&profile))
            .unwrap();
    assert_eq!(effective.to_value()["shell"]["fonts"]["ui_family"], "user");
    assert_eq!(effective.to_value()["shell"]["render"]["blur"]["passes"], 3);
    assert_eq!(
        effective.to_value()["shell"]["theme"]["active"],
        "user-theme"
    );
    profile.settings.insert(
        "shell".into(),
        serde_json::json!({"theme": {"active": "pinned-theme"}}),
    );
    let pinned =
        super::super::discovery::effective_profile_settings(shared, Some(&profile)).unwrap();
    assert_eq!(
        pinned.to_value()["shell"]["theme"]["active"],
        "pinned-theme"
    );
}

#[test]
fn unavailable_startup_composition_enters_recovery() {
    let dir = tempfile::tempdir().unwrap();
    composition_switch_fixture(dir.path());
    fs::remove_file(dir.path().join("modules/first-shell/module.json")).unwrap();
    let (mode, profile) = startup_composition(&dir.path().join("module.json"));
    assert!(matches!(mode, ShellCompositionMode::Recovery { .. }));
    assert!(profile.is_none());
}
