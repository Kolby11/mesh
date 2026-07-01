use mesh_core_component::parser::ParseError;
use mesh_core_elements::{BASE_ELEMENT_FIELDS, element_contract_for_tag, element_type_for_tag};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::document::{Document, ElementRefAliasTarget, block_content_range, extract_block_text};

pub fn from_document(doc: &Document) -> Vec<Diagnostic> {
    if let Some(err) = &doc.parse_error {
        return diagnostics_from_error(err, &doc.source);
    }

    let mut diagnostics = diagnostics_from_script_refs(doc);
    diagnostics.extend(diagnostics_from_quoted_expression_attrs(doc));
    diagnostics
}

fn diagnostics_from_error(err: &ParseError, source: &str) -> Vec<Diagnostic> {
    match err {
        ParseError::UnclosedBlock { tag, line } => vec![make_diag(
            source,
            "template",
            *line,
            0,
            format!("Unclosed block <{tag}>"),
            DiagnosticSeverity::ERROR,
        )],

        ParseError::UnexpectedClose { tag, line } => vec![make_diag(
            source,
            "template",
            *line,
            0,
            format!("Unexpected closing tag </{tag}>"),
            DiagnosticSeverity::ERROR,
        )],

        ParseError::InvalidTemplate { message } => vec![make_diag(
            source,
            "template",
            0,
            0,
            format!("Template error: {message}"),
            DiagnosticSeverity::ERROR,
        )],

        ParseError::InvalidStyle { message, line } => vec![make_diag(
            source,
            "style",
            *line,
            0,
            format!("Style error: {message}"),
            DiagnosticSeverity::ERROR,
        )],

        ParseError::InvalidProps { message } => vec![make_diag(
            source,
            "props",
            0,
            0,
            format!("Props error: {message}"),
            DiagnosticSeverity::ERROR,
        )],

        ParseError::InvalidI18n(msg) => vec![make_diag(
            source,
            "i18n",
            0,
            0,
            format!("i18n error: {msg}"),
            DiagnosticSeverity::ERROR,
        )],

        ParseError::InvalidImport { line, message } => vec![make_diag(
            source,
            "script",
            *line,
            0,
            format!("Import error: {message}"),
            DiagnosticSeverity::ERROR,
        )],

        ParseError::UnknownBlock { name, line } => vec![make_diag(
            source,
            "template",
            *line,
            0,
            format!("Unknown block <{name}>"),
            DiagnosticSeverity::WARNING,
        )],
    }
}

/// Build a diagnostic anchored to the start of a named block, offset by `line_in_block`
/// (0 = report at the opening tag line of the block; N = N lines into the block content).
fn make_diag(
    source: &str,
    block_name: &str,
    line_in_block: usize,
    _col: usize,
    message: String,
    severity: DiagnosticSeverity,
) -> Diagnostic {
    let range = diag_range(source, block_name, line_in_block);
    Diagnostic {
        range,
        severity: Some(severity),
        message,
        source: Some("mesh-tools-lsp".to_string()),
        ..Default::default()
    }
}

fn diag_range(source: &str, block_name: &str, line_in_block: usize) -> Range {
    // Find the start byte of the block
    let block_start_byte = block_content_range(source, block_name)
        .map(|(s, _)| s)
        .unwrap_or(0);

    // Count newlines in source up to block_start_byte to get the absolute line number
    let block_start_line = source[..block_start_byte]
        .chars()
        .filter(|&c| c == '\n')
        .count() as u32;

    // line_in_block: 0 → block opening line, 1 → first content line, etc.
    // ParseError lines are 1-based when they refer to source lines, 0 when unknown.
    let abs_line = if line_in_block == 0 {
        block_start_line
    } else {
        block_start_line + (line_in_block as u32).saturating_sub(1)
    };

    Range {
        start: Position {
            line: abs_line,
            character: 0,
        },
        end: Position {
            line: abs_line,
            character: u32::MAX,
        },
    }
}

