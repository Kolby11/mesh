//! Brace lexer/parser for template expressions and control-flow directives.
//!
//! Braces are deliberately parsed before markup is lowered through quick-xml.
//! The XML lowering step is useful for ordinary element structure, but it is
//! not a language parser and cannot preserve the source ownership of rewritten
//! control-flow or interpolation tokens.

use super::ParseError;
use crate::SourceSpan;

/// Literal `window=` / `overscan=` options of a `{#for}` header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct VirtualWindowOption {
    pub(super) item_extent: f32,
    pub(super) overscan: usize,
}

#[derive(Debug, Clone)]
pub(super) struct BraceToken {
    pub(super) span: SourceSpan,
    pub(super) kind: BraceKind,
    /// For an opening control-flow token, the end of its matching close token.
    pub(super) matching_end: Option<usize>,
}

#[derive(Debug, Clone)]
pub(super) enum BraceKind {
    Expression {
        expression: SourceSpan,
    },
    IfOpen {
        condition: SourceSpan,
    },
    ForOpen {
        item: String,
        iterable: SourceSpan,
        key: Option<SourceSpan>,
        window: Option<VirtualWindowOption>,
    },
    ElseIf {
        condition: SourceSpan,
    },
    Else,
    IfClose,
    ForClose,
}

#[derive(Debug, Clone)]
pub(super) struct BraceLex {
    pub(super) tokens: Vec<BraceToken>,
}

impl BraceLex {
    pub(super) fn token(&self, id: usize) -> Option<&BraceToken> {
        self.tokens.get(id)
    }

    pub(super) fn marker(id: usize) -> String {
        format!("__mesh_expr_{id}__")
    }

    pub(super) fn quoted_marker(id: usize) -> String {
        format!("__mesh_quoted_expr_{id}__")
    }

    pub(super) fn marker_id(value: &str) -> Option<usize> {
        let value = value.strip_prefix("__mesh_expr_")?.strip_suffix("__")?;
        value.parse().ok()
    }

    pub(super) fn quoted_marker_id(value: &str) -> Option<usize> {
        let value = value
            .strip_prefix("__mesh_quoted_expr_")?
            .strip_suffix("__")?;
        value.parse().ok()
    }
}

pub(super) fn lex(source: &str) -> Result<BraceLex, ParseError> {
    let mut tokens = Vec::new();
    let mut cursor = 0;

    while cursor < source.len() {
        let ch = source[cursor..]
            .chars()
            .next()
            .expect("cursor stays on a UTF-8 boundary");
        if ch == '{' {
            let close = scan_braced_expression(source, cursor)?;
            let token = classify(source, cursor, close)?;
            tokens.push(token);
            cursor = close;
        } else if ch == '}' {
            return Err(error_at(
                source,
                cursor,
                "unexpected `}`; every closing brace must close an interpolation or directive",
            ));
        } else {
            cursor += ch.len_utf8();
        }
    }

    validate_control_flow(source, &mut tokens)?;
    Ok(BraceLex { tokens })
}

fn classify(source: &str, start: usize, close: usize) -> Result<BraceToken, ParseError> {
    let body_start = start + 1;
    let body_end = close - 1;
    let body = trimmed_span(source, body_start, body_end);
    if body.start == body.end {
        return Err(error_at(
            source,
            start,
            "empty interpolation or control-flow directive",
        ));
    }

    let text = &source[body.start..body.end];
    let kind = if let Some(rest) = text.strip_prefix("#if") {
        if !rest.chars().next().is_some_and(char::is_whitespace) {
            return Err(error_at(source, start, "unknown `#` directive"));
        }
        let condition = trimmed_span(source, body.start + 3, body.end);
        validate_expression(source, condition, "if condition")?;
        BraceKind::IfOpen { condition }
    } else if let Some(rest) = text.strip_prefix("#for") {
        if !rest.chars().next().is_some_and(char::is_whitespace) {
            return Err(error_at(source, start, "unknown `#` directive"));
        }
        parse_for_header(source, body.start + 4, body.end)?
    } else if let Some(rest) = text.strip_prefix(":else if") {
        if !rest.starts_with(char::is_whitespace) {
            return Err(error_at(source, start, "malformed `:else if` directive"));
        }
        let condition = trimmed_span(source, body.start + 8, body.end);
        validate_expression(source, condition, "else-if condition")?;
        BraceKind::ElseIf { condition }
    } else if text == ":else" {
        BraceKind::Else
    } else if text == "/if" {
        BraceKind::IfClose
    } else if text == "/for" {
        BraceKind::ForClose
    } else if text.starts_with('#') || text.starts_with(':') || text.starts_with('/') {
        return Err(error_at(
            source,
            start,
            "unknown or malformed control-flow directive",
        ));
    } else {
        let expression = trimmed_span(source, body_start, body_end);
        validate_expression(source, expression, "interpolation")?;
        BraceKind::Expression { expression }
    };

    Ok(BraceToken {
        span: SourceSpan::new(start, close),
        kind,
        matching_end: None,
    })
}

