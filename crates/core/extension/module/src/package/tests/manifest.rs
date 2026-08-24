use super::super::*;
use super::common::*;
use std::fs;
use std::path::PathBuf;

#[test]
fn binary_available_accepts_explicit_existing_paths() {
    let dir = temp_dir("explicit-binary-path");
    let executable = dir.join("tool");
    fs::write(&executable, "test").unwrap();

    assert!(binary_available(executable.to_str().unwrap()));
    assert!(!binary_available(dir.join("missing").to_str().unwrap()));

    fs::remove_dir_all(dir).unwrap();
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
      "capabilities": ["exec.argv:wpctl:[\"get-volume\"]"],
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
    assert_eq!(
        manifest.mesh.capabilities.required,
        vec!["exec.argv:wpctl:[\"get-volume\"]"]
    );
    assert_eq!(manifest.mesh.dependencies.binaries[0].name, "wpctl");
    assert_eq!(manifest.mesh.i18n.default_locale.as_deref(), Some("en"));
    assert_eq!(manifest.mesh.i18n.supported_locales, vec!["en", "sk"]);
    assert_eq!(
        manifest.mesh.implements[0].base_module.as_deref(),
        Some("@mesh/audio-interface")
    );
}

#[test]
fn language_pack_catalogs_require_one_target_per_locale() {
    let missing_target = r#"
{
  "name": "@community/cs-pack",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "language-pack",
    "provides": {
      "i18n": [{ "id": "cs", "locale": "cs", "path": "cs.json" }]
    }
  }
}
"#;
    let error = ModuleManifest::from_json_str(missing_target)
        .expect_err("a language pack without a target is ambiguous")
        .to_string();
    assert!(error.contains("must declare its target module"), "{error}");

    let duplicate_locale = r#"
{
  "name": "@community/cs-pack",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "language-pack",
    "provides": {
      "i18n": [
        { "id": "first", "locale": "cs", "module": "@mesh/panel", "path": "first.json" },
        { "id": "second", "locale": "CS", "module": "@mesh/panel", "path": "second.json" }
      ]
    }
  }
}
"#;
    let error = ModuleManifest::from_json_str(duplicate_locale)
        .expect_err("one pack must not provide ambiguous duplicate target locales")
        .to_string();
    assert!(error.contains("duplicate target/locale pair"), "{error}");
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
fn module_manifest_rejects_legacy_surface_layout_with_migration_diagnostic() {
    let content = r#"
{
  "name": "@mesh/panel",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "surfaceLayout": { "anchor": "top" }
  }
}
"#;

    let err = ModuleManifest::from_json_str(content).unwrap_err();
    let ModuleManifestError::Diagnostic { diagnostic } = err else {
        panic!("legacy surface layout should emit a migration diagnostic");
    };
    assert_eq!(diagnostic.severity, ModuleManifestDiagnosticSeverity::Error);
    assert_eq!(diagnostic.module_id.as_deref(), Some("@mesh/panel"));
    assert_eq!(diagnostic.field_path.as_deref(), Some("mesh.surfaceLayout"));
    assert!(diagnostic.message.contains("legacy surface declaration"));
    assert_eq!(
        diagnostic.suggested_action,
        "replace mesh.surfaceLayout with mesh.surface"
    );
}

#[test]
fn module_manifest_rejects_duplicate_contribution_identities() {
    let content = r#"
{
  "name": "@mesh/duplicate-contributions",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "provides": {
      "extensionPoints": {
        "mesh.settings.page": [{"id":"shared","entry":"src/one.mesh"}],
        "mesh.other.page": [{"id":"shared","entry":"src/two.mesh"}]
      }
    }
  }
}
"#;

    let error = ModuleManifest::from_json_str(content).unwrap_err();

    assert_eq!(
        error.to_string(),
        "invalid module manifest: mesh.provides.extensionPoints contributions contains duplicate contribution identity 'shared'"
    );
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
    assert!(interface.contract.is_none());
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
