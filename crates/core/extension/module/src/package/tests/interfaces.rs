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
fn duplicate_interface_declaration_prefers_interface_module() {
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

    let graph = InstalledModuleGraph::from_parts(root, vec![interface, backend]).unwrap();

    let contract = graph.interface_contract("mesh.example").unwrap();
    assert_eq!(
        contract.methods[0].name, "from_interface_module",
        "standalone interface module contract must win over inline duplicates"
    );
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "duplicate_interface_declaration"
                && d.module_id == "@mesh/example-backend"),
        "losing inline declaration should be a diagnostic; got: {:?}",
        graph.diagnostics()
    );
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
            ("@mesh/example-frontend", ModuleKind::Frontend),
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
        ModuleKind::Frontend,
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
