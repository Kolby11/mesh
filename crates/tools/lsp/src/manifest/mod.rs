//! Language support for canonical MESH `module.json` manifests.
//!
//! Two manifest flavors share the same `name`/`version`/`mesh` envelope:
//! per-module manifests ([`ManifestFlavor::Module`]) and the workspace root
//! graph config ([`ManifestFlavor::RootConfig`]). [`schema`] describes both;
//! the generic [`crate::json`] engine serves diagnostics, completion, and
//! hover from that description.

use tower_lsp::lsp_types::{
    CompletionItem, Diagnostic, DiagnosticSeverity, Hover, NumberOrString, Position, Range, Url,
};

use crate::json;

pub mod schema;

pub use schema::ManifestFlavor;

/// True if `uri` points at a manifest file the LSP should serve as JSON.
pub fn is_manifest_uri(uri: &Url) -> bool {
    matches!(uri.path().rsplit('/').next(), Some("module.json"))
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

/// Editor-schema diagnostics plus canonical runtime validation for both
/// manifest flavors. The schema tree supplies completion, hover, and useful
/// structural feedback; the runtime contract remains the authority for
/// semantic validation and serde shape errors.
pub fn diagnostics(doc: &ManifestDocument) -> Vec<Diagnostic> {
    let source = &doc.source;
    let mut out = json::diagnostics::check(&schema::root(doc.flavor), source, "mesh-manifest");

    // A syntax error is reported alone; do not pile runtime parse failures on it.
    let has_syntax_error = out
        .iter()
        .any(|d| d.message.starts_with("JSON syntax error"));

    if !has_syntax_error && let Err(error) = canonical_validation(doc.flavor, source) {
        out.push(canonical_diagnostic(source, doc.flavor, error));
    }

    out
}

fn canonical_validation(
    flavor: ManifestFlavor,
    source: &str,
) -> Result<(), mesh_core_module::package::ModuleManifestError> {
    match flavor {
        ManifestFlavor::Module => {
            mesh_core_module::package::ModuleManifest::from_json_str(source).map(|_| ())
        }
        ManifestFlavor::RootConfig => {
            mesh_core_module::package::RootModuleGraphManifest::from_json_str(source).map(|_| ())
        }
    }
}

fn canonical_diagnostic(
    source: &str,
    flavor: ManifestFlavor,
    error: mesh_core_module::package::ModuleManifestError,
) -> Diagnostic {
    let (severity, range, message) = match error {
        mesh_core_module::package::ModuleManifestError::Json {
            source: parse_error,
            ..
        } => {
            let offset = json::line_col_to_offset(source, parse_error.line(), parse_error.column());
            (
                DiagnosticSeverity::ERROR,
                json::diagnostics::range_at(source, offset, offset.saturating_add(1)),
                format!("runtime manifest JSON error: {parse_error}"),
            )
        }
        mesh_core_module::package::ModuleManifestError::Diagnostic { diagnostic } => {
            let range = diagnostic
                .field_path
                .as_deref()
                .and_then(|path| field_range(source, path))
                .or_else(|| json::diagnostics::find_key_range(source, "mesh"))
                .unwrap_or_else(|| json::diagnostics::range_at(source, 0, 1));
            let message = if diagnostic.suggested_action.trim().is_empty() {
                diagnostic.message
            } else {
                format!(
                    "{}; suggested action: {}",
                    diagnostic.message, diagnostic.suggested_action
                )
            };
            let severity = match diagnostic.severity {
                mesh_core_module::package::ModuleManifestDiagnosticSeverity::Warning => {
                    DiagnosticSeverity::WARNING
                }
                mesh_core_module::package::ModuleManifestDiagnosticSeverity::Error => {
                    DiagnosticSeverity::ERROR
                }
            };
            (severity, range, format!("runtime manifest: {message}"))
        }
        mesh_core_module::package::ModuleManifestError::Validation(message) => {
            let range = validation_range(source, &message);
            (
                DiagnosticSeverity::ERROR,
                range,
                format!("runtime manifest validation: {message}"),
            )
        }
        error => (
            DiagnosticSeverity::ERROR,
            validation_range(source, ""),
            format!("runtime manifest validation: {error}"),
        ),
    };

    let flavor_code = match flavor {
        ManifestFlavor::Module => "module",
        ManifestFlavor::RootConfig => "root",
    };
    Diagnostic {
        range,
        severity: Some(severity),
        source: Some("mesh-runtime".into()),
        code: Some(NumberOrString::String(format!(
            "mesh.manifest.runtime.{flavor_code}"
        ))),
        message,
        ..Default::default()
    }
}

fn field_range(source: &str, field_path: &str) -> Option<Range> {
    let key = field_path
        .rsplit(|character| character == '.' || character == '[')
        .next()?
        .trim_end_matches(']');
    (!key.is_empty()).then(|| json::diagnostics::find_key_range(source, key))?
}

fn validation_range(source: &str, message: &str) -> Range {
    if let Some(path) = message
        .find("mesh.")
        .map(|start| &message[start..])
        .and_then(|path| {
            let end = path.find(|character: char| {
                character.is_whitespace() || matches!(character, '\'' | '"' | '`' | ':' | ',' | ')')
            });
            Some(&path[..end.unwrap_or(path.len())])
        })
        && let Some(range) = field_range(source, path)
    {
        return range;
    }

    for key in [
        "schemaVersion",
        "modulesDir",
        "capabilityApprovals",
        "trustPolicy",
        "version",
        "name",
        "mesh",
    ] {
        if message.contains(key)
            && let Some(range) = json::diagnostics::find_key_range(source, key)
        {
            return range;
        }
    }

    json::diagnostics::find_key_range(source, "mesh")
        .unwrap_or_else(|| json::diagnostics::range_at(source, 0, 1))
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
        // An incomplete document has no trustworthy object structure. Keep
        // the common per-module default rather than treating quoted text or a
        // key in an unrelated, malformed region as a root graph config.
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
        complete(
            &doc(&clean),
            crate::util::offset_to_position(&clean, offset),
        )
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
    fn only_canonical_module_json_is_a_manifest_document() {
        for name in ["package.json", "mesh.toml"] {
            let uri = Url::parse(&format!("file:///m/{name}")).unwrap();
            assert!(
                !is_manifest_uri(&uri),
                "{name} must not receive MESH manifest support"
            );
        }
        assert!(is_manifest_uri(
            &Url::parse("file:///m/module.json").expect("canonical manifest URI")
        ));
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
    fn accepts_unicode_escaped_known_manifest_keys() {
        let src = r#"{ "\u006eame": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontend" } }"#;
        let d = diagnostics(&doc(src));
        assert!(
            !d.iter()
                .any(|d| d.message.contains("unknown property `name`"))
        );
    }

    #[test]
    fn key_ranges_use_object_keys_instead_of_matching_string_values() {
        let src = r#"{ "description": "mesh", "mesh": {} }"#;
        let range = crate::json::diagnostics::find_key_range(src, "mesh").unwrap();
        let key_start = src.find("\"mesh\":").unwrap();

        assert_eq!(range.start, crate::json::offset_to_position(src, key_start));
        assert_eq!(
            range.end,
            crate::json::offset_to_position(src, key_start + "\"mesh\"".len())
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
    fn reports_canonical_runtime_validation_for_module_manifests() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "entry": "../outside.mesh" } }"#;
        let d = diagnostics(&doc(src));
        let runtime = d
            .iter()
            .find(|diagnostic| diagnostic.source.as_deref() == Some("mesh-runtime"))
            .expect("canonical runtime diagnostic");
        assert!(runtime.message.contains("relative path"));
        assert_eq!(
            runtime.range.start,
            json::offset_to_position(src, src.find("\"entry\"").unwrap())
        );
    }

    #[test]
    fn reports_canonical_runtime_shape_errors() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "entry": 7 } }"#;
        let d = diagnostics(&doc(src));
        let runtime = d
            .iter()
            .find(|diagnostic| diagnostic.source.as_deref() == Some("mesh-runtime"))
            .expect("canonical runtime shape diagnostic");
        assert!(runtime.message.contains("expected a string"));
        assert!(runtime.range.start.character > src.find("\"entry\"").unwrap() as u32);
    }

    #[test]
    fn reports_canonical_runtime_validation_for_root_configs() {
        let src = r#"{ "mesh": { "schemaVersion": 2 } }"#;
        let root = ManifestDocument::new(
            Url::parse("file:///m/config/module.json").unwrap(),
            src.to_string(),
        );
        let d = diagnostics(&root);
        let runtime = d
            .iter()
            .find(|diagnostic| diagnostic.source.as_deref() == Some("mesh-runtime"))
            .expect("canonical root runtime diagnostic");
        assert!(runtime.message.contains("supported version is 1"));
        assert_eq!(
            runtime.range.start,
            json::offset_to_position(src, src.find("\"schemaVersion\"").unwrap())
        );
    }

    #[test]
    fn preserves_runtime_migration_diagnostic_field_spans() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "surfaceLayout": { "anchor": "top" } } }"#;
        let d = diagnostics(&doc(src));
        let runtime = d
            .iter()
            .find(|diagnostic| diagnostic.source.as_deref() == Some("mesh-runtime"))
            .expect("canonical migration diagnostic");
        assert!(runtime.message.contains("surfaceLayout"));
        assert!(runtime.message.contains("suggested action"));
        assert_eq!(
            runtime.range.start,
            json::offset_to_position(src, src.find("\"surfaceLayout\"").unwrap())
        );
    }

    #[test]
    fn reports_syntax_error() {
        let src = r#"{ "name": "@x/y" "version": "1.0.0" }"#;
        let d = diagnostics(&doc(src));
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("syntax error"));
    }

    #[test]
    fn malformed_text_does_not_select_root_flavor() {
        let src = r#"{ "description": "schemaVersion""#;
        assert_eq!(doc(src).flavor, ManifestFlavor::Module);
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
