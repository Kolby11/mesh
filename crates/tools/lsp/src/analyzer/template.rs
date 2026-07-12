use mesh_core_component::ComponentImportTarget;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

use crate::{
    analyzer::script::resolve_component_public_members,
    document::Document,
    knowledge::tags::{EVENT_ATTRS, TAG_DEFS, TagDef, UNIVERSAL_ATTRS},
    module_registry::ModuleRegistry,
    util::TemplateContext,
};

pub fn complete(
    ctx: TemplateContext,
    doc: &Document,
    registry: &ModuleRegistry,
) -> Vec<CompletionItem> {
    match ctx {
        TemplateContext::TagName { .. } => complete_tags(doc),
        TemplateContext::AttrName { tag } => complete_attrs(&tag, doc, registry),
        TemplateContext::AttrValue { tag, attr } => complete_attr_value(&tag, &attr, doc, registry),
        TemplateContext::Expr => complete_expr(doc),
        TemplateContext::Content => vec![],
    }
}

fn complete_tags(doc: &Document) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = TAG_DEFS.iter().map(tag_completion_item).collect();

    for import in &doc.imports {
        if matches!(
            import.target,
            ComponentImportTarget::ComponentLocal(_) | ComponentImportTarget::ComponentModule(_)
        ) {
            let alias = &import.alias;
            items.push(CompletionItem {
                label: alias.clone(),
                kind: Some(CompletionItemKind::MODULE),
                detail: Some("imported component".to_string()),
                insert_text: Some(format!("{alias}>\n  $1\n</{alias}>")),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }
    }

    items
}

fn tag_completion_item(tag: &TagDef) -> CompletionItem {
    let insert_text = if tag.self_closing {
        format!("{} $1/>", tag.name)
    } else {
        format!("{}>\n  $1\n</{}>", tag.name, tag.name)
    };

    CompletionItem {
        label: tag.name.to_string(),
        kind: Some(CompletionItemKind::CLASS),
        detail: Some(tag.category.to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: tag.description.to_string(),
        })),
        insert_text: Some(insert_text),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn complete_attrs(tag: &str, doc: &Document, registry: &ModuleRegistry) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Tag-specific attributes
    if let Some(tag_def) = TAG_DEFS.iter().find(|t| t.name == tag) {
        for attr in tag_def.all_attributes() {
            items.push(attr_completion_item(attr.name, attr.description, false));
        }
    } else if let Some((variables, _functions)) =
        resolve_component_public_members(doc, tag, registry)
    {
        // Custom PascalCase component tag: offer the imported component's own
        // public script members as its props, mirroring what `bind:this`
        // exposes at runtime.
        for variable in variables {
            let is_handler = variable.starts_with("on");
            items.push(attr_completion_item(
                &variable,
                &format!("public member of <{tag}>"),
                is_handler,
            ));
        }
    } else {
        for attr in UNIVERSAL_ATTRS {
            items.push(attr_completion_item(attr.name, attr.description, false));
        }
        for attr in EVENT_ATTRS {
            items.push(attr_completion_item(attr.name, attr.description, true));
        }
    }

    // `bind:this` exposes a live reference to this element or mounted component
    // instance to the script as `{var}`. Available on every tag. Uses brace
    // (instance-binding) value syntax, not quotes.
    items.push(instance_binding_completion_item(
        "bind:this",
        "Live reference: exposes this element/component instance to the script as {var}",
    ));

    // Two-way binding prefix for input-like tags
    if matches!(
        tag,
        "input"
            | "text-input"
            | "password-input"
            | "search-input"
            | "number-input"
            | "email-input"
            | "url-input"
            | "slider"
            | "switch"
            | "checkbox"
    ) {
        items.push(attr_completion_item(
            "bind:value",
            "Two-way binding: reads from and writes back to a state variable",
            false,
        ));
    }

    items
}

