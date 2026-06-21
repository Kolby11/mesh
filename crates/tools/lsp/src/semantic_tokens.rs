use std::collections::HashSet;

use mesh_core_component::ComponentImportTarget;
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensResult,
    SemanticTokensServerCapabilities, WorkDoneProgressOptions,
};

use crate::{
    document::{Document, block_content_range},
    knowledge::tags::TAG_DEFS,
};

const TOKEN_MESH_BUILTIN_ELEMENT: u32 = 0;
const TOKEN_MESH_COMPONENT_ELEMENT: u32 = 1;
const TOKEN_KEYWORD: u32 = 2;
const TOKEN_VARIABLE: u32 = 3;
const TOKEN_FUNCTION: u32 = 4;
const TOKEN_STRING: u32 = 5;
const TOKEN_NUMBER: u32 = 6;
const TOKEN_OPERATOR: u32 = 7;
const TOKEN_PROPERTY: u32 = 8;

pub fn server_capabilities() -> SemanticTokensServerCapabilities {
    SemanticTokensOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        legend: SemanticTokensLegend {
            token_types: vec![
                SemanticTokenType::new("meshBuiltinElement"),
                SemanticTokenType::new("meshComponentElement"),
                SemanticTokenType::KEYWORD,
                SemanticTokenType::VARIABLE,
                SemanticTokenType::FUNCTION,
                SemanticTokenType::STRING,
                SemanticTokenType::NUMBER,
                SemanticTokenType::OPERATOR,
                SemanticTokenType::PROPERTY,
            ],
            token_modifiers: vec![SemanticTokenModifier::DEFAULT_LIBRARY],
        },
        range: Some(false),
        full: Some(SemanticTokensFullOptions::Bool(true)),
    }
    .into()
}

pub fn full(doc: &Document) -> SemanticTokensResult {
    let mut builder = TokenBuilder::new(&doc.source);
    tokenize_top_level_block_tags(&doc.source, &mut builder);
    tokenize_template(doc, &mut builder);

    SemanticTokens {
        result_id: None,
        data: builder.finish(),
    }
    .into()
}

fn tokenize_top_level_block_tags(source: &str, builder: &mut TokenBuilder<'_>) {
    for block in ["template", "script", "style"] {
        let mut search_start = 0;
        while let Some(rel) = source[search_start..].find(block) {
            let start = search_start + rel;
            let before = source[..start].chars().next_back();
            let after = source[start + block.len()..].chars().next();
            if before == Some('<') || before == Some('/') && source[..start].ends_with("</") {
                if after.is_none_or(|ch| ch.is_ascii_whitespace() || ch == '>') {
                    builder.push(start, block.len(), TOKEN_KEYWORD, 0);
                }
            }
            search_start = start + block.len();
        }
    }
}

fn tokenize_template(doc: &Document, builder: &mut TokenBuilder<'_>) {
    let Some((template_start, template_end)) = block_content_range(&doc.source, "template") else {
        return;
    };

    let builtin_tags: HashSet<&'static str> = TAG_DEFS.iter().map(|tag| tag.name).collect();
    let imported_components: HashSet<&str> = doc
        .imports
        .iter()
        .filter(|import| {
            matches!(
                import.target,
                ComponentImportTarget::ComponentLocal(_)
                    | ComponentImportTarget::ComponentModule(_)
            )
        })
        .map(|import| import.alias.as_str())
        .collect();

    let template = &doc.source[template_start..template_end];
    let bytes = template.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'<' => {
                if template[i..].starts_with("<!--") {
                    i += template[i + 4..]
                        .find("-->")
                        .map(|end| end + 7)
                        .unwrap_or(bytes.len() - i);
                    continue;
                }
                i = tokenize_tag(
                    template,
                    i,
                    template_start,
                    &builtin_tags,
                    &imported_components,
                    builder,
                );
            }
            b'{' => {
                i = tokenize_brace_expr(template, i, template_start, builder);
            }
            _ => i += 1,
        }
    }
}