fn parse_for_header(
    source: &str,
    body_start: usize,
    body_end: usize,
) -> Result<BraceKind, ParseError> {
    let header = trimmed_span(source, body_start, body_end);
    let Some(in_start) = find_top_level_word(source, header, "in") else {
        return Err(error_at(
            source,
            body_start,
            "`#for` must have the form `item in iterable`",
        ));
    };
    let item = trimmed_span(source, header.start, in_start);
    if item.start == item.end || !is_identifier(&source[item.start..item.end]) {
        return Err(error_at(
            source,
            item.start,
            "`#for` item must be an identifier",
        ));
    }

    let after_in = trimmed_span(source, in_start + 2, header.end);
    let mut markers = Vec::new();
    for name in ["key", "window", "overscan"] {
        if let Some(start) = find_top_level_option(source, after_in, name) {
            markers.push((start, name));
        }
    }
    markers.sort_unstable();
    let iterable = trimmed_span(
        source,
        after_in.start,
        markers.first().map_or(after_in.end, |(start, _)| *start),
    );
    let mut key = None;
    let mut window_extent = None;
    let mut overscan = None;
    for (index, (marker_start, name)) in markers.iter().enumerate() {
        let value_end = markers
            .get(index + 1)
            .map_or(after_in.end, |(next, _)| *next);
        let value_start = skip_ascii_whitespace(source, marker_start + name.len());
        if source.as_bytes().get(value_start) != Some(&b'=') {
            return Err(error_at(
                source,
                *marker_start,
                &format!("expected `=` after `{name}`"),
            ));
        }
        let value = trimmed_span(source, value_start + 1, value_end);
        match *name {
            "key" => {
                key = Some(if source.as_bytes().get(value.start) == Some(&b'{') {
                    if source.as_bytes().get(value.end.saturating_sub(1)) != Some(&b'}') {
                        return Err(error_at(
                            source,
                            value.start,
                            "unclosed `key={...}` expression",
                        ));
                    }
                    trimmed_span(source, value.start + 1, value.end - 1)
                } else {
                    value
                });
            }
            "window" => {
                let text = unquote_literal(&source[value.start..value.end]);
                let extent = text.parse::<f32>().ok().filter(|v| v.is_finite() && *v > 0.0);
                window_extent = Some(extent.ok_or_else(|| {
                    error_at(
                        source,
                        value.start,
                        "`window` must be a positive item height or width in px",
                    )
                })?);
            }
            _ => {
                let text = unquote_literal(&source[value.start..value.end]);
                overscan = Some(text.parse::<usize>().map_err(|_| {
                    error_at(
                        source,
                        value.start,
                        "`overscan` must be a non-negative whole number of items",
                    )
                })?);
            }
        }
    }
    if overscan.is_some() && window_extent.is_none() {
        return Err(error_at(
            source,
            body_start,
            "`overscan` requires `window` on the same `#for`",
        ));
    }
    let window = window_extent.map(|item_extent| VirtualWindowOption {
        item_extent,
        overscan: overscan.unwrap_or(crate::DEFAULT_VIRTUAL_OVERSCAN),
    });

    if iterable.start == iterable.end {
        return Err(error_at(
            source,
            body_start,
            "`#for` iterable expression is empty",
        ));
    }
    validate_expression(source, iterable, "for iterable")?;
    if let Some(key) = key {
        if key.start == key.end {
            return Err(error_at(
                source,
                key.start,
                "`#for` key expression is empty",
            ));
        }
        validate_expression(source, key, "for key")?;
    }

    Ok(BraceKind::ForOpen {
        item: source[item.start..item.end].to_string(),
        iterable,
        key,
        window,
    })
}

fn unquote_literal(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(text)
}

