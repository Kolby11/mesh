use std::path::{Path, PathBuf};

use mesh_core_component::ComponentImportTarget;
use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use crate::{
    document::{ByteSpan, Document, ScriptSymbolKind, block_content_range},
    module_registry::ModuleRegistry,
    util::{Block, block_at_offset, position_to_offset},
};

pub fn definition(
    doc: &Document,
    position: Position,
    registry: &ModuleRegistry,
) -> Option<GotoDefinitionResponse> {
    let offset = position_to_offset(&doc.source, position);
    let loc = block_at_offset(&doc.source, offset);

    let location = match loc.block {
        Block::Template => template_definition(doc, offset, registry),
        Block::Script => script_definition(doc, offset, registry),
        _ => None,
    }?;

    Some(GotoDefinitionResponse::Scalar(location))
}

fn template_definition(
    doc: &Document,
    offset: usize,
    registry: &ModuleRegistry,
) -> Option<Location> {
    let token = identifier_at(&doc.source, offset, true)?;

    if cursor_is_component_tag(&doc.source, token.start) {
        if let Some(location) = resolve_component_target(doc, &token.text, registry) {
            return Some(location);
        }
    }

    resolve_script_symbol(doc, &token.text)
}

fn script_definition(doc: &Document, offset: usize, registry: &ModuleRegistry) -> Option<Location> {
    let token = identifier_at(&doc.source, offset, false)?;

    if let Some(location) = resolve_component_target(doc, &token.text, registry) {
        return Some(location);
    }

    resolve_script_symbol(doc, &token.text)
}

fn resolve_script_symbol(doc: &Document, name: &str) -> Option<Location> {
    let symbol = doc
        .script_symbols
        .iter()
        .find(|symbol| symbol.name == name && symbol.kind == ScriptSymbolKind::Function)
        .or_else(|| doc.script_symbols.iter().find(|symbol| symbol.name == name))?;

    Some(Location::new(
        doc.uri.clone(),
        range_from_span(&doc.source, symbol.span),
    ))
}

fn resolve_component_target(
    doc: &Document,
    name: &str,
    registry: &ModuleRegistry,
) -> Option<Location> {
    let import = doc.imports.iter().find(|import| import.alias == name);
    let path = if let Some(import) = import {
        resolve_import_target(doc, &import.target, registry)?
    } else {
        let module_id = registry.exported_component_tags().get(name)?;
        registry.module_entrypoint(module_id)?.to_path_buf()
    };

    let uri = Url::from_file_path(&path).ok()?;
    Some(Location::new(
        uri,
        Range::new(Position::new(0, 0), Position::new(0, 0)),
    ))
}

fn resolve_import_target(
    doc: &Document,
    target: &ComponentImportTarget,
    registry: &ModuleRegistry,
) -> Option<PathBuf> {
    match target {
        ComponentImportTarget::ComponentLocal(path) => resolve_local_component_path(doc, path),
        ComponentImportTarget::ComponentModule(module_id) => {
            registry.module_entrypoint(module_id).map(Path::to_path_buf)
        }
        ComponentImportTarget::InterfaceApi { .. } => None,
    }
}

fn resolve_local_component_path(doc: &Document, path: &str) -> Option<PathBuf> {
    let doc_path = doc.uri.to_file_path().ok()?;
    if path.starts_with("@src/") {
        let module_root = find_module_root(&doc_path)?;
        return Some(
            module_root
                .join("src")
                .join(path.trim_start_matches("@src/")),
        );
    }

    let base = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        doc_path.parent()?.join(path)
    };
    Some(base)
}

fn find_module_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent()?;
    loop {
        if current.join("module.json").exists()
            || current.join("package.json").exists()
            || current.join("mesh.toml").exists()
        {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn cursor_is_component_tag(source: &str, token_start: usize) -> bool {
    let Some((template_start, template_end)) = block_content_range(source, "template") else {
        return false;
    };
    if token_start < template_start || token_start > template_end {
        return false;
    }

    let before = &source[template_start..token_start];
    let last_lt = before.rfind('<');
    let last_gt = before.rfind('>');
    matches!(last_lt, Some(lt) if last_gt.is_none_or(|gt| gt < lt))
}

struct Identifier<'a> {
    start: usize,
    text: &'a str,
}

fn identifier_at(source: &str, offset: usize, allow_hyphen: bool) -> Option<Identifier<'_>> {
    let bytes = source.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || (allow_hyphen && b == b'-');

    let mut probe = offset.min(bytes.len().saturating_sub(1));
    if !is_ident(bytes[probe]) {
        if probe > 0 && is_ident(bytes[probe - 1]) {
            probe -= 1;
        } else {
            return None;
        }
    }

    let mut start = probe;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }

    let mut end = probe + 1;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }

    Some(Identifier {
        start,
        text: &source[start..end],
    })
}

fn range_from_span(source: &str, span: ByteSpan) -> Range {
    Range::new(
        offset_to_position(source, span.start),
        offset_to_position(source, span.end),
    )
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    let mut seen = 0usize;

    for ch in source.chars() {
        if seen >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
        seen += ch.len_utf8();
    }

    Position::new(line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn fixture_position(source: &str) -> (String, Position) {
        let marker = "$0";
        let idx = source.find(marker).expect("cursor marker");
        let source = source.replacen(marker, "", 1);
        let pos = offset_to_position(&source, idx);
        (source, pos)
    }

    #[test]
    fn template_custom_component_definitions_resolve_to_local_file() {
        let (source, position) = fixture_position(
            r#"<template>
  <ThemeBut$0ton />
</template>

<script lang="luau">
local ThemeButton = require("./components/theme-button.mesh")
</script>
"#,
        );
        let doc = Document::new(
            Url::parse("file:///workspace/module/src/main.mesh").unwrap(),
            source,
        );
        let location = definition(&doc, position, &ModuleRegistry::empty())
            .and_then(|response| match response {
                GotoDefinitionResponse::Scalar(location) => Some(location),
                _ => None,
            })
            .expect("component definition");

        assert_eq!(
            location.uri.to_file_path().unwrap(),
            PathBuf::from("/workspace/module/src/components/theme-button.mesh")
        );
    }

    #[test]
    fn script_function_definitions_resolve_in_same_file() {
        let (source, position) = fixture_position(
            r#"<template></template>

<script lang="luau">
local function apply_theme()
end

appl$0y_theme()
</script>
"#,
        );
        let doc = Document::new(
            Url::parse("file:///workspace/module/src/main.mesh").unwrap(),
            source,
        );
        let location = definition(&doc, position, &ModuleRegistry::empty())
            .and_then(|response| match response {
                GotoDefinitionResponse::Scalar(location) => Some(location),
                _ => None,
            })
            .expect("function definition");

        assert_eq!(location.uri, doc.uri);
        assert_eq!(location.range.start.line, 3);
    }

    #[test]
    fn template_handler_references_resolve_to_script_function() {
        let (source, position) = fixture_position(
            r#"<template>
  <button onclick={onTogg$0leThemeSurface} />
</template>

<script lang="luau">
function onToggleThemeSurface(event)
end
</script>
"#,
        );
        let doc = Document::new(
            Url::parse("file:///workspace/module/src/main.mesh").unwrap(),
            source,
        );
        let location = definition(&doc, position, &ModuleRegistry::empty())
            .and_then(|response| match response {
                GotoDefinitionResponse::Scalar(location) => Some(location),
                _ => None,
            })
            .expect("handler definition");

        assert_eq!(location.uri, doc.uri);
        assert_eq!(location.range.start.line, 5);
    }
}
