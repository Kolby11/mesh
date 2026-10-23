use super::super::*;
use super::common::*;
use std::collections::HashMap;
use std::fs;

#[test]
fn library_module_with_required_capabilities_is_rejected() {
    let content = r#"
{
  "name": "@mesh/example-lib",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "library",
    "capabilities": { "required": ["exec.argv:run:*"] }
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
fn inline_backend_interface_declaration_registers_contract() {
    let root = root_with_modules(
        &[("@mesh/hyprland-wm", ModuleKind::Backend)],
        &[("mesh.wm", "@mesh/hyprland-wm")],
        None,
    );
    let mut backend = loaded_module(
        "@mesh/hyprland-wm",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.wm".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some("hyprland".into()),
            label: None,
            priority: 100,
            features: Vec::new(),
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.interfaces =
        vec![crate::package::module_manifest::MeshInterfaceDeclaration {
            name: "mesh.wm".into(),
            version: Some("1.0".into()),
            contract: Some(serde_json::json!({
                "state": [{ "name": "available", "type": "boolean" }],
                "methods": [{
                    "name": "focus_workspace",
                    "args": [{ "name": "id", "type": "int" }],
                    "returns": "Result"
                }],
                "capabilities": { "required": ["service.wm.read"] }
            })),
            domain: Some("wm".into()),
            extends: None,
            relationship: Some(crate::package::module_manifest::InterfaceRelationship::Base),
            reason: None,
        }];

    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();

    let contract = graph
        .interface_contract("mesh.wm")
        .expect("inline backend declaration should register a typed contract");
    assert_eq!(contract.methods[0].name, "focus_workspace");
    assert_eq!(
        contract.capabilities.required,
        vec!["service.wm.read".to_string()]
    );
    assert!(
        graph
            .declared_interface("mesh.wm")
            .is_some_and(|declaration| declaration.module_id == "@mesh/hyprland-wm")
    );
}

#[test]
fn duplicate_interface_declaration_rejects_inline_copy() {
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
    interface.manifest.mesh.interface =
        Some(crate::package::module_manifest::MeshInterfaceDeclaration {
            name: "mesh.example".into(),
            version: Some("1.0".into()),
            contract: Some(serde_json::json!({
                "methods": [{ "name": "from_interface_module" }]
            })),
            domain: Some("example".into()),
            extends: None,
            relationship: Some(crate::package::module_manifest::InterfaceRelationship::Base),
            reason: None,
        });
    let mut backend = loaded_module(
        "@mesh/example-backend",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some("example".into()),
            label: None,
            priority: 100,
            features: Vec::new(),
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.interfaces =
        vec![crate::package::module_manifest::MeshInterfaceDeclaration {
            name: "mesh.example".into(),
            version: Some("1.0".into()),
            contract: Some(serde_json::json!({
                "methods": [{ "name": "from_inline_backend" }]
            })),
            domain: Some("example".into()),
            extends: None,
            relationship: Some(crate::package::module_manifest::InterfaceRelationship::Base),
            reason: None,
        }];

    let error = InstalledModuleGraph::from_parts(root, vec![interface, backend]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("duplicate active interface declaration 'mesh.example'")
    );
}

fn inline_example_backend(module_id: &str, priority: u32, method: &str) -> LoadedModuleManifest {
    let mut backend = loaded_module(
        module_id,
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "mesh.example".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some(module_id.into()),
            label: None,
            priority,
            features: Vec::new(),
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.interfaces =
        vec![crate::package::module_manifest::MeshInterfaceDeclaration {
            name: "mesh.example".into(),
            version: Some("1.0".into()),
            contract: Some(serde_json::json!({ "methods": [{ "name": method }] })),
            domain: Some("example".into()),
            extends: None,
            relationship: Some(crate::package::module_manifest::InterfaceRelationship::Base),
            reason: None,
        }];
    backend
}

#[test]
fn provider_priority_cannot_replace_an_inline_interface_contract() {
    for method in ["focus", "rewritten"] {
        let root = root_with_modules(
            &[
                ("@mesh/example-backend", ModuleKind::Backend),
                ("@mesh/other-backend", ModuleKind::Backend),
            ],
            &[("mesh.example", "@mesh/example-backend")],
            None,
        );
        let error = InstalledModuleGraph::from_parts(
            root,
            vec![
                inline_example_backend("@mesh/example-backend", 100, "focus"),
                inline_example_backend("@mesh/other-backend", 101, method),
            ],
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate active interface declaration 'mesh.example'")
        );
    }
}

#[test]
fn duplicate_standalone_interface_declarations_are_rejected_deterministically() {
    let first = interface_module(
        "@mesh/z-interface",
        "mesh.example",
        "example",
        InterfaceRelationship::Base,
        None,
    );
    let second = interface_module(
        "@mesh/a-interface",
        "mesh.example",
        "example",
        InterfaceRelationship::Base,
        None,
    );
    let root = root_with_modules(
        &[
            ("@mesh/z-interface", ModuleKind::Interface),
            ("@mesh/a-interface", ModuleKind::Interface),
        ],
        &[],
        None,
    );

    let error = InstalledModuleGraph::from_parts(root, vec![first, second]).unwrap_err();

    assert_eq!(
        error.to_string(),
        "invalid module manifest: duplicate active interface declaration 'mesh.example' in modules @mesh/a-interface and @mesh/z-interface"
    );
}

#[test]
fn invalid_interface_contract_becomes_graph_diagnostic() {
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
            // References a named type that is never declared.
            contract: Some(serde_json::json!({
                "methods": [{ "name": "sensors", "returns": "Sensor[]" }]
            })),
            domain: Some("test".into()),
            extends: None,
            relationship: Some(crate::package::module_manifest::InterfaceRelationship::Base),
            reason: None,
        });

    let graph = InstalledModuleGraph::from_parts(root, vec![iface]).unwrap();

    assert!(graph.interface_contract("mesh.test").is_none());
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "invalid_interface_contract" && d.message.contains("Sensor")),
        "invalid contract should surface as a diagnostic; got: {:?}",
        graph.diagnostics()
    );
}

#[test]
fn graph_diagnostics_report_missing_interface_contract() {
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
            contract: None,
            domain: Some("test".into()),
            extends: None,
            relationship: Some(crate::package::module_manifest::InterfaceRelationship::Base),
            reason: None,
        });
    let graph = InstalledModuleGraph::from_parts(root, vec![iface]).unwrap();
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "missing_interface_contract"),
        "expected missing_interface_contract diagnostic; got: {:?}",
        graph.diagnostics()
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
<template><box /></template>
<script lang="luau">
mesh.events.publish("shell.set-theme", { theme_id = "dark" })
mesh.events.publish("mesh.hyprland.switch_workspace", { id = 1 })
</script>
"#,
    )
    .unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Ui)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Ui,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    module.path = dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();

    let authoring = graph.authoring_diagnostics();
    assert!(authoring.iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/test-frontend"
            && diagnostic.status == "raw_interface_domain_event_publish"
            && diagnostic
                .message
                .contains("mesh.hyprland.switch_workspace")
    }));
    assert!(!authoring.iter().any(|diagnostic| {
        diagnostic.status == "raw_interface_domain_event_publish"
            && diagnostic.message.contains("shell.set-theme")
    }));
}

