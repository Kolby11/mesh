//! Validation of stored settings values against the declarations that own them.
//!
//! `docs/spec/08-settings.md` §1 requires an invalid stored value to be
//! rejected with a diagnostic and fall through to its declared default. Falling
//! through is free in the sparse model; this module supplies the rest: it walks
//! a namespace against a declarative schema ([`FieldSpec`] / [`FieldKind`]),
//! drops what it cannot accept, and says so.

use serde_json::{Map as JsonMap, Value as JsonValue};

/// A wrong type or unrecognized enum value is an error: the user meant to
/// change behavior and nothing changed. An unknown key is an error only within
/// typo distance of a known one; otherwise it warns, since it may belong to a
/// reader this walk does not know about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingsDiagnosticSeverity {
    Warning,
    Error,
}

impl SettingsDiagnosticSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// One rejected (or inert) stored settings value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsDiagnostic {
    pub severity: SettingsDiagnosticSeverity,
    /// The top-level key the offending value lives under.
    pub namespace: String,
    /// Dotted path within the namespace, empty for the namespace itself.
    pub key_path: String,
    pub message: String,
    pub suggested_action: String,
}

impl SettingsDiagnostic {
    pub fn warning(
        namespace: impl Into<String>,
        key_path: impl Into<String>,
        message: impl Into<String>,
        suggested_action: impl Into<String>,
    ) -> Self {
        Self {
            severity: SettingsDiagnosticSeverity::Warning,
            namespace: namespace.into(),
            key_path: key_path.into(),
            message: message.into(),
            suggested_action: suggested_action.into(),
        }
    }

    pub fn error(
        namespace: impl Into<String>,
        key_path: impl Into<String>,
        message: impl Into<String>,
        suggested_action: impl Into<String>,
    ) -> Self {
        Self {
            severity: SettingsDiagnosticSeverity::Error,
            namespace: namespace.into(),
            key_path: key_path.into(),
            message: message.into(),
            suggested_action: suggested_action.into(),
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity == SettingsDiagnosticSeverity::Error
    }

    /// Where the value lives, as a user would point at it in the file.
    pub fn location(&self) -> String {
        if self.key_path.is_empty() {
            self.namespace.clone()
        } else {
            format!("{}.{}", self.namespace, self.key_path)
        }
    }
}

impl std::fmt::Display for SettingsDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}: {} — {}",
            self.severity.label(),
            self.location(),
            self.message,
            self.suggested_action
        )
    }
}

/// `source` names the read that produced these (`"settings"`, `"settings
/// reload"`) so a live-reload warning is not mistaken for a startup one.
pub fn log_settings_diagnostics(source: &str, diagnostics: &[SettingsDiagnostic]) {
    for diagnostic in diagnostics {
        match diagnostic.severity {
            SettingsDiagnosticSeverity::Error => {
                tracing::error!(
                    "{source}: {}: {} — {}; using the declared default",
                    diagnostic.location(),
                    diagnostic.message,
                    diagnostic.suggested_action
                );
            }
            SettingsDiagnosticSeverity::Warning => {
                tracing::warn!(
                    "{source}: {}: {} — {}",
                    diagnostic.location(),
                    diagnostic.message,
                    diagnostic.suggested_action
                );
            }
        }
    }
}

