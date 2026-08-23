use crate::style::{
    ContainerQuery, Declaration, KeyframeRule, KeyframeStop, Selector, StyleBlock, StyleRule,
    StyleValue, is_transition_safe_keyframe_property,
};
use lightningcss::{
    media_query::{
        MediaFeatureComparison, MediaFeatureName, MediaFeatureValue, Operator,
        QueryFeature as LightningQueryFeature,
    },
    rules::container::{ContainerCondition, ContainerSizeFeature, ContainerSizeFeatureId},
    rules::{
        CssRule as LightningCssRule,
        keyframes::{KeyframeSelector, KeyframesName},
        style::StyleRule as LightningStyleRule,
    },
    stylesheet::{ParserOptions as CssParserOptions, PrinterOptions, StyleSheet},
    traits::ToCss as LightningToCss,
};

use super::ParseError;

pub(super) fn parse_style(source: &str) -> Result<StyleBlock, ParseError> {
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
    let mut keyframes = Vec::new();
    lower_css_rules(&stylesheet.rules.0, None, &mut rules, &mut keyframes)?;
    Ok(StyleBlock { rules, keyframes })
}

pub(super) fn parse_inline_style(source: &str) -> Result<Vec<Declaration>, ParseError> {
    let wrapped = format!(".mesh-inline-style {{ {source} }}");
    let mut block = parse_style(&wrapped)?;
    Ok(block
        .rules
        .pop()
        .map(|rule| rule.declarations)
        .unwrap_or_default())
}

fn lower_css_rules(
    source_rules: &[LightningCssRule<'_>],
    inherited_query: Option<ContainerQuery>,
    rules: &mut Vec<StyleRule>,
    keyframes: &mut Vec<KeyframeRule>,
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
                lower_css_rules(&container_rule.rules.0, combined_query, rules, keyframes)?;
            }
            LightningCssRule::Keyframes(keyframes_rule) => {
                keyframes.push(lower_keyframes_rule(keyframes_rule)?);
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

fn lower_keyframes_rule(
    source_rule: &lightningcss::rules::keyframes::KeyframesRule<'_>,
) -> Result<KeyframeRule, ParseError> {
    let name = lower_keyframe_name(&source_rule.name);
    let mut stops = Vec::new();

    for keyframe in &source_rule.keyframes {
        let declarations = lower_keyframe_declarations(&name, &keyframe.declarations)?;
        if declarations.is_empty() {
            continue;
        }

        for selector in &keyframe.selectors {
            let offset = lower_keyframe_selector(selector)?;
            stops.push(KeyframeStop {
                offset,
                declarations: declarations.clone(),
            });
        }
    }

    if stops.is_empty() {
        return Err(ParseError::InvalidStyle {
            message: format!("keyframes '{name}' has no supported animatable properties"),
            line: 0,
        });
    }

    stops.sort_by(|left, right| left.offset.total_cmp(&right.offset));
    Ok(KeyframeRule { name, stops })
}

fn lower_keyframe_name(name: &KeyframesName<'_>) -> String {
    match name {
        KeyframesName::Ident(ident) => ident.0.to_string(),
        KeyframesName::Custom(name) => name.to_string(),
    }
}

fn lower_keyframe_selector(selector: &KeyframeSelector) -> Result<f32, ParseError> {
    match selector {
        KeyframeSelector::Percentage(value) => Ok(value.0.clamp(0.0, 1.0)),
        KeyframeSelector::From | KeyframeSelector::To => Err(ParseError::InvalidStyle {
            message: "from/to keyframe aliases are not supported".into(),
            line: 0,
        }),
        KeyframeSelector::TimelineRangePercentage(_) => Err(ParseError::InvalidStyle {
            message: "timeline-range keyframe selectors are not supported".into(),
            line: 0,
        }),
    }
}

fn lower_keyframe_declarations(
    rule_name: &str,
    source_block: &lightningcss::declaration::DeclarationBlock<'_>,
) -> Result<Vec<Declaration>, ParseError> {
    let mut declarations = Vec::new();

    for property in &source_block.declarations {
        let declaration = lower_property(property)?;
        validate_keyframe_declaration(rule_name, &declaration)?;
        declarations.push(declaration);
    }
    for property in &source_block.important_declarations {
        let declaration = lower_property(property)?;
        validate_keyframe_declaration(rule_name, &declaration)?;
        declarations.push(declaration);
    }

    Ok(declarations)
}

fn validate_keyframe_declaration(
    rule_name: &str,
    declaration: &Declaration,
) -> Result<(), ParseError> {
    if contains_keyframe_value_reference(&declaration.value) {
        return Err(ParseError::InvalidStyle {
            message: format!("keyframes '{rule_name}' cannot use var() references in stop values"),
            line: 0,
        });
    }
    if !is_transition_safe_keyframe_property(&declaration.property) {
        return Err(ParseError::InvalidStyle {
            message: format!("unsupported keyframe property '{}'", declaration.property),
            line: 0,
        });
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
    mesh_core_theme::css::parse_selector(source)
        .map_err(|message| ParseError::InvalidStyle { message, line: 0 })
}

fn classify_style_value(value: &str) -> StyleValue {
    let value = value.trim();
    if value.starts_with("var(") && value.ends_with(')') {
        StyleValue::Var(value[4..value.len() - 1].trim().to_string())
    } else if let Some(name) = standalone_prop_reference(value) {
        StyleValue::Prop(name)
    } else {
        StyleValue::Literal(value.to_string())
    }
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
        "opacity" | "z-index" | "order" | "flex" | "flex-grow" | "flex-shrink"
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

/// Match a value that is *exactly* one `prop(name)` reference. Embedded uses
/// (inside `calc()` or a shorthand) stay `Literal` and are substituted later.
fn standalone_prop_reference(value: &str) -> Option<String> {
    let inner = value.strip_prefix("prop(")?.strip_suffix(')')?;
    // Reject multi-function values like `prop(a) prop(b)`: the inner must hold a
    // single identifier with no nested parens.
    if inner.contains('(') || inner.contains(')') {
        return None;
    }
    Some(inner.trim().to_string())
}

fn contains_keyframe_value_reference(value: &StyleValue) -> bool {
    match value {
        StyleValue::Var(_) | StyleValue::Prop(_) => true,
        StyleValue::Literal(value) => value.contains("var(") || value.contains("prop("),
    }
}