#[test]
fn graph_diagnostics_report_backend_undeclared_interface_event_emit() {
    let interface_dir = temp_dir("interface-event-contract-test");
    let backend_dir = temp_dir("backend-event-emit-test");
    let backend_src = backend_dir.join("src");
    fs::create_dir_all(&backend_src).unwrap();
    fs::write(
        backend_src.join("main.luau"),
        r#"
function on_poll()
    self.MissingChanged:fire({ value = 1 })
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
    interface.manifest.version = "1.0.0".into();
    interface.manifest.mesh.interface = Some(MeshInterfaceDeclaration {
        name: "mesh.example".into(),
        version: Some("1.0".into()),
        contract: Some(serde_json::json!({
            "events": [{ "name": "DeclaredChanged" }]
        })),
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
            features: Vec::new(),
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
fn graph_diagnostics_report_frontend_undeclared_interface_event_subscription() {
    let interface_dir = temp_dir("frontend-interface-event-contract-test");
    let frontend_dir = temp_dir("frontend-event-subscription-test");
    let frontend_src = frontend_dir.join("src");
    fs::create_dir_all(&frontend_src).unwrap();
    fs::write(
        frontend_src.join("main.mesh"),
        r#"
<template><box /></template>
<script lang="luau">
local example = require("mesh.example")
example.MissingChanged:on(function(_event) end)
</script>
"#,
    )
    .unwrap();

    let root = root_with_modules(
        &[
            ("@mesh/example-interface", ModuleKind::Interface),
            ("@mesh/example-frontend", ModuleKind::Ui),
        ],
        &[],
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
        contract: Some(serde_json::json!({
            "events": [{ "name": "DeclaredChanged" }]
        })),
        domain: Some("example".into()),
        extends: None,
        relationship: Some(InterfaceRelationship::Base),
        reason: None,
    });
    let mut frontend = loaded_module(
        "@mesh/example-frontend",
        ModuleKind::Ui,
        MeshDependencies {
            backend: HashMap::from([("mesh.example".into(), ">=1.0".into())]),
            ..MeshDependencies::default()
        },
        vec![],
        MeshContributes::default(),
    );
    frontend.path = frontend_dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![interface, frontend]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/example-frontend"
            && diagnostic.status == "undeclared_interface_event_subscription"
            && diagnostic.message.contains("MissingChanged")
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
            features: Vec::new(),
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.capabilities.required = vec!["exec.argv:test:*".into()];

    let graph = InstalledModuleGraph::from_parts(root, typed(vec![backend])).unwrap();

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
            ("@mesh/frontend", ModuleKind::Ui),
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
        ModuleKind::Ui,
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
            features: Vec::new(),
        }],
        MeshContributes::default(),
    );

    let graph = InstalledModuleGraph::from_parts(root, typed(vec![frontend, backend])).unwrap();

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
        contract: Some(serde_json::json!({
            "capabilities": {
                "required": ["service.example.read"],
                "optional": ["service.example.control"]
            }
        })),
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
            features: Vec::new(),
        }],
        MeshContributes::default(),
    );
    // The provider restates the interface's consumer capabilities (read +
    // control) on top of its legitimate host power (an executable argv rule).
    backend.manifest.mesh.capabilities.required =
        vec!["exec.argv:example:*".into(), "service.example.read".into()];
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
            .any(|message| message.contains("exec.argv:example"))
    );
}