fn tokenize_tag(
    template: &str,
    lt: usize,
    base_offset: usize,
    builtin_tags: &HashSet<&'static str>,
    imported_components: &HashSet<&str>,
    builder: &mut TokenBuilder<'_>,
) -> usize {
    let bytes = template.as_bytes();
    let mut i = lt + 1;
    if i < bytes.len() && bytes[i] == b'/' {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    let tag_start = i;
    while i < bytes.len() && is_tag_name_byte(bytes[i]) {
        i += 1;
    }
    let tag = &template[tag_start..i];
    if !tag.is_empty() {
        if builtin_tags.contains(tag) {
            builder.push(
                base_offset + tag_start,
                tag.len(),
                TOKEN_MESH_BUILTIN_ELEMENT,
                1,
            );
        } else if imported_components.contains(tag) {
            builder.push(
                base_offset + tag_start,
                tag.len(),
                TOKEN_MESH_COMPONENT_ELEMENT,
                0,
            );
        } else if tag.chars().next().is_some_and(|ch| ch.is_ascii_uppercase()) {
            builder.push(
                base_offset + tag_start,
                tag.len(),
                TOKEN_MESH_COMPONENT_ELEMENT,
                0,
            );
        }
    }

    tokenize_attrs_until_tag_end(template, i, base_offset, builder)
}

fn tokenize_attrs_until_tag_end(
    template: &str,
    mut i: usize,
    base_offset: usize,
    builder: &mut TokenBuilder<'_>,
) -> usize {
    let bytes = template.as_bytes();
    let mut quote: Option<u8> = None;

    while i < bytes.len() {
        if let Some(q) = quote {
            if bytes[i] == q {
                quote = None;
                i += 1;
                continue;
            }
            if bytes[i] == b'{' {
                i = tokenize_brace_expr(template, i, base_offset, builder);
            } else {
                i += 1;
            }
            continue;
        }

        match bytes[i] {
            b'\'' | b'"' => {
                quote = Some(bytes[i]);
                i += 1;
            }
            b'>' => return i + 1,
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'>' => return i + 2,
            b'{' => i = tokenize_brace_expr(template, i, base_offset, builder),
            b if is_attr_name_start_byte(b) => {
                let attr_start = i;
                i += 1;
                while i < bytes.len() && is_attr_name_byte(bytes[i]) {
                    i += 1;
                }
                builder.push(base_offset + attr_start, i - attr_start, TOKEN_PROPERTY, 0);
            }
            _ => i += 1,
        }
    }

    i
}

fn tokenize_brace_expr(
    template: &str,
    open: usize,
    base_offset: usize,
    builder: &mut TokenBuilder<'_>,
) -> usize {
    let Some(close) = find_matching_brace(template, open) else {
        builder.push(base_offset + open, 1, TOKEN_OPERATOR, 0);
        return open + 1;
    };

    builder.push(base_offset + open, 1, TOKEN_OPERATOR, 0);
    builder.push(base_offset + close, 1, TOKEN_OPERATOR, 0);

    let mut expr_start = open + 1;
    if matches!(
        template.as_bytes().get(expr_start),
        Some(b'#' | b'/' | b':')
    ) {
        builder.push(base_offset + expr_start, 1, TOKEN_OPERATOR, 0);
        expr_start += 1;
    }
    tokenize_lua_like_expr(
        &template[expr_start..close],
        base_offset + expr_start,
        builder,
    );
    close + 1
}

fn tokenize_lua_like_expr(expr: &str, base_offset: usize, builder: &mut TokenBuilder<'_>) {
    let bytes = expr.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' => {
                let start = i;
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == quote {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                builder.push(base_offset + start, i - start, TOKEN_STRING, 0);
            }
            b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                builder.push(base_offset + start, i - start, TOKEN_NUMBER, 0);
            }
            b if is_lua_ident_start_byte(b) => {
                let start = i;
                i += 1;
                while i < bytes.len() && is_lua_ident_byte(bytes[i]) {
                    i += 1;
                }
                let word = &expr[start..i];
                let token_type = if is_lua_keyword(word) {
                    TOKEN_KEYWORD
                } else if next_non_ws_byte(bytes, i) == Some(b'(') {
                    TOKEN_FUNCTION
                } else {
                    TOKEN_VARIABLE
                };
                builder.push(base_offset + start, i - start, token_type, 0);
            }
            b if is_lua_operator_byte(b) => {
                builder.push(base_offset + i, 1, TOKEN_OPERATOR, 0);
                i += 1;
            }
            _ => i += 1,
        }
    }
}

fn find_matching_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut quote: Option<u8> = None;

    while i < bytes.len() {
        if let Some(q) = quote {
            if bytes[i] == b'\\' {
                i = (i + 2).min(bytes.len());
                continue;
            }
            if bytes[i] == q {
                quote = None;
            }
            i += 1;
            continue;
        }

        match bytes[i] {
            b'\'' | b'"' => {
                quote = Some(bytes[i]);
                i += 1;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }

    None
}

fn next_non_ws_byte(bytes: &[u8], mut i: usize) -> Option<u8> {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    bytes.get(i).copied()
}

fn is_lua_keyword(word: &str) -> bool {
    matches!(
        word,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

fn is_lua_operator_byte(b: u8) -> bool {
    matches!(
        b,
        b'+' | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'^'
            | b'#'
            | b'='
            | b'<'
            | b'>'
            | b'~'
            | b'('
            | b')'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'.'
            | b','
            | b':'
    )
}

fn is_tag_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':')
}

fn is_attr_name_start_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':'
}

