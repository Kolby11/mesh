/// Parser for `.mesh` single-file components.
///
/// Splits the source into top-level blocks (`<template>`, `<script>`, `<style>`,
/// `<schema>`, `<i18n>`, `<meta>`) then parses each block with its own sub-parser.
use crate::{
    ComponentFile, ScriptBlock, ScriptLang,
    i18n::I18nBlock,
    meta::MetaBlock,
    schema::SchemaBlock,
    style::{Declaration, Selector, StyleBlock, StyleRule, StyleValue},
    template::*,
};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unclosed block <{tag}> opened at line {line}")]
    UnclosedBlock { tag: String, line: usize },

    #[error("unexpected closing tag </{tag}> at line {line}")]
    UnexpectedClose { tag: String, line: usize },

    #[error("invalid style syntax at line {line}: {message}")]
    InvalidStyle { message: String, line: usize },

    #[error("invalid schema TOML: {0}")]
    InvalidSchema(#[from] toml::de::Error),

    #[error("invalid i18n block: {0}")]
    InvalidI18n(String),

    #[error("invalid meta block: {0}")]
    InvalidMeta(String),

    #[error("unknown block <{name}> at line {line}")]
    UnknownBlock { name: String, line: usize },
}

/// Parse a `.mesh` file source into a `ComponentFile`.
pub fn parse_component(source: &str) -> Result<ComponentFile, ParseError> {
    let blocks = extract_blocks(source)?;

    let template = blocks
        .get("template")
        .map(|s| parse_template(s))
        .transpose()?;

    let script = blocks.get("script").map(|s| parse_script(s, &blocks));

    let style = blocks
        .get("style")
        .map(|s| parse_style(s))
        .transpose()?;

    let schema = blocks
        .get("schema")
        .map(|s| parse_schema(s))
        .transpose()?;

    let i18n = blocks
        .get("i18n")
        .map(|s| parse_i18n(s))
        .transpose()?;

    let meta = blocks
        .get("meta")
        .map(|s| parse_meta(s))
        .transpose()?;

    Ok(ComponentFile {
        template,
        script,
        style,
        schema,
        i18n,
        meta,
    })
}

/// Extract top-level blocks from the source text.
fn extract_blocks(source: &str) -> Result<HashMap<String, String>, ParseError> {
    let mut blocks = HashMap::new();
    let known_tags = ["template", "script", "style", "schema", "i18n", "meta"];

    let mut remaining = source;
    let mut line_offset = 1;

    while !remaining.is_empty() {
        // Find next opening tag.
        let Some(open_start) = remaining.find('<') else {
            break;
        };

        // Skip whitespace before the tag.
        let before = &remaining[..open_start];
        line_offset += before.chars().filter(|&c| c == '\n').count();

        let after_open = &remaining[open_start + 1..];
        let Some(open_end) = after_open.find('>') else {
            break;
        };

        let tag_content = &after_open[..open_end];
        // Extract tag name (might have attributes like `script lang="luau"`).
        let tag_name = tag_content.split_whitespace().next().unwrap_or("");

        if tag_name.starts_with('/') || tag_name.is_empty() {
            remaining = &after_open[open_end + 1..];
            continue;
        }

        if !known_tags.contains(&tag_name) {
            // Skip unknown top-level content (could be text between blocks).
            remaining = &after_open[open_end + 1..];
            continue;
        }

        let close_tag = format!("</{tag_name}>");
        let body_start = open_start + 1 + open_end + 1;
        let body_source = &remaining[body_start..];

        let Some(close_pos) = body_source.find(close_tag.as_str()) else {
            return Err(ParseError::UnclosedBlock {
                tag: tag_name.to_string(),
                line: line_offset,
            });
        };

        let body = &body_source[..close_pos];
        blocks.insert(tag_name.to_string(), body.to_string());

        let skip = body_start + close_pos + close_tag.len();
        line_offset += remaining[..skip].chars().filter(|&c| c == '\n').count();
        remaining = &remaining[skip..];
    }

    Ok(blocks)
}

