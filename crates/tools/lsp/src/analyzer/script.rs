use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

use crate::{
    document::{Document, ElementRefAliasTarget, ElementRefSource},
    knowledge::mesh_api::MESH_API_ENTRIES,
    module_registry::ModuleRegistry,
    util::ScriptContext,
};

use InsertTextFormat as Fmt;
use mesh_core_elements::{
    BASE_ELEMENT_FIELDS, ELEMENT_TYPE_DEFS, ElementAttributeDef, ElementFieldDef, ElementFieldType,
    element_contract_for_tag, element_type_for_tag,
};

pub fn complete(
    ctx: ScriptContext,
    doc: &Document,
    registry: &ModuleRegistry,
) -> Vec<CompletionItem> {
    match ctx {
        ScriptContext::MeshApi { prefix } => complete_mesh_api(&prefix, doc, registry),
        ScriptContext::Refs { prefix } => complete_refs(&prefix, doc),
        ScriptContext::RefMember { ref_name, prefix } => {
            complete_ref_members(&ref_name, &prefix, doc)
        }
        ScriptContext::ElementRefAliasMember { alias, prefix } => {
            complete_element_ref_alias_members(&alias, &prefix, doc)
        }
        ScriptContext::EventCurrentTarget { prefix } => complete_event_current_target(&prefix),
        ScriptContext::ServiceName => complete_service_names(registry),
        ScriptContext::ImportSpecifier { prefix } => complete_import_specifier(&prefix, registry),
        ScriptContext::ImportMember { specifier, prefix } => {
            complete_import_member(&specifier, &prefix, registry)
        }
        ScriptContext::InterfaceProxy { var_name, prefix } => {
            complete_interface_proxy(&var_name, &prefix, doc, registry)
        }
        ScriptContext::ComponentInstanceMember { var_name, prefix } => {
            complete_component_instance_members(&var_name, &prefix, doc, registry)
        }
        ScriptContext::Props { prefix } => complete_props(&prefix, doc),
        ScriptContext::General => complete_state_vars(doc),
    }
}

fn complete_mesh_api(
    prefix: &str,
    _doc: &Document,
    _registry: &ModuleRegistry,
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
    registry: &ModuleRegistry,
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

fn complete_props(prefix: &str, doc: &Document) -> Vec<CompletionItem> {
    let Some(block) = doc.parsed.as_ref().and_then(|parsed| parsed.props.as_ref()) else {
        return vec![];
    };
    let mut items: Vec<CompletionItem> = block
        .props
        .iter()
        .filter(|prop| prop.name.starts_with(prefix))
        .map(|prop| CompletionItem {
            label: prop.name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(prop.ty.lua_type().to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "**`props.{}`**: `{}`\n\nDeclared as `{}` in `<props>`.",
                    prop.name,
                    prop.ty.lua_type(),
                    prop.ty.as_str()
                ),
            })),
            ..Default::default()
        })
        .collect();
    for helper in [
        ("source", "source(name: string): string"),
        ("at", "at(name: string, scope: string): any"),
    ] {
        if helper.0.starts_with(prefix) {
            items.push(CompletionItem {
                label: helper.0.to_string(),
                kind: Some(CompletionItemKind::METHOD),
                detail: Some(helper.1.to_string()),
                insert_text: Some(format!("{}($1)", helper.0)),
                insert_text_format: Some(Fmt::SNIPPET),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value:
                        "Read a specific prop precedence layer. The common path is `props.name`."
                            .to_string(),
                })),
                ..Default::default()
            });
        }
    }
    items
}

