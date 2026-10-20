use super::SettingsDocument;
use crate::{
    json::cursor,
    module_registry::ModuleRegistry,
    util::{offset_to_position, position_to_offset},
};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Diagnostic, DiagnosticSeverity,
    Position, Range, TextEdit,
};

pub(super) fn complete(
    doc: &SettingsDocument,
    position: Position,
    registry: &ModuleRegistry,
) -> Option<Vec<CompletionItem>> {
    let offset = position_to_offset(&doc.source, position);
    let ctx = cursor::context_at(&doc.source, offset);
    if ctx.role != cursor::Role::Value
        || ctx.value_key.as_deref() != Some("stylesheet")
        || !ctx.in_string
        || !ctx.path.ends_with(&["shell".into(), "theme".into()])
    {
        return None;
    }
    let prefix = ctx
        .partial
        .rsplit_once('@')
        .map(|(_, tail)| format!("@{tail}"))
        .unwrap_or_default();
    if !prefix.is_empty() && prefix.contains(['{', '}', ';']) {
        return Some(vec![]);
    }
    // Selector identifiers contain no JSON escapes, so their byte length is
    // also the length of the authored suffix to replace.
    let start = offset.saturating_sub(prefix.len());
    let mut items = Vec::new();
    for (module, parts) in &registry.style_parts {
        for part in parts {
            let target = format!("{module}::part({part})");
            if target.starts_with(&prefix) {
                items.push(CompletionItem {
                    label: target.clone(),
                    kind: Some(CompletionItemKind::REFERENCE),
                    detail: Some("Public module style part".into()),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: Range::new(offset_to_position(&doc.source, start), position),
                        new_text: target,
                    })),
                    ..Default::default()
                });
            }
        }
    }
    items.sort_by(|left, right| left.label.cmp(&right.label));
    Some(items)
}

pub(super) fn diagnostics(doc: &SettingsDocument, registry: &ModuleRegistry) -> Vec<Diagnostic> {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(&doc.source) else {
        return vec![];
    };
    let settings = if super::is_profile_uri(&doc.uri) {
        &document["settings"]
    } else {
        &document
    };
    let Some(source) = settings
        .pointer("/shell/theme/stylesheet")
        .and_then(|value| value.as_str())
    else {
        return vec![];
    };
    let messages = if let Some(snapshot) = &registry.snapshot {
        snapshot.user_stylesheet_diagnostics(source)
    } else {
        mesh_core_theme::parts::parse_user_stylesheet(source)
            .err()
            .into_iter()
            .collect()
    };
    let range =
        crate::json::diagnostics::find_key_range(&doc.source, "stylesheet").unwrap_or_default();
    messages
        .into_iter()
        .map(|message| Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("mesh-style-parts".into()),
            message,
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;

    #[test]
    fn profile_stylesheet_completes_only_public_parts() {
        let mut registry = ModuleRegistry::empty();
        registry
            .style_parts
            .insert("@mesh/bar".into(), ["glyph".into(), "frame".into()].into());
        let source = r#"{"settings":{"shell":{"theme":{"stylesheet":"@mesh/bar::part(gl"}}}}"#;
        let offset = source.find("(gl").unwrap() + 3;
        let doc = SettingsDocument::new(
            Url::parse("file:///w/config/profiles/desk.json").unwrap(),
            source.into(),
        );
        assert!(super::super::is_settings_uri(&doc.uri));
        let items = complete(&doc, offset_to_position(source, offset), &registry).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "@mesh/bar::part(glyph)");
        let Some(CompletionTextEdit::Edit(edit)) = &items[0].text_edit else {
            panic!("missing edit");
        };
        assert_eq!(edit.new_text, "@mesh/bar::part(glyph)");
        assert_eq!(
            position_to_offset(source, edit.range.start),
            source.find('@').unwrap()
        );
    }

    #[test]
    fn profile_stylesheet_reports_invalid_private_selectors() {
        let doc = SettingsDocument::new(
            Url::parse("file:///w/config/profiles/desk.json").unwrap(),
            r#"{"settings":{"shell":{"theme":{"stylesheet":".private { width: 3px; }"}}}}"#.into(),
        );
        let diagnostics = diagnostics(&doc, &ModuleRegistry::empty());
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("::part"));
    }
}
