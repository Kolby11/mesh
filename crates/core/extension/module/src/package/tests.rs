use super::*;
use crate::ModuleType;
use crate::manifest::CapabilitiesSection;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    old: Option<String>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let old = std::env::var(key).ok();
        unsafe {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        Self {
            key,
            old,
            _lock: lock,
        }
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
fn module_root_manifest_rejects_legacy_top_level_shape() {
    let content = r#"
{
  "schemaVersion": 1,
  "modulesDir": "modules",
  "modules": {},
  "providers": {},
  "layout": { "entrypoint": "@mesh/panel:main" }
}
"#;
    let err = RootModuleGraphManifest::from_json_str(content).unwrap_err();
    assert!(
        err.to_string()
            .contains("root module graph must use canonical name/version/mesh shape")
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
    "uses": {
      "capabilities": ["exec.wpctl"],
      "binaries": [{ "name": "wpctl", "reason": "PipeWire control" }]
    },
    "i18n": { "defaultLocale": "en", "supportedLocales": ["en", "sk"] },
    "entry": "src/main.luau",
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
fn compact_surface_block_normalizes_into_surface_layout() {
    let content = r#"
{
  "name": "@mesh/panel",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "surface": {
      "anchor": "bottom",
      "layer": "overlay",
      "exclusive_zone": 48,
      "keyboard_mode": "on_demand",
      "visible_on_start": true
    }
  }
}
"#;
    let manifest = ModuleManifest::from_json_str(content).unwrap();
    // The compact `mesh.surface` block is moved into the single typed
    // `surface_layout` home during normalization. It carries placement only —
    // sizing and the show/hide transition are CSS concerns now.
    assert!(manifest.mesh.surface.is_none());
    let surface = manifest
        .mesh
        .surface_layout
        .expect("surface_layout populated from compact block");
    assert_eq!(surface.anchor.as_deref(), Some("bottom"));
    assert_eq!(surface.layer.as_deref(), Some("overlay"));
    assert_eq!(surface.exclusive_zone, Some(48));
    assert_eq!(surface.keyboard_mode.as_deref(), Some("on_demand"));
    assert_eq!(surface.visible_on_start, Some(true));
}

#[test]
fn interface_module_without_contract_file_is_valid() {
    // v0: an interface module may ship only name/version/domain and infer the
    // contract from emitted state — no `interface.toml` required.
    let content = r#"
{
  "name": "@me/cputemp-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "me.cputemp",
      "version": "1.0",
      "domain": "thermal"
    }
  }
}
"#;
    let manifest = ModuleManifest::from_json_str(content).unwrap();
    let interface = manifest.mesh.interface.unwrap();
    assert_eq!(interface.name, "me.cputemp");
    assert!(interface.file.is_none());
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
            "transition": "background-color var(--animation-duration-short) var(--animation-curves-bezier-standard)"
          },
          "button": {
            "background": "var(--weather-color-sunny)"
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
        "var(--weather-color-sunny)"
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
fn module_package_manifest_rejects_non_icon_pack_icon_pack_contribution() {
    let content = r##"
{
  "name": "@mesh/bad-icon-frontend",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "icon_pack": {
      "id": "bad",
      "mappings": {
        "audio-volume-high": "bad/audio-volume-high"
      }
    }
  }
}
"##;
    let err = ModuleManifest::from_json_str(content).unwrap_err();
    assert!(err.to_string().contains("icon-pack modules"));
}

#[test]
fn module_package_manifest_rejects_resource_pack_contributions_from_wrong_kind() {
    let bad_icons = r##"
{
  "name": "@mesh/bad-icons",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "provides": {
      "icons": [{ "id": "bad", "path": "icons" }]
    }
  }
}
"##;
    let bad_fonts = r##"
{
  "name": "@mesh/bad-fonts",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "provides": {
      "fonts": [{ "id": "bad", "path": "fonts" }]
    }
  }
}
"##;
    let bad_themes = r##"
{
  "name": "@mesh/bad-themes",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "provides": {
      "themes": [{
        "id": "bad",
        "label": "Bad",
        "modes": { "dark": "themes/dark/theme.css" }
      }]
    }
  }
}
"##;

    assert!(
        ModuleManifest::from_json_str(bad_icons)
            .unwrap_err()
            .to_string()
            .contains("icon-pack modules")
    );
    assert!(
        ModuleManifest::from_json_str(bad_fonts)
            .unwrap_err()
            .to_string()
            .contains("font-pack modules")
    );
    assert!(
        ModuleManifest::from_json_str(bad_themes)
            .unwrap_err()
            .to_string()
            .contains("theme modules")
    );
}

