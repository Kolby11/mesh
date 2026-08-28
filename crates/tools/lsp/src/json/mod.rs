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

use tower_lsp::lsp_types::Position;

pub mod complete;
pub mod cursor;
pub mod diagnostics;
pub mod hover;
pub mod schema;

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
