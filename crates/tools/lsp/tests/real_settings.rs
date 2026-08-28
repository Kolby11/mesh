//! Regression guard for settings language support, run against the real
//! workspace: the shipped `config/settings.json` must validate cleanly, and
//! discovery must actually find the themes, locales, and packs it offers. If
//! this fails, either the settings file drifted or the schema derived from the
//! runtime field tables no longer matches what the store accepts.

use mesh_tools_lsp::module_registry::ModuleRegistry;
use mesh_tools_lsp::settings::{self, SettingsDocument};
use tower_lsp::lsp_types::{Position, Url};

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
        .canonicalize()
        .expect("workspace root")
}

fn registry() -> ModuleRegistry {
    ModuleRegistry::discover(&workspace_root())
}

fn document() -> SettingsDocument {
    let path = workspace_root().join("config/settings.json");
    let source = std::fs::read_to_string(&path).expect("config/settings.json");
    SettingsDocument::new(Url::from_file_path(&path).unwrap(), source)
}

/// Labels offered at the `|` marker in `src`.
fn complete_at(src: &str, registry: &ModuleRegistry) -> Vec<String> {
    let offset = src.find('|').expect("need a | cursor marker");
    let clean = src.replacen('|', "", 1);
    let before = &clean[..offset];
    let line = before.matches('\n').count() as u32;
    let col = before.rsplit('\n').next().unwrap().chars().count() as u32;
    let doc = SettingsDocument::new(Url::parse("file:///w/config/settings.json").unwrap(), clean);
    settings::complete(&doc, Position::new(line, col), registry)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

#[test]
fn shipped_settings_validate_cleanly() {
    let diags = settings::diagnostics(&document(), &registry());
    assert!(
        diags.is_empty(),
        "config/settings.json produced diagnostics:\n{}",
        diags
            .iter()
            .map(|d| format!("  [{:?}] {}", d.severity.unwrap(), d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn registry_exposes_the_canonical_graph_snapshot() {
    let registry = registry();
    let snapshot = registry
        .snapshot
        .as_ref()
        .expect("LSP registry should retain its canonical graph snapshot");
    assert_ne!(snapshot.revision(), 0);
    assert_eq!(snapshot.modules().len(), registry.manifests.len());
}

#[test]
fn discovers_the_shipped_themes() {
    let registry = registry();
    assert!(
        registry.themes.contains(&"gruvbox-dark".to_string()),
        "themes found: {:?}",
        registry.themes
    );
    let labels = complete_at(r#"{ "shell": { "theme": { "active": "|" } } }"#, &registry);
    assert!(labels.contains(&"gruvbox-dark".to_string()), "{labels:?}");
}

#[test]
fn discovers_the_locales_modules_ship_catalogs_for() {
    let registry = registry();
    let labels = complete_at(r#"{ "shell": { "i18n": { "locale": "|" } } }"#, &registry);
    assert!(labels.contains(&"en".to_string()), "{labels:?}");
    assert!(labels.contains(&"sk".to_string()), "{labels:?}");
}

#[test]
fn offers_installed_icon_packs_where_a_pack_belongs() {
    let registry = registry();
    let default_pack = complete_at(
        r#"{ "shell": { "icons": { "default_pack": "|" } } }"#,
        &registry,
    );
    assert!(
        default_pack
            .iter()
            .any(|id| id.contains("icons-material-symbols")),
        "{default_pack:?}"
    );

    let chain = complete_at(
        r#"{ "@mesh/navigation-bar": { "icons": { "use_packs": [ "|" ] } } }"#,
        &registry,
    );
    assert_eq!(chain, default_pack, "the same packs belong in both places");
}

#[test]
fn offers_installed_modules_as_namespaces() {
    let registry = registry();
    let labels = complete_at(r#"{ "|" }"#, &registry);
    assert!(labels.contains(&"shell".to_string()), "{labels:?}");
    assert!(
        labels.contains(&"@mesh/navigation-bar".to_string()),
        "{labels:?}"
    );
}
