use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::{
    analyzer::script::{element_field_markdown, element_ref_markdown, service_member_markdown},
    document::Document,
    knowledge::{css::CSS_PROPERTIES, mesh_api::MESH_API_ENTRIES, tags::TAG_DEFS},
    module_registry::ModuleRegistry,
    util::{Block, block_at_offset, block_content, position_to_offset},
};

pub fn hover(doc: &Document, position: Position, registry: &ModuleRegistry) -> Option<Hover> {
    let offset = position_to_offset(&doc.source, position);
    let loc = block_at_offset(&doc.source, offset);
    let content = block_content(&doc.source, &loc.block);

    let markdown = match &loc.block {
        Block::Template => hover_template(content, loc.offset_in_block)?,
        Block::Style => hover_style(doc, content, loc.offset_in_block)?,
        Block::Script => hover_script(doc, content, loc.offset_in_block, registry)?,
        _ => return None,
    };

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: None,
    })
}

fn hover_template(content: &str, offset: usize) -> Option<String> {
    let word = word_at(content, offset);
    if word.is_empty() {
        return None;
    }

    if let Some(tag) = TAG_DEFS.iter().find(|t| t.name == word) {
        let inherits = tag.inherited_base_names();
        let inherits_line = if inherits.is_empty() {
            String::new()
        } else {
            format!("\n\nInherits: `{}`", inherits.join("`, `"))
        };
        return Some(format!(
            "**`<{}>`** — {}\n\nCategory: `{}`{}",
            tag.name, tag.description, tag.category, inherits_line,
        ));
    }

    for attr in crate::knowledge::tags::UNIVERSAL_ATTRS {
        if attr.name == word {
            return Some(format!(
                "**`{}`** — {}\n\nDefined on: `MeshElement`",
                attr.name, attr.description
            ));
        }
    }

    for attr in crate::knowledge::tags::EVENT_ATTRS {
        if attr.name == word {
            return Some(format!(
                "**`{}`** — {}\n\nDefined on: `InteractiveElement`",
                attr.name, attr.description
            ));
        }
    }

    for tag in TAG_DEFS {
        for attr in tag.attributes {
            if attr.name == word {
                return Some(format!(
                    "**`{}`** — {}\n\nUsed by: `<{}>`",
                    attr.name, attr.description, tag.name
                ));
            }
        }

        for base in tag.bases {
            for attr in base.attributes {
                if attr.name == word {
                    return Some(format!(
                        "**`{}`** — {}\n\nDefined on: `{}`",
                        attr.name, attr.description, base.name
                    ));
                }
            }
        }
    }

    None
}

fn hover_style(doc: &Document, content: &str, offset: usize) -> Option<String> {
    if let Some(prop_name) = prop_call_name_at(content, offset)
        && let Some(markdown) = prop_markdown(doc, prop_name)
    {
        return Some(markdown);
    }

    let word = word_at(content, offset);
    if word.is_empty() {
        return None;
    }

    if let Some(prop) = CSS_PROPERTIES.iter().find(|p| p.name == word) {
        return Some(format!(
            "**`{}`**\n\n{}{}",
            prop.name,
            prop.description,
            if prop.values.is_empty() {
                String::new()
            } else {
                format!("\n\nValues: `{}`", prop.values.join("`, `"))
            }
        ));
    }

    None
}

fn hover_script(
    doc: &Document,
    content: &str,
    offset: usize,
    registry: &ModuleRegistry,
) -> Option<String> {
    let before = &content[..offset.min(content.len())];
    let after = &content[offset.min(content.len())..];

    let prefix = before
        .rsplit(|c: char| c.is_whitespace() || c == '(' || c == ',' || c == ';')
        .next()
        .unwrap_or("");
    let suffix_end = after
        .find(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ';')
        .unwrap_or(after.len());
    let suffix = &after[..suffix_end];

    let token = format!("{}{}", prefix, suffix);

    if let Some(rest) = token.strip_prefix("refs.") {
        let mut parts = rest.split('.');
        let ref_name = parts.next().unwrap_or("");
        if ref_name.is_empty() {
            return Some(
                "`refs` contains template elements declared with `ref=\"...\"` or `id=\"...\"`."
                    .to_string(),
            );
        }

        if let Some(field_name) = parts.next() {
            let element_ref = doc
                .element_refs
                .iter()
                .find(|element_ref| element_ref.name == ref_name)?;
            return element_field_markdown(&element_ref.tag, field_name);
        }

        return element_ref_markdown(doc, ref_name);
    }

    if let Some(rest) = token.strip_prefix("props.") {
        let prop_name = rest.split('.').next().unwrap_or(rest);
        if let Some(markdown) = prop_markdown(doc, prop_name) {
            return Some(markdown);
        }
    }

    if let Some(markdown) = hover_service_member(doc, &token, registry) {
        return Some(markdown);
    }

    if token.starts_with("mesh.") {
        let api_path = token.trim_start_matches("mesh.");
        if let Some(entry) = MESH_API_ENTRIES.iter().find(|e| e.path == api_path) {
            return Some(format!(
                "```lua\n{}\n```\n\n{}{}",
                entry.signature,
                entry.description,
                if entry.backend_only {
                    "\n\n_Backend-only API._"
                } else {
                    ""
                }
            ));
        }
    }

    None
}

fn hover_service_member(doc: &Document, token: &str, registry: &ModuleRegistry) -> Option<String> {
    let separator = token.find(['.', ':'])?;
    let (var_name, member) = token.split_at(separator);
    let member = member.strip_prefix(['.', ':'])?;
    let interface = doc.interface_proxies.get(var_name)?;
    service_member_markdown(var_name, interface, member, registry)
}

