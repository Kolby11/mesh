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
use cssparser::{Parser, ParserInput, ToCss as CssParserToCss, Token};
use lightningcss::{
    media_query::{
        MediaFeatureComparison, MediaFeatureName, MediaFeatureValue, Operator,
        QueryFeature as LightningQueryFeature,
    },
    rules::container::{ContainerCondition, ContainerSizeFeature, ContainerSizeFeatureId},
    rules::{CssRule as LightningCssRule, style::StyleRule as LightningStyleRule},
    stylesheet::{ParserOptions as CssParserOptions, PrinterOptions, StyleSheet},
    traits::ToCss as LightningToCss,
};
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

    let (imports, script) = if let Some(s) = blocks.get("script") {
        let (imports, stripped) = extract_imports(s);
        (imports, Some(parse_script(&stripped, &blocks)))
    } else {
        (HashMap::new(), None)
    };

    let style = blocks.get("style").map(|s| parse_style(s)).transpose()?;

    let schema = blocks.get("schema").map(|s| parse_schema(s)).transpose()?;

    let meta = blocks.get("meta").map(|s| parse_meta(s)).transpose()?;

    Ok(ComponentFile {
        imports,
        template,
        script,
        style,
        schema,
        meta,
    })
}

/// Extract `import "plugin-id" as Alias` lines from the script source.
/// Returns (imports map, script with those lines removed).
fn extract_imports(source: &str) -> (HashMap<String, String>, String) {
    let mut imports = HashMap::new();
    let mut stripped = String::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("import ") {
            // expect: import "<plugin-id>" as Alias
            if let Some((plugin_id, alias)) = parse_import_line(rest) {
                imports.insert(alias, plugin_id);
                stripped.push('\n'); // preserve line count for error messages
                continue;
            }
        }
        stripped.push_str(line);
        stripped.push('\n');
    }

    (imports, stripped)
}

