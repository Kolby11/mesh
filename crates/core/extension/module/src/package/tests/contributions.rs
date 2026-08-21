use super::super::*;
use super::common::*;
use std::collections::HashMap;

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
    frontend.manifest.mesh.contributes.extension_points.insert(
        "mesh.settings.page".into(),
        vec![crate::manifest::ExtensionPointContribution {
            id: "example-widget".into(),
            entry: "src/settings.mesh".into(),
            order: None,
            props: serde_json::Map::new(),
        }],
    );
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

    // A settings page is a contribution to an extension point, not a second
    // frontend entrypoint.
    assert_eq!(graph.frontend_entrypoints().len(), 1);
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
    assert_eq!(
        graph.settings_schemas()[0].settings_page.as_deref(),
        Some("src/settings.mesh")
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
        capability_approvals: Default::default(),
        trust_policy: Default::default(),
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