// -- Template parser --

fn parse_template(source: &str) -> Result<TemplateBlock, ParseError> {
    let nodes = parse_nodes(source.trim())?;
    Ok(TemplateBlock { root: nodes })
}

fn parse_nodes(source: &str) -> Result<Vec<TemplateNode>, ParseError> {
    let mut nodes = Vec::new();
    let mut remaining = source;

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        if remaining.starts_with("{{") {
            // Expression interpolation.
            let end = remaining.find("}}").unwrap_or(remaining.len());
            let expr = remaining[2..end].trim().to_string();
            nodes.push(TemplateNode::Expr(ExprNode { expression: expr }));
            remaining = if end + 2 <= remaining.len() {
                &remaining[end + 2..]
            } else {
                ""
            };
        } else if remaining.starts_with("<") {
            // Element or self-closing tag.
            let (node, rest) = parse_element(remaining)?;
            nodes.push(node);
            remaining = rest;
        } else {
            // Text content — consume until we hit `<` or `{{`.
            let end = remaining
                .find(|c: char| c == '<')
                .unwrap_or(remaining.len());
            let end = remaining
                .find("{{")
                .map(|e| e.min(end))
                .unwrap_or(end);
            let text = remaining[..end].trim();
            if !text.is_empty() {
                nodes.push(TemplateNode::Text(TextNode {
                    content: text.to_string(),
                }));
            }
            remaining = &remaining[end..];
        }
    }

    Ok(nodes)
}

fn parse_element(source: &str) -> Result<(TemplateNode, &str), ParseError> {
    // Find the end of the opening tag.
    let tag_end = source.find('>').unwrap_or(source.len());
    let tag_content = &source[1..tag_end];
    let self_closing = tag_content.ends_with('/');
    let tag_content = tag_content.trim_end_matches('/');

    // Split into tag name and attributes.
    let mut parts = tag_content.split_whitespace();
    let tag_name = parts.next().unwrap_or("").to_string();

    // Parse attributes.
    let attr_str: String = parts.collect::<Vec<&str>>().join(" ");
    let attributes = parse_attributes(&attr_str);

    let after_open = &source[tag_end + 1..];

    if self_closing {
        let node = TemplateNode::Element(ElementNode {
            tag: tag_name,
            attributes,
            children: Vec::new(),
        });
        return Ok((node, after_open));
    }

    // Find the matching closing tag.
    let close_tag = format!("</{tag_name}>");
    let (children_src, rest) = find_matching_close(after_open, &tag_name, &close_tag)?;

    let children = parse_nodes(children_src)?;

    let node = TemplateNode::Element(ElementNode {
        tag: tag_name,
        attributes,
        children,
    });

    Ok((node, rest))
}

fn find_matching_close<'a>(
    source: &'a str,
    tag_name: &str,
    close_tag: &str,
) -> Result<(&'a str, &'a str), ParseError> {
    let mut depth = 1u32;
    let open_pattern = format!("<{tag_name}");
    let mut pos = 0;

    while pos < source.len() {
        if source[pos..].starts_with(close_tag) {
            depth -= 1;
            if depth == 0 {
                let children = &source[..pos];
                let rest = &source[pos + close_tag.len()..];
                return Ok((children, rest));
            }
            pos += close_tag.len();
        } else if source[pos..].starts_with(&open_pattern) {
            depth += 1;
            pos += open_pattern.len();
        } else {
            pos += 1;
        }
    }

    Err(ParseError::UnclosedBlock {
        tag: tag_name.to_string(),
        line: 0,
    })
}

