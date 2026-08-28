use full_moon::{
    LuaVersion,
    tokenizer::{Lexer, LexerResult, Symbol, TokenType},
};
use tower_lsp::lsp_types::Position;

use crate::document::block_content_range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Template,
    Script,
    Style,
    I18n,
    TopLevel,
}

#[derive(Debug, Clone)]
pub struct BlockLocation {
    pub block: Block,
    /// Byte offset of the cursor within the block's content.
    pub offset_in_block: usize,
}

#[derive(Debug, Clone)]
pub enum TemplateContext {
    TagName { partial: String },
    AttrName { tag: String },
    AttrValue { tag: String, attr: String },
    Expr,
    Content,
}

#[derive(Debug, Clone)]
pub enum StyleContext {
    Property,
    Value { property: String },
    Variable { prefix: String },
    Prop { prefix: String },
    Selector,
}

#[derive(Debug, Clone)]
pub enum ScriptContext {
    /// Cursor is after `mesh.` — prefix is what was typed after the dot.
    MeshApi {
        prefix: String,
    },
    /// Cursor is after `refs.` or a partial `refs.<name>`.
    Refs {
        prefix: String,
    },
    /// Cursor is after `refs.<name>.`.
    RefMember {
        ref_name: String,
        prefix: String,
    },
    /// Cursor is after a Lua variable known to hold an element ref, e.g. `node.`
    /// after `local node = refs.panel`.
    ElementRefAliasMember {
        alias: String,
        prefix: String,
    },
    /// Cursor is after `event.current_target.`.
    EventCurrentTarget {
        prefix: String,
    },
    /// Cursor is after `mesh.service.bind("` / `mesh.service.on("`.
    ServiceName,
    /// Cursor is inside the first string argument of `require(...)` / `import(...)`.
    /// Completes module specifiers (host APIs, interfaces, components).
    ImportSpecifier {
        prefix: String,
    },
    /// Cursor is inside a name argument of `import("<specifier>", "<name>...`.
    /// Completes the named members exported by `specifier`.
    ImportMember {
        specifier: String,
        prefix: String,
    },
    /// Cursor is after `<proxy_var>.` where `proxy_var` is bound to an interface via `require`.
    InterfaceProxy {
        /// The Lua variable name that holds the proxy (e.g. "audio").
        var_name: String,
        /// Characters typed after the dot so far (may be empty).
        prefix: String,
    },
    /// Cursor is after `<var>.` where `var` is a `bind:this={var}` component
    /// instance. Completes the child's base element fields plus its exported
    /// (public) variables and functions.
    ComponentInstanceMember {
        /// The Lua variable name bound to the mounted component instance.
        var_name: String,
        /// Characters typed after the dot so far (may be empty).
        prefix: String,
    },
    /// Cursor is after `props.` in a component script.
    Props {
        prefix: String,
    },
    General,
}

/// Convert an LSP Position (0-based line + UTF-16 character units) to a byte
/// offset in the UTF-8 source.
pub fn position_to_offset(source: &str, pos: Position) -> usize {
    let mut current_line = 0u32;
    let mut line_byte_start = 0;

    for (i, ch) in source.char_indices() {
        if current_line == pos.line {
            let mut utf16 = 0u32;
            for (offset, line_char) in source[line_byte_start..].char_indices() {
                if line_char == '\n' {
                    return line_byte_start + offset;
                }
                let width = line_char.len_utf16() as u32;
                if utf16 + width > pos.character {
                    return line_byte_start + offset;
                }
                utf16 += width;
                if utf16 == pos.character {
                    return line_byte_start + offset + line_char.len_utf8();
                }
            }
            return source.len();
        }
        if ch == '\n' {
            current_line += 1;
            line_byte_start = i + 1;
        }
    }

    if current_line == pos.line {
        return source.len();
    }

    source.len()
}

