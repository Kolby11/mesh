//! Schema-driven hover for JSON documents: documents the key (or enum value)
//! under the cursor.

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use super::cursor::{self, Role};
use super::schema::{self, Kind, Node};
use crate::util::position_to_offset;

pub fn hover(root: &Node, source: &str, position: Position) -> Option<Hover> {
    let offset = position_to_offset(source, position);
    let ctx = cursor::context_at(source, offset);

    let (title, node) = match ctx.role {
        Role::Key => {
            let container = schema::navigate(root, &ctx.path)?;
            let node = schema::field_node(container, &ctx.token)?;
            (format!("`{}`", ctx.token), node)
        }
        Role::Value => {
            if ctx.innermost_is_array {
                match &schema::navigate(root, &ctx.path)?.kind {
                    Kind::Array(element) => ("value".to_string(), element.as_ref()),
                    _ => return None,
                }
            } else {
                let key = ctx.value_key.as_deref()?;
                let container = schema::navigate(root, &ctx.path)?;
                let node = schema::field_node(container, key)?;
                (format!("`{key}`"), node)
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
        Kind::SuggestDiscovered(values) if !values.is_empty() => {
            out.push_str("\n\nFound in this workspace:\n");
            for v in values {
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
        Kind::Suggest(_) | Kind::SuggestDiscovered(_) => "string".to_string(),
        _ => node.type_hint.clone(),
    }
}
