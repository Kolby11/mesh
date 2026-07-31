//! Schema-driven diagnostics for JSON documents: syntax, unknown keys, enum
//! violations, missing required fields, and structural type mismatches.
//!
//! Document-specific rules a schema tree cannot express (the root graph
//! config's canonical validation, for instance) are layered on by the caller.

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
                        if let Some(v) = &m.value {
                            check_node(source, v, &field.node, origin, out);
                        }
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
                let Some(v) = &m.value else { continue };
                let target = fields
                    .iter()
                    .find(|f| f.name == m.key)
                    .map(|f| &f.node)
                    .unwrap_or(other.as_ref());
                check_node(source, v, target, origin, out);
            }
        }
        (JValue::Object(members), Kind::Map(value)) => {
            for m in members {
                if let Some(v) = &m.value {
                    check_node(source, v, value, origin, out);
                }
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

/// The range of the first `"key"` token in `source`, for diagnostics that
/// belong to a section rather than to one value.
pub fn find_key_range(source: &str, key: &str) -> Option<Range> {
    let needle = format!("\"{key}\"");
    let start = source.find(&needle)?;
    Some(range_at(source, start, start + needle.len()))
}

// ---------------------------------------------------------------------------
// A minimal strict span-recording JSON parser, used only for diagnostics on
// input that serde_json already accepted. It records byte spans for values and
// object keys so diagnostics can point at the exact token.
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
    value: Option<JNode>,
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl JNode {
    fn parse(source: &str) -> Option<JNode> {
        let mut p = Parser {
            bytes: source.as_bytes(),
            pos: 0,
        };
        p.skip_ws();
        p.parse_value()
    }
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\r' | b'\n' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_value(&mut self) -> Option<JNode> {
        self.skip_ws();
        match self.peek()? {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => {
                let start = self.pos;
                let s = self.parse_string()?;
                Some(JNode {
                    span: (start, self.pos),
                    value: JValue::String(s),
                })
            }
            _ => {
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if matches!(c, b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n') {
                        break;
                    }
                    self.pos += 1;
                }
                Some(JNode {
                    span: (start, self.pos),
                    value: JValue::Other,
                })
            }
        }
    }

    fn parse_object(&mut self) -> Option<JNode> {
        let start = self.pos;
        self.pos += 1; // {
        let mut members = Vec::new();
        loop {
            self.skip_ws();
            match self.peek()? {
                b'}' => {
                    self.pos += 1;
                    break;
                }
                b'"' => {
                    let key_start = self.pos;
                    let key = self.parse_string()?;
                    let key_span = (key_start, self.pos);
                    self.skip_ws();
                    if self.peek() == Some(b':') {
                        self.pos += 1;
                    }
                    let value = self.parse_value();
                    members.push(Member {
                        key,
                        key_span,
                        value,
                    });
                    self.skip_ws();
                    if self.peek() == Some(b',') {
                        self.pos += 1;
                    }
                }
                b',' => {
                    self.pos += 1;
                }
                _ => return None,
            }
        }
        Some(JNode {
            span: (start, self.pos),
            value: JValue::Object(members),
        })
    }

    fn parse_array(&mut self) -> Option<JNode> {
        let start = self.pos;
        self.pos += 1; // [
        let mut elements = Vec::new();
        loop {
            self.skip_ws();
            match self.peek()? {
                b']' => {
                    self.pos += 1;
                    break;
                }
                b',' => {
                    self.pos += 1;
                }
                _ => {
                    elements.push(self.parse_value()?);
                }
            }
        }
        Some(JNode {
            span: (start, self.pos),
            value: JValue::Array(elements),
        })
    }

    fn parse_string(&mut self) -> Option<String> {
        debug_assert_eq!(self.peek(), Some(b'"'));
        self.pos += 1;
        let mut s = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                b'"' => return Some(s),
                b'\\' => {
                    if let Some(esc) = self.peek() {
                        self.pos += 1;
                        s.push(esc as char);
                    }
                }
                _ => s.push(c as char),
            }
        }
        None
    }
}