#[test]
fn module_package_manifest_rejects_dependency_capability_bucket_mismatches() {
    let interface_as_capability = r##"
{
  "name": "@mesh/bad-interface-capability",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "uses": {
      "capabilities": ["mesh.audio"]
    }
  }
}
"##;
    let capability_as_module = r##"
{
  "name": "@mesh/bad-capability-module",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "uses": {
      "modules": {
        "service.audio.read": "*"
      }
    }
  }
}
"##;
    let module_as_interface = r##"
{
  "name": "@mesh/bad-module-interface",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "uses": {
      "interfaces": {
        "@mesh/audio-interface": ">=1.0"
      }
    }
  }
}
"##;

    assert!(
        ModuleManifest::from_json_str(interface_as_capability)
            .unwrap_err()
            .to_string()
            .contains("interfaces belong in mesh.uses.interfaces")
    );
    assert!(
        ModuleManifest::from_json_str(capability_as_module)
            .unwrap_err()
            .to_string()
            .contains("host powers belong in mesh.uses.capabilities")
    );
    assert!(
        ModuleManifest::from_json_str(module_as_interface)
            .unwrap_err()
            .to_string()
            .contains("module ids belong in mesh.uses.modules")
    );
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
fn module_manifest_loader_warns_for_raw_dotted_keybind_label() {
    let dir = temp_dir("canonical-module-raw-keybind-label");
    fs::write(
        dir.join("module.json"),
        r#"{"name":"@mesh/module","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","keybinds":{"mute":{"label":"keybind.mute.label","trigger":{"kind":"shortcut","key":"m"}}}}}"#,
    )
    .unwrap();

    let loaded = load_module_manifest(&dir).unwrap();

    assert_eq!(loaded.source, ModuleManifestSource::CanonicalModuleJson);
    assert_eq!(loaded.diagnostics.len(), 1);
    let diagnostic = &loaded.diagnostics[0];
    assert_eq!(
        diagnostic.severity,
        ModuleManifestDiagnosticSeverity::Warning
    );
    assert_eq!(diagnostic.module_id.as_deref(), Some("@mesh/module"));
    assert_eq!(
        diagnostic.field_path.as_deref(),
        Some("mesh.keybinds.mute.label")
    );
    assert!(
        diagnostic
            .message
            .contains("looks like an i18n key but is a raw literal string")
    );
    assert!(
        diagnostic
            .suggested_action
            .contains(r#"{ "t": "keybind.mute.label", "fallback": "..." }"#)
    );
}

#[test]
fn module_manifest_loader_does_not_warn_for_literal_keybind_label() {
    let dir = temp_dir("canonical-module-literal-keybind-label");
    fs::write(
        dir.join("module.json"),
        r#"{"name":"@mesh/module","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","keybinds":{"mute":{"label":"Mute","trigger":{"kind":"shortcut","key":"m"}}}}}"#,
    )
    .unwrap();

    let loaded = load_module_manifest(&dir).unwrap();

    assert_eq!(loaded.source, ModuleManifestSource::CanonicalModuleJson);
    assert!(loaded.diagnostics.is_empty());
}

#[test]
fn module_manifest_loader_warns_for_raw_dotted_layout_label() {
    let dir = temp_dir("canonical-module-raw-layout-label");
    fs::write(
        dir.join("module.json"),
        r#"{"name":"@mesh/module","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","provides":{"layout":[{"id":"main","entrypoint":"src/main.mesh","label":"layout.main.label"}]}}}"#,
    )
    .unwrap();

    let loaded = load_module_manifest(&dir).unwrap();

    assert_eq!(loaded.source, ModuleManifestSource::CanonicalModuleJson);
    assert_eq!(loaded.diagnostics.len(), 1);
    let diagnostic = &loaded.diagnostics[0];
    assert_eq!(
        diagnostic.severity,
        ModuleManifestDiagnosticSeverity::Warning
    );
    assert_eq!(diagnostic.module_id.as_deref(), Some("@mesh/module"));
    assert_eq!(
        diagnostic.field_path.as_deref(),
        Some("mesh.provides.layout[0].label")
    );
    assert!(
        diagnostic
            .message
            .contains("looks like an i18n key but is a raw literal string")
    );
    assert!(
        diagnostic
            .suggested_action
            .contains(r#"{ "t": "layout.main.label", "fallback": "..." }"#)
    );
}

#[test]
fn module_manifest_loader_accepts_localized_layout_label_object() {
    let dir = temp_dir("canonical-module-localized-layout-label");
    fs::write(
        dir.join("module.json"),
        r#"{"name":"@mesh/module","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","provides":{"layout":[{"id":"main","entrypoint":"src/main.mesh","label":{"t":"layout.main.label","fallback":"Main"}}]}}}"#,
    )
    .unwrap();

    let loaded = load_module_manifest(&dir).unwrap();

    assert_eq!(loaded.source, ModuleManifestSource::CanonicalModuleJson);
    assert!(loaded.diagnostics.is_empty());
    assert_eq!(
        loaded.manifest.mesh.contributes.layout[0]
            .label
            .as_ref()
            .and_then(crate::manifest::LocalizedText::translation_key),
        Some("layout.main.label")
    );
}

#[test]
fn module_manifest_loader_rejects_legacy_package_json() {
    let dir = temp_dir("legacy-package");
    fs::write(
        dir.join("package.json"),
        r#"{"name":"@mesh/package","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend"}}"#,
    )
    .unwrap();
    let err = load_module_manifest(&dir).unwrap_err();
    let ModuleManifestError::Diagnostic { diagnostic } = err else {
        panic!("expected diagnostic error for legacy package.json");
    };
    assert_eq!(diagnostic.severity, ModuleManifestDiagnosticSeverity::Error);
    assert_eq!(
        diagnostic.suggested_action,
        "rename package.json to module.json"
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
fn module_manifest_loader_rejects_legacy_module_json() {
    let dir = temp_dir("legacy-module");
    fs::write(
        dir.join("module.json"),
        r#"{"id":"@mesh/module","version":"0.1.0","type":"surface","api_version":"0.1","entrypoints":{"main":"src/main.mesh"}}"#,
    )
    .unwrap();
    let err = load_module_manifest(&dir).unwrap_err();
    let ModuleManifestError::Diagnostic { diagnostic } = err else {
        panic!("expected diagnostic error for legacy module.json");
    };
    assert_eq!(diagnostic.severity, ModuleManifestDiagnosticSeverity::Error);
    assert_eq!(
        diagnostic.suggested_action,
        "replace legacy module.json fields with canonical name/version/mesh"
    );
}

#[test]
fn module_manifest_loader_rejects_legacy_mesh_toml() {
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
    let err = load_module_manifest(&dir).unwrap_err();
    let ModuleManifestError::Diagnostic { diagnostic } = err else {
        panic!("expected diagnostic error for legacy mesh.toml");
    };
    assert_eq!(diagnostic.severity, ModuleManifestDiagnosticSeverity::Error);
    assert_eq!(
        diagnostic.suggested_action,
        "replace mesh.toml with canonical module.json"
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
fn shipped_navigation_manifest_uses_explicit_localized_keybind_text() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/frontend/navigation-bar");
    let loaded = load_module_manifest(&dir).unwrap();

    assert!(
        loaded.diagnostics.iter().all(|diagnostic| {
            !diagnostic
                .message
                .contains("looks like an i18n key but is a raw literal string")
        }),
        "shipped navigation manifest should not use ambiguous raw i18n keys: {:?}",
        loaded.diagnostics
    );
    assert_eq!(
        loaded.manifest.mesh.i18n.default_locale.as_deref(),
        Some("en")
    );
    // supportedLocales removed from navigation-bar; locales declared once via provides.i18n
    assert!(loaded.manifest.mesh.i18n.supported_locales.is_empty());
    assert!(
        loaded
            .manifest
            .mesh
            .contributes
            .i18n
            .iter()
            .any(|entry| entry.locale == "en" && entry.path == "config/i18n/en.json")
    );
    assert!(
        loaded
            .manifest
            .mesh
            .contributes
            .i18n
            .iter()
            .any(|entry| entry.locale == "sk" && entry.path == "config/i18n/sk.json")
    );

    let action = loaded
        .manifest
        .mesh
        .keybinds
        .actions
        .get("mute")
        .expect("navigation mute keybind");
    assert_eq!(
        action.label,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.label".into(),
            fallback: "Mute audio".into(),
        })
    );
    assert_eq!(
        action.description,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.description".into(),
            fallback: "Toggle audio mute".into(),
        })
    );
    assert_eq!(
        action.category,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.category.audio".into(),
            fallback: "Audio".into(),
        })
    );
}

#[test]
fn shipped_module_graph_loads_repo_module_fixture() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let graph = load_installed_module_graph(&workspace_root.join("config/module.json")).unwrap();

    assert_eq!(graph.frontend_modules().len(), 3);
    let component_ids: std::collections::HashSet<_> = graph
        .modules_by_kind(ModuleKind::Component)
        .into_iter()
        .map(|module| module.id.as_str())
        .collect();
    assert_eq!(component_ids.len(), 2);
    assert!(component_ids.contains("@mesh/language-popover"));
    assert!(component_ids.contains("@mesh/theme-selector"));
    assert_eq!(
        graph
            .module("@mesh/navigation-bar")
            .unwrap()
            .manifest_source,
        ModuleManifestSource::CanonicalModuleJson
    );
    assert_eq!(
        graph
            .module("@mesh/audio-interface")
            .unwrap()
            .manifest_source,
        ModuleManifestSource::CanonicalModuleJson
    );
    assert_eq!(
        graph.module("@mesh/icons-default").unwrap().manifest_source,
        ModuleManifestSource::CanonicalModuleJson
    );
    assert_eq!(
        graph.declared_interface("mesh.audio").unwrap().module_id,
        "@mesh/audio-interface"
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
    assert!(
        graph
            .backend_providers_for_interface("mesh.audio")
            .iter()
            .any(|provider| provider.module_id == "@mesh/pulseaudio-audio")
    );
    let layout = graph.layout_entrypoint().unwrap();
    assert_eq!(layout.module_id, "@mesh/navigation-bar");
    assert_eq!(layout.entrypoint_id, "main");
    assert!(graph.frontend_entrypoints().iter().any(|entrypoint| {
        entrypoint.module_id == "@mesh/navigation-bar"
            && entrypoint.source.local_id == "main"
            && entrypoint.path == "src/main.mesh"
    }));
    assert!(
        graph
            .settings_schemas()
            .iter()
            .any(|settings| settings.namespace == "@mesh/navigation-bar")
    );
    assert!(
        graph
            .keybind_actions()
            .iter()
            .any(|keybind| keybind.module_id == "@mesh/navigation-bar"
                && keybind.action_id == "mute")
    );
    assert!(
        graph
            .icon_requirements()
            .iter()
            .any(|icon| icon.module_id == "@mesh/navigation-bar"
                && icon.name == "audio-volume-high"
                && icon.required)
    );
    assert!(
        graph
            .icon_pack_contributions()
            .iter()
            .any(|icon_pack| icon_pack.module_id == "@mesh/icons-default"
                && icon_pack.id == "default")
    );
}