/// Convert a UTF-8 byte offset into an LSP [`Position`] (0-based line and
/// UTF-16 code-unit column).
pub fn offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut character = 0u32;

    for (index, ch) in source.char_indices() {
        let end = index + ch.len_utf8();
        if end > offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    Position::new(line, character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_positions_to_utf8_offsets() {
        let source = "é😀x\nnext";

        assert_eq!(position_to_offset(source, Position::new(0, 0)), 0);
        assert_eq!(position_to_offset(source, Position::new(0, 1)), 2);
        assert_eq!(position_to_offset(source, Position::new(0, 3)), 6);
        assert_eq!(position_to_offset(source, Position::new(0, 4)), 7);
        assert_eq!(position_to_offset(source, Position::new(1, 0)), 8);
    }

    #[test]
    fn converts_utf8_offsets_to_utf16_positions() {
        let source = "é😀x\nnext";

        assert_eq!(offset_to_position(source, 0), Position::new(0, 0));
        assert_eq!(offset_to_position(source, 2), Position::new(0, 1));
        assert_eq!(offset_to_position(source, 6), Position::new(0, 3));
        assert_eq!(offset_to_position(source, 7), Position::new(0, 4));
        assert_eq!(offset_to_position(source, 8), Position::new(1, 0));
    }

    #[test]
    fn rounds_offsets_inside_a_utf8_codepoint_back_to_its_start() {
        assert_eq!(offset_to_position("é", 1), Position::new(0, 0));
    }

    #[test]
    fn luau_tokens_retain_an_incomplete_member_path() {
        let tokens = significant_lua_tokens("mesh.locale.");
        assert_eq!(
            trailing_lua_path(&tokens, "mesh.locale.".len()).as_deref(),
            Some("mesh.locale.")
        );
        assert!(matches!(
            script_context_at("mesh.locale.", "mesh.locale.".len()),
            ScriptContext::MeshApi { prefix } if prefix == "locale."
        ));
    }

    #[test]
    fn script_context_handles_a_script_block_prefix() {
        assert!(matches!(
            script_context_at("\nmesh.locale.", "\nmesh.locale.".len()),
            ScriptContext::MeshApi { prefix } if prefix == "locale."
        ));
    }

    #[test]
    fn luau_context_ignores_comments_and_string_contents() {
        let source = "-- refs.fake.\nlocal text = \"mesh.fake.\"\nrefs.real.";
        assert!(matches!(
            script_context_at(source, source.len()),
            ScriptContext::RefMember { ref_name, prefix }
                if ref_name == "real" && prefix.is_empty()
        ));

        let source = "local text = \"refs.fake.\"";
        assert!(matches!(
            script_context_at(source, source.len()),
            ScriptContext::General
        ));
    }
}

/// Determine which top-level block the cursor byte offset falls in.
pub fn block_at_offset(source: &str, offset: usize) -> BlockLocation {
    const BLOCKS: &[(&str, Block)] = &[
        ("template", Block::Template),
        ("script", Block::Script),
        ("style", Block::Style),
        ("i18n", Block::I18n),
    ];

    for (name, kind) in BLOCKS {
        if let Some((start, end)) = block_content_range(source, name) {
            if offset >= start && offset <= end {
                return BlockLocation {
                    block: kind.clone(),
                    offset_in_block: offset - start,
                };
            }
        }
    }

    BlockLocation {
        block: Block::TopLevel,
        offset_in_block: offset,
    }
}

/// Extract the content of a named block for context analysis.
pub fn block_content<'a>(source: &'a str, block: &Block) -> &'a str {
    let name = match block {
        Block::Template => "template",
        Block::Script => "script",
        Block::Style => "style",
        Block::I18n => "i18n",
        Block::TopLevel => return source,
    };
    crate::document::extract_block_text(source, name)
}