#[cfg(test)]
mod tests {
    use mesh_core_service::parse_interface_contract;
    use tower_lsp::lsp_types::{HoverContents, Url};

    use super::{hover, hover_template};
    use crate::{
        document::Document,
        module_registry::{InterfaceShape, ModuleRegistry},
        util::offset_to_position,
    };

    #[test]
    fn gesture_and_touch_hover_uses_event_documentation() {
        for handler in ["onswipe", "onpinch", "ontouchstart", "ontap", "onlongpress"] {
            let markdown = hover_template(handler, handler.len() / 2)
                .unwrap_or_else(|| panic!("missing hover for {handler}"));
            assert!(markdown.contains(handler));
            assert!(markdown.contains("InteractiveElement"));
        }
    }

    #[test]
    fn service_member_hover_uses_declared_contract_documentation() {
        let source = r#"<template></template>

<script lang="luau">
local audio = require("mesh.audio")
audio.percent
audio.set_volume()
</script>
"#;
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source.into());
        let mut registry = ModuleRegistry::empty();
        let contract = parse_interface_contract(
            "mesh.audio",
            "1.0",
            &serde_json::json!({
                "state": {
                    "percent": { "type": "float", "description": "Current volume." }
                },
                "methods": {
                    "set_volume": {
                        "args": [{ "name": "percent", "type": "float" }],
                        "returns": "Result"
                    }
                },
                "events": {},
                "types": {}
            }),
        )
        .unwrap();
        registry
            .interface_contracts
            .insert("mesh.audio".into(), contract);

        let field = hover_at(&doc, source.find("percent").unwrap(), &registry);
        assert!(field.contains("audio.percent"));
        assert!(field.contains("number"));
        assert!(field.contains("Current volume."));

        let command = hover_at(&doc, source.find("set_volume").unwrap(), &registry);
        assert!(command.contains("audio.set_volume"));
        assert!(command.contains("set_volume(percent: number): Result"));
        assert!(command.contains("Typed command declared by `mesh.audio`."));
    }

    #[test]
    fn service_member_hover_uses_inferred_backend_shape_when_contract_is_missing() {
        let source = r#"<template></template>

<script lang="luau">
local power = require("mesh.power")
power.available
power.refresh()
</script>
"#;
        let doc = Document::new(Url::parse("file:///test.mesh").unwrap(), source.into());
        let mut registry = ModuleRegistry::empty();
        registry.interface_shapes.insert(
            "mesh.power".into(),
            InterfaceShape {
                state_fields: vec!["available".into()],
                commands: vec!["refresh".into()],
            },
        );

        let field = hover_at(&doc, source.find("available").unwrap(), &registry);
        assert!(field.contains("State field emitted by the `mesh.power` backend service."));
        assert!(field.contains("Read as `power.available`."));

        let command = hover_at(&doc, source.find("refresh").unwrap(), &registry);
        assert!(
            command.contains("Sends the `refresh` command to the `mesh.power` backend service.")
        );
        assert!(command.contains("Call as `power.refresh()`."));
    }

    fn hover_at(doc: &Document, offset: usize, registry: &ModuleRegistry) -> String {
        let hover = hover(doc, offset_to_position(&doc.source, offset), registry).expect("hover");
        let HoverContents::Markup(markdown) = hover.contents else {
            panic!("expected markdown hover");
        };
        markdown.value
    }
}

fn prop_markdown(doc: &Document, prop_name: &str) -> Option<String> {
    let prop = doc
        .parsed
        .as_ref()
        .and_then(|parsed| parsed.props.as_ref())?
        .props
        .iter()
        .find(|prop| prop.name == prop_name)?;
    Some(format!(
        "**`{}`** prop\n\nType: `{}` / Lua `{}`{}{}",
        prop.name,
        prop.ty.as_str(),
        prop.ty.lua_type(),
        prop.default
            .as_ref()
            .map(|value| format!("\n\nDefault: `{}`", prop_default_label(value)))
            .unwrap_or_default(),
        prop.label
            .as_ref()
            .map(|label| format!("\n\nLabel: {}", prop_label(label)))
            .unwrap_or_default()
    ))
}

fn prop_default_label(value: &mesh_core_component::PropValue) -> String {
    match value {
        mesh_core_component::PropValue::String(value) => value.clone(),
        mesh_core_component::PropValue::Number(value) => value.to_string(),
        mesh_core_component::PropValue::Bool(value) => value.to_string(),
    }
}

fn prop_label(label: &mesh_core_component::LocalizedLabel) -> String {
    match label {
        mesh_core_component::LocalizedLabel::Literal(value) => format!("`{value}`"),
        mesh_core_component::LocalizedLabel::Translation { key, fallback } => {
            if let Some(fallback) = fallback {
                format!("`{key}` (`{fallback}`)")
            } else {
                format!("`{key}`")
            }
        }
    }
}

fn prop_call_name_at(source: &str, offset: usize) -> Option<&str> {
    let before = &source[..offset.min(source.len())];
    let call_start = before.rfind("prop(")?;
    let after_start = call_start + "prop(".len();
    let after = &source[after_start..];
    let close = after.find(')')?;
    let name = after[..close].trim();
    if name.is_empty() { None } else { Some(name) }
}

/// Extract the word (alphanumeric + hyphens) around a byte offset.
fn word_at(source: &str, offset: usize) -> &str {
    let offset = offset.min(source.len());
    let bytes = source.as_bytes();

    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';

    let start = bytes[..offset]
        .iter()
        .rposition(|&b| !is_word(b))
        .map(|p| p + 1)
        .unwrap_or(0);

    let end = bytes[offset..]
        .iter()
        .position(|&b| !is_word(b))
        .map(|p| p + offset)
        .unwrap_or(source.len());

    &source[start..end]
}
