//! Cross-block validation for a parsed component contract.
//!
//! The individual component parsers intentionally stay focused on their own
//! languages. This pass is the point where the declarations in `<props>` are
//! joined to the style references that consume them.

use super::{ParseError, styles};
use crate::{Attribute, ComponentFile, PropDef, SourceSpan, TemplateNode};

type Declarations<'a> = std::collections::HashMap<&'a str, &'a PropDef>;

pub(super) fn validate(source: &str, component: &ComponentFile) -> Result<(), ParseError> {
    let declarations = component
        .props
        .as_ref()
        .map(|block| block.props.iter().map(|prop| (prop.name.as_str(), prop)))
        .into_iter()
        .flatten()
        .collect::<Declarations<'_>>();
    if let Some(style) = &component.style {
        validate_css(
            source,
            &declarations,
            &source[style.span.start..style.span.end],
            style.span.start,
        )?;
    }
    if let Some(template) = &component.template {
        validate_inline_styles(source, &declarations, &template.root)?;
    }
    Ok(())
}

fn validate_css(
    source: &str,
    declarations: &Declarations<'_>,
    css: &str,
    base: usize,
) -> Result<(), ParseError> {
    let stylesheet = mesh_core_theme::css::parse_stylesheet(css, base).map_err(|error| {
        ParseError::InvalidStyle {
            message: error.message,
            span: SourceSpan::new(error.span.start, error.span.end),
        }
    })?;
    validate_rules(source, declarations, &stylesheet.rules)
}

fn validate_rules(
    source: &str,
    declarations: &Declarations<'_>,
    rules: &[mesh_core_theme::css::Rule],
) -> Result<(), ParseError> {
    use mesh_core_theme::css::Rule;
    for rule in rules {
        match rule {
            Rule::Style(rule) => {
                for declaration in &rule.declarations {
                    validate_declaration(source, declarations, declaration)?;
                }
            }
            Rule::Container(rule) => validate_rules(source, declarations, &rule.rules)?,
            Rule::Keyframes(rule) => {
                for stop in &rule.stops {
                    for declaration in &stop.declarations {
                        validate_declaration(source, declarations, declaration)?;
                    }
                }
            }
            Rule::Custom(_) => {}
        }
    }
    Ok(())
}

