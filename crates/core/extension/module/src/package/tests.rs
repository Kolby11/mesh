use super::*;
use crate::manifest::CapabilitiesSection;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct EnvGuard {
    key: &'static str,
    old: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let old = std::env::var(key).ok();
        unsafe {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        Self { key, old }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mesh-{name}-{nonce}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn module_package_paths_default_to_dot_mesh() {
    let _guard = EnvGuard::set("MESH_HOME", None);
    let path = root_package_manifest_path().unwrap();
    assert!(path.ends_with(".mesh/package.json"));
}

#[test]
fn module_package_paths_reject_relative_mesh_home() {
    let _guard = EnvGuard::set("MESH_HOME", Some("relative/path"));
    assert!(matches!(
        mesh_home(),
        Err(PackageManifestError::InvalidMeshHome(_))
    ));
}

#[test]
fn module_root_manifest_parses_minimal_package_json() {
    let content = r#"
{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "private": true,
  "mesh": {
  "schemaVersion": 1,
  "modulesDir": "modules",
  "modules": {},
  "providers": {},
  "layout": { "entrypoint": "@mesh/panel:main" },
  "theme": { "active": "@mesh/default-theme", "mode": "dark" }
  }
}
"#;
    let manifest = RootPackageManifest::from_json_str(content).unwrap();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.modules_dir, "modules");
    assert_eq!(
        manifest.layout.unwrap().entrypoint.as_str(),
        "@mesh/panel:main"
    );
}

#[test]
fn module_root_manifest_accepts_legacy_top_level_shape() {
    let content = r#"
{
  "schemaVersion": 1,
  "modulesDir": "modules",
  "modules": {},
  "providers": {},
  "layout": { "entrypoint": "@mesh/panel:main" }
}
"#;
    let manifest = RootPackageManifest::from_json_str(content).unwrap();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.modules_dir, "modules");
    assert_eq!(
        manifest.layout.unwrap().entrypoint.as_str(),
        "@mesh/panel:main"
    );
}

#[test]
fn module_package_manifest_parses_backend_package_json() {
    let content = r#"
{
  "name": "@mesh/pipewire-audio",
  "version": "0.1.0",
  "repository": {
    "type": "git",
    "url": "git+https://example.invalid/pipewire-audio.git"
  },
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "capabilities": { "required": ["exec.wpctl"] },
    "i18n": { "defaultLocale": "en", "supportedLocales": ["en", "sk"] },
    "dependencies": {
      "binaries": [{ "name": "wpctl", "reason": "PipeWire control" }]
    },
    "entrypoints": { "main": "src/main.luau" },
    "implements": [
      { "interface": "mesh.audio", "version": "1.0", "baseModule": "@mesh/audio-interface", "provider": "pipewire", "label": "PipeWire", "priority": 100 }
    ]
  }
}
"#;
    let manifest = ModulePackageManifest::from_json_str(content).unwrap();
    assert_eq!(manifest.name, "@mesh/pipewire-audio");
    assert_eq!(manifest.mesh.kind, ModuleKind::Backend);
    assert_eq!(
        manifest.mesh.entrypoints.main.as_deref(),
        Some("src/main.luau")
    );
    assert_eq!(
        manifest.repository.unwrap().url,
        "git+https://example.invalid/pipewire-audio.git"
    );
    assert_eq!(manifest.mesh.capabilities.required, vec!["exec.wpctl"]);
    assert_eq!(manifest.mesh.dependencies.binaries[0].name, "wpctl");
    assert_eq!(manifest.mesh.i18n.default_locale.as_deref(), Some("en"));
    assert_eq!(manifest.mesh.i18n.supported_locales, vec!["en", "sk"]);
    assert_eq!(
        manifest.mesh.implements[0].base_module.as_deref(),
        Some("@mesh/audio-interface")
    );
}

