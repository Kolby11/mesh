use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

use crate::{knowledge::css::CSS_PROPERTIES, util::StyleContext};

pub fn complete(ctx: StyleContext) -> Vec<CompletionItem> {
    match ctx {
        StyleContext::Property => complete_properties(),
        StyleContext::Value { property } => complete_values(&property),
        StyleContext::Selector => vec![],
    }
}

fn complete_properties() -> Vec<CompletionItem> {
    CSS_PROPERTIES
        .iter()
        .map(|prop| CompletionItem {
            label: prop.name.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(prop.description.to_string()),
            insert_text: Some(format!("{}: $1;", prop.name)),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "**{}**\n\n{}{}",
                    prop.name,
                    prop.description,
                    if prop.values.is_empty() {
                        String::new()
                    } else {
                        format!("\n\nValues: `{}`", prop.values.join("`, `"))
                    }
                ),
            })),
            ..Default::default()
        })
        .collect()
}

fn complete_values(property: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // token() function — always available
    items.push(CompletionItem {
        label: "token()".to_string(),
        insert_text: Some("token($1)".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some("Reference a theme token".to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "Reference a theme design token by name.\n\nExample: `token(color.primary)`, `token(spacing.md)`".to_string(),
        })),
        sort_text: Some("0".to_string()), // sort first
        ..Default::default()
    });

    // var() function
    items.push(CompletionItem {
        label: "var()".to_string(),
        insert_text: Some("var($1)".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some("Reference a CSS variable".to_string()),
        sort_text: Some("1".to_string()),
        ..Default::default()
    });

    // Property-specific enum values
    if let Some(prop_def) = CSS_PROPERTIES.iter().find(|p| p.name == property) {
        for &val in prop_def.values {
            if val == "token()" || val == "var()" {
                continue; // already added above
            }
            items.push(CompletionItem {
                label: val.to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                ..Default::default()
            });
        }
    }

    // Color keywords for color properties
    if matches!(
        property,
        "color" | "background" | "background-color" | "border-color"
    ) {
        for color in &["transparent", "black", "white", "currentColor"] {
            items.push(CompletionItem {
                label: color.to_string(),
                kind: Some(CompletionItemKind::COLOR),
                ..Default::default()
            });
        }
    }

    items
}
