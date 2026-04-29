use crate::style::{ContainerQuery, Declaration, Selector, StyleBlock, StyleRule, StyleValue};
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
