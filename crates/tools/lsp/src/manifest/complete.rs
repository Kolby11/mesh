//! Completion for manifest documents: object keys at the cursor's path and
//! enum values (module kinds, capabilities, ...) in value position.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind, Position,
};

use super::ManifestDocument;
use super::cursor::{self, Role};
use super::schema::{self, Kind, Node};
use crate::util::position_to_offset;

pub fn complete(doc: &ManifestDocument, position: Position) -> Vec<CompletionItem> {
    let offset = position_to_offset(&doc.source, position);
    let ctx = cursor::context_at(&doc.source, offset);
    let root = schema::root(doc.flavor);

    match ctx.role {
        Role::Key => complete_keys(&root, &ctx),
        Role::Value => complete_values(&root, &ctx),
    }
}

fn complete_keys(root: &Node, ctx: &cursor::CursorContext) -> Vec<CompletionItem> {
    let Some(node) = schema::navigate(root, &ctx.path) else {
        return vec![];
    };
    let Kind::Object(fields) = &node.kind else {
        return vec![];
    };

    fields
        .iter()
        .filter(|f| !ctx.existing_keys.iter().any(|k| k == f.name))
        .filter(|f| f.name.starts_with(ctx.partial.as_str()))
        .map(|f| {
            let insert = if ctx.in_string {
                f.name.to_string()
            } else {
                format!("\"{}\"", f.name)
            };
            CompletionItem {
                label: f.name.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(detail_for(&f.node)),
                documentation: Some(doc_markup(f.node.doc)),
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
        match schema::navigate(root, &ctx.path).map(|n| &n.kind) {
            Some(Kind::Object(fields)) => fields.iter().find(|f| f.name == key).map(|f| &f.node),
            _ => None,
        }
    };

    let Some(node) = value_node else {
        return vec![];
    };

    let values = match &node.kind {
        Kind::Enum(values) | Kind::Suggest(values) => *values,
        _ => return vec![],
    };

    values
        .iter()
        .filter(|v| v.starts_with(ctx.partial.as_str()))
        .map(|v| {
            let insert = if ctx.in_string {
                v.to_string()
            } else {
                format!("\"{v}\"")
            };
            CompletionItem {
                label: v.to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                documentation: Some(doc_markup(node.doc)),
                insert_text: Some(insert),
                ..Default::default()
            }
        })
        .collect()
}

fn detail_for(node: &Node) -> String {
    match &node.kind {
        Kind::Enum(values) => values.join(" | "),
        _ => node.type_hint.to_string(),
    }
}

fn doc_markup(doc: &str) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: doc.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ManifestDocument;
    use tower_lsp::lsp_types::Url;

    fn complete_at(src: &str) -> Vec<String> {
        let offset = src.find('|').expect("need a | cursor marker");
        let clean = src.replacen('|', "", 1);
        let before = &clean[..offset];
        let line = before.matches('\n').count() as u32;
        let col = before.rsplit('\n').next().unwrap().chars().count() as u32;
        let doc = ManifestDocument::new(Url::parse("file:///m/module.json").unwrap(), clean);
        complete(&doc, Position::new(line, col))
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
}
