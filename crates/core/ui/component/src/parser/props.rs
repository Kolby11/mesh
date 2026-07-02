//! Parser for the `<props>` block — a component's typed, defaulted, localized
//! configuration. See `docs/spec/03-components.md`.
//!
//! Syntax (one entry per `name: { ...fields... }`, commas/newlines separate):
//!
//! ```text
//! <props>
//!   width:   { type: "size", default: "fit-content", label: t("var.width") }
//!   density: { type: "enum", options: ["compact", "cozy"], default: "cozy" }
//!   anim_ms: { type: "duration", default: 120, min: 0, max: 1000 }
//! </props>
//! ```

use crate::{LocalizedLabel, PropDef, PropType, PropValue, PropsBlock, validate_prop_value};

use super::ParseError;

/// A raw, untyped value scanned from the block, before mapping onto typed fields.
#[derive(Debug, Clone)]
enum RawValue {
    Str(String),
    Num(f64),
    Bool(bool),
    Array(Vec<RawValue>),
    /// A function call such as `t("var.width")` — used for localized labels.
    Call {
        name: String,
        args: Vec<RawValue>,
    },
}

struct Scanner<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Scanner<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().peekable(),
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn next(&mut self) -> Option<char> {
        self.chars.next()
    }

    /// Skip whitespace and entry/field separators (commas).
    fn skip_trivia(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == ',' {
                self.next();
            } else {
                break;
            }
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.next();
            } else {
                break;
            }
        }
    }

    fn read_ident(&mut self) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                out.push(c);
                self.next();
            } else {
                break;
            }
        }
        out
    }

    fn read_string(&mut self) -> Result<String, ParseError> {
        // Opening quote already confirmed by the caller.
        let quote = self.next().expect("opening quote");
        let mut out = String::new();
        while let Some(c) = self.next() {
            match c {
                '\\' => {
                    if let Some(escaped) = self.next() {
                        out.push(escaped);
                    }
                }
                c if c == quote => return Ok(out),
                c => out.push(c),
            }
        }
        Err(invalid("unterminated string literal"))
    }

    fn read_number(&mut self) -> Result<f64, ParseError> {
        let mut raw = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' {
                raw.push(c);
                self.next();
            } else {
                break;
            }
        }
        raw.parse::<f64>()
            .map_err(|_| invalid(format!("invalid number `{raw}`")))
    }

    fn read_value(&mut self) -> Result<RawValue, ParseError> {
        self.skip_ws();
        match self.peek() {
            Some('"') | Some('\'') => Ok(RawValue::Str(self.read_string()?)),
            Some('[') => self.read_array(),
            Some(c) if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' => {
                Ok(RawValue::Num(self.read_number()?))
            }
            Some(c) if c.is_alphabetic() => {
                let ident = self.read_ident();
                match ident.as_str() {
                    "true" => Ok(RawValue::Bool(true)),
                    "false" => Ok(RawValue::Bool(false)),
                    _ => {
                        self.skip_ws();
                        if self.peek() == Some('(') {
                            let args = self.read_call_args()?;
                            Ok(RawValue::Call { name: ident, args })
                        } else {
                            // A bare identifier (e.g. an unquoted keyword) is treated
                            // as a string so `default: fit-content` is tolerated.
                            Ok(RawValue::Str(ident))
                        }
                    }
                }
            }
            other => Err(invalid(format!(
                "expected a value, found {}",
                describe(other)
            ))),
        }
    }

    fn read_array(&mut self) -> Result<RawValue, ParseError> {
        self.next(); // consume '['
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(']') => {
                    self.next();
                    return Ok(RawValue::Array(items));
                }
                None => return Err(invalid("unterminated array")),
                _ => items.push(self.read_value()?),
            }
        }
    }

    fn read_call_args(&mut self) -> Result<Vec<RawValue>, ParseError> {
        self.next(); // consume '('
        let mut args = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(')') => {
                    self.next();
                    return Ok(args);
                }
                None => return Err(invalid("unterminated call")),
                _ => args.push(self.read_value()?),
            }
        }
    }

    /// Read a `{ key: value, ... }` object into its field list.
    fn read_object(&mut self) -> Result<Vec<(String, RawValue)>, ParseError> {
        self.skip_ws();
        if self.next() != Some('{') {
            return Err(invalid("expected `{` to open a prop definition"));
        }
        let mut fields = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                Some('}') => {
                    self.next();
                    return Ok(fields);
                }
                None => return Err(invalid("unterminated prop definition (missing `}`)")),
                _ => {}
            }
            let key = self.read_ident();
            if key.is_empty() {
                return Err(invalid("expected a field name inside a prop definition"));
            }
            self.skip_ws();
            if self.next() != Some(':') {
                return Err(invalid(format!("expected `:` after field `{key}`")));
            }
            let value = self.read_value()?;
            fields.push((key, value));
        }
    }
}