#[test]
fn module_package_manifest_parses_interface_relationship_metadata() {
    let content = r#"
{
  "name": "@alice/audio-streams-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "alice.audio-streams",
      "version": "1.0",
      "file": "interface.toml",
      "domain": "audio",
      "extends": "mesh.audio",
      "relationship": "extension"
    }
  }
}
"#;
    let manifest = ModulePackageManifest::from_json_str(content).unwrap();
    let interface = manifest.mesh.interface.unwrap();
    assert_eq!(interface.name, "alice.audio-streams");
    assert_eq!(interface.domain.as_deref(), Some("audio"));
    assert_eq!(interface.extends.as_deref(), Some("mesh.audio"));
    assert_eq!(
        interface.relationship,
        Some(InterfaceRelationship::Extension)
    );
}

#[test]
fn module_package_manifest_rejects_empty_git_origin_url() {
    let content = r#"
{
  "name": "@mesh/bad",
  "version": "0.1.0",
  "repository": { "type": "git", "url": "" },
  "mesh": { "apiVersion": "0.1", "kind": "backend" }
}
"#;
    assert!(ModulePackageManifest::from_json_str(content).is_err());
}

#[test]
fn module_package_manifest_parses_frontend_theme_contributions() {
    let content = r##"
{
  "name": "@mesh/weather",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "theme": {
      "tokens": {
        "weather.color.sunny": "#f6b73c"
      },
      "defaults": {
        "components": {
          "base": {
            "transition": "background-color token(animation.duration.short) token(animation.curves.bezier.standard)"
          },
          "button": {
            "background": "token(@mesh/weather.weather.color.sunny)"
          }
        }
      }
    }
  }
}
"##;
    let manifest = ModulePackageManifest::from_json_str(content).unwrap();
    let theme = manifest.mesh.theme.as_ref().expect("mesh.theme section");
    assert_eq!(
        theme
            .tokens
            .get("weather.color.sunny")
            .map(ToString::to_string)
            .as_deref(),
        Some("#f6b73c")
    );
    assert_eq!(
        theme.defaults.components["button"]["background"],
        "token(@mesh/weather.weather.color.sunny)"
    );

    let runtime = manifest.into_runtime_manifest();
    let runtime_theme = runtime.theme.expect("runtime theme");
    assert_eq!(
        runtime_theme
            .tokens
            .get("weather.color.sunny")
            .map(ToString::to_string)
            .as_deref(),
        Some("#f6b73c")
    );
}

#[test]
fn module_package_manifest_rejects_non_frontend_theme_contributions() {
    let content = r##"
{
  "name": "@mesh/bad-theme-backend",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "theme": {
      "tokens": {
        "bad.color.token": "#000000"
      }
    }
  }
}
"##;
    assert!(ModulePackageManifest::from_json_str(content).is_err());
}

#[test]
fn module_manifest_loader_prefers_package_json_over_module_json() {
    let dir = temp_dir("module-precedence");
    fs::write(
        dir.join("package.json"),
        r#"{"name":"@mesh/package","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend"}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("module.json"),
        r#"{"id":"@mesh/module","version":"0.1.0","type":"surface","api_version":"0.1"}"#,
    )
    .unwrap();
    let loaded = load_module_manifest(&dir).unwrap();
    assert_eq!(loaded.source, ModuleManifestSource::PackageJson);
    assert_eq!(loaded.manifest.name, "@mesh/package");
}

#[test]
fn module_manifest_loader_accepts_legacy_module_json() {
    let dir = temp_dir("legacy-module");
    fs::write(
        dir.join("module.json"),
        r#"{"id":"@mesh/module","version":"0.1.0","type":"surface","api_version":"0.1","entrypoints":{"main":"src/main.mesh"}}"#,
    )
    .unwrap();
    let loaded = load_module_manifest(&dir).unwrap();
    assert_eq!(loaded.source, ModuleManifestSource::LegacyModuleJson);
    assert_eq!(loaded.manifest.name, "@mesh/module");
}

