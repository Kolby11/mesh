use tower_lsp::lsp_types::{CompletionItem, Position};

use crate::{
    document::Document,
    module_registry::ModuleRegistry,
    util::{Block, ScriptContext, block_at_offset, block_content, position_to_offset},
};

pub(crate) mod script;
mod style;
mod template;

/// If the cursor is after `<proxy_var>.<prefix>` and `proxy_var` is a known interface
/// proxy, upgrade the context to `InterfaceProxy`. Otherwise returns `ctx` unchanged.
fn try_upgrade_to_proxy_ctx(
    ctx: ScriptContext,
    block_content: &str,
    offset: usize,
    doc: &Document,
) -> ScriptContext {
    if doc.interface_proxies.is_empty() {
        return ctx;
    }
    let before = &block_content[..offset.min(block_content.len())];
    // Find the last dot-separated token: everything from the last whitespace/delimiter to cursor
    let token_start = before
        .rfind(|c: char| {
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
                        | '='
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                )
        })
        .map(|i| i + 1)
        .unwrap_or(0);
    let token = &before[token_start..];
    // Check if token looks like `<var>.<prefix>`
    if let Some(dot_pos) = token.find('.') {
        let var_name = &token[..dot_pos];
        let prefix = &token[dot_pos + 1..];
        // Only upgrade if we actually know this var is a proxy
        if doc.interface_proxies.contains_key(var_name) {
            return ScriptContext::InterfaceProxy {
                var_name: var_name.to_string(),
                prefix: prefix.to_string(),
            };
        }
    }
    ctx
}

pub fn complete(
    doc: &Document,
    position: Position,
    registry: &ModuleRegistry,
) -> Vec<CompletionItem> {
    let offset = position_to_offset(&doc.source, position);
    let loc = block_at_offset(&doc.source, offset);
    let content = block_content(&doc.source, &loc.block);

    match &loc.block {
        Block::Template => {
            let ctx = crate::util::template_context_at(content, loc.offset_in_block);
            template::complete(ctx, doc, registry)
        }
        Block::Style => {
            let ctx = crate::util::style_context_at(content, loc.offset_in_block);
            style::complete(ctx)
        }
        Block::Script => {
            let ctx = crate::util::script_context_at(content, loc.offset_in_block);
            let ctx = try_upgrade_to_proxy_ctx(ctx, content, loc.offset_in_block, doc);
            script::complete(ctx, doc, registry)
        }
        _ => vec![],
    }
}
