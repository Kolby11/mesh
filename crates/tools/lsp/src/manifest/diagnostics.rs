//! Diagnostics for manifest documents: JSON syntax, unknown keys, enum
//! violations, missing required fields, structural type mismatches, and the
//! canonical runtime validation rules for the root graph config.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use super::schema::{self, Kind, ManifestFlavor, Node};
use super::{ManifestDocument, line_col_to_offset, offset_to_position};

pub fn diagnostics(doc: &ManifestDocument) -> Vec<Diagnostic> {
    let source = &doc.source;

    // 1. Syntax: a parse failure is fatal — report it and stop.
    let value: serde_json::Value = match serde_json::from_str(source) {
        Ok(value) => value,
        Err(err) => {
            let offset = line_col_to_offset(source, err.line(), err.column());
            return vec![error_at(
                source,
                offset,
                offset + 1,
                format!("JSON syntax error: {err}"),
            )];
        }
    };

    let mut out = Vec::new();

    // 2. Schema walk over the span AST for precise key/enum/type diagnostics.
    if let Some(ast) = JNode::parse(source) {
        let root = schema::root(doc.flavor);
        check_node(source, &ast, &root, &mut out);
    } else {
        // The strict parser only fails on input serde already rejected, so this
        // is unreachable in practice; guard anyway.
        let _ = &value;
    }

    // 3. Canonical runtime validation for the root graph config (schemaVersion,
    // entrypoint format, relative-path rules) which the schema tree can't express.
    if doc.flavor == ManifestFlavor::RootConfig {
        if let Err(err) = mesh_core_module::package::RootModuleGraphManifest::from_json_str(source)
        {
            // Attach to the `mesh` key when we can find it, else the document start.
            let range = find_key_range(source, "mesh").unwrap_or_else(|| range_at(source, 0, 1));
            out.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("mesh-manifest".into()),
                message: format!("invalid root config: {err}"),
                ..Default::default()
            });
        }
    }

    out
}

/// Recursively validate `node` (a JSON AST node) against `schema`.
fn check_node(source: &str, node: &JNode, schema: &Node, out: &mut Vec<Diagnostic>) {
    match (&node.value, &schema.kind) {
        (JValue::Object(members), Kind::Object(fields)) => {
            // Unknown keys.
            for m in members {
                match fields.iter().find(|f| f.name == m.key) {
                    Some(field) => {
                        if let Some(v) = &m.value {
                            check_node(source, v, &field.node, out);
                        }
                    }
                    None => out.push(warn(
                        source,
                        m.key_span.0,
                        m.key_span.1,
                        format!("unknown property `{}`", m.key),
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
                    ));
                }
            }
        }
        (JValue::Object(members), Kind::Map(value)) => {
            for m in members {
                if let Some(v) = &m.value {
                    check_node(source, v, value, out);
                }
            }
        }
        (JValue::Array(elements), Kind::Array(element)) => {
            for e in elements {
                check_node(source, e, element, out);
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
                ));
            }
        }
        // Suggested-value strings are never validated (extensible vocabulary).
        (JValue::String(_), Kind::Suggest(_)) => {}
        // Structural mismatches: a container was expected but a scalar appeared
        // (or vice versa). Scalar schema nodes accept any JSON value.
        (actual, expected) => {
            if let Some(msg) = type_mismatch(actual, expected) {
                out.push(error_at(source, node.span.0, node.span.1, msg));
            }
        }
    }
}

fn type_mismatch(actual: &JValue, expected: &Kind) -> Option<String> {
    let want = match expected {
        Kind::Object(_) | Kind::Map(_) => "object",
        Kind::Array(_) => "array",
        Kind::Enum(_) => "string",
        Kind::Suggest(_) | Kind::Scalar => return None,
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

fn error_at(source: &str, start: usize, end: usize, message: String) -> Diagnostic {
    Diagnostic {
        range: range_at(source, start, end),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("mesh-manifest".into()),
        message,
        ..Default::default()
    }
}

fn warn(source: &str, start: usize, end: usize, message: String) -> Diagnostic {
    Diagnostic {
        range: range_at(source, start, end),
        severity: Some(DiagnosticSeverity::WARNING),
        source: Some("mesh-manifest".into()),
        message,
        ..Default::default()
    }
}

fn range_at(source: &str, start: usize, end: usize) -> Range {
    Range::new(
        offset_to_position(source, start),
        offset_to_position(source, end),
    )
}

fn find_key_range(source: &str, key: &str) -> Option<Range> {
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
        let node = p.parse_value()?;
        Some(node)
    }
}

impl<'a> Parser<'a> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    fn diag(src: &str) -> Vec<Diagnostic> {
        let doc = ManifestDocument::new(
            Url::parse("file:///m/module.json").unwrap(),
            src.to_string(),
        );
        diagnostics(&doc)
    }

    #[test]
    fn flags_unknown_property() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "wat": 1 } }"#;
        let d = diag(src);
        assert!(
            d.iter()
                .any(|d| d.message.contains("unknown property `wat`"))
        );
    }

    #[test]
    fn flags_bad_kind() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontnd" } }"#;
        let d = diag(src);
        assert!(d.iter().any(|d| d.message.contains("not a valid value")));
    }

    #[test]
    fn flags_missing_required() {
        let src = r#"{ "name": "@x/y", "mesh": { "kind": "frontend" } }"#;
        let d = diag(src);
        assert!(
            d.iter()
                .any(|d| d.message.contains("missing required property `version`"))
        );
        assert!(
            d.iter()
                .any(|d| d.message.contains("missing required property `apiVersion`"))
        );
    }

    #[test]
    fn accepts_valid_manifest() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "entry": "src/main.mesh" } }"#;
        let d = diag(src);
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn reports_syntax_error() {
        let src = r#"{ "name": "@x/y" "version": "1.0.0" }"#;
        let d = diag(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("syntax error"));
    }

    #[test]
    fn type_mismatch_for_object_field() {
        let src = r#"{ "name": "@x/y", "version": "1.0.0", "mesh": "nope" }"#;
        let d = diag(src);
        assert!(d.iter().any(|d| d.message.contains("expected object")));
    }
}
