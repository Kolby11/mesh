use super::ParseError;
use crate::SourceSpan;
use crate::style::{
    ContainerQuery, Declaration, KeyframeRule, KeyframeStop, StyleBlock, StyleRule, StyleValue,
    classify_style_value,
};

/// Lower component CSS through the same restricted parser used by themes.
/// Component-only concerns (prop references and the component AST) are applied
/// after the shared CSS/token/keyframe syntax has been validated.
pub(super) fn parse_style(source: &str, source_base: usize) -> Result<StyleBlock, ParseError> {
    let stylesheet =
        mesh_core_theme::css::parse_stylesheet(source, source_base).map_err(map_shared_error)?;
    let mut rules = Vec::new();
    let mut keyframes = Vec::new();
    lower_rules(&stylesheet.rules, None, &mut rules, &mut keyframes)?;
    Ok(StyleBlock {
        rules,
        keyframes,
        span: SourceSpan::new(source_base, source_base + source.len()),
    })
}

pub(super) fn parse_inline_style(source: &str) -> Result<Vec<Declaration>, ParseError> {
    let wrapped = format!(".mesh-inline-style {{ {source} }}");
    let mut block = parse_style(&wrapped, 0)?;
    Ok(block
        .rules
        .pop()
        .map(|rule| rule.declarations)
        .unwrap_or_default())
}

fn lower_rules(
    source_rules: &[mesh_core_theme::css::Rule],
    inherited_query: Option<ContainerQuery>,
    rules: &mut Vec<StyleRule>,
    keyframes: &mut Vec<KeyframeRule>,
) -> Result<(), ParseError> {
    for rule in source_rules {
        match rule {
            mesh_core_theme::css::Rule::Style(style_rule) => {
                let container_query = inherited_query;
                let declarations = style_rule
                    .declarations
                    .iter()
                    .map(lower_declaration)
                    .collect::<Vec<_>>();
                for selector in &style_rule.selectors {
                    rules.push(StyleRule {
                        selector: selector.clone(),
                        declarations: declarations.clone(),
                        container_query,
                    });
                }
            }
            mesh_core_theme::css::Rule::Container(container_rule) => {
                let query = convert_container_query(container_rule.query);
                let combined_query = inherited_query
                    .map(|existing| existing.intersect(query))
                    .or(Some(query));
                lower_rules(&container_rule.rules, combined_query, rules, keyframes)?;
            }
            mesh_core_theme::css::Rule::Keyframes(keyframes_rule) => {
                keyframes.push(lower_keyframes_rule(keyframes_rule)?);
            }
            mesh_core_theme::css::Rule::Custom(custom_rule) => {
                return Err(ParseError::InvalidStyle {
                    message: format!("unsupported at-rule '@{}'", custom_rule.name),
                    span: to_source_span(custom_rule.span),
                });
            }
        }
    }
    Ok(())
}

fn lower_declaration(declaration: &mesh_core_theme::css::Declaration) -> Declaration {
    Declaration {
        property: declaration.property.clone(),
        value: classify_style_value(&declaration.value),
    }
}

