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
                let missing = mesh_core_theme::missing_semantic_tokens(
                    &source,
                    mesh_core_theme::SEMANTIC_TOKEN_CONTRACT_VERSION,
                )
                .unwrap();
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
                    | "token_default_outside_contract"
                    | "theme_styles_component_internals"
                    | "theme_sets_undeclared_module_token"
            ) || (diagnostic.status == "hardcoded_color_in_component_style"
                && diagnostic.module_id.starts_with("@mesh/glass-"))
        })
        .collect::<Vec<_>>();
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

fn write_module(dir: &std::path::Path, manifest: serde_json::Value, files: &[(&str, &str)]) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("module.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();
    for (path, content) in files {
        let path = dir.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

fn family_fixture(theme_css: &str) -> (std::path::PathBuf, InstalledModuleGraph) {
    let root = temp_dir("semantic-token-family");
    write_module(
        &root.join("material"),
        serde_json::json!({
            "name":"@alice/material","version":"1.0.0",
            "mesh":{"apiVersion":"0.1","kind":"library",
              "theme":{"tokens":{
                "glass.color.rim":"var(--color-outline-variant)",
                "glass.color.rim-hover":"var(--glass-color-rim)",
                "glass.color.edge":"var(--glass-color-missing)"
              }}}
        }),
        &[],
    );
    write_module(
        &root.join("bar"),
        serde_json::json!({
            "name":"@alice/bar","version":"1.0.0",
            "mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh",
              "uses":{"modules":{"@alice/material":">=1.0.0"}},
              "theme":{"tokens":{"bar.color.pill":"var(--glass-color-rim)"}}}
        }),
        &[(
            "src/main.mesh",
            "<template><box/></template><style>.box { border-color: var(--glass-color-rim-hover); background: var(--bar-color-pill); outline-color: var(--color-focus-ring); }</style>",
        )],
    );
    write_module(
        &root.join("logo"),
        serde_json::json!({
            "name":"@alice/logo","version":"1.0.0",
            "mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh",
              "theme":{"independent":true}}
        }),
        &[("src/main.mesh", "<template><box/></template>")],
    );
    write_module(
        &root.join("theme"),
        serde_json::json!({
            "name":"@alice/theme","version":"1.0.0",
            "mesh":{"apiVersion":"0.1","kind":"theme","contributes":{"themes":[
              {"id":"alice","modes":{"dark":"theme.css"},"default_mode":"dark","tokenContract":"1.0"}
            ]}}
        }),
        &[("theme.css", theme_css)],
    );
    let modules = ["material", "bar", "logo", "theme"]
        .iter()
        .map(|name| load_module_manifest(&root.join(name)).unwrap())
        .collect::<Vec<_>>();
    let graph = InstalledModuleGraph::from_parts(
        root_with_modules(
            &[
                ("@alice/material", ModuleKind::Library),
                ("@alice/bar", ModuleKind::Ui),
                ("@alice/logo", ModuleKind::Ui),
                ("@alice/theme", ModuleKind::Theme),
            ],
            &[],
            None,
        ),
        modules,
    )
    .unwrap();
    (root, graph)
}

