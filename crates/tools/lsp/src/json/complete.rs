//! Schema-driven completion for JSON documents: object keys at the cursor's
//! path, enumerated / suggested values in value position, and type names
//! inside service-contract type expressions and method signatures.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent,
    MarkupKind, Position,
};

use super::cursor::{self, Role};
use super::schema::{self, ContractSyntax, Kind, Node};
use crate::util::position_to_offset;

pub fn complete(root: &Node, source: &str, position: Position) -> Vec<CompletionItem> {
    let offset = position_to_offset(source, position);
    let ctx = cursor::context_at(source, offset);

    match ctx.role {
        Role::Key => complete_keys(root, &ctx),
        Role::Value => complete_values(root, source, &ctx),
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

fn complete_values(root: &Node, source: &str, ctx: &cursor::CursorContext) -> Vec<CompletionItem> {
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
    if let Kind::ContractText { syntax, .. } = &node.kind {
        return complete_contract_text(*syntax, source, ctx);
    }
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

const PRIMITIVE_TYPES: &[(&str, &str)] = &[
    ("string", "UTF-8 text."),
    ("int", "A whole number."),
    ("float", "A number."),
    ("boolean", "`true` or `false`."),
    (
        "blob",
        "A `mesh-blob:sha256-…` handle to image bytes registered with `mesh.blob.put`.",
    ),
    ("object", "Any JSON object."),
    ("any", "Any JSON value."),
];

/// Type names for the expression being typed: the contract's own `types`,
/// the primitives, and `Result`. In a signature, types are offered after `:`
/// and `->`; an empty signature offers a template.
fn complete_contract_text(
    syntax: ContractSyntax,
    source: &str,
    ctx: &cursor::CursorContext,
) -> Vec<CompletionItem> {
    let fragment_start = ctx
        .partial
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map_or(0, |index| index + 1);
    let (before, fragment) = ctx.partial.split_at(fragment_start);
    if syntax == ContractSyntax::Signature {
        let before = before.trim_end();
        if ctx.partial.trim().is_empty() {
            return vec![signature_template(ctx.in_string)];
        }
        if !(before.ends_with(':') || before.ends_with("->")) {
            return vec![];
        }
    }

    let quote = |name: &str| {
        if ctx.in_string {
            name.to_string()
        } else {
            format!("\"{name}\"")
        }
    };
    let named = ctx
        .path
        .iter()
        .rposition(|segment| segment == "contract")
        .map(|index| {
            let mut types_path = ctx.path[..=index].to_vec();
            types_path.push("types".into());
            cursor::keys_at_path(source, &types_path)
        })
        .unwrap_or_default();
    let named = named.iter().map(|name| {
        (
            name.as_str(),
            "Named type declared under `types` in this contract.",
            "0",
            CompletionItemKind::STRUCT,
        )
    });
    let primitives = PRIMITIVE_TYPES
        .iter()
        .map(|(name, doc)| (*name, *doc, "1", CompletionItemKind::KEYWORD));
    let result = std::iter::once((
        "Result",
        "Builtin command result: `{ ok: boolean, error: string? }`.",
        "2",
        CompletionItemKind::STRUCT,
    ));
    let fragment = fragment.to_ascii_lowercase();
    named
        .chain(primitives)
        .chain(result)
        .filter(|(name, ..)| name.to_ascii_lowercase().starts_with(&fragment))
        .map(|(name, doc, rank, kind)| CompletionItem {
            label: name.to_string(),
            kind: Some(kind),
            detail: Some("type — add `[]` for an array, `?` for optional".into()),
            documentation: Some(doc_markup(doc)),
            sort_text: Some(format!("{rank}{name}")),
            insert_text: Some(quote(name)),
            ..Default::default()
        })
        .collect()
}

fn signature_template(in_string: bool) -> CompletionItem {
    let body = "(${1:name}: ${2:type}) -> ${3:Result}";
    CompletionItem {
        label: "(name: type) -> Result".into(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some("method signature".into()),
        documentation: Some(doc_markup(
            "Arguments in call order, then the return type. Drop `-> Type` for a \
             method that returns nothing.",
        )),
        insert_text: Some(if in_string {
            body.to_string()
        } else {
            format!("\"{body}\"")
        }),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
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
