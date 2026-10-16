//! Static Luau table declarations for component props.

use super::{ParseError, luau_literal, script::on_parser_stack};
use crate::{
    LocalizedLabel, PropDef, PropType, PropValue, PropsBlock, SourceSpan, normalize_prop_value,
    validate_prop_definition,
};
use full_moon::{
    ast::{
        Call, Expression, Field, FunctionArgs, LastStmt, Prefix, Suffix, TableConstructor, UnOp,
    },
    node::Node,
    tokenizer::{Symbol, TokenType},
};

const EXPRESSION_PREFIX: &str = "return ";

/// Names reserved for runtime layer introspection.
pub const RESERVED_PROP_NAMES: [&str; 2] = ["source", "at"];

#[derive(Debug, Clone)]
enum RawValue {
    Str(String),
    Num(f64),
    Bool(bool),
    Array(Vec<RawValue>),
    Call { name: String, args: Vec<RawValue> },
}

struct DeclarationSource {
    base: usize,
    len: usize,
}

impl DeclarationSource {
    fn span(&self, node: &impl Node) -> SourceSpan {
        let (start, end) = node
            .range()
            .map(|(start, end)| (start.bytes(), end.bytes()))
            .unwrap_or((EXPRESSION_PREFIX.len(), EXPRESSION_PREFIX.len()));
        self.range(start, end)
    }

    fn range(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(
            self.base + start.saturating_sub(EXPRESSION_PREFIX.len()).min(self.len),
            self.base + end.saturating_sub(EXPRESSION_PREFIX.len()).min(self.len),
        )
    }

    fn error(&self, node: &impl Node, message: impl Into<String>) -> ParseError {
        ParseError::InvalidProps {
            message: message.into(),
            span: self.span(node),
        }
    }

    fn named_fields<'a>(
        &self,
        table: &'a TableConstructor,
    ) -> Result<Vec<(String, &'a Expression, SourceSpan)>, ParseError> {
        table
            .fields()
            .iter()
            .map(|field| {
                let (key, value) = match field {
                    Field::NameKey { key, value, .. } => (key.token().to_string(), value),
                    Field::ExpressionKey {
                        key: Expression::String(key),
                        value,
                        ..
                    } => (
                        luau_literal::string(key).map_err(|message| self.error(key, message))?,
                        value,
                    ),
                    _ => {
                        return Err(self.error(
                            field,
                            "prop tables require named fields or literal string keys",
                        ));
                    }
                };
                Ok((key, value, self.span(field)))
            })
            .collect()
    }

    fn value(&self, expression: &Expression, depth: usize) -> Result<RawValue, ParseError> {
        if depth > 64 {
            return Err(self.error(expression, "prop declaration nesting exceeds 64 levels"));
        }
        match expression {
            Expression::String(token) => luau_literal::string(token).map(RawValue::Str).map_err(|message| self.error(token, message)),
            Expression::Number(token) => luau_literal::number(token).map(RawValue::Num).map_err(|message| self.error(token, message)),
            Expression::Symbol(token) => match token.token_type() {
                TokenType::Symbol { symbol: Symbol::True } => Ok(RawValue::Bool(true)),
                TokenType::Symbol { symbol: Symbol::False } => Ok(RawValue::Bool(false)),
                _ => Err(self.error(expression, "props accept only static strings, numbers, booleans, arrays and translation labels")),
            },
            Expression::Parentheses { expression, .. } => self.value(expression, depth + 1),
            Expression::UnaryOperator { unop: UnOp::Minus(_), expression: value } => match self.value(value, depth + 1)? {
                RawValue::Num(value) => Ok(RawValue::Num(-value)),
                _ => Err(self.error(expression, "unary minus requires a numeric literal")),
            },
            Expression::TableConstructor(table) => {
                let mut values = Vec::new();
                for field in table.fields().iter() {
                    let Field::NoKey(value) = field else { return Err(self.error(field, "array values must use sequential Luau table entries")); };
                    values.push(self.value(value, depth + 1)?);
                }
                Ok(RawValue::Array(values))
            }
            Expression::FunctionCall(call) => {
                let Prefix::Name(name) = call.prefix() else { return Err(self.error(expression, "only literal t(...) translation labels are supported")); };
                let suffixes = call.suffixes().collect::<Vec<_>>();
                if name.token().to_string() != "t" || suffixes.len() != 1 { return Err(self.error(expression, "only literal t(...) translation labels are supported")); }
                let args = match suffixes[0] {
                    Suffix::Call(Call::AnonymousCall(FunctionArgs::Parentheses { arguments, .. })) => arguments.iter().map(|arg| self.value(arg, depth + 1)).collect::<Result<Vec<_>, _>>()?,
                    Suffix::Call(Call::AnonymousCall(FunctionArgs::String(token))) => vec![RawValue::Str(luau_literal::string(token).map_err(|message| self.error(token, message))?)],
                    _ => return Err(self.error(expression, "translation labels require literal string arguments")),
                };
                Ok(RawValue::Call { name: "t".into(), args })
            }
            _ => Err(self.error(expression, "prop declarations must be static Luau literals; expressions and identifiers are not evaluated")),
        }
    }
}

