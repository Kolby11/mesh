pub mod parser;
pub mod style;
/// Single-file component parser for `.mesh` files.
///
/// A `.mesh` file contains these blocks:
///
/// ```text
/// <template>  — XHTML-like markup
/// <script>    — Luau logic
/// <style>     — CSS-like styling with theme token references
/// ```
///
/// This crate parses these blocks into a typed AST. It has no runtime
/// dependencies — it does not depend on mesh-core-theme, mesh-core-service, or
/// any other mesh crate.
pub mod template;

pub use parser::{ParseError, parse_component};
pub use style::*;
pub use template::*;

/// A parsed authoring-time import from a `.mesh` script block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentImport {
    pub alias: String,
    pub target: ComponentImportTarget,
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
    pub imports: Vec<ComponentImport>,
    pub props: Option<PropsBlock>,
    pub template: Option<TemplateBlock>,
    pub script: Option<ScriptBlock>,
    pub style: Option<StyleBlock>,
}

/// A parsed `<props>` block: the component's typed, defaulted configuration.
///
/// Each entry auto-projects to a `prop(name)` CSS reference, a reactive
/// `props.name` script field, and a generated settings-UI row. See
/// `docs/spec/03-components.md`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropsBlock {
    pub props: Vec<PropDef>,
}

/// A single declared prop.
#[derive(Debug, Clone, PartialEq)]
pub struct PropDef {
    pub name: String,
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

pub fn json_to_prop_value(value: serde_json::Value) -> Option<PropValue> {
    match value {
        serde_json::Value::String(s) => Some(PropValue::String(s)),
        serde_json::Value::Number(n) => n.as_f64().map(PropValue::Number),
        serde_json::Value::Bool(b) => Some(PropValue::Bool(b)),
        serde_json::Value::Null => None,
        other => Some(PropValue::String(other.to_string())),
    }
}

/// Convert a JSON value to the scalar prop domain without taking ownership.
///
/// Host/runtime callers frequently need to validate or project a value while
/// retaining the original JSON. Borrowing avoids a deep clone for arrays and
/// objects before their compatibility string conversion.
pub fn json_to_prop_value_ref(value: &serde_json::Value) -> Option<PropValue> {
    match value {
        serde_json::Value::String(value) => Some(PropValue::String(value.clone())),
        serde_json::Value::Number(value) => value.as_f64().map(PropValue::Number),
        serde_json::Value::Bool(value) => Some(PropValue::Bool(*value)),
        serde_json::Value::Null => None,
        other => Some(PropValue::String(other.to_string())),
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
    if matches!(
        trimmed,
        "auto" | "fit-content" | "min-content" | "max-content"
    ) {
        return true;
    }
    if trimmed.starts_with("var(") && trimmed.ends_with(')') {
        return true;
    }
    if trimmed.starts_with("calc(") && trimmed.ends_with(')') {
        return true;
    }
    parse_dimension(trimmed).is_some()
}

fn parse_dimension(value: &str) -> Option<(f64, &str)> {
    let split_at = value
        .find(|ch: char| !(ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+')))
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split_at);
    if number.is_empty() || number.parse::<f64>().is_err() {
        return None;
    }
    if unit.is_empty()
        || matches!(
            unit,
            "px" | "%" | "em" | "rem" | "vh" | "vw" | "vmin" | "vmax" | "ch"
        )
    {
        Some((number.parse().ok()?, unit))
    } else {
        None
    }
}

fn is_css_color_value(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed == "transparent"
        || trimmed == "currentColor"
        || trimmed.starts_with('#')
        || trimmed.starts_with("rgb(")
        || trimmed.starts_with("rgba(")
        || trimmed.starts_with("hsl(")
        || trimmed.starts_with("hsla(")
        || trimmed.starts_with("var(")
}

fn is_token_value(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("var(--")
        || trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn parse_duration_ms(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if let Some(ms) = trimmed.strip_suffix("ms") {
        return ms.trim().parse::<f64>().ok();
    }
    trimmed.parse::<f64>().ok()
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

/// A script block with its language and source code.
#[derive(Debug, Clone)]
pub struct ScriptBlock {
    pub lang: ScriptLang,
    pub source: String,
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
    fn borrowed_json_prop_conversion_matches_owned_conversion() {
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
    }

    // cargo test -p mesh-core-component --release -- borrowed_json_prop_conversion_avoids_nested_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only borrowed prop conversion microbenchmark"]
    fn borrowed_json_prop_conversion_avoids_nested_clone() {
        use std::time::Instant;

        let value = serde_json::json!({
            "items": (0..64)
                .map(|index| serde_json::json!({
                    "id": index,
                    "label": "x".repeat(128),
                    "enabled": index % 2 == 0
                }))
                .collect::<Vec<_>>()
        });
        let iterations = 20_000usize;

        let owned_started = Instant::now();
        let mut owned_total = 0usize;
        for _ in 0..iterations {
            let converted = json_to_prop_value(std::hint::black_box(value.clone())).unwrap();
            owned_total += match converted {
                PropValue::String(value) => value.len(),
                _ => 0,
            };
        }
        let owned_time = owned_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_total = 0usize;
        for _ in 0..iterations {
            let converted = json_to_prop_value_ref(std::hint::black_box(&value)).unwrap();
            borrowed_total += match converted {
                PropValue::String(value) => value.len(),
                _ => 0,
            };
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "nested JSON prop conversion: owned {owned_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; bytes={owned_total}/{borrowed_total}",
            owned_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(owned_total, borrowed_total);
        assert!(borrowed_time < owned_time);
    }
}
