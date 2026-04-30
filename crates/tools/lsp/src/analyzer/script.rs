use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

use crate::{
    document::{Document, ElementRefSource},
    knowledge::mesh_api::MESH_API_ENTRIES,
    plugin_registry::PluginRegistry,
    util::ScriptContext,
};

use InsertTextFormat as Fmt;
use mesh_core_elements::{
    BASE_ELEMENT_FIELDS, ELEMENT_TYPE_DEFS, ElementFieldDef, ElementFieldType, element_type_for_tag,
};

pub fn complete(
    ctx: ScriptContext,
    doc: &Document,
    registry: &PluginRegistry,
) -> Vec<CompletionItem> {
    match ctx {
        ScriptContext::MeshApi { prefix } => complete_mesh_api(&prefix, doc, registry),
        ScriptContext::Refs { prefix } => complete_refs(&prefix, doc),
        ScriptContext::RefMember { ref_name, prefix } => {
            complete_ref_members(&ref_name, &prefix, doc)
        }
        ScriptContext::EventCurrentTarget { prefix } => complete_event_current_target(&prefix),
        ScriptContext::ServiceName => complete_service_names(registry),
        ScriptContext::InterfaceProxy { var_name, prefix } => {
            complete_interface_proxy(&var_name, &prefix, doc, registry)
        }
        ScriptContext::General => complete_state_vars(doc),
    }
}

fn complete_mesh_api(
    prefix: &str,
    _doc: &Document,
    _registry: &PluginRegistry,
) -> Vec<CompletionItem> {
    MESH_API_ENTRIES
        .iter()
        .filter(|entry| entry.path.starts_with(prefix))
        .map(|entry| {
            // Full sub-path as label; insert only what hasn't been typed yet + call params.
            let label = entry.path.to_string();
            let remaining = entry.path.strip_prefix(prefix).unwrap_or(entry.path);
            let (insert_text, insert_text_format) = call_snippet(entry.path, remaining);

            CompletionItem {
                label,
                kind: Some(CompletionItemKind::METHOD),
                detail: Some(entry.signature.to_string()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "```lua\n{}\n```\n\n{}{}",
                        entry.signature,
                        entry.description,
                        if entry.backend_only {
                            "\n\n_Backend-only API._"
                        } else {
                            ""
                        }
                    ),
                })),
                insert_text: Some(insert_text),
                insert_text_format: Some(insert_text_format),
                ..Default::default()
            }
        })
        .collect()
}

/// Build an insert snippet from the remaining suffix (entry.path minus already-typed prefix).
/// Call-parameter snippets keyed by the full API path from `MESH_API_ENTRIES`.
/// The snippet is appended after `remaining` (the part the user hasn't typed yet).
/// e.g. path="state.set", params=`("$1", $2)` → insert "state.set(\"$1\", $2)"
static CALL_PARAMS: &[(&str, &str)] = &[
    ("state.set", "(\"$1\", $2)"),
    ("state.get", "(\"$1\")"),
    ("service.bind", "(\"$1\", \"$2\")"),
    ("service.on", "(\"$1\", \"$2\")"),
    ("service.emit", "($1)"),
    ("service.emit_json", "($1)"),
    ("service.emit_unavailable", "()"),
    ("service.set_poll_interval", "($1)"),
    ("service.payload", "()"),
    ("service.has_capability", "(\"$1\")"),
    ("interfaces.get", "(\"$1\")"),
    ("events.subscribe", "(\"$1\", \"$2\")"),
    ("events.publish", "(\"$1\", $2)"),
    ("theme.token", "(\"$1\")"),
    ("locale.translate", "(\"$1\")"),
    ("ui.request_redraw", "()"),
    ("exec", "(\"$1\", {$2})"),
    ("exec_shell", "(\"$1\")"),
    ("log.info", "($1)"),
    ("log.warn", "($1)"),
    ("log.error", "($1)"),
];

/// Build the insert snippet for an API entry.
/// `path` is the full entry path; `remaining` is the suffix after what the user has already typed.
fn call_snippet(path: &str, remaining: &str) -> (String, InsertTextFormat) {
    if let Some(&(_, params)) = CALL_PARAMS.iter().find(|&&(key, _)| key == path) {
        (format!("{remaining}{params}"), InsertTextFormat::SNIPPET)
    } else {
        (remaining.to_string(), InsertTextFormat::PLAIN_TEXT)
    }
}

fn complete_interface_proxy(
    var_name: &str,
    prefix: &str,
    doc: &Document,
    registry: &PluginRegistry,
) -> Vec<CompletionItem> {
    let Some(iface_name) = doc.interface_proxies.get(var_name) else {
        return vec![];
    };

    let shape = registry.interface_shapes.get(iface_name);
    let mut items: Vec<CompletionItem> = Vec::new();

    // State fields from backend script analysis
    if let Some(shape) = shape {
        for field in &shape.state_fields {
            if !field.starts_with(prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: field.clone(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(format!("{iface_name} state")),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "State field emitted by the `{iface_name}` backend service.\n\nRead as `{var_name}.{field}`."
                    ),
                })),
                ..Default::default()
            });
        }

        // Commands from backend script analysis
        for cmd in &shape.commands {
            if !cmd.starts_with(prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: cmd.clone(),
                kind: Some(CompletionItemKind::METHOD),
                detail: Some(format!("{iface_name} command")),
                insert_text: Some(format!("{cmd}()")),
                insert_text_format: Some(Fmt::PLAIN_TEXT),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "Sends the `{cmd}` command to the `{iface_name}` backend service.\n\nCall as `{var_name}.{cmd}()`."
                    ),
                })),
                ..Default::default()
            });
        }
    }

    // Always include on_change
    if "on_change".starts_with(prefix) {
        items.push(CompletionItem {
            label: "on_change".to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some("register change handler".to_string()),
            insert_text: Some("on_change(function()\n\t$0\nend)".to_string()),
            insert_text_format: Some(Fmt::SNIPPET),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "Register a handler called whenever the `{iface_name}` service emits an update.\n\n```lua\n{var_name}.on_change(function()\n    -- read {var_name}.percent, {var_name}.muted, etc.\nend)\n```"
                ),
            })),
            ..Default::default()
        });
    }

    items
}