#[test]
fn load_installed_module_graph_auto_discovers_modules() {
    let root = temp_dir("auto-discovery-test");
    let config_dir = root.join("config");
    let modules_dir = root.join("modules");
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(modules_dir.join("frontend/panel")).unwrap();
    fs::create_dir_all(modules_dir.join("backend/audio")).unwrap();

    fs::write(
        modules_dir.join("frontend/panel/module.json"),
        r#"{ "name": "@me/panel", "version": "0.1.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "entry": "src/main.mesh", "surface": { "anchor": "top" }, "accessibility": { "role": "toolbar" } } }"#,
    )
    .unwrap();
    fs::write(
        modules_dir.join("backend/audio/module.json"),
        r#"{ "name": "@me/audio-backend", "version": "0.1.0", "mesh": { "apiVersion": "0.1", "kind": "backend", "entry": "src/main.luau", "implements": [{ "interface": "me.audio", "version": "1.0", "provider": "demo" }] } }"#,
    )
    .unwrap();

    // Decisions-only root graph: no `modules` inventory, one disabled module.
    fs::write(
        config_dir.join("module.json"),
        r#"{ "name": "@me/config", "version": "0.1.0", "mesh": { "schemaVersion": 1, "modulesDir": "../modules", "disabled": ["@me/audio-backend"] } }"#,
    )
    .unwrap();

    let graph = load_installed_module_graph(&config_dir.join("module.json")).unwrap();

    // Both modules are discovered from disk without a `modules` map...
    assert!(graph.module("@me/panel").unwrap().enabled);
    // ...and the `disabled` decision is honored.
    assert!(!graph.module("@me/audio-backend").unwrap().enabled);
    let frontends = graph.frontend_modules();
    assert_eq!(frontends.len(), 1);
    assert_eq!(frontends[0].id, "@me/panel");
}

#[test]
fn shipped_module_graph_preserves_navigation_localized_keybind_text() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let graph = load_installed_module_graph(&workspace_root.join("config/module.json")).unwrap();
    let keybind = graph
        .keybind_actions()
        .iter()
        .find(|keybind| keybind.module_id == "@mesh/navigation-bar" && keybind.action_id == "mute")
        .expect("navigation mute keybind contribution");

    assert_eq!(
        keybind.label.as_ref(),
        Some(&crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.label".into(),
            fallback: "Mute audio".into(),
        })
    );
    assert_eq!(
        keybind.description.as_ref(),
        Some(&crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.description".into(),
            fallback: "Toggle audio mute".into(),
        })
    );
    assert_eq!(
        keybind.category.as_ref(),
        Some(&crate::manifest::LocalizedText::Translation {
            key: "keybind.category.audio".into(),
            fallback: "Audio".into(),
        })
    );
}

#[test]
fn shipped_module_diagnostics_report_missing_navigation_icon() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let mut navigation =
        load_module_manifest(&workspace_root.join("modules/frontend/navigation-bar")).unwrap();
    navigation
        .manifest
        .mesh
        .icon_requirements
        .required
        .push("missing-shipped-proof-icon".into());
    let icons = load_module_manifest(&workspace_root.join("modules/icon-packs/default")).unwrap();
    let root = root_with_modules(
        &[
            ("@mesh/navigation-bar", ModuleKind::Frontend),
            ("@mesh/icons-default", ModuleKind::IconPack),
        ],
        &[],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![navigation, icons]).unwrap();
    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.status == "missing_required_icon")
        .unwrap();

    assert_eq!(diagnostic.module_id, "@mesh/navigation-bar");
    assert!(
        diagnostic
            .contribution_id
            .as_deref()
            .is_some_and(|id| id.contains("required:missing-shipped-proof-icon"))
    );
}

#[test]
fn shipped_frontend_icon_literals_are_declared() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let module_dir = workspace_root.join("modules/frontend/navigation-bar");
    let loaded = load_module_manifest(&module_dir).unwrap();
    let declared = loaded
        .manifest
        .mesh
        .icon_requirements
        .required
        .iter()
        .chain(loaded.manifest.mesh.icon_requirements.optional.iter())
        .collect::<std::collections::HashSet<_>>();

    for source_path in [
        module_dir.join("src/main.mesh"),
        module_dir.join("src/components/battery-button.mesh"),
        module_dir.join("src/components/volume-button.mesh"),
        module_dir.join("src/components/now-playing.mesh"),
        module_dir.join("src/components/settings-button.mesh"),
        module_dir.join("src/components/theme-button.mesh"),
    ] {
        let source = fs::read_to_string(&source_path).unwrap();
        for icon in obvious_semantic_icon_literals(&source) {
            assert!(
                declared.contains(&icon),
                "{} uses semantic icon '{icon}' but @mesh/navigation-bar does not declare it in iconRequirements",
                source_path.display()
            );
        }
    }
}

fn obvious_semantic_icon_literals(source: &str) -> std::collections::HashSet<String> {
    let prefixes = [
        "audio-volume-",
        "battery-",
        "media-playback-",
        "preferences-",
        "weather-",
        "window-",
    ];
    source
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|literal| prefixes.iter().any(|prefix| literal.starts_with(prefix)))
        .filter(|literal| {
            !literal.ends_with("-widget")
                && !literal.ends_with("-button")
                && !literal.ends_with("-glyph")
                && !literal.ends_with("-value")
        })
        .map(str::to_string)
        .collect()
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
                entry: None,
                uses: MeshUses::default(),
                capabilities: CapabilitiesSection::default(),
                i18n: MeshI18nSupport::default(),
                entrypoints: MeshEntrypoints::default(),
                keybinds: crate::manifest::KeybindsSection::default(),
                dependencies,
                provides: MeshProvides::default(),
                implements: provides,
                interface: None,
                contributes,
                icons: None,
                icon_pack: None,
                icon_requirements: crate::manifest::IconRequirementsSection::default(),
                accessibility: None,
                surface: None,
                surface_layout: None,
                theme: None,
                experimental: serde_json::Value::Null,
            },
        },
        path: PathBuf::from(format!("{name}/module.json")),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    }
}