#[test]
fn module_manifest_loader_preserves_legacy_navigation_bar_entrypoint() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/frontend/navigation-bar");
    let loaded = load_module_manifest(&dir).unwrap();
    assert_eq!(loaded.source, ModuleManifestSource::LegacyModuleJson);
    assert_eq!(loaded.manifest.name, "@mesh/navigation-bar");
    assert_eq!(
        loaded.manifest.mesh.entrypoints.main.as_deref(),
        Some("src/main.mesh")
    );
}

#[test]
fn installed_module_graph_loads_repo_package_fixture() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let graph = load_installed_module_graph(&workspace_root.join("config/package.json")).unwrap();

    assert_eq!(graph.frontend_modules().len(), 2);
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
    let layout = graph.layout_entrypoint().unwrap();
    assert_eq!(layout.module_id, "@mesh/navigation-bar");
    assert_eq!(layout.entrypoint_id, "main");
}

fn loaded_module(
    name: &str,
    kind: ModuleKind,
    dependencies: MeshDependencies,
    provides: Vec<MeshProvidesDeclaration>,
    contributes: MeshContributes,
) -> LoadedModuleManifest {
    LoadedModuleManifest {
        manifest: ModulePackageManifest {
            name: name.into(),
            version: "0.1.0".into(),
            description: None,
            license: None,
            repository: None,
            mesh: MeshModuleSection {
                api_version: "0.1".into(),
                kind,
                capabilities: CapabilitiesSection::default(),
                i18n: MeshI18nSupport::default(),
                entrypoints: MeshEntrypoints::default(),
                keybinds: crate::manifest::KeybindsSection::default(),
                dependencies,
                provides,
                implements: Vec::new(),
                interface: None,
                contributes,
                icons: None,
                icon_pack: None,
                theme: None,
                experimental: serde_json::Value::Null,
            },
        },
        path: PathBuf::from(format!("{name}/package.json")),
        source: ModuleManifestSource::PackageJson,
    }
}

fn root_with_modules(
    modules: &[(&str, ModuleKind)],
    providers: &[(&str, &str)],
    layout: Option<&str>,
) -> RootPackageManifest {
    RootPackageManifest {
        schema_version: 1,
        modules_dir: "modules".into(),
        modules: modules
            .iter()
            .map(|(id, kind)| {
                (
                    (*id).into(),
                    InstalledModuleEntry {
                        kind: *kind,
                        path: format!("modules/{id}"),
                        enabled: true,
                    },
                )
            })
            .collect(),
        providers: providers
            .iter()
            .map(|(interface, module_id)| ((*interface).into(), (*module_id).into()))
            .collect(),
        layout: layout.map(|entrypoint| RootLayoutSelection {
            entrypoint: entrypoint.into(),
        }),
        theme: None,
    }
}

#[test]
fn installed_module_graph_exposes_kind_views_from_single_modules_map() {
    let root = root_with_modules(
        &[
            ("@mesh/front", ModuleKind::Frontend),
            ("@mesh/back", ModuleKind::Backend),
            ("@mesh/theme", ModuleKind::Theme),
            ("@mesh/icons", ModuleKind::IconPack),
            ("@mesh/fonts", ModuleKind::FontPack),
            ("@mesh/lang-en", ModuleKind::LanguagePack),
            ("@mesh/backend-kit", ModuleKind::Library),
        ],
        &[],
        None,
    );
    let modules = vec![
        loaded_module(
            "@mesh/front",
            ModuleKind::Frontend,
            MeshDependencies::default(),
            vec![],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/back",
            ModuleKind::Backend,
            MeshDependencies::default(),
            vec![],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/theme",
            ModuleKind::Theme,
            MeshDependencies::default(),
            vec![],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/icons",
            ModuleKind::IconPack,
            MeshDependencies::default(),
            vec![],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/fonts",
            ModuleKind::FontPack,
            MeshDependencies::default(),
            vec![],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/lang-en",
            ModuleKind::LanguagePack,
            MeshDependencies::default(),
            vec![],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/backend-kit",
            ModuleKind::Library,
            MeshDependencies::default(),
            vec![],
            MeshContributes::default(),
        ),
    ];

    let graph = InstalledModuleGraph::from_parts(root, modules).unwrap();
    assert_eq!(graph.frontend_modules().len(), 1);
    assert_eq!(graph.backend_modules().len(), 1);
    assert_eq!(graph.theme_modules().len(), 1);
    assert_eq!(graph.icon_modules().len(), 1);
    assert_eq!(graph.font_modules().len(), 1);
    assert_eq!(graph.language_modules().len(), 1);
    assert_eq!(graph.library_modules().len(), 1);
}