/// Classify the cursor position within a template block.
pub fn template_context_at(block_content: &str, offset: usize) -> TemplateContext {
    let before = &block_content[..offset.min(block_content.len())];

    // Inside { expr }?
    let last_open_brace = before.rfind('{');
    let last_close_brace = before.rfind('}');
    let last_lt = before.rfind('<');
    let last_gt = before.rfind('>');

    if let Some(ob) = last_open_brace {
        let after_brace_closed = last_close_brace.is_some_and(|cb| cb > ob);
        // An unclosed `{` is an expression context whether it appears in template
        // content (`{state}`) or inside a tag as an attribute value
        // (`value={state}`, `bind:this={ref}`). The brace must be the innermost
        // open construct: it has to come after the most recent `<`.
        let brace_after_tag_start = last_lt.is_none_or(|lt| ob > lt);
        if !after_brace_closed && brace_after_tag_start {
            return TemplateContext::Expr;
        }
    }

    // Inside an open tag?
    if let Some(lt) = last_lt {
        if last_gt.is_none_or(|gt| gt < lt) {
            let inside = &before[lt + 1..];
            // Closing tag?
            if inside.trim_start().starts_with('/') {
                return TemplateContext::Content;
            }
            // Split tag name and rest
            let after_lt = inside.trim_start_matches('!');
            let ws_pos = after_lt.find(|c: char| c.is_ascii_whitespace());
            let (tag_name, after_tag) = if let Some(p) = ws_pos {
                (&after_lt[..p], &after_lt[p..])
            } else {
                (after_lt, "")
            };

            if after_tag.is_empty() {
                return TemplateContext::TagName {
                    partial: tag_name.to_string(),
                };
            }

            // Find the most recently started attribute name in after_tag
            // Scan for the last attribute-like token
            let trimmed = after_tag.trim_start();
            let last_eq = trimmed.rfind('=');
            let last_ws_or_start = trimmed
                .char_indices()
                .rev()
                .find(|(_, c)| c.is_ascii_whitespace())
                .map(|(i, _)| i + 1)
                .unwrap_or(0);
            let _attr_partial = trimmed[last_ws_or_start..]
                .split('=')
                .next()
                .unwrap_or("")
                .trim();

            if let Some(eq_pos) = last_eq {
                let after_eq = trimmed[eq_pos + 1..].trim_start_matches(['"', '\'']);
                if !after_eq.ends_with('"') && !after_eq.ends_with('\'') {
                    let attr_name = trimmed[..eq_pos]
                        .rsplit(|c: char| c.is_ascii_whitespace())
                        .next()
                        .unwrap_or("")
                        .to_string();
                    return TemplateContext::AttrValue {
                        tag: tag_name.to_string(),
                        attr: attr_name,
                    };
                }
            }

            return TemplateContext::AttrName {
                tag: tag_name.to_string(),
            };
        }
    }

    TemplateContext::Content
}

/// Classify the cursor position within a style block.
pub fn style_context_at(block_content: &str, offset: usize) -> StyleContext {
    let before = &block_content[..offset.min(block_content.len())];

    let last_open = before.rfind('{');
    let last_close = before.rfind('}');

    let Some(open) = last_open else {
        return StyleContext::Selector;
    };

    if last_close.is_some_and(|close| close > open) {
        return StyleContext::Selector;
    }

    // Inside a declaration block
    let inside = &before[open + 1..];

    // Find the last complete declaration (ends with ';')
    let last_semi = inside.rfind(';');
    let after_last_semi = last_semi.map(|s| &inside[s + 1..]).unwrap_or(inside);
    let current_decl = after_last_semi.trim_start();

    if let Some(colon_pos) = current_decl.rfind(':') {
        let property = current_decl[..colon_pos].trim().to_string();
        let value_before_cursor = &current_decl[colon_pos + 1..];
        if let Some(var_start) = value_before_cursor.rfind("var(") {
            let after_var = &value_before_cursor[var_start + "var(".len()..];
            if !after_var.contains(')') {
                return StyleContext::Variable {
                    prefix: after_var.trim().to_string(),
                };
            }
        }
        if let Some(prop_start) = value_before_cursor.rfind("prop(") {
            let after_prop = &value_before_cursor[prop_start + "prop(".len()..];
            if !after_prop.contains(')') {
                return StyleContext::Prop {
                    prefix: after_prop.trim().to_string(),
                };
            }
        }
        StyleContext::Value { property }
    } else {
        StyleContext::Property
    }
}

