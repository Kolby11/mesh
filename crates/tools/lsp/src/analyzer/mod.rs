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
    if doc.interface_proxies.is_empty()
        && doc.element_ref_aliases.is_empty()
        && doc.component_instances.is_empty()
        && doc
            .parsed
            .as_ref()
            .and_then(|parsed| parsed.props.as_ref())
            .is_none()
    {
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
        if doc
            .component_instances
            .iter()
            .any(|instance| instance.var_name == var_name)
        {
            return ScriptContext::ComponentInstanceMember {
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
            style::complete(ctx, doc, content)
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
    fn script_completion_on_bind_this_component_offers_base_fields_and_exported_members() {
        // A mounted child component bound via `bind:this` should complete the
        // MeshElement base fields plus the child's exported (public) variables
        // and functions — but not its private locals or lifecycle hooks.
        let dir = std::env::temp_dir().join(format!(
            "mesh-lsp-bind-this-component-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("item-row.mesh"),
            r#"<template>
  <row />
</template>

<script lang="luau">
selected_id = nil
label_text = ""

local private_state = 1

function setSelected(id)
end

local function privateHelper()
end

function render(self)
end
</script>
"#,
        )
        .unwrap();

        let (source, position) = fixture_with_cursor(
            r#"<template>
  <ItemRow bind:this={item_row} />
</template>

<script lang="luau">
local ItemRow = require("./item-row.mesh")

item_row.$0
</script>
"#,
        );
        let uri = Url::from_file_path(dir.join("main.mesh")).unwrap();
        let doc = Document::new(uri, source);
        let labels = completion_labels(&doc, position);

        std::fs::remove_dir_all(&dir).ok();

        // Tier 1: inherited MeshElement base fields.
        assert!(labels.contains(&"width".to_string()));
        // Tier 2: exported variables and functions.
        assert!(labels.contains(&"selected_id".to_string()));
        assert!(labels.contains(&"label_text".to_string()));
        assert!(labels.contains(&"setSelected".to_string()));
        // Private locals and lifecycle hooks must not cross the boundary.
        assert!(!labels.contains(&"private_state".to_string()));
        assert!(!labels.contains(&"privateHelper".to_string()));
        assert!(!labels.contains(&"render".to_string()));
    }

    #[test]
    fn script_completion_on_props_offers_declared_props_and_helpers() {
        let (source, position) = fixture_with_cursor(
            r#"<props>
  track_width: { type: "size", default: "20px" }
  anim_ms: { type: "duration", default: 120 }
</props>
<script lang="luau">
props.$0
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"track_width".to_string()));
        assert!(labels.contains(&"anim_ms".to_string()));
        assert!(labels.contains(&"source".to_string()));
        assert!(labels.contains(&"at".to_string()));
    }

    #[test]
    fn style_completion_inside_prop_call_offers_declared_props() {
        let (source, position) = fixture_with_cursor(
            r#"<props>
  track_width: { type: "size", default: "20px" }
</props>
<style>
.slider { width: prop(track_$0); }
</style>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert_eq!(labels, vec!["track_width".to_string()]);
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

    #[test]
    fn import_specifier_completion_offers_builtin_modules() {
        let (source, position) = fixture_with_cursor(
            r#"<template></template>

<script lang="luau">
local i18n = require("$0")
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"mesh.i18n".to_string()));
        assert!(labels.contains(&"mesh.ui".to_string()));
        assert!(labels.contains(&"mesh.log".to_string()));
    }

    #[test]
    fn import_specifier_completion_works_for_import_too() {
        let (source, position) = fixture_with_cursor(
            r#"<template></template>

<script lang="luau">
local t = import("mesh.i$0")
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let items = complete(&doc, position, &ModuleRegistry::empty());

        // Prefix "mesh.i" should match mesh.i18n and insert only the suffix.
        let item = items
            .iter()
            .find(|item| item.label == "mesh.i18n")
            .expect("mesh.i18n suggested");
        assert_eq!(item.insert_text.as_deref(), Some("18n"));
    }

    #[test]
    fn import_member_completion_offers_i18n_t() {
        let (source, position) = fixture_with_cursor(
            r#"<template></template>

<script lang="luau">
local t = import("mesh.i18n", "$0")
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"t".to_string()));
    }

    #[test]
    fn import_member_completion_offers_host_api_methods() {
        let (source, position) = fixture_with_cursor(
            r#"<template></template>

<script lang="luau">
local redraw = import("mesh.ui", "$0")
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"request_redraw".to_string()));
    }

    #[test]
    fn import_default_binding_enables_member_completion() {
        // `local audio = import("mesh.audio")` (no extra names) should bind a
        // proxy var just like `require`, so `audio.` completes its members.
        let (source, position) = fixture_with_cursor(
            r#"<template></template>

<script lang="luau">
local audio = import("mesh.audio")
audio.$0
</script>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        // on_change is always offered for a bound interface proxy.
        let labels = completion_labels(&doc, position);
        assert!(labels.contains(&"on_change".to_string()));
    }

    #[test]
    fn template_attr_completion_offers_bind_this_with_brace_value() {
        let (source, position) = fixture_with_cursor(
            r#"<template>
  <box $0 />
</template>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let items = complete(&doc, position, &ModuleRegistry::empty());

        let bind_this = items
            .iter()
            .find(|item| item.label == "bind:this")
            .expect("bind:this offered as an attribute");
        // Instance bindings reference a script variable, so the value is a brace
        // expression, not a quoted string.
        assert_eq!(bind_this.insert_text.as_deref(), Some("bind:this={$1}"));
    }

    #[test]
    fn template_attr_completion_offers_custom_component_public_members() {
        // Attribute-name completion on a custom PascalCase component tag
        // should offer the imported component's own public script members
        // (its props), the same members `bind:this` exposes at runtime —
        // not just the generic universal/event attrs.
        let dir =
            std::env::temp_dir().join(format!("mesh-lsp-component-attrs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("item-row.mesh"),
            r#"<template>
  <row />
</template>

<script lang="luau">
label_text = ""

local private_state = 1

onselect = nil

function render(self)
end
</script>
"#,
        )
        .unwrap();

        let (source, position) = fixture_with_cursor(
            r#"<template>
  <ItemRow $0 />
</template>

<script lang="luau">
local ItemRow = require("./item-row.mesh")
</script>
"#,
        );
        let uri = Url::from_file_path(dir.join("main.mesh")).unwrap();
        let doc = Document::new(uri, source);
        let labels = completion_labels(&doc, position);

        std::fs::remove_dir_all(&dir).ok();

        assert!(labels.contains(&"label_text".to_string()));
        assert!(labels.contains(&"onselect".to_string()));
        assert!(!labels.contains(&"private_state".to_string()));
    }

    #[test]
    fn template_attr_value_completion_offers_popover_anchor_enum() {
        let (source, position) = fixture_with_cursor(
            r#"<template>
  <popover anchor="$0" />
</template>
"#,
        );
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source);
        let labels = completion_labels(&doc, position);

        assert!(labels.contains(&"bottom".to_string()));
        assert!(labels.contains(&"top-left".to_string()));
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
