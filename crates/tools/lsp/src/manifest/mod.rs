//! Language support for MESH `module.json` / `package.json` manifests.
//!
//! Two manifest flavors share the same `name`/`version`/`mesh` envelope:
//! per-module manifests ([`ManifestFlavor::Module`]) and the workspace root
//! graph config ([`ManifestFlavor::RootConfig`]). This module provides
//! diagnostics, completion, and hover for both, driven by [`schema`].

use tower_lsp::lsp_types::{Position, Url};

pub mod complete;
pub mod cursor;
pub mod diagnostics;
pub mod hover;
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

/// Convert a byte offset into an LSP [`Position`] (0-based line + column).
/// Columns are counted in UTF-16 code units, matching the LSP spec.
pub fn offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    Position::new(line, col)
}

/// Convert a 1-based line / 0-based column (as serde_json reports) to a byte
/// offset into `source`.
pub fn line_col_to_offset(source: &str, line: usize, column: usize) -> usize {
    let mut current_line = 1usize;
    let mut offset = 0usize;
    for (i, ch) in source.char_indices() {
        if current_line == line {
            // serde_json columns are 1-based byte-ish counts within the line.
            return (i + column.saturating_sub(1)).min(source.len());
        }
        if ch == '\n' {
            current_line += 1;
        }
        offset = i + ch.len_utf8();
    }
    offset.min(source.len())
}