#[test]
fn token_families_are_visible_to_users_and_defaults_must_reach_the_contract() {
    let (root, graph) = family_fixture(":root { --color-primary: #123456; }");
    assert_eq!(
        graph.theme_token_families("@alice/bar"),
        ["@alice/material"]
    );
    assert!(
        graph
            .visible_theme_tokens("@alice/bar")
            .contains(&"glass.color.rim.hover".to_string())
    );
    let diagnostics = graph.authoring_diagnostics();
    let unknown = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.status == "unknown_semantic_token")
        .collect::<Vec<_>>();
    assert!(unknown.is_empty(), "{unknown:#?}");
    let outside = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.status == "token_default_outside_contract")
        .collect::<Vec<_>>();
    assert_eq!(outside.len(), 1, "{diagnostics:#?}");
    assert_eq!(outside[0].module_id, "@alice/material");
    assert!(outside[0].message.contains("--glass-color-missing"));
    assert!(
        !diagnostics.iter().any(
            |diagnostic| diagnostic.status == "incomplete_token_contract"
                && diagnostic.message.contains("focus-ring")
        ),
        "roles with platform defaults are never required"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn theme_module_blocks_report_internals_unknown_hooks_and_coverage() {
    let (root, graph) = family_fixture(
        r#":root { --color-primary: #123456; }
        @module "@alice/material" { :root { --glass-color-rim: #ff00ff40; } }
        @module "@alice/bar" {
          :root { --bar-color-pil: #000; }
          button { border-radius: 4px; }
          .pill { opacity: 0.9; }
        }
        @module "@alice/absent" { :root { --absent-color-x: #000; } }"#,
    );
    let diagnostics = graph.authoring_diagnostics();
    let statuses = |status: &str| {
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.status == status)
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
    };
    let internals = statuses("theme_styles_component_internals");
    assert_eq!(internals.len(), 1, "{diagnostics:#?}");
    assert!(internals[0].contains(".pill"));
    let undeclared = statuses("theme_sets_undeclared_module_token");
    assert_eq!(undeclared.len(), 1, "{diagnostics:#?}");
    assert!(undeclared[0].contains("bar.color.pil"));

    let coverage = graph.theme_coverage("@alice/theme:alice").unwrap();
    assert_eq!(coverage.token_contract.as_deref(), Some("1.0"));
    assert_eq!(
        coverage
            .tuned
            .iter()
            .map(|entry| (
                entry.module.as_str(),
                entry.tokens_set,
                entry.tokens_declared
            ))
            .collect::<Vec<_>>(),
        [("@alice/bar", 0, 1), ("@alice/material", 1, 3)]
    );
    assert_eq!(coverage.tuned[0].elements, ["button"]);
    assert!(coverage.defaults.is_empty());
    assert_eq!(coverage.independent, ["@alice/logo"]);
    assert_eq!(coverage.absent_targets, ["@alice/absent"]);
    assert_eq!(coverage.internal_selectors.len(), 1);
    assert_eq!(coverage.internal_selectors[0].selector, ".pill");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn module_tokens_cannot_claim_a_semantic_contract_group() {
    let dir = temp_dir("semantic-token-group");
    write_module(
        &dir,
        serde_json::json!({
            "name":"@alice/shadow","version":"1.0.0",
            "mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh",
              "theme":{"tokens":{"color.rim":"#fff"}}}
        }),
        &[("src/main.mesh", "<template><box/></template>")],
    );
    let error = load_module_manifest(&dir).unwrap_err().to_string();
    assert!(error.contains("semantic contract group 'color'"), "{error}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn doctor_warns_for_fixed_colours_in_reusable_components_unless_independent() {
    for independent in [false, true] {
        let dir = temp_dir("component-colour-coherence");
        write_module(
            &dir,
            serde_json::json!({
                "name":"@alice/colour-widget","version":"1.0.0",
                "mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh",
                    "theme":{"independent":independent}}
            }),
            &[
                (
                    "src/main.mesh",
                    "<template><box/></template><style>.box { color: var(--color-on-surface); }</style>",
                ),
                (
                    "src/components/card.mesh",
                    "<template><box/></template><style>.box { background: linear-gradient(red, #123456); border-color: blue; /* mesh-allow-literal */ }</style>",
                ),
            ],
        );
        let graph = InstalledModuleGraph::from_parts(
            root_with_modules(&[("@alice/colour-widget", ModuleKind::Ui)], &[], None),
            vec![load_module_manifest(&dir).unwrap()],
        )
        .unwrap();
        let warnings = graph
            .authoring_diagnostics()
            .into_iter()
            .filter(|diagnostic| diagnostic.status == "hardcoded_color_in_component_style")
            .collect::<Vec<_>>();
        assert_eq!(
            warnings.len(),
            if independent { 0 } else { 2 },
            "{warnings:#?}"
        );
        assert!(
            warnings
                .iter()
                .all(|warning| warning.message.contains("components/card.mesh")
                    && warning.message.contains("at bytes"))
        );
        assert!(graph.module("@alice/colour-widget").unwrap().enabled);
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn shipped_glass_material_resolves_and_paints_under_every_theme_mode() {
    use mesh_core_elements::style::{BackgroundPaint, Color, StyleContext, StyleResolver};
    use mesh_core_theme::{Theme, ThemeModule, ThemeModuleLayer, ThemeSourceHandle};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let material = load_module_manifest(&root.join("modules/libraries/glass-material")).unwrap();
    let tokens = material
        .manifest
        .mesh
        .theme
        .as_ref()
        .unwrap()
        .tokens
        .clone();
    let rules = vec![mesh_core_component::StyleRule {
        selector: mesh_core_component::Selector::Universal,
        declarations: mesh_core_component::parse_inline_style(
            "background-image: var(--glass-surface-pill); color: var(--glass-text); box-shadow: var(--glass-shadow-pill);"
        ).unwrap(),
        container_query: None,
    }];
    for entry in fs::read_dir(root.join("modules/themes")).unwrap() {
        let dir = entry.unwrap().path();
        let module = load_module_manifest(&dir).unwrap();
        for declaration in &module.manifest.mesh.contributes.themes {
            for (mode, path) in &declaration.modes {
                let pack = mesh_core_theme::load_theme_from_source(
                    &ThemeSourceHandle::new(&dir, path).unwrap(),
                    &declaration.id,
                    &declaration.id,
                )
                .unwrap();
                let theme = Theme::compose_layers(
                    &mesh_core_theme::default_theme(),
                    &pack,
                    &declaration.id,
                    mode,
                    [
                        ThemeModuleLayer {
                            module_id: "@mesh/glass-material".into(),
                            module: ThemeModule {
                                tokens: tokens.clone(),
                                ..Default::default()
                            },
                            families: vec![],
                        },
                        ThemeModuleLayer {
                            module_id: "@mesh/glass-bar".into(),
                            module: ThemeModule::default(),
                            families: vec!["@mesh/glass-material".into()],
                        },
                    ],
                    &std::collections::HashMap::new(),
                )
                .unwrap();
                for name in tokens.keys() {
                    let resolved = theme
                        .resolve_token_value(&format!("@mesh/glass-bar.{name}"))
                        .unwrap()
                        .unwrap()
                        .to_string();
                    assert!(
                        !resolved.contains("var("),
                        "{} {mode} {name}: {resolved}",
                        declaration.id
                    );
                    if name.starts_with("glass.text")
                        || name.starts_with("glass.fill")
                        || name.starts_with("glass.rim")
                        || name.starts_with("glass.indicator")
                        || name == "glass.hover"
                    {
                        assert!(
                            Color::from_css(&resolved).is_some(),
                            "{} {mode} {name}: {resolved}",
                            declaration.id
                        );
                    }
                }
                let style = StyleResolver::new(&theme).resolve_node_style_for_module(
                    &rules,
                    "box",
                    &[],
                    None,
                    StyleContext::default(),
                    mesh_core_elements::tree::NodeState::default(),
                    Some("@mesh/glass-bar"),
                );
                let BackgroundPaint::LinearGradient(gradient) = style.background_paint else {
                    panic!("{} {mode}: missing material gradient", declaration.id);
                };
                assert!(gradient.from.a > 0 && gradient.from.a < 255);
                assert!(gradient.to.a > 0 && gradient.to.a < 255);
                assert_eq!(style.box_shadow.blur_radius, 14.0);
                assert!(style.box_shadow.color.a > 0 && style.box_shadow.color.a < 255);
                if declaration.id == "violet-glass" {
                    assert_eq!(gradient.from, Color::from_hex("#d4b8ff36").unwrap());
                    assert_eq!(gradient.to, Color::from_hex("#44206e24").unwrap());
                    assert_eq!(style.color, Color::from_hex("#f5ecff").unwrap());
                    assert_eq!(
                        style.box_shadow.color,
                        Color::from_hex("#10002045").unwrap()
                    );
                } else {
                    let surface = theme
                        .resolve_token_value("--color-surface-container-high")
                        .unwrap()
                        .unwrap()
                        .to_string();
                    let foreground = theme
                        .resolve_token_value("--color-on-surface")
                        .unwrap()
                        .unwrap()
                        .to_string();
                    let expected = Color::from_css(&surface).unwrap();
                    assert_eq!(
                        (gradient.from.r, gradient.from.g, gradient.from.b),
                        (expected.r, expected.g, expected.b)
                    );
                    assert_eq!(style.color, Color::from_css(&foreground).unwrap());
                }
            }
        }
    }
}

#[test]
fn doctor_reports_removed_parts_but_retains_uninstalled_targets() {
    let dir = temp_dir("public-style-parts");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("module.json"), r#"{"name":"@alice/parts","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh"}}"#).unwrap();
    fs::write(
        dir.join("src/main.mesh"),
        "<template><box part=\"frame\"/><icon class=\"private\" part=\"glyph\"/></template>",
    )
    .unwrap();
    fs::create_dir_all(dir.join("extras")).unwrap();
    fs::write(
        dir.join("extras/alternate.mesh"),
        "<template><text part=\"label\"/></template>",
    )
    .unwrap();
    let graph = InstalledModuleGraph::from_parts(
        root_with_modules(&[("@alice/parts", ModuleKind::Ui)], &[], None),
        vec![load_module_manifest(&dir).unwrap()],
    )
    .unwrap();
    assert_eq!(
        graph
            .style_parts("@alice/parts")
            .into_iter()
            .collect::<Vec<_>>(),
        ["frame", "glyph", "label"]
    );
    assert!(
        graph
            .user_stylesheet_diagnostics("@alice/parts::part(glyph) { width: 20px; }")
            .is_empty()
    );
    let diagnostics = graph.user_stylesheet_diagnostics(
        "@alice/parts::part(private) { width: 20px; } @alice/missing::part(glyph) { width: 20px; }",
    );
    assert_eq!(diagnostics.len(), 2);
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("no longer exposes style part 'private'"))
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("no installed module; rule retained"))
    );
    fs::write(
        dir.join("src/main.mesh"),
        "<template><box part=\"frame\"/></template>",
    )
    .unwrap();
    assert!(
        graph.user_stylesheet_diagnostics("@alice/parts::part(glyph) { width: 20px; }")[0]
            .contains("no longer exposes")
    );
    fs::remove_dir_all(dir).unwrap();
}

/// The core's `:focus-visible` outline is the one focus ring
/// ([04 §12](../../../../../../docs/spec/04-styling.md)); a shipped module that
/// also recolours a border on focus draws two.
#[test]
fn shipped_modules_leave_the_focus_ring_to_the_platform_outline() {
    fn mesh_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                mesh_files(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "mesh") {
                files.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let mut files = Vec::new();
    mesh_files(&root.join("modules/frontend"), &mut files);
    let mut duplicates = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).unwrap();
        let mut rest = source.as_str();
        while let Some(open) = rest.find('{') {
            let selector = rest[..open].rsplit(['}', '\n']).next().unwrap_or_default();
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            let body = &rest[open + 1..open + close];
            if selector.contains(":focus-visible")
                && !body.contains('{')
                && body
                    .split(';')
                    .map(str::trim_start)
                    .any(|declaration| {
                        declaration.starts_with("border")
                            && !declaration.starts_with("border-radius")
                    })
            {
                duplicates.push(format!("{}: {}", path.display(), selector.trim()));
            }
            rest = &rest[open + 1..];
        }
    }
    assert!(duplicates.is_empty(), "{duplicates:#?}");
}
