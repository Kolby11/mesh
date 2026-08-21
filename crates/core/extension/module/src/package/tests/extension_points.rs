use super::super::*;
use super::common::*;
use crate::manifest::{ExtensionPointContribution, HostedExtensionPoint};

use std::collections::{BTreeMap, BTreeSet, HashMap};

const POINT: &str = "mesh.settings.page";

fn declaring_interface(multiple: bool) -> LoadedModuleManifest {
    let mut module = loaded_module(
        "@mesh/shell-ui-interface",
        ModuleKind::Interface,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    module.manifest.mesh.extension_points = HashMap::from([(
        POINT.to_string(),
        MeshExtensionPointDeclaration {
            version: "1.0".into(),
            description: None,
            multiple,
            props: vec![
                MeshExtensionPointProp {
                    name: "title".into(),
                    prop_type: "string".into(),
                    description: None,
                },
                MeshExtensionPointProp {
                    name: "order".into(),
                    prop_type: "int".into(),
                    description: None,
                },
            ],
        },
    )]);
    module
}

#[test]
fn duplicate_extension_point_declarations_are_rejected_deterministically() {
    let first = declaring_interface(true);
    let mut second = declaring_interface(true);
    second.manifest.name = "@mesh/other-shell-ui-interface".into();
    let root = root_with_modules(
        &[
            ("@mesh/shell-ui-interface", ModuleKind::Interface),
            ("@mesh/other-shell-ui-interface", ModuleKind::Interface),
        ],
        &[],
        None,
    );

    let error = InstalledModuleGraph::from_parts(root, vec![first, second]).unwrap_err();

    assert_eq!(
        error.to_string(),
        "invalid module manifest: duplicate active extension point declaration 'mesh.settings.page' in modules @mesh/other-shell-ui-interface and @mesh/shell-ui-interface"
    );
}

fn host(module_id: &str, version_req: Option<&str>) -> LoadedModuleManifest {
    let mut module = loaded_module(
        module_id,
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    module.manifest.mesh.hosts = HashMap::from([(
        POINT.to_string(),
        HostedExtensionPoint {
            version: version_req.map(str::to_string),
            layout: Some("column".into()),
            max: None,
            slots: Default::default(),
        },
    )]);
    module
}

fn contributor(module_id: &str, id: &str, order: i64) -> LoadedModuleManifest {
    contributor_with_props(module_id, id, order, serde_json::Map::new())
}

fn contributor_with_props(
    module_id: &str,
    id: &str,
    order: i64,
    props: serde_json::Map<String, serde_json::Value>,
) -> LoadedModuleManifest {
    loaded_module(
        module_id,
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            extension_points: HashMap::from([(
                POINT.to_string(),
                vec![ExtensionPointContribution {
                    id: id.into(),
                    entry: "src/settings.mesh".into(),
                    order: Some(order),
                    props,
                }],
            )]),
            ..MeshContributes::default()
        },
    )
}

fn graph_of(modules: Vec<LoadedModuleManifest>) -> InstalledModuleGraph {
    let descriptors: Vec<(&str, ModuleKind)> = modules
        .iter()
        .map(|module| (module.manifest.name.as_str(), module.manifest.mesh.kind))
        .collect();
    let root = root_with_modules(&descriptors, &[], None);
    InstalledModuleGraph::from_parts(root, modules).unwrap()
}

fn statuses(graph: &InstalledModuleGraph, status: &str) -> Vec<String> {
    graph
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.status == status)
        .map(|diagnostic| diagnostic.message.clone())
        .collect()
}

