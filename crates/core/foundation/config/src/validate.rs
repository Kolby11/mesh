//! Validation of stored settings values against the declarations that own them.
//!
//! `docs/spec/08-settings.md` §1 requires that an invalid stored value be
//! *rejected with a diagnostic* and fall through to its declared default. The
//! falling-through half comes free from the sparse model — a value that never
//! reaches a reader simply is not there. This module supplies the other half:
//! it walks a stored namespace against a schema, drops what it cannot accept,
//! and says so.
//!
//! The schema is declarative ([`FieldSpec`] / [`FieldKind`]) so the same walk
//! serves the `"shell"` namespace here and the `surface` placement block in
//! `mesh-core-surface-config`, which owns its own vocabulary. Enum fields carry
//! the parser that defines them ([`FieldKind::Enum`]) rather than a copy of its
//! accepted strings, so a new variant cannot drift away from its own error
//! message.

use serde_json::{Map as JsonMap, Value as JsonValue};

/// How much a rejected value matters.
///
/// A wrong type or an unrecognized enum value is an [`Error`]: the user wrote
/// something meaning to change behavior and nothing changed. An unknown key is
/// an error only when a known key sits within typo distance of it — the strong
/// signal that it was meant to be that key. An unknown key with no near match
/// is a [`Warning`]: it may be a forward-looking key, a key owned by a reader
/// this walk does not know about, or a leftover, and none of those deserve a
/// non-zero exit.
///
/// [`Error`]: SettingsDiagnosticSeverity::Error
/// [`Warning`]: SettingsDiagnosticSeverity::Warning
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
///
/// Modeled on `ModuleManifestDiagnostic`: the pairing of a located `message`
/// with a `suggested_action` is what makes the manifest diagnostics actionable,
/// and a hand-edited settings file needs exactly the same thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsDiagnostic {
    pub severity: SettingsDiagnosticSeverity,
    /// `"shell"`, `"@mesh/navigation-bar"`, `"mesh.audio"` — the top-level key
    /// the offending value lives under.
    pub namespace: String,
    /// Dotted path within the namespace: `"surface.anchor"`,
    /// `"tooltip.delay_ms"`. Empty for a diagnostic about the namespace itself.
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

/// Log a batch of diagnostics through `tracing`.
///
/// `source` names the read that produced them (`"settings"`, `"settings
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

/// The diagnostics in `current` that were not already in `previous`.
///
/// Live reload re-validates the whole file on every save, so a user fixing one
/// of five mistakes would otherwise be told about the other four again. Keeping
/// only what is new means each save reports only what changed.
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

/// One declared key in a settings schema.
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
    /// Non-negative integer (`tooltip.delay_ms`).
    UInt,
    /// Signed integer that has to survive the trip into `i32`
    /// (`surface.exclusive_zone`, margins).
    Int32,
    Float,
    StrArray,
    /// A string whose vocabulary belongs to a parser. `accepts` *is* the
    /// parser, so aliases it takes are accepted here too; `values` is only the
    /// canonical list quoted back in the suggestion.
    Enum {
        accepts: fn(&str) -> bool,
        values: &'static [&'static str],
    },
    /// Object with a known key set; unknown keys inside it are reported.
    Section(&'static [FieldSpec]),
    /// Object whose keys the user chooses (module ids, action names). Keys are
    /// not checked; each value is validated against the inner kind.
    Map(&'static FieldKind),
    /// An object this walk does not own the schema for yet, so its contents
    /// pass through untouched. Used for `props`, which cannot be validated
    /// until the `<props>` block exists (`docs/spec/03-components.md`).
    Opaque,
}

impl FieldKind {
    /// The phrase used after "expected" in a diagnostic.
    fn expectation(&self) -> String {
        match self {
            Self::Str => "a string".to_string(),
            Self::Bool => "a boolean".to_string(),
            Self::UInt => "a non-negative integer".to_string(),
            Self::Int32 => "an integer".to_string(),
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

/// Validate a stored object against a known key set.
///
/// Returns the subset of `value` that may be applied: every key that validated,
/// with nested objects likewise filtered. Anything rejected is left out, which
/// is what makes the value fall through to its declared default, and is
/// reported in `diagnostics`.
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

/// Validate one stored value. `None` means "reject it and keep the default".
pub fn validate_value(
    namespace: &str,
    key_path: &str,
    kind: &FieldKind,
    value: &JsonValue,
    diagnostics: &mut Vec<SettingsDiagnostic>,
) -> Option<JsonValue> {
    let ok = match kind {
        FieldKind::Str => value.is_string(),
        FieldKind::Bool => value.is_boolean(),
        FieldKind::UInt => value.as_u64().is_some(),
        FieldKind::Int32 => value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
        FieldKind::Float => value.is_number(),
        FieldKind::StrArray => value
            .as_array()
            .is_some_and(|items| items.iter().all(JsonValue::is_string)),
        FieldKind::Enum { accepts, .. } => value.as_str().is_some_and(accepts),
        FieldKind::Opaque => value.is_object(),
        FieldKind::Section(fields) => {
            return Some(validate_object(
                namespace,
                key_path,
                fields,
                value,
                diagnostics,
            ));
        }
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
            return Some(JsonValue::Object(accepted));
        }
    };

    if ok {
        return Some(value.clone());
    }

    diagnostics.push(SettingsDiagnostic::error(
        namespace,
        key_path,
        format!("expected {}, found {}", kind.expectation(), describe(value)),
        kind.suggestion(),
    ));
    None
}

/// Build the diagnostic for a key that no declaration claims, suggesting the
/// nearest known key when there is one.
pub fn unknown_key_diagnostic(
    namespace: &str,
    key_prefix: &str,
    key: &str,
    fields: &[FieldSpec],
) -> SettingsDiagnostic {
    let known: Vec<&str> = fields.iter().map(|spec| spec.key).collect();
    unknown_key_diagnostic_from(namespace, key_prefix, key, &known)
}

/// As [`unknown_key_diagnostic`], for callers holding a plain key list (the
/// settings file's own top level, which has no [`FieldSpec`] table).
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

/// The known key closest to `key`, if one is close enough to be a typo of it.
///
/// The distance budget scales with the key's length so that short keys cannot
/// be "corrected" into unrelated short keys (`top` is not a typo of `left`),
/// while a long key tolerates the two-character slip that a long key invites.
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

/// Describe a found value for a diagnostic message, quoting scalars so the user
/// can see the thing they typed (`the string "300"` rather than `a string`).
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
        // `blur` and `left` are two edits apart but nothing to do with each
        // other; a short key gets a budget of one.
        assert_eq!(nearest_key("wat", &["anchor", "layer", "blur"]), None);
    }

    #[test]
    fn edit_distance_handles_empty_inputs() {
        assert_eq!(edit_distance("", "anchor"), 6);
        assert_eq!(edit_distance("anchor", ""), 6);
        assert_eq!(edit_distance("anchor", "anchor"), 0);
    }
}
