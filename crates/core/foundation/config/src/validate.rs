//! Validation of stored settings values against the declarations that own them.
//!
//! `docs/spec/08-settings.md` §1 requires an invalid stored value to be
//! rejected with a diagnostic and fall through to its declared default. Falling
//! through is free in the sparse model; this module supplies the rest: it walks
//! a namespace against a declarative schema ([`FieldSpec`] / [`FieldKind`]),
//! drops what it cannot accept, and says so.

use mesh_core_theme::{ThemeModeSchedule, validate_theme_schedule_times};
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
        match (self.namespace.is_empty(), self.key_path.is_empty()) {
            (true, _) => self.key_path.clone(),
            (false, true) => self.namespace.clone(),
            (false, false) => format!("{}.{}", self.namespace, self.key_path),
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
    /// A canonicalized BCP 47 locale tag.
    Locale,
    Bool,
    UInt,
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
    /// A literal string or a localized text object with `t` and `fallback`.
    LocalizedText,
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
    /// The tagged policy object used by the shell theme mode selector.
    ThemeModePolicy,
    /// A scalar theme token: string, finite number, or boolean.
    Token,
}

impl FieldKind {
    fn expectation(&self) -> String {
        match self {
            Self::Str => "a string".to_string(),
            Self::Locale => "a valid BCP 47 locale tag".to_string(),
            Self::Bool => "a boolean".to_string(),
            Self::UInt => "a non-negative integer".to_string(),
            Self::UIntRange { min, max } => format!("an integer from {min} through {max}"),
            Self::FloatRange { min, max } => match max {
                Some(max) => format!("a number from {min} through {max}"),
                None => format!("a number greater than or equal to {min}"),
            },
            Self::Float => "a number".to_string(),
            Self::StrArray => "an array of strings".to_string(),
            Self::LocalizedText => "a string or localized text object".to_string(),
            Self::Enum { values, .. } => format!("one of [{}]", values.join(", ")),
            Self::Section(_) | Self::Map(_) | Self::Opaque => "an object".to_string(),
            Self::ThemeModePolicy => "a theme mode policy object".to_string(),
            Self::Token => "a string, number, or boolean".to_string(),
        }
    }