/// Classify the cursor position within a script block.
pub fn script_context_at(block_content: &str, offset: usize) -> ScriptContext {
    let before = &block_content[..offset.min(block_content.len())];
    let tokens = significant_lua_tokens(before);

    // Cursor inside a `require(...)` / `import(...)` string argument: the first
    // argument completes module specifiers; later `import` arguments complete the
    // named members of the already-typed specifier.
    if let Some(cursor) = import_cursor(&tokens, before.len()) {
        if cursor.arg_string_index == 0 {
            return ScriptContext::ImportSpecifier {
                prefix: cursor.prefix,
            };
        }
        if cursor.callee == "import" {
            if let Some(specifier) = cursor.first_arg {
                return ScriptContext::ImportMember {
                    specifier,
                    prefix: cursor.prefix,
                };
            }
        }
        // `require` takes a single argument; nothing to complete past it.
        return ScriptContext::General;
    }

    // Service names are string arguments, so identify the call from parsed
    // tokens rather than looking for a textual `mesh.service.*("` suffix.
    if let Some(cursor) = service_cursor(&tokens, before.len()) {
        if cursor.arg_string_index == 0 {
            return ScriptContext::ServiceName;
        }
    }

    // Check for member contexts from the Luau token path.
    if let Some(path) = trailing_lua_path(&tokens, before.len()) {
        if let Some(prefix) = path.strip_prefix("event.current_target.") {
            return ScriptContext::EventCurrentTarget {
                prefix: prefix.to_string(),
            };
        }
        if let Some(rest) = path.strip_prefix("refs.") {
            if let Some((separator, _)) = rest
                .char_indices()
                .find(|(_, character)| matches!(character, '.' | ':'))
            {
                let (ref_name, prefix) = rest.split_at(separator);
                let prefix = prefix.trim_start_matches(['.', ':']);
                return ScriptContext::RefMember {
                    ref_name: ref_name.to_string(),
                    prefix: prefix.to_string(),
                };
            }
            return ScriptContext::Refs {
                prefix: rest.to_string(),
            };
        }
        if let Some(prefix) = path.strip_prefix("props.") {
            return ScriptContext::Props {
                prefix: prefix.to_string(),
            };
        }
        if let Some(prefix) = path.strip_prefix("mesh.") {
            return ScriptContext::MeshApi {
                prefix: prefix.to_string(),
            };
        }
    }

    ScriptContext::General
}

#[derive(Debug, Clone)]
struct LuaToken {
    kind: TokenType,
    start: usize,
    end: usize,
    recovered: bool,
}

fn significant_lua_tokens(source: &str) -> Vec<LuaToken> {
    let mut lexer = Lexer::new(source, LuaVersion::new());
    let mut tokens = Vec::new();

    while let Some(result) = lexer.consume() {
        let (reference, recovered) = match result {
            LexerResult::Ok(reference) => (reference, false),
            LexerResult::Recovered(reference, _) => (reference, true),
            LexerResult::Fatal(_) => break,
        };
        let token = reference.token();
        if matches!(token.token_type(), TokenType::Eof) {
            break;
        }
        if token.token_type().is_trivia() {
            continue;
        }
        tokens.push(LuaToken {
            kind: token.token_type().clone(),
            start: token.start_position().bytes(),
            end: token.end_position().bytes(),
            recovered,
        });
    }

    tokens
}

fn is_symbol(token: &LuaToken, symbol: Symbol) -> bool {
    matches!(
        &token.kind,
        TokenType::Symbol { symbol: actual } if *actual == symbol
    )
}

