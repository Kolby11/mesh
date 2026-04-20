/// Parser for `.mesh` single-file components.
///
/// Splits the source into top-level blocks (`<template>`, `<script>`, `<style>`,
/// `<schema>`, `<i18n>`, `<meta>`) then parses each block with parser libraries.
use crate::{
    ComponentFile, ScriptBlock, ScriptLang,
    meta::MetaBlock,
    schema::SchemaBlock,
    style::{ContainerQuery, Declaration, Selector, StyleBlock, StyleRule, StyleValue},
    template::*,
};
use cssparser::{Parser, ParserInput, ToCss, Token};
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

    let style = blocks.get("style").map(|s| parse_style(s)).transpose()?;

    let schema = blocks.get("schema").map(|s| parse_schema(s)).transpose()?;

    let meta = blocks.get("meta").map(|s| parse_meta(s)).transpose()?;

    Ok(ComponentFile {
        template,
        script,
        style,
        schema,
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
            Ok(Event::Comment(_))
            | Ok(Event::Decl(_))
            | Ok(Event::PI(_))
            | Ok(Event::DocType(_))
            | Ok(Event::GeneralRef(_)) => {}
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

    while !remaining.is_empty() {
        // Find the next `{` — could be `{{expr}}` or `{expr}`.
        let Some(start) = remaining.find('{') else {
            break;
        };

        let prefix = &remaining[..start];
        if !prefix.trim().is_empty() {
            nodes.push(TemplateNode::Text(TextNode {
                content: prefix.trim().to_string(),
            }));
        }

        let after_brace = &remaining[start + 1..];

        if after_brace.starts_with('{') {
            // `{{expr}}` — double-brace form
            let expr_body = &after_brace[1..];
            if let Some(end) = expr_body.find("}}") {
                let expr = expr_body[..end].trim();
                if !expr.is_empty() {
                    nodes.push(TemplateNode::Expr(ExprNode {
                        expression: expr.to_string(),
                    }));
                }
                remaining = &expr_body[end + 2..];
            } else {
                // Unclosed `{{` — emit as literal and stop processing
                nodes.push(TemplateNode::Text(TextNode {
                    content: remaining[start..].to_string(),
                }));
                remaining = "";
            }
        } else {
            // `{expr}` — single-brace form; find the matching `}`
            // respecting nested parens so `{t(a.b)}` works correctly.
            let expr_body = after_brace;
            if let Some(end) = find_closing_brace(expr_body) {
                let expr = expr_body[..end].trim();
                if !expr.is_empty() {
                    nodes.push(TemplateNode::Expr(ExprNode {
                        expression: expr.to_string(),
                    }));
                }
                remaining = &expr_body[end + 1..];
            } else {
                // Unclosed `{` — emit as literal and stop
                nodes.push(TemplateNode::Text(TextNode {
                    content: remaining[start..].to_string(),
                }));
                remaining = "";
            }
        }
    }

    if !remaining.trim().is_empty() {
        nodes.push(TemplateNode::Text(TextNode {
            content: remaining.trim().to_string(),
        }));
    }

    nodes
}

/// Find the index of the `}` that closes the expression, respecting nested
/// parentheses and string literals so `t(a.b)` and `t("key")` are handled.
fn find_closing_brace(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut string_char = '\0';
    let mut chars = s.char_indices();

    while let Some((i, ch)) = chars.next() {
        if in_string {
            if ch == string_char {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_string = true;
                string_char = ch;
            }
            '(' | '[' => depth += 1,
            ')' | ']' => {
                if depth == 0 {
                    return None; // unbalanced
                }
                depth -= 1;
            }
            '}' if depth == 0 => return Some(i),
            _ => {}
        }
    }

    None
}

fn build_template_node(
    tag: String,
    attributes: Vec<Attribute>,
    children: Vec<TemplateNode>,
) -> TemplateNode {
    if tag == "slot" {
        let name = attributes.iter().find_map(|attribute| {
            if attribute.name != "name" {
                return None;
            }

            match &attribute.value {
                AttributeValue::Static(value) => Some(value.clone()),
                _ => None,
            }
        });

        return TemplateNode::Slot(SlotNode { name });
    }

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

fn push_template_node(stack: &mut [OpenNode], root: &mut Vec<TemplateNode>, node: TemplateNode) {
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
    parse_style_rules(source, None, &mut rules)?;

    Ok(StyleBlock { rules })
}

fn parse_style_rules(
    source: &str,
    inherited_query: Option<ContainerQuery>,
    rules: &mut Vec<StyleRule>,
) -> Result<(), ParseError> {
    let mut remaining = source.trim();

    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        let (header, body, rest) = take_style_block(remaining)?;
        if header.starts_with("@container") {
            let query = parse_container_query(header)?;
            let combined_query = inherited_query
                .map(|existing| existing.intersect(query))
                .or(Some(query));
            parse_style_rules(body, combined_query, rules)?;
        } else if header.starts_with('@') {
            return Err(ParseError::InvalidStyle {
                message: format!("unsupported at-rule '{header}'"),
                line: 0,
            });
        } else {
            let selector = parse_selector(header)?;
            let declarations = parse_declarations(body)?;
            rules.push(StyleRule {
                selector,
                declarations,
                container_query: inherited_query,
            });
        }

        remaining = rest;
    }

    Ok(())
}

fn take_style_block(source: &str) -> Result<(&str, &str, &str), ParseError> {
    let Some(brace_start) = source.find('{') else {
        return Err(ParseError::InvalidStyle {
            message: "expected '{' in style block".to_string(),
            line: 0,
        });
    };
    let Some(brace_end) = find_matching_delimiter(source, brace_start, '{', '}') else {
        return Err(ParseError::InvalidStyle {
            message: "unclosed rule block".to_string(),
            line: 0,
        });
    };

    Ok((
        source[..brace_start].trim(),
        &source[brace_start + 1..brace_end],
        &source[brace_end + 1..],
    ))
}

fn find_matching_delimiter(source: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut string_char = '\0';
    let mut escaped = false;

    for (index, ch) in source
        .char_indices()
        .skip_while(|(index, _)| *index < start)
    {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                _ if ch == string_char => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                in_string = true;
                string_char = ch;
            }
            _ if ch == open => depth += 1,
            _ if ch == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }

    None
}

