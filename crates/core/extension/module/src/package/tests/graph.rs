use super::super::*;
use super::common::*;
use crate::ModuleType;
use std::collections::HashMap;

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

#[test]
fn installed_module_graph_blocks_a_frontend_with_a_missing_required_module() {
    let mut dependencies = MeshDependencies::default();
    dependencies.modules.insert(
        "@mesh/missing".into(),
        crate::manifest::DependencySpec::Simple(">=1.0.0".into()),
    );
    let frontend = loaded_module(
        "@mesh/frontend",
        ModuleKind::Frontend,
        dependencies,
        vec![],
        MeshContributes::default(),
    );
    let root = root_with_modules(&[("@mesh/frontend", ModuleKind::Frontend)], &[], None);

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    assert!(!graph.module("@mesh/frontend").unwrap().enabled);
    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/frontend"
            && diagnostic.status == "missing_required_module_dependency"
    }));
}

#[test]
fn installed_module_graph_keeps_an_optional_module_dependency_degraded() {
    let mut dependencies = MeshDependencies::default();
    dependencies.modules.insert(
        "@mesh/optional".into(),
        crate::manifest::DependencySpec::Detailed {
            version: ">=1.0.0".into(),
            optional: Some(true),
        },
    );
    let frontend = loaded_module(
        "@mesh/frontend",
        ModuleKind::Frontend,
        dependencies,
        vec![],
        MeshContributes::default(),
    );
    let root = root_with_modules(&[("@mesh/frontend", ModuleKind::Frontend)], &[], None);

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    assert!(graph.module("@mesh/frontend").unwrap().enabled);
    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/frontend"
            && diagnostic.status == "optional_module_dependency_missing"
    }));
}

#[test]
fn installed_module_graph_blocks_required_consumers_from_incompatible_provider_versions() {
    let mut frontend_dependencies = MeshDependencies::default();
    frontend_dependencies
        .backend
        .insert("mesh.example".into(), ">=2.0.0".into());
    let frontend = loaded_module(
        "@mesh/frontend",
        ModuleKind::Frontend,
        frontend_dependencies,
        vec![],
        MeshContributes::default(),
    );
    let interface = interface_module(
        "@mesh/example-interface",
        "mesh.example",
        "example",
        InterfaceRelationship::Base,
        None,
    );
    let backend = loaded_module(
        "@mesh/backend",
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
    let root = root_with_modules(
        &[
            ("@mesh/frontend", ModuleKind::Frontend),
            ("@mesh/example-interface", ModuleKind::Interface),
            ("@mesh/backend", ModuleKind::Backend),
        ],
        &[("mesh.example", "@mesh/backend")],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend, interface, backend]).unwrap();

    assert!(!graph.module("@mesh/frontend").unwrap().enabled);
    assert!(graph.active_provider("mesh.example").is_none());
    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/frontend"
            && diagnostic.status == "interface_dependency_blocked"
    }));
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
        MeshDependencies::default(),
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
        diagnostic.status != "missing_interface_contract"
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
