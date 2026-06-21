//! Hover for manifest documents: documents the key (or enum value) under the
//! cursor using the schema tree.

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use super::ManifestDocument;
use super::cursor::{self, Role};
use super::schema::{self, Kind, Node};
use crate::util::position_to_offset;

pub fn hover(doc: &ManifestDocument, position: Position) -> Option<Hover> {
    let offset = position_to_offset(&doc.source, position);
    let ctx = cursor::context_at(&doc.source, offset);
    let root = schema::root(doc.flavor);

    let (title, node) = match ctx.role {
        Role::Key => {
            let container = schema::navigate(&root, &ctx.path)?;
            let Kind::Object(fields) = &container.kind else {
                return None;
            };
            let field = fields.iter().find(|f| f.name == ctx.partial)?;
            (format!("`{}`", field.name), &field.node)
        }
        Role::Value => {
            if ctx.innermost_is_array {
                match &schema::navigate(&root, &ctx.path)?.kind {
                    Kind::Array(element) => ("value".to_string(), element.as_ref()),
                    _ => return None,
                }
            } else {
                let key = ctx.value_key.as_deref()?;
                let container = schema::navigate(&root, &ctx.path)?;
                let Kind::Object(fields) = &container.kind else {
                    return None;
                };
                let field = fields.iter().find(|f| f.name == key)?;
                (format!("`{}`", field.name), &field.node)
            }
        }
    };

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: render(&title, node),
        }),
        range: None,
    })
}

fn render(title: &str, node: &Node) -> String {
    let mut out = format!("{title} — *{}*\n\n{}", type_line(node), node.doc);
    match &node.kind {
        Kind::Enum(values) => {
            out.push_str("\n\nAllowed values:\n");
            for v in *values {
                out.push_str(&format!("- `{v}`\n"));
            }
        }
        Kind::Suggest(values) => {
            out.push_str("\n\nSuggested values (not exhaustive):\n");
            for v in *values {
                out.push_str(&format!("- `{v}`\n"));
            }
        }
        _ => {}
    }
    out
}

fn type_line(node: &Node) -> String {
    match &node.kind {
        Kind::Enum(_) => "enum".to_string(),
        Kind::Suggest(_) => "string".to_string(),
        _ => node.type_hint.to_string(),
    }
}