fn diagnostics_from_script_refs(doc: &Document) -> Vec<Diagnostic> {
    let Some((script_start, _)) = block_content_range(&doc.source, "script") else {
        return vec![];
    };
    let script = extract_block_text(&doc.source, "script");
    let mut diagnostics = Vec::new();
    let mut offset = 0;

    while let Some(relative) = script[offset..].find("refs.") {
        let refs_start = offset + relative;
        let name_start = refs_start + "refs.".len();
        let Some((ref_name, name_end)) = parse_identifier_at(script, name_start) else {
            offset = name_start;
            continue;
        };

        let Some(element_ref) = doc
            .element_refs
            .iter()
            .find(|element_ref| element_ref.name == ref_name)
        else {
            diagnostics.push(Diagnostic {
                range: byte_range_to_lsp_range(
                    &doc.source,
                    script_start + refs_start,
                    script_start + name_end,
                ),
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!(
                    "Unknown element ref `refs.{ref_name}`. Add `ref=\"{ref_name}\"` to a template element."
                ),
                source: Some("mesh-tools-lsp".to_string()),
                ..Default::default()
            });
            offset = name_end;
            continue;
        };

        if script[name_end..].starts_with('.') {
            let field_start = name_end + 1;
            if let Some((field_name, field_end)) = parse_identifier_at(script, field_start) {
                if !element_field_exists(&element_ref.tag, field_name) {
                    diagnostics.push(Diagnostic {
                        range: byte_range_to_lsp_range(
                            &doc.source,
                            script_start + field_start,
                            script_start + field_end,
                        ),
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!(
                            "`{}` does not expose field `{field_name}`",
                            element_ref.element_type
                        ),
                        source: Some("mesh-tools-lsp".to_string()),
                        ..Default::default()
                    });
                }
                offset = field_end;
                continue;
            }
        }

        offset = name_end;
    }

    for alias in &doc.element_ref_aliases {
        let ElementRefAliasTarget::Ref(ref_name) = &alias.target else {
            continue;
        };
        let Some(element_ref) = doc
            .element_refs
            .iter()
            .find(|element_ref| element_ref.name == *ref_name)
        else {
            continue;
        };
        let mut offset = 0;
        let needle = format!("{}.", alias.alias);
        while let Some(relative) = script[offset..].find(&needle) {
            let member_start = offset + relative + needle.len();
            let Some((field_name, field_end)) = parse_identifier_at(script, member_start) else {
                offset = member_start;
                continue;
            };
            if !element_field_exists(&element_ref.tag, field_name) {
                diagnostics.push(Diagnostic {
                    range: byte_range_to_lsp_range(
                        &doc.source,
                        script_start + member_start,
                        script_start + field_end,
                    ),
                    severity: Some(DiagnosticSeverity::WARNING),
                    message: format!(
                        "`{}` does not expose field `{field_name}`",
                        element_ref.element_type
                    ),
                    source: Some("mesh-tools-lsp".to_string()),
                    ..Default::default()
                });
            }
            offset = field_end;
        }
    }

    diagnostics
}

fn diagnostics_from_quoted_expression_attrs(doc: &Document) -> Vec<Diagnostic> {
    let Some((template_start, template_end)) = block_content_range(&doc.source, "template") else {
        return vec![];
    };
    let template = &doc.source[template_start..template_end];
    let mut diagnostics = Vec::new();
    let mut offset = 0usize;

    while let Some(relative) = template[offset..].find("=\"") {
        let equals = offset + relative;
        let value_start = equals + 2;
        let Some(value_end) = find_string_end(template, value_start, b'"') else {
            break;
        };
        let value = &template[value_start..value_end];

        if is_exact_brace_expr(value) && is_inside_template_tag(template, equals) {
            diagnostics.push(Diagnostic {
                range: byte_range_to_lsp_range(
                    &doc.source,
                    template_start + equals,
                    template_start + value_end + 1,
                ),
                severity: Some(DiagnosticSeverity::WARNING),
                message: "Quoted expression attribute can be written as `attr={expr}` instead of `attr=\"{expr}\"`.".to_string(),
                source: Some("mesh-tools-lsp".to_string()),
                ..Default::default()
            });
        }

        offset = value_end + 1;
    }

    diagnostics
}

