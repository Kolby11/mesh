pub mod parser;
pub mod style;
/// Single-file component parser for `.mesh` files.
///
/// A `.mesh` file contains these blocks:
///
/// ```text
/// <template>  — XHTML-like markup
/// <script lang="luau"> — Luau logic
/// <style>     — CSS-like styling with theme token references
/// ```
///
/// This crate parses these blocks into a typed AST. It has no runtime
/// dependencies — it does not depend on mesh-core-theme, mesh-core-service, or
/// any other mesh crate.
pub mod template;

pub use parser::{
    ParseError, parse_component, parse_inline_style, parse_luau_script, referenced_identifiers,
};
pub use style::*;
pub use template::*;

use lightningcss::{
    properties::size::Size,
    traits::Parse,
    values::{
        angle::Angle,
        color::CssColor,
        ident::{CustomIdent, DashedIdent},
        length::LengthValue,
        percentage::Percentage,
        resolution::Resolution,
        time::Time,
    },
};

pub use mesh_core_expression::{
    CompiledExpression, ExpressionCompileError, ExpressionEvaluationError,
    SharedCompiledExpression, compile_expression,
};

/// A parsed authoring-time import from a `.mesh` script block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentImport {
    pub alias: String,
    pub target: ComponentImportTarget,
    /// The complete import statement in the owning script block.
    pub span: SourceSpan,
    /// The imported local name.
    pub alias_span: SourceSpan,
    /// The quoted import target, including its quotes when authored that way.
    pub target_span: SourceSpan,
}

/// Supported explicit import targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentImportTarget {
    /// A local component file, either relative to the importing file or `@src/...`.
    ComponentLocal(String),
    /// A frontend module ID, such as `@mesh/volume-bar`.
    ComponentModule(String),
    /// A MESH interface API, such as `mesh.audio` with an optional version requirement.
    InterfaceApi {
        interface: String,
        version: Option<String>,
    },
}

/// A parsed `.mesh` single-file component.
#[derive(Debug, Clone)]
pub struct ComponentFile {
    /// The validated top-level block sequence, in source order.
    ///
    /// The parsed block ASTs below intentionally remain specialized, but this
    /// metadata keeps their source ownership and ordering available to
    /// compiler/tooling consumers without re-scanning the source text.
    pub blocks: Vec<ComponentBlock>,
    pub imports: Vec<ComponentImport>,
    pub props: Option<PropsBlock>,
    pub template: Option<TemplateBlock>,
    pub script: Option<ScriptBlock>,
    pub style: Option<StyleBlock>,
    /// Every template expression, deduplicated and compiled once while the
    /// component is parsed. Renderers and the Luau runtime share these values.
    pub template_expressions: Vec<SharedCompiledExpression>,
}

/// A half-open byte range into the original `.mesh` source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// The typed source category assigned to parser diagnostics.
///
/// This deliberately lives beside the AST rather than in the runtime
/// diagnostics crate: component parsing is a low-level authoring operation and
/// must remain usable by compiler and tooling consumers without runtime
/// dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseDiagnosticCategory {
    Syntax,
    Template,
    Style,
    Props,
    Semantics,
    I18n,
    Import,
}

impl ParseDiagnosticCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Template => "template",
            Self::Style => "style",
            Self::Props => "props",
            Self::Semantics => "semantics",
            Self::I18n => "i18n",
            Self::Import => "import",
        }
    }
}

impl SourceSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// A validated attribute on a top-level component block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAttribute {
    pub name: String,
    pub value: String,
    /// The range of the complete attribute, including its name and value.
    pub span: SourceSpan,
    /// The range of the value without its surrounding quotes.
    pub value_span: SourceSpan,
}

/// Source metadata for one top-level `.mesh` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentBlock {
    pub name: String,
    pub attributes: Vec<BlockAttribute>,
    /// The complete block, from the opening `<` through the closing `>`.
    pub span: SourceSpan,
    pub open_tag: SourceSpan,
    pub content: SourceSpan,
    pub close_tag: SourceSpan,
}

/// A parsed `<props>` block: the component's typed, defaulted configuration.
///
/// Each entry auto-projects to a `prop(name)` CSS reference, a reactive
/// `props.name` script field, and a generated settings-UI row. See
/// `docs/spec/03-components.md`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropsBlock {
    pub props: Vec<PropDef>,
    pub span: SourceSpan,
}