#[cfg(test)]
pub(super) fn parse_props(source: &str) -> Result<PropsBlock, ParseError> {
    parse_props_at(source, 0)
}

pub(super) fn parse_props_at(source: &str, source_base: usize) -> Result<PropsBlock, ParseError> {
    let context = DeclarationSource {
        base: source_base,
        len: source.len(),
    };
    if source.trim().is_empty() {
        return Ok(PropsBlock {
            props: Vec::new(),
            span: SourceSpan::new(source_base, source_base + source.len()),
        });
    }
    let wrapped = format!("{EXPRESSION_PREFIX}{source}");
    let ast = on_parser_stack(|| full_moon::parse(&wrapped)).map_err(|errors| {
        let error = &errors[0];
        let (start, end) = error.range();
        ParseError::InvalidProps {
            message: format!(
                "expected a static Luau table (use `name = {{ type = \"size\" }}`): {}",
                error.error_message()
            ),
            span: context.range(start.bytes(), end.bytes()),
        }
    })?;
    let Some(LastStmt::Return(statement)) = ast.nodes().last_stmt() else {
        return Err(invalid("expected a single Luau table literal"));
    };
    if ast.nodes().stmts().next().is_some() || statement.returns().len() != 1 {
        return Err(context.error(statement, "expected a single Luau table literal"));
    }
    let Expression::TableConstructor(table) = statement.returns().iter().next().unwrap() else {
        return Err(context.error(
            statement,
            "expected a Luau table literal enclosed in `{ ... }`",
        ));
    };
    let mut props: Vec<PropDef> = Vec::new();
    for (name, expression, span) in context.named_fields(table)? {
        if props.iter().any(|prop| prop.name == name) {
            return Err(with_span(invalid(format!("duplicate prop `{name}`")), span));
        }
        if RESERVED_PROP_NAMES.contains(&name.as_str()) {
            return Err(with_span(
                invalid(format!(
                    "prop name `{name}` is reserved for `props.{name}(...)` layer introspection"
                )),
                span,
            ));
        }
        let Expression::TableConstructor(fields) = expression else {
            return Err(context.error(expression, "prop definitions must be Luau table literals"));
        };
        let fields = context
            .named_fields(fields)?
            .into_iter()
            .map(|(name, value, _)| Ok((name, context.value(value, 0)?)))
            .collect::<Result<Vec<_>, ParseError>>()?;
        props.push(build_prop(name, fields, span).map_err(|error| with_span(error, span))?);
    }
    Ok(PropsBlock {
        props,
        span: SourceSpan::new(source_base, source_base + source.len()),
    })
}