fn parse_import_line(rest: &str) -> Option<(String, String)> {
    // Support two forms:
    // 1) import "@mesh/foo" as Alias
    // 2) import Alias from "@mesh/foo"
    let rest = rest.trim();

    // form 1: starts with quoted plugin id
    if rest.starts_with('"') || rest.starts_with('\'') {
        let quote = rest.chars().next().unwrap();
        let end = rest[1..].find(quote)?;
        let plugin_id = rest[1..end + 1].to_string();
        let after_id = rest[plugin_id.len() + 2..].trim();
        let alias = after_id.strip_prefix("as ")?.trim().to_string();
        if alias.is_empty() || alias.contains(|c: char| !c.is_alphanumeric() && c != '_') {
            return None;
        }
        return Some((plugin_id, alias));
    }

    // form 2: import Alias from "plugin-id"
    // expect: <Alias> from "<plugin-id>"
    let parts: Vec<&str> = rest.splitn(2, " from ").collect();
    if parts.len() == 2 {
        let alias = parts[0].trim();
        if alias.is_empty() || alias.contains(|c: char| !c.is_alphanumeric() && c != '_') {
            return None;
        }
        let rhs = parts[1].trim();
        if !(rhs.starts_with('"') || rhs.starts_with('\'')) {
            return None;
        }
        let quote = rhs.chars().next().unwrap();
        let end = rhs[1..].find(quote)?;
        let plugin_id = rhs[1..end + 1].to_string();
        return Some((plugin_id, alias.to_string()));
    }

    None
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

/// Convert unquoted brace attribute values to quoted form so quick_xml can parse them.
///
/// `onclick={handler}` → `onclick="{handler}"`
/// `value="{expr}"` is left unchanged (already quoted).
fn preprocess_template(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 32);
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut in_tag = false;
    let mut in_quoted = false;
    let mut quote_char = b'"';

    while i < len {
        let b = bytes[i];

        if !in_tag {
            if b == b'<' {
                in_tag = true;
            }
            out.push(b as char);
            i += 1;
        } else if in_quoted {
            if b == quote_char {
                in_quoted = false;
            }
            out.push(b as char);
            i += 1;
        } else if b == b'"' || b == b'\'' {
            in_quoted = true;
            quote_char = b;
            out.push(b as char);
            i += 1;
        } else if b == b'>' {
            in_tag = false;
            out.push(b as char);
            i += 1;
        } else if b == b'=' && i + 1 < len && bytes[i + 1] == b'{' {
            // Unquoted brace value: wrap it.
            out.push('=');
            out.push('"');
            i += 1; // skip '=', now pointing at '{'
            let mut depth: i32 = 0;
            while i < len {
                let c = bytes[i] as char;
                out.push(c);
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            out.push('"');
        } else {
            out.push(b as char);
            i += 1;
        }
    }

    out
}

fn parse_template(source: &str) -> Result<TemplateBlock, ParseError> {
    let preprocessed = preprocess_template(source.trim());
    let wrapped = format!("<mesh-root>{}</mesh-root>", preprocessed);
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

        let (attr_name, attr_value) = if let Some(var) = name.strip_prefix("bind:") {
            // bind:value="variable" — two-way binding.
            (var.to_string(), AttributeValue::TwoWayBinding(value))
        } else if is_event_attr(&name) {
            // onclick="handler" or onclick="{handler}" — strip braces if present.
            let handler = extract_brace_expr(&value).unwrap_or(value);
            (name, AttributeValue::EventHandler(handler))
        } else if let Some(expr) = extract_brace_expr(&value) {
            // title="{expr}" — dynamic binding, expression inside braces.
            (name, AttributeValue::Binding(expr))
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

/// Returns true if the attribute name is an HTML event handler (`onclick`, `oninput`, etc.).
fn is_event_attr(name: &str) -> bool {
    name.len() > 2 && name.starts_with("on") && name[2..].chars().all(|c| c.is_ascii_alphabetic())
}

/// If `value` is exactly `{expr}`, returns the inner expression; otherwise `None`.
fn extract_brace_expr(value: &str) -> Option<String> {
    if value.starts_with('{') && value.ends_with('}') && value.len() >= 2 {
        Some(value[1..value.len() - 1].trim().to_string())
    } else {
        None
    }
}

fn parse_inline_nodes(text: &str) -> Vec<TemplateNode> {
    let mut nodes = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the next `{expr}` expression.
        let Some(start) = remaining.find('{') else {
            break;
        };

        let prefix = &remaining[..start];
        if !prefix.trim().is_empty() {
            nodes.push(TemplateNode::Text(TextNode {
                content: prefix.trim().to_string(),
            }));
        }

        // `{expr}` — find the matching `}` respecting nested parens so `{t(a.b)}` works.
        let expr_body = &remaining[start + 1..];
        if let Some(end) = find_closing_brace(expr_body) {
            let expr = expr_body[..end].trim();
            if !expr.is_empty() {
                nodes.push(TemplateNode::Expr(ExprNode {
                    expression: expr.to_string(),
                }));
            }
            remaining = &expr_body[end + 1..];
        } else {
            // Unclosed `{` — emit as literal and stop.
            nodes.push(TemplateNode::Text(TextNode {
                content: remaining[start..].to_string(),
            }));
            remaining = "";
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
    let stylesheet = StyleSheet::parse(
        source,
        CssParserOptions {
            filename: "<style>".into(),
            error_recovery: false,
            ..CssParserOptions::default()
        },
    )
    .map_err(map_lightning_error)?;

    let mut rules = Vec::new();
    lower_css_rules(&stylesheet.rules.0, None, &mut rules)?;
    Ok(StyleBlock { rules })
}

fn lower_css_rules(
    source_rules: &[LightningCssRule<'_>],
    inherited_query: Option<ContainerQuery>,
    rules: &mut Vec<StyleRule>,
) -> Result<(), ParseError> {
    for rule in source_rules {
        match rule {
            LightningCssRule::Style(style_rule) => {
                lower_style_rule(style_rule, inherited_query, rules)?;
            }
            LightningCssRule::Container(container_rule) => {
                let query = lower_container_query(container_rule)?;
                let combined_query = inherited_query
                    .map(|existing| existing.intersect(query))
                    .or(Some(query));
                lower_css_rules(&container_rule.rules.0, combined_query, rules)?;
            }
            LightningCssRule::Ignored => {}
            other => {
                return Err(ParseError::InvalidStyle {
                    message: format!("unsupported at-rule '{}'", css_rule_name(other)),
                    line: 0,
                });
            }
        }
    }

    Ok(())
}

fn lower_style_rule(
    source_rule: &LightningStyleRule<'_>,
    inherited_query: Option<ContainerQuery>,
    rules: &mut Vec<StyleRule>,
) -> Result<(), ParseError> {
    if !source_rule.rules.0.is_empty() {
        return Err(ParseError::InvalidStyle {
            message: "nested style rules are not supported".into(),
            line: 0,
        });
    }

    let declarations = lower_declarations(&source_rule.declarations)?;
    for selector in &source_rule.selectors.0 {
        let selector_source = selector
            .to_css_string(PrinterOptions::default())
            .map_err(map_lightning_printer_error)?;
        let selector = parse_selector(&selector_source)?;
        rules.push(StyleRule {
            selector,
            declarations: declarations.clone(),
            container_query: inherited_query,
        });
    }

    Ok(())
}

fn lower_declarations(
    source_block: &lightningcss::declaration::DeclarationBlock<'_>,
) -> Result<Vec<Declaration>, ParseError> {
    let mut declarations = Vec::new();

    for property in &source_block.declarations {
        declarations.push(lower_property(property)?);
    }
    for property in &source_block.important_declarations {
        declarations.push(lower_property(property)?);
    }

    Ok(declarations)
}

fn lower_property(
    property: &lightningcss::properties::Property<'_>,
) -> Result<Declaration, ParseError> {
    let property_name = property.property_id().name().to_string();
    let value = property
        .value_to_css_string(PrinterOptions::default())
        .map_err(map_lightning_printer_error)?;

    Ok(Declaration {
        property: property_name,
        value: classify_style_value(&value),
    })
}

fn lower_container_query(
    source_rule: &lightningcss::rules::container::ContainerRule<'_>,
) -> Result<ContainerQuery, ParseError> {
    let Some(condition) = &source_rule.condition else {
        return Err(ParseError::InvalidStyle {
            message: "container query is missing a condition".into(),
            line: 0,
        });
    };

    lower_container_condition(condition)
}

fn css_rule_name(rule: &LightningCssRule<'_>) -> &'static str {
    match rule {
        LightningCssRule::Media(_) => "@media",
        LightningCssRule::Import(_) => "@import",
        LightningCssRule::Style(_) => "style",
        LightningCssRule::Keyframes(_) => "@keyframes",
        LightningCssRule::FontFace(_) => "@font-face",
        LightningCssRule::FontPaletteValues(_) => "@font-palette-values",
        LightningCssRule::FontFeatureValues(_) => "@font-feature-values",
        LightningCssRule::Page(_) => "@page",
        LightningCssRule::Supports(_) => "@supports",
        LightningCssRule::CounterStyle(_) => "@counter-style",
        LightningCssRule::Namespace(_) => "@namespace",
        LightningCssRule::MozDocument(_) => "@-moz-document",
        LightningCssRule::Nesting(_) => "@nest",
        LightningCssRule::NestedDeclarations(_) => "nested declarations",
        LightningCssRule::Viewport(_) => "@viewport",
        LightningCssRule::CustomMedia(_) => "@custom-media",
        LightningCssRule::LayerStatement(_) => "@layer",
        LightningCssRule::LayerBlock(_) => "@layer",
        LightningCssRule::Property(_) => "@property",
        LightningCssRule::Container(_) => "@container",
        LightningCssRule::Scope(_) => "@scope",
        LightningCssRule::StartingStyle(_) => "@starting-style",
        LightningCssRule::ViewTransition(_) => "@view-transition",
        LightningCssRule::Ignored => "ignored rule",
        LightningCssRule::Unknown(_) => "unknown at-rule",
        LightningCssRule::Custom(_) => "custom at-rule",
    }
}

fn map_lightning_error<T: std::fmt::Display>(err: lightningcss::error::Error<T>) -> ParseError {
    ParseError::InvalidStyle {
        message: err.kind.to_string(),
        line: err.loc.map(|loc| loc.line as usize + 1).unwrap_or(0),
    }
}

fn map_lightning_printer_error(err: lightningcss::error::PrinterError) -> ParseError {
    ParseError::InvalidStyle {
        message: err.to_string(),
        line: 0,
    }
}

fn lower_container_condition(
    condition: &ContainerCondition<'_>,
) -> Result<ContainerQuery, ParseError> {
    match condition {
        ContainerCondition::Feature(feature) => lower_container_feature(feature),
        ContainerCondition::Operation {
            operator: Operator::And,
            conditions,
        } => {
            let mut query = ContainerQuery::default();
            for condition in conditions {
                query = query.intersect(lower_container_condition(condition)?);
            }
            Ok(query)
        }
        ContainerCondition::Operation {
            operator: Operator::Or,
            ..
        } => Err(ParseError::InvalidStyle {
            message: "container queries with 'or' are not supported".into(),
            line: 0,
        }),
        ContainerCondition::Not(_) => Err(ParseError::InvalidStyle {
            message: "negated container queries are not supported".into(),
            line: 0,
        }),
        ContainerCondition::Style(_) => Err(ParseError::InvalidStyle {
            message: "style container queries are not supported".into(),
            line: 0,
        }),
        ContainerCondition::ScrollState(_) => Err(ParseError::InvalidStyle {
            message: "scroll-state container queries are not supported".into(),
            line: 0,
        }),
        ContainerCondition::Unknown(_) => Err(ParseError::InvalidStyle {
            message: "unsupported container query condition".into(),
            line: 0,
        }),
    }
}

fn lower_container_feature(
    feature: &ContainerSizeFeature<'_>,
) -> Result<ContainerQuery, ParseError> {
    match feature {
        LightningQueryFeature::Plain { name, value } => {
            let axis = container_feature_axis(name)?;
            let value = container_feature_length(value)?;
            let mut query = ContainerQuery::default();
            apply_container_bound(&mut query, axis, MediaFeatureComparison::Equal, value);
            Ok(query)
        }
        LightningQueryFeature::Range {
            name,
            operator,
            value,
        } => {
            let axis = container_feature_axis(name)?;
            let value = container_feature_length(value)?;
            let mut query = ContainerQuery::default();
            apply_container_bound(&mut query, axis, *operator, value);
            Ok(query)
        }
        LightningQueryFeature::Interval {
            name,
            start,
            start_operator,
            end,
            end_operator,
        } => {
            let axis = container_feature_axis(name)?;
            let start = container_feature_length(start)?;
            let end = container_feature_length(end)?;
            let mut query = ContainerQuery::default();
            apply_container_bound(&mut query, axis, invert_comparison(*start_operator), start);
            apply_container_bound(&mut query, axis, *end_operator, end);
            Ok(query)
        }
        LightningQueryFeature::Boolean { .. } => Err(ParseError::InvalidStyle {
            message: "boolean container queries are not supported".into(),
            line: 0,
        }),
    }
}

fn container_feature_axis(
    name: &MediaFeatureName<'_, ContainerSizeFeatureId>,
) -> Result<ContainerAxis, ParseError> {
    match name {
        MediaFeatureName::Standard(ContainerSizeFeatureId::Width)
        | MediaFeatureName::Standard(ContainerSizeFeatureId::InlineSize) => {
            Ok(ContainerAxis::Width)
        }
        MediaFeatureName::Standard(ContainerSizeFeatureId::Height)
        | MediaFeatureName::Standard(ContainerSizeFeatureId::BlockSize) => {
            Ok(ContainerAxis::Height)
        }
        MediaFeatureName::Standard(other) => Err(ParseError::InvalidStyle {
            message: format!("unsupported container query property '{other:?}'"),
            line: 0,
        }),
        MediaFeatureName::Custom(_) | MediaFeatureName::Unknown(_) => {
            Err(ParseError::InvalidStyle {
                message: "custom container query properties are not supported".into(),
                line: 0,
            })
        }
    }
}

fn container_feature_length(value: &MediaFeatureValue<'_>) -> Result<f32, ParseError> {
    match value {
        MediaFeatureValue::Length(length) => {
            length.to_px().ok_or_else(|| ParseError::InvalidStyle {
                message: "container query length must be convertible to px".into(),
                line: 0,
            })
        }
        other => Err(ParseError::InvalidStyle {
            message: format!("unsupported container query value '{other:?}'"),
            line: 0,
        }),
    }
}

fn apply_container_bound(
    query: &mut ContainerQuery,
    axis: ContainerAxis,
    operator: MediaFeatureComparison,
    value: f32,
) {
    match (axis, operator) {
        (ContainerAxis::Width, MediaFeatureComparison::GreaterThan)
        | (ContainerAxis::Width, MediaFeatureComparison::GreaterThanEqual) => {
            query.min_width = Some(query.min_width.map_or(value, |current| current.max(value)));
        }
        (ContainerAxis::Width, MediaFeatureComparison::LessThan)
        | (ContainerAxis::Width, MediaFeatureComparison::LessThanEqual) => {
            query.max_width = Some(query.max_width.map_or(value, |current| current.min(value)));
        }
        (ContainerAxis::Width, MediaFeatureComparison::Equal) => {
            query.min_width = Some(query.min_width.map_or(value, |current| current.max(value)));
            query.max_width = Some(query.max_width.map_or(value, |current| current.min(value)));
        }
        (ContainerAxis::Height, MediaFeatureComparison::GreaterThan)
        | (ContainerAxis::Height, MediaFeatureComparison::GreaterThanEqual) => {
            query.min_height = Some(query.min_height.map_or(value, |current| current.max(value)));
        }
        (ContainerAxis::Height, MediaFeatureComparison::LessThan)
        | (ContainerAxis::Height, MediaFeatureComparison::LessThanEqual) => {
            query.max_height = Some(query.max_height.map_or(value, |current| current.min(value)));
        }
        (ContainerAxis::Height, MediaFeatureComparison::Equal) => {
            query.min_height = Some(query.min_height.map_or(value, |current| current.max(value)));
            query.max_height = Some(query.max_height.map_or(value, |current| current.min(value)));
        }
    }
}

fn invert_comparison(operator: MediaFeatureComparison) -> MediaFeatureComparison {
    match operator {
        MediaFeatureComparison::Equal => MediaFeatureComparison::Equal,
        MediaFeatureComparison::GreaterThan => MediaFeatureComparison::LessThan,
        MediaFeatureComparison::GreaterThanEqual => MediaFeatureComparison::LessThanEqual,
        MediaFeatureComparison::LessThan => MediaFeatureComparison::GreaterThan,
        MediaFeatureComparison::LessThanEqual => MediaFeatureComparison::GreaterThanEqual,
    }
}

#[derive(Clone, Copy)]
enum ContainerAxis {
    Width,
    Height,
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
  <span>Hello</span>
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
  <div>
    <span class="title">{ title }</span>
    <button onclick="onTap">Click me</button>
  </div>
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
  <span>Time: { formatTime(time) }</span>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert_eq!(el.tag, "span");
                assert!(!el.children.is_empty());
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_style_tokens_and_literals() {
        let source = r#"
<style>
div {
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
    fn parse_grouped_selectors_into_multiple_rules() {
        let source = r#"
<style>
.panel, #main {
    color: #fff;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        assert_eq!(style.rules.len(), 2);
        assert!(matches!(&style.rules[0].selector, Selector::Class(name) if name == "panel"));
        assert!(matches!(&style.rules[1].selector, Selector::Id(name) if name == "main"));
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
  <input value="{volume}" onchange="onVolumeChange"/>
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
    fn parse_unquoted_brace_event_attribute() {
        let source = r#"
<template>
  <button onclick={onTap}>Click</button>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(
                    matches!(&el.attributes[0].value, AttributeValue::EventHandler(v) if v == "onTap")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn script_source_passed_through_unchanged() {
        let source = r#"
<template>
  <span>{title}</span>
</template>

<script lang="luau">
mesh.state.set("title", "Hello")
mesh.state.set("count", 0)

function onTap()
    local tmp = count + 1
    count = tmp
end
</script>
"#;
        let file = parse_component(source).unwrap();
        let script = file.script.unwrap();
        assert!(script.source.contains("mesh.state.set(\"title\", \"Hello\")"));
        assert!(script.source.contains("local tmp = count + 1"));
    }

    #[test]
    fn local_declarations_preserved_verbatim() {
        let source = r#"
<script lang="luau">
local handler = function() end
local audio = mesh.interfaces.get("mesh.audio")
mesh.service.bind("audio.muted", "audio_muted")
mesh.service.on("audio", "sync_audio_state")
</script>
"#;
        let file = parse_component(source).unwrap();
        let script = file.script.unwrap();
        assert!(script.source.contains("local handler = function()"));
        assert!(script.source.contains("local audio = mesh.interfaces.get(\"mesh.audio\")"));
        assert!(script.source.contains("mesh.service.bind(\"audio.muted\", \"audio_muted\")"));
        assert!(script.source.contains("mesh.service.on(\"audio\", \"sync_audio_state\")"));
    }

    #[test]
    fn parse_two_way_binding() {
        let source = r#"
<template>
  <input type="text" bind:value="searchQuery"/>
</template>
"#;
        let file = parse_component(source).unwrap();
        let tmpl = file.template.unwrap();
        match &tmpl.root[0] {
            TemplateNode::Element(el) => {
                assert!(matches!(&el.attributes[0].value, AttributeValue::Static(_)));
                assert!(
                    matches!(&el.attributes[1].value, AttributeValue::TwoWayBinding(v) if v == "searchQuery")
                );
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn parse_named_slot() {
        let source = r#"
<template>
  <div>
    <slot name="sidebar"/>
  </div>
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

    #[test]
    fn padding_property_names_from_lightningcss() {
        // Verify which property names lightningcss emits for the padding variants
        // we use in .mesh components so apply_declaration handles them all.
        let source = r#"
<style>
.box {
    padding: 8px;
    padding-inline: 16px;
    padding-block: 12px;
    padding-top: 4px;
    padding-right: 5px;
    padding-bottom: 6px;
    padding-left: 7px;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let style = file.style.unwrap();
        let decls = &style.rules[0].declarations;
        let props: Vec<&str> = decls.iter().map(|d| d.property.as_str()).collect();
        // Confirm every declaration landed under a name our style resolver knows.
        let known = [
            "padding",
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
            "padding-inline",
            "padding-block",
            "padding-inline-start",
            "padding-inline-end",
            "padding-block-start",
            "padding-block-end",
        ];
        for p in &props {
            assert!(known.contains(p), "unrecognised padding property: {p}");
        }
        // Spot-check that the shorthand emitted individual sides (lightningcss expands it).
        eprintln!("padding properties emitted by lightningcss: {props:?}");
    }
}
