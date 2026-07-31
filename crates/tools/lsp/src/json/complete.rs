//! Schema-driven completion for JSON documents: object keys at the cursor's
//! path and enumerated / suggested values in value position.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind, Position,
};

use super::cursor::{self, Role};
use super::schema::{self, Kind, Node};
use crate::util::position_to_offset;

pub fn complete(root: &Node, source: &str, position: Position) -> Vec<CompletionItem> {
    let offset = position_to_offset(source, position);
    let ctx = cursor::context_at(source, offset);

    match ctx.role {
        Role::Key => complete_keys(root, &ctx),
        Role::Value => complete_values(root, &ctx),
    }
}

fn complete_keys(root: &Node, ctx: &cursor::CursorContext) -> Vec<CompletionItem> {
    let Some(node) = schema::navigate(root, &ctx.path) else {
        return vec![];
    };
    let Some(fields) = schema::fields_of(node) else {
        return vec![];
    };

    fields
        .iter()
        .filter(|f| !ctx.existing_keys.contains(&f.name))
        .filter(|f| f.name.starts_with(ctx.partial.as_str()))
        .map(|f| {
            let insert = if ctx.in_string {
                f.name.clone()
            } else {
                format!("\"{}\"", f.name)
            };
            CompletionItem {
                label: f.name.clone(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(detail_for(&f.node)),
                documentation: Some(doc_markup(&f.node.doc)),
                insert_text: Some(insert),
                ..Default::default()
            }
        })
        .collect()
}

fn complete_values(root: &Node, ctx: &cursor::CursorContext) -> Vec<CompletionItem> {
    // Resolve the schema node describing the value being edited.
    let value_node = if ctx.innermost_is_array {
        // `ctx.path` ends at the array key; navigate yields the Array node.
        match schema::navigate(root, &ctx.path).map(|n| &n.kind) {
            Some(Kind::Array(element)) => Some(element.as_ref()),
            _ => None,
        }
    } else {
        let Some(key) = ctx.value_key.as_deref() else {
            return vec![];
        };
        schema::navigate(root, &ctx.path).and_then(|container| schema::field_node(container, key))
    };

    let Some(node) = value_node else {
        return vec![];
    };
    let Some(values) = schema::suggested_values(node) else {
        return vec![];
    };

    values
        .iter()
        .filter(|v| v.starts_with(ctx.partial.as_str()))
        .map(|v| {
            let insert = if ctx.in_string {
                v.clone()
            } else {
                format!("\"{v}\"")
            };
            CompletionItem {
                label: v.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                documentation: Some(doc_markup(&node.doc)),
                insert_text: Some(insert),
                ..Default::default()
            }
        })
        .collect()
}

fn detail_for(node: &Node) -> String {
    match &node.kind {
        Kind::Enum(values) => values.join(" | "),
        _ => node.type_hint.clone(),
    }
}

fn doc_markup(doc: &str) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: doc.to_string(),
    })
}