fn lower_keyframes_rule(
    source_rule: &mesh_core_theme::css::KeyframesRule,
) -> Result<KeyframeRule, ParseError> {
    let stops = source_rule
        .stops
        .iter()
        .map(|stop| {
            let declarations = stop
                .declarations
                .iter()
                .map(lower_declaration)
                .collect::<Vec<_>>();
            if let Some(declaration) = declarations.iter().find(|declaration| {
                !mesh_core_theme::css::is_transition_safe_keyframe_property(&declaration.property)
            }) {
                return Err(ParseError::InvalidStyle {
                    message: format!(
                        "unsupported keyframe property '{}'",
                        declaration.property
                    ),
                    span: to_source_span(stop.span),
                });
            }
            if declarations.iter().any(contains_keyframe_value_reference) {
                return Err(ParseError::InvalidStyle {
                    message: format!(
                        "keyframes '{}' cannot use var() references in stop values (prop() references are also unavailable)",
                        source_rule.name
                    ),
                    span: to_source_span(stop.span),
                });
            }
            let easing = stop
                .easing
                .as_deref()
                .map(|value| {
                    crate::style::parse_easing(value).ok_or_else(|| ParseError::InvalidStyle {
                        message: format!(
                            "unsupported keyframe easing '{}' in '{}'",
                            value.trim(),
                            source_rule.name
                        ),
                        span: to_source_span(stop.span),
                    })
                })
                .transpose()?;
            Ok(KeyframeStop {
                offset: stop.offset,
                declarations,
                easing,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(KeyframeRule {
        name: source_rule.name.clone(),
        stops,
    })
}

fn contains_keyframe_value_reference(declaration: &Declaration) -> bool {
    match &declaration.value {
        StyleValue::Var(_) | StyleValue::Prop(_) => true,
        StyleValue::Literal(value) => value.contains("var(") || value.contains("prop("),
    }
}

fn convert_container_query(query: mesh_core_theme::css::ContainerQuery) -> ContainerQuery {
    ContainerQuery {
        min_width: query.min_width,
        max_width: query.max_width,
        min_height: query.min_height,
        max_height: query.max_height,
    }
}

fn map_shared_error(error: mesh_core_theme::css::LoweringError) -> ParseError {
    ParseError::InvalidStyle {
        message: error.message,
        span: to_source_span(error.span),
    }
}

fn to_source_span(span: mesh_core_theme::css::SourceSpan) -> SourceSpan {
    SourceSpan::new(span.start, span.end)
}

#[derive(Debug, Clone, Copy)]
pub(super) enum PropCssDomain {
    Any,
    Length,
    Number,
    Color,
    Time,
    Keyword,
}

impl PropCssDomain {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Any => "CSS-compatible",
            Self::Length => "length",
            Self::Number => "number",
            Self::Color => "color",
            Self::Time => "time",
            Self::Keyword => "keyword",
        }
    }
}

/// Extract component-prop references from a serialized CSS value.
pub(super) fn prop_references(value: &str) -> Result<Vec<String>, String> {
    let mut references = Vec::new();
    let mut cursor = 0;
    while cursor < value.len() {
        let ch = value[cursor..]
            .chars()
            .next()
            .expect("cursor stays on a UTF-8 boundary");
        if matches!(ch, '\'' | '"') {
            cursor = skip_css_string(value, cursor)?;
            continue;
        }
        if !value[cursor..].starts_with("prop(") {
            cursor += ch.len_utf8();
            continue;
        }
        let start = cursor;
        if start > 0 && is_css_identifier_byte(value.as_bytes()[start - 1]) {
            cursor = start + "prop".len();
            continue;
        }

        let end = find_function_end(value, start + "prop".len())
            .ok_or_else(|| "unterminated `prop(...)` reference".to_string())?;
        let name = value[start + "prop(".len()..end].trim();
        if name.is_empty() {
            return Err("`prop()` needs a non-empty prop name".into());
        }
        if name.contains('(') || name.contains(')') {
            return Err(format!("`prop({name})` cannot contain nested calls"));
        }
        references.push(name.to_string());
        cursor = end + 1;
    }
    Ok(references)
}

fn skip_css_string(value: &str, start: usize) -> Result<usize, String> {
    let quote = value.as_bytes()[start] as char;
    let mut cursor = start + 1;
    while cursor < value.len() {
        let ch = value[cursor..]
            .chars()
            .next()
            .expect("cursor stays on a UTF-8 boundary");
        if ch == '\\' {
            cursor += ch.len_utf8();
            if cursor < value.len() {
                cursor += value[cursor..]
                    .chars()
                    .next()
                    .expect("escaped character is valid UTF-8")
                    .len_utf8();
            }
            continue;
        }
        cursor += ch.len_utf8();
        if ch == quote {
            return Ok(cursor);
        }
    }
    Err("unterminated CSS string while reading prop references".into())
}

fn find_function_end(value: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut cursor = open;
    while cursor < value.len() {
        let ch = value[cursor..].chars().next()?;
        if let Some(expected) = quote {
            if ch == '\\' {
                cursor += ch.len_utf8();
                if cursor < value.len() {
                    cursor += value[cursor..].chars().next()?.len_utf8();
                }
                continue;
            }
            if ch == expected {
                quote = None;
            }
            cursor += ch.len_utf8();
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => {}
        }
        cursor += ch.len_utf8();
    }
    None
}

fn is_css_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

pub(super) fn prop_css_domain(property: &str) -> PropCssDomain {
    if property.starts_with("--") {
        return PropCssDomain::Any;
    }
    if matches!(
        property,
        "width"
            | "height"
            | "min-width"
            | "max-width"
            | "min-height"
            | "max-height"
            | "font-size"
            | "letter-spacing"
            | "inset"
            | "top"
            | "right"
            | "bottom"
            | "left"
    ) || property.starts_with("margin")
        || property.starts_with("padding")
        || property.starts_with("gap")
        || (property.starts_with("border") && property.ends_with("width"))
        || (property.starts_with("border") && property.ends_with("radius"))
    {
        return PropCssDomain::Length;
    }
    if matches!(
        property,
        "opacity" | "z-index" | "order" | "flex" | "flex-grow" | "flex-shrink" | "aspect-ratio"
    ) {
        return PropCssDomain::Number;
    }
    if matches!(
        property,
        "color"
            | "background-color"
            | "border-color"
            | "outline-color"
            | "caret-color"
            | "accent-color"
            | "text-decoration-color"
            | "column-rule-color"
    ) || (property.starts_with("border") && property.ends_with("color"))
    {
        return PropCssDomain::Color;
    }
    if matches!(
        property,
        "transition-duration" | "transition-delay" | "animation-duration" | "animation-delay"
    ) {
        return PropCssDomain::Time;
    }
    if matches!(
        property,
        "display"
            | "position"
            | "exclusive-zone"
            | "visibility"
            | "overflow"
            | "overflow-x"
            | "overflow-y"
            | "flex-direction"
            | "flex-wrap"
            | "align-items"
            | "align-content"
            | "align-self"
            | "justify-content"
            | "justify-items"
            | "justify-self"
            | "white-space"
            | "text-align"
            | "font-family"
            | "font-style"
            | "cursor"
            | "content"
    ) {
        return PropCssDomain::Keyword;
    }
    PropCssDomain::Any
}

pub(super) fn prop_type_matches(prop_type: crate::PropType, domain: PropCssDomain) -> bool {
    use crate::PropType;
    match domain {
        PropCssDomain::Any => true,
        PropCssDomain::Length => matches!(
            prop_type,
            PropType::Size | PropType::Number | PropType::Int | PropType::Token
        ),
        PropCssDomain::Number => matches!(
            prop_type,
            PropType::Number | PropType::Int | PropType::Bool | PropType::Token
        ),
        PropCssDomain::Color => matches!(prop_type, PropType::Color | PropType::Token),
        PropCssDomain::Time => matches!(prop_type, PropType::Duration | PropType::Token),
        PropCssDomain::Keyword => matches!(
            prop_type,
            PropType::Enum | PropType::String | PropType::Icon | PropType::Token
        ),
    }
}
