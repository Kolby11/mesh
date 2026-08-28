//! Schema-driven diagnostics for JSON documents: syntax, unknown keys, enum
//! violations, missing required fields, and structural type mismatches.
//!
//! Document-specific rules a schema tree cannot express, including canonical
//! runtime validation for manifests, are layered on by the caller.

use json_syntax::{CodeMap, Parse, Value as JsonValue, array::JsonArray};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use super::schema::{Kind, Node};
use super::{line_col_to_offset, offset_to_position};

/// Validate `source` against `schema`. `origin` is the diagnostic `source`
/// field shown by the editor (`"mesh-manifest"`, `"mesh-settings"`).
///
/// A syntax error is fatal: it is reported alone, because every schema
/// complaint downstream of it would be noise about text the user is still
/// typing.
pub fn check(schema: &Node, source: &str, origin: &str) -> Vec<Diagnostic> {
    if let Err(err) = serde_json::from_str::<serde_json::Value>(source) {
        let offset = line_col_to_offset(source, err.line(), err.column());
        return vec![error_at(
            source,
            offset,
            offset + 1,
            format!("JSON syntax error: {err}"),
            origin,
        )];
    }

    let mut out = Vec::new();
    if let Some(ast) = JNode::parse(source) {
        check_node(source, &ast, schema, origin, &mut out);
    }
    out
}

/// Recursively validate `node` (a JSON AST node) against `schema`.
fn check_node(source: &str, node: &JNode, schema: &Node, origin: &str, out: &mut Vec<Diagnostic>) {
    match (&node.value, &schema.kind) {
        (JValue::Object(members), Kind::Object(fields)) => {
            // Unknown keys.
            for m in members {
                match fields.iter().find(|f| f.name == m.key) {
                    Some(field) => {
                        check_node(source, &m.value, &field.node, origin, out);
                    }
                    None => out.push(warn(
                        source,
                        m.key_span.0,
                        m.key_span.1,
                        format!("unknown property `{}`", m.key),
                        origin,
                    )),
                }
            }
            // Missing required keys.
            for f in fields.iter().filter(|f| f.required) {
                if !members.iter().any(|m| m.key == f.name) {
                    out.push(error_at(
                        source,
                        node.span.0,
                        node.span.0 + 1,
                        format!("missing required property `{}`", f.name),
                        origin,
                    ));
                }
            }
        }
        // An open object's unlisted keys are the user's own vocabulary (module
        // ids, interface ids), so they are described by `other`, not flagged.
        (JValue::Object(members), Kind::OpenObject { fields, other }) => {
            for m in members {
                let target = fields
                    .iter()
                    .find(|f| f.name == m.key)
                    .map(|f| &f.node)
                    .unwrap_or(other.as_ref());
                check_node(source, &m.value, target, origin, out);
            }
        }
        (JValue::Object(members), Kind::Map(value)) => {
            for m in members {
                check_node(source, &m.value, value, origin, out);
            }
        }
        (JValue::Array(elements), Kind::Array(element)) => {
            for e in elements {
                check_node(source, e, element, origin, out);
            }
        }
        (JValue::String(s), Kind::Enum(values)) => {
            if !values.contains(&s.as_str()) {
                out.push(error_at(
                    source,
                    node.span.0,
                    node.span.1,
                    format!(
                        "`{}` is not a valid value here (expected one of: {})",
                        s,
                        values.join(", ")
                    ),
                    origin,
                ));
            }
        }
        // Suggested-value strings are never validated (extensible vocabulary,
        // or a discovery that cannot see everything the runtime can).
        (JValue::String(_), Kind::Suggest(_) | Kind::SuggestDiscovered(_)) => {}
        // Structural mismatches: a container was expected but a scalar appeared
        // (or vice versa). Scalar schema nodes accept any JSON value.
        (actual, expected) => {
            if let Some(msg) = type_mismatch(actual, expected) {
                out.push(error_at(source, node.span.0, node.span.1, msg, origin));
            }
        }
    }
}