fn find_string_end(source: &str, start: usize, quote: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == quote && (i == 0 || bytes[i - 1] != b'\\') {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_inside_template_tag(template: &str, offset: usize) -> bool {
    let before = &template[..offset];
    let last_lt = before.rfind('<');
    let last_gt = before.rfind('>');
    matches!((last_lt, last_gt), (Some(lt), Some(gt)) if lt > gt)
        || matches!((last_lt, last_gt), (Some(_), None))
}

fn is_exact_brace_expr(value: &str) -> bool {
    let trimmed = value.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed.len() < 2 {
        return false;
    }

    let bytes = trimmed.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut quote = b'"';

    for (i, b) in bytes.iter().copied().enumerate() {
        if in_string {
            if b == quote && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
            continue;
        }

        if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
            continue;
        }

        if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 && i != bytes.len() - 1 {
                return false;
            }
            if depth < 0 {
                return false;
            }
        }
    }

    depth == 0
}

fn parse_identifier_at(source: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = source.as_bytes();
    let first = *bytes.get(start)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }

    let mut end = start + 1;
    while let Some(byte) = bytes.get(end) {
        if byte.is_ascii_alphanumeric() || *byte == b'_' {
            end += 1;
        } else {
            break;
        }
    }

    Some((&source[start..end], end))
}

fn element_field_exists(tag: &str, field_name: &str) -> bool {
    let type_def = element_type_for_tag(tag);
    BASE_ELEMENT_FIELDS
        .iter()
        .chain(type_def.fields.iter())
        .any(|field| field.name == field_name)
        || element_contract_for_tag(tag).is_some_and(|contract| {
            contract.attributes.iter().any(|attribute| {
                crate::analyzer::script::is_script_member_attribute(attribute.name)
                    && (attribute.name == field_name
                        || crate::analyzer::script::attribute_member_name(attribute.name)
                            == field_name)
            })
        })
}

fn byte_range_to_lsp_range(source: &str, start: usize, end: usize) -> Range {
    Range {
        start: byte_offset_to_position(source, start),
        end: byte_offset_to_position(source, end),
    }
}

fn byte_offset_to_position(source: &str, target: usize) -> Position {
    let mut line = 0u32;
    let mut line_start = 0usize;

    for (index, ch) in source.char_indices() {
        if index >= target {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = index + 1;
        }
    }

    Position {
        line,
        character: target.saturating_sub(line_start) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    #[test]
    fn warns_for_unknown_refs_and_invalid_element_fields() {
        let source = r#"
<template>
  <icon ref="batteryIcon" name="battery-full" />
</template>

<script lang="luau">
local ok = refs.batteryIcon.name
local missing = refs.notReal.width
local invalid = refs.batteryIcon.value
</script>
"#;
        let doc = Document::new(
            Url::parse("file:///tmp/battery-button.mesh").unwrap(),
            source.to_string(),
        );

        let diagnostics = from_document(&doc);

        assert_eq!(diagnostics.len(), 2);
        assert!(
            diagnostics[0]
                .message
                .contains("Unknown element ref `refs.notReal`")
        );
        assert!(
            diagnostics[1]
                .message
                .contains("`IconElement` does not expose field `value`")
        );
    }

    #[test]
    fn bind_this_element_allows_attribute_member_aliases() {
        let source = r#"
<template>
  <popover bind:this={popover} aria-label="Audio controls" />
</template>
<script lang="luau">
popover.ariaLabel
popover.notReal
</script>
"#;
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source.to_string());
        let diags = from_document(&doc);

        assert!(
            !diags.iter().any(|diag| diag.message.contains("ariaLabel")),
            "ariaLabel should resolve as aria-label attribute"
        );
        assert!(
            diags.iter().any(|diag| diag.message.contains("notReal")),
            "unknown direct bind:this member should be diagnosed"
        );
    }

    #[test]
    fn warns_for_quoted_expression_attrs() {
        let source = r#"
<template>
  <button title="{t('nav.open')}" class="chip {active}" onclick="{onTap}" />
</template>

<script lang="luau">
function onTap() end
</script>
"#;
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source.to_string());
        let diags = from_document(&doc);

        let messages = diags
            .iter()
            .filter(|diag| diag.message.contains("Quoted expression attribute"))
            .count();
        assert_eq!(messages, 2);
    }
}