fn declare_frontend_surface_contract(module: &mut LoadedModuleManifest) {
    module.manifest.mesh.accessibility = Some(crate::manifest::AccessibilitySection {
        role: Some("application".into()),
        label: None,
        description: None,
    });
    module.manifest.mesh.surface_layout = Some(crate::manifest::SurfaceLayoutSection {
        keyboard_mode: Some("on_demand".into()),
        ..Default::default()
    });
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
        disabled: Vec::new(),
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
fn module_kind_to_legacy_module_type_keeps_specific_resource_kinds() {
    assert_eq!(ModuleType::from(ModuleKind::FontPack), ModuleType::FontPack);
    assert_eq!(ModuleType::from(ModuleKind::Library), ModuleType::Library);
    assert_eq!(
        ModuleType::from(ModuleKind::Component),
        ModuleType::Component
    );
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
                label: Some(crate::manifest::LocalizedText::Literal(
                    "PipeWire".to_string(),
                )),
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
                label: Some(crate::manifest::LocalizedText::Literal(
                    "PulseAudio".to_string(),
                )),
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
        .implements
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
                label: Some(crate::manifest::LocalizedText::Literal(
                    "Example".to_string(),
                )),
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
            label: Some(crate::manifest::LocalizedText::Literal(
                "Example".to_string(),
            )),
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
        MeshDependencies {
            modules: HashMap::from([(
                "@mesh/example-interface".into(),
                crate::manifest::DependencySpec::Simple(">=1.0.0".into()),
            )]),
            ..MeshDependencies::default()
        },
        vec![MeshProvidesDeclaration {
            interface: "mesh.example.alt".into(),
            version: Some("1.0.0".into()),
            base_module: None,
            provider: Some("example-alt".into()),
            label: Some(crate::manifest::LocalizedText::Literal(
                "Example Alt".to_string(),
            )),
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
fn installed_module_graph_auto_selects_sole_provider() {
    // No explicit `providers` entry, exactly one enabled implementer.
    let root = root_with_modules(&[("@mesh/pipewire-audio", ModuleKind::Backend)], &[], None);
    let modules = vec![audio_modules().remove(0)];
    let graph = InstalledModuleGraph::from_parts(root, modules).unwrap();
    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
}

#[test]
fn installed_module_graph_does_not_auto_select_among_multiple_providers() {
    // Two implementers and no explicit selection: the choice stays unresolved.
    let root = root_with_modules(
        &[
            ("@mesh/pipewire-audio", ModuleKind::Backend),
            ("@mesh/pulseaudio-audio", ModuleKind::Backend),
        ],
        &[],
        None,
    );
    let graph = InstalledModuleGraph::from_parts(root, audio_modules()).unwrap();
    assert!(graph.active_provider("mesh.audio").is_none());
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
}

#[test]
fn installed_module_graph_supports_backend_without_interface_module() {
    // A standalone backend implements an interface with no separate interface
    // module and no contract file. The graph builds clean, auto-selects the
    // sole provider, and emits no contract/interface-module diagnostics.
    let root = root_with_modules(&[("@me/cputemp-backend", ModuleKind::Backend)], &[], None);
    let backend = loaded_module(
        "@me/cputemp-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "me.cputemp".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some("lmsensors".into()),
            label: None,
            priority: 100,
        }],
        MeshContributes::default(),
    );
    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();
    assert_eq!(
        graph.active_provider("me.cputemp").unwrap().module_id,
        "@me/cputemp-backend"
    );
    assert!(graph.diagnostics().iter().all(|diagnostic| {
        diagnostic.status != "missing_interface_contract_file"
            && diagnostic.status != "missing_provider_interface_module_dependency"
    }));
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
            label: Some(crate::manifest::LocalizedText::Literal(
                "NetworkManager".to_string(),
            )),
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
    modes.insert("dark".into(), "themes/dark/theme.css".into());
    let theme_contributes = MeshContributes {
        themes: vec![ThemeContribution {
            id: "mesh-default".into(),
            label: Some(crate::manifest::LocalizedText::Literal(
                "MESH Default".to_string(),
            )),
            modes,
            default_mode: Some("dark".into()),
        }],
        ..MeshContributes::default()
    };
    let icon_contributes = MeshContributes {
        icons: vec![PathContribution {
            id: "material".into(),
            path: "icons".into(),
            label: None,
        }],
        ..MeshContributes::default()
    };
    let font_contributes = MeshContributes {
        fonts: vec![PathContribution {
            id: "inter".into(),
            path: "fonts".into(),
            label: None,
        }],
        ..MeshContributes::default()
    };
    let i18n_contributes = MeshContributes {
        i18n: vec![I18nContribution {
            id: "en".into(),
            locale: "en".into(),
            path: "i18n/en.json".into(),
        }],
        ..MeshContributes::default()
    };
    let root = root_with_modules(
        &[
            ("@mesh/theme", ModuleKind::Theme),
            ("@mesh/icons", ModuleKind::IconPack),
            ("@mesh/fonts", ModuleKind::FontPack),
            ("@mesh/lang-en", ModuleKind::LanguagePack),
        ],
        &[],
        None,
    );
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![
            loaded_module(
                "@mesh/theme",
                ModuleKind::Theme,
                MeshDependencies::default(),
                vec![],
                theme_contributes,
            ),
            loaded_module(
                "@mesh/icons",
                ModuleKind::IconPack,
                MeshDependencies::default(),
                vec![],
                icon_contributes,
            ),
            loaded_module(
                "@mesh/fonts",
                ModuleKind::FontPack,
                MeshDependencies::default(),
                vec![],
                font_contributes,
            ),
            loaded_module(
                "@mesh/lang-en",
                ModuleKind::LanguagePack,
                MeshDependencies::default(),
                vec![],
                i18n_contributes,
            ),
        ],
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
        let mut module = loaded_module(
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
        );
        module.manifest.mesh.icon_pack = Some(crate::manifest::IconPackSection {
            id: module_id
                .rsplit('/')
                .next()
                .unwrap_or(module_id)
                .trim_start_matches("icons-")
                .into(),
            mappings: HashMap::from([(
                "audio-volume-high".into(),
                format!("{module_id}/audio-volume-high"),
            )]),
            ..crate::manifest::IconPackSection::default()
        });
        module
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
    let mut icon_pack_ids = graph
        .icon_pack_contributions()
        .iter()
        .map(|pack| format!("{}:{}", pack.module_id, pack.id))
        .collect::<Vec<_>>();
    icon_pack_ids.sort();
    assert_eq!(
        icon_pack_ids,
        vec!["@mesh/icons-a:a".to_string(), "@mesh/icons-b:b".to_string()]
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
        ModuleManifestSource::CanonicalModuleJson
    );
    assert!(
        icon.source
            .manifest_path
            .ends_with("@mesh/icons-a/module.json")
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
    declare_frontend_surface_contract(&mut frontend);
    frontend.manifest.mesh.keybinds.actions.insert(
        "mute".into(),
        crate::manifest::KeybindAction {
            label: Some(crate::manifest::LocalizedText::Literal("Mute".to_string())),
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
        MeshDependencies {
            modules: HashMap::from([(
                "@mesh/example-interface".into(),
                crate::manifest::DependencySpec::Simple(">=1.0.0".into()),
            )]),
            ..MeshDependencies::default()
        },
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
    assert_eq!(graph.frontend_surfaces().len(), 1);
    assert_eq!(graph.frontend_surfaces()[0].path, "src/main.mesh");
    assert_eq!(
        graph.frontend_surfaces()[0].settings_namespace.as_deref(),
        Some("@mesh/example-widget")
    );
    assert!(graph.frontend_surfaces()[0].accessibility.is_some());
    assert!(graph.frontend_surfaces()[0].surface_layout.is_some());
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
fn contribution_index_preserves_keybind_localized_text() {
    let mut frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.keybinds.actions.insert(
        "mute".into(),
        crate::manifest::KeybindAction {
            label: Some(crate::manifest::LocalizedText::Translation {
                key: "keybind.mute.label".into(),
                fallback: "Mute".into(),
            }),
            description: Some(crate::manifest::LocalizedText::Translation {
                key: "keybind.mute.description".into(),
                fallback: "Mute audio".into(),
            }),
            category: Some(crate::manifest::LocalizedText::Translation {
                key: "keybind.category.audio".into(),
                fallback: "Audio".into(),
            }),
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let root = root_with_modules(&[("@mesh/example-widget", ModuleKind::Frontend)], &[], None);

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();
    let keybind = &graph.keybind_actions()[0];

    assert_eq!(
        keybind.label,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.label".into(),
            fallback: "Mute".into()
        })
    );
    assert_eq!(
        keybind.description,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.description".into(),
            fallback: "Mute audio".into()
        })
    );
    assert_eq!(
        keybind.category,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.category.audio".into(),
            fallback: "Audio".into()
        })
    );
    assert_eq!(keybind.label_text(), Some("Mute"));
    assert_eq!(keybind.description_text(), Some("Mute audio"));
    assert_eq!(keybind.category_text(), Some("Audio"));
}

#[test]
fn contribution_index_preserves_layout_localized_text() {
    let mut contributes = MeshContributes::default();
    contributes.layout.push(LayoutContribution {
        id: "main".into(),
        entrypoint: "src/main.mesh".into(),
        label: Some(crate::manifest::LocalizedText::Translation {
            key: "layout.main.label".into(),
            fallback: "Main shell".into(),
        }),
    });
    let frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        contributes,
    );
    let root = root_with_modules(
        &[("@mesh/example-widget", ModuleKind::Frontend)],
        &[],
        Some("@mesh/example-widget:main"),
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();
    let layout = graph
        .contributed_layouts()
        .iter()
        .find(|layout| layout.id == "main")
        .unwrap();

    assert_eq!(
        layout.label,
        Some(crate::manifest::LocalizedText::Translation {
            key: "layout.main.label".into(),
            fallback: "Main shell".into()
        })
    );
    assert_eq!(layout.label_text(), Some("Main shell"));
    assert_eq!(
        graph.layout_entrypoint().unwrap().module_id,
        "@mesh/example-widget"
    );

    let parsed = ModuleManifest::from_json_str(
        r#"{
          "name": "@mesh/layout",
          "version": "0.1.0",
          "mesh": {
            "apiVersion": "0.1",
            "kind": "frontend",
            "provides": {
              "layout": [
                {
                  "id": "main",
                  "entrypoint": "src/main.mesh",
                  "label": { "t": "layout.main.label", "fallback": "Main shell" }
                }
              ]
            }
          }
        }"#,
    )
    .unwrap();
    assert_eq!(
        parsed.mesh.contributes.layout[0].label,
        Some(crate::manifest::LocalizedText::Translation {
            key: "layout.main.label".into(),
            fallback: "Main shell".into()
        })
    );
}

#[test]
fn contribution_index_preserves_settings_schema_localized_descriptions() {
    let mut contributes = MeshContributes::default();
    contributes.settings = Some(SettingsContribution {
        namespace: "@mesh/example-widget".into(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "description": {
                        "t": "settings.mode.description",
                        "fallback": "Theme mode"
                    }
                }
            }
        }),
    });
    let frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        contributes,
    );
    let root = root_with_modules(&[("@mesh/example-widget", ModuleKind::Frontend)], &[], None);

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();
    let description = &graph.settings_schemas()[0].schema["properties"]["mode"]["description"];

    assert_eq!(description["t"], "settings.mode.description");
    assert_eq!(description["fallback"], "Theme mode");
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
fn graph_diagnostics_report_required_icon_without_enabled_icon_pack() {
    let root = root_with_modules(&[("@mesh/example-widget", ModuleKind::Frontend)], &[], None);
    let mut frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.icon_requirements.required = vec!["audio-volume-high".into()];

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.status == "missing_required_icon")
        .unwrap();
    assert_eq!(diagnostic.module_id, "@mesh/example-widget");
    assert!(diagnostic.message.contains("required semantic icon"));
    assert!(diagnostic.message.contains("audio-volume-high"));
}