fn build_prop(
    name: String,
    fields: Vec<(String, RawValue)>,
    span: SourceSpan,
) -> Result<PropDef, ParseError> {
    let mut ty: Option<PropType> = None;
    let mut default: Option<PropValue> = None;
    let mut label: Option<LocalizedLabel> = None;
    let mut description: Option<LocalizedLabel> = None;
    let mut options: Vec<String> = Vec::new();
    let mut min: Option<f64> = None;
    let mut max: Option<f64> = None;
    let mut step: Option<f64> = None;
    let mut unit: Option<String> = None;
    let mut expose = true;
    let mut seen_fields = Vec::new();
    let mut options_provided = false;
    let mut min_provided = false;
    let mut max_provided = false;
    let mut step_provided = false;

    for (key, value) in fields {
        if seen_fields.iter().any(|field| field == &key) {
            return Err(invalid(format!(
                "prop `{name}` has duplicate field `{key}`"
            )));
        }
        seen_fields.push(key.clone());
        match key.as_str() {
            "type" => {
                let raw = expect_string(&key, &name, &value)?;
                ty =
                    Some(PropType::from_str(&raw).ok_or_else(|| {
                        invalid(format!("prop `{name}` has unknown type `{raw}`"))
                    })?);
            }
            "default" => default = Some(to_prop_value(&key, &name, value)?),
            "label" => label = Some(to_label(&key, &name, value)?),
            "description" => description = Some(to_label(&key, &name, value)?),
            "options" => {
                options_provided = true;
                options = to_string_array(&key, &name, value)?;
            }
            "min" => {
                min_provided = true;
                min = Some(expect_number(&key, &name, &value)?);
            }
            "max" => {
                max_provided = true;
                max = Some(expect_number(&key, &name, &value)?);
            }
            "step" => {
                step_provided = true;
                step = Some(expect_number(&key, &name, &value)?);
            }
            "unit" => unit = Some(expect_string(&key, &name, &value)?),
            "expose" => expose = expect_bool(&key, &name, &value)?,
            other => {
                return Err(invalid(format!(
                    "prop `{name}` has unknown field `{other}`"
                )));
            }
        }
    }

    let ty = ty.ok_or_else(|| invalid(format!("prop `{name}` is missing required `type`")))?;

    if options_provided && ty != PropType::Enum {
        return Err(invalid(format!(
            "prop `{name}` field `options` is only valid for enum props"
        )));
    }
    if (min_provided || max_provided || step_provided)
        && !matches!(
            ty,
            PropType::Size | PropType::Number | PropType::Int | PropType::Duration
        )
    {
        return Err(invalid(format!(
            "prop `{name}` fields `min`, `max`, and `step` are only valid for size, number, int, or duration props"
        )));
    }
    if ty == PropType::Enum && options.is_empty() {
        return Err(invalid(format!(
            "enum prop `{name}` requires a non-empty `options` list"
        )));
    }

    let mut def = PropDef {
        name,
        span,
        ty,
        default,
        label,
        description,
        options,
        min,
        max,
        step,
        unit,
        expose,
    };

    if let Some(unit) = &mut def.unit {
        *unit = unit.trim().to_ascii_lowercase();
    }

    validate_prop_definition(&def).map_err(|err| invalid(err.message))?;

    if let Some(default) = def.default.take() {
        def.default = Some(normalize_prop_value(&def, default).map_err(|err| {
            invalid(format!(
                "prop `{}` default is invalid for type `{}`: {}",
                def.name,
                def.ty.as_str(),
                err
            ))
        })?);
    }

    Ok(def)
}

fn to_prop_value(field: &str, prop: &str, value: RawValue) -> Result<PropValue, ParseError> {
    match value {
        RawValue::Str(s) => Ok(PropValue::String(s)),
        RawValue::Num(n) => Ok(PropValue::Number(n)),
        RawValue::Bool(b) => Ok(PropValue::Bool(b)),
        _ => Err(field_type_error(
            field,
            prop,
            "a string, number, or boolean",
        )),
    }
}

