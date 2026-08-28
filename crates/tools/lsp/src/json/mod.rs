//! Schema-driven language support for the JSON documents MESH owns.
//!
//! [`schema`] describes a document as a tree of nodes; [`complete`], [`hover`],
//! and [`diagnostics`] read that tree. [`cursor`] locates the cursor inside
//! JSON that may not parse, which is the normal state of a file being typed in.
//!
//! Callers supply editor metadata: [`crate::manifest`] uses a compact tree for
//! completion and hover while canonical manifest validation stays in
//! `mesh_core_module`; [`crate::settings`] derives its tree from the runtime's
//! own settings field tables.

pub mod complete;
pub mod cursor;
pub mod diagnostics;
pub mod hover;
pub mod schema;

pub use crate::util::offset_to_position;

/// Convert serde_json's 1-based line and 1-based UTF-8 byte column to a byte
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

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::Position;

    use super::*;

    #[test]
    fn serde_json_byte_columns_become_utf16_diagnostic_positions() {
        let source = r#"{ "é": }"#;
        let error = serde_json::from_str::<serde_json::Value>(source).unwrap_err();
        let offset = line_col_to_offset(source, error.line(), error.column());

        assert_eq!(source.as_bytes()[offset], b'}');
        assert_eq!(offset_to_position(source, offset), Position::new(0, 7));
    }
}