/// A single declared prop.
#[derive(Debug, Clone, PartialEq)]
pub struct PropDef {
    pub name: String,
    /// The complete `name: { ... }` declaration.
    pub span: SourceSpan,
    pub ty: PropType,
    pub default: Option<PropValue>,
    pub label: Option<LocalizedLabel>,
    pub description: Option<LocalizedLabel>,
    /// Allowed values for `enum` props.
    pub options: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub unit: Option<String>,
    /// Whether the prop appears in the generated settings UI (default `true`).
    pub expose: bool,
}

/// The validated value domain of a prop. Drives CSS projection, the Lua value
/// kind, the generated settings control, and use-site type checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropType {
    Size,
    Number,
    Int,
    Bool,
    Enum,
    String,
    Color,
    Token,
    Duration,
    Icon,
}

impl PropType {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "size" => Self::Size,
            "number" => Self::Number,
            "int" => Self::Int,
            "bool" => Self::Bool,
            "enum" => Self::Enum,
            "string" => Self::String,
            "color" => Self::Color,
            "token" => Self::Token,
            "duration" => Self::Duration,
            "icon" => Self::Icon,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Number => "number",
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Enum => "enum",
            Self::String => "string",
            Self::Color => "color",
            Self::Token => "token",
            Self::Duration => "duration",
            Self::Icon => "icon",
        }
    }

    pub fn lua_type(self) -> &'static str {
        match self {
            Self::Number | Self::Int | Self::Duration => "number",
            Self::Bool => "boolean",
            Self::Size | Self::Enum | Self::String | Self::Color | Self::Token | Self::Icon => {
                "string"
            }
        }
    }
}

/// A scalar prop value (used for `default`).
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    String(String),
    Number(f64),
    Bool(bool),
}

impl PropValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Number(_) => "number",
            Self::Bool(_) => "boolean",
        }
    }
}

/// JSON values that cannot cross the scalar prop boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JsonPropValueError {
    #[error("null is not a scalar prop value")]
    Null,
    #[error(
        "array values are not supported as props until an explicit structured prop type exists"
    )]
    Array,
    #[error(
        "object values are not supported as props until an explicit structured prop type exists"
    )]
    Object,
    #[error("number cannot be represented as a finite prop number")]
    InvalidNumber,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropValidationError {
    pub message: String,
}

impl std::fmt::Display for PropValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PropValidationError {}

/// Validate the metadata of one normalized prop definition.
///
/// The parser calls this once after collecting and normalizing the declaration;
/// runtime value checks then only need to validate the incoming scalar value.
pub fn validate_prop_definition(def: &PropDef) -> Result<(), PropValidationError> {
    if def.ty == PropType::Enum && def.options.is_empty() {
        return Err(definition_error(
            def,
            "enum props require a non-empty `options` list",
        ));
    }
    if def.ty != PropType::Enum && !def.options.is_empty() {
        return Err(definition_error(
            def,
            "`options` is only valid for enum props",
        ));
    }

    for (index, option) in def.options.iter().enumerate() {
        if option.trim().is_empty() {
            return Err(definition_error(def, "enum options must not be empty"));
        }
        if option != option.trim() || CustomIdent::parse_string(option).is_err() {
            return Err(definition_error(
                def,
                &format!("enum option `{option}` is not a valid CSS identifier"),
            ));
        }
        if def.options[..index]
            .iter()
            .any(|previous| previous == option)
        {
            return Err(definition_error(
                def,
                &format!("enum option `{option}` is duplicated"),
            ));
        }
    }

    let has_numeric_constraints = def.min.is_some() || def.max.is_some() || def.step.is_some();
    if has_numeric_constraints && !supports_numeric_constraints(def.ty) {
        return Err(definition_error(
            def,
            "`min`, `max`, and `step` are only valid for size, number, int, or duration props",
        ));
    }

    if let Some(min) = def.min
        && !min.is_finite()
    {
        return Err(definition_error(def, "`min` must be finite"));
    }
    if let Some(max) = def.max
        && !max.is_finite()
    {
        return Err(definition_error(def, "`max` must be finite"));
    }
    if let (Some(min), Some(max)) = (def.min, def.max)
        && min > max
    {
        return Err(definition_error(
            def,
            "`min` must not be greater than `max`",
        ));
    }
    if let Some(step) = def.step {
        if !step.is_finite() || step <= 0.0 {
            return Err(definition_error(
                def,
                "`step` must be finite and greater than zero",
            ));
        }
        if def.ty == PropType::Int && step.fract() != 0.0 {
            return Err(definition_error(
                def,
                "int prop `step` must be a whole number",
            ));
        }
    }

    if let Some(unit) = &def.unit {
        if !is_valid_prop_unit(def.ty, unit) {
            return Err(definition_error(
                def,
                &format!("unit `{unit}` is not valid for `{}` props", def.ty.as_str()),
            ));
        }
    }

    Ok(())
}