fn parse_attributes(attr_str: &str) -> Vec<Attribute> {
    let mut attrs = Vec::new();
    let mut remaining = attr_str.trim();

    while !remaining.is_empty() {
        // Skip whitespace.
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        // Find the `=` separator.
        let Some(eq_pos) = remaining.find('=') else {
            break;
        };

        let name = remaining[..eq_pos].trim();
        let after_eq = remaining[eq_pos + 1..].trim();

        // Extract quoted value.
        let (value_str, rest) = if after_eq.starts_with('"') {
            let end = after_eq[1..].find('"').unwrap_or(after_eq.len() - 1);
            (&after_eq[1..=end], &after_eq[end + 2..])
        } else {
            let end = after_eq
                .find(char::is_whitespace)
                .unwrap_or(after_eq.len());
            (&after_eq[..end], &after_eq[end..])
        };

        // Determine binding type from name prefix.
        let (attr_name, attr_value) = if let Some(stripped) = name.strip_prefix(':') {
            (stripped, AttributeValue::Binding(value_str.to_string()))
        } else if let Some(stripped) = name.strip_prefix('@') {
            (
                stripped,
                AttributeValue::EventHandler(value_str.to_string()),
            )
        } else {
            (name, AttributeValue::Static(value_str.to_string()))
        };

        attrs.push(Attribute {
            name: attr_name.to_string(),
            value: attr_value,
        });

        remaining = rest;
    }

    attrs
}

// -- Script parser --

fn parse_script(source: &str, blocks: &HashMap<String, String>) -> ScriptBlock {
    // Check for lang attribute in the original block tag.
    let _ = blocks; // Lang detection would come from the tag attributes.
    ScriptBlock {
        lang: ScriptLang::Luau,
        source: source.to_string(),
    }
}

// -- Style parser --

fn parse_style(source: &str) -> Result<StyleBlock, ParseError> {
    let mut rules = Vec::new();
    let mut remaining = source.trim();

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        // Find selector (everything before `{`).
        let Some(brace_start) = remaining.find('{') else {
            break;
        };
        let selector_str = remaining[..brace_start].trim();
        let selector = parse_selector(selector_str);

        // Find matching `}`.
        let after_brace = &remaining[brace_start + 1..];
        let Some(brace_end) = after_brace.find('}') else {
            return Err(ParseError::InvalidStyle {
                message: "unclosed rule block".to_string(),
                line: 0,
            });
        };

        let declarations_str = &after_brace[..brace_end];
        let declarations = parse_declarations(declarations_str);

        rules.push(StyleRule {
            selector,
            declarations,
        });

        remaining = &after_brace[brace_end + 1..];
    }

    Ok(StyleBlock { rules })
}

fn parse_selector(s: &str) -> Selector {
    let s = s.trim();
    if s == "*" {
        Selector::Universal
    } else if let Some(class) = s.strip_prefix('.') {
        Selector::Class(class.to_string())
    } else if let Some(id) = s.strip_prefix('#') {
        Selector::Id(id.to_string())
    } else if let Some((tag, state)) = s.split_once(':') {
        Selector::State(tag.to_string(), state.to_string())
    } else {
        Selector::Tag(s.to_string())
    }
}

fn parse_declarations(s: &str) -> Vec<Declaration> {
    s.split(';')
        .filter_map(|decl| {
            let decl = decl.trim();
            if decl.is_empty() {
                return None;
            }
            let (prop, value) = decl.split_once(':')?;
            let prop = prop.trim().to_string();
            let value_str = value.trim();

            let style_value = if value_str.starts_with("token(") && value_str.ends_with(')') {
                let token_name = &value_str[6..value_str.len() - 1];
                StyleValue::Token(token_name.trim().to_string())
            } else if value_str.starts_with("var(") && value_str.ends_with(')') {
                let var_name = &value_str[4..value_str.len() - 1];
                StyleValue::Var(var_name.trim().to_string())
            } else {
                StyleValue::Literal(value_str.to_string())
            };

            Some(Declaration {
                property: prop,
                value: style_value,
            })
        })
        .collect()
}

// -- Schema parser --

fn parse_schema(source: &str) -> Result<SchemaBlock, ParseError> {
    let block: SchemaBlock = toml::from_str(source)?;
    Ok(block)
}

// -- I18n parser --

