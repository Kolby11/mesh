//! Cross-block validation for a parsed component contract.
//!
//! The individual component parsers intentionally stay focused on their own
//! languages. This pass is the point where the declarations in `<props>` are
//! joined to the style references that consume them.

use crate::{ComponentFile, PropDef, SourceSpan, style::StyleValue};

use super::{ParseError, styles};

pub(super) fn validate(source: &str, component: &ComponentFile) -> Result<(), ParseError> {
    let declarations = component
        .props
        .as_ref()
        .map(|block| block.props.iter().map(|prop| (prop.name.as_str(), prop)))
        .into_iter()
        .flatten()
        .collect::<std::collections::HashMap<_, _>>();

    let Some(style) = &component.style else {
        return Ok(());
    };
    let style_span = component
        .blocks
        .iter()
        .find(|block| block.name == "style")
        .map(|block| block.content);

    for rule in &style.rules {
        for declaration in &rule.declarations {
            validate_style_value(
                source,
                &declarations,
                style_span,
                &declaration.property,
                &declaration.value,
            )?;
        }
    }
    for keyframe in &style.keyframes {
        for stop in &keyframe.stops {
            for declaration in &stop.declarations {
                validate_style_value(
                    source,
                    &declarations,
                    style_span,
                    &declaration.property,
                    &declaration.value,
                )?;
            }
        }
    }

    Ok(())
}

fn validate_style_value(
    source: &str,
    declarations: &std::collections::HashMap<&str, &PropDef>,
    style_span: Option<SourceSpan>,
    property: &str,
    value: &StyleValue,
) -> Result<(), ParseError> {
    let references = match value {
        StyleValue::Prop(name) => vec![name.clone()],
        StyleValue::Literal(value) => styles::prop_references(value).map_err(|message| {
            semantic_error(
                source,
                find_prop_reference(source, "", style_span).unwrap_or_default(),
                message,
            )
        })?,
        StyleValue::Var(_) => Vec::new(),
    };

    let domain = styles::prop_css_domain(property);
    for name in references {
        let offset = find_prop_reference(source, &name, style_span).unwrap_or_default();
        if name.trim().is_empty() {
            return Err(semantic_error(
                source,
                offset,
                "`prop()` needs a non-empty prop name",
            ));
        }
        if !is_prop_name(&name) {
            return Err(semantic_error(
                source,
                offset,
                format!("`prop({name})` must use an identifier"),
            ));
        }
        let Some(definition) = declarations.get(name.as_str()) else {
            return Err(semantic_error(
                source,
                offset,
                format!("style references undefined prop `{name}`"),
            ));
        };
        if !styles::prop_type_matches(definition.ty, domain) {
            return Err(semantic_error(
                source,
                offset,
                format!(
                    "prop `{name}` has type `{}` but CSS property `{property}` requires a {} value",
                    definition.ty.as_str(),
                    domain.as_str(),
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

fn find_prop_reference(source: &str, name: &str, span: Option<SourceSpan>) -> Option<usize> {
    let needle = format!("prop({name})");
    let (base, source) = span
        .map(|span| (span.start, &source[span.start..span.end]))
        .unwrap_or((0, source));
    source
        .find(&needle)
        .map(|offset| base + offset)
        .or_else(|| source.find("prop(").map(|offset| base + offset))
}

fn semantic_error(source: &str, offset: usize, message: impl Into<String>) -> ParseError {
    let offset = offset.min(source.len());
    let line = source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = source[..offset]
        .rsplit('\n')
        .next()
        .map_or(1, |line| line.chars().count() + 1);
    ParseError::InvalidSemantics {
        message: message.into(),
        line,
        column,
    }
}
