//! Language support for MESH `module.json` / `package.json` manifests.
//!
//! Two manifest flavors share the same `name`/`version`/`mesh` envelope:
//! per-module manifests ([`ManifestFlavor::Module`]) and the workspace root
//! graph config ([`ManifestFlavor::RootConfig`]). [`schema`] describes both;
//! the generic [`crate::json`] engine serves diagnostics, completion, and
//! hover from that description.

use tower_lsp::lsp_types::{CompletionItem, Diagnostic, DiagnosticSeverity, Hover, Position, Url};

use crate::json;

pub mod schema;

pub use schema::ManifestFlavor;

/// True if `uri` points at a manifest file the LSP should serve as JSON.
pub fn is_manifest_uri(uri: &Url) -> bool {
    matches!(
        uri.path().rsplit('/').next(),
        Some("module.json") | Some("package.json")
    )
}

/// A parsed-on-demand manifest document.
pub struct ManifestDocument {
    pub uri: Url,
    pub source: String,
    pub flavor: ManifestFlavor,
}

impl ManifestDocument {
    pub fn new(uri: Url, source: String) -> Self {
        let flavor = detect_flavor(&source);
        Self {
            uri,
            source,
            flavor,
        }
    }
}

pub fn complete(doc: &ManifestDocument, position: Position) -> Vec<CompletionItem> {
    json::complete::complete(&schema::root(doc.flavor), &doc.source, position)
}

pub fn hover(doc: &ManifestDocument, position: Position) -> Option<Hover> {
    json::hover::hover(&schema::root(doc.flavor), &doc.source, position)
}

/// Schema diagnostics plus, for the root graph config, the canonical runtime
/// validation (schemaVersion, entrypoint format, relative-path rules) that the
/// schema tree cannot express.
pub fn diagnostics(doc: &ManifestDocument) -> Vec<Diagnostic> {
    let source = &doc.source;
    let mut out = json::diagnostics::check(&schema::root(doc.flavor), source, "mesh-manifest");

    // A syntax error is reported alone; do not pile runtime parse failures on it.
    let has_syntax_error = out
        .iter()
        .any(|d| d.message.starts_with("JSON syntax error"));

    if doc.flavor == ManifestFlavor::RootConfig
        && !has_syntax_error
        && let Err(err) = mesh_core_module::package::RootModuleGraphManifest::from_json_str(source)
    {
        // Attach to the `mesh` key when we can find it, else the document start.
        let range = json::diagnostics::find_key_range(source, "mesh")
            .unwrap_or_else(|| json::diagnostics::range_at(source, 0, 1));
        out.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("mesh-manifest".into()),
            message: format!("invalid root config: {err}"),
            ..Default::default()
        });
    }

    out
}

/// The markdown body of a hover, for tests.
#[cfg(test)]
fn hover_text(hover: &Hover) -> String {
    use tower_lsp::lsp_types::{HoverContents, MarkupContent, MarkupKind};
    match &hover.contents {
        HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }) => value.clone(),
        _ => String::new(),
    }
}

/// Decide whether a manifest is a per-module manifest or the root graph config.
///
/// The root config is identified by `mesh.schemaVersion` / `mesh.modulesDir`;
/// a per-module manifest by `mesh.kind` / `mesh.apiVersion`. Anything else
/// defaults to the per-module flavor (the common case).
fn detect_flavor(source: &str) -> ManifestFlavor {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(source) else {
        // Fall back to a cheap textual heuristic when the JSON does not parse
        // (e.g. mid-edit), so completion still targets the right schema.
        if source.contains("\"schemaVersion\"") || source.contains("\"modulesDir\"") {
            return ManifestFlavor::RootConfig;
        }
        return ManifestFlavor::Module;
    };

    let mesh = value.get("mesh");
    let has = |key: &str| mesh.and_then(|m| m.get(key)).is_some();

    if has("schemaVersion") || has("modulesDir") || has("providers") {
        ManifestFlavor::RootConfig
    } else {
        ManifestFlavor::Module
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> ManifestDocument {
        ManifestDocument::new(
            Url::parse("file:///m/module.json").unwrap(),
            src.to_string(),
        )
    }

    fn complete_at(src: &str) -> Vec<String> {
        let offset = src.find('|').expect("need a | cursor marker");
        let clean = src.replacen('|', "", 1);
        let before = &clean[..offset];
        let line = before.matches('\n').count() as u32;
        let col = before.rsplit('\n').next().unwrap().chars().count() as u32;
        complete(&doc(&clean), Position::new(line, col))
            .into_iter()
            .map(|i| i.label)
            .collect()
    }

    #[test]
    fn completes_top_level_keys() {
        let labels = complete_at(r#"{ "|" }"#);
        assert!(labels.contains(&"name".to_string()));
        assert!(labels.contains(&"mesh".to_string()));
    }

    #[test]
    fn completes_kind_enum() {
        let labels = complete_at(r#"{ "mesh": { "kind": "|" } }"#);
        assert!(labels.contains(&"frontend".to_string()));
        assert!(labels.contains(&"backend".to_string()));
    }

    #[test]
    fn suggests_capabilities_without_requiring() {
        let labels = complete_at(r#"{ "mesh": { "uses": { "capabilities": [ "|" ] } } }"#);
        assert!(labels.contains(&"shell.surface".to_string()));
    }

    #[test]
    fn omits_already_present_keys() {
        let labels = complete_at(r#"{ "name": "x", "|" }"#);
        assert!(!labels.contains(&"name".to_string()));
    }

    #[test]
    fn flags_unknown_property() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "wat": 1 } }"#;
        let d = diagnostics(&doc(src));
        assert!(
            d.iter()
                .any(|d| d.message.contains("unknown property `wat`"))
        );
    }

    #[test]
    fn flags_bad_kind() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontnd" } }"#;
        let d = diagnostics(&doc(src));
        assert!(d.iter().any(|d| d.message.contains("not a valid value")));
    }

    #[test]
    fn flags_missing_required() {
        let src = r#"{ "name": "@x/y", "mesh": { "kind": "frontend" } }"#;
        let d = diagnostics(&doc(src));
        assert!(
            d.iter()
                .any(|d| d.message.contains("missing required property `version`"))
        );
        assert!(
            d.iter()
                .any(|d| d.message.contains("missing required property `apiVersion`"))
        );
    }

    #[test]
    fn accepts_valid_manifest() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "entry": "src/main.mesh" } }"#;
        let d = diagnostics(&doc(src));
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn reports_syntax_error() {
        let src = r#"{ "name": "@x/y" "version": "1.0.0" }"#;
        let d = diagnostics(&doc(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("syntax error"));
    }

    #[test]
    fn type_mismatch_for_object_field() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": "nope" }"#;
        let d = diagnostics(&doc(src));
        assert!(d.iter().any(|d| d.message.contains("expected object")));
    }

    #[test]
    fn hovers_a_known_key() {
        let src = r#"{ "mesh": { "kind": "frontend" } }"#;
        let position = {
            let offset = src.find("\"kind\"").unwrap() + 2;
            json::offset_to_position(src, offset)
        };
        let hover = hover(&doc(src), position).expect("hover");
        assert!(hover_text(&hover).contains("module role"));
    }
}