#[test]
fn graph_diagnostics_report_optional_icon_missing_mapping() {
    let root = root_with_modules(
        &[
            ("@mesh/example-widget", ModuleKind::Frontend),
            ("@mesh/icons-material", ModuleKind::IconPack),
        ],
        &[],
        None,
    );
    let mut frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.icon_requirements.optional = vec!["weather-clear".into()];
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

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend, icon_pack]).unwrap();

    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.status == "missing_optional_icon")
        .unwrap();
    assert_eq!(diagnostic.module_id, "@mesh/example-widget");
    assert!(
        diagnostic
            .contribution_id
            .as_deref()
            .is_some_and(|id| id.contains("optional:weather-clear"))
    );
    assert!(diagnostic.message.contains("optional semantic icon"));
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
        disabled: Vec::new(),
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
    declare_frontend_surface_contract(&mut frontend);
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
        MeshDependencies {
            modules: HashMap::from([(
                "@mesh/example-interface".into(),
                crate::manifest::DependencySpec::Simple(">=1.0.0".into()),
            )]),
            ..MeshDependencies::default()
        },
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/example-interface".into()),
            provider: Some("example".into()),
            label: Some(crate::manifest::LocalizedText::Literal(
                "Example".to_string(),
            )),
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
                label: Some(crate::manifest::LocalizedText::Literal("Inter".to_string())),
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
    theme_modes.insert("dark".into(), "themes/dark/theme.css".into());
    let theme = loaded_module(
        "@mesh/example-theme",
        ModuleKind::Theme,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            themes: vec![ThemeContribution {
                id: "mesh-default".into(),
                label: Some(crate::manifest::LocalizedText::Literal(
                    "Default".to_string(),
                )),
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

    assert!(
        graph.diagnostics().is_empty(),
        "expected no diagnostics, got: {:?}",
        graph.diagnostics()
    );
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

#[test]
fn entry_auto_generates_default_layout_contribution_for_frontend() {
    let content = r#"
{
  "name": "@mesh/simple-frontend",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "uses": { "capabilities": ["shell.surface"] }
  }
}
"#;
    let manifest = ModuleManifest::from_json_str(content).unwrap();
    assert_eq!(manifest.mesh.contributes.layout.len(), 1);
    let layout = &manifest.mesh.contributes.layout[0];
    assert_eq!(layout.id, "main");
    assert_eq!(layout.entrypoint, "src/main.mesh");
}

#[test]
fn explicit_provides_layout_is_not_overridden_by_entry() {
    let content = r#"
{
  "name": "@mesh/custom-frontend",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "provides": {
      "layout": [
        { "id": "compact", "entrypoint": "src/compact.mesh" },
        { "id": "full",    "entrypoint": "src/full.mesh" }
      ]
    }
  }
}
"#;
    let manifest = ModuleManifest::from_json_str(content).unwrap();
    assert_eq!(manifest.mesh.contributes.layout.len(), 2);
    assert!(
        manifest
            .mesh
            .contributes
            .layout
            .iter()
            .any(|l| l.id == "compact")
    );
    assert!(
        manifest
            .mesh
            .contributes
            .layout
            .iter()
            .any(|l| l.id == "full")
    );
}

#[test]
fn backend_entry_does_not_auto_generate_layout_contribution() {
    let content = r#"
{
  "name": "@mesh/my-backend",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "entry": "src/main.luau",
    "implements": [{ "interface": "mesh.audio" }]
  }
}
"#;
    let manifest = ModuleManifest::from_json_str(content).unwrap();
    assert!(manifest.mesh.contributes.layout.is_empty());
}

#[test]
fn uses_icon_requirements_normalized_into_mesh_icon_requirements() {
    let content = r#"
{
  "name": "@mesh/example-frontend",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "uses": {
      "iconRequirements": {
        "required": ["audio-volume-high"],
        "optional": ["audio-volume-low"]
      },
      "capabilities": ["shell.surface"]
    },
    "entry": "src/main.mesh"
  }
}
"#;
    let manifest = ModuleManifest::from_json_str(content).unwrap();
    assert!(
        manifest
            .mesh
            .icon_requirements
            .required
            .contains(&"audio-volume-high".into())
    );
    assert!(
        manifest
            .mesh
            .icon_requirements
            .optional
            .contains(&"audio-volume-low".into())
    );
}

#[test]
fn uses_icon_requirements_merges_with_top_level_icon_requirements() {
    let content = r#"
{
  "name": "@mesh/example-frontend",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "uses": {
      "iconRequirements": { "required": ["audio-volume-high"] }
    },
    "iconRequirements": {
      "required": ["audio-volume-muted"],
      "optional": ["audio-volume-low"]
    },
    "entry": "src/main.mesh"
  }
}
"#;
    let manifest = ModuleManifest::from_json_str(content).unwrap();
    let required = &manifest.mesh.icon_requirements.required;
    assert!(required.contains(&"audio-volume-high".into()));
    assert!(required.contains(&"audio-volume-muted".into()));
    assert!(
        manifest
            .mesh
            .icon_requirements
            .optional
            .contains(&"audio-volume-low".into())
    );
}

#[test]
fn graph_diagnostics_report_missing_required_binary() {
    let dep = MeshDependencies {
        binaries: vec![crate::manifest::BinaryDependency {
            name: "this-binary-definitely-does-not-exist-on-any-system-12345".into(),
            version: None,
            reason: Some("test binary".into()),
            optional: false,
            packages: HashMap::from([
                ("arch".into(), "test-bin-arch".into()),
                ("debian".into(), "test-bin-deb".into()),
            ]),
        }],
        ..MeshDependencies::default()
    };
    let root = root_with_modules(&[("@mesh/backend", ModuleKind::Backend)], &[], None);
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![loaded_module(
            "@mesh/backend",
            ModuleKind::Backend,
            dep,
            vec![],
            MeshContributes::default(),
        )],
    )
    .unwrap();
    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|d| d.status == "missing_required_binary")
        .expect("missing_required_binary diagnostic");
    assert!(diagnostic.message.contains("arch:test-bin-arch"));
    assert!(diagnostic.message.contains("debian:test-bin-deb"));
}