#[test]
fn installed_module_graph_rejects_root_module_without_loaded_package() {
    let root = root_with_modules(&[("@mesh/missing", ModuleKind::Frontend)], &[], None);
    assert!(InstalledModuleGraph::from_parts(root, vec![]).is_err());
}

fn audio_modules() -> Vec<LoadedModuleManifest> {
    vec![
        loaded_module(
            "@mesh/pipewire-audio",
            ModuleKind::Backend,
            MeshDependencies::default(),
            vec![MeshProvidesDeclaration {
                interface: "mesh.audio".into(),
                version: None,
                base_module: None,
                provider: Some("pipewire".into()),
                label: Some("PipeWire".into()),
                priority: 100,
            }],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/pulseaudio-audio",
            ModuleKind::Backend,
            MeshDependencies::default(),
            vec![MeshProvidesDeclaration {
                interface: "mesh.audio".into(),
                version: None,
                base_module: None,
                provider: Some("pulseaudio".into()),
                label: Some("PulseAudio".into()),
                priority: 50,
            }],
            MeshContributes::default(),
        ),
    ]
}

fn interface_module(
    module_id: &str,
    name: &str,
    domain: &str,
    relationship: InterfaceRelationship,
    extends: Option<&str>,
) -> LoadedModuleManifest {
    let mut module = loaded_module(
        module_id,
        ModuleKind::Interface,
        MeshDependencies::default(),
        Vec::new(),
        MeshContributes::default(),
    );
    module.manifest.mesh.interface = Some(MeshInterfaceDeclaration {
        name: name.into(),
        version: Some("1.0".into()),
        file: Some("interface.toml".into()),
        domain: Some(domain.into()),
        extends: extends.map(str::to_string),
        relationship: Some(relationship),
        reason: None,
    });
    module
}

#[test]
fn installed_module_graph_exposes_frontend_backend_requirements() {
    let mut deps = MeshDependencies::default();
    deps.backend.insert("mesh.audio".into(), ">=1.0.0".into());
    deps.backend.insert("mesh.network".into(), ">=1.0.0".into());
    deps.backend.insert("mesh.power".into(), ">=1.0.0".into());
    let mut modules = audio_modules();
    modules.push(loaded_module(
        "@mesh/quick-settings",
        ModuleKind::Frontend,
        deps,
        vec![],
        MeshContributes::default(),
    ));
    let root = root_with_modules(
        &[
            ("@mesh/quick-settings", ModuleKind::Frontend),
            ("@mesh/pipewire-audio", ModuleKind::Backend),
            ("@mesh/pulseaudio-audio", ModuleKind::Backend),
        ],
        &[("mesh.audio", "@mesh/pipewire-audio")],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(root, modules).unwrap();
    let requirements = graph
        .requirements_for_frontend("@mesh/quick-settings")
        .unwrap();
    assert!(requirements.backend.contains_key("mesh.audio"));
    assert!(requirements.backend.contains_key("mesh.network"));
    assert!(requirements.backend.contains_key("mesh.power"));
}

#[test]
fn installed_module_graph_keeps_multiple_audio_providers() {
    let root = root_with_modules(
        &[
            ("@mesh/pipewire-audio", ModuleKind::Backend),
            ("@mesh/pulseaudio-audio", ModuleKind::Backend),
        ],
        &[],
        None,
    );
    let graph = InstalledModuleGraph::from_parts(root, audio_modules()).unwrap();
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
}

#[test]
fn installed_module_graph_records_interface_extension_guidance() {
    let root = root_with_modules(
        &[
            ("@mesh/audio-interface", ModuleKind::Interface),
            ("@alice/audio-mixer-interface", ModuleKind::Interface),
        ],
        &[],
        None,
    );
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![
            interface_module(
                "@mesh/audio-interface",
                "mesh.audio",
                "audio",
                InterfaceRelationship::Base,
                None,
            ),
            interface_module(
                "@alice/audio-mixer-interface",
                "alice.audio-mixer",
                "audio",
                InterfaceRelationship::Independent,
                None,
            ),
        ],
    )
    .unwrap();

    let guidance = graph.interface_guidance();
    assert_eq!(guidance.len(), 1);
    assert_eq!(guidance[0].interface, "alice.audio-mixer");
    assert_eq!(guidance[0].recommended_base, "mesh.audio");
}

