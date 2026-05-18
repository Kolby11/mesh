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
    let path = root_module_graph_manifest_path().unwrap();
    assert!(path.ends_with(".mesh/module.json"));
}

#[test]
fn module_package_paths_reject_relative_mesh_home() {
    let _guard = EnvGuard::set("MESH_HOME", Some("relative/path"));
    assert!(matches!(
        mesh_home(),
        Err(ModuleManifestError::InvalidMeshHome(_))
    ));
}

#[test]
fn module_root_manifest_parses_minimal_module_json() {
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
    let manifest = RootModuleGraphManifest::from_json_str(content).unwrap();
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
    let manifest = RootModuleGraphManifest::from_json_str(content).unwrap();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.modules_dir, "modules");
    assert_eq!(
        manifest.layout.unwrap().entrypoint.as_str(),
        "@mesh/panel:main"
    );
}

#[test]
fn module_manifest_parses_backend_module_json() {
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
    let manifest = ModuleManifest::from_json_str(content).unwrap();
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
    let manifest = ModuleManifest::from_json_str(content).unwrap();
    let interface = manifest.mesh.interface.unwrap();
    assert_eq!(interface.name, "alice.audio-streams");
    assert_eq!(interface.domain.as_deref(), Some("audio"));
    assert_eq!(interface.extends.as_deref(), Some("mesh.audio"));
    assert_eq!(
        interface.relationship,
        Some(InterfaceRelationship::Extension)
    );
}

fn interface_relationship_manifest(relationship: Option<&str>, extends: Option<&str>) -> String {
    let relationship_json = relationship
        .map(|relationship| format!(r#","relationship":"{relationship}""#))
        .unwrap_or_default();
    let extends_json = extends
        .map(|extends| format!(r#","extends":"{extends}""#))
        .unwrap_or_default();
    format!(
        r#"{{
  "name": "@alice/example-interface",
  "version": "1.0.0",
  "mesh": {{
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {{
      "name": "alice.example",
      "version": "1.0",
      "file": "interface.toml",
      "domain": "example"{extends_json}{relationship_json}
    }}
  }}
}}"#
    )
}

#[test]
fn interface_relationship_extension_requires_extends() {
    let err =
        ModuleManifest::from_json_str(&interface_relationship_manifest(Some("extension"), None))
            .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("mesh.interface.relationship"));
    assert!(message.contains("mesh.interface.extends"));
}

#[test]
fn interface_relationship_base_rejects_extends() {
    let err = ModuleManifest::from_json_str(&interface_relationship_manifest(
        Some("base"),
        Some("mesh.example"),
    ))
    .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("mesh.interface.relationship"));
    assert!(message.contains("mesh.interface.extends"));
}

#[test]
fn interface_relationship_independent_rejects_extends() {
    let err = ModuleManifest::from_json_str(&interface_relationship_manifest(
        Some("independent"),
        Some("mesh.example"),
    ))
    .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("mesh.interface.relationship"));
    assert!(message.contains("mesh.interface.extends"));
}