fn complete_service_names(registry: &PluginRegistry) -> Vec<CompletionItem> {
    registry
        .service_names()
        .into_iter()
        .map(|name| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::INTERFACE),
            detail: Some("service interface".to_string()),
            ..Default::default()
        })
        .collect()
}

fn complete_state_vars(doc: &Document) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = doc
        .state_vars
        .iter()
        .map(|v| CompletionItem {
            label: v.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("state variable".to_string()),
            ..Default::default()
        })
        .collect();

    for (_, local) in &doc.service_bindings {
        items.push(CompletionItem {
            label: local.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("service binding".to_string()),
            ..Default::default()
        });
    }

    for fname in &doc.script_functions {
        items.push(CompletionItem {
            label: fname.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("function".to_string()),
            ..Default::default()
        });
    }

    if !doc.element_refs.is_empty() {
        items.push(CompletionItem {
            label: "refs".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("element refs".to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value:
                    "`refs` contains template elements declared with `ref=\"...\"` or `id=\"...\"`."
                        .to_string(),
            })),
            ..Default::default()
        });
    }

    items
}

fn complete_refs(prefix: &str, doc: &Document) -> Vec<CompletionItem> {
    doc.element_refs
        .iter()
        .filter(|element_ref| element_ref.name.starts_with(prefix))
        .map(|element_ref| CompletionItem {
            label: element_ref.name.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some(format!(
                "{} from <{}>{}",
                element_ref.element_type,
                element_ref.tag,
                match element_ref.source {
                    ElementRefSource::Ref => "",
                    ElementRefSource::Id => " id",
                }
            )),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "`refs.{}` is a `{}` exposed by the MESH core element model.",
                    element_ref.name, element_ref.element_type
                ),
            })),
            ..Default::default()
        })
        .collect()
}

fn complete_ref_members(ref_name: &str, prefix: &str, doc: &Document) -> Vec<CompletionItem> {
    let Some(element_ref) = doc
        .element_refs
        .iter()
        .find(|element_ref| element_ref.name == ref_name)
    else {
        return vec![];
    };

    complete_element_fields_for_tag(&element_ref.tag, prefix)
}

fn complete_event_current_target(prefix: &str) -> Vec<CompletionItem> {
    complete_element_fields_for_tag("box", prefix)
}

fn complete_element_fields_for_tag(tag: &str, prefix: &str) -> Vec<CompletionItem> {
    let type_def = element_type_for_tag(tag);
    let mut fields: Vec<&'static ElementFieldDef> = Vec::new();
    push_fields(&mut fields, BASE_ELEMENT_FIELDS);
    push_fields(&mut fields, type_def.fields);

    fields
        .into_iter()
        .filter(|field| field.name.starts_with(prefix))
        .map(|field| field_completion_item(field, type_def.type_name))
        .collect()
}

fn push_fields(fields: &mut Vec<&'static ElementFieldDef>, new_fields: &'static [ElementFieldDef]) {
    for field in new_fields {
        if fields.iter().any(|existing| existing.name == field.name) {
            continue;
        }
        fields.push(field);
    }
}

fn field_completion_item(field: &ElementFieldDef, element_type: &str) -> CompletionItem {
    CompletionItem {
        label: field.name.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(format!(
            "{}{}",
            luau_type_name(field.field_type),
            if field.optional { "?" } else { "" }
        )),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**`{}.{}`**: `{}`{}\n\n{}",
                element_type,
                field.name,
                luau_type_name(field.field_type),
                if field.optional { "?" } else { "" },
                field.description
            ),
        })),
        ..Default::default()
    }
}

fn luau_type_name(field_type: ElementFieldType) -> &'static str {
    match field_type {
        ElementFieldType::String => "string",
        ElementFieldType::Number => "number",
        ElementFieldType::Boolean => "boolean",
        ElementFieldType::Rect => "MeshRect",
        ElementFieldType::Object => "table<string, string>",
    }
}

pub(crate) fn element_field_markdown(tag: &str, field_name: &str) -> Option<String> {
    let type_def = element_type_for_tag(tag);
    BASE_ELEMENT_FIELDS
        .iter()
        .chain(type_def.fields.iter())
        .find(|field| field.name == field_name)
        .map(|field| {
            format!(
                "**`{}.{}`**: `{}`{}\n\n{}",
                type_def.type_name,
                field.name,
                luau_type_name(field.field_type),
                if field.optional { "?" } else { "" },
                field.description
            )
        })
}

pub(crate) fn element_ref_markdown(doc: &Document, ref_name: &str) -> Option<String> {
    let element_ref = doc
        .element_refs
        .iter()
        .find(|element_ref| element_ref.name == ref_name)?;
    let type_def = ELEMENT_TYPE_DEFS
        .iter()
        .find(|def| def.tag == element_ref.tag)
        .unwrap_or_else(|| element_type_for_tag(&element_ref.tag));
    Some(format!(
        "**`refs.{}`**: `{}`\n\nTemplate binding for `<{}>` via `{}`.",
        element_ref.name,
        type_def.type_name,
        element_ref.tag,
        match element_ref.source {
            ElementRefSource::Ref => "ref",
            ElementRefSource::Id => "id",
        }
    ))
}
