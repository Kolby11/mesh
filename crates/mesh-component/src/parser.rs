/// Parser for `.mesh` single-file components.
///
/// Splits the source into top-level blocks (`<template>`, `<script>`, `<style>`,
/// `<schema>`, `<i18n>`, `<meta>`) then parses each block with parser libraries.
use crate::{
    ComponentFile, ScriptBlock, ScriptLang,
    i18n::I18nBlock,
    meta::MetaBlock,
    schema::SchemaBlock,
    style::{Declaration, Selector, StyleBlock, StyleRule, StyleValue},
    template::*,
};
use cssparser::{Parser, ParserInput, Token, ToCss};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unclosed block <{tag}> opened at line {line}")]
    UnclosedBlock { tag: String, line: usize },

    #[error("unexpected closing tag </{tag}> at line {line}")]
    UnexpectedClose { tag: String, line: usize },

    #[error("invalid template syntax: {message}")]
    InvalidTemplate { message: String },

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

fn extract_blocks(source: &str) -> Result<HashMap<String, String>, ParseError> {
    let mut blocks = HashMap::new();
    let known_tags = ["template", "script", "style", "schema", "i18n", "meta"];

    let mut remaining = source;
    let mut line_offset = 1;

    while !remaining.is_empty() {
        let Some(open_start) = remaining.find('<') else {
            break;
        };

        let before = &remaining[..open_start];
        line_offset += before.chars().filter(|&c| c == '\n').count();

        let after_open = &remaining[open_start + 1..];
        let Some(open_end) = after_open.find('>') else {
            break;
        };

        let tag_content = &after_open[..open_end];
        let tag_name = tag_content.split_whitespace().next().unwrap_or("");

        if tag_name.starts_with('/') || tag_name.is_empty() {
            remaining = &after_open[open_end + 1..];
            continue;
        }

        if !known_tags.contains(&tag_name) {
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

        blocks.insert(tag_name.to_string(), body_source[..close_pos].to_string());

        let skip = body_start + close_pos + close_tag.len();
        line_offset += remaining[..skip].chars().filter(|&c| c == '\n').count();
        remaining = &remaining[skip..];
    }

    Ok(blocks)
}

fn parse_template(source: &str) -> Result<TemplateBlock, ParseError> {
    let wrapped = format!("<mesh-root>{}</mesh-root>", source.trim());
    let mut reader = Reader::from_str(&wrapped);
    reader.config_mut().trim_text(false);

    let mut stack: Vec<OpenNode> = Vec::new();
    let mut root = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let tag = decode_name(event.name().as_ref());
                if tag == "mesh-root" {
                    continue;
                }
                let attrs = parse_xml_attributes(&reader, &event)?;
                stack.push(OpenNode {
                    tag,
                    attributes: attrs,
                    children: Vec::new(),
                });
            }
            Ok(Event::Empty(event)) => {
                let tag = decode_name(event.name().as_ref());
                if tag == "mesh-root" {
                    continue;
                }
                let attrs = parse_xml_attributes(&reader, &event)?;
                let node = build_template_node(tag, attrs, Vec::new());
                push_template_node(&mut stack, &mut root, node);
            }
            Ok(Event::Text(event)) => {
                let text = event
                    .xml_content()
                    .map_err(|err| ParseError::InvalidTemplate {
                        message: err.to_string(),
                    })?
                    .into_owned();
                for node in parse_inline_nodes(&text) {
                    push_template_node(&mut stack, &mut root, node);
                }
            }
            Ok(Event::CData(event)) => {
                let text = event
                    .xml_content()
                    .map_err(|err| ParseError::InvalidTemplate {
                        message: err.to_string(),
                    })?
                    .into_owned();
                for node in parse_inline_nodes(&text) {
                    push_template_node(&mut stack, &mut root, node);
                }
            }
            Ok(Event::End(event)) => {
                let tag = decode_name(event.name().as_ref());
                if tag == "mesh-root" {
                    break;
                }

                let open = stack.pop().ok_or_else(|| ParseError::UnexpectedClose {
                    tag: tag.clone(),
                    line: 0,
                })?;

                if open.tag != tag {
                    return Err(ParseError::UnexpectedClose { tag, line: 0 });
                }

                let node = build_template_node(open.tag, open.attributes, open.children);
                push_template_node(&mut stack, &mut root, node);
            }
            Ok(Event::Eof) => break,
            Ok(Event::Comment(_)) | Ok(Event::Decl(_)) | Ok(Event::PI(_)) | Ok(Event::DocType(_)) | Ok(Event::GeneralRef(_)) => {}
            Err(err) => {
                return Err(ParseError::InvalidTemplate {
                    message: err.to_string(),
                });
            }
        }
    }

    if let Some(open) = stack.pop() {
        return Err(ParseError::UnclosedBlock {
            tag: open.tag,
            line: 0,
        });
    }

    Ok(TemplateBlock { root })
}