#[test]
fn interface_relationship_infers_extension_from_extends() {
    let manifest =
        ModuleManifest::from_json_str(&interface_relationship_manifest(None, Some("mesh.example")))
            .unwrap();
    let interface = manifest.mesh.interface.unwrap();
    assert_eq!(
        interface.effective_relationship(),
        InterfaceRelationship::Extension
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
    assert!(ModuleManifest::from_json_str(content).is_err());
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
    let manifest = ModuleManifest::from_json_str(content).unwrap();
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
    assert!(ModuleManifest::from_json_str(content).is_err());
}

#[test]
fn module_manifest_loader_rejects_ambiguous_module_and_package_json() {
    let dir = temp_dir("module-ambiguity");
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
    let err = load_module_manifest(&dir).unwrap_err();
    let ModuleManifestError::Diagnostic { diagnostic } = err else {
        panic!("expected diagnostic error for ambiguous manifest files");
    };
    assert_eq!(diagnostic.severity, ModuleManifestDiagnosticSeverity::Error);
    assert!(
        diagnostic
            .message
            .contains("ambiguous module manifest files found")
    );
    assert_eq!(
        diagnostic.suggested_action,
        "keep canonical module.json and remove the old manifest file"
    );
}

#[test]
fn module_manifest_loader_accepts_canonical_module_json() {
    let dir = temp_dir("canonical-module");
    fs::write(
        dir.join("module.json"),
        r#"{"name":"@mesh/module","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend"}}"#,
    )
    .unwrap();
    let loaded = load_module_manifest(&dir).unwrap();
    assert_eq!(loaded.source, ModuleManifestSource::CanonicalModuleJson);
    assert_eq!(loaded.manifest.name, "@mesh/module");
    assert!(loaded.diagnostics.is_empty());
}

#[test]
fn module_manifest_loader_accepts_legacy_package_json_with_replacement_warning() {
    let dir = temp_dir("legacy-package");
    fs::write(
        dir.join("package.json"),
        r#"{"name":"@mesh/package","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend"}}"#,
    )
    .unwrap();
    let loaded = load_module_manifest(&dir).unwrap();
    assert_eq!(loaded.source, ModuleManifestSource::LegacyPackageJson);
    assert_eq!(loaded.manifest.name, "@mesh/package");
    assert_eq!(
        loaded.diagnostics[0].severity,
        ModuleManifestDiagnosticSeverity::Warning
    );
    assert_eq!(
        loaded.diagnostics[0].suggested_action,
        "replace package.json with module.json"
    );
}

#[test]
fn module_manifest_loader_rejects_plugin_json() {
    let dir = temp_dir("plugin-json");
    fs::write(dir.join("plugin.json"), r#"{}"#).unwrap();
    let err = load_module_manifest(&dir).unwrap_err();
    let ModuleManifestError::Diagnostic { diagnostic } = err else {
        panic!("expected diagnostic error for plugin.json");
    };
    assert_eq!(diagnostic.severity, ModuleManifestDiagnosticSeverity::Error);
    assert_eq!(
        diagnostic.message,
        "plugin.json is not a supported MESH module manifest"
    );
    assert_eq!(
        diagnostic.suggested_action,
        "remove plugin.json or replace it with module.json"
    );
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
    assert_eq!(
        loaded.diagnostics[0].severity,
        ModuleManifestDiagnosticSeverity::Warning
    );
    assert_eq!(
        loaded.diagnostics[0].suggested_action,
        "replace legacy module.json fields with name/version/mesh"
    );
}

#[test]
fn module_manifest_loader_accepts_legacy_mesh_toml_with_replacement_warning() {
    let dir = temp_dir("legacy-mesh-toml");
    fs::write(
        dir.join("mesh.toml"),
        r#"
[package]
id = "@mesh/toml-module"
version = "0.1.0"
type = "surface"
api_version = "0.1"
"#,
    )
    .unwrap();
    let loaded = load_module_manifest(&dir).unwrap();
    assert_eq!(loaded.source, ModuleManifestSource::LegacyMeshToml);
    assert_eq!(loaded.manifest.name, "@mesh/toml-module");
    assert_eq!(
        loaded.diagnostics[0].severity,
        ModuleManifestDiagnosticSeverity::Warning
    );
    assert_eq!(
        loaded.diagnostics[0].suggested_action,
        "replace mesh.toml with module.json"
    );
}

#[test]
fn module_manifest_loader_preserves_navigation_bar_entrypoint() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/frontend/navigation-bar");
    let loaded = load_module_manifest(&dir).unwrap();
    assert_eq!(loaded.source, ModuleManifestSource::CanonicalModuleJson);
    assert_eq!(loaded.manifest.name, "@mesh/navigation-bar");
    assert_eq!(
        loaded.manifest.mesh.entrypoints.main.as_deref(),
        Some("src/main.mesh")
    );
    assert_eq!(loaded.manifest.mesh.contributes.layout[0].id, "main");
}

#[test]
fn installed_module_graph_loads_repo_module_fixture() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let graph = load_installed_module_graph(&workspace_root.join("config/module.json")).unwrap();

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
        manifest: ModuleManifest {
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
                icon_requirements: crate::manifest::IconRequirementsSection::default(),
                accessibility: None,
                surface_layout: None,
                theme: None,
                experimental: serde_json::Value::Null,
            },
        },
        path: PathBuf::from(format!("{name}/package.json")),
        source: ModuleManifestSource::LegacyPackageJson,
        diagnostics: Vec::new(),
    }
}

