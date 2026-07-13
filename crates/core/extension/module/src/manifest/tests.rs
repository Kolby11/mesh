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
fn embeddable_component_has_export_and_no_surface() {
    let exported = r#"
{
  "id": "@mesh/language-popover",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "entrypoints": { "main": "src/main.mesh" },
  "exports": { "component": { "tag": "LanguagePopover" } }
}
"#;
    let manifest = serde_json::from_str::<JsonManifest>(exported)
        .unwrap()
        .into_manifest();
    assert!(!manifest.declares_surface());
    assert!(manifest.is_embeddable_component());

    // A module that both exports a component *and* declares a surface is a
    // surface owner, not an embeddable-only module.
    let surface_owner = r#"
{
  "id": "@mesh/navigation-bar",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "entrypoints": { "main": "src/main.mesh" },
  "exports": { "component": { "tag": "NavRoot" } },
  "surface_layout": { "anchor": "top", "layer": "top", "height": 56 }
}
"#;
    let manifest = serde_json::from_str::<JsonManifest>(surface_owner)
        .unwrap()
        .into_manifest();
    assert!(manifest.declares_surface());
    assert!(!manifest.is_embeddable_component());
}

#[test]
fn parses_module_json_keybind_declarations() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "accept": {
      "trigger": {
        "kind": "shortcut",
        "key": "a",
        "modifiers": ["ctrl"]
      }
    }
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();
    manifest.validate_keybinds().unwrap();

    let action = &manifest.keybinds.actions["accept"];
    assert_eq!(action.trigger.kind, KeybindTriggerKind::Shortcut);
    assert_eq!(action.trigger.key.as_deref(), Some("a"));
    assert_eq!(action.trigger.modifiers, vec!["ctrl"]);
    assert!(action.localized_triggers.is_empty());
}

#[test]
fn parses_module_json_localized_keybind_triggers() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "accept": {
      "trigger": {
        "kind": "access_key",
        "key": "a"
      },
      "localized_triggers": {
        "sk": {
          "kind": "access_key",
          "key": "p"
        },
        "sk-SK": {
          "kind": "access_key",
          "key": "r"
        }
      }
    }
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();
    manifest.validate_keybinds().unwrap();

    let action = &manifest.keybinds.actions["accept"];
    assert_eq!(action.trigger.kind, KeybindTriggerKind::AccessKey);
    assert_eq!(action.trigger.key.as_deref(), Some("a"));
    assert_eq!(
        action.localized_triggers["sk"].kind,
        KeybindTriggerKind::AccessKey
    );
    assert_eq!(action.localized_triggers["sk"].key.as_deref(), Some("p"));
    assert_eq!(
        action.localized_triggers["sk-SK"].kind,
        KeybindTriggerKind::AccessKey
    );
    assert_eq!(action.localized_triggers["sk-SK"].key.as_deref(), Some("r"));
}

#[test]
fn parses_module_json_keybind_display_keys() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "mute": {
      "scope": "surface",
      "label": "keybind.mute.label",
      "description": "keybind.mute.description",
      "category": "keybind.category.audio",
      "trigger": {
        "kind": "shortcut",
        "key": "m"
      },
      "localizedTriggers": {
        "sk": {
          "kind": "shortcut",
          "key": "s"
        }
      }
    }
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();
    manifest.validate_keybinds().unwrap();

    let action = &manifest.keybinds.actions["mute"];
    assert_eq!(action.scope, KeybindScope::Surface);
    assert_eq!(
        action.label.as_ref(),
        Some(&LocalizedText::Literal("keybind.mute.label".into()))
    );
    assert_eq!(
        action.description.as_ref(),
        Some(&LocalizedText::Literal("keybind.mute.description".into()))
    );
    assert_eq!(
        action.category.as_ref(),
        Some(&LocalizedText::Literal("keybind.category.audio".into()))
    );
    assert_eq!(action.localized_triggers["sk"].key.as_deref(), Some("s"));
}

#[test]
fn parses_module_json_keybind_localized_display_text() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "mute": {
      "label": { "t": "keybind.mute.label", "fallback": "Mute" },
      "description": { "t": "keybind.mute.description", "fallback": "Mute audio" },
      "category": { "t": "keybind.category.audio", "fallback": "Audio" }
    }
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();
    manifest.validate_keybinds().unwrap();

    let action = &manifest.keybinds.actions["mute"];
    assert_eq!(
        action.label.as_ref(),
        Some(&LocalizedText::Translation {
            key: "keybind.mute.label".into(),
            fallback: "Mute".into(),
        })
    );
    assert_eq!(
        action
            .description
            .as_ref()
            .map(LocalizedText::fallback_text),
        Some("Mute audio")
    );
    assert_eq!(
        action
            .category
            .as_ref()
            .and_then(LocalizedText::translation_key),
        Some("keybind.category.audio")
    );
}

