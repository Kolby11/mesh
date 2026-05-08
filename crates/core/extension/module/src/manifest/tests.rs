use super::json::JsonManifest;
use super::toml::TomlManifest;
use super::*;
use std::collections::HashMap;

#[test]
fn parses_legacy_mesh_toml_manifest() {
    let content = r#"
[package]
id = "@mesh/panel"
version = "0.1.0"
type = "surface"
api_version = "0.1"

[service]
provides = "audio"
backend_name = "PipeWire"
priority = 100

[entrypoints]
main = "src/main.mesh"
"#;

    let parsed: TomlManifest = ::toml::from_str(content).unwrap();
    let manifest = parsed.into_manifest();

    assert_eq!(manifest.package.id, "@mesh/panel");
    assert_eq!(manifest.primary_service().unwrap().provides, "audio");
}

#[test]
fn parses_module_json_manifest() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "dependencies": {
    "modules": {
      "@mesh/audio-contract": ">=1.0.0"
    },
    "interfaces": [
      { "name": "mesh.audio", "version": ">=1.0", "required": false }
    ]
  },
  "entrypoints": {
    "main": "src/main.mesh"
  },
  "exports": {
    "component": {
      "tag": "PanelRoot"
    }
  },
  "provides": [
    {
      "interface": "mesh.audio",
      "version": "1.0",
      "base_module": "@mesh/audio-interface",
      "backend_name": "PipeWire",
      "priority": 100
    }
  ]
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();

    assert_eq!(manifest.package.id, "@mesh/panel");
    assert_eq!(manifest.exported_component_tag(), Some("PanelRoot"));
    assert_eq!(
        manifest.dependencies.modules["@mesh/audio-contract"],
        DependencySpec::Simple(">=1.0.0".to_string())
    );
    assert_eq!(manifest.declared_provides()[0].interface, "mesh.audio");
    assert_eq!(
        manifest.declared_provides()[0].base_module.as_deref(),
        Some("@mesh/audio-interface")
    );
}

#[test]
fn parses_package_json_module_manifest() {
    let dir = std::env::temp_dir().join(format!("mesh-package-json-module-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{
  "name": "@mesh/pipewire-audio",
  "version": "0.1.0",
  "description": "PipeWire backend",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "capabilities": { "required": ["exec.wpctl"] },
    "i18n": { "defaultLocale": "en", "supportedLocales": ["en", "sk"] },
    "dependencies": {
      "binaries": [{ "name": "wpctl", "reason": "PipeWire control" }]
    },
    "entrypoints": { "main": "src/main.luau" },
    "provides": [
      {
        "interface": "mesh.audio",
        "version": "1.0",
        "baseModule": "@mesh/audio-interface",
        "provider": "pipewire",
        "label": "PipeWire",
        "priority": 100
      }
    ]
  }
}"#,
    )
    .unwrap();

    let loaded = load_manifest(&dir).unwrap();
    assert_eq!(loaded.path, dir.join("package.json"));
    assert_eq!(loaded.manifest.package.id, "@mesh/pipewire-audio");
    assert_eq!(loaded.manifest.package.module_type, ModuleType::Backend);
    assert_eq!(
        loaded.manifest.entrypoints.main.as_deref(),
        Some("src/main.luau")
    );
    assert_eq!(loaded.manifest.capabilities.required, vec!["exec.wpctl"]);
    assert_eq!(loaded.manifest.dependencies.binaries[0].name, "wpctl");
    assert_eq!(
        loaded.manifest.declared_provides()[0]
            .base_module
            .as_deref(),
        Some("@mesh/audio-interface")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parses_module_json_icon_requirements() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "dependencies": {
    "icon_packs": {
      "required": ["system"]
    }
  },
  "assets": {
    "icons": "assets/icons"
  },
  "icon_requirements": {
    "required": ["audio-volume-muted", "network-wireless"],
    "optional": ["weather-clear"]
  },
  "entrypoints": {
    "main": "src/main.mesh"
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();

    assert_eq!(
        manifest.icon_requirements.required,
        vec!["audio-volume-muted", "network-wireless"]
    );
    assert_eq!(manifest.icon_requirements.optional, vec!["weather-clear"]);
    assert_eq!(
        manifest.dependencies.icon_packs.required,
        vec!["system".to_string()]
    );
    assert_eq!(
        manifest
            .assets
            .unwrap()
            .icons
            .as_ref()
            .map(|icons| icons.path()),
        Some("assets/icons")
    );
}

#[test]
fn navigation_bar_declares_icon_pack_dependency() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/frontend/navigation-bar");
    let loaded = super::load_manifest(&dir).expect("navigation-bar manifest should load");
    assert!(
        loaded
            .manifest
            .dependencies
            .icon_packs
            .required
            .iter()
            .any(|id| id == "@mesh/icons-default"),
        "navigation-bar should depend on @mesh/icons-default",
    );
    // Frontend no longer carries inline mappings — those live in the
    // icon-pack module now.
    assert!(
        loaded
            .manifest
            .icons
            .as_ref()
            .map_or(true, |i| i.is_empty())
    );
    assert!(loaded.manifest.icon_pack.is_none());
}