fn root_with_modules(
    modules: &[(&str, ModuleKind)],
    providers: &[(&str, &str)],
    layout: Option<&str>,
) -> RootModuleGraphManifest {
    RootModuleGraphManifest {
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
fn installed_module_graph_keeps_provider_interface_and_frontend_requirements_separate() {
    let mut deps = MeshDependencies::default();
    deps.backend.insert("mesh.example".into(), ">=1.0.0".into());

    let mut interface = interface_module(
        "@mesh/example-interface",
        "mesh.example",
        "example",
        InterfaceRelationship::Base,
        None,
    );
    interface
        .manifest
        .mesh
        .provides
        .push(MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: None,
            base_module: None,
            provider: Some("interface-owned-provider".into()),
            label: None,
            priority: 200,
        });

    let modules = vec![
        loaded_module(
            "@mesh/example-widget",
            ModuleKind::Frontend,
            deps,
            vec![],
            MeshContributes::default(),
        ),
        loaded_module(
            "@mesh/example-backend",
            ModuleKind::Backend,
            MeshDependencies::default(),
            vec![MeshProvidesDeclaration {
                interface: "mesh.example".into(),
                version: None,
                base_module: Some("@mesh/example-interface".into()),
                provider: Some("example".into()),
                label: Some("Example".into()),
                priority: 100,
            }],
            MeshContributes::default(),
        ),
        interface,
    ];
    let root = root_with_modules(
        &[
            ("@mesh/example-widget", ModuleKind::Frontend),
            ("@mesh/example-backend", ModuleKind::Backend),
            ("@mesh/example-interface", ModuleKind::Interface),
        ],
        &[("mesh.example", "@mesh/example-backend")],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(root, modules).unwrap();
    assert!(graph.declared_interface("mesh.example").is_some());

    let providers = graph.backend_providers_for_interface("mesh.example");
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].module_id, "@mesh/example-backend");
    assert_eq!(providers[0].provider.as_deref(), Some("example"));

    let requirements = graph
        .requirements_for_frontend("@mesh/example-widget")
        .unwrap();
    assert_eq!(
        requirements.backend.get("mesh.example").map(String::as_str),
        Some(">=1.0.0")
    );
    assert!(
        graph
            .requirements_for_frontend("@mesh/example-backend")
            .is_none()
    );
}

#[test]
fn provider_capability_metadata_comes_only_from_backend_manifest() {
    let mut deps = MeshDependencies::default();
    deps.backend.insert("mesh.example".into(), ">=1.0.0".into());

    let frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        deps,
        vec![],
        MeshContributes::default(),
    );
    let mut backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.2.0".into()),
            base_module: Some("@mesh/example-interface".into()),
            provider: Some("example".into()),
            label: Some("Example".into()),
            priority: 100,
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.capabilities.required = vec!["service.example.read".into()];
    backend.manifest.mesh.capabilities.optional = vec!["service.example.control".into()];

    let root = root_with_modules(
        &[
            ("@mesh/example-widget", ModuleKind::Frontend),
            ("@mesh/example-backend", ModuleKind::Backend),
        ],
        &[("mesh.example", "@mesh/example-backend")],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend, backend]).unwrap();
    let provider = graph.active_provider("mesh.example").unwrap();
    assert_eq!(provider.version.as_deref(), Some("1.2.0"));
    assert_eq!(
        provider.base_module.as_deref(),
        Some("@mesh/example-interface")
    );
    assert_eq!(provider.provider.as_deref(), Some("example"));
    assert_eq!(
        provider.required_capabilities,
        vec!["service.example.read".to_string()]
    );
    assert_eq!(
        provider.optional_capabilities,
        vec!["service.example.control".to_string()]
    );
}