#[test]
fn manifest_localized_text_raw_string_is_literal() {
    let text: LocalizedText = serde_json::from_str(r#""Mute""#).unwrap();

    assert_eq!(text, LocalizedText::Literal("Mute".into()));
    assert_eq!(text.fallback_text(), "Mute");
    assert_eq!(text.translation_key(), None);
    text.validate("mesh.keybinds.mute.label").unwrap();
}

#[test]
fn manifest_localized_text_object_is_translation() {
    let text: LocalizedText =
        serde_json::from_str(r#"{ "t": "keybind.mute.label", "fallback": "Mute" }"#).unwrap();

    assert_eq!(
        text,
        LocalizedText::Translation {
            key: "keybind.mute.label".into(),
            fallback: "Mute".into(),
        }
    );
    assert_eq!(text.fallback_text(), "Mute");
    assert_eq!(text.translation_key(), Some("keybind.mute.label"));
}

#[test]
fn manifest_localized_text_empty_key_is_rejected() {
    let text: LocalizedText = serde_json::from_str(r#"{ "t": " ", "fallback": "Mute" }"#).unwrap();

    let err = text.validate("mesh.keybinds.mute.label").unwrap_err();

    assert!(err.contains("mesh.keybinds.mute.label.t cannot be empty"));
}

#[test]
fn manifest_localized_text_empty_fallback_is_rejected() {
    let text: LocalizedText =
        serde_json::from_str(r#"{ "t": "keybind.mute.label", "fallback": "" }"#).unwrap();

    let err = text.validate("mesh.keybinds.mute.label").unwrap_err();

    assert!(err.contains("mesh.keybinds.mute.label.fallback cannot be empty"));
}

#[test]
fn localized_keybind_trigger_blank_key_is_manifest_valid() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "accept": {
      "trigger": {
        "kind": "access_key",
        "key": "a"
      },
      "localized_triggers": {
        "sk": {
          "kind": "access_key",
          "key": " "
        }
      }
    }
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();

    manifest.validate_keybinds().unwrap();
    assert_eq!(
        manifest.keybinds.actions["accept"].trigger.key.as_deref(),
        Some("a")
    );
}

#[test]
fn localized_keybind_trigger_empty_locale_is_rejected() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "accept": {
      "trigger": {
        "kind": "access_key",
        "key": "a"
      },
      "localized_triggers": {
        " ": {
          "kind": "access_key",
          "key": "p"
        }
      }
    }
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();
    let err = manifest.validate_keybinds().unwrap_err();

    assert!(err.contains("localized_triggers cannot contain empty locale ids"));
}

#[test]
fn keybind_trigger_unsupported_modifier_is_rejected() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "accept": {
      "trigger": {
        "kind": "shortcut",
        "key": "a",
        "modifiers": ["meta"]
      }
    }
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();
    let err = manifest.validate_keybinds().unwrap_err();

    assert!(err.contains("unsupported modifier 'meta'"));
}

#[test]
fn localized_keybind_trigger_unsupported_modifier_is_rejected() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "accept": {
      "trigger": {
        "kind": "shortcut",
        "key": "a"
      },
      "localized_triggers": {
        "sk": {
          "kind": "shortcut",
          "key": "p",
          "modifiers": ["super"]
        }
      }
    }
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();
    let err = manifest.validate_keybinds().unwrap_err();

    assert!(err.contains("unsupported modifier 'super'"));
}

#[test]
fn module_json_without_keybinds_has_empty_keybinds() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1"
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();

    assert!(manifest.keybinds.is_empty());
    manifest.validate_keybinds().unwrap();
}

#[test]
fn keybind_declaration_without_default_trigger_is_valid() {
    let content = r#"
{
  "id": "@mesh/panel",
  "version": "0.1.0",
  "type": "surface",
  "api_version": "0.1",
  "keybinds": {
    "mute": {}
  }
}
"#;

    let parsed: JsonManifest = serde_json::from_str(content).unwrap();
    let manifest = parsed.into_manifest();

    manifest.validate_keybinds().unwrap();
    assert_eq!(manifest.keybinds.actions["mute"].trigger.key, None);
}

#[test]
fn canonical_module_json_keybinds_round_trip_to_runtime_manifest() {
    let input = r#"{
  "name": "@mesh/panel",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "keybinds": {
      "mute": {
        "trigger": { "kind": "shortcut", "key": "m" },
        "localizedTriggers": {
          "sk": { "kind": "shortcut", "key": "s" }
        }
      }
    }
  }
}"#;

    let parsed = crate::package::ModuleManifest::from_json_str(input).unwrap();
    let manifest = parsed.into_runtime_manifest();
    let action = &manifest.keybinds.actions["mute"];

    assert_eq!(action.trigger.key.as_deref(), Some("m"));
    assert_eq!(action.localized_triggers["sk"].key.as_deref(), Some("s"));
}

#[test]
fn invalid_keybind_declaration_is_rejected() {
    let input = r#"{
  "name": "@mesh/panel",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "keybinds": {
      "mute": {
        "trigger": { "kind": "shortcut", "key": " " }
      }
    }
  }
}"#;

    let err = crate::package::ModuleManifest::from_json_str(input).unwrap_err();

    assert!(err.to_string().contains("trigger.key cannot be empty"));
}

#[test]
fn parses_canonical_module_json_module_manifest() {
    let dir = std::env::temp_dir().join(format!("mesh-canonical-module-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("module.json"),
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
    "implements": [
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
    assert_eq!(loaded.path, dir.join("module.json"));
    assert_eq!(loaded.source, ManifestSource::CanonicalModuleJson);
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
        package: ModuleSection {
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
        keybinds: KeybindsSection::default(),
        i18n: None,
        theme: None,
        service: None,
        provides: Vec::new(),
        interface: None,
        interfaces: Vec::new(),
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
