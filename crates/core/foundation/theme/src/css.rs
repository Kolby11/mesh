//! Restricted CSS syntax and lowering shared by themes and components.
//!
//! This module is deliberately renderer-neutral. It parses CSS with one
//! implementation, retains the restricted selector AST used by the element
//! matcher, and lowers declarations/keyframes without depending on the UI
//! component crate. Theme and component consumers apply their own runtime
//! value classification after this point.

use cssparser::{Parser, ParserInput, ToCss, Token};
use lightningcss::{
    media_query::{
        MediaFeatureComparison, MediaFeatureName, MediaFeatureValue, Operator,
        QueryFeature as LightningQueryFeature,
    },
    rules::{
        CssRule as LightningCssRule,
        container::{ContainerCondition, ContainerSizeFeature, ContainerSizeFeatureId},
        keyframes::{KeyframeSelector, KeyframesName},
        style::StyleRule as LightningStyleRule,
    },
    stylesheet::{ParserOptions as CssParserOptions, PrinterOptions, StyleSheet},
    traits::ToCss as LightningToCss,
};
use serde::{Deserialize, Serialize};

/// The selector subset MESH can match against a single element node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Selector {
    Tag(String),
    Class(String),
    Id(String),
    State(String, String),
    Compound(Vec<Selector>),
    Universal,
}

/// Parse one selector and reject combinators, functions, attributes, and
/// other CSS syntax for which the retained element tree has no matcher.
pub fn parse_selector(source: &str) -> Result<Selector, String> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut parts = Vec::new();

    while let Ok(token) = parser.next() {
        match token {
            Token::Delim('*') => parts.push(Selector::Universal),
            Token::Delim('.') => {
                let class = parser
                    .expect_ident_cloned()
                    .map_err(|error| format!("{error:?}"))?;
                parts.push(Selector::Class(class.to_string()));
            }
            Token::IDHash(id) => parts.push(Selector::Id(id.to_string())),
            Token::Colon => {
                let state = parser
                    .expect_ident_cloned()
                    .map_err(|error| format!("{error:?}"))?;
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
            Token::WhiteSpace(_) => {
                return Err("descendant and sibling combinators are not supported".into());
            }
            other => {
                return Err(format!(
                    "unsupported selector token {}",
                    other.to_css_string()
                ));
            }
        }
    }

    if parts.is_empty() {
        return Err("empty selector".into());
    }
    if parts.len() == 1 {
        Ok(parts.remove(0))
    } else {
        Ok(Selector::Compound(parts))
    }
}

/// Classify a value that consists of one CSS custom-property reference. The
/// complete contents are retained so fallback recipes can be resolved later.
pub fn variable_reference(value: &str) -> Option<String> {
    let value = value.trim();
    let inner = value.strip_prefix("var(")?.strip_suffix(')')?;
    (!inner.trim().is_empty()).then(|| inner.trim().to_string())
}

/// A byte range in the source passed to [`parse_stylesheet`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A source-located error produced while parsing or lowering restricted CSS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweringError {
    pub message: String,
    pub span: SourceSpan,
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LoweringError {}

/// The result of the shared restricted CSS lowering pass.
#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

/// One rule in a restricted stylesheet.
#[derive(Debug, Clone)]
pub enum Rule {
    Style(StyleRule),
    Container(ContainerRule),
    Keyframes(KeyframesRule),
    /// An at-rule not understood by the shared layer. Theme parsing uses this
    /// for its graph-scoped `@module` extension; components reject it.
    Custom(CustomRule),
}

#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ContainerRule {
    pub query: ContainerQuery,
    pub rules: Vec<Rule>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct KeyframesRule {
    pub name: String,
    pub stops: Vec<KeyframeStop>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct KeyframeStop {
    pub offset: f32,
    pub declarations: Vec<Declaration>,
    pub easing: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct CustomRule {
    pub name: String,
    pub prelude: String,
    pub block: Option<String>,
    pub block_base: Option<usize>,
    pub span: SourceSpan,
}

/// A raw, ordered CSS declaration. Consumers classify `value` into their
/// runtime value types, while retaining the exact lowered CSS spelling here.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    /// The authored value before lightningcss canonicalization. Runtime
    /// consumers normally use `value`; theme serialization uses this to keep
    /// token recipes and MESH-specific literals lossless.
    pub raw_value: String,
    pub span: SourceSpan,
}

/// A simple container-size query supported by MESH.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ContainerQuery {
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
}