#[test]
fn installed_module_graph_routes_generic_interface_provider_without_service_branch() {
    let backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example.alt".into(),
            version: Some("1.0.0".into()),
            base_module: None,
            provider: Some("example-alt".into()),
            label: Some("Example Alt".into()),
            priority: 25,
        }],
        MeshContributes::default(),
    );
    let root = root_with_modules(
        &[("@mesh/example-backend", ModuleKind::Backend)],
        &[("mesh.example.alt", "@mesh/example-backend")],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();
    let provider = graph.active_provider("mesh.example.alt").unwrap();
    assert_eq!(provider.module_id, "@mesh/example-backend");
    assert_eq!(provider.provider.as_deref(), Some("example-alt"));
    assert_eq!(
        graph
            .backend_providers_for_interface("mesh.example.alt")
            .len(),
        1
    );
    assert!(
        graph
            .backend_providers_for_interface("mesh.audio")
            .is_empty()
    );
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
fn installed_module_graph_records_interface_guidance_for_independent_domain_peer() {
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
    assert_eq!(guidance[0].status, "consider_extending_base_interface");
    assert_eq!(guidance[0].interface, "alice.audio-mixer");
    assert_eq!(guidance[0].recommended_base, "mesh.audio");
    assert_eq!(
        graph
            .declared_interface("alice.audio-mixer")
            .unwrap()
            .relationship,
        InterfaceRelationship::Independent
    );
}

#[test]
fn installed_module_graph_interface_guidance_ignores_declared_interface_extension() {
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
    let declared = graph.declared_interface("alice.audio-streams").unwrap();
    assert_eq!(declared.extends.as_deref(), Some("mesh.audio"));
    assert_eq!(declared.relationship, InterfaceRelationship::Extension);
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
fn contribution_index_records_source_metadata_and_scoped_ids() {
    let icon_pack = |module_id: &str| {
        loaded_module(
            module_id,
            ModuleKind::IconPack,
            MeshDependencies::default(),
            vec![],
            MeshContributes {
                icons: vec![PathContribution {
                    id: "shared".into(),
                    path: "icons".into(),
                    label: None,
                }],
                ..MeshContributes::default()
            },
        )
    };
    let root = root_with_modules(
        &[
            ("@mesh/icons-a", ModuleKind::IconPack),
            ("@mesh/icons-b", ModuleKind::IconPack),
        ],
        &[],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![icon_pack("@mesh/icons-a"), icon_pack("@mesh/icons-b")],
    )
    .unwrap();
    let mut scoped_ids = graph
        .contributed_icons()
        .iter()
        .map(|icon| icon.source.scoped_id.clone())
        .collect::<Vec<_>>();
    scoped_ids.sort();

    assert_eq!(
        scoped_ids,
        vec![
            "@mesh/icons-a:shared".to_string(),
            "@mesh/icons-b:shared".to_string()
        ]
    );
    let icon = graph
        .contributed_icons()
        .iter()
        .find(|icon| icon.module_id == "@mesh/icons-a")
        .unwrap();
    assert_eq!(icon.source.module_kind, ModuleKind::IconPack);
    assert_eq!(icon.source.local_id, "shared");
    assert_eq!(
        icon.source.manifest_source,
        ModuleManifestSource::LegacyPackageJson
    );
    assert!(
        icon.source
            .manifest_path
            .ends_with("@mesh/icons-a/package.json")
    );
}

#[test]
fn contribution_index_exposes_frontend_keybind_resource_interface_and_provider_records() {
    let mut frontend_contributes = MeshContributes::default();
    frontend_contributes.settings = Some(SettingsContribution {
        namespace: "@mesh/example-widget".into(),
        schema: serde_json::json!({ "type": "object" }),
    });
    let mut frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        frontend_contributes,
    );
    frontend.manifest.mesh.entrypoints.main = Some("src/main.mesh".into());
    frontend.manifest.mesh.entrypoints.settings_ui = Some("src/settings.mesh".into());
    frontend.manifest.mesh.keybinds.actions.insert(
        "mute".into(),
        crate::manifest::KeybindAction {
            label: Some("Mute".into()),
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            localized_triggers: HashMap::from([(
                "sk".into(),
                crate::manifest::KeybindTrigger {
                    kind: crate::manifest::KeybindTriggerKind::Shortcut,
                    key: Some("s".into()),
                    modifiers: Vec::new(),
                },
            )]),
            ..crate::manifest::KeybindAction::default()
        },
    );
    frontend.manifest.mesh.icon_requirements.required = vec!["audio-volume-high".into()];

    let mut icon_pack = loaded_module(
        "@mesh/icons-material",
        ModuleKind::IconPack,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    icon_pack.manifest.mesh.icon_pack = Some(crate::manifest::IconPackSection {
        id: "material".into(),
        mappings: HashMap::from([(
            "audio-volume-high".into(),
            "material-symbols/volume_up".into(),
        )]),
        ..crate::manifest::IconPackSection::default()
    });

    let backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/example-interface".into()),
            provider: Some("example".into()),
            label: None,
            priority: 10,
        }],
        MeshContributes::default(),
    );
    let interface = interface_module(
        "@mesh/example-interface",
        "mesh.example",
        "example",
        InterfaceRelationship::Base,
        None,
    );
    let root = root_with_modules(
        &[
            ("@mesh/example-widget", ModuleKind::Frontend),
            ("@mesh/icons-material", ModuleKind::IconPack),
            ("@mesh/example-backend", ModuleKind::Backend),
            ("@mesh/example-interface", ModuleKind::Interface),
        ],
        &[("mesh.example", "@mesh/example-backend")],
        None,
    );

    let graph =
        InstalledModuleGraph::from_parts(root, vec![frontend, icon_pack, backend, interface])
            .unwrap();

    assert_eq!(graph.frontend_entrypoints().len(), 2);
    assert!(graph.frontend_entrypoints().iter().any(|entrypoint| {
        entrypoint.kind == FrontendEntrypointKind::Main && entrypoint.path == "src/main.mesh"
    }));
    assert_eq!(
        graph.settings_schemas()[0].namespace,
        "@mesh/example-widget"
    );
    let keybind = &graph.keybind_actions()[0];
    assert_eq!(keybind.action_id, "mute");
    assert_eq!(keybind.trigger.key.as_deref(), Some("m"));
    assert_eq!(
        keybind
            .localized_triggers
            .get("sk")
            .and_then(|trigger| trigger.key.as_deref()),
        Some("s")
    );
    assert_eq!(graph.icon_requirements()[0].name, "audio-volume-high");
    assert!(graph.icon_requirements()[0].required);
    assert_eq!(graph.icon_pack_contributions()[0].id, "material");
    assert_eq!(
        graph.icon_pack_contributions()[0]
            .mappings
            .get("audio-volume-high")
            .map(String::as_str),
        Some("material-symbols/volume_up")
    );
    assert_eq!(graph.declared_interfaces()[0].name, "mesh.example");
    assert_eq!(
        graph.backend_provider_contributions()[0].interface,
        "mesh.example"
    );
}

