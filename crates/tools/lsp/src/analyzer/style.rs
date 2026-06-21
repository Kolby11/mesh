use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

use crate::{knowledge::css::CSS_PROPERTIES, util::StyleContext};
use std::{collections::BTreeSet, sync::OnceLock};

pub fn complete(ctx: StyleContext, style_source: &str) -> Vec<CompletionItem> {
    match ctx {
        StyleContext::Property => complete_properties(),
        StyleContext::Value { property } => complete_values(&property),
        StyleContext::Variable { prefix } => complete_variables(&prefix, style_source),
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

    // Property-specific enum values
    if let Some(prop_def) = CSS_PROPERTIES.iter().find(|p| p.name == property) {
        for &val in prop_def.values {
            if val == "var()" {
                continue; // already added above
            }
            items.push(CompletionItem {
                label: val.to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                sort_text: Some(format!("0_{val}")),
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
                sort_text: Some(format!("0_{color}")),
                ..Default::default()
            });
        }
    }

    // var() is callable syntax, so keep it after concrete string values.
    items.push(CompletionItem {
        label: "var()".to_string(),
        insert_text: Some("var(--$1)".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some("Reference a theme or local CSS variable".to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "Reference a theme variable or local custom property.\n\nExamples: `var(--color-primary)`, `var(--spacing-md)`".to_string(),
        })),
        sort_text: Some("9_var".to_string()),
        ..Default::default()
    });

    items
}

fn complete_variables(prefix: &str, style_source: &str) -> Vec<CompletionItem> {
    let normalized_prefix = prefix.trim();
    let alternate_prefix = normalized_prefix
        .strip_prefix("--")
        .map(|prefix| prefix.to_string())
        .unwrap_or_else(|| format!("--{normalized_prefix}"));

    available_css_variables(style_source)
        .into_iter()
        .filter(|name| {
            normalized_prefix.is_empty()
                || name.starts_with(normalized_prefix)
                || name.starts_with(&alternate_prefix)
        })
        .map(|name| CompletionItem {
            label: name.clone(),
            insert_text: Some(name),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("CSS variable".to_string()),
            sort_text: Some("0".to_string()),
            ..Default::default()
        })
        .collect()
}

fn available_css_variables(style_source: &str) -> Vec<String> {
    let mut variables: BTreeSet<String> = theme_css_variables().iter().cloned().collect();
    variables.extend(local_css_variables(style_source));
    variables.into_iter().collect()
}

fn theme_css_variables() -> &'static Vec<String> {
    static VARIABLES: OnceLock<Vec<String>> = OnceLock::new();
    VARIABLES.get_or_init(|| {
        let mut variables: Vec<String> = mesh_core_theme::default_theme()
            .tokens
            .keys()
            .map(|name| format!("--{}", name.replace('.', "-")))
            .collect();
        variables.sort();
        variables.dedup();
        variables
    })
}

fn local_css_variables(style_source: &str) -> Vec<String> {
    let mut variables = BTreeSet::new();
    for declaration in style_source.split(';') {
        let Some((property, _value)) = declaration.rsplit_once(':') else {
            continue;
        };
        let property = property
            .split(['{', '}'])
            .next_back()
            .unwrap_or(property)
            .trim();
        if property.starts_with("--")
            && property[2..]
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            variables.insert(property.to_string());
        }
    }
    variables.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    #[test]
    fn value_completions_put_literal_values_before_var_callable() {
        let items = complete_values("background");
        let labels = labels(&items);
        let transparent = labels
            .iter()
            .position(|label| *label == "transparent")
            .expect("transparent completion");
        let var = labels
            .iter()
            .position(|label| *label == "var()")
            .expect("var completion");

        assert!(transparent < var);
        assert!(!labels.contains(&"token()"));
    }

    #[test]
    fn variable_completions_include_theme_and_local_custom_properties() {
        let items = complete_variables(
            "--co",
            r#"
            .panel {
                --custom-accent: #fff;
                color: var(--co
            }
            "#,
        );
        let item_labels = labels(&items);

        assert!(item_labels.contains(&"--color-primary"));
        assert!(item_labels.contains(&"--color-on-surface"));
        assert!(!item_labels.contains(&"--custom-accent"));

        let local_items = complete_variables(
            "--custom",
            r#"
            .panel {
                --custom-accent: #fff;
                color: var(--custom
            }
            "#,
        );
        assert!(labels(&local_items).contains(&"--custom-accent"));
    }

    #[test]
    fn variable_completions_accept_prefix_without_custom_property_marker() {
        let items = complete_variables("color-p", "");
        assert!(labels(&items).contains(&"--color-primary"));
    }
}