/// The Stage 1 gate: renaming the host module changes nothing for contributors.
///
/// Under module-keyed slots (`@mesh/settings:custom-settings`) this test could
/// not pass — every contributor named the host, so replacing the settings
/// frontend silently dropped every contributed page.
#[test]
fn contributions_survive_renaming_the_host_module() {
    let resolved_for = |host_id: &str| {
        let graph = graph_of(vec![
            declaring_interface(true),
            host(host_id, Some(">=1.0")),
            contributor("@mesh/navigation-bar", "navigation-bar", 100),
        ]);
        graph
            .extension_point_contributions(host_id, POINT)
            .iter()
            .map(|contribution| contribution.source_module_id.clone())
            .collect::<Vec<_>>()
    };

    assert_eq!(resolved_for("@mesh/settings"), vec!["@mesh/navigation-bar"]);
    assert_eq!(
        resolved_for("@alice/settings"),
        vec!["@mesh/navigation-bar"]
    );
}

/// A contribution resolves into every enabled host of its point. Two settings
/// frontends both receive the pages; that is correct, not a conflict.
#[test]
fn a_contribution_resolves_into_every_enabled_host() {
    let graph = graph_of(vec![
        declaring_interface(true),
        host("@mesh/settings", None),
        host("@alice/settings", None),
        contributor("@mesh/navigation-bar", "navigation-bar", 0),
    ]);

    for host_id in ["@mesh/settings", "@alice/settings"] {
        assert_eq!(graph.extension_point_contributions(host_id, POINT).len(), 1);
    }
}

#[test]
fn contributions_render_in_declared_then_module_order() {
    let graph = graph_of(vec![
        declaring_interface(true),
        host("@mesh/settings", None),
        contributor("@mesh/zzz", "zzz", 10),
        contributor("@mesh/aaa", "aaa", 20),
        contributor("@mesh/mmm", "mmm", 10),
    ]);

    let order: Vec<_> = graph
        .extension_point_contributions("@mesh/settings", POINT)
        .iter()
        .map(|contribution| contribution.source_module_id.as_str())
        .collect();
    assert_eq!(order, vec!["@mesh/mmm", "@mesh/zzz", "@mesh/aaa"]);
}

#[test]
fn hosting_an_undeclared_point_is_a_diagnostic() {
    let graph = graph_of(vec![host("@mesh/settings", None)]);
    assert_eq!(statuses(&graph, "unknown_extension_point").len(), 1);
}

#[test]
fn contributing_to_an_undeclared_point_is_a_diagnostic() {
    let graph = graph_of(vec![contributor("@mesh/navigation-bar", "nav", 0)]);
    let messages = statuses(&graph, "unknown_extension_point");
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("@mesh/navigation-bar"));
}

#[test]
fn a_host_version_range_excluding_the_declared_version_is_a_diagnostic() {
    let graph = graph_of(vec![
        declaring_interface(true),
        host("@mesh/settings", Some(">=2.0")),
        contributor("@mesh/navigation-bar", "nav", 0),
    ]);

    assert_eq!(
        statuses(&graph, "extension_point_version_mismatch").len(),
        1
    );
    assert!(
        graph
            .extension_point_contributions("@mesh/settings", POINT)
            .is_empty()
    );
}

#[test]
fn a_contribution_with_no_enabled_host_reports_rather_than_disappearing() {
    let graph = graph_of(vec![
        declaring_interface(true),
        contributor("@mesh/navigation-bar", "nav", 0),
    ]);
    assert_eq!(statuses(&graph, "unhosted_contribution").len(), 1);
}

#[test]
fn contribution_props_are_typechecked_against_the_declaration() {
    let bad_type = contributor_with_props(
        "@mesh/navigation-bar",
        "nav",
        0,
        serde_json::Map::from_iter([("order".to_string(), serde_json::json!("not-an-int"))]),
    );
    let graph = graph_of(vec![
        declaring_interface(true),
        host("@mesh/settings", None),
        bad_type,
    ]);
    let messages = statuses(&graph, "invalid_extension_point_props");
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("order"));

    let undeclared = contributor_with_props(
        "@mesh/navigation-bar",
        "nav",
        0,
        serde_json::Map::from_iter([("nope".to_string(), serde_json::json!("x"))]),
    );
    let graph = graph_of(vec![
        declaring_interface(true),
        host("@mesh/settings", None),
        undeclared,
    ]);
    assert_eq!(statuses(&graph, "invalid_extension_point_props").len(), 1);
}