fn identifier(token: &LuaToken) -> Option<&str> {
    match &token.kind {
        TokenType::Identifier { identifier } => Some(identifier.as_str()),
        _ => None,
    }
}

fn string_literal(token: &LuaToken) -> Option<&str> {
    match &token.kind {
        TokenType::StringLiteral { literal, .. } => Some(literal.as_str()),
        _ => None,
    }
}

/// Return the contiguous member path ending at `end`, where `end` is an
/// exclusive token index. Every part is supplied by the Luau lexer, so the
/// result cannot be sourced from a comment or another string literal.
fn lua_path_before(tokens: &[LuaToken], end: usize) -> Option<String> {
    let mut start = end;
    while start > 0 {
        let token = &tokens[start - 1];
        if identifier(token).is_some()
            || is_symbol(token, Symbol::Dot)
            || is_symbol(token, Symbol::Colon)
        {
            start -= 1;
        } else {
            break;
        }
    }
    if start == end || identifier(&tokens[start]).is_none() {
        return None;
    }

    let mut path = String::new();
    for token in &tokens[start..end] {
        if let Some(name) = identifier(token) {
            path.push_str(name);
        } else if is_symbol(token, Symbol::Dot) {
            path.push('.');
        } else if is_symbol(token, Symbol::Colon) {
            path.push(':');
        } else {
            return None;
        }
    }
    Some(path)
}

fn trailing_lua_path(tokens: &[LuaToken], source_len: usize) -> Option<String> {
    if tokens.last()?.end != source_len {
        return None;
    }
    let path = lua_path_before(tokens, tokens.len())?;
    path.contains('.').then_some(path)
}

struct CallCursor {
    callee: String,
    arg_string_index: usize,
    prefix: String,
    first_arg: Option<String>,
}

fn open_call_cursor(tokens: &[LuaToken], source_len: usize) -> Option<CallCursor> {
    let current = tokens.last()?;
    if current.start >= current.end || current.end != source_len {
        return None;
    }
    let prefix = string_literal(current)?.to_string();
    if !current.recovered {
        return None;
    }

    for open in (0..tokens.len().saturating_sub(1)).rev() {
        if !is_symbol(&tokens[open], Symbol::LeftParen) {
            continue;
        }
        let Some(callee) = lua_path_before(tokens, open) else {
            continue;
        };
        let mut depth = 1usize;
        let mut arg_string_index = 0usize;
        let mut first_arg = None;
        let mut current_is_top_level = false;

        for (index, token) in tokens.iter().enumerate().skip(open + 1) {
            if is_symbol(token, Symbol::LeftParen) {
                depth += 1;
                continue;
            }
            if is_symbol(token, Symbol::RightParen) {
                if depth == 1 {
                    break;
                }
                depth -= 1;
                continue;
            }
            if depth != 1 {
                continue;
            }
            if is_symbol(token, Symbol::Comma) {
                arg_string_index += 1;
                continue;
            }
            if let Some(value) = string_literal(token) {
                if index == tokens.len() - 1 {
                    current_is_top_level = true;
                } else if arg_string_index == 0 && first_arg.is_none() {
                    first_arg = Some(value.to_string());
                }
            }
        }

        if current_is_top_level {
            return Some(CallCursor {
                callee,
                arg_string_index,
                prefix,
                first_arg,
            });
        }
    }

    None
}

fn import_cursor(tokens: &[LuaToken], source_len: usize) -> Option<CallCursor> {
    let cursor = open_call_cursor(tokens, source_len)?;
    matches!(cursor.callee.as_str(), "require" | "import").then_some(cursor)
}

fn service_cursor(tokens: &[LuaToken], source_len: usize) -> Option<CallCursor> {
    let cursor = open_call_cursor(tokens, source_len)?;
    matches!(
        cursor.callee.as_str(),
        "mesh.service.bind" | "mesh.service.on"
    )
    .then_some(cursor)
}