/// Reload re-validates the whole file, so fixing one of five mistakes would
/// otherwise re-announce the other four. Each save reports only what changed.
pub fn new_settings_diagnostics(
    previous: &[SettingsDiagnostic],
    current: &[SettingsDiagnostic],
) -> Vec<SettingsDiagnostic> {
    current
        .iter()
        .filter(|diagnostic| !previous.contains(diagnostic))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct FieldSpec {
    pub key: &'static str,
    pub kind: FieldKind,
}

impl FieldSpec {
    pub const fn new(key: &'static str, kind: FieldKind) -> Self {
        Self { key, kind }
    }
}

/// What a declared key accepts.
#[derive(Debug, Clone, Copy)]
pub enum FieldKind {
    Str,
    Bool,
    UInt,
    /// Signed integer that must survive the trip into `i32`.
    Int32,
    /// Unsigned integer constrained by an inclusive range.
    UIntRange {
        min: u64,
        max: u64,
    },
    /// Floating-point number constrained by inclusive bounds. `max = None`
    /// means that only the lower bound is finite.
    FloatRange {
        min: f64,
        max: Option<f64>,
    },
    Float,
    StrArray,
    /// `accepts` *is* the defining parser, so aliases it takes are accepted
    /// here too; `values` is only the canonical list quoted in the suggestion.
    Enum {
        canonicalize: fn(&str) -> Option<&'static str>,
        values: &'static [&'static str],
    },
    /// Object with a known key set; unknown keys inside it are reported.
    Section(&'static [FieldSpec]),
    /// Object whose keys the user chooses. Keys are not checked; each value is
    /// validated against the inner kind.
    Map(&'static FieldKind),
    /// Contents pass through untouched — used where the schema is owned
    /// elsewhere, such as `props`.
    Opaque,
}

impl FieldKind {
    fn expectation(&self) -> String {
        match self {
            Self::Str => "a string".to_string(),
            Self::Bool => "a boolean".to_string(),
            Self::UInt => "a non-negative integer".to_string(),
            Self::Int32 => "an integer".to_string(),
            Self::UIntRange { min, max } => format!("an integer from {min} through {max}"),
            Self::FloatRange { min, max } => match max {
                Some(max) => format!("a number from {min} through {max}"),
                None => format!("a number greater than or equal to {min}"),
            },
            Self::Float => "a number".to_string(),
            Self::StrArray => "an array of strings".to_string(),
            Self::Enum { values, .. } => format!("one of [{}]", values.join(", ")),
            Self::Section(_) | Self::Map(_) | Self::Opaque => "an object".to_string(),
        }
    }

    fn suggestion(&self) -> String {
        match self {
            Self::Enum { values, .. } => format!("use one of: {}", values.join(", ")),
            other => format!(
                "use {}, or remove the key to fall back to the declared default",
                other.expectation()
            ),
        }
    }
}

/// Returns the subset of `value` that may be applied, nested objects likewise
/// filtered. What is left out is what falls through to its declared default.
pub fn validate_object(
    namespace: &str,
    key_prefix: &str,
    fields: &'static [FieldSpec],
    value: &JsonValue,
    diagnostics: &mut Vec<SettingsDiagnostic>,
) -> JsonValue {
    let Some(object) = value.as_object() else {
        diagnostics.push(SettingsDiagnostic::error(
            namespace,
            key_prefix,
            format!("expected an object, found {}", describe(value)),
            "make this key an object, or remove it",
        ));
        return JsonValue::Object(JsonMap::new());
    };

    let mut accepted = JsonMap::new();
    for (key, entry) in object {
        let path = join_path(key_prefix, key);
        let Some(spec) = fields.iter().find(|spec| spec.key == *key) else {
            diagnostics.push(unknown_key_diagnostic(namespace, key_prefix, key, fields));
            continue;
        };
        if let Some(kept) = validate_value(namespace, &path, &spec.kind, entry, diagnostics) {
            accepted.insert(key.clone(), kept);
        }
    }

    JsonValue::Object(accepted)
}

/// Validate a value against the small JSON-schema vocabulary used by module
/// settings declarations.
///
/// The schema is deliberately data-only so module owners can register it after
/// graph resolution without making the foundation config crate depend on the
/// module system. Objects are open unless a schema explicitly sets
/// additionalProperties to false; this lets an owner describe only the portion
/// of a namespace it owns while other portions remain available to core
/// readers.
pub fn validate_json_schema(
    namespace: &str,
    key_path: &str,
    schema: &JsonValue,
    value: &JsonValue,
    diagnostics: &mut Vec<SettingsDiagnostic>,
) -> JsonValue {
    let Some(schema) = schema.as_object() else {
        return value.clone();
    };

    if let Some(expected) = schema.get("type").and_then(JsonValue::as_str)
        && !json_schema_type_matches(expected, value)
    {
        diagnostics.push(SettingsDiagnostic::error(
            namespace,
            key_path,
            format!(
                "expected {}, found {}",
                json_schema_expectation(expected),
                describe(value)
            ),
            "remove the value or replace it with the declared type",
        ));
        return JsonValue::Null;
    }

    if let Some(enumeration) = schema.get("enum").and_then(JsonValue::as_array)
        && !enumeration.iter().any(|candidate| candidate == value)
    {
        diagnostics.push(SettingsDiagnostic::error(
            namespace,
            key_path,
            format!("value is not one of {}", describe_enum_values(enumeration)),
            format!("use one of {}", describe_enum_values(enumeration)),
        ));
        return JsonValue::Null;
    }

    if let Some(minimum) = schema.get("minimum").and_then(JsonValue::as_f64)
        && value.as_f64().is_some_and(|number| number < minimum)
    {
        diagnostics.push(SettingsDiagnostic::error(
            namespace,
            key_path,
            format!("value must be greater than or equal to {minimum}"),
            "raise the value to the declared minimum or remove it",
        ));
        return JsonValue::Null;
    }
    if let Some(maximum) = schema.get("maximum").and_then(JsonValue::as_f64)
        && value.as_f64().is_some_and(|number| number > maximum)
    {
        diagnostics.push(SettingsDiagnostic::error(
            namespace,
            key_path,
            format!("value must be less than or equal to {maximum}"),
            "lower the value to the declared maximum or remove it",
        ));
        return JsonValue::Null;
    }

    if let Some(object) = value.as_object() {
        let properties = schema
            .get("properties")
            .and_then(JsonValue::as_object)
            .cloned()
            .unwrap_or_default();
        let allow_unknown = schema
            .get("additionalProperties")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true);
        let additional_schema = schema
            .get("additionalProperties")
            .filter(|value| value.is_object());
        let mut accepted = JsonMap::new();
        for (key, entry) in object {
            let path = join_path(key_path, key);
            if let Some(child_schema) = properties.get(key) {
                let kept = validate_json_schema(namespace, &path, child_schema, entry, diagnostics);
                if !kept.is_null() {
                    accepted.insert(key.clone(), kept);
                }
            } else if let Some(child_schema) = additional_schema {
                let kept = validate_json_schema(namespace, &path, child_schema, entry, diagnostics);
                if !kept.is_null() {
                    accepted.insert(key.clone(), kept);
                }
            } else if allow_unknown {
                accepted.insert(key.clone(), entry.clone());
            } else {
                let known = properties.keys().map(String::as_str).collect::<Vec<_>>();
                diagnostics.push(unknown_key_diagnostic_from(
                    namespace, key_path, key, &known,
                ));
            }
        }
        return JsonValue::Object(accepted);
    }

    if let Some(items_schema) = schema.get("items")
        && let Some(items) = value.as_array()
    {
        let accepted = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let path = join_path(key_path, &format!("[{index}]"));
                let kept = validate_json_schema(namespace, &path, items_schema, item, diagnostics);
                (!kept.is_null()).then_some(kept)
            })
            .collect();
        return JsonValue::Array(accepted);
    }

    value.clone()
}