fn definition_error(def: &PropDef, message: &str) -> PropValidationError {
    PropValidationError {
        message: format!("prop `{}` {message}", def.name),
    }
}

fn supports_numeric_constraints(ty: PropType) -> bool {
    matches!(
        ty,
        PropType::Size | PropType::Number | PropType::Int | PropType::Duration
    )
}

fn is_valid_prop_unit(ty: PropType, unit: &str) -> bool {
    let unit = unit.trim();
    if unit.is_empty() || unit.chars().any(char::is_whitespace) {
        return false;
    }

    let candidate = format!("1{unit}");
    match ty {
        PropType::Size => Size::parse_string(&candidate).is_ok(),
        PropType::Number | PropType::Int => {
            LengthValue::parse_string(&candidate).is_ok()
                || Percentage::parse_string(&candidate).is_ok()
                || Angle::parse_string(&candidate).is_ok()
                || Time::parse_string(&candidate).is_ok()
                || Resolution::parse_string(&candidate).is_ok()
        }
        _ => false,
    }
}

pub fn validate_prop_value(def: &PropDef, value: &PropValue) -> Result<(), PropValidationError> {
    match def.ty {
        PropType::Size => validate_size_prop(def, value),
        PropType::Number => validate_number_prop(def, value),
        PropType::Int => validate_int_prop(def, value),
        PropType::Bool => match value {
            PropValue::Bool(_) => Ok(()),
            _ => Err(type_error(def, "boolean", value)),
        },
        PropType::Enum => match value {
            PropValue::String(value) if def.options.iter().any(|option| option == value) => Ok(()),
            PropValue::String(value) => Err(PropValidationError {
                message: format!(
                    "prop `{}` enum value `{}` is not one of `{}`",
                    def.name,
                    value,
                    def.options.join("`, `")
                ),
            }),
            _ => Err(type_error(def, "string enum value", value)),
        },
        PropType::String => match value {
            PropValue::String(_) => Ok(()),
            _ => Err(type_error(def, "string", value)),
        },
        PropType::Color => validate_color_prop(def, value),
        PropType::Token => validate_token_prop(def, value),
        PropType::Duration => validate_duration_prop(def, value),
        PropType::Icon => validate_icon_prop(def, value),
    }
}

pub fn prop_value_to_css(def: &PropDef, value: &PropValue) -> Result<String, PropValidationError> {
    validate_prop_value(def, value)?;
    Ok(match (def.ty, value) {
        (PropType::Bool, PropValue::Bool(value)) => if *value { "1" } else { "0" }.to_string(),
        (PropType::Duration, PropValue::Number(value)) => {
            format!("{}ms", format_prop_number(*value))
        }
        (PropType::Duration, PropValue::String(value)) => {
            if value.trim().parse::<f64>().is_ok() {
                format!("{}ms", value.trim())
            } else {
                value.clone()
            }
        }
        (PropType::Int, PropValue::Number(value)) => format!("{}", *value as i64),
        (_, PropValue::String(value)) => value.clone(),
        (_, PropValue::Number(value)) => {
            let unit = def.unit.as_deref().unwrap_or("");
            format!("{}{}", format_prop_number(*value), unit)
        }
        (_, PropValue::Bool(value)) => if *value { "1" } else { "0" }.to_string(),
    })
}