#[test]
fn contribution_index_reports_resource_and_settings_compatibility_diagnostics() {
    let mut deps = MeshDependencies::default();
    deps.icons.insert("@mesh/missing-icons".into(), "*".into());
    deps.fonts.insert("@mesh/missing-fonts".into(), "*".into());
    deps.i18n.insert("@mesh/missing-lang".into(), "*".into());
    deps.themes.insert("@mesh/missing-theme".into(), "*".into());

    let settings = SettingsContribution {
        namespace: "shared.settings".into(),
        schema: serde_json::json!({ "type": "object" }),
    };
    let mut frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        deps,
        vec![],
        MeshContributes {
            settings: Some(settings.clone()),
            ..MeshContributes::default()
        },
    );
    frontend.manifest.mesh.icon_requirements.required = vec!["missing-semantic-icon".into()];
    let other_settings = loaded_module(
        "@mesh/other-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            settings: Some(settings),
            ..MeshContributes::default()
        },
    );
    let mut icon_pack = loaded_module(
        "@mesh/icons-material",
        ModuleKind::IconPack,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    icon_pack.manifest.mesh.icon_pack = Some(crate::manifest::IconPackSection {
        id: "material".into(),
        mappings: HashMap::from([(
            "available-semantic-icon".into(),
            "material-symbols/check".into(),
        )]),
        ..crate::manifest::IconPackSection::default()
    });
    let root = root_with_modules(
        &[
            ("@mesh/example-widget", ModuleKind::Frontend),
            ("@mesh/other-widget", ModuleKind::Frontend),
            ("@mesh/icons-material", ModuleKind::IconPack),
        ],
        &[],
        None,
    );

    let graph =
        InstalledModuleGraph::from_parts(root, vec![frontend, other_settings, icon_pack]).unwrap();

    let statuses = graph
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.status.as_str())
        .collect::<Vec<_>>();
    assert!(statuses.contains(&"missing_icon_pack_requirement"));
    assert!(statuses.contains(&"missing_font_pack_requirement"));
    assert!(statuses.contains(&"missing_i18n_pack_requirement"));
    assert!(statuses.contains(&"missing_theme_requirement"));
    assert!(statuses.contains(&"missing_required_icon"));
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == "duplicate_settings_namespace")
            .count(),
        2
    );
    let icon_diagnostic = graph
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.status == "missing_required_icon")
        .unwrap();
    assert_eq!(icon_diagnostic.module_id, "@mesh/example-widget");
    assert!(
        icon_diagnostic
            .contribution_id
            .as_deref()
            .is_some_and(|id| id.contains("required:missing-semantic-icon"))
    );
}

