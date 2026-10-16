use super::super::*;
use super::common::*;
use std::fs;

#[test]
fn doctor_checks_component_reads_with_module_and_css_local_tokens() {
    let dir = temp_dir("semantic-token-component");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("module.json"),
        r##"{
      "name":"@alice/token-widget","version":"1.0.0",
      "mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh",
        "uses":{"capabilities":["theme.read"]},
        "theme":{"tokens":{"panel.color":"#112233"}}}
    }"##,
    )
    .unwrap();
    fs::write(dir.join("src/main.mesh"), r#"<props>{ accent = { type = "token", default = "color.prop-tp" }, panel = { type = "token", default = "panel.color" } }</props>
<template><box/></template>
<style>.box { --local: 4px; width: var(--local); color: var(--color-tp); background: var(--panel-color); }</style>
<script lang="luau">local value = mesh.theme.token("color.script-tp")</script>"#).unwrap();
    let module = load_module_manifest(&dir).unwrap();
    let graph = InstalledModuleGraph::from_parts(
        root_with_modules(&[("@alice/token-widget", ModuleKind::Ui)], &[], None),
        vec![module],
    )
    .unwrap();
    let diagnostics = graph.authoring_diagnostics();
    let unknown = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.status == "unknown_semantic_token")
        .collect::<Vec<_>>();
    assert_eq!(unknown.len(), 3, "{diagnostics:#?}");
    assert!(
        unknown
            .iter()
            .any(|diagnostic| diagnostic.message.contains("--color-prop-tp"))
    );
    assert!(
        unknown
            .iter()
            .any(|diagnostic| diagnostic.message.contains("--color-tp"))
    );
    assert!(
        unknown
            .iter()
            .any(|diagnostic| diagnostic.message.contains("color.script-tp"))
    );
    assert!(graph.module("@alice/token-widget").unwrap().enabled);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn doctor_reports_missing_unknown_and_incomplete_theme_contract_versions() {
    for (version, expected) in [
        (None, "missing_token_contract_version"),
        (Some("99.0"), "unsupported_token_contract_version"),
        (Some("1.0"), "incomplete_token_contract"),
    ] {
        let dir = temp_dir("semantic-token-theme");
        let mut declaration =
            serde_json::json!({"id":"custom","modes":{"dark":"theme.css"},"default_mode":"dark"});
        if let Some(version) = version {
            declaration["tokenContract"] = version.into();
        }
        fs::write(
            dir.join("module.json"),
            serde_json::to_string(&serde_json::json!({
                "name":"@alice/token-theme","version":"1.0.0",
                "mesh":{"apiVersion":"0.1","kind":"theme","contributes":{"themes":[declaration]}}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(dir.join("theme.css"), ":root { --color-primary: #123456; --private-palette: #345678; } node { color: var(--color-on-surfac); background: var(--private-palette); }").unwrap();
        let module = load_module_manifest(&dir).unwrap();
        let graph = InstalledModuleGraph::from_parts(
            root_with_modules(&[("@alice/token-theme", ModuleKind::Theme)], &[], None),
            vec![module],
        )
        .unwrap();
        let diagnostics = graph.authoring_diagnostics();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.status == expected),
            "{diagnostics:#?}"
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.status == "unknown_semantic_token")
                .count(),
            1
        );
        assert!(graph.module("@alice/token-theme").unwrap().enabled);
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn shipped_themes_implement_the_published_semantic_contract() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    for entry in fs::read_dir(root.join("modules/themes")).unwrap() {
        let dir = entry.unwrap().path();
        let module = load_module_manifest(&dir).unwrap();
        for theme in &module.manifest.mesh.contributes.themes {
            assert_eq!(
                theme.token_contract.as_deref(),
                Some(mesh_core_theme::SEMANTIC_TOKEN_CONTRACT_VERSION)
            );
            for path in theme.modes.values() {
                let source = fs::read_to_string(dir.join(path)).unwrap();
                let missing = mesh_core_theme::missing_semantic_tokens(&source).unwrap();
                assert!(missing.is_empty(), "{} {path}: {missing:?}", theme.id);
            }
        }
    }
}

#[test]
fn doctor_checks_reusable_component_token_reads() {
    let dir = temp_dir("semantic-token-reusable");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("module.json"), r#"{"name":"@alice/reusable","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh"}}"#).unwrap();
    fs::write(
        dir.join("src/main.mesh"),
        "<template><box/></template><style>.box { color: var(--color-typo); }</style>",
    )
    .unwrap();
    let module = load_module_manifest(&dir).unwrap();
    let graph = InstalledModuleGraph::from_parts(
        root_with_modules(&[("@alice/reusable", ModuleKind::Ui)], &[], None),
        vec![module],
    )
    .unwrap();
    assert!(
        graph
            .authoring_diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.status == "unknown_semantic_token"
                && diagnostic.message.contains("--color-typo"))
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shipped_ui_and_theme_reads_use_the_semantic_contract_or_local_tokens() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let graph = load_authoring_snapshot(&root.join("config/module.json")).unwrap();
    let diagnostics = graph
        .authoring_diagnostics()
        .into_iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.status.as_str(),
                "unknown_semantic_token"
                    | "missing_token_contract_version"
                    | "unsupported_token_contract_version"
                    | "incomplete_token_contract"
            )
        })
        .collect::<Vec<_>>();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}