#[test]
fn graph_diagnostics_report_frontend_missing_interface_required_capability() {
    let dir = temp_dir("interface-capability-frontend-test");
    let root = root_with_modules(
        &[
            ("@mesh/example-interface", ModuleKind::Interface),
            ("@mesh/example-backend", ModuleKind::Backend),
            ("@mesh/example-frontend", ModuleKind::Ui),
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
        contract: Some(serde_json::json!({
            "capabilities": { "required": ["service.example.read"] }
        })),
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
            features: Vec::new(),
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.capabilities.required = vec!["service.example.read".into()];
    let frontend = loaded_module(
        "@mesh/example-frontend",
        ModuleKind::Ui,
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
            features: Vec::new(),
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
            features: Vec::new(),
        }],
        MeshContributes::default(),
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();

    assert!(!graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/example-backend"
            && diagnostic.status == "missing_provider_interface_module_dependency"
    }));
}

#[test]
fn interface_contracts_declare_owned_service_permissions() {
    let root = root_with_modules(
        &[("@acme/weather", ModuleKind::Backend)],
        &[("acme.weather", "@acme/weather")],
        None,
    );
    let mut backend = with_inline_contracts(loaded_module(
        "@acme/weather",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "acme.weather".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some("openmeteo".into()),
            label: None,
            priority: 100,
            features: Vec::new(),
        }],
        MeshContributes::default(),
    ));
    backend.manifest.mesh.interfaces[0].domain = Some("weather".into());
    backend.manifest.mesh.interfaces[0].contract = Some(serde_json::json!({
        "state": [{ "name": "temperature", "type": "float" }],
        "capabilities": {
            "read": ["service.acme.weather.read"],
            "permissions": {
                "service.acme.weather.read": "standard",
                "service.acme.weather.refresh": "elevated",
                "service.audio.control": "standard",
                "service.acme.weather.everything": "root"
            }
        }
    }));

    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();
    let catalog = graph.capability_catalog();
    assert_eq!(
        catalog.validate("service.acme.weather.read"),
        Ok(mesh_core_capability::PrivilegeLevel::Standard)
    );
    assert_eq!(
        catalog.validate("service.acme.weather.refresh"),
        Ok(mesh_core_capability::PrivilegeLevel::Elevated)
    );
    assert!(catalog.validate("service.acme.weather.everything").is_err());
    let refused = graph
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.status == "invalid_service_permission")
        .map(|diagnostic| (diagnostic.module_id.as_str(), diagnostic.message.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(refused.len(), 2, "{refused:#?}");
    assert!(
        refused
            .iter()
            .all(|(module_id, _)| *module_id == "@acme/weather")
    );
    assert!(
        refused
            .iter()
            .any(|(_, message)| message.contains("service.audio.control"))
    );
    assert!(
        refused
            .iter()
            .any(|(_, message)| message.contains("'root'"))
    );
}

/// The one required-provider rule (spec 01 §5, spec 02 §3): a consumer whose
/// required interface has no installed provider still loads and sees the
/// interface `unavailable`; a consumer whose required interface has providers
/// that cannot serve it is blocked; unrelated modules are never affected.
#[test]
fn a_missing_required_provider_degrades_while_an_unusable_one_blocks() {
    let root = root_with_modules(
        &[
            ("@me/waits", ModuleKind::Ui),
            ("@me/blocked", ModuleKind::Ui),
            ("@me/unrelated", ModuleKind::Ui),
            ("@me/old-provider", ModuleKind::Backend),
            ("@me/missing-interface", ModuleKind::Interface),
        ],
        &[],
        None,
    );
    let requiring = |id: &str, interface: &str, range: &str| {
        loaded_module(
            id,
            ModuleKind::Ui,
            MeshDependencies {
                backend: HashMap::from([(interface.into(), range.into())]),
                ..MeshDependencies::default()
            },
            vec![],
            MeshContributes::default(),
        )
    };
    let unrelated = loaded_module(
        "@me/unrelated",
        ModuleKind::Ui,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    // Declared (and typed) but nothing implements it.
    let mut declared_only = interface_module(
        "@me/missing-interface",
        "me.missing",
        "missing",
        InterfaceRelationship::Base,
        None,
    );
    declared_only
        .manifest
        .mesh
        .interface
        .as_mut()
        .unwrap()
        .version = Some("1.0".into());
    // Implemented, but only at a version the consumer rejects.
    let old_provider = with_inline_contracts(loaded_module(
        "@me/old-provider",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "me.versioned".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some("old".into()),
            label: None,
            priority: 100,
            features: Vec::new(),
        }],
        MeshContributes::default(),
    ));

    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![
            requiring("@me/waits", "me.missing", ">=1.0"),
            requiring("@me/blocked", "me.versioned", ">=2.0"),
            unrelated,
            old_provider,
            declared_only,
        ],
    )
    .unwrap();

    assert!(graph.module("@me/waits").unwrap().enabled);
    assert!(graph.health().iter().any(|record| {
        record.interface.as_deref() == Some("me.missing") && record.status.contains("unavailable")
    }));
    assert!(!graph.module("@me/blocked").unwrap().enabled);
    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@me/blocked" && diagnostic.status == "interface_dependency_blocked"
    }));
    assert!(graph.module("@me/unrelated").unwrap().enabled);
    assert!(graph.module("@me/old-provider").unwrap().enabled);
}