fn parse_xml_attributes(
    reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<Vec<Attribute>, ParseError> {
    let mut attrs = Vec::new();

    for attr in event.attributes().with_checks(false) {
        let attr = attr.map_err(|err| ParseError::InvalidTemplate {
            message: err.to_string(),
        })?;
        let name = decode_name(attr.key.as_ref());
        let value = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(|err| ParseError::InvalidTemplate {
                message: err.to_string(),
            })?
            .into_owned();

        let (attr_name, attr_value) = if let Some(stripped) = name.strip_prefix(':') {
            (stripped.to_string(), AttributeValue::Binding(value))
        } else if let Some(stripped) = name.strip_prefix('@') {
            (stripped.to_string(), AttributeValue::EventHandler(value))
        } else {
            (name, AttributeValue::Static(value))
        };

        attrs.push(Attribute {
            name: attr_name,
            value: attr_value,
        });
    }

    Ok(attrs)
}

fn parse_inline_nodes(text: &str) -> Vec<TemplateNode> {
    let mut nodes = Vec::new();
    let mut remaining = text;

    while let Some(start) = remaining.find("{{") {
        let prefix = &remaining[..start];
        if !prefix.trim().is_empty() {
            nodes.push(TemplateNode::Text(TextNode {
                content: prefix.trim().to_string(),
            }));
        }

        let expr_body = &remaining[start + 2..];
        if let Some(end) = expr_body.find("}}") {
            let expr = expr_body[..end].trim();
            if !expr.is_empty() {
                nodes.push(TemplateNode::Expr(ExprNode {
                    expression: expr.to_string(),
                }));
            }
            remaining = &expr_body[end + 2..];
        } else {
            remaining = &remaining[start..];
            break;
        }
    }

    if !remaining.trim().is_empty() {
        nodes.push(TemplateNode::Text(TextNode {
            content: remaining.trim().to_string(),
        }));
    }

    nodes
}

fn build_template_node(tag: String, attributes: Vec<Attribute>, children: Vec<TemplateNode>) -> TemplateNode {
    if tag.chars().next().is_some_and(char::is_uppercase) {
        return TemplateNode::Component(ComponentRef {
            name: tag,
            props: attributes,
            children,
        });
    }

    TemplateNode::Element(ElementNode {
        tag,
        attributes,
        children,
    })
}

fn push_template_node(
    stack: &mut [OpenNode],
    root: &mut Vec<TemplateNode>,
    node: TemplateNode,
) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else {
        root.push(node);
    }
}

fn decode_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}

struct OpenNode {
    tag: String,
    attributes: Vec<Attribute>,
    children: Vec<TemplateNode>,
}

fn parse_script(source: &str, blocks: &HashMap<String, String>) -> ScriptBlock {
    let _ = blocks;
    ScriptBlock {
        lang: ScriptLang::Luau,
        source: source.to_string(),
    }
}

fn parse_style(source: &str) -> Result<StyleBlock, ParseError> {
    let mut rules = Vec::new();
    let mut remaining = source.trim();

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        let Some(brace_start) = remaining.find('{') else {
            break;
        };
        let selector_str = remaining[..brace_start].trim();
        let selector = parse_selector(selector_str)?;

        let after_brace = &remaining[brace_start + 1..];
        let Some(brace_end) = after_brace.find('}') else {
            return Err(ParseError::InvalidStyle {
                message: "unclosed rule block".to_string(),
                line: 0,
            });
        };

        let declarations_str = &after_brace[..brace_end];
        let declarations = parse_declarations(declarations_str)?;

        rules.push(StyleRule {
            selector,
            declarations,
        });

        remaining = &after_brace[brace_end + 1..];
    }

    Ok(StyleBlock { rules })
}

fn parse_selector(source: &str) -> Result<Selector, ParseError> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut parts = Vec::new();

    while let Ok(token) = parser.next() {
        match token {
            Token::Delim('*') => parts.push(Selector::Universal),
            Token::Delim('.') => {
                let class = parser.expect_ident_cloned().map_err(|err| ParseError::InvalidStyle {
                    message: format!("{err:?}"),
                    line: 0,
                })?;
                parts.push(Selector::Class(class.to_string()));
            }
            Token::IDHash(id) => parts.push(Selector::Id(id.to_string())),
            Token::Colon => {
                let state = parser.expect_ident_cloned().map_err(|err| ParseError::InvalidStyle {
                    message: format!("{err:?}"),
                    line: 0,
                })?;
                match parts.pop() {
                    Some(Selector::Tag(tag)) => {
                        parts.push(Selector::State(tag, state.to_string()));
                    }
                    Some(previous) => {
                        parts.push(previous);
                        parts.push(Selector::State("*".into(), state.to_string()));
                    }
                    None => parts.push(Selector::State("*".into(), state.to_string())),
                }
            }
            Token::Ident(tag) => parts.push(Selector::Tag(tag.to_string())),
            Token::WhiteSpace(_) => {}
            other => {
                return Err(ParseError::InvalidStyle {
                    message: format!("unsupported selector token {}", other.to_css_string()),
                    line: 0,
                });
            }
        }
    }

    if parts.is_empty() {
        return Err(ParseError::InvalidStyle {
            message: "empty selector".into(),
            line: 0,
        });
    }

    if parts.len() == 1 {
        Ok(parts.remove(0))
    } else {
        Ok(Selector::Compound(parts))
    }
}