#[test]
fn graph_diagnostics_skip_optional_missing_binary() {
    let dep = MeshDependencies {
        binaries: vec![crate::manifest::BinaryDependency {
            name: "this-binary-definitely-does-not-exist-on-any-system-12345".into(),
            version: None,
            reason: None,
            optional: true,
            packages: Default::default(),
        }],
        ..MeshDependencies::default()
    };
    let root = root_with_modules(&[("@mesh/backend", ModuleKind::Backend)], &[], None);
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![loaded_module(
            "@mesh/backend",
            ModuleKind::Backend,
            dep,
            vec![],
            MeshContributes::default(),
        )],
    )
    .unwrap();
    assert!(
        !graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "missing_required_binary")
    );
}

#[test]
fn extract_icon_names_from_mesh_source_finds_static_names() {
    use super::installed_graph::extract_icon_names_from_mesh_source;
    let src = r#"<icon name="audio-volume-high" size="24"/><icon name="battery-full"/>"#;
    let names = extract_icon_names_from_mesh_source(src);
    assert!(names.contains(&"audio-volume-high".into()));
    assert!(names.contains(&"battery-full".into()));
}

#[test]
fn extract_icon_names_ignores_dynamic_expressions() {
    use super::installed_graph::extract_icon_names_from_mesh_source;
    let src = r#"<icon name="{icon_name}" /><icon name="audio-volume-muted"/>"#;
    let names = extract_icon_names_from_mesh_source(src);
    assert!(!names.iter().any(|n| n.contains('{')));
    assert!(names.contains(&"audio-volume-muted".into()));
}

#[test]
fn library_module_with_required_capabilities_is_rejected() {
    let content = r#"
{
  "name": "@mesh/example-lib",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "library",
    "capabilities": { "required": ["exec.run"] }
  }
}
"#;
    let result = ModuleManifest::from_json_str(content);
    assert!(
        result.is_err(),
        "library module must not declare required capabilities"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("library modules must not"),
        "error message was: {err}"
    );
}

#[test]
fn library_module_with_no_capabilities_is_accepted() {
    let content = r#"
{
  "name": "@mesh/example-lib",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "library"
  }
}
"#;
    let result = ModuleManifest::from_json_str(content);
    assert!(
        result.is_ok(),
        "library module with no capabilities should be valid"
    );
}

#[test]
fn graph_diagnostics_report_missing_interface_contract_file() {
    // Use a real temp dir so the module directory exists; the contract file is deliberately absent.
    let dir = temp_dir("interface-contract-test");
    let manifest_path = dir.join("module.json");
    let root = root_with_modules(
        &[("@mesh/test-interface", ModuleKind::Interface)],
        &[],
        None,
    );
    let mut iface = loaded_module(
        "@mesh/test-interface",
        ModuleKind::Interface,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    iface.manifest.mesh.interface =
        Some(crate::package::module_manifest::MeshInterfaceDeclaration {
            name: "mesh.test".into(),
            version: Some("1.0".into()),
            file: Some("contract.toml".into()),
            domain: Some("test".into()),
            extends: None,
            relationship: Some(crate::package::module_manifest::InterfaceRelationship::Base),
            reason: None,
        });
    iface.path = manifest_path;
    let graph = InstalledModuleGraph::from_parts(root, vec![iface]).unwrap();
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "missing_interface_contract_file"),
        "expected missing_interface_contract_file diagnostic; got: {:?}",
        graph.diagnostics()
    );
}

#[test]
fn graph_diagnostics_report_duplicate_keybind_trigger() {
    let root = root_with_modules(
        &[
            ("@mesh/mod-a", ModuleKind::Frontend),
            ("@mesh/mod-b", ModuleKind::Frontend),
        ],
        &[],
        None,
    );
    let mut mod_a = loaded_module(
        "@mesh/mod-a",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    mod_a.manifest.mesh.keybinds.actions.insert(
        "toggle".into(),
        crate::manifest::KeybindAction {
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("t".into()),
                modifiers: vec!["ctrl".into()],
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let mut mod_b = loaded_module(
        "@mesh/mod-b",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    mod_b.manifest.mesh.keybinds.actions.insert(
        "open".into(),
        crate::manifest::KeybindAction {
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("t".into()),
                modifiers: vec!["ctrl".into()],
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let graph = InstalledModuleGraph::from_parts(root, vec![mod_a, mod_b]).unwrap();
    let dupes: Vec<_> = graph
        .diagnostics()
        .iter()
        .filter(|d| d.status == "duplicate_keybind_trigger")
        .collect();
    assert_eq!(
        dupes.len(),
        2,
        "both conflicting actions should get a diagnostic"
    );
}

#[test]
fn extract_t_keys_from_mesh_source_finds_static_keys() {
    use super::installed_graph::extract_t_keys_from_mesh_source;
    let src = r#"
        <text>{t('nav.volume')}</text>
        <text aria-label="{t("nav.mute")}"/>
        <text>{t(dynamic_key)}</text>
    "#;
    let keys = extract_t_keys_from_mesh_source(src);
    assert!(
        keys.contains(&"nav.volume".into()),
        "single-quote key should be found"
    );
    assert!(
        keys.contains(&"nav.mute".into()),
        "double-quote key should be found"
    );
    assert!(
        !keys.iter().any(|k: &String| k.contains("dynamic")),
        "dynamic key must not appear"
    );
}

#[test]
fn extract_t_keys_ignores_dynamic_expressions() {
    use super::installed_graph::extract_t_keys_from_mesh_source;
    let src = r#"{t(audio_title_key)}{t("audio.fixed")}"#;
    let keys = extract_t_keys_from_mesh_source(src);
    assert_eq!(keys, vec!["audio.fixed".to_string()]);
}

#[test]
fn extract_mesh_event_publish_channels_finds_static_channels() {
    use super::installed_graph::extract_mesh_event_publish_channels;

    let src = r#"
<script>
mesh.events.publish("shell.set-theme", { theme_id = "dark" })
mesh.events.publish('mesh.hyprland.switch_workspace', { id = 1 })
</script>
"#;
    let channels = extract_mesh_event_publish_channels(src);
    assert_eq!(
        channels,
        vec!["mesh.hyprland.switch_workspace", "shell.set-theme"]
    );
}

#[test]
fn extract_mesh_event_publish_channels_ignores_dynamic_channels() {
    use super::installed_graph::extract_mesh_event_publish_channels;

    let src = r#"
<script>
local channel = "mesh." .. domain
mesh.events.publish(channel, {})
</script>
"#;
    let channels = extract_mesh_event_publish_channels(src);
    assert!(channels.is_empty());
}

#[test]
fn extract_backend_emit_event_names_finds_static_events() {
    use super::installed_graph::extract_backend_emit_event_names;

    let src = r#"
function on_poll()
    mesh.service.emit_event("VolumeChanged", { level = 67 })
    mesh.service.emit_event('DeviceChanged', { id = "default" })
end
"#;
    let names = extract_backend_emit_event_names(src);
    assert_eq!(names, vec!["DeviceChanged", "VolumeChanged"]);
}

#[test]
fn extract_keybind_subscriptions_from_mesh_source_finds_static_actions() {
    use super::installed_graph::extract_keybind_subscriptions_from_mesh_source;

    let src = r#"
<template>
  <button keybind="{this.keybinds.mute.id}" onkeybind={onMute}></button>
  <button keybind="open"></button>
  <button keybind="{dynamic_id}" onkeybind={onDynamic}></button>
</template>
"#;
    let subscriptions = extract_keybind_subscriptions_from_mesh_source(src);
    assert_eq!(
        subscriptions,
        vec![("mute".to_string(), true), ("open".to_string(), false)]
    );
}

#[test]
fn extract_keybind_subscriptions_handles_quoted_angle_brackets_in_tag() {
    use super::installed_graph::extract_keybind_subscriptions_from_mesh_source;

    let src = r#"
<template>
  <button title="2 < 3" keybind="open" data-note="x > y" onkeybind={onOpen}></button>
</template>
"#;
    let subscriptions = extract_keybind_subscriptions_from_mesh_source(src);
    assert_eq!(subscriptions, vec![("open".to_string(), true)]);
}

#[test]
fn graph_diagnostics_report_undeclared_i18n_key() {
    let dir = temp_dir("i18n-key-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let catalog_dir = dir.join("config").join("i18n");
    fs::create_dir_all(&catalog_dir).unwrap();

    // Write a .mesh file that uses a key not present in the catalog.
    fs::write(
        src_dir.join("main.mesh"),
        r#"<text>{t('nav.volume')}{t('nav.missing')}</text>"#,
    )
    .unwrap();
    // Write catalog with only one of those keys.
    fs::write(catalog_dir.join("en.json"), r#"{"nav.volume": "Volume"}"#).unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            i18n: vec![crate::package::module_manifest::I18nContribution {
                id: "en".into(),
                locale: "en".into(),
                path: "config/i18n/en.json".into(),
            }],
            ..MeshContributes::default()
        },
    );
    module.manifest.mesh.i18n.default_locale = Some("en".into());
    module.path = dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();
    let i18n_diags: Vec<_> = graph
        .diagnostics()
        .iter()
        .filter(|d| d.status == "undeclared_i18n_key")
        .collect();
    assert_eq!(
        i18n_diags.len(),
        1,
        "exactly one undeclared key; got: {:?}",
        i18n_diags
    );
    assert!(
        i18n_diags[0].message.contains("nav.missing"),
        "diagnostic should name the missing key"
    );
}

#[test]
fn graph_diagnostics_report_raw_interface_domain_event_publish() {
    let dir = temp_dir("raw-interface-event-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("main.mesh"),
        r#"
<script>
mesh.events.publish("shell.set-theme", { theme_id = "dark" })
mesh.events.publish("mesh.hyprland.switch_workspace", { id = 1 })
</script>
"#,
    )
    .unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    module.path = dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/test-frontend"
            && diagnostic.status == "raw_interface_domain_event_publish"
            && diagnostic
                .message
                .contains("mesh.hyprland.switch_workspace")
    }));
    assert!(!graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.status == "raw_interface_domain_event_publish"
            && diagnostic.message.contains("shell.set-theme")
    }));
}