fn validate_control_flow(source: &str, tokens: &mut [BraceToken]) -> Result<(), ParseError> {
    #[derive(Clone, Copy)]
    enum Open {
        If { index: usize, has_else: bool },
        For { index: usize },
    }

    let mut stack = Vec::<Open>::new();
    for index in 0..tokens.len() {
        match tokens[index].kind {
            BraceKind::IfOpen { .. } => stack.push(Open::If {
                index,
                has_else: false,
            }),
            BraceKind::ForOpen { .. } => stack.push(Open::For { index }),
            BraceKind::ElseIf { .. } | BraceKind::Else => {
                let Some(Open::If {
                    index: open,
                    has_else,
                }) = stack.last_mut()
                else {
                    return Err(error_at(
                        source,
                        tokens[index].span.start,
                        "`else` is only valid inside an open `if` directive",
                    ));
                };
                if *has_else {
                    return Err(error_at(
                        source,
                        tokens[index].span.start,
                        "an `if` directive can have only one else branch",
                    ));
                }
                if matches!(&tokens[index].kind, BraceKind::Else) {
                    *has_else = true;
                }
                let _ = open;
            }
            BraceKind::IfClose => match stack.pop() {
                Some(Open::If { index: open, .. }) => {
                    tokens[open].matching_end = Some(tokens[index].span.end);
                }
                Some(Open::For { .. }) => {
                    return Err(error_at(
                        source,
                        tokens[index].span.start,
                        "`{/if}` closes an open `for`; expected `{/for}`",
                    ));
                }
                None => {
                    return Err(error_at(
                        source,
                        tokens[index].span.start,
                        "unexpected `{/if}`",
                    ));
                }
            },
            BraceKind::ForClose => match stack.pop() {
                Some(Open::For { index: open }) => {
                    tokens[open].matching_end = Some(tokens[index].span.end);
                }
                Some(Open::If { .. }) => {
                    return Err(error_at(
                        source,
                        tokens[index].span.start,
                        "`{/for}` closes an open `if`; expected `{/if}`",
                    ));
                }
                None => {
                    return Err(error_at(
                        source,
                        tokens[index].span.start,
                        "unexpected `{/for}`",
                    ));
                }
            },
            BraceKind::Expression { .. } => {}
        }
    }

    if let Some(open) = stack.last() {
        let index = match open {
            Open::If { index, .. } | Open::For { index } => *index,
        };
        return Err(error_at(
            source,
            tokens[index].span.start,
            "control-flow directive is not closed",
        ));
    }
    Ok(())
}

fn scan_braced_expression(source: &str, start: usize) -> Result<usize, ParseError> {
    let mut cursor = start + 1;
    let mut curly = 1usize;
    let mut parentheses = 0usize;
    let mut brackets = 0usize;

    while cursor < source.len() {
        if let Some(end) = scan_short_string(source, cursor)? {
            cursor = end;
            continue;
        }
        if source[cursor..].starts_with("--") {
            cursor += 2;
            if source[cursor..].starts_with('[') {
                let Some(end) = scan_long_bracket_string(source, cursor) else {
                    return Err(error_at(
                        source,
                        cursor,
                        "unterminated long comment in expression",
                    ));
                };
                cursor = end;
            } else {
                while cursor < source.len() {
                    let ch = source[cursor..]
                        .chars()
                        .next()
                        .expect("cursor stays on a UTF-8 boundary");
                    cursor += ch.len_utf8();
                    if ch == '\n' {
                        break;
                    }
                }
            }
            continue;
        }
        if source[cursor..].starts_with('[') {
            if let Some(end) = scan_long_bracket_string(source, cursor) {
                cursor = end;
                continue;
            }
        }

        let ch = source[cursor..]
            .chars()
            .next()
            .expect("cursor stays on a UTF-8 boundary");
        match ch {
            '{' => curly += 1,
            '}' => {
                if curly == 1 && (parentheses != 0 || brackets != 0) {
                    return Err(error_at(
                        source,
                        cursor,
                        "closing brace appears before a balanced expression",
                    ));
                }
                curly -= 1;
                if curly == 0 {
                    return Ok(cursor + 1);
                }
            }
            '(' => parentheses += 1,
            ')' => {
                if parentheses == 0 {
                    return Err(error_at(source, cursor, "unmatched `)` in expression"));
                }
                parentheses -= 1;
            }
            '[' => brackets += 1,
            ']' => {
                if brackets == 0 {
                    return Err(error_at(source, cursor, "unmatched `]` in expression"));
                }
                brackets -= 1;
            }
            _ => {}
        }
        cursor += ch.len_utf8();
    }

    Err(error_at(
        source,
        start,
        "unterminated interpolation or control-flow directive",
    ))
}

fn scan_short_string(source: &str, start: usize) -> Result<Option<usize>, ParseError> {
    let quote = source.as_bytes().get(start).copied();
    if !matches!(quote, Some(b'\'' | b'"')) {
        return Ok(None);
    }
    let mut cursor = start + 1;
    while cursor < source.len() {
        let ch = source[cursor..]
            .chars()
            .next()
            .expect("cursor stays on a UTF-8 boundary");
        if ch == '\\' {
            cursor += ch.len_utf8();
            if cursor < source.len() {
                cursor += source[cursor..]
                    .chars()
                    .next()
                    .expect("escaped character is valid UTF-8")
                    .len_utf8();
            }
            continue;
        }
        cursor += ch.len_utf8();
        if quote == Some(ch as u8) {
            return Ok(Some(cursor));
        }
    }
    Err(error_at(source, start, "unterminated string in expression"))
}