fn is_attr_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.')
}

fn is_lua_ident_start_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_lua_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[derive(Debug, Clone, Copy)]
struct AbsoluteToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    token_modifiers_bitset: u32,
}

struct TokenBuilder<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
    tokens: Vec<AbsoluteToken>,
}

impl<'a> TokenBuilder<'a> {
    fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (i, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            source,
            line_starts,
            tokens: Vec::new(),
        }
    }

    fn push(&mut self, offset: usize, length: usize, token_type: u32, modifiers: u32) {
        if length == 0 || offset >= self.source.len() {
            return;
        }
        let Some((line, start)) = self.line_and_character(offset) else {
            return;
        };
        self.tokens.push(AbsoluteToken {
            line,
            start,
            length: length as u32,
            token_type,
            token_modifiers_bitset: modifiers,
        });
    }

    fn finish(mut self) -> Vec<SemanticToken> {
        self.tokens.sort_by_key(|token| (token.line, token.start));
        self.tokens
            .into_iter()
            .scan((0, 0), |last, token| {
                let delta_line = token.line - last.0;
                let delta_start = if delta_line == 0 {
                    token.start - last.1
                } else {
                    token.start
                };
                *last = (token.line, token.start);
                Some(SemanticToken {
                    delta_line,
                    delta_start,
                    length: token.length,
                    token_type: token.token_type,
                    token_modifiers_bitset: token.token_modifiers_bitset,
                })
            })
            .collect()
    }

    fn line_and_character(&self, offset: usize) -> Option<(u32, u32)> {
        let line = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.line_starts.get(line).copied()?;
        Some((line as u32, (offset - line_start) as u32))
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::SemanticTokensResult;

    use super::*;

    #[test]
    fn template_semantic_tokens_distinguish_elements_and_lua_insertions() {
        let source = r#"<template>
  <row title={if count > 0 then "ready" else "empty"}>
    <StatusPill value={format_count(count)} />
  </row>
</template>

<script lang="luau">
import StatusPill from "./status-pill.mesh"
</script>
"#;
        let doc = Document::new(
            tower_lsp::lsp_types::Url::parse("file:///test.mesh").unwrap(),
            source.to_string(),
        );

        let SemanticTokensResult::Tokens(tokens) = full(&doc) else {
            panic!("expected full semantic tokens");
        };
        let absolute = absolute_tokens(&tokens.data);

        assert_token(source, &absolute, "row", TOKEN_MESH_BUILTIN_ELEMENT, 1);
        assert_token(
            source,
            &absolute,
            "StatusPill",
            TOKEN_MESH_COMPONENT_ELEMENT,
            0,
        );
        assert_token(source, &absolute, "if", TOKEN_KEYWORD, 0);
        assert_token(source, &absolute, "count", TOKEN_VARIABLE, 0);
        assert_token(source, &absolute, "format_count", TOKEN_FUNCTION, 0);
        assert_token(source, &absolute, "\"ready\"", TOKEN_STRING, 0);
    }

    fn assert_token(
        source: &str,
        tokens: &[AbsoluteToken],
        text: &str,
        token_type: u32,
        token_modifiers_bitset: u32,
    ) {
        let offset = source.find(text).expect("test fixture contains text");
        let (line, start) = line_and_start(source, offset);
        assert!(
            tokens.iter().any(|token| {
                token.line == line
                    && token.start == start
                    && token.length == text.len() as u32
                    && token.token_type == token_type
                    && token.token_modifiers_bitset == token_modifiers_bitset
            }),
            "missing token {text:?} at {line}:{start} with type {token_type}"
        );
    }

    fn absolute_tokens(tokens: &[SemanticToken]) -> Vec<AbsoluteToken> {
        let mut line = 0;
        let mut start = 0;
        tokens
            .iter()
            .map(|token| {
                line += token.delta_line;
                start = if token.delta_line == 0 {
                    start + token.delta_start
                } else {
                    token.delta_start
                };
                AbsoluteToken {
                    line,
                    start,
                    length: token.length,
                    token_type: token.token_type,
                    token_modifiers_bitset: token.token_modifiers_bitset,
                }
            })
            .collect()
    }

    fn line_and_start(source: &str, offset: usize) -> (u32, u32) {
        let mut line = 0;
        let mut line_start = 0;
        for (i, ch) in source.char_indices() {
            if i == offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                line_start = i + 1;
            }
        }
        (line, (offset - line_start) as u32)
    }
}