/// A `bind:this` attribute completion. Inserts brace value syntax `name={$1}`
/// because the instance binding references a script variable, not a string
/// literal.
fn instance_binding_completion_item(name: &str, description: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(description.to_string()),
        insert_text: Some(format!("{name}={{$1}}")),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn attr_completion_item(name: &str, description: &str, is_handler: bool) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(if is_handler {
            CompletionItemKind::EVENT
        } else {
            CompletionItemKind::FIELD
        }),
        detail: Some(description.to_string()),
        insert_text: Some(format!("{}=\"$1\"", name)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn complete_attr_value(
    tag: &str,
    attr: &str,
    doc: &Document,
    registry: &ModuleRegistry,
) -> Vec<CompletionItem> {
    // Event handler attrs: complete with function names. Applies both to core
    // element handlers (onclick, ...) and to a mounted component's own
    // handler-shaped public members (onfirst, onselect, ...).
    if attr.starts_with("on") {
        let mut items: Vec<CompletionItem> = doc
            .script_functions
            .iter()
            .map(|name| CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("script function".to_string()),
                ..Default::default()
            })
            .collect();
        if let Some((_variables, functions)) = resolve_component_public_members(doc, tag, registry)
        {
            for name in functions {
                if !items.iter().any(|item| item.label == name) {
                    items.push(CompletionItem {
                        label: name,
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some("script function".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
        return items;
    }

    // class attr: complete with class names from the style block
    if attr == "class" {
        return class_completions(doc);
    }

    // icon name attr: common XDG icon names
    if attr == "name" && matches!(tag, "icon" | "icon-button") {
        return icon_name_completions();
    }

    // Known enum-like attribute: complete with its declared value set, e.g.
    // <popover anchor="..."> or <row overflow="...">.
    if let Some(tag_def) = TAG_DEFS.iter().find(|t| t.name == tag) {
        if let Some(attr_def) = tag_def
            .all_attributes()
            .into_iter()
            .find(|a| a.name == attr)
        {
            if !attr_def.values.is_empty() {
                return attr_def
                    .values
                    .iter()
                    .map(|value| CompletionItem {
                        label: value.to_string(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some(format!("{attr} value")),
                        ..Default::default()
                    })
                    .collect();
            }
        }
    }

    vec![]
}

fn complete_expr(doc: &Document) -> Vec<CompletionItem> {
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

    for name in &doc.script_functions {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("script function".to_string()),
            ..Default::default()
        });
    }

    items
}

fn class_completions(doc: &Document) -> Vec<CompletionItem> {
    let Some(parsed) = &doc.parsed else {
        return vec![];
    };
    let Some(style) = &parsed.style else {
        return vec![];
    };

    let mut names = Vec::new();
    for rule in &style.rules {
        if let mesh_core_component::style::Selector::Class(name) = &rule.selector {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }

    names
        .into_iter()
        .map(|name| CompletionItem {
            label: name,
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("CSS class".to_string()),
            ..Default::default()
        })
        .collect()
}

fn icon_name_completions() -> Vec<CompletionItem> {
    const COMMON_ICONS: &[&str] = &[
        "audio-volume-high",
        "audio-volume-medium",
        "audio-volume-low",
        "audio-volume-muted",
        "audio-volume-off",
        "network-wireless",
        "network-wireless-signal-excellent",
        "network-wireless-signal-good",
        "network-wireless-signal-weak",
        "network-wireless-offline",
        "network-wired",
        "network-offline",
        "battery-full",
        "battery-good",
        "battery-low",
        "battery-caution",
        "battery-empty",
        "battery-missing",
        "battery-charging",
        "media-playback-start",
        "media-playback-pause",
        "media-playback-stop",
        "media-skip-forward",
        "media-skip-backward",
        "media-record",
        "system-shutdown",
        "system-reboot",
        "system-log-out",
        "system-lock-screen",
        "system-search",
        "system-run",
        "system-file-manager",
        "preferences-system",
        "preferences-desktop",
        "applications-all",
        "applications-internet",
        "applications-graphics",
        "folder",
        "folder-open",
        "folder-home",
        "edit-delete",
        "edit-copy",
        "edit-paste",
        "edit-cut",
        "go-up",
        "go-down",
        "go-previous",
        "go-next",
        "list-add",
        "list-remove",
        "view-refresh",
        "dialog-information",
        "dialog-warning",
        "dialog-error",
        "dialog-question",
        "weather-clear",
        "weather-clouds",
        "weather-rain",
        "bluetooth",
        "bluetooth-active",
        "bluetooth-disabled",
        "display-brightness",
        "input-keyboard",
        "input-mouse",
        "notification-new",
        "notification-read",
    ];

    COMMON_ICONS
        .iter()
        .map(|name| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some("XDG icon name".to_string()),
            ..Default::default()
        })
        .collect()
}