/// Completions after `<var>.` where `var` is a `bind:this={var}` component
/// instance. Offers, in order: the base fields inherited from the `MeshElement`
/// type, then the child component's exported (public) variables and functions.
/// Private `local`s and lifecycle hooks are excluded — they do not cross the
/// `bind:this` boundary.
fn complete_component_instance_members(
    var_name: &str,
    prefix: &str,
    doc: &Document,
    registry: &ModuleRegistry,
) -> Vec<CompletionItem> {
    let Some(instance) = doc
        .component_instances
        .iter()
        .find(|instance| instance.var_name == var_name)
    else {
        return vec![];
    };

    let mut items: Vec<CompletionItem> = Vec::new();

    // Tier 1: base fields inherited from the MeshElement type.
    for field in BASE_ELEMENT_FIELDS {
        if field.name.starts_with(prefix) {
            items.push(field_completion_item(field, "MeshElement"));
        }
    }

    // Tier 2: the child component's exported variables and functions.
    if let Some((variables, functions)) =
        resolve_component_public_members(doc, &instance.component_tag, registry)
    {
        for variable in variables {
            if variable.starts_with(prefix) && !items.iter().any(|item| item.label == variable) {
                items.push(component_member_variable_item(
                    &variable,
                    &instance.component_tag,
                ));
            }
        }
        for function in functions {
            if function.starts_with(prefix) && !items.iter().any(|item| item.label == function) {
                items.push(component_member_function_item(
                    &function,
                    &instance.component_tag,
                ));
            }
        }
    }

    items
}

/// Resolve a mounted component tag to the public members exported by its source
/// file. Returns `(variables, functions)`.
pub(crate) fn resolve_component_public_members(
    doc: &Document,
    component_tag: &str,
    registry: &ModuleRegistry,
) -> Option<(Vec<String>, Vec<String>)> {
    let import = doc
        .imports
        .iter()
        .find(|import| import.alias == component_tag)?;
    let path = crate::definition::resolve_import_target(doc, &import.target, registry)?;
    let source = std::fs::read_to_string(path).ok()?;
    Some(crate::document::public_component_members(&source))
}

fn component_member_variable_item(name: &str, component_tag: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(format!("public member of <{component_tag}>")),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**`{component_tag}.{name}`** — public reactive variable exported by the mounted component, read live through `bind:this`."
            ),
        })),
        ..Default::default()
    }
}

fn component_member_function_item(name: &str, component_tag: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::METHOD),
        detail: Some(format!("public function of <{component_tag}>")),
        insert_text: Some(format!("{name}($1)")),
        insert_text_format: Some(Fmt::SNIPPET),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**`{component_tag}.{name}(...)`** — public function exported by the mounted component, called synchronously through `bind:this`."
            ),
        })),
        ..Default::default()
    }
}

fn complete_service_names(registry: &ModuleRegistry) -> Vec<CompletionItem> {
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

/// Builtin `mesh.*` module specifiers resolvable via `require`/`import`,
/// independent of any installed backend interface.
static BUILTIN_IMPORT_SPECIFIERS: &[(&str, &str)] = &[
    ("mesh.i18n", "translation helpers (t)"),
    ("mesh.ui", "UI host API (request_redraw, …)"),
    ("mesh.log", "logging host API (info, warn, error)"),
    ("mesh.locale", "locale host API (current, translate, set)"),
    ("mesh.events", "event bus (subscribe, publish)"),
    ("mesh.popover", "popover host API (activate, hide)"),
];

/// Completion inside the first string argument of `require(...)`/`import(...)`:
/// builtin host-API/library specifiers plus discovered service interfaces.
fn complete_import_specifier(prefix: &str, registry: &ModuleRegistry) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = BUILTIN_IMPORT_SPECIFIERS
        .iter()
        .filter_map(|(spec, detail)| {
            specifier_item(spec, prefix, detail, CompletionItemKind::MODULE)
        })
        .collect();

    for name in registry.service_names() {
        if BUILTIN_IMPORT_SPECIFIERS
            .iter()
            .any(|(spec, _)| *spec == name)
        {
            continue;
        }
        if let Some(item) = specifier_item(
            name,
            prefix,
            "service interface",
            CompletionItemKind::INTERFACE,
        ) {
            items.push(item);
        }
    }

    items
}