#[test]
fn installed_module_graph_does_not_warn_for_declared_interface_extension() {
    let root = root_with_modules(
        &[
            ("@mesh/audio-interface", ModuleKind::Interface),
            ("@alice/audio-streams-interface", ModuleKind::Interface),
        ],
        &[],
        None,
    );
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![
            interface_module(
                "@mesh/audio-interface",
                "mesh.audio",
                "audio",
                InterfaceRelationship::Base,
                None,
            ),
            interface_module(
                "@alice/audio-streams-interface",
                "alice.audio-streams",
                "audio",
                InterfaceRelationship::Extension,
                Some("mesh.audio"),
            ),
        ],
    )
    .unwrap();

    assert!(graph.interface_guidance().is_empty());
    assert_eq!(
        graph
            .declared_interface("alice.audio-streams")
            .unwrap()
            .extends
            .as_deref(),
        Some("mesh.audio")
    );
}

#[test]
fn installed_module_graph_returns_explicit_active_provider() {
    let root = root_with_modules(
        &[
            ("@mesh/pipewire-audio", ModuleKind::Backend),
            ("@mesh/pulseaudio-audio", ModuleKind::Backend),
        ],
        &[("mesh.audio", "@mesh/pipewire-audio")],
        None,
    );
    let graph = InstalledModuleGraph::from_parts(root, audio_modules()).unwrap();
    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
}

#[test]
fn installed_module_graph_rejects_unknown_active_provider() {
    let root = root_with_modules(
        &[("@mesh/pipewire-audio", ModuleKind::Backend)],
        &[("mesh.audio", "@mesh/missing")],
        None,
    );
    let modules = vec![audio_modules().remove(0)];
    assert!(InstalledModuleGraph::from_parts(root, modules).is_err());
}

#[test]
fn installed_module_graph_rejects_active_provider_interface_mismatch() {
    let root = root_with_modules(
        &[("@mesh/network", ModuleKind::Backend)],
        &[("mesh.audio", "@mesh/network")],
        None,
    );
    let network = loaded_module(
        "@mesh/network",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.network".into(),
            version: None,
            base_module: None,
            provider: Some("networkmanager".into()),
            label: Some("NetworkManager".into()),
            priority: 100,
        }],
        MeshContributes::default(),
    );
    assert!(InstalledModuleGraph::from_parts(root, vec![network]).is_err());
}

#[test]
fn installed_module_graph_resolves_layout_entrypoint() {
    let contributes = MeshContributes {
        layout: vec![LayoutContribution {
            id: "main".into(),
            entrypoint: "src/main.mesh".into(),
            label: None,
        }],
        ..MeshContributes::default()
    };
    let root = root_with_modules(
        &[("@mesh/panel", ModuleKind::Frontend)],
        &[],
        Some("@mesh/panel:main"),
    );
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![loaded_module(
            "@mesh/panel",
            ModuleKind::Frontend,
            MeshDependencies::default(),
            vec![],
            contributes,
        )],
    )
    .unwrap();
    let entrypoint = graph.layout_entrypoint().unwrap();
    assert_eq!(entrypoint.module_id, "@mesh/panel");
    assert_eq!(entrypoint.entrypoint_id, "main");
    assert_eq!(entrypoint.path, "src/main.mesh");
}