fn parse_declarations(source: &str) -> Result<Vec<Declaration>, ParseError> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut declarations = Vec::new();

    while !parser.is_exhausted() {
        parser.skip_whitespace();
        if parser.is_exhausted() {
            break;
        }

        let property = parser.expect_ident_cloned().map_err(|err| ParseError::InvalidStyle {
            message: format!("{err:?}"),
            line: 0,
        })?;
        parser.expect_colon().map_err(|err| ParseError::InvalidStyle {
            message: format!("{err:?}"),
            line: 0,
        })?;

        let raw_value = parser
            .parse_until_before(cssparser::Delimiter::Bang | cssparser::Delimiter::Semicolon, |input| {
                serialize_css_value(input)
            })
            .map_err(|err| ParseError::InvalidStyle {
                message: format!("{err:?}"),
                line: 0,
            })?;

        let _ = parser.try_parse(|input| {
            input.expect_delim('!')?;
            input.expect_ident_matching("important")
        });
        let _ = parser.try_parse(|input| input.expect_semicolon());

        declarations.push(Declaration {
            property: property.to_string(),
            value: classify_style_value(&raw_value),
        });
    }

    Ok(declarations)
}

fn classify_style_value(value: &str) -> StyleValue {
    let value = value.trim();
    if value.starts_with("token(") && value.ends_with(')') {
        StyleValue::Token(value[6..value.len() - 1].trim().to_string())
    } else if value.starts_with("var(") && value.ends_with(')') {
        StyleValue::Var(value[4..value.len() - 1].trim().to_string())
    } else {
        StyleValue::Literal(value.to_string())
    }
}

fn serialize_css_value<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<String, cssparser::ParseError<'i, ()>> {
    let mut rendered = String::new();

    while let Ok(token) = input.next_including_whitespace_and_comments() {
        match token {
            Token::Function(name) => {
                rendered.push_str(name);
                rendered.push('(');
                let inner = input.parse_nested_block(serialize_css_value)?;
                rendered.push_str(&inner);
                rendered.push(')');
            }
            Token::ParenthesisBlock => {
                rendered.push('(');
                let inner = input.parse_nested_block(serialize_css_value)?;
                rendered.push_str(&inner);
                rendered.push(')');
            }
            Token::SquareBracketBlock => {
                rendered.push('[');
                let inner = input.parse_nested_block(serialize_css_value)?;
                rendered.push_str(&inner);
                rendered.push(']');
            }
            Token::CurlyBracketBlock => {
                rendered.push('{');
                let inner = input.parse_nested_block(serialize_css_value)?;
                rendered.push_str(&inner);
                rendered.push('}');
            }
            other => rendered.push_str(&other.to_css_string()),
        }
    }

    Ok(rendered.trim().to_string())
}

fn parse_schema(source: &str) -> Result<SchemaBlock, ParseError> {
    let block: SchemaBlock = toml::from_str(source)?;
    Ok(block)
}

fn parse_i18n(source: &str) -> Result<I18nBlock, ParseError> {
    let raw: HashMap<String, HashMap<String, String>> =
        toml::from_str(source).map_err(|e| ParseError::InvalidI18n(e.to_string()))?;
    Ok(I18nBlock { entries: raw })
}

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

        let tmpl = file.template.unwrap();
        assert_eq!(tmpl.root.len(), 1);

        let script = file.script.unwrap();
        assert_eq!(script.lang, ScriptLang::Luau);
        assert!(script.source.contains("function onTap"));

        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 2);
        match &style.rules[0].declarations[0].value {
            StyleValue::Token(name) => assert_eq!(name, "color.on-surface"),
            other => panic!("expected token, got {other:?}"),
        }

        let schema = file.schema.unwrap();
        assert!(schema.fields.contains_key("greeting"));

        let i18n = file.i18n.unwrap();
        assert_eq!(i18n.entries["en"]["greeting"], "Hello");
        assert_eq!(i18n.entries["fr"]["greeting"], "Bonjour");

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
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "text");
                assert!(!el.children.is_empty());
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