pub fn prop_value_to_json(value: &PropValue) -> serde_json::Value {
    match value {
        PropValue::String(s) => serde_json::Value::String(s.clone()),
        PropValue::Number(n) => serde_json::json!(n),
        PropValue::Bool(b) => serde_json::Value::Bool(*b),
    }
}

/// Derive the settings schema projected by a component's exposed props.
///
/// This lives with the prop grammar so graph discovery, the compiler, the LSP,
/// and settings UI cannot grow separate schema translations.
pub fn props_settings_schema(block: Option<&PropsBlock>) -> Option<serde_json::Value> {
    let block = block?;
    let mut properties = serde_json::Map::new();
    for def in &block.props {
        if !def.expose {
            continue;
        }
        let mut field = serde_json::Map::new();
        field.insert(
            "type".into(),
            serde_json::Value::String(def.ty.as_str().into()),
        );
        if let Some(default) = &def.default {
            field.insert("default".into(), prop_value_to_json(default));
        }
        if let Some(label) = &def.label {
            field.insert("label".into(), localized_label_to_json(label));
        }
        if let Some(description) = &def.description {
            field.insert("description".into(), localized_label_to_json(description));
        }
        if !def.options.is_empty() {
            field.insert(
                "enum".into(),
                serde_json::Value::Array(
                    def.options
                        .iter()
                        .map(|option| serde_json::Value::String(option.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(min) = def.min {
            field.insert("minimum".into(), serde_json::json!(min));
        }
        if let Some(max) = def.max {
            field.insert("maximum".into(), serde_json::json!(max));
        }
        if let Some(step) = def.step {
            field.insert("step".into(), serde_json::json!(step));
        }
        if let Some(unit) = &def.unit {
            field.insert("unit".into(), serde_json::Value::String(unit.clone()));
        }
        properties.insert(def.name.clone(), serde_json::Value::Object(field));
    }
    (!properties.is_empty())
        .then(|| serde_json::json!({ "type": "object", "properties": properties }))
}

/// Return the normalized public prop declarations exposed by a component.
///
/// The parser has already applied defaults and validated the complete
/// declaration by the time this projection is requested. Keeping the public
/// projection as typed `PropDef` values lets compiler and host import
/// boundaries share the same value grammar without reparsing settings JSON.
pub fn normalized_public_prop_schema(block: Option<&PropsBlock>) -> Vec<PropDef> {
    block
        .map(|block| {
            block
                .props
                .iter()
                .filter(|definition| definition.expose)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn localized_label_to_json(label: &LocalizedLabel) -> serde_json::Value {
    match label {
        LocalizedLabel::Literal(text) => serde_json::Value::String(text.clone()),
        LocalizedLabel::Translation { key, fallback } => {
            let mut object = serde_json::Map::new();
            object.insert("t".into(), serde_json::Value::String(key.clone()));
            if let Some(fallback) = fallback {
                object.insert(
                    "fallback".into(),
                    serde_json::Value::String(fallback.clone()),
                );
            }
            serde_json::Value::Object(object)
        }
    }
}

pub fn json_to_prop_value(value: serde_json::Value) -> Result<PropValue, JsonPropValueError> {
    match value {
        serde_json::Value::String(s) => Ok(PropValue::String(s)),
        serde_json::Value::Number(n) => n
            .as_f64()
            .map(PropValue::Number)
            .ok_or(JsonPropValueError::InvalidNumber),
        serde_json::Value::Bool(b) => Ok(PropValue::Bool(b)),
        serde_json::Value::Null => Err(JsonPropValueError::Null),
        serde_json::Value::Array(_) => Err(JsonPropValueError::Array),
        serde_json::Value::Object(_) => Err(JsonPropValueError::Object),
    }
}

/// Convert a JSON value to the scalar prop domain without taking ownership.
pub fn json_to_prop_value_ref(value: &serde_json::Value) -> Result<PropValue, JsonPropValueError> {
    match value {
        serde_json::Value::String(value) => Ok(PropValue::String(value.clone())),
        serde_json::Value::Number(value) => value
            .as_f64()
            .map(PropValue::Number)
            .ok_or(JsonPropValueError::InvalidNumber),
        serde_json::Value::Bool(value) => Ok(PropValue::Bool(*value)),
        serde_json::Value::Null => Err(JsonPropValueError::Null),
        serde_json::Value::Array(_) => Err(JsonPropValueError::Array),
        serde_json::Value::Object(_) => Err(JsonPropValueError::Object),
    }
}

fn validate_size_prop(def: &PropDef, value: &PropValue) -> Result<(), PropValidationError> {
    match value {
        PropValue::Number(n) => validate_numeric_bounds(def, *n),
        PropValue::String(value) if is_css_size_value(value) => Ok(()),
        PropValue::String(value) => Err(PropValidationError {
            message: format!(
                "prop `{}` size value `{value}` is not a valid CSS size",
                def.name
            ),
        }),
        _ => Err(type_error(def, "CSS size string or number", value)),
    }
}

fn validate_number_prop(def: &PropDef, value: &PropValue) -> Result<(), PropValidationError> {
    match value {
        PropValue::Number(n) => validate_numeric_bounds(def, *n),
        _ => Err(type_error(def, "number", value)),
    }
}

fn validate_int_prop(def: &PropDef, value: &PropValue) -> Result<(), PropValidationError> {
    match value {
        PropValue::Number(n) if n.fract() == 0.0 => validate_numeric_bounds(def, *n),
        PropValue::Number(n) => Err(PropValidationError {
            message: format!("prop `{}` int value `{n}` must be a whole number", def.name),
        }),
        _ => Err(type_error(def, "integer", value)),
    }
}

fn validate_color_prop(def: &PropDef, value: &PropValue) -> Result<(), PropValidationError> {
    match value {
        PropValue::String(value) if is_css_color_value(value) => Ok(()),
        PropValue::String(value) => Err(PropValidationError {
            message: format!(
                "prop `{}` color value `{value}` is not a valid color",
                def.name
            ),
        }),
        _ => Err(type_error(def, "color string", value)),
    }
}

fn validate_token_prop(def: &PropDef, value: &PropValue) -> Result<(), PropValidationError> {
    match value {
        PropValue::String(value) if is_token_value(value) => Ok(()),
        PropValue::String(value) => Err(PropValidationError {
            message: format!(
                "prop `{}` token value `{value}` is not a valid theme token reference",
                def.name
            ),
        }),
        _ => Err(type_error(def, "theme token string", value)),
    }
}

fn validate_duration_prop(def: &PropDef, value: &PropValue) -> Result<(), PropValidationError> {
    match value {
        PropValue::Number(n) => validate_numeric_bounds(def, *n),
        PropValue::String(value) if parse_duration_ms(value).is_some() => Ok(()),
        PropValue::String(value) => Err(PropValidationError {
            message: format!(
                "prop `{}` duration value `{value}` must be a number or `<n>ms`",
                def.name
            ),
        }),
        _ => Err(type_error(def, "duration number or string", value)),
    }
}

fn validate_icon_prop(def: &PropDef, value: &PropValue) -> Result<(), PropValidationError> {
    match value {
        PropValue::String(value) if is_icon_name(value) => Ok(()),
        PropValue::String(value) => Err(PropValidationError {
            message: format!(
                "prop `{}` icon value `{value}` is not a valid logical icon name",
                def.name
            ),
        }),
        _ => Err(type_error(def, "icon name string", value)),
    }
}

fn validate_numeric_bounds(def: &PropDef, value: f64) -> Result<(), PropValidationError> {
    if !value.is_finite() {
        return Err(PropValidationError {
            message: format!("prop `{}` value {value} must be finite", def.name),
        });
    }
    if let Some(min) = def.min
        && value < min
    {
        return Err(PropValidationError {
            message: format!("prop `{}` value {value} is below minimum {min}", def.name),
        });
    }
    if let Some(max) = def.max
        && value > max
    {
        return Err(PropValidationError {
            message: format!("prop `{}` value {value} is above maximum {max}", def.name),
        });
    }
    Ok(())
}

fn type_error(def: &PropDef, expected: &str, value: &PropValue) -> PropValidationError {
    PropValidationError {
        message: format!(
            "prop `{}` expects {}, got {}",
            def.name,
            expected,
            value.type_name()
        ),
    }
}

fn format_prop_number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

fn is_css_size_value(value: &str) -> bool {
    let trimmed = value.trim();
    Size::parse_string(trimmed).is_ok() || is_css_var_reference(trimmed)
}

fn is_css_color_value(value: &str) -> bool {
    let trimmed = value.trim();
    CssColor::parse_string(trimmed).is_ok() || is_css_var_reference(trimmed)
}

fn is_token_value(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_css_var_reference(trimmed) {
        return true;
    }
    trimmed
        .split('.')
        .all(|part| !part.is_empty() && CustomIdent::parse_string(part).is_ok())
}

fn is_css_var_reference(value: &str) -> bool {
    let Some(inner) = value
        .strip_prefix("var(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return false;
    };
    let name = inner.split_once(',').map_or(inner, |(name, _)| name).trim();
    DashedIdent::parse_string(name).is_ok()
}

fn parse_duration_ms(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if let Some(ms) = trimmed.strip_suffix("ms") {
        return ms
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite());
    }
    trimmed
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn is_icon_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.'))
}

/// A user-facing prop label/description: a literal or an i18n reference.
///
/// Mirrors `LocalizedText` in `mesh-core-module`; kept independent here so the
/// component crate stays free of runtime dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalizedLabel {
    Literal(String),
    Translation {
        key: String,
        fallback: Option<String>,
    },
}

/// A script block with its language, source code, and parser-derived metadata.
#[derive(Debug, Clone)]
pub struct ScriptBlock {
    pub lang: ScriptLang,
    pub source: String,
    pub metadata: ScriptMetadata,
    /// The script body in the owning `.mesh` source.
    pub span: SourceSpan,
}

/// Metadata collected from a Luau script by the component parser.
///
/// The metadata is deliberately shared by compiler and editor tooling.  It is
/// derived from the Luau AST rather than from source lines, so comments,
/// strings, multiline calls, and nested expressions cannot make consumers
/// disagree about what the script declares.
#[derive(Debug, Clone, Default)]
pub struct ScriptMetadata {
    pub state_vars: Vec<String>,
    pub service_bindings: Vec<(String, String)>,
    pub functions: Vec<String>,
    pub public_functions: Vec<String>,
    pub required_aliases: Vec<String>,
    pub interface_proxies: std::collections::HashMap<String, String>,
    /// Interface event members subscribed to by a statically resolvable proxy.
    /// Each entry is `(interface, event)`, for example
    /// `("mesh.audio", "VolumeChanged")`.
    pub interface_event_subscriptions: Vec<(String, String)>,
    /// State fields found in backend service payload tables.
    pub backend_state_fields: Vec<String>,
    /// Global `on_command_<name>` functions exposed by a backend service.
    pub backend_commands: Vec<String>,
    pub symbols: Vec<ScriptSymbol>,
    pub element_ref_aliases: Vec<ScriptAlias>,
}

/// A script symbol and its source location, relative to the script block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptSymbol {
    pub name: String,
    pub kind: ScriptSymbolKind,
    pub span: SourceSpan,
}

/// The kinds of symbols exported by a component script or made available to
/// editor navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptSymbolKind {
    Function,
    Variable,
}

/// A script alias used for element-reference tooling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptAlias {
    pub alias: String,
    pub target: ScriptAliasTarget,
}

/// The target of a script alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptAliasTarget {
    Ref(String),
    CurrentTarget,
}

/// Supported scripting languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptLang {
    Luau,
}

#[cfg(test)]
mod prop_value_conversion_tests {
    use super::*;

    #[test]
    fn json_prop_conversion_accepts_only_scalars() {
        let values = [
            serde_json::json!("text"),
            serde_json::json!(42.5),
            serde_json::json!(true),
            serde_json::Value::Null,
            serde_json::json!({"nested": [1, 2, 3]}),
            serde_json::json!(["a", "b"]),
        ];

        for value in values {
            assert_eq!(
                json_to_prop_value_ref(&value),
                json_to_prop_value(value.clone())
            );
        }

        assert_eq!(
            json_to_prop_value_ref(&serde_json::json!(["a", "b"])),
            Err(JsonPropValueError::Array)
        );
        assert_eq!(
            json_to_prop_value(serde_json::json!({"nested": [1, 2, 3]})),
            Err(JsonPropValueError::Object)
        );
        assert_eq!(
            json_to_prop_value_ref(&serde_json::Value::Null),
            Err(JsonPropValueError::Null)
        );
    }
}