    fn suggestion(&self) -> String {
        match self {
            Self::Enum { values, .. } => format!("use one of: {}", values.join(", ")),
            Self::Locale => {
                "use a valid BCP 47 locale tag, or remove it to fall back to the declared default"
                    .to_string()
            }
            Self::LocalizedText => {
                "use a literal string or an object with non-empty `t` and `fallback` strings"
                    .to_string()
            }
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
        // Arrays represent ordered wholesale replacements in the sparse
        // settings model. Do not silently compact a bad member out of a pack
        // chain or key list unless an owner explicitly opts into filtering.
        let filter_invalid_items = schema
            .get("filterInvalidItems")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        let mut accepted = Vec::with_capacity(items.len());
        let mut rejected_item = false;
        for (index, item) in items.iter().enumerate() {
            let path = join_path(key_path, &format!("[{index}]"));
            let diagnostics_before = diagnostics.len();
            let kept = validate_json_schema(namespace, &path, items_schema, item, diagnostics);
            let rejected = kept.is_null() && diagnostics.len() != diagnostics_before;
            rejected_item |= rejected;
            if !rejected {
                accepted.push(kept);
            }
        }
        if rejected_item && !filter_invalid_items {
            return JsonValue::Null;
        }
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
        "localized-text" => localized_text_value(value),
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
        "localized-text" => "a string or localized text object",
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
        FieldKind::Locale => value
            .as_str()
            .and_then(|locale| mesh_core_locale::normalize_locale_tag(locale).ok())
            .map(JsonValue::String),
        FieldKind::Bool => value.is_boolean().then(|| value.clone()),
        FieldKind::UInt => value.as_u64().map(|_| value.clone()),
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
        FieldKind::LocalizedText => localized_text_value(value).then(|| value.clone()),
        FieldKind::Enum { canonicalize, .. } => value
            .as_str()
            .and_then(canonicalize)
            .map(|canonical| JsonValue::String(canonical.to_string())),
        FieldKind::Opaque => value.is_object().then(|| value.clone()),
        FieldKind::ThemeModePolicy => {
            validate_theme_mode_policy(namespace, key_path, value, diagnostics)
        }
        FieldKind::Token => match value {
            JsonValue::String(_) | JsonValue::Bool(_) => Some(value.clone()),
            JsonValue::Number(number) if number.as_f64().is_some_and(f64::is_finite) => {
                Some(value.clone())
            }
            _ => None,
        },
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

fn validate_theme_mode_policy(
    namespace: &str,
    key_path: &str,
    value: &JsonValue,
    diagnostics: &mut Vec<SettingsDiagnostic>,
) -> Option<JsonValue> {
    let object = value.as_object()?;
    let kind = object.get("kind").and_then(JsonValue::as_str)?;
    if !matches!(kind, "manual" | "follow_system" | "scheduled") {
        diagnostics.push(SettingsDiagnostic::error(
            namespace,
            &join_path(key_path, "kind"),
            format!("expected one of [manual, follow_system, scheduled], found {kind:?}"),
            "use one of: manual, follow_system, scheduled",
        ));
        return None;
    }

    let mut accepted = JsonMap::new();
    accepted.insert("kind".into(), JsonValue::String(kind.into()));
    for key in object
        .keys()
        .filter(|key| *key != "kind" && *key != "entries")
    {
        diagnostics.push(SettingsDiagnostic::warning(
            namespace,
            &join_path(key_path, key),
            format!("unknown theme mode policy key '{key}'"),
            "remove the key or check the theme mode policy shape",
        ));
    }

    let Some(entries) = object.get("entries") else {
        if kind == "scheduled" {
            diagnostics.push(SettingsDiagnostic::error(
                namespace,
                &join_path(key_path, "entries"),
                "scheduled theme mode policy requires an entries array",
                "add entries with at and mode strings, or use manual",
            ));
            return None;
        }
        return Some(JsonValue::Object(accepted));
    };
    if kind != "scheduled" {
        diagnostics.push(SettingsDiagnostic::warning(
            namespace,
            &join_path(key_path, "entries"),
            "entries is only used by the scheduled theme mode policy",
            "remove entries or use kind scheduled",
        ));
        return Some(JsonValue::Object(accepted));
    }
    let Some(entries) = entries.as_array() else {
        diagnostics.push(SettingsDiagnostic::error(
            namespace,
            &join_path(key_path, "entries"),
            format!("expected an array, found {}", describe(entries)),
            "make entries an array of { at, mode } objects",
        ));
        return None;
    };
    let mut accepted_entries = Vec::with_capacity(entries.len());
    let mut schedule_entries = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let entry_path = join_path(&join_path(key_path, "entries"), &index.to_string());
        let Some(entry) = entry.as_object() else {
            diagnostics.push(SettingsDiagnostic::error(
                namespace,
                &entry_path,
                format!("expected an object, found {}", describe(entry)),
                "use an object with at and mode strings",
            ));
            return None;
        };
        let Some(at) = entry.get("at").and_then(JsonValue::as_str) else {
            diagnostics.push(SettingsDiagnostic::error(
                namespace,
                &join_path(&entry_path, "at"),
                "scheduled theme mode entries require an at string",
                "add a time in HH:MM form",
            ));
            return None;
        };
        let Some(mode) = entry.get("mode").and_then(JsonValue::as_str) else {
            diagnostics.push(SettingsDiagnostic::error(
                namespace,
                &join_path(&entry_path, "mode"),
                "scheduled theme mode entries require a mode string",
                "add the declared theme mode name",
            ));
            return None;
        };
        if at.trim().is_empty() || mode.trim().is_empty() {
            diagnostics.push(SettingsDiagnostic::error(
                namespace,
                &entry_path,
                "scheduled theme mode entries cannot be empty",
                "use non-empty at and mode strings",
            ));
            return None;
        }
        schedule_entries.push(ThemeModeSchedule {
            at: at.to_string(),
            mode: mode.to_string(),
        });
        accepted_entries.push(serde_json::json!({ "at": at, "mode": mode }));
    }
    if let Err(error) = validate_theme_schedule_times(&schedule_entries) {
        let entry_path = join_path(
            &join_path(key_path, "entries"),
            &error.entry_index().to_string(),
        );
        diagnostics.push(SettingsDiagnostic::error(
            namespace,
            join_path(&entry_path, "at"),
            error.to_string(),
            "use a valid, unique time in HH:MM form",
        ));
        return None;
    }
    accepted.insert("entries".into(), JsonValue::Array(accepted_entries));
    Some(JsonValue::Object(accepted))
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

fn localized_text_value(value: &JsonValue) -> bool {
    if value.is_string() {
        return true;
    }
    let Some(object) = value.as_object() else {
        return false;
    };
    object.len() == 2
        && object
            .get("t")
            .and_then(JsonValue::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        && object
            .get("fallback")
            .and_then(JsonValue::as_str)
            .is_some_and(|value| !value.trim().is_empty())
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

    #[test]
    fn root_level_diagnostic_location_omits_the_namespace_separator() {
        let diagnostic =
            SettingsDiagnostic::warning("", "unknown_root_key", "unknown key", "remove it");

        assert_eq!(diagnostic.location(), "unknown_root_key");
        assert_eq!(
            diagnostic.to_string(),
            "warning: unknown_root_key: unknown key — remove it"
        );
    }

    #[test]
    fn localized_text_validation_keeps_literals_and_translation_objects() {
        let kind = FieldKind::LocalizedText;
        let mut diagnostics = Vec::new();

        assert_eq!(
            validate_value(
                "@mesh/test",
                "surface.title",
                &kind,
                &serde_json::json!("Settings"),
                &mut diagnostics,
            ),
            Some(serde_json::json!("Settings"))
        );
        assert_eq!(
            validate_value(
                "@mesh/test",
                "surface.title",
                &kind,
                &serde_json::json!({ "t": "settings.title", "fallback": "Settings" }),
                &mut diagnostics,
            ),
            Some(serde_json::json!({ "t": "settings.title", "fallback": "Settings" }))
        );
        assert!(
            validate_value(
                "@mesh/test",
                "surface.title",
                &kind,
                &serde_json::json!({ "t": "", "fallback": "Settings" }),
                &mut diagnostics,
            )
            .is_none()
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn json_schema_arrays_reject_invalid_members_without_compacting() {
        let schema = serde_json::json!({
            "type": "array",
            "items": { "type": "string" }
        });
        let mut diagnostics = Vec::new();

        assert_eq!(
            validate_json_schema(
                "@mesh/test",
                "icons.use_packs",
                &schema,
                &serde_json::json!(["pack-a", 7, "pack-b"]),
                &mut diagnostics,
            ),
            serde_json::Value::Null,
            "one invalid member rejects the whole ordered override"
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].key_path, "icons.use_packs.[1]");

        let filtering_schema = serde_json::json!({
            "type": "array",
            "items": { "type": "string" },
            "filterInvalidItems": true
        });
        let mut filtering_diagnostics = Vec::new();
        assert_eq!(
            validate_json_schema(
                "@mesh/test",
                "icons.use_packs",
                &filtering_schema,
                &serde_json::json!(["pack-a", 7, "pack-b"]),
                &mut filtering_diagnostics,
            ),
            serde_json::json!(["pack-a", "pack-b"])
        );
        assert_eq!(filtering_diagnostics.len(), 1);
    }
}