#[test]
fn disabled_modules_remain_catalog_nodes_but_not_runtime_contributions() {
    let mut deps = MeshDependencies::default();
    deps.backend.insert("mesh.example".into(), ">=1.0.0".into());
    let frontend = loaded_module(
        "@mesh/disabled-widget",
        ModuleKind::Frontend,
        deps,
        vec![],
        MeshContributes {
            layout: vec![LayoutContribution {
                id: "main".into(),
                entrypoint: "src/main.mesh".into(),
                label: None,
            }],
            ..MeshContributes::default()
        },
    );
    let backend = loaded_module(
        "@mesh/disabled-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: None,
            base_module: None,
            provider: Some("disabled".into()),
            label: None,
            priority: 100,
        }],
        MeshContributes::default(),
    );
    let interface = interface_module(
        "@mesh/disabled-interface",
        "mesh.example",
        "example",
        InterfaceRelationship::Base,
        None,
    );
    let root = RootModuleGraphManifest {
        schema_version: 1,
        modules_dir: "modules".into(),
        modules: [
            ("@mesh/disabled-widget", ModuleKind::Frontend),
            ("@mesh/disabled-backend", ModuleKind::Backend),
            ("@mesh/disabled-interface", ModuleKind::Interface),
        ]
        .into_iter()
        .map(|(id, kind)| {
            (
                id.to_string(),
                InstalledModuleEntry {
                    kind,
                    path: format!("modules/{id}"),
                    enabled: false,
                },
            )
        })
        .collect(),
        providers: HashMap::new(),
        layout: None,
        theme: None,
    };

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend, backend, interface]).unwrap();

    assert!(!graph.module("@mesh/disabled-widget").unwrap().enabled);
    assert!(!graph.module("@mesh/disabled-backend").unwrap().enabled);
    assert!(graph.frontend_modules().is_empty());
    assert!(graph.backend_modules().is_empty());
    assert!(graph.interface_modules().is_empty());
    assert!(
        graph
            .requirements_for_frontend("@mesh/disabled-widget")
            .is_none()
    );
    assert!(
        graph
            .backend_providers_for_interface("mesh.example")
            .is_empty()
    );
    assert!(graph.declared_interface("mesh.example").is_none());
    assert!(graph.frontend_entrypoints().is_empty());
    assert!(graph.contributed_themes().is_empty());
    assert!(graph.contributed_icons().is_empty());
    assert!(graph.keybind_actions().is_empty());
    assert!(graph.layout_entrypoint().is_none());
}

