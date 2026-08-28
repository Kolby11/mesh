//! Language support for the settings store (`config/settings.json`,
//! `$MESH_HOME/settings.json`).
//!
//! The file is described in `docs/spec/08-settings.md`: one sparse JSON
//! document whose top-level keys are `schemaVersion`, `shell`, and one
//! namespace per module or interface. Its schema is not restated here — it is
//! derived from the runtime's own field tables ([`schema`]), so a key the store
//! stops accepting stops being completed.
//!
//! What the runtime tables cannot supply is *which values exist on this
//! machine*: the themes in `modules/themes`, the locales modules ship catalogs
//! for, the installed icon packs. Those come from the module registry and are
//! offered as suggestions rather than enforced, because discovery from the
//! workspace can never see everything the shell can.

use tower_lsp::lsp_types::{CompletionItem, Diagnostic, Hover, Position, Url};

use crate::json;
use crate::module_registry::ModuleRegistry;

pub mod schema;

/// True if `uri` points at a MESH settings file.
///
/// A bare `settings.json` is far too common to claim — editors, linters, and
/// other tools all use the name. It is ours when it sits in a `config/`
/// directory (the repo checkout layout), under `mesh/` (the `$MESH_HOME`
/// layout), or when `MESH_SETTINGS_PATH` points straight at it.
pub fn is_settings_uri(uri: &Url) -> bool {
    let Some(path) = uri.to_file_path().ok() else {
        return false;
    };
    if path.file_name().and_then(|name| name.to_str()) != Some("settings.json") {
        return false;
    }
    if path == mesh_core_config::default_settings_path() {
        return true;
    }
    matches!(
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str()),
        Some("config") | Some("mesh")
    )
}

/// An open settings document.
pub struct SettingsDocument {
    pub uri: Url,
    pub source: String,
}

impl SettingsDocument {
    pub fn new(uri: Url, source: String) -> Self {
        Self { uri, source }
    }
}

pub fn complete(
    doc: &SettingsDocument,
    position: Position,
    registry: &ModuleRegistry,
) -> Vec<CompletionItem> {
    json::complete::complete(&schema::root(registry), &doc.source, position)
}

pub fn hover(
    doc: &SettingsDocument,
    position: Position,
    registry: &ModuleRegistry,
) -> Option<Hover> {
    json::hover::hover(&schema::root(registry), &doc.source, position)
}

pub fn diagnostics(doc: &SettingsDocument, registry: &ModuleRegistry) -> Vec<Diagnostic> {
    json::diagnostics::check(&schema::root(registry), &doc.source, "mesh-settings")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ModuleRegistry {
        let mut registry = ModuleRegistry::empty();
        registry.themes = vec!["gruvbox-dark".to_string(), "nord".to_string()];
        registry.locales = vec!["en".to_string(), "sk".to_string()];
        registry
            .interface_fields
            .insert("mesh.audio".into(), vec![]);
        registry
    }

    fn doc(src: &str) -> SettingsDocument {
        SettingsDocument::new(
            Url::parse("file:///w/config/settings.json").unwrap(),
            src.to_string(),
        )
    }

    /// Complete at the `|` marker, returning the labels offered.
    fn complete_at(src: &str) -> Vec<String> {
        let offset = src.find('|').expect("need a | cursor marker");
        let clean = src.replacen('|', "", 1);
        complete(
            &doc(&clean),
            crate::util::offset_to_position(&clean, offset),
            &registry(),
        )
        .into_iter()
        .map(|item| item.label)
        .collect()
    }

    fn diagnose(src: &str) -> Vec<Diagnostic> {
        diagnostics(&doc(src), &registry())
    }

    #[test]
    fn completes_top_level_namespaces() {
        let labels = complete_at(r#"{ "|" }"#);
        assert!(labels.contains(&"shell".to_string()));
        assert!(labels.contains(&"schemaVersion".to_string()));
        assert!(labels.contains(&"mesh.audio".to_string()));
    }

    #[test]
    fn completes_shell_keys_from_the_runtime_table() {
        let labels = complete_at(r#"{ "shell": { "|" } }"#);
        for key in ["theme", "i18n", "icons", "keyboard", "tooltip", "render"] {
            assert!(
                labels.contains(&key.to_string()),
                "missing {key} in {labels:?}"
            );
        }
    }

    #[test]
    fn completes_enum_values_from_the_runtime_table() {
        let labels = complete_at(r#"{ "shell": { "tooltip": { "position": "|" } } }"#);
        assert!(labels.contains(&"cursor".to_string()));
        assert!(labels.contains(&"auto".to_string()));
    }

    #[test]
    fn completes_discovered_themes() {
        let labels = complete_at(r#"{ "shell": { "theme": { "active": "|" } } }"#);
        assert_eq!(labels, vec!["gruvbox-dark".to_string(), "nord".to_string()]);
    }

    #[test]
    fn completes_discovered_locales() {
        let labels = complete_at(r#"{ "shell": { "i18n": { "locale": "|" } } }"#);
        assert_eq!(labels, vec!["en".to_string(), "sk".to_string()]);
    }

    #[test]
    fn completes_surface_placement_inside_a_module_namespace() {
        let labels = complete_at(r#"{ "@who/knows": { "surface": { "anchor": "|" } } }"#);
        assert!(labels.contains(&"bottom".to_string()));
    }

    #[test]
    fn flags_an_unknown_shell_key() {
        let d = diagnose(r#"{ "shell": { "tooltip": { "dely_ms": 200 } } }"#);
        assert!(
            d.iter()
                .any(|d| d.message.contains("unknown property `dely_ms`")),
            "{d:?}"
        );
    }

    #[test]
    fn flags_a_bad_enum_value() {
        let d = diagnose(r#"{ "@mesh/navigation-bar": { "surface": { "anchor": "diagonal" } } }"#);
        assert!(
            d.iter().any(|d| d.message.contains("not a valid value")),
            "{d:?}"
        );
    }

    #[test]
    fn accepts_a_namespace_for_a_module_that_is_not_installed() {
        // Namespaces of uninstalled modules are kept, not deleted (spec 08 §7),
        // so they must not be reported as mistakes.
        let d =
            diagnose(r#"{ "@who/knows": { "props": { "global": { "density": "compact" } } } }"#);
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn does_not_enforce_discovered_values() {
        // A theme can live outside the scanned workspace.
        let d = diagnose(r#"{ "shell": { "theme": { "active": "something-else" } } }"#);
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn hover_documents_a_key_the_cursor_sits_inside() {
        let src = r#"{ "shell": { "render": { "blur": { "passes": 2 } } } }"#;
        let offset = src.find("passes").unwrap() + 3;
        let hover = hover(
            &doc(src),
            json::offset_to_position(src, offset),
            &registry(),
        )
        .expect("hover");
        let tower_lsp::lsp_types::HoverContents::Markup(markup) = hover.contents else {
            panic!("expected markup");
        };
        assert!(markup.value.contains("Blur passes"), "{}", markup.value);
    }

    #[test]
    fn claims_a_config_settings_file() {
        assert!(is_settings_uri(
            &Url::parse("file:///home/u/projects/mesh/config/settings.json").unwrap()
        ));
    }

    #[test]
    fn ignores_an_unrelated_settings_file() {
        assert!(!is_settings_uri(
            &Url::parse("file:///home/u/projects/mesh/.vscode/settings.json").unwrap()
        ));
        assert!(!is_settings_uri(
            &Url::parse("file:///home/u/projects/mesh/config/module.json").unwrap()
        ));
    }
}