#[test]
fn graph_diagnostics_report_unknown_shell_event_publish() {
    let dir = temp_dir("unknown-shell-event-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("main.mesh"),
        r#"
<script>
mesh.events.publish("shell.set-theme", { theme_id = "dark" })
mesh.events.publish("shell.not-declared", {})
</script>
"#,
    )
    .unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    module.path = dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/test-frontend"
            && diagnostic.status == "unknown_shell_event_publish"
            && diagnostic.message.contains("shell.not-declared")
    }));
    assert!(!graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.status == "unknown_shell_event_publish"
            && diagnostic.message.contains("shell.set-theme")
    }));
}

#[test]
fn graph_diagnostics_report_keybind_subscription_contract_gaps() {
    let dir = temp_dir("keybind-subscription-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("main.mesh"),
        r#"
<template>
  <button keybind="{this.keybinds.mute.id}" onkeybind={onMute}></button>
  <button keybind="missing" onkeybind={onMissing}></button>
  <button keybind="mute"></button>
</template>
"#,
    )
    .unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    module.path = dir.join("module.json");
    module.manifest.mesh.keybinds.actions.insert(
        "mute".into(),
        crate::manifest::KeybindAction {
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            ..crate::manifest::KeybindAction::default()
        },
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/test-frontend"
            && diagnostic.status == "undeclared_keybind_subscription"
            && diagnostic.message.contains("missing")
    }));
    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/test-frontend"
            && diagnostic.status == "keybind_subscription_missing_handler"
            && diagnostic.message.contains("mute")
    }));
}

#[test]
fn graph_diagnostics_report_backend_undeclared_interface_event_emit() {
    let interface_dir = temp_dir("interface-event-contract-test");
    fs::write(
        interface_dir.join("interface.toml"),
        r#"
[[events]]
name = "DeclaredChanged"
"#,
    )
    .unwrap();
    let backend_dir = temp_dir("backend-event-emit-test");
    let backend_src = backend_dir.join("src");
    fs::create_dir_all(&backend_src).unwrap();
    fs::write(
        backend_src.join("main.luau"),
        r#"
function on_poll()
    mesh.service.emit_event("MissingChanged", { value = 1 })
end
"#,
    )
    .unwrap();

    let root = root_with_modules(
        &[
            ("@mesh/example-interface", ModuleKind::Interface),
            ("@mesh/example-backend", ModuleKind::Backend),
        ],
        &[("mesh.example", "@mesh/example-backend")],
        None,
    );
    let mut interface = loaded_module(
        "@mesh/example-interface",
        ModuleKind::Interface,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    interface.path = interface_dir.join("module.json");
    interface.manifest.mesh.interface = Some(MeshInterfaceDeclaration {
        name: "mesh.example".into(),
        version: Some("1.0".into()),
        file: Some("interface.toml".into()),
        domain: Some("example".into()),
        extends: None,
        relationship: Some(InterfaceRelationship::Base),
        reason: None,
    });
    let mut backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies {
            modules: HashMap::from([(
                "@mesh/example-interface".into(),
                crate::manifest::DependencySpec::Simple(">=1.0.0".into()),
            )]),
            ..MeshDependencies::default()
        },
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/example-interface".into()),
            provider: Some("example".into()),
            label: None,
            priority: 100,
        }],
        MeshContributes::default(),
    );
    backend.path = backend_dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![interface, backend]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/example-backend"
            && diagnostic.status == "undeclared_interface_event_emit"
            && diagnostic.message.contains("MissingChanged")
    }));
}

#[test]
fn graph_diagnostics_no_undeclared_i18n_key_when_all_present() {
    let dir = temp_dir("i18n-key-ok-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let catalog_dir = dir.join("config").join("i18n");
    fs::create_dir_all(&catalog_dir).unwrap();

    fs::write(
        src_dir.join("main.mesh"),
        r#"<text>{t('nav.volume')}</text>"#,
    )
    .unwrap();
    fs::write(catalog_dir.join("en.json"), r#"{"nav.volume": "Volume"}"#).unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            i18n: vec![crate::package::module_manifest::I18nContribution {
                id: "en".into(),
                locale: "en".into(),
                path: "config/i18n/en.json".into(),
            }],
            ..MeshContributes::default()
        },
    );
    module.manifest.mesh.i18n.default_locale = Some("en".into());
    module.path = dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();
    assert!(
        !graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "undeclared_i18n_key"),
        "no undeclared_i18n_key diagnostic when all keys are in catalog"
    );
}

#[test]
fn graph_diagnostics_no_duplicate_keybind_for_unique_triggers() {
    let root = root_with_modules(
        &[
            ("@mesh/mod-a", ModuleKind::Frontend),
            ("@mesh/mod-b", ModuleKind::Frontend),
        ],
        &[],
        None,
    );
    let mut mod_a = loaded_module(
        "@mesh/mod-a",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    mod_a.manifest.mesh.keybinds.actions.insert(
        "toggle".into(),
        crate::manifest::KeybindAction {
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("t".into()),
                modifiers: vec!["ctrl".into()],
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let mut mod_b = loaded_module(
        "@mesh/mod-b",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    mod_b.manifest.mesh.keybinds.actions.insert(
        "open".into(),
        crate::manifest::KeybindAction {
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("o".into()),
                modifiers: vec!["ctrl".into()],
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let graph = InstalledModuleGraph::from_parts(root, vec![mod_a, mod_b]).unwrap();
    assert!(
        !graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "duplicate_keybind_trigger"),
        "different trigger keys must not generate duplicate_keybind_trigger"
    );
}

#[test]
fn graph_diagnostics_report_frontend_surface_contract_gaps() {
    let root = root_with_modules(&[("@mesh/surface", ModuleKind::Frontend)], &[], None);
    let mut frontend = loaded_module(
        "@mesh/surface",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.entrypoints.main = Some("src/main.mesh".into());

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/surface"
            && diagnostic.status == "missing_frontend_surface_layout"
            && diagnostic.message.contains("mesh.surfaceLayout")
    }));
    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/surface"
            && diagnostic.status == "missing_frontend_accessibility"
            && diagnostic.message.contains("mesh.accessibility")
    }));
}