fn parse_i18n(source: &str) -> Result<I18nBlock, ParseError> {
    let raw: HashMap<String, HashMap<String, String>> =
        toml::from_str(source).map_err(|e| ParseError::InvalidI18n(e.to_string()))?;
    Ok(I18nBlock { entries: raw })
}

// -- Meta parser --

fn parse_meta(source: &str) -> Result<MetaBlock, ParseError> {
    let block: MetaBlock =
        toml::from_str(source).map_err(|e| ParseError::InvalidMeta(e.to_string()))?;
    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_component() {
        let source = r#"
<template>
  <text>Hello</text>
</template>
"#;
        let file = parse_component(source).unwrap();
        assert!(file.template.is_some());
        assert!(file.script.is_none());
        assert!(file.style.is_none());
    }

    #[test]
    fn parse_full_component() {
        let source = r#"
<template>
  <column>
    <text class="title">{{ title }}</text>
    <button @click="onTap">Click me</button>
  </column>
</template>

<script lang="luau">
local title = "Hello"
function onTap()
    title = "Clicked!"
end
</script>

<style>
.title {
    color: token(color.on-surface);
    font-size: token(typography.size.lg);
}

button {
    background: token(color.primary);
    padding: 8px;
}
</style>

<schema>
[greeting]
type = "string"
default = "Hello"
description = "The greeting text"
</schema>

<i18n>
[en]
greeting = "Hello"

[fr]
greeting = "Bonjour"
</i18n>

<meta>
name = "Greeter"
description = "A simple greeting component"
role = "region"
</meta>
"#;
        let file = parse_component(source).unwrap();

        // Template.
        let tmpl = file.template.unwrap();
        assert_eq!(tmpl.root.len(), 1);

        // Script.
        let script = file.script.unwrap();
        assert_eq!(script.lang, ScriptLang::Luau);
        assert!(script.source.contains("function onTap"));

        // Style.
        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 2);
        match &style.rules[0].declarations[0].value {
            StyleValue::Token(name) => assert_eq!(name, "color.on-surface"),
            other => panic!("expected token, got {other:?}"),
        }

        // Schema.
        let schema = file.schema.unwrap();
        assert!(schema.fields.contains_key("greeting"));

        // I18n.
        let i18n = file.i18n.unwrap();
        assert_eq!(i18n.entries["en"]["greeting"], "Hello");
        assert_eq!(i18n.entries["fr"]["greeting"], "Bonjour");

        // Meta.
        let meta = file.meta.unwrap();
        assert_eq!(meta.name.unwrap(), "Greeter");
    }

    #[test]
    fn parse_expression_interpolation() {
        let source = r#"
<template>
  <text>Time: {{ formatTime(time) }}</text>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        // The text element should contain text + expr children.
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "text");
                assert!(el.children.len() >= 1);
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_style_tokens_and_literals() {
        let source = r#"
<style>
row {
    gap: 8px;
    padding: token(spacing.md);
    background: var(--bg);
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        let decls = &style.rules[0].declarations;
        assert!(matches!(&decls[0].value, StyleValue::Literal(v) if v == "8px"));
        assert!(matches!(&decls[1].value, StyleValue::Token(v) if v == "spacing.md"));
        assert!(matches!(&decls[2].value, StyleValue::Var(v) if v == "--bg"));
    }

    #[test]
    fn parse_self_closing_element() {
        let source = r#"
<template>
  <icon name="battery" size="24"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "icon");
                assert_eq!(el.children.len(), 0);
                assert_eq!(el.attributes.len(), 2);
            }
            _ => panic!("expected self-closing element"),
        }
    }

    #[test]
    fn parse_binding_and_event_attributes() {
        let source = r#"
<template>
  <slider :value="volume" @change="onVolumeChange"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(matches!(&el.attributes[0].value, AttributeValue::Binding(v) if v == "volume"));
                assert!(matches!(&el.attributes[1].value, AttributeValue::EventHandler(v) if v == "onVolumeChange"));
            }
            _ => panic!("expected element"),
        }
    }
}