/// Completion inside an `import("<specifier>", "<name>...` member argument.
fn complete_import_member(
    specifier: &str,
    prefix: &str,
    registry: &ModuleRegistry,
) -> Vec<CompletionItem> {
    // Strip any version suffix: `mesh.audio@>=1.0` → `mesh.audio`.
    let canonical = match specifier.rsplit_once('@') {
        Some((left, _)) if left.starts_with("mesh.") => left,
        _ => specifier,
    };

    if canonical == "mesh.i18n" {
        return ["t"]
            .into_iter()
            .filter_map(|m| {
                member_item(
                    m,
                    prefix,
                    "i18n.t(key) -> string",
                    CompletionItemKind::METHOD,
                )
            })
            .collect();
    }

    if let Some(namespace) = canonical.strip_prefix("mesh.") {
        if matches!(namespace, "ui" | "log" | "locale" | "events") {
            let dotted = format!("{namespace}.");
            let items: Vec<CompletionItem> = MESH_API_ENTRIES
                .iter()
                .filter_map(|entry| entry.path.strip_prefix(&dotted).map(|m| (m, entry)))
                .filter(|(member, _)| !member.contains('.'))
                .filter_map(|(member, entry)| {
                    member_item(member, prefix, entry.signature, CompletionItemKind::METHOD)
                })
                .collect();
            if !items.is_empty() {
                return items;
            }
        }
        if namespace == "popover" {
            return [
                ("activate", "popover.activate(surface_id, event?, focus?)"),
                ("hide", "popover.hide(surface_id)"),
            ]
            .into_iter()
            .filter_map(|(m, sig)| member_item(m, prefix, sig, CompletionItemKind::METHOD))
            .collect();
        }
    }

    if let Some(shape) = registry.interface_shapes.get(canonical) {
        let mut items: Vec<CompletionItem> = Vec::new();
        for field in &shape.state_fields {
            if let Some(item) = member_item(
                field,
                prefix,
                &format!("{canonical} state"),
                CompletionItemKind::FIELD,
            ) {
                items.push(item);
            }
        }
        for cmd in &shape.commands {
            if let Some(item) = member_item(
                cmd,
                prefix,
                &format!("{canonical} command"),
                CompletionItemKind::METHOD,
            ) {
                items.push(item);
            }
        }
        if let Some(item) = member_item(
            "on_change",
            prefix,
            "register change handler",
            CompletionItemKind::METHOD,
        ) {
            items.push(item);
        }
        return items;
    }

    vec![]
}

/// A module-specifier completion item that inserts only the untyped suffix.
fn specifier_item(
    label: &str,
    prefix: &str,
    detail: &str,
    kind: CompletionItemKind,
) -> Option<CompletionItem> {
    let remaining = label.strip_prefix(prefix)?;
    Some(CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        insert_text: Some(remaining.to_string()),
        insert_text_format: Some(Fmt::PLAIN_TEXT),
        ..Default::default()
    })
}