fn json_schema_type_matches(expected: &str, value: &JsonValue) -> bool {
    match expected {
        "any" => true,
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" | "str" | "color" | "enum" => value.is_string(),
        "size" | "duration" => value.is_string() || value.is_number(),
        "boolean" | "bool" => value.is_boolean(),
        "integer" | "int" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" | "float" => value.as_f64().is_some_and(f64::is_finite),
        _ => true,
    }
}

fn json_schema_expectation(expected: &str) -> &'static str {
    match expected {
        "object" => "an object",
        "array" => "an array",
        "string" | "str" | "color" | "enum" => "a string",
        "size" | "duration" => "a string or number",
        "boolean" | "bool" => "a boolean",
        "integer" | "int" => "an integer",
        "number" | "float" => "a number",
        _ => "the declared type",
    }
}

fn describe_enum_values(values: &[JsonValue]) -> String {
    let values = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    format!("[{}]", values.join(", "))
}

/// Validate one stored value. `None` means "reject it and keep the default".
pub fn validate_value(
    namespace: &str,
    key_path: &str,
    kind: &FieldKind,
    value: &JsonValue,
    diagnostics: &mut Vec<SettingsDiagnostic>,
) -> Option<JsonValue> {
    let accepted = match kind {
        FieldKind::Str => value.is_string().then(|| value.clone()),
        FieldKind::Bool => value.is_boolean().then(|| value.clone()),
        FieldKind::UInt => value.as_u64().map(|_| value.clone()),
        FieldKind::Int32 => value
            .as_i64()
            .filter(|n| i32::try_from(*n).is_ok())
            .map(|_| value.clone()),
        FieldKind::UIntRange { min, max } => value
            .as_u64()
            .filter(|n| (*min..=*max).contains(n))
            .map(|_| value.clone()),
        FieldKind::FloatRange { min, max } => value
            .as_f64()
            .filter(|number| number.is_finite() && *number >= *min)
            .filter(|number| max.is_none_or(|max| *number <= max))
            .map(|_| value.clone()),
        FieldKind::Float => value
            .as_f64()
            .filter(|number| number.is_finite())
            .map(|_| value.clone()),
        FieldKind::StrArray => value
            .as_array()
            .filter(|items| items.iter().all(JsonValue::is_string))
            .map(|_| value.clone()),
        FieldKind::Enum { canonicalize, .. } => value
            .as_str()
            .and_then(canonicalize)
            .map(|canonical| JsonValue::String(canonical.to_string())),
        FieldKind::Opaque => value.is_object().then(|| value.clone()),
        FieldKind::Section(fields) => Some(validate_object(
            namespace,
            key_path,
            fields,
            value,
            diagnostics,
        )),
        FieldKind::Map(inner) => {
            let Some(entries) = value.as_object() else {
                diagnostics.push(SettingsDiagnostic::error(
                    namespace,
                    key_path,
                    format!("expected an object, found {}", describe(value)),
                    "make this key an object, or remove it",
                ));
                return None;
            };
            let mut accepted = JsonMap::new();
            for (key, entry) in entries {
                let path = join_path(key_path, key);
                if let Some(kept) = validate_value(namespace, &path, inner, entry, diagnostics) {
                    accepted.insert(key.clone(), kept);
                }
            }
            Some(JsonValue::Object(accepted))
        }
    };

    if let Some(accepted) = accepted {
        return Some(accepted);
    }

    diagnostics.push(SettingsDiagnostic::error(
        namespace,
        key_path,
        format!("expected {}, found {}", kind.expectation(), describe(value)),
        kind.suggestion(),
    ));
    None
}