impl ContainerQuery {
    pub fn intersect(self, other: Self) -> Self {
        Self {
            min_width: match (self.min_width, other.min_width) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            max_width: match (self.max_width, other.max_width) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            min_height: match (self.min_height, other.min_height) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            max_height: match (self.max_height, other.max_height) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
        }
    }
}

/// Parse and lower the restricted stylesheet shared by component and theme
/// sources. Parser and lowering failures retain an absolute source span.
pub fn parse_stylesheet(source: &str, source_base: usize) -> Result<Stylesheet, LoweringError> {
    validate_source_structure(source, source_base)?;
    let stylesheet = StyleSheet::parse(
        source,
        CssParserOptions {
            filename: "<style>".into(),
            error_recovery: false,
            ..CssParserOptions::default()
        },
    )
    .map_err(|error| map_lightning_error(error, source, source_base))?;

    let rules = lower_rules(&stylesheet.rules.0, source, source_base)?;
    Ok(Stylesheet { rules })
}

fn lower_rules(
    source_rules: &[LightningCssRule<'_>],
    source: &str,
    source_base: usize,
) -> Result<Vec<Rule>, LoweringError> {
    source_rules
        .iter()
        .map(|rule| match rule {
            LightningCssRule::Style(style_rule) => {
                lower_style_rule(style_rule, source, source_base)
            }
            LightningCssRule::Container(container_rule) => {
                let span = location_span(container_rule.loc, source, source_base);
                let Some(condition) = &container_rule.condition else {
                    return Err(error("container query is missing a condition", span));
                };
                Ok(Rule::Container(ContainerRule {
                    query: lower_container_condition(condition, source, source_base)?,
                    rules: lower_rules(&container_rule.rules.0, source, source_base)?,
                    span,
                }))
            }
            LightningCssRule::Keyframes(keyframes_rule) => {
                lower_keyframes_rule(keyframes_rule, source, source_base)
            }
            LightningCssRule::Ignored => Err(error(
                "ignored CSS rules are not supported",
                SourceSpan::new(source_base, source_base + source.len()),
            )),
            LightningCssRule::Unknown(unknown) => {
                let span = location_span(unknown.loc, source, source_base);
                let source_offset = line_column_to_offset(
                    source,
                    unknown.loc.line as usize,
                    unknown.loc.column as usize,
                );
                let name_end = source_offset
                    .saturating_add(1)
                    .saturating_add(unknown.name.len());
                let tail = &source[name_end.min(source.len())..];
                let open = tail.find('{').map(|offset| name_end + offset);
                let semicolon = tail.find(';').map(|offset| name_end + offset);
                let (prelude, block, block_base) = match (open, semicolon) {
                    (Some(open), Some(semicolon)) if semicolon < open => {
                        (source[name_end..semicolon].trim().to_string(), None, None)
                    }
                    (Some(open), _) => {
                        let close = find_matching_brace(source, open).ok_or_else(|| {
                            error("missing closing brace for custom at-rule", span)
                        })?;
                        (
                            source[name_end..open].trim().to_string(),
                            Some(source[open + 1..close].to_string()),
                            Some(source_base + open + 1),
                        )
                    }
                    _ => (String::new(), None, None),
                };
                Ok(Rule::Custom(CustomRule {
                    name: unknown.name.to_string(),
                    prelude,
                    block,
                    block_base,
                    span,
                }))
            }
            other => Err(error(
                format!("unsupported at-rule '{}'", css_rule_name(other)),
                rule_location(other, source, source_base),
            )),
        })
        .collect()
}

fn lower_style_rule(
    source_rule: &LightningStyleRule<'_>,
    source: &str,
    source_base: usize,
) -> Result<Rule, LoweringError> {
    let span = location_span(source_rule.loc, source, source_base);
    if !source_rule.rules.0.is_empty() {
        return Err(error("nested style rules are not supported", span));
    }

    let raw_declarations = raw_declarations_for_span(source, source_base, span);
    let declarations = lower_declarations(&source_rule.declarations, &raw_declarations, span)?;
    let selectors = source_rule
        .selectors
        .0
        .iter()
        .map(|selector| {
            let selector_source = selector
                .to_css_string(PrinterOptions::default())
                .map_err(|err| error(err.to_string(), span))?;
            parse_selector(&selector_source).map_err(|message| error(message, span))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Rule::Style(StyleRule {
        selectors,
        declarations,
        span,
    }))
}

fn lower_keyframes_rule(
    source_rule: &lightningcss::rules::keyframes::KeyframesRule<'_>,
    source: &str,
    source_base: usize,
) -> Result<Rule, LoweringError> {
    let span = location_span(source_rule.loc, source, source_base);
    let name = match &source_rule.name {
        KeyframesName::Ident(ident) => ident.0.to_string(),
        KeyframesName::Custom(name) => name.to_string(),
    };
    let mut stops = Vec::new();
    let raw_blocks = raw_keyframe_blocks(source, source_base, span);

    for (keyframe_index, keyframe) in source_rule.keyframes.iter().enumerate() {
        let stop_span = span;
        let raw_declarations = raw_blocks
            .get(keyframe_index)
            .map(|block| split_raw_declarations(block.as_str()))
            .unwrap_or_default();
        let declarations =
            lower_declarations(&keyframe.declarations, &raw_declarations, stop_span)?;
        if let Some(declaration) = declarations.iter().find(|declaration| {
            declaration.property != "animation-timing-function"
                && !is_transition_safe_keyframe_property(&declaration.property)
        }) {
            return Err(error(
                format!("unsupported keyframe property '{}'", declaration.property),
                declaration.span,
            ));
        }
        let easing = declarations
            .iter()
            .find(|declaration| declaration.property == "animation-timing-function")
            .map(|declaration| declaration.value.clone());
        let declarations = declarations
            .into_iter()
            .filter(|declaration| declaration.property != "animation-timing-function")
            .collect::<Vec<_>>();

        if declarations.is_empty() {
            continue;
        }
        for selector in &keyframe.selectors {
            stops.push(KeyframeStop {
                offset: lower_keyframe_selector(selector, stop_span)?,
                declarations: declarations.clone(),
                easing: easing.clone(),
                span: stop_span,
            });
        }
    }

    if stops.is_empty() {
        return Err(error(
            format!("keyframes '{name}' has no supported animatable properties"),
            span,
        ));
    }
    stops.sort_by(|left, right| left.offset.total_cmp(&right.offset));
    Ok(Rule::Keyframes(KeyframesRule { name, stops, span }))
}

fn lower_declarations(
    source_block: &lightningcss::declaration::DeclarationBlock<'_>,
    raw_declarations: &[(String, String)],
    span: SourceSpan,
) -> Result<Vec<Declaration>, LoweringError> {
    let mut raw_used = vec![false; raw_declarations.len()];
    source_block
        .declarations
        .iter()
        .chain(source_block.important_declarations.iter())
        .map(|property| {
            let property_name = property.property_id().name().to_string();
            let value = property
                .value_to_css_string(PrinterOptions::default())
                .map_err(|err| error(err.to_string(), span))?;
            let raw_value = raw_declarations
                .iter()
                .enumerate()
                .find(|(index, (name, _))| !raw_used[*index] && name == &property_name)
                .map(|(index, (_, value))| {
                    raw_used[index] = true;
                    value.clone()
                })
                .unwrap_or_else(|| value.clone());
            Ok(Declaration {
                property: property_name,
                value,
                raw_value,
                span,
            })
        })
        .collect()
}

fn lower_keyframe_selector(
    selector: &KeyframeSelector,
    span: SourceSpan,
) -> Result<f32, LoweringError> {
    match selector {
        KeyframeSelector::Percentage(value) => Ok(value.0.clamp(0.0, 1.0)),
        KeyframeSelector::From => Ok(0.0),
        KeyframeSelector::To => Ok(1.0),
        KeyframeSelector::TimelineRangePercentage(_) => Err(error(
            "timeline-range keyframe selectors are not supported",
            span,
        )),
    }
}

fn lower_container_condition(
    condition: &ContainerCondition<'_>,
    source: &str,
    source_base: usize,
) -> Result<ContainerQuery, LoweringError> {
    let span = SourceSpan::new(source_base, source_base + source.len());
    match condition {
        ContainerCondition::Feature(feature) => lower_container_feature(feature, span),
        ContainerCondition::Operation {
            operator: Operator::And,
            conditions,
        } => conditions
            .iter()
            .try_fold(ContainerQuery::default(), |query, condition| {
                Ok(query.intersect(lower_container_condition(condition, source, source_base)?))
            }),
        ContainerCondition::Operation {
            operator: Operator::Or,
            ..
        } => Err(error("container queries with 'or' are not supported", span)),
        ContainerCondition::Not(_) => {
            Err(error("negated container queries are not supported", span))
        }
        ContainerCondition::Style(_) => {
            Err(error("style container queries are not supported", span))
        }
        ContainerCondition::ScrollState(_) => Err(error(
            "scroll-state container queries are not supported",
            span,
        )),
        ContainerCondition::Unknown(_) => Err(error("unsupported container query condition", span)),
    }
}

fn lower_container_feature(
    feature: &ContainerSizeFeature<'_>,
    span: SourceSpan,
) -> Result<ContainerQuery, LoweringError> {
    match feature {
        LightningQueryFeature::Plain { name, value } => {
            let axis = container_feature_axis(name, span)?;
            let value = container_feature_length(value, span)?;
            let mut query = ContainerQuery::default();
            apply_container_bound(&mut query, axis, MediaFeatureComparison::Equal, value);
            Ok(query)
        }
        LightningQueryFeature::Range {
            name,
            operator,
            value,
        } => {
            let axis = container_feature_axis(name, span)?;
            let value = container_feature_length(value, span)?;
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
            let axis = container_feature_axis(name, span)?;
            let start = container_feature_length(start, span)?;
            let end = container_feature_length(end, span)?;
            let mut query = ContainerQuery::default();
            apply_container_bound(&mut query, axis, invert_comparison(*start_operator), start);
            apply_container_bound(&mut query, axis, *end_operator, end);
            Ok(query)
        }
        LightningQueryFeature::Boolean { .. } => {
            Err(error("boolean container queries are not supported", span))
        }
    }
}

#[derive(Clone, Copy)]
enum ContainerAxis {
    Width,
    Height,
}

fn container_feature_axis(
    name: &MediaFeatureName<'_, ContainerSizeFeatureId>,
    span: SourceSpan,
) -> Result<ContainerAxis, LoweringError> {
    match name {
        MediaFeatureName::Standard(ContainerSizeFeatureId::Width)
        | MediaFeatureName::Standard(ContainerSizeFeatureId::InlineSize) => {
            Ok(ContainerAxis::Width)
        }
        MediaFeatureName::Standard(ContainerSizeFeatureId::Height)
        | MediaFeatureName::Standard(ContainerSizeFeatureId::BlockSize) => {
            Ok(ContainerAxis::Height)
        }
        MediaFeatureName::Standard(other) => Err(error(
            format!("unsupported container query property '{other:?}'"),
            span,
        )),
        MediaFeatureName::Custom(_) | MediaFeatureName::Unknown(_) => Err(error(
            "custom container query properties are not supported",
            span,
        )),
    }
}

fn container_feature_length(
    value: &MediaFeatureValue<'_>,
    span: SourceSpan,
) -> Result<f32, LoweringError> {
    match value {
        MediaFeatureValue::Length(length) => length
            .to_px()
            .ok_or_else(|| error("container query length must be convertible to px", span)),
        other => Err(error(
            format!("unsupported container query value '{other:?}'"),
            span,
        )),
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

/// Properties that the animation sampler can interpolate safely.
pub fn is_transition_safe_keyframe_property(property: &str) -> bool {
    matches!(
        property,
        "background"
            | "background-color"
            | "border-color"
            | "border-radius"
            | "border-top-left-radius"
            | "border-top-right-radius"
            | "border-bottom-right-radius"
            | "border-bottom-left-radius"
            | "border-width"
            | "border-top-width"
            | "border-right-width"
            | "border-bottom-width"
            | "border-left-width"
            | "color"
            | "opacity"
            | "width"
            | "height"
            | "min-width"
            | "max-width"
            | "min-height"
            | "max-height"
            | "padding"
            | "padding-top"
            | "padding-right"
            | "padding-bottom"
            | "padding-left"
            | "padding-x"
            | "padding-y"
            | "padding-inline"
            | "padding-block"
            | "margin"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "margin-x"
            | "margin-y"
            | "margin-inline"
            | "margin-block"
            | "transform"
            | "box-shadow"
            | "filter"
            | "backdrop-filter"
            | "font-size"
            | "letter-spacing"
            | "line-height"
            | "gap"
            | "gap-x"
            | "row-gap"
            | "column-gap"
            | "inset"
            | "top"
            | "right"
            | "bottom"
            | "left"
    )
}

fn error(message: impl Into<String>, span: SourceSpan) -> LoweringError {
    LoweringError {
        message: message.into(),
        span,
    }
}

fn map_lightning_error<T: std::fmt::Display>(
    err: lightningcss::error::Error<T>,
    source: &str,
    source_base: usize,
) -> LoweringError {
    let span = err
        .loc
        .map(|loc| {
            let offset = line_column_to_offset(source, loc.line as usize, loc.column as usize);
            SourceSpan::new(source_base + offset, source_base + offset + 1)
        })
        .unwrap_or_else(|| SourceSpan::new(source_base, source_base + source.len()));
    error(err.kind.to_string(), span)
}

fn map_location(line: u32, column: u32, source: &str, source_base: usize) -> SourceSpan {
    let offset = line_column_to_offset(source, line as usize, column as usize);
    SourceSpan::new(source_base + offset, source_base + offset.saturating_add(1))
}

fn location_span(
    location: lightningcss::rules::Location,
    source: &str,
    source_base: usize,
) -> SourceSpan {
    map_location(location.line, location.column, source, source_base)
}

fn rule_location(rule: &LightningCssRule<'_>, source: &str, source_base: usize) -> SourceSpan {
    match rule {
        LightningCssRule::Style(rule) => location_span(rule.loc, source, source_base),
        LightningCssRule::Container(rule) => location_span(rule.loc, source, source_base),
        LightningCssRule::Keyframes(rule) => location_span(rule.loc, source, source_base),
        LightningCssRule::Unknown(rule) => location_span(rule.loc, source, source_base),
        _ => SourceSpan::new(source_base, source_base + source.len()),
    }
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

fn line_column_to_offset(source: &str, line: usize, column: usize) -> usize {
    let mut current_line = 0;
    let mut line_start = 0;
    for (offset, ch) in source.char_indices() {
        if current_line == line {
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = offset + 1;
        }
    }
    source[line_start..]
        .char_indices()
        .take_while(|(_, ch)| *ch != '\n')
        .take(column.saturating_sub(1))
        .last()
        .map_or(line_start, |(offset, ch)| {
            line_start + offset + ch.len_utf8()
        })
        .min(source.len())
}

fn validate_source_structure(source: &str, source_base: usize) -> Result<(), LoweringError> {
    let mut in_comment = false;
    let mut in_quote = None;
    let mut escaped = false;
    let mut last_close = None;
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if in_comment {
            if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                in_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if let Some(quote) = in_quote {
            if escaped {
                escaped = false;
            } else if bytes[index] == b'\\' {
                escaped = true;
            } else if bytes[index] == quote {
                in_quote = None;
            }
            index += 1;
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            in_comment = true;
            index += 2;
            continue;
        }
        if bytes[index] == b'\'' || bytes[index] == b'"' {
            in_quote = Some(bytes[index]);
        } else if bytes[index] == b'}' {
            last_close = Some(index);
        }
        index += 1;
    }
    if in_comment {
        return Err(error(
            "unterminated CSS comment",
            SourceSpan::new(
                source_base + source.len().saturating_sub(2),
                source_base + source.len(),
            ),
        ));
    }
    if in_quote.is_some() {
        return Err(error(
            "unterminated CSS string",
            SourceSpan::new(
                source_base + source.len().saturating_sub(1),
                source_base + source.len(),
            ),
        ));
    }
    if let Some(close) = last_close {
        let tail = &source[close + 1..];
        let mut tail_without_comments = String::with_capacity(tail.len());
        let mut rest = tail;
        while let Some(start) = rest.find("/*") {
            tail_without_comments.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("*/") else {
                break;
            };
            rest = &after[end + 2..];
        }
        tail_without_comments.push_str(rest);
        if !tail_without_comments.trim().is_empty() {
            return Err(error(
                format!(
                    "unexpected trailing CSS: '{}'",
                    tail_without_comments.trim()
                ),
                SourceSpan::new(source_base + close + 1, source_base + source.len()),
            ));
        }
    }
    Ok(())
}

fn raw_declarations_for_span(
    source: &str,
    source_base: usize,
    span: SourceSpan,
) -> Vec<(String, String)> {
    let local_start = span.start.saturating_sub(source_base).min(source.len());
    let Some(open_offset) = source[local_start..].find('{') else {
        return Vec::new();
    };
    let open = local_start + open_offset;
    let Some(close) = find_matching_brace(source, open) else {
        return Vec::new();
    };
    split_raw_declarations(&source[open + 1..close])
}

fn raw_keyframe_blocks(source: &str, source_base: usize, span: SourceSpan) -> Vec<String> {
    let local_start = span.start.saturating_sub(source_base).min(source.len());
    let Some(open_offset) = source[local_start..].find('{') else {
        return Vec::new();
    };
    let open = local_start + open_offset;
    let Some(close) = find_matching_brace(source, open) else {
        return Vec::new();
    };
    let mut blocks = Vec::new();
    let mut cursor = open + 1;
    while cursor < close {
        let Some(relative_open) = source[cursor..close].find('{') else {
            break;
        };
        let nested_open = cursor + relative_open;
        let Some(nested_close) = find_matching_brace(source, nested_open) else {
            break;
        };
        if nested_close > close {
            break;
        }
        blocks.push(source[nested_open + 1..nested_close].to_string());
        cursor = nested_close + 1;
    }
    blocks
}

fn split_raw_declarations(body: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    let bytes = body.as_bytes();
    for index in 0..bytes.len() {
        let byte = bytes[index];
        if comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                comment = false;
            }
            continue;
        }
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == active_quote {
                quote = None;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            comment = true;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => {
                push_raw_declaration(&mut result, &body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    push_raw_declaration(&mut result, &body[start..]);
    result
}

fn push_raw_declaration(result: &mut Vec<(String, String)>, raw: &str) {
    let raw = strip_css_comments(raw);
    let Some(colon) = raw.find(':') else {
        return;
    };
    let property = raw[..colon].trim();
    let value = raw[colon + 1..].trim();
    if !property.is_empty() && !value.is_empty() {
        result.push((property.to_string(), value.to_string()));
    }
}

fn strip_css_comments(value: &str) -> String {
    let mut result = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut quote = None;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            result.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            result.push(byte);
            index += 1;
        } else if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            if index + 1 < bytes.len() {
                index += 2;
            }
        } else {
            result.push(byte);
            index += 1;
        }
    }
    String::from_utf8(result).expect("comment stripping preserves UTF-8")
}

fn find_matching_brace(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut comment = false;
    let mut escaped = false;
    let bytes = source.as_bytes();
    let mut index = open;
    while index < bytes.len() {
        let byte = bytes[index];
        if comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            comment = true;
            index += 2;
            continue;
        }
        if let Some(active_quote) = quote {
            if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == b'{' {
            depth += 1;
        } else if byte == b'}' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}
