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
    /// Cursor is after `require("mesh.` or `mesh.service.bind("`.
    ServiceName,
    /// Cursor is after `<proxy_var>.` where `proxy_var` is bound to an interface via `require`.
    InterfaceProxy {
        /// The Lua variable name that holds the proxy (e.g. "audio").
        var_name: String,
        /// Characters typed after the dot so far (may be empty).
        prefix: String,
    },
    General,
}

/// Convert an LSP Position (0-based line + UTF-16 char) to a byte offset.
/// Treats character as a byte offset within the line for ASCII-heavy content;
/// full UTF-16 conversion is not required for .mesh files.
pub fn position_to_offset(source: &str, pos: Position) -> usize {
    let mut current_line = 0u32;
    let mut line_byte_start = 0;

    for (i, ch) in source.char_indices() {
        if current_line == pos.line {
            // Approximate: character == byte offset within line
            return line_byte_start + pos.character as usize;
        }
        if ch == '\n' {
            current_line += 1;
            line_byte_start = i + 1;
        }
    }

    if current_line == pos.line {
        return (line_byte_start + pos.character as usize).min(source.len());
    }

    source.len()
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
        StyleContext::Value { property }
    } else {
        StyleContext::Property
    }
}

/// Classify the cursor position within a script block.
pub fn script_context_at(block_content: &str, offset: usize) -> ScriptContext {
    let before = &block_content[..offset.min(block_content.len())];

    if let Some(token) = current_lua_path_token(before) {
        if let Some(prefix) = token.strip_prefix("event.current_target.") {
            return ScriptContext::EventCurrentTarget {
                prefix: prefix.to_string(),
            };
        }

        if let Some(rest) = token.strip_prefix("refs.") {
            if let Some((ref_name, prefix)) = rest.split_once(['.', ':']) {
                return ScriptContext::RefMember {
                    ref_name: ref_name.to_string(),
                    prefix: prefix.to_string(),
                };
            }
            return ScriptContext::Refs {
                prefix: rest.to_string(),
            };
        }
    }

    // Check for service name context: require("mesh." or mesh.service.bind(" or mesh.service.on("
    for pattern in &[
        "require(\"mesh.",
        "require('mesh.",
        "mesh.service.bind(\"",
        "mesh.service.bind('",
        "mesh.service.on(\"",
        "mesh.service.on('",
    ] {
        if before.ends_with(pattern)
            || (before.contains(pattern)
                && !before[before.rfind(pattern).unwrap()..].contains(['\n', ')', ';']))
        {
            return ScriptContext::ServiceName;
        }
    }

    // Check for mesh. API context
    if let Some(mesh_pos) = before.rfind("mesh.") {
        let after_mesh = &before[mesh_pos + 5..];
        // Valid if no statement-terminating characters between mesh. and cursor
        let is_continuation = !after_mesh.contains(['\n', ';', ')', '(']) || {
            // Allow nested like mesh.state. if the parens belong to an outer call
            after_mesh.chars().filter(|&c| c == '(').count()
                == after_mesh.chars().filter(|&c| c == ')').count()
        };
        if is_continuation {
            return ScriptContext::MeshApi {
                prefix: after_mesh.to_string(),
            };
        }
    }

    ScriptContext::General
}

fn current_lua_path_token(before: &str) -> Option<&str> {
    let token = before
        .rsplit(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '(' | ')'
                        | ','
                        | ';'
                        | '{'
                        | '}'
                        | '['
                        | ']'
                        | '"'
                        | '\''
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '='
                )
        })
        .next()
        .unwrap_or("");

    if token.is_empty() { None } else { Some(token) }
}