#[test]
fn standard_namespace_checks_provenance_including_spoofed_mesh_ids() {
    for (id, trust) in [
        ("@alice/wm-interface", TrustTier::Verified),
        ("@mesh/wm-interface", TrustTier::Local),
        ("@mesh/wm-interface", TrustTier::Community),
    ] {
        let module = interface_module(id, "mesh.wm", "wm", InterfaceRelationship::Base, None);
        let root = root_with_modules(&[(id, ModuleKind::Interface)], &[], None);
        let error = InstalledModuleGraph::from_parts_with_trust(
            root,
            vec![module],
            Default::default(),
            std::collections::BTreeMap::from([(id.into(), trust)]),
        )
        .unwrap_err();
        assert!(error.to_string().contains("core provenance"), "{error}");
    }
}

#[test]
fn community_interfaces_use_their_publishers_namespace() {
    let module = interface_module(
        "@alice/weather-interface",
        "alice.weather",
        "weather",
        InterfaceRelationship::Base,
        None,
    );
    let root = root_with_modules(
        &[("@alice/weather-interface", ModuleKind::Interface)],
        &[],
        None,
    );
    let graph = InstalledModuleGraph::from_parts(root.clone(), vec![module.clone()]).unwrap();
    assert!(graph.interface_contract("alice.weather").is_some());
    let mut invalid = module;
    invalid.manifest.mesh.interface.as_mut().unwrap().name = "bob.weather".into();
    assert!(
        InstalledModuleGraph::from_parts(root, vec![invalid])
            .unwrap_err()
            .to_string()
            .contains("publisher namespace 'alice.'")
    );
}

#[test]
fn standard_extension_point_names_also_require_core_provenance() {
    let mut module = interface_module(
        "@alice/weather-interface",
        "alice.weather",
        "weather",
        InterfaceRelationship::Base,
        None,
    );
    module.manifest.mesh.extension_points.insert(
        "mesh.weather.item".into(),
        serde_json::from_value(serde_json::json!({"version":"1.0"})).unwrap(),
    );
    let root = root_with_modules(
        &[("@alice/weather-interface", ModuleKind::Interface)],
        &[],
        None,
    );
    assert!(
        InstalledModuleGraph::from_parts(root, vec![module])
            .unwrap_err()
            .to_string()
            .contains("core provenance")
    );
}