fn to_label(field: &str, prop: &str, value: RawValue) -> Result<LocalizedLabel, ParseError> {
    match value {
        RawValue::Str(s) => Ok(LocalizedLabel::Literal(s)),
        RawValue::Call { name, args } if name == "t" => {
            if !(1..=2).contains(&args.len()) {
                return Err(invalid(format!(
                    "prop `{prop}` field `{field}`: t(...) needs one string key and an optional string fallback"
                )));
            }
            let mut it = args.into_iter();
            let key = match it.next() {
                Some(RawValue::Str(s)) => s,
                _ => {
                    return Err(invalid(format!(
                        "prop `{prop}` field `{field}`: t(...) needs a string key"
                    )));
                }
            };
            let fallback = match it.next() {
                Some(RawValue::Str(s)) => Some(s),
                Some(_) => {
                    return Err(invalid(format!(
                        "prop `{prop}` field `{field}`: t(...) fallback must be a string"
                    )));
                }
                None => None,
            };
            Ok(LocalizedLabel::Translation { key, fallback })
        }
        _ => Err(field_type_error(
            field,
            prop,
            "a string literal or t(\"key\"[, \"fallback\"])",
        )),
    }
}

fn to_string_array(field: &str, prop: &str, value: RawValue) -> Result<Vec<String>, ParseError> {
    match value {
        RawValue::Array(items) => items
            .into_iter()
            .map(|item| match item {
                RawValue::Str(s) => Ok(s),
                _ => Err(field_type_error(field, prop, "an array of strings")),
            })
            .collect(),
        _ => Err(field_type_error(field, prop, "an array of strings")),
    }
}

fn expect_string(field: &str, prop: &str, value: &RawValue) -> Result<String, ParseError> {
    match value {
        RawValue::Str(s) => Ok(s.clone()),
        _ => Err(field_type_error(field, prop, "a string")),
    }
}

fn expect_number(field: &str, prop: &str, value: &RawValue) -> Result<f64, ParseError> {
    match value {
        RawValue::Num(n) => Ok(*n),
        _ => Err(field_type_error(field, prop, "a number")),
    }
}

fn expect_bool(field: &str, prop: &str, value: &RawValue) -> Result<bool, ParseError> {
    match value {
        RawValue::Bool(b) => Ok(*b),
        _ => Err(field_type_error(field, prop, "a boolean")),
    }
}

fn field_type_error(field: &str, prop: &str, expected: &str) -> ParseError {
    invalid(format!("prop `{prop}` field `{field}` must be {expected}"))
}

fn invalid(message: impl Into<String>) -> ParseError {
    ParseError::InvalidProps {
        message: message.into(),
        span: SourceSpan::default(),
    }
}