pub(super) fn parse_props(source: &str) -> Result<PropsBlock, ParseError> {
    let mut scanner = Scanner::new(source);
    let mut props: Vec<PropDef> = Vec::new();

    loop {
        scanner.skip_trivia();
        if scanner.peek().is_none() {
            break;
        }
        let name = scanner.read_ident();
        if name.is_empty() {
            return Err(invalid(format!(
                "expected a prop name, found {}",
                describe(scanner.peek())
            )));
        }
        scanner.skip_ws();
        if scanner.next() != Some(':') {
            return Err(invalid(format!("expected `:` after prop `{name}`")));
        }
        let fields = scanner.read_object()?;
        if props.iter().any(|p| p.name == name) {
            return Err(invalid(format!("duplicate prop `{name}`")));
        }
        props.push(build_prop(name, fields)?);
    }

    Ok(PropsBlock { props })
}

fn build_prop(name: String, fields: Vec<(String, RawValue)>) -> Result<PropDef, ParseError> {
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

    for (key, value) in fields {
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
            "options" => options = to_string_array(&key, &name, value)?,
            "min" => min = Some(expect_number(&key, &name, &value)?),
            "max" => max = Some(expect_number(&key, &name, &value)?),
            "step" => step = Some(expect_number(&key, &name, &value)?),
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

    if ty == PropType::Enum && options.is_empty() {
        return Err(invalid(format!(
            "enum prop `{name}` requires a non-empty `options` list"
        )));
    }

    let def = PropDef {
        name,
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

    if let Some(default) = &def.default {
        validate_prop_value(&def, default).map_err(|err| {
            invalid(format!(
                "prop `{}` default is invalid for type `{}`: {}",
                def.name,
                def.ty.as_str(),
                err
            ))
        })?;
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

fn describe(c: Option<char>) -> String {
    match c {
        Some(c) => format!("`{c}`"),
        None => "end of block".to_string(),
    }
}

fn invalid(message: impl Into<String>) -> ParseError {
    ParseError::InvalidProps {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_props() {
        let block = parse_props(
            r#"
            width:   { type: "size", default: "fit-content", label: t("var.width") }
            density: { type: "enum", options: ["compact", "cozy"], default: "cozy" }
            show:    { type: "bool", default: true }
            anim_ms: { type: "duration", default: 120, min: 0, max: 1000 }
            "#,
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
    fn accepts_bare_keyword_default() {
        let block = parse_props(r#"width: { type: "size", default: fit-content }"#).unwrap();
        assert_eq!(
            block.props[0].default,
            Some(PropValue::String("fit-content".into()))
        );
    }

    #[test]
    fn label_literal_and_fallback() {
        let block = parse_props(
            r#"
            a: { type: "string", label: "Plain" }
            b: { type: "string", label: t("k.b", "Fallback") }
            "#,
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
        let block = parse_props(r#"x: { type: "size", expose: false }"#).unwrap();
        assert!(!block.props[0].expose);
    }

    #[test]
    fn empty_block_is_ok() {
        let block = parse_props("   \n  ").unwrap();
        assert!(block.props.is_empty());
    }

    #[test]
    fn rejects_missing_type() {
        let err = parse_props(r#"x: { default: 1 }"#).unwrap_err().to_string();
        assert!(err.contains("missing required `type`"), "{err}");
    }

    #[test]
    fn rejects_unknown_type() {
        let err = parse_props(r#"x: { type: "widget" }"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown type `widget`"), "{err}");
    }

    #[test]
    fn rejects_enum_without_options() {
        let err = parse_props(r#"x: { type: "enum" }"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("requires a non-empty `options`"), "{err}");
    }

    #[test]
    fn rejects_duplicate_prop() {
        let err = parse_props(
            r#"
            x: { type: "size" }
            x: { type: "bool" }
            "#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("duplicate prop `x`"), "{err}");
    }

    #[test]
    fn rejects_unknown_field() {
        let err = parse_props(r#"x: { type: "size", widht: 1 }"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown field `widht`"), "{err}");
    }

    #[test]
    fn validates_typed_defaults() {
        let err = parse_props(r#"x: { type: "bool", default: "yes" }"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("default is invalid"), "{err}");

        let err = parse_props(r#"x: { type: "enum", options: ["a"], default: "b" }"#)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not one of"), "{err}");

        parse_props(
            r##"
            color: { type: "color", default: "#ff00aa" }
            token: { type: "token", default: "color-primary" }
            icon: { type: "icon", default: "audio-volume-high" }
            duration: { type: "duration", default: "120ms" }
            "##,
        )
        .unwrap();
    }
}