fn type_mismatch(actual: &JValue, expected: &Kind) -> Option<String> {
    let want = match expected {
        Kind::Object(_) | Kind::OpenObject { .. } | Kind::Map(_) => "object",
        Kind::Array(_) => "array",
        Kind::Enum(_) => "string",
        Kind::Suggest(_) | Kind::SuggestDiscovered(_) | Kind::Scalar => return None,
    };
    let got = match actual {
        JValue::Object(_) => "object",
        JValue::Array(_) => "array",
        JValue::String(_) => "string",
        JValue::Other => return None,
    };
    if want != got {
        Some(format!("expected {want} here, found {got}"))
    } else {
        None
    }
}

pub fn error_at(
    source: &str,
    start: usize,
    end: usize,
    message: String,
    origin: &str,
) -> Diagnostic {
    Diagnostic {
        range: range_at(source, start, end),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some(origin.to_string()),
        message,
        ..Default::default()
    }
}

pub fn warn(source: &str, start: usize, end: usize, message: String, origin: &str) -> Diagnostic {
    Diagnostic {
        range: range_at(source, start, end),
        severity: Some(DiagnosticSeverity::WARNING),
        source: Some(origin.to_string()),
        message,
        ..Default::default()
    }
}

pub fn range_at(source: &str, start: usize, end: usize) -> Range {
    Range::new(
        offset_to_position(source, start),
        offset_to_position(source, end),
    )
}

/// The range of the first object key whose decoded value is `key`, for
/// diagnostics that belong to a section rather than to one value.
pub fn find_key_range(source: &str, key: &str) -> Option<Range> {
    let (value, code_map) = JsonValue::parse_str(source).ok()?;
    let span = find_key_span(&value, &code_map, 0, key)?;
    Some(range_at(source, span.0, span.1))
}

// ---------------------------------------------------------------------------
// The strict parser is used only for diagnostics on input that serde_json
// already accepted. Its code map records byte spans for every value and key,
// so decoded strings and source ranges stay separate.
// ---------------------------------------------------------------------------

type Span = (usize, usize);

struct JNode {
    span: Span,
    value: JValue,
}

enum JValue {
    Object(Vec<Member>),
    Array(Vec<JNode>),
    String(String),
    /// Numbers, booleans, null — diagnostics don't distinguish these.
    Other,
}

struct Member {
    key: String,
    key_span: Span,
    value: JNode,
}

impl JNode {
    fn parse(source: &str) -> Option<JNode> {
        let (value, code_map) = JsonValue::parse_str(source).ok()?;
        Some(from_json_value(&value, &code_map, 0))
    }
}

fn source_span(code_map: &CodeMap, offset: usize) -> Span {
    let span = code_map[offset].span;
    (span.start(), span.end())
}

fn from_json_value(value: &JsonValue, code_map: &CodeMap, offset: usize) -> JNode {
    let span = source_span(code_map, offset);
    let value = match value {
        JsonValue::Object(object) => JValue::Object(
            object
                .iter_mapped(code_map, offset)
                .map(|mapped| Member {
                    key: mapped.value.key.value.to_string(),
                    key_span: source_span(code_map, mapped.value.key.offset),
                    value: from_json_value(
                        mapped.value.value.value,
                        code_map,
                        mapped.value.value.offset,
                    ),
                })
                .collect(),
        ),
        JsonValue::Array(array) => JValue::Array(
            array
                .iter_mapped(code_map, offset)
                .map(|mapped| from_json_value(mapped.value, code_map, mapped.offset))
                .collect(),
        ),
        JsonValue::String(string) => JValue::String(string.to_string()),
        JsonValue::Null | JsonValue::Boolean(_) | JsonValue::Number(_) => JValue::Other,
    };

    JNode { span, value }
}

fn find_key_span(value: &JsonValue, code_map: &CodeMap, offset: usize, key: &str) -> Option<Span> {
    match value {
        JsonValue::Object(object) => {
            for mapped in object.iter_mapped(code_map, offset) {
                if mapped.value.key.value.as_str() == key {
                    return Some(source_span(code_map, mapped.value.key.offset));
                }
                if let Some(span) = find_key_span(
                    mapped.value.value.value,
                    code_map,
                    mapped.value.value.offset,
                    key,
                ) {
                    return Some(span);
                }
            }
            None
        }
        JsonValue::Array(array) => array
            .iter_mapped(code_map, offset)
            .find_map(|mapped| find_key_span(mapped.value, code_map, mapped.offset, key)),
        JsonValue::Null | JsonValue::Boolean(_) | JsonValue::Number(_) | JsonValue::String(_) => {
            None
        }
    }
}