#[test]
fn a_single_valued_point_reports_when_more_than_one_module_fills_it() {
    let graph = graph_of(vec![
        declaring_interface(false),
        host("@mesh/settings", None),
        contributor("@mesh/a", "a", 0),
        contributor("@mesh/b", "b", 0),
    ]);
    assert_eq!(statuses(&graph, "extension_point_overfilled").len(), 1);
}

/// The Stage 5 gate: composition override > module page > generated fallback.
///
/// The generated fallback keys off the absence of a resolved contribution, so
/// suppressing a page must make the module read as having none — otherwise a
/// suppressed module would show neither its page nor the generated rows.
#[test]
fn a_composition_replaces_suppresses_and_orders_contributed_pages() {
    let modules = vec![
        declaring_interface(true),
        host("@mesh/settings", None),
        contributor("@mesh/audio", "audio", 10),
        contributor("@mesh/network", "network", 20),
        contributor("@alice/audio-page", "audio", 30),
    ];
    let descriptors: Vec<(&str, ModuleKind)> = modules
        .iter()
        .map(|module| (module.manifest.name.as_str(), module.manifest.mesh.kind))
        .collect();
    let root = root_with_modules(&descriptors, &[], None);

    let mut composition = CompositionContext::default();
    composition.slots.insert(
        POINT.to_string(),
        SlotOverride {
            replace: BTreeMap::from([("@mesh/audio".to_string(), "@alice/audio-page".to_string())]),
            suppress: BTreeSet::from(["@mesh/network".to_string()]),
            order: Vec::new(),
        },
    );
    let graph =
        InstalledModuleGraph::from_parts_with_composition(root, modules, composition).unwrap();

    let sources: Vec<&str> = graph
        .extension_point_contributions("@mesh/settings", POINT)
        .iter()
        .map(|contribution| contribution.source_module_id.as_str())
        .collect();
    // The suppressed module is gone; the replaced one now reads as the family's.
    assert!(!sources.contains(&"@mesh/network"));
    assert!(!sources.contains(&"@mesh/audio"));
    assert_eq!(
        sources
            .iter()
            .filter(|id| **id == "@alice/audio-page")
            .count(),
        2
    );

    // A suppressed module has no resolved page, so the host falls back to the
    // generated rows for it rather than showing nothing.
    assert!(
        graph
            .resolved_contribution_entry("@mesh/network", POINT)
            .is_none()
    );
    assert!(
        graph
            .resolved_contribution_entry("@alice/audio-page", POINT)
            .is_some()
    );
}

#[test]
fn only_interface_modules_may_declare_extension_points() {
    let manifest = ModuleManifest::from_json_str(
        r#"{"name":"@me/panel","version":"1","mesh":{"apiVersion":"0.1","kind":"frontend",
            "entry":"main.mesh","extensionPoints":{"mesh.x.y":{"version":"1.0"}}}}"#,
    );
    assert!(manifest.is_err());
}

#[test]
fn extension_point_names_are_contract_names_not_module_ids() {
    for field in ["extensionPoints", "hosts"] {
        let manifest = ModuleManifest::from_json_str(&format!(
            r#"{{"name":"@me/x","version":"1","mesh":{{"apiVersion":"0.1","kind":"interface",
                "{field}":{{"@mesh/settings:custom-settings":{{"version":"1.0"}}}}}}}}"#
        ));
        assert!(manifest.is_err(), "{field} accepted a module id");
    }
}

#[test]
fn an_interface_module_may_declare_only_extension_points() {
    let manifest = ModuleManifest::from_json_str(
        r#"{"name":"@mesh/shell-ui-interface","version":"1.0.0","mesh":{"apiVersion":"0.1",
            "kind":"interface","extensionPoints":{"mesh.settings.page":{"version":"1.0"}}}}"#,
    )
    .unwrap();
    assert!(manifest.mesh.interface.is_none());
    assert_eq!(manifest.mesh.extension_points.len(), 1);
}