fn with_span(error: ParseError, span: SourceSpan) -> ParseError {
    match error {
        ParseError::InvalidProps { message, .. } => ParseError::InvalidProps { message, span },
        error => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luau_tables_support_comments_string_keys_and_numeric_literals() {
        let block = parse_props(
            r#"{
            -- Literal keys share the same namespace as identifier keys.
            ["width"] = { type = "number"; default = -0x1f; min = -40 },
            density = { type = "enum", options = { 'compact'; 'cozy' }, default = 'cozy' },
            count = { type = "int", default = 0b1010 },
        }"#,
        )
        .unwrap();
        assert_eq!(block.props[0].default, Some(PropValue::Number(-31.0)));
        assert_eq!(block.props[1].options, ["compact", "cozy"]);
        assert_eq!(block.props[2].default, Some(PropValue::Number(10.0)));
    }

    #[test]
    fn rejects_dynamic_expressions_and_legacy_props_syntax() {
        for source in [
            r#"x: { type: "bool", default: true }"#,
            r#"{ x = create_prop() }"#,
            r#"{ x = { type = "number", default = os.time() } }"#,
            r#"{ x = { type = "number", default = 1 + 2 } }"#,
            r#"{ x = { type = "bool", default = if true then false else true } }"#,
            r#"{ x = { type = "string", default = `interpolated` } }"#,
            r#"{ x = { type = "string", default = nil } }"#,
            r#"{ x = { type = "string", label = t("one", "two", "three") } }"#,
            r#"{ x = { type = "enum", options = {[1] = "a"} } }"#,
            r#"{}; execute()"#,
        ] {
            assert!(parse_props(source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn escaped_duplicate_keys_and_value_errors_keep_source_spans() {
        let source = r#"{ x = { type = "bool" }, ["\120"] = { type = "bool" } }"#;
        assert!(
            parse_props(source)
                .unwrap_err()
                .to_string()
                .contains("duplicate prop `x`")
        );
        let source = r#"{ ["é"] = { type = "number", default = unknown } }"#;
        let error = parse_props_at(source, 123).unwrap_err();
        let span = error.span();
        assert_eq!(&source[span.start - 123..span.end - 123], "unknown");
        let syntax = r#"{ ["é"] = { type: "number" } }"#;
        let error = parse_props_at(syntax, 123).unwrap_err();
        assert!(error.to_string().contains("static Luau table"));
        assert!(syntax.is_char_boundary(error.span().start - 123));
        assert!(syntax.is_char_boundary(error.span().end - 123));
    }

    #[test]
    fn parses_basic_props() {
        let block = parse_props(
            r#"{
            width = { type = "size", default = "fit-content", label = t("var.width") },
            density = { type = "enum", options = {"compact", "cozy"}, default = "cozy" },
            show = { type = "bool", default = true },
            anim_ms = { type = "duration", default = 120, min = 0, max = 1000 },
            }"#,
        )
        .unwrap();

        assert_eq!(block.props.len(), 4);

        let width = &block.props[0];
        assert_eq!(width.name, "width");
        assert_eq!(width.ty, PropType::Size);
        assert_eq!(width.default, Some(PropValue::String("fit-content".into())));
        assert_eq!(
            width.label,
            Some(LocalizedLabel::Translation {
                key: "var.width".into(),
                fallback: None
            })
        );
        assert!(width.expose);

        let density = &block.props[1];
        assert_eq!(density.ty, PropType::Enum);
        assert_eq!(density.options, vec!["compact", "cozy"]);
        assert_eq!(density.default, Some(PropValue::String("cozy".into())));

        let show = &block.props[2];
        assert_eq!(show.default, Some(PropValue::Bool(true)));

        let anim = &block.props[3];
        assert_eq!(anim.default, Some(PropValue::Number(120.0)));
        assert_eq!(anim.min, Some(0.0));
        assert_eq!(anim.max, Some(1000.0));
    }

    #[test]
    fn rejects_bare_keyword_default() {
        let error =
            parse_props(r#"{ width = { type = "size", default = fit_content } }"#).unwrap_err();
        assert!(error.to_string().contains("static Luau literals"));
    }

    #[test]
    fn label_literal_and_fallback() {
        let block = parse_props(
            r#"{
            a = { type = "string", label = "Plain" },
            b = { type = "string", label = t("k.b", "Fallback") },
            }"#,
        )
        .unwrap();
        assert_eq!(
            block.props[0].label,
            Some(LocalizedLabel::Literal("Plain".into()))
        );
        assert_eq!(
            block.props[1].label,
            Some(LocalizedLabel::Translation {
                key: "k.b".into(),
                fallback: Some("Fallback".into())
            })
        );
    }

    #[test]
    fn expose_false_is_honored() {
        let block = parse_props(r#"{x = { type = "size", expose = false },}"#).unwrap();
        assert!(!block.props[0].expose);
    }

    #[test]
    fn empty_block_is_ok() {
        let block = parse_props("   \n  ").unwrap();
        assert!(block.props.is_empty());
    }

    #[test]
    fn rejects_missing_type() {
        let err = parse_props(r#"{x = { default = 1 },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing required `type`"), "{err}");
    }

    #[test]
    fn rejects_unknown_type() {
        let err = parse_props(r#"{x = { type = "widget" },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown type `widget`"), "{err}");
    }

    #[test]
    fn rejects_enum_without_options() {
        let err = parse_props(r#"{x = { type = "enum" },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("requires a non-empty `options`"), "{err}");
    }

    #[test]
    fn rejects_reserved_introspection_names() {
        for name in RESERVED_PROP_NAMES {
            let err = parse_props(&format!(r#"{{ {name} = {{ type = "bool" }} }}"#))
                .unwrap_err()
                .to_string();
            assert!(err.contains("is reserved"), "{err}");
        }
    }

    #[test]
    fn rejects_duplicate_prop() {
        let err = parse_props(
            r#"{
            x = { type = "size" },
            x = { type = "bool" },
            }"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("duplicate prop `x`"), "{err}");
    }

    #[test]
    fn rejects_unknown_field() {
        let err = parse_props(r#"{x = { type = "size", widht = 1 },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown field `widht`"), "{err}");
    }

    #[test]
    fn validates_typed_defaults() {
        let err = parse_props(r#"{x = { type = "bool", default = "yes" },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("default is invalid"), "{err}");

        let err = parse_props(r#"{x = { type = "enum", options = {"a"}, default = "b" },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not one of"), "{err}");

        parse_props(
            r##"{
            color = { type = "color", default = "#ff00aa" },
            token = { type = "token", default = "color-primary" },
            icon = { type = "icon", default = "audio-volume-high" },
            duration = { type = "duration", default = "120ms" },
            }"##,
        )
        .unwrap();
    }

    #[test]
    fn rejects_duplicate_fields() {
        let err = parse_props(r#"{x = { type = "size", type = "number" },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("duplicate field `type`"), "{err}");
    }

    #[test]
    fn rejects_type_inapplicable_metadata() {
        for (source, expected) in [
            (
                r#"{x = { type = "bool", options = {} },}"#,
                "`options` is only valid for enum props",
            ),
            (
                r#"{x = { type = "string", min = 0 },}"#,
                "only valid for size, number, int, or duration props",
            ),
            (
                r#"{x = { type = "color", unit = "px" },}"#,
                "unit `px` is not valid for `color` props",
            ),
        ] {
            let err = parse_props(source).unwrap_err().to_string();
            assert!(err.contains(expected), "{err}");
        }
    }

    #[test]
    fn rejects_invalid_constraints() {
        let err = parse_props(r#"{x = { type = "number", min = 10, max = 1 },}"#)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("`min` must not be greater than `max`"),
            "{err}"
        );

        let err = parse_props(r#"{x = { type = "number", step = 0 },}"#)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("`step` must be finite and greater than zero"),
            "{err}"
        );

        let err = parse_props(r#"{x = { type = "int", step = 0.5 },}"#)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("int prop `step` must be a whole number"),
            "{err}"
        );
    }

    #[test]
    fn rejects_duplicate_enum_options() {
        let err = parse_props(r#"{x = { type = "enum", options = {"compact", "compact"} },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("enum option `compact` is duplicated"), "{err}");
    }

    #[test]
    fn validates_css_domain_values_with_the_css_parser() {
        let err = parse_props(r##"{x = { type = "token", default = "" },}"##)
            .unwrap_err()
            .to_string();
        assert!(err.contains("default is invalid"), "{err}");

        let err = parse_props(r#"{x = { type = "color", default = "rgb(255, 0)" },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("default is invalid"), "{err}");

        let err = parse_props(r#"{x = { type = "size", default = "12watts" },}"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("default is invalid"), "{err}");

        parse_props(
            r##"{
            size = { type = "size", default = "1cm" },
            color = { type = "color", default = "rebeccapurple" },
            token = { type = "token", default = "module.surface" },
            }"##,
        )
        .unwrap();
    }

    #[test]
    fn normalizes_and_validates_units() {
        let block =
            parse_props(r#"{x = { type = "number", unit = " PX ", default = 2 },}"#).unwrap();
        assert_eq!(block.props[0].unit.as_deref(), Some("px"));

        let err = parse_props(r#"{x = { type = "size", unit = "watts" },}"#)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("unit `watts` is not valid for `size` props"),
            "{err}"
        );

        let err = parse_props(r#"{x = { type = "duration", unit = "ms" },}"#)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("unit `ms` is not valid for `duration` props"),
            "{err}"
        );
    }

    #[test]
    fn public_prop_schema_is_normalized_and_excludes_private_declarations() {
        let block = parse_props(
            r#"{
            visible = { type = "number", unit = " PX ", min = 1, default = 2 },
            internal = { type = "string", expose = false },
            }"#,
        )
        .unwrap();

        let public = crate::normalized_public_prop_schema(Some(&block));
        assert_eq!(public.len(), 1);
        assert_eq!(public[0].name, "visible");
        assert_eq!(public[0].unit.as_deref(), Some("px"));
        assert_eq!(public[0].min, Some(1.0));
        assert_eq!(public[0].default, Some(PropValue::Number(2.0)));
    }
}