#[test]
fn material_symbols_module_parses_with_font_requirement() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/icon-packs/material-symbols");
    let loaded = super::load_manifest(&dir).expect("material-symbols manifest should load");
    let ip = loaded.manifest.icon_pack.expect("icon_pack section");
    assert_eq!(ip.id, "material-rounded");
    assert_eq!(ip.requires.fonts.len(), 1);
    assert_eq!(ip.requires.fonts[0].family, "Material Symbols Rounded");
    assert_eq!(ip.requires.fonts[0].alias, "ms");
    assert!(ip.axes.fill);
    assert_eq!(
        ip.mappings.get("audio-volume-high").map(String::as_str),
        Some("ms/volume_up")
    );
}

#[test]
fn icons_default_module_parses_as_icon_pack() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/icon-packs/default");
    let loaded = super::load_manifest(&dir).expect("icons-default manifest should load");
    let icon_pack = loaded.manifest.icon_pack.expect("icon_pack section");
    assert_eq!(icon_pack.id, "default");
    assert_eq!(
        icon_pack
            .mappings
            .get("audio-volume-high")
            .map(String::as_str),
        Some("hicolor/audio-volume-high")
    );
    assert_eq!(
        icon_pack.mappings.get("settings").map(String::as_str),
        Some("hicolor/preferences-system")
    );
}

fn manifest_with_dependencies(
    id: &str,
    dependencies: &[(&str, bool)],
    slot_contributions: &[&str],
) -> Manifest {
    Manifest {
        package: PackageSection {
            id: id.to_string(),
            name: None,
            version: "0.1.0".into(),
            module_type: ModuleType::Widget,
            api_version: "0.1".into(),
            license: None,
            description: None,
            authors: Vec::new(),
            repository: None,
        },
        compatibility: CompatibilitySection::default(),
        dependencies: DependenciesSection {
            modules: dependencies
                .iter()
                .map(|(dependency_id, optional)| {
                    let spec = if *optional {
                        DependencySpec::Detailed {
                            version: ">=0.1.0".into(),
                            optional: Some(true),
                        }
                    } else {
                        DependencySpec::Simple(">=0.1.0".into())
                    };
                    ((*dependency_id).to_string(), spec)
                })
                .collect(),
            ..DependenciesSection::default()
        },
        capabilities: CapabilitiesSection::default(),
        entrypoints: EntrypointsSection {
            main: Some("src/main.mesh".into()),
            settings_ui: None,
        },
        accessibility: None,
        settings: None,
        i18n: None,
        theme: None,
        service: None,
        provides: Vec::new(),
        interface: None,
        extensions: Vec::new(),
        exports: ExportsSection::default(),
        provides_slots: HashMap::new(),
        slot_contributions: slot_contributions
            .iter()
            .map(|slot_id| ((*slot_id).to_string(), vec![SlotContribution::default()]))
            .collect(),
        assets: None,
        icons: None,
        icon_pack: None,
        icon_requirements: IconRequirementsSection::default(),
        translations: HashMap::new(),
        surface_layout: None,
    }
}

#[test]
fn detects_required_module_dependency_cycles() {
    let a = manifest_with_dependencies("@mesh/a", &[("@mesh/b", false)], &[]);
    let b = manifest_with_dependencies("@mesh/b", &[("@mesh/a", false)], &[]);

    let err = validate_module_dependency_graph([&a, &b]).unwrap_err();
    match err {
        DependencyGraphError::Cycle { cycle } => {
            assert_eq!(cycle, vec!["@mesh/a", "@mesh/b", "@mesh/a"]);
        }
    }
}

#[test]
fn ignores_optional_dependencies_for_cycle_detection() {
    let a = manifest_with_dependencies("@mesh/a", &[("@mesh/b", true)], &[]);
    let b = manifest_with_dependencies("@mesh/b", &[("@mesh/a", false)], &[]);

    validate_module_dependency_graph([&a, &b]).unwrap();
}

#[test]
fn detects_cycles_through_slot_hosts() {
    let a = manifest_with_dependencies("@mesh/a", &[("@mesh/b", false)], &[]);
    let b = manifest_with_dependencies("@mesh/b", &[], &["@mesh/a:main"]);

    let err = validate_module_dependency_graph([&a, &b]).unwrap_err();
    match err {
        DependencyGraphError::Cycle { cycle } => {
            assert_eq!(cycle, vec!["@mesh/a", "@mesh/b", "@mesh/a"]);
        }
    }
}