fn parse_container_query(source: &str) -> Result<ContainerQuery, ParseError> {
    let mut query = ContainerQuery::default();
    let mut found_clause = false;
    let mut remaining = source
        .trim()
        .strip_prefix("@container")
        .ok_or_else(|| ParseError::InvalidStyle {
            message: format!("invalid container query '{source}'"),
            line: 0,
        })?
        .trim();

    while let Some(start) = remaining.find('(') {
        let Some(end) = find_matching_delimiter(remaining, start, '(', ')') else {
            return Err(ParseError::InvalidStyle {
                message: format!("unclosed container query '{source}'"),
                line: 0,
            });
        };
        let clause = remaining[start + 1..end].trim();
        apply_container_clause(&mut query, clause)?;
        found_clause = true;
        remaining = remaining[end + 1..].trim();
    }

    if !found_clause {
        return Err(ParseError::InvalidStyle {
            message: format!("container query '{source}' is missing a condition"),
            line: 0,
        });
    }

    Ok(query)
}

fn apply_container_clause(query: &mut ContainerQuery, clause: &str) -> Result<(), ParseError> {
    let (property, value) = clause
        .split_once(':')
        .ok_or_else(|| ParseError::InvalidStyle {
            message: format!("invalid container clause '{clause}'"),
            line: 0,
        })?;
    let parsed_value = parse_style_length(value)?;

    match property.trim().to_ascii_lowercase().as_str() {
        "min-width" => query.min_width = Some(parsed_value),
        "max-width" => query.max_width = Some(parsed_value),
        "min-height" => query.min_height = Some(parsed_value),
        "max-height" => query.max_height = Some(parsed_value),
        other => {
            return Err(ParseError::InvalidStyle {
                message: format!("unsupported container query property '{other}'"),
                line: 0,
            });
        }
    }

    Ok(())
}

fn parse_style_length(value: &str) -> Result<f32, ParseError> {
    value
        .trim()
        .trim_end_matches("px")
        .parse::<f32>()
        .map_err(|_| ParseError::InvalidStyle {
            message: format!("invalid style length '{value}'"),
            line: 0,
        })
}

fn parse_selector(source: &str) -> Result<Selector, ParseError> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut parts = Vec::new();

    while let Ok(token) = parser.next() {
        match token {
            Token::Delim('*') => parts.push(Selector::Universal),
            Token::Delim('.') => {
                let class =
                    parser
                        .expect_ident_cloned()
                        .map_err(|err| ParseError::InvalidStyle {
                            message: format!("{err:?}"),
                            line: 0,
                        })?;
                parts.push(Selector::Class(class.to_string()));
            }
            Token::IDHash(id) => parts.push(Selector::Id(id.to_string())),
            Token::Colon => {
                let state =
                    parser
                        .expect_ident_cloned()
                        .map_err(|err| ParseError::InvalidStyle {
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

        let property = parser
            .expect_ident_cloned()
            .map_err(|err| ParseError::InvalidStyle {
                message: format!("{err:?}"),
                line: 0,
            })?;
        parser
            .expect_colon()
            .map_err(|err| ParseError::InvalidStyle {
                message: format!("{err:?}"),
                line: 0,
            })?;

        let raw_value = parser
            .parse_until_before(
                cssparser::Delimiter::Bang | cssparser::Delimiter::Semicolon,
                |input| serialize_css_value(input),
            )
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
        assert!(style.rules[0].container_query.is_none());
    }

    #[test]
    fn parse_container_query_rules() {
        let source = r#"
<style>
@container (max-width: 640px) {
    .sidebar {
        width: 100%;
        overflow-y: auto;
    }
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 1);
        let rule = &style.rules[0];
        assert_eq!(
            rule.container_query,
            Some(ContainerQuery {
                max_width: Some(640.0),
                ..Default::default()
            })
        );
        assert!(matches!(&rule.selector, Selector::Class(name) if name == "sidebar"));
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
                assert!(
                    matches!(&el.attributes[0].value, AttributeValue::Binding(v) if v == "volume")
                );
                assert!(
                    matches!(&el.attributes[1].value, AttributeValue::EventHandler(v) if v == "onVolumeChange")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_named_slot() {
        let source = r#"
<template>
  <column>
    <slot name="sidebar"/>
  </column>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => match &el.children[0] {
                TemplateNode::Slot(slot) => assert_eq!(slot.name.as_deref(), Some("sidebar")),
                other => panic!("expected slot node, got {other:?}"),
            },
            other => panic!("expected element node, got {other:?}"),
        }
    }
}
