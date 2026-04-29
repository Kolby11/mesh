use tower_lsp::lsp_types::{CompletionItem, Position};

use crate::{
    document::Document,
    plugin_registry::PluginRegistry,
    util::{Block, block_at_offset, block_content, position_to_offset},
};

pub(crate) mod script;
mod style;
mod template;

pub fn complete(
    doc: &Document,
    position: Position,
    registry: &PluginRegistry,
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
            script::complete(ctx, doc, registry)
        }
        _ => vec![],
    }
}
