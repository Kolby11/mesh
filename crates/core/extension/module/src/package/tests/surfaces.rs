use super::super::*;
use super::common::*;

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
            && diagnostic.message.contains("mesh.surface")
    }));
    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/surface"
            && diagnostic.status == "missing_frontend_accessibility"
            && diagnostic.message.contains("mesh.accessibility")
    }));
}

#[test]
fn graph_diagnostics_reject_layer_placement_on_a_window_surface() {
    // A window is placed by the compositor, so an anchor/layer/margin on one is
    // a contradiction in the manifest, not a field to quietly drop.
    let root = root_with_modules(&[("@mesh/window", ModuleKind::Frontend)], &[], None);
    let mut frontend = loaded_module(
        "@mesh/window",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.entrypoints.main = Some("src/main.mesh".into());
    declare_frontend_surface_contract(&mut frontend);
    frontend.manifest.mesh.surface_layout = Some(crate::manifest::SurfaceLayoutSection {
        role: Some("window".into()),
        anchor: Some("right".into()),
        exclusive_zone: Some(48),
        ..Default::default()
    });

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/window"
            && diagnostic.status == "surface_role_field_mismatch"
            && diagnostic.message.contains("anchor")
            && diagnostic.message.contains("exclusiveZone")
    }));
}

#[test]
fn graph_diagnostics_reject_window_fields_on_a_layer_surface() {
    let root = root_with_modules(&[("@mesh/panel", ModuleKind::Frontend)], &[], None);
    let mut frontend = loaded_module(
        "@mesh/panel",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.entrypoints.main = Some("src/main.mesh".into());
    declare_frontend_surface_contract(&mut frontend);
    frontend.manifest.mesh.surface_layout = Some(crate::manifest::SurfaceLayoutSection {
        anchor: Some("top".into()),
        app_id: Some("mesh.panel".into()),
        resizable: Some(false),
        ..Default::default()
    });

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/panel"
            && diagnostic.status == "surface_role_field_mismatch"
            && diagnostic.message.contains("appId")
            && diagnostic.message.contains("resizable")
    }));
}

#[test]
fn graph_diagnostics_accept_a_well_formed_window_surface() {
    let root = root_with_modules(&[("@mesh/window", ModuleKind::Frontend)], &[], None);
    let mut frontend = loaded_module(
        "@mesh/window",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.entrypoints.main = Some("src/main.mesh".into());
    declare_frontend_surface_contract(&mut frontend);
    frontend.manifest.mesh.surface_layout = Some(crate::manifest::SurfaceLayoutSection {
        role: Some("window".into()),
        title: Some(crate::manifest::LocalizedText::Literal("Settings".into())),
        resizable: Some(false),
        ..Default::default()
    });

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    assert!(
        !graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.status == "surface_role_field_mismatch"),
        "window-only fields on a window must not be flagged"
    );
}

#[test]
fn graph_diagnostics_accept_both_roles_fields_on_a_promotable_surface() {
    // A promotable surface is realized as chrome *and* as a window at different
    // points in its life, so declaring both sets is the only way to describe it.
    // This is the one exemption from the role-mismatch rule, and it is explicit:
    // without `promotable`, the same manifest is still a contradiction.
    let root = root_with_modules(&[("@mesh/settings", ModuleKind::Frontend)], &[], None);
    let mut frontend = loaded_module(
        "@mesh/settings",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.entrypoints.main = Some("src/main.mesh".into());
    declare_frontend_surface_contract(&mut frontend);
    let both_roles = crate::manifest::SurfaceLayoutSection {
        role: Some("layer".into()),
        promotable: Some(true),
        anchor: Some("right".into()),
        layer: Some("overlay".into()),
        keyboard_mode: Some("on_demand".into()),
        title: Some(crate::manifest::LocalizedText::Literal("Settings".into())),
        app_id: Some("mesh.settings".into()),
        ..Default::default()
    };
    frontend.manifest.mesh.surface_layout = Some(both_roles.clone());

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend.clone()]).unwrap();

    assert!(
        !graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.status == "surface_role_field_mismatch"),
        "a promotable surface may describe both of the roles it holds"
    );

    // The exemption is the flag, not the field combination.
    let root = root_with_modules(&[("@mesh/settings", ModuleKind::Frontend)], &[], None);
    frontend.manifest.mesh.surface_layout = Some(crate::manifest::SurfaceLayoutSection {
        promotable: None,
        ..both_roles
    });
    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.status == "surface_role_field_mismatch"),
        "the same manifest without `promotable` is still a contradiction"
    );
}