#[test]
fn standard_domain_contracts_load_without_any_provider() {
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let domains = ["wm", "power", "brightness"];
    let modules = domains
        .iter()
        .map(|domain| {
            load_module_manifest(&repository.join("modules/interfaces").join(domain)).unwrap()
        })
        .collect::<Vec<_>>();
    let ids = domains
        .iter()
        .map(|domain| format!("@mesh/{domain}-interface"))
        .collect::<Vec<_>>();
    let entries = ids
        .iter()
        .map(|id| (id.as_str(), ModuleKind::Interface))
        .collect::<Vec<_>>();
    let graph =
        InstalledModuleGraph::from_parts(root_with_modules(&entries, &[], None), modules).unwrap();
    for domain in domains {
        assert!(
            graph
                .interface_contract(&format!("mesh.{domain}"))
                .is_some(),
            "{:#?}",
            graph.diagnostics()
        );
        let provider = match domain {
            "wm" => "hyprland-wm",
            "power" => "upower-power",
            _ => "backlight-brightness",
        };
        let backend =
            load_module_manifest(&repository.join("modules/backend").join(provider)).unwrap();
        assert!(backend.manifest.mesh.interfaces.is_empty());
        assert!(
            backend
                .manifest
                .mesh
                .dependencies
                .modules
                .contains_key(&format!("@mesh/{domain}-interface"))
        );
    }
}

#[test]
fn wm_contract_separates_workspace_identity_position_and_output_focus() {
    let repository = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let module = load_module_manifest(&repository.join("modules/interfaces/wm")).unwrap();
    let graph = InstalledModuleGraph::from_parts(
        root_with_modules(&[("@mesh/wm-interface", ModuleKind::Interface)], &[], None),
        vec![module],
    )
    .unwrap();
    let contract = graph
        .interface_contract("mesh.wm")
        .unwrap()
        .compile(mesh_core_service::DeclarationProvenance::unknown())
        .unwrap();
    for workspaces in [
        serde_json::json!([
            {"id":42,"name":"code","windows":2,"index":1,"output":"DP-1","visible":true,"focused":true},
            {"id":99,"name":"chat","windows":1,"index":1,"output":"DP-2","visible":true,"focused":false}
        ]),
        serde_json::json!([
            {"id":42,"name":"mail","windows":1,"index":null,"output":"eDP-1","visible":true,"focused":true,"urgent":true},
            {"id":-1337,"name":"notes","windows":0,"output":null}
        ]),
    ] {
        let state = serde_json::json!({"available":true,"active_workspace":42,"active_window":{"title":"Editor","app_id":"org.editor"},"workspaces":workspaces,"source_module":"@test/provider"});
        assert!(
            contract.state_warnings(&state).is_empty(),
            "{:?}",
            contract.state_warnings(&state)
        );
    }
}

#[test]
fn providers_advertising_undeclared_features_get_a_diagnostic() {
    let root = root_with_modules(
        &[("@alice/thermal", ModuleKind::Backend)],
        &[("alice.thermal", "@alice/thermal")],
        None,
    );
    let mut backend = loaded_module(
        "@alice/thermal",
        ModuleKind::Backend,
        MeshDependencies::default(),
        vec![MeshProvidesDeclaration {
            interface: "alice.thermal".into(),
            version: Some("1.0".into()),
            base_module: None,
            provider: Some("thermal".into()),
            label: None,
            priority: 100,
            features: vec!["fan-control".into(), "turbo".into()],
        }],
        MeshContributes::default(),
    );
    backend.manifest.mesh.interfaces =
        vec![crate::package::module_manifest::MeshInterfaceDeclaration {
            name: "alice.thermal".into(),
            version: Some("1.0".into()),
            contract: Some(serde_json::json!({
                "state": { "fans": { "type": "int?" } },
                "methods": { "set_fan": {} },
                "features": { "fan-control": { "members": ["fans", "set_fan"] } }
            })),
            domain: Some("thermal".into()),
            extends: None,
            relationship: Some(crate::package::module_manifest::InterfaceRelationship::Base),
            reason: None,
        }];

    let graph = InstalledModuleGraph::from_parts(root, vec![backend]).unwrap();

    let provider = &graph.backend_provider_contributions()[0];
    assert_eq!(provider.features, vec!["fan-control".to_string(), "turbo".to_string()]);
    let unknown = graph
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.status == "unknown_provider_feature")
        .collect::<Vec<_>>();
    assert_eq!(unknown.len(), 1, "{unknown:?}");
    assert!(unknown[0].message.contains("'turbo'"));
}
