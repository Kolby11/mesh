use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

use crate::{
    document::Document, knowledge::css::CSS_PROPERTIES, module_registry::ModuleRegistry,
    util::StyleContext,
};
use std::{collections::BTreeSet, sync::OnceLock};

pub fn complete(
    ctx: StyleContext,
    doc: &Document,
    style_source: &str,
    registry: &ModuleRegistry,
) -> Vec<CompletionItem> {
    match ctx {
        StyleContext::Property => complete_properties(),
        StyleContext::Value { property } => complete_values(&property),
        StyleContext::Variable { prefix } => {
            let tokens = doc
                .uri
                .to_file_path()
                .ok()
                .map(|path| registry.local_theme_tokens(&path))
                .unwrap_or_default();
            complete_variables(&prefix, style_source, &tokens)
        }
        StyleContext::Prop { prefix } => complete_props(&prefix, doc),
        StyleContext::Selector => vec![],
        StyleContext::Part { host } => complete_parts(host.as_deref()),
    }
}

/// Control parts the host tag exposes, or every control's parts when the
/// host names no tag (`.volume::`).
fn complete_parts(host: Option<&str>) -> Vec<CompletionItem> {
    const HOSTS: &[&str] = &["slider", "progress", "meter", "checkbox", "radio", "switch"];
    let hosts = match host {
        Some(tag) => vec![tag],
        None => HOSTS.to_vec(),
    };
    let mut parts = BTreeSet::new();
    for host in hosts {
        for part in mesh_core_elements::control_part_names(host) {
            parts.insert((*part, host));
        }
    }
    let mut seen = BTreeSet::new();
    parts
        .into_iter()
        .filter(|(part, _)| seen.insert(*part))
        .map(|(part, host)| CompletionItem {
            label: part.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(format!("{host} control part")),
            ..Default::default()
        })
        .collect()
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
    items.push(CompletionItem {
        label: "prop()".to_string(),
        insert_text: Some("prop($1)".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some("Reference a typed component prop".to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "Reference a prop declared in this component's `<props>` block.".to_string(),
        })),
        sort_text: Some("9_prop".to_string()),
        ..Default::default()
    });

    items
}

fn complete_props(prefix: &str, doc: &Document) -> Vec<CompletionItem> {
    let Some(block) = doc.parsed.as_ref().and_then(|parsed| parsed.props.as_ref()) else {
        return vec![];
    };
    block
        .props
        .iter()
        .filter(|prop| prop.name.starts_with(prefix))
        .map(|prop| CompletionItem {
            label: prop.name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(format!("prop: {}", prop.ty.as_str())),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "**`prop({})`**\n\nType: `{}`{}",
                    prop.name,
                    prop.ty.as_str(),
                    prop.default
                        .as_ref()
                        .map(|value| format!("\n\nDefault: `{}`", prop_default_label(value)))
                        .unwrap_or_default()
                ),
            })),
            ..Default::default()
        })
        .collect()
}

fn prop_default_label(value: &mesh_core_component::PropValue) -> String {
    match value {
        mesh_core_component::PropValue::String(value) => value.clone(),
        mesh_core_component::PropValue::Number(value) => value.to_string(),
        mesh_core_component::PropValue::Bool(value) => value.to_string(),
    }
}

fn complete_variables(prefix: &str, style_source: &str, tokens: &[String]) -> Vec<CompletionItem> {
    let normalized_prefix = prefix.trim();
    let alternate_prefix = normalized_prefix
        .strip_prefix("--")
        .map(|prefix| prefix.to_string())
        .unwrap_or_else(|| format!("--{normalized_prefix}"));

    available_css_variables(style_source, tokens)
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

fn available_css_variables(style_source: &str, tokens: &[String]) -> Vec<String> {
    let mut variables: BTreeSet<String> = theme_css_variables().iter().cloned().collect();
    variables.extend(local_css_variables(style_source));
    variables.extend(
        tokens
            .iter()
            .map(|name| format!("--{}", name.replace('.', "-"))),
    );
    variables.into_iter().collect()
}

fn theme_css_variables() -> &'static Vec<String> {
    static VARIABLES: OnceLock<Vec<String>> = OnceLock::new();
    VARIABLES.get_or_init(|| {
        let mut variables: Vec<String> = mesh_core_theme::semantic_token_contract()
            .tokens
            .iter()
            .map(|token| token.css.clone())
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
    fn part_completions_follow_the_host_tag() {
        let ctx = crate::util::style_context_at("slider:active::th", 17);
        assert!(matches!(&ctx, StyleContext::Part { host: Some(host) } if host == "slider"));
        assert_eq!(labels(&complete_parts(Some("slider"))), ["fill", "thumb", "track"]);
        assert_eq!(labels(&complete_parts(Some("switch"))), ["thumb", "track"]);
        assert!(complete_parts(Some("button")).is_empty());
        let ctx = crate::util::style_context_at(".a { width: 1px; }\n.volume::", 28);
        assert!(matches!(ctx, StyleContext::Part { host: None }));
        assert!(labels(&complete_parts(None)).contains(&"indicator"));
        assert!(matches!(
            crate::util::style_context_at("slider:hover", 12),
            StyleContext::Selector
        ));
    }

    #[test]
    fn completions_publish_semantics_and_keep_palette_tokens_private() {
        let items = complete_variables("", "", &[]);
        assert!(labels(&items).contains(&"--font-body"));
        assert!(labels(&items).contains(&"--icon-size-md"));
        assert!(!labels(&items).contains(&"--base16-base00"));
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
        assert!(labels.contains(&"prop()"));
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
            &[],
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
            &[],
        );
        assert!(labels(&local_items).contains(&"--custom-accent"));
    }

    #[test]
    fn variable_completions_accept_prefix_without_custom_property_marker() {
        let items = complete_variables("color-p", "", &[]);
        assert!(labels(&items).contains(&"--color-primary"));
    }
}