fn validate_inline_styles(
    source: &str,
    declarations: &Declarations<'_>,
    nodes: &[TemplateNode],
) -> Result<(), ParseError> {
    for node in nodes {
        match node {
            TemplateNode::Element(element) => {
                validate_attributes(source, declarations, &element.attributes)?;
                validate_inline_styles(source, declarations, &element.children)?;
            }
            TemplateNode::Component(component) => {
                if let Some(attribute) = component.props.iter().find(|attr| attr.name == "part") {
                    return Err(semantic_error(
                        source,
                        attribute.span.unwrap_or(component.span),
                        "part is declared on native elements, not component instances",
                    ));
                }
                validate_attributes(source, declarations, &component.props)?;
                validate_inline_styles(source, declarations, &component.children)?;
            }
            TemplateNode::If(condition) => {
                validate_inline_styles(source, declarations, &condition.then_children)?;
                validate_inline_styles(source, declarations, &condition.else_children)?;
            }
            TemplateNode::For(repetition) => {
                validate_inline_styles(source, declarations, &repetition.children)?
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_attributes(
    source: &str,
    declarations: &Declarations<'_>,
    attributes: &[Attribute],
) -> Result<(), ParseError> {
    for attribute in attributes {
        if attribute.name == "part" {
            let valid = match &attribute.value {
                crate::AttributeValue::Static(value) => {
                    !value.trim().is_empty()
                        && value
                            .split_whitespace()
                            .all(mesh_core_theme::parts::valid_part_name)
                }
                _ => false,
            };
            if !valid {
                return Err(semantic_error(
                    source,
                    attribute.span.unwrap_or_default(),
                    "part requires static, space-separated public names (letters, digits, '_' and '-')",
                ));
            }
        }
        if attribute.name != "style" {
            continue;
        }
        let Some(span) = attribute.span else {
            continue;
        };
        let authored = &source[span.start..span.end];
        if !authored.starts_with("style") {
            continue;
        }
        let Some(equals) = authored.find('=') else {
            continue;
        };
        let after_equals = &authored[equals + 1..];
        let trimmed = after_equals.trim_start();
        let quote = trimmed.as_bytes().first().copied();
        let quoted = matches!(quote, Some(b'\'' | b'"'));
        let raw_start =
            span.start + equals + 1 + after_equals.len() - trimmed.len() + usize::from(quoted);
        let raw_end = span.end - usize::from(quoted);
        let raw = &source[raw_start..raw_end];
        let value = quick_xml::escape::unescape(raw)
            .map_err(|error| semantic_error(source, span, error.to_string()))?;
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        let mut cursor = 0;
        while cursor < raw.len() {
            let length = if raw.as_bytes()[cursor] == b'&' {
                raw[cursor..].find(';').map_or(1, |end| end + 1)
            } else {
                raw[cursor..].chars().next().unwrap().len_utf8()
            };
            let unit = &raw[cursor..cursor + length];
            let decoded = quick_xml::escape::unescape(unit)
                .map_err(|error| semantic_error(source, span, error.to_string()))?;
            starts.extend(std::iter::repeat_n(raw_start + cursor, decoded.len()));
            ends.extend(std::iter::repeat_n(
                raw_start + cursor + length,
                decoded.len(),
            ));
            cursor += length;
        }
        let wrapped = format!(".i{{{value}}}");
        validate_css(&wrapped, declarations, &wrapped, 0).map_err(|error| {
            let map = |span: SourceSpan| {
                let start = span.start.saturating_sub(3).min(starts.len());
                let end = span.end.saturating_sub(3).min(ends.len());
                SourceSpan::new(
                    starts.get(start).copied().unwrap_or(raw_end),
                    end.checked_sub(1)
                        .and_then(|index| ends.get(index))
                        .copied()
                        .unwrap_or(raw_start),
                )
            };
            match error {
                ParseError::InvalidSemantics { message, span } => ParseError::InvalidSemantics {
                    message,
                    span: map(span),
                },
                ParseError::InvalidStyle { message, span } => ParseError::InvalidStyle {
                    message,
                    span: map(span),
                },
                other => other,
            }
        })?;
    }
    Ok(())
}

fn validate_declaration(
    source: &str,
    declarations: &Declarations<'_>,
    declaration: &mesh_core_theme::css::Declaration,
) -> Result<(), ParseError> {
    let value_span = declaration.span;
    let value = &source[value_span.start..value_span.end];
    let references = styles::prop_references(value).map_err(|(span, message)| {
        semantic_error(
            source,
            SourceSpan::new(value_span.start + span.start, value_span.start + span.end),
            message,
        )
    })?;
    let property = &declaration.property;
    let domain = styles::prop_css_domain(property);
    for (name, relative_span) in references {
        let span = SourceSpan::new(
            value_span.start + relative_span.start,
            value_span.start + relative_span.end,
        );
        if !is_prop_name(&name) {
            return Err(semantic_error(
                source,
                span,
                format!("`prop({name})` must use an identifier"),
            ));
        }
        let Some(definition) = declarations.get(name.as_str()) else {
            return Err(semantic_error(
                source,
                span,
                format!("style references undefined prop `{name}`"),
            ));
        };
        if !styles::prop_type_matches(definition.ty, domain) {
            return Err(semantic_error(
                source,
                span,
                format!(
                    "prop `{name}` has type `{}` but CSS property `{property}` requires a {} value",
                    definition.ty.as_str(),
                    domain.as_str()
                ),
            ));
        }
    }
    Ok(())
}

fn is_prop_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn semantic_error(source: &str, span: SourceSpan, message: impl Into<String>) -> ParseError {
    ParseError::InvalidSemantics {
        message: message.into(),
        span: SourceSpan::new(span.start.min(source.len()), span.end.min(source.len())),
    }
}
