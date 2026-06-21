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
    if doc.interface_proxies.is_empty() && doc.element_ref_aliases.is_empty() {
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
    // Check if token looks like `<var>.<prefix>` or `<var>:<prefix>`.
    if let Some(member_pos) = token.find(['.', ':']) {
        let var_name = &token[..member_pos];
        let prefix = &token[member_pos + 1..];
        if doc
            .element_ref_aliases
            .iter()
            .any(|alias| alias.alias == var_name)
        {
            return ScriptContext::ElementRefAliasMember {
                alias: var_name.to_string(),
                prefix: prefix.to_string(),
            };
        }
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
            style::complete(ctx, content)
        }
        Block::Script => {
            let ctx = crate::util::script_context_at(content, loc.offset_in_block);
            let ctx = try_upgrade_to_proxy_ctx(ctx, content, loc.offset_in_block, doc);
            script::complete(ctx, doc, registry)
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::{CompletionItemKind, Url};

    use super::*;

    #[test]
    fn script_completion_on_element_ref_alias_offers_element_members_not_scoped_functions() {
        let (source, position) = fixture_with_cursor(
            r#"<template>
  <input ref="field" />
</template>

<script lang="luau">
function scoped_handler()
end

local node = refs.field
node.$0
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"value".to_string()));
        assert!(labels.contains(&"focus".to_string()));
        assert!(labels.contains(&"set_value".to_string()));
        assert!(!labels.contains(&"scoped_handler".to_string()));
    }

    #[test]
    fn script_completion_on_direct_element_ref_colon_offers_methods() {
        let (source, position) = fixture_with_cursor(
            r#"<template>
  <button ref="action" />
</template>

<script lang="luau">
refs.action:$0
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let items = complete(&doc, position, &ModuleRegistry::empty());

        assert!(items.iter().any(|item| {
            item.label == "click" && item.kind == Some(CompletionItemKind::METHOD)
        }));
        assert!(
            items.iter().any(|item| {
                item.label == "width" && item.kind == Some(CompletionItemKind::FIELD)
            })
        );
    }

    #[test]
    fn script_completion_on_current_target_alias_offers_element_members_only() {
        let (source, position) = fixture_with_cursor(
            r#"<template>
  <button onclick="activate" />
</template>

<script lang="luau">
function activate(event)
  local node = event.current_target
  node.$0
end

function unrelated()
end
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"width".to_string()));
        assert!(labels.contains(&"focus".to_string()));
        assert!(!labels.contains(&"unrelated".to_string()));
    }

    #[test]
    fn script_completion_on_bind_this_element_offers_attributes_and_methods() {
        let (source, position) = fixture_with_cursor(
            r#"<template>
  <popover bind:this={popover} aria-label="Audio controls" />
</template>

<script lang="luau">
popover.$0
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"ariaLabel".to_string()));
        assert!(labels.contains(&"focus".to_string()));
        assert!(labels.contains(&"width".to_string()));
    }

    #[test]
    fn template_completion_inside_attribute_value_brace_offers_script_members() {
        let (source, position) = fixture_with_cursor(
            r#"<template>
  <slider value={$0} />
</template>

<script lang="luau">
slider_value = 0

function onVolumeChange(event)
end
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"slider_value".to_string()));
        assert!(labels.contains(&"onVolumeChange".to_string()));
    }

    fn completion_labels(doc: &Document, position: Position) -> Vec<String> {
        complete(doc, position, &ModuleRegistry::empty())
            .into_iter()
            .map(|item| item.label)
            .collect()
    }

    fn fixture_with_cursor(source: &str) -> (String, Position) {
        let marker = "$0";
        let offset = source.find(marker).expect("fixture has cursor marker");
        let source = source.replacen(marker, "", 1);
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
        (source, Position::new(line, (offset - line_start) as u32))
    }
}