#[test]
fn graph_health_marks_active_provider_unavailable_when_required_binary_is_missing() {
    let root = root_with_modules(
        &[("@mesh/backend", ModuleKind::Backend)],
        &[("mesh.example", "@mesh/backend")],
        None,
    );
    let dependencies = MeshDependencies {
        binaries: vec![crate::manifest::BinaryDependency {
            name: "this-binary-definitely-does-not-exist-on-any-system-graph-health".into(),
            version: None,
            reason: Some("graph health test".into()),
            optional: false,
            packages: Default::default(),
        }],
        ..MeshDependencies::default()
    };
    let mut backend = loaded_module(
        "@mesh/backend",
        ModuleKind::Backend,
        dependencies,
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some("test".into()),
            label: Some(crate::manifest::LocalizedText::Literal("Test".to_string())),
            priority: 100,
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.capabilities.required = vec!["exec.test".into()];

    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();

    assert!(graph.health().iter().any(|record| {
        record.module_id == "@mesh/backend"
            && record.interface.as_deref() == Some("mesh.example")
            && record.provider_id.as_deref() == Some("@mesh/backend")
            && record.status == "provider_unavailable"
    }));
    assert!(graph.health().iter().any(|record| {
        record.interface.as_deref() == Some("mesh.example")
            && record.provider_id.as_deref() == Some("@mesh/backend")
            && record.status == "interface_unavailable"
    }));
}

#[test]
fn graph_health_marks_frontend_required_interface_unavailable_when_active_provider_is_unhealthy() {
    let root = root_with_modules(
        &[
            ("@mesh/frontend", ModuleKind::Frontend),
            ("@mesh/backend", ModuleKind::Backend),
        ],
        &[("mesh.example", "@mesh/backend")],
        None,
    );
    let frontend_dependencies = MeshDependencies {
        backend: HashMap::from([("mesh.example".into(), ">=1.0".into())]),
        ..MeshDependencies::default()
    };
    let frontend = loaded_module(
        "@mesh/frontend",
        ModuleKind::Frontend,
        frontend_dependencies,
        vec![],
        MeshContributes::default(),
    );
    let backend_dependencies = MeshDependencies {
        binaries: vec![crate::manifest::BinaryDependency {
            name: "this-binary-definitely-does-not-exist-on-any-system-frontend-health".into(),
            version: None,
            reason: None,
            optional: false,
            packages: Default::default(),
        }],
        ..MeshDependencies::default()
    };
    let backend = loaded_module(
        "@mesh/backend",
        ModuleKind::Backend,
        backend_dependencies,
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some("test".into()),
            label: None,
            priority: 100,
        }],
        MeshContributes::default(),
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend, backend]).unwrap();

    assert!(graph.health().iter().any(|record| {
        record.module_id == "@mesh/frontend"
            && record.interface.as_deref() == Some("mesh.example")
            && record.provider_id.as_deref() == Some("@mesh/backend")
            && record.status == "required_interface_unavailable"
    }));
}

#[test]
fn graph_diagnostics_flag_backend_provider_restating_consumer_capability() {
    let dir = temp_dir("interface-capability-backend-test");
    fs::write(
        dir.join("interface.toml"),
        r#"
[capabilities]
required = ["service.example.read"]
optional = ["service.example.control"]
"#,
    )
    .unwrap();
    let root = root_with_modules(
        &[
            ("@mesh/example-interface", ModuleKind::Interface),
            ("@mesh/example-backend", ModuleKind::Backend),
        ],
        &[("mesh.example", "@mesh/example-backend")],
        None,
    );
    let mut interface = loaded_module(
        "@mesh/example-interface",
        ModuleKind::Interface,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    interface.path = dir.join("module.json");
    interface.manifest.mesh.interface = Some(MeshInterfaceDeclaration {
        name: "mesh.example".into(),
        version: Some("1.0".into()),
        file: Some("interface.toml".into()),
        domain: Some("example".into()),
        extends: None,
        relationship: Some(InterfaceRelationship::Base),
        reason: None,
    });
    let mut backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/example-interface".into()),
            provider: Some("example".into()),
            label: None,
            priority: 100,
        }],
        MeshContributes::default(),
    );
    // The provider restates the interface's consumer capabilities (read +
    // control) on top of its legitimate host power (exec.example).
    backend.manifest.mesh.capabilities.required =
        vec!["exec.example".into(), "service.example.read".into()];
    backend.manifest.mesh.capabilities.optional = vec!["service.example.control".into()];

    let graph = InstalledModuleGraph::from_parts(root, vec![interface, backend]).unwrap();

    let flagged: Vec<&str> = graph
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.module_id == "@mesh/example-backend"
                && diagnostic.status == "provider_declares_consumer_capability"
        })
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();

    // Both the required and optional consumer capabilities are flagged.
    assert!(
        flagged
            .iter()
            .any(|message| message.contains("service.example.read"))
    );
    assert!(
        flagged
            .iter()
            .any(|message| message.contains("service.example.control"))
    );
    // The generic host power is not flagged — providers legitimately request it.
    assert!(
        !flagged
            .iter()
            .any(|message| message.contains("exec.example"))
    );
}

#[test]
fn graph_diagnostics_report_frontend_missing_interface_required_capability() {
    let dir = temp_dir("interface-capability-frontend-test");
    fs::write(
        dir.join("interface.toml"),
        r#"
[capabilities]
required = ["service.example.read"]
"#,
    )
    .unwrap();
    let root = root_with_modules(
        &[
            ("@mesh/example-interface", ModuleKind::Interface),
            ("@mesh/example-backend", ModuleKind::Backend),
            ("@mesh/example-frontend", ModuleKind::Frontend),
        ],
        &[("mesh.example", "@mesh/example-backend")],
        None,
    );
    let mut interface = loaded_module(
        "@mesh/example-interface",
        ModuleKind::Interface,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    interface.path = dir.join("module.json");
    interface.manifest.mesh.interface = Some(MeshInterfaceDeclaration {
        name: "mesh.example".into(),
        version: Some("1.0".into()),
        file: Some("interface.toml".into()),
        domain: Some("example".into()),
        extends: None,
        relationship: Some(InterfaceRelationship::Base),
        reason: None,
    });
    let mut backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/example-interface".into()),
            provider: Some("example".into()),
            label: None,
            priority: 100,
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.capabilities.required = vec!["service.example.read".into()];
    let frontend = loaded_module(
        "@mesh/example-frontend",
        ModuleKind::Frontend,
        MeshDependencies {
            backend: HashMap::from([("mesh.example".into(), ">=1.0".into())]),
            ..MeshDependencies::default()
        },
        vec![],
        MeshContributes::default(),
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![interface, backend, frontend]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/example-frontend"
            && diagnostic.status == "missing_interface_required_capability"
            && diagnostic.message.contains("service.example.read")
    }));
}

#[test]
fn graph_diagnostics_report_backend_provider_missing_base_module_dependency() {
    let root = root_with_modules(
        &[("@mesh/example-backend", ModuleKind::Backend)],
        &[("mesh.example", "@mesh/example-backend")],
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
            label: None,
            priority: 100,
        }],
        MeshContributes::default(),
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/example-backend"
            && diagnostic.status == "missing_provider_interface_module_dependency"
            && diagnostic.message.contains("@mesh/example-interface")
            && diagnostic.message.contains("mesh.uses.modules")
    }));
}

#[test]
fn graph_diagnostics_accept_backend_provider_declared_base_module_dependency() {
    let root = root_with_modules(
        &[("@mesh/example-backend", ModuleKind::Backend)],
        &[("mesh.example", "@mesh/example-backend")],
        None,
    );
    let backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies {
            modules: HashMap::from([(
                "@mesh/example-interface".into(),
                crate::manifest::DependencySpec::Simple(">=1.0.0".into()),
            )]),
            ..MeshDependencies::default()
        },
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/example-interface".into()),
            provider: Some("example".into()),
            label: None,
            priority: 100,
        }],
        MeshContributes::default(),
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();

    assert!(!graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/example-backend"
            && diagnostic.status == "missing_provider_interface_module_dependency"
    }));
}