#[test]
fn installed_module_graph_indexes_theme_icon_font_i18n_contributions() {
    let mut modes = HashMap::new();
    modes.insert("dark".into(), "themes/dark.json".into());
    let contributes = MeshContributes {
        themes: vec![ThemeContribution {
            id: "mesh-default".into(),
            label: "MESH Default".into(),
            modes,
            default_mode: Some("dark".into()),
        }],
        icons: vec![PathContribution {
            id: "material".into(),
            path: "icons".into(),
            label: None,
        }],
        fonts: vec![PathContribution {
            id: "inter".into(),
            path: "fonts".into(),
            label: None,
        }],
        i18n: vec![I18nContribution {
            id: "en".into(),
            locale: "en".into(),
            path: "i18n/en.json".into(),
        }],
        ..MeshContributes::default()
    };
    let root = root_with_modules(&[("@mesh/resources", ModuleKind::Theme)], &[], None);
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![loaded_module(
            "@mesh/resources",
            ModuleKind::Theme,
            MeshDependencies::default(),
            vec![],
            contributes,
        )],
    )
    .unwrap();
    assert_eq!(graph.contributed_themes().len(), 1);
    assert_eq!(graph.contributed_icons().len(), 1);
    assert_eq!(graph.contributed_fonts().len(), 1);
    assert_eq!(graph.contributed_i18n().len(), 1);
}

#[test]
fn installed_module_graph_indexes_library_contributions() {
    let contributes = MeshContributes {
        libraries: vec![LibraryContribution {
            namespace: "@mesh/backend-kit".into(),
            path: "lib".into(),
        }],
        ..MeshContributes::default()
    };
    let root = root_with_modules(&[("@mesh/backend-kit", ModuleKind::Library)], &[], None);
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![loaded_module(
            "@mesh/backend-kit",
            ModuleKind::Library,
            MeshDependencies::default(),
            vec![],
            contributes,
        )],
    )
    .unwrap();

    assert_eq!(graph.library_modules().len(), 1);
    assert_eq!(graph.contributed_libraries().len(), 1);
    assert_eq!(
        graph.contributed_libraries()[0],
        ContributedLibrary {
            module_id: "@mesh/backend-kit".into(),
            namespace: "@mesh/backend-kit".into(),
            path: "lib".into(),
        }
    );
}

#[test]
fn installed_module_graph_rejects_library_path_escape() {
    let contributes = MeshContributes {
        libraries: vec![LibraryContribution {
            namespace: "@mesh/backend-kit".into(),
            path: "../lib".into(),
        }],
        ..MeshContributes::default()
    };
    let root = root_with_modules(&[("@mesh/backend-kit", ModuleKind::Library)], &[], None);
    let result = InstalledModuleGraph::from_parts(
        root,
        vec![loaded_module(
            "@mesh/backend-kit",
            ModuleKind::Library,
            MeshDependencies::default(),
            vec![],
            contributes,
        )],
    );

    assert!(result.is_err());
}

#[test]
fn installed_module_graph_rejects_contribution_path_escape() {
    let contributes = MeshContributes {
        icons: vec![PathContribution {
            id: "bad".into(),
            path: "../outside.json".into(),
            label: None,
        }],
        ..MeshContributes::default()
    };
    let root = root_with_modules(&[("@mesh/icons", ModuleKind::IconPack)], &[], None);
    assert!(
        InstalledModuleGraph::from_parts(
            root,
            vec![loaded_module(
                "@mesh/icons",
                ModuleKind::IconPack,
                MeshDependencies::default(),
                vec![],
                contributes,
            )]
        )
        .is_err()
    );
}