/// Diagnose a key no declaration claims, suggesting the nearest known one.
pub fn unknown_key_diagnostic(
    namespace: &str,
    key_prefix: &str,
    key: &str,
    fields: &[FieldSpec],
) -> SettingsDiagnostic {
    let known: Vec<&str> = fields.iter().map(|spec| spec.key).collect();
    unknown_key_diagnostic_from(namespace, key_prefix, key, &known)
}

/// As [`unknown_key_diagnostic`], for callers holding a plain key list.
pub fn unknown_key_diagnostic_from(
    namespace: &str,
    key_prefix: &str,
    key: &str,
    known: &[&str],
) -> SettingsDiagnostic {
    let path = join_path(key_prefix, key);
    match nearest_key(key, known) {
        Some(candidate) => SettingsDiagnostic::error(
            namespace,
            path,
            format!("unknown key \"{key}\"; it is ignored"),
            format!("did you mean \"{candidate}\"?"),
        ),
        None => SettingsDiagnostic::warning(
            namespace,
            path,
            format!("unknown key \"{key}\"; it is ignored"),
            if known.is_empty() {
                "remove it".to_string()
            } else {
                format!("remove it; known keys are: {}", known.join(", "))
            },
        ),
    }
}

/// The known key closest to `key`, if close enough to be a typo of it. The
/// budget scales with length so short keys are not "corrected" into unrelated
/// short keys (`top` is not a typo of `left`).
pub fn nearest_key<'a>(key: &str, known: &[&'a str]) -> Option<&'a str> {
    let lowered = key.to_ascii_lowercase();
    let budget = match lowered.chars().count() {
        0..=3 => 1,
        4..=7 => 2,
        _ => 3,
    };

    known
        .iter()
        .filter_map(|candidate| {
            let distance = edit_distance(&lowered, &candidate.to_ascii_lowercase());
            (distance <= budget).then_some((distance, *candidate))
        })
        .min_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)))
        .map(|(_, candidate)| candidate)
}

/// Levenshtein distance, single-row DP.
fn edit_distance(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut row: Vec<usize> = (0..=right.len()).collect();

    for (i, left_char) in left.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let cost = usize::from(left_char != *right_char);
            let candidate = (diagonal + cost).min(row[j] + 1).min(row[j + 1] + 1);
            diagonal = row[j + 1];
            row[j + 1] = candidate;
        }
    }

    row[right.len()]
}

fn join_path(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

/// Quotes scalars so the user sees what they typed: `the string "300"`.
pub fn describe(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(flag) => format!("the boolean {flag}"),
        JsonValue::Number(number) => format!("the number {number}"),
        JsonValue::String(text) => format!("the string \"{text}\""),
        JsonValue::Array(_) => "an array".to_string(),
        JsonValue::Object(_) => "an object".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_key_finds_a_single_character_typo() {
        assert_eq!(
            nearest_key("anchr", &["anchor", "layer", "blur"]),
            Some("anchor")
        );
    }

    #[test]
    fn nearest_key_refuses_unrelated_short_keys() {
        assert_eq!(nearest_key("wat", &["anchor", "layer", "blur"]), None);
    }

    #[test]
    fn edit_distance_handles_empty_inputs() {
        assert_eq!(edit_distance("", "anchor"), 6);
        assert_eq!(edit_distance("anchor", ""), 6);
        assert_eq!(edit_distance("anchor", "anchor"), 0);
    }
}
