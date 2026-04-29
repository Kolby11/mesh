use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::{
    analyzer::script::{element_field_markdown, element_ref_markdown},
    document::Document,
    knowledge::{css::CSS_PROPERTIES, mesh_api::MESH_API_ENTRIES, tags::TAG_DEFS},
    plugin_registry::PluginRegistry,
    util::{Block, block_at_offset, block_content, position_to_offset},
};

pub fn hover(doc: &Document, position: Position, _registry: &PluginRegistry) -> Option<Hover> {
    let offset = position_to_offset(&doc.source, position);
    let loc = block_at_offset(&doc.source, offset);
    let content = block_content(&doc.source, &loc.block);

    let markdown = match &loc.block {
        Block::Template => hover_template(content, loc.offset_in_block)?,
        Block::Style => hover_style(content, loc.offset_in_block)?,
        Block::Script => hover_script(doc, content, loc.offset_in_block)?,
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

    // Check for tag name
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

    // Check for universal / event attribute
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

fn hover_style(content: &str, offset: usize) -> Option<String> {
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

fn hover_script(doc: &Document, content: &str, offset: usize) -> Option<String> {
    // Try to match "mesh.xxx.yyy" pattern around the cursor
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

    // Look for full mesh.xxx.yyy path
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