#[test]
fn manifest_driven_extension_graph_indexes_provider_library_resource_and_frontend_requirement() {
    let mut deps = MeshDependencies::default();
    deps.backend.insert("mesh.example".into(), ">=1.0.0".into());
    deps.icons.insert("material".into(), "*".into());
    deps.fonts.insert("inter".into(), "*".into());
    deps.i18n.insert("en".into(), "*".into());
    deps.themes.insert("mesh-default".into(), "*".into());
    let mut frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        deps,
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.entrypoints.main = Some("src/main.mesh".into());
    frontend.manifest.mesh.icon_requirements.required = vec!["example-action".into()];

    let interface = interface_module(
        "@mesh/example-interface",
        "mesh.example",
        "example",
        InterfaceRelationship::Base,
        None,
    );
    let backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/example-interface".into()),
            provider: Some("example".into()),
            label: Some("Example".into()),
            priority: 100,
        }],
        MeshContributes::default(),
    );
    let library = loaded_module(
        "@mesh/example-lib",
        ModuleKind::Library,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            libraries: vec![LibraryContribution {
                namespace: "@mesh/example-lib".into(),
                path: "lib".into(),
            }],
            ..MeshContributes::default()
        },
    );
    let mut icon_pack = loaded_module(
        "@mesh/example-icons",
        ModuleKind::IconPack,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    icon_pack.manifest.mesh.icon_pack = Some(crate::manifest::IconPackSection {
        id: "material".into(),
        mappings: HashMap::from([("example-action".into(), "material-symbols/check".into())]),
        ..crate::manifest::IconPackSection::default()
    });
    let font_pack = loaded_module(
        "@mesh/example-fonts",
        ModuleKind::FontPack,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            fonts: vec![PathContribution {
                id: "inter".into(),
                path: "fonts".into(),
                label: Some("Inter".into()),
            }],
            ..MeshContributes::default()
        },
    );
    let language_pack = loaded_module(
        "@mesh/example-lang",
        ModuleKind::LanguagePack,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            i18n: vec![I18nContribution {
                id: "en".into(),
                locale: "en".into(),
                path: "i18n/en.json".into(),
            }],
            ..MeshContributes::default()
        },
    );
    let mut theme_modes = HashMap::new();
    theme_modes.insert("dark".into(), "themes/dark.json".into());
    let theme = loaded_module(
        "@mesh/example-theme",
        ModuleKind::Theme,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            themes: vec![ThemeContribution {
                id: "mesh-default".into(),
                label: "Default".into(),
                modes: theme_modes,
                default_mode: Some("dark".into()),
            }],
            ..MeshContributes::default()
        },
    );
    let root = root_with_modules(
        &[
            ("@mesh/example-widget", ModuleKind::Frontend),
            ("@mesh/example-interface", ModuleKind::Interface),
            ("@mesh/example-backend", ModuleKind::Backend),
            ("@mesh/example-lib", ModuleKind::Library),
            ("@mesh/example-icons", ModuleKind::IconPack),
            ("@mesh/example-fonts", ModuleKind::FontPack),
            ("@mesh/example-lang", ModuleKind::LanguagePack),
            ("@mesh/example-theme", ModuleKind::Theme),
        ],
        &[("mesh.example", "@mesh/example-backend")],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![
            frontend,
            interface,
            backend,
            library,
            icon_pack,
            font_pack,
            language_pack,
            theme,
        ],
    )
    .unwrap();

    assert!(graph.diagnostics().is_empty());
    assert_eq!(
        graph
            .requirements_for_frontend("@mesh/example-widget")
            .unwrap()
            .backend
            .get("mesh.example")
            .map(String::as_str),
        Some(">=1.0.0")
    );
    assert_eq!(
        graph.declared_interface("mesh.example").unwrap().module_id,
        "@mesh/example-interface"
    );
    assert_eq!(
        graph.active_provider("mesh.example").unwrap().module_id,
        "@mesh/example-backend"
    );
    assert_eq!(
        graph.contributed_libraries()[0].namespace,
        "@mesh/example-lib"
    );
    assert_eq!(graph.icon_requirements()[0].name, "example-action");
    assert_eq!(graph.icon_pack_contributions()[0].id, "material");
    assert_eq!(graph.contributed_fonts()[0].id, "inter");
    assert_eq!(graph.contributed_i18n()[0].locale, "en");
    assert_eq!(graph.contributed_themes()[0].id, "mesh-default");
    assert_eq!(graph.frontend_entrypoints()[0].path, "src/main.mesh");
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
    let library = &graph.contributed_libraries()[0];
    assert_eq!(library.module_id, "@mesh/backend-kit");
    assert_eq!(library.namespace, "@mesh/backend-kit");
    assert_eq!(library.path, "lib");
    assert_eq!(
        library.source.scoped_id,
        "@mesh/backend-kit:@mesh/backend-kit"
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