fn scan_long_bracket_string(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut cursor = start + 1;
    while bytes.get(cursor) == Some(&b'=') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'[') {
        return None;
    }
    let equals = cursor - start - 1;
    let close = format!("]{}]", "=".repeat(equals));
    let content_start = cursor + 1;
    source[content_start..]
        .find(&close)
        .map(|offset| content_start + offset + close.len())
}

fn validate_expression(source: &str, span: SourceSpan, context: &str) -> Result<(), ParseError> {
    if span.start == span.end {
        return Err(error_at(source, span.start, format!("{context} is empty")));
    }
    let expression = source[span.start..span.end].to_string();
    crate::compile_expression(&expression).map_err(|error| {
        error_at(
            source,
            span.start,
            format!("malformed Luau {context}: {error}"),
        )
    })?;
    Ok(())
}

fn find_top_level_word(source: &str, span: SourceSpan, word: &str) -> Option<usize> {
    let mut cursor = span.start;
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    while cursor < span.end {
        if let Ok(Some(end)) = scan_short_string(source, cursor) {
            cursor = end;
            continue;
        }
        let ch = source[cursor..].chars().next()?;
        match ch {
            '(' => parentheses += 1,
            ')' => parentheses = parentheses.saturating_sub(1),
            '[' => brackets += 1,
            ']' => brackets = brackets.saturating_sub(1),
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            _ => {}
        }
        if parentheses == 0
            && brackets == 0
            && braces == 0
            && source[cursor..].starts_with(word)
            && (cursor == span.start || !is_identifier_char(source[..cursor].chars().next_back()?))
            && (cursor + word.len() == span.end
                || !is_identifier_char(source[cursor + word.len()..].chars().next()?))
        {
            return Some(cursor);
        }
        cursor += ch.len_utf8();
    }
    None
}

fn find_top_level_option(source: &str, span: SourceSpan, name: &str) -> Option<usize> {
    let mut cursor = span.start;
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    while cursor < span.end {
        if let Ok(Some(end)) = scan_short_string(source, cursor) {
            cursor = end;
            continue;
        }
        let ch = source[cursor..].chars().next()?;
        match ch {
            '(' => parentheses += 1,
            ')' => parentheses = parentheses.saturating_sub(1),
            '[' => brackets += 1,
            ']' => brackets = brackets.saturating_sub(1),
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            _ => {}
        }
        if parentheses == 0
            && brackets == 0
            && braces == 0
            && source[cursor..].starts_with(name)
            && (cursor == span.start || source.as_bytes()[cursor - 1].is_ascii_whitespace())
            && source[cursor + name.len()..]
                .chars()
                .next()
                .is_some_and(|next| next.is_ascii_whitespace() || next == '=')
            && (name == "key" || is_single_assignment(source, cursor + name.len()))
        {
            return Some(cursor);
        }
        cursor += ch.len_utf8();
    }
    None
}

fn trimmed_span(source: &str, start: usize, end: usize) -> SourceSpan {
    let mut start = start;
    let mut end = end;
    while start < end {
        let ch = source[start..].chars().next().expect("valid span");
        if !ch.is_whitespace() {
            break;
        }
        start += ch.len_utf8();
    }
    while end > start {
        let ch = source[..end].chars().next_back().expect("valid span");
        if !ch.is_whitespace() {
            break;
        }
        end -= ch.len_utf8();
    }
    SourceSpan::new(start, end)
}

/// Whether the next non-space character at `cursor` is `=` but not `==`, so an
/// iterable that merely names a variable `window` is not read as an option.
fn is_single_assignment(source: &str, cursor: usize) -> bool {
    let cursor = skip_ascii_whitespace(source, cursor);
    source.as_bytes().get(cursor) == Some(&b'=') && source.as_bytes().get(cursor + 1) != Some(&b'=')
}

fn skip_ascii_whitespace(source: &str, mut cursor: usize) -> usize {
    while cursor < source.len() && source.as_bytes()[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    cursor
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic()) && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn error_at(source: &str, offset: usize, message: impl Into<String>) -> ParseError {
    let line = source[..offset.min(source.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = source[..offset.min(source.len())]
        .rsplit('\n')
        .next()
        .map_or(1, |line| line.chars().count() + 1);
    ParseError::InvalidTemplate {
        message: format!(
            "{} at line {line}, column {column} (byte {})",
            message.into(),
            offset
        ),
        span: SourceSpan::new(offset, offset.saturating_add(1).min(source.len())),
    }
}