/// A member-name completion item (used inside `import(...)` quoted name args).
fn member_item(
    member: &str,
    prefix: &str,
    detail: &str,
    kind: CompletionItemKind,
) -> Option<CompletionItem> {
    let remaining = member.strip_prefix(prefix)?;
    Some(CompletionItem {
        label: member.to_string(),
        kind: Some(kind),
        detail: Some(detail.to_string()),
        insert_text: Some(remaining.to_string()),
        insert_text_format: Some(Fmt::PLAIN_TEXT),
        ..Default::default()
    })
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
                    ElementRefSource::BindThis => " bind:this",
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

fn complete_element_ref_alias_members(
    alias: &str,
    prefix: &str,
    doc: &Document,
) -> Vec<CompletionItem> {
    let Some(alias) = doc
        .element_ref_aliases
        .iter()
        .find(|element_alias| element_alias.alias == alias)
    else {
        return vec![];
    };

    match &alias.target {
        ElementRefAliasTarget::Ref(ref_name) => {
            let Some(element_ref) = doc
                .element_refs
                .iter()
                .find(|element_ref| element_ref.name == *ref_name)
            else {
                return vec![];
            };
            complete_element_fields_for_tag(&element_ref.tag, prefix)
        }
        ElementRefAliasTarget::CurrentTarget => complete_element_fields_for_tag("box", prefix),
    }
}

fn complete_event_current_target(prefix: &str) -> Vec<CompletionItem> {
    complete_element_fields_for_tag("box", prefix)
}

fn complete_element_fields_for_tag(tag: &str, prefix: &str) -> Vec<CompletionItem> {
    let type_def = element_type_for_tag(tag);
    let mut fields: Vec<&'static ElementFieldDef> = Vec::new();
    push_fields(&mut fields, BASE_ELEMENT_FIELDS);
    push_fields(&mut fields, type_def.fields);

    let mut items: Vec<CompletionItem> = fields
        .into_iter()
        .filter(|field| field.name.starts_with(prefix))
        .map(|field| field_completion_item(field, type_def.type_name))
        .collect();

    for method in ELEMENT_REF_METHODS {
        if method.name.starts_with(prefix) {
            items.push(element_method_completion_item(method, type_def.type_name));
        }
    }
    if let Some(contract) = element_contract_for_tag(tag) {
        for attribute in contract.attributes {
            if !is_script_member_attribute(attribute.name) {
                continue;
            }
            let alias = attribute_member_name(attribute.name);
            if alias.starts_with(prefix) && !items.iter().any(|item| item.label == alias) {
                items.push(attribute_completion_item(
                    attribute,
                    alias,
                    type_def.type_name,
                ));
            }
        }
    }

    items
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

struct ElementMethodDef {
    name: &'static str,
    signature: &'static str,
    snippet: &'static str,
    description: &'static str,
}

const ELEMENT_REF_METHODS: &[ElementMethodDef] = &[
    ElementMethodDef {
        name: "focus",
        signature: "focus()",
        snippet: "focus()",
        description: "Move focus to this live element.",
    },
    ElementMethodDef {
        name: "blur",
        signature: "blur()",
        snippet: "blur()",
        description: "Remove focus from this live element.",
    },
    ElementMethodDef {
        name: "scroll_into_view",
        signature: "scroll_into_view(options?: table)",
        snippet: "scroll_into_view($1)",
        description: "Scroll the nearest container so this live element becomes visible.",
    },
    ElementMethodDef {
        name: "scroll_to",
        signature: "scroll_to(top: number, left?: number, options?: table)",
        snippet: "scroll_to($1)",
        description: "Scroll this live element to the given offset.",
    },
    ElementMethodDef {
        name: "click",
        signature: "click()",
        snippet: "click()",
        description: "Synthesize a click on this live element.",
    },
    ElementMethodDef {
        name: "set_value",
        signature: "set_value(value: string)",
        snippet: "set_value($1)",
        description: "Set this live input element's text value.",
    },
];

fn element_method_completion_item(method: &ElementMethodDef, element_type: &str) -> CompletionItem {
    CompletionItem {
        label: method.name.to_string(),
        kind: Some(CompletionItemKind::METHOD),
        detail: Some(method.signature.to_string()),
        insert_text: Some(method.snippet.to_string()),
        insert_text_format: Some(Fmt::SNIPPET),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**`{}:{}`**\n\n{}",
                element_type, method.signature, method.description
            ),
        })),
        ..Default::default()
    }
}

fn attribute_completion_item(
    attribute: &ElementAttributeDef,
    label: String,
    element_type: &str,
) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some("element attribute".to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**`{}.{}`**\n\n{}",
                element_type, attribute.name, attribute.description
            ),
        })),
        ..Default::default()
    }
}

pub(crate) fn attribute_member_name(name: &str) -> String {
    match name {
        "class" => return "className".to_string(),
        "ref" => return "ref".to_string(),
        _ => {}
    }

    let mut result = String::new();
    let mut uppercase_next = false;
    for ch in name.chars() {
        if ch == '-' || ch == '_' {
            uppercase_next = true;
        } else if uppercase_next {
            result.push(ch.to_ascii_uppercase());
            uppercase_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

pub(crate) fn is_script_member_attribute(name: &str) -> bool {
    matches!(
        name,
        "id" | "class"
            | "style"
            | "ref"
            | "label"
            | "aria-label"
            | "role"
            | "aria-role"
            | "title"
            | "disabled"
            | "name"
            | "src"
            | "alt"
            | "placeholder"
            | "type"
            | "min"
            | "max"
            | "step"
            | "checked"
            | "selected"
    )
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
        .or_else(|| {
            element_contract_for_tag(tag)?
                .attributes
                .iter()
                .find(|attribute| {
                    is_script_member_attribute(attribute.name)
                        && (attribute.name == field_name
                            || attribute_member_name(attribute.name) == field_name)
                })
                .map(|attribute| {
                    format!(
                        "**`{}.{}`**\n\n{}",
                        type_def.type_name,
                        attribute_member_name(attribute.name),
                        attribute.description
                    )
                })
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
            ElementRefSource::BindThis => "bind:this",
        }
    ))
}
