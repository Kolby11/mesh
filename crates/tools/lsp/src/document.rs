use mesh_core_component::{
    ComponentFile, ComponentImport, ScriptAliasTarget,
    ScriptMemberAccess as ComponentScriptMemberAccess,
    ScriptSymbolKind as ComponentScriptSymbolKind,
    parser::{ParseError, parse_component, parse_luau_script},
    template::{AttributeValue, TemplateNode},
};
use mesh_core_elements::element_type_for_tag;
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Clone)]
pub struct ElementRef {
    pub name: String,
    pub tag: String,
    pub element_type: String,
    pub source: ElementRefSource,
}

#[derive(Debug, Clone)]
pub struct ElementRefAlias {
    pub alias: String,
    pub target: ElementRefAliasTarget,
}

#[derive(Debug, Clone)]
pub enum ElementRefAliasTarget {
    Ref(String),
    CurrentTarget,
}

/// A child component mounted with `bind:this={var}`. The bound variable is a
/// live reference to the child instance, so `var.<member>` should complete the
/// child's exported members (see `public_component_members`).
#[derive(Debug, Clone)]
pub struct ComponentInstance {
    /// Local variable name from `bind:this={var_name}`.
    pub var_name: String,
    /// PascalCase component tag it is mounted from; matches an import alias.
    pub component_tag: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementRefSource {
    Ref,
    Id,
    BindThis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptSymbolKind {
    Function,
    Variable,
}

#[derive(Debug, Clone)]
pub struct ScriptSymbol {
    pub name: String,
    pub kind: ScriptSymbolKind,
    pub span: ByteSpan,
}

#[derive(Debug, Clone)]
pub struct ScriptMemberAccess {
    pub path: Vec<String>,
    pub spans: Vec<ByteSpan>,
}

pub struct Document {
    pub uri: Url,
    pub source: String,
    pub parsed: Option<ComponentFile>,
    pub parse_error: Option<ParseError>,
    /// State variables declared via `mesh.state.set("key", ...)`.
    pub state_vars: Vec<String>,
    /// Bindings from `mesh.service.bind("svc.field", "local_name")`.
    pub service_bindings: Vec<(String, String)>,
    /// Explicit imports parsed from the `.mesh` script block.
    pub imports: Vec<ComponentImport>,
    /// Top-level and local function names found in the script block.
    pub script_functions: Vec<String>,
    /// Script-local symbols that support navigation.
    pub script_symbols: Vec<ScriptSymbol>,
    /// Dotted Luau member paths collected by the parser, with absolute source
    /// spans for syntax-aware diagnostics.
    pub script_member_accesses: Vec<ScriptMemberAccess>,
    /// Template element bindings exposed to Luau through `refs.<name>`.
    pub element_refs: Vec<ElementRef>,
    /// Lua variables assigned from `refs.<name>`, e.g. `local panel = refs.panel`.
    pub element_ref_aliases: Vec<ElementRefAlias>,
    /// Local variables bound to interface proxies via `require("mesh....")`.
    /// Maps variable name → canonical interface name (e.g. "audio" → "mesh.audio").
    pub interface_proxies: HashMap<String, String>,
    /// Child component instances mounted with `bind:this={var}`.
    pub component_instances: Vec<ComponentInstance>,
}

impl Document {
    pub fn new(uri: Url, source: String) -> Self {
        let (parsed, parse_error) = match parse_component(&source) {
            Ok(file) => (Some(file), None),
            Err(err) => (
                mesh_core_component::parser::parse_component_for_tooling(&source).ok(),
                Some(err),
            ),
        };

        let (
            state_vars,
            service_bindings,
            script_functions,
            script_symbols,
            script_member_accesses,
            interface_proxies,
            element_ref_aliases,
        ) = extract_script_info(&source, parsed.as_ref());

        let element_refs = parsed
            .as_ref()
            .map(extract_element_refs)
            .unwrap_or_default();
        let component_instances = parsed
            .as_ref()
            .map(extract_component_instances)
            .unwrap_or_default();
        let mut element_ref_aliases = element_ref_aliases;
        for element_ref in &element_refs {
            if element_ref.source == ElementRefSource::BindThis
                && !element_ref_aliases
                    .iter()
                    .any(|alias| alias.alias == element_ref.name)
            {
                element_ref_aliases.push(ElementRefAlias {
                    alias: element_ref.name.clone(),
                    target: ElementRefAliasTarget::Ref(element_ref.name.clone()),
                });
            }
        }
        let imports = parsed
            .as_ref()
            .map(|parsed| parsed.imports.clone())
            .unwrap_or_default();

        Self {
            uri,
            source,
            parsed,
            parse_error,
            state_vars,
            service_bindings,
            imports,
            script_functions,
            script_symbols,
            script_member_accesses,
            element_refs,
            element_ref_aliases,
            interface_proxies,
            component_instances,
        }
    }
}

fn extract_element_refs(parsed: &ComponentFile) -> Vec<ElementRef> {
    let mut refs = Vec::new();
    let Some(template) = &parsed.template else {
        return refs;
    };

    for node in &template.root {
        collect_element_refs(node, &mut refs);
    }

    refs
}

fn collect_element_refs(node: &TemplateNode, refs: &mut Vec<ElementRef>) {
    match node {
        TemplateNode::Element(element) => {
            let tag = element.tag.as_str();
            if let Some(name) = static_attr_value(node, "ref") {
                push_element_ref(refs, name, tag, ElementRefSource::Ref);
            }
            if let Some(name) = static_attr_value(node, "id") {
                push_element_ref(refs, name, tag, ElementRefSource::Id);
            }
            if let Some(name) = instance_binding_value(element, "bind:this") {
                push_element_ref(refs, name, tag, ElementRefSource::BindThis);
            }
            for child in &element.children {
                collect_element_refs(child, refs);
            }
        }
        TemplateNode::If(if_node) => {
            for child in &if_node.then_children {
                collect_element_refs(child, refs);
            }
            for child in &if_node.else_children {
                collect_element_refs(child, refs);
            }
        }
        TemplateNode::For(for_node) => {
            for child in &for_node.children {
                collect_element_refs(child, refs);
            }
        }
        TemplateNode::Component(component) => {
            for child in &component.children {
                collect_element_refs(child, refs);
            }
        }
        TemplateNode::Slot(_) | TemplateNode::Text(_) | TemplateNode::Expr(_) => {}
    }
}

fn extract_component_instances(parsed: &ComponentFile) -> Vec<ComponentInstance> {
    let mut instances = Vec::new();
    let Some(template) = &parsed.template else {
        return instances;
    };

    for node in &template.root {
        collect_component_instances(node, &mut instances);
    }

    instances
}

fn collect_component_instances(node: &TemplateNode, instances: &mut Vec<ComponentInstance>) {
    match node {
        TemplateNode::Component(component) => {
            if let Some(var_name) = component_instance_binding(&component.props) {
                if !instances
                    .iter()
                    .any(|existing| existing.var_name == var_name)
                {
                    instances.push(ComponentInstance {
                        var_name,
                        component_tag: component.name.clone(),
                    });
                }
            }
            for child in &component.children {
                collect_component_instances(child, instances);
            }
        }
        TemplateNode::Element(element) => {
            for child in &element.children {
                collect_component_instances(child, instances);
            }
        }
        TemplateNode::If(if_node) => {
            for child in &if_node.then_children {
                collect_component_instances(child, instances);
            }
            for child in &if_node.else_children {
                collect_component_instances(child, instances);
            }
        }
        TemplateNode::For(for_node) => {
            for child in &for_node.children {
                collect_component_instances(child, instances);
            }
        }
        TemplateNode::Slot(_) | TemplateNode::Text(_) | TemplateNode::Expr(_) => {}
    }
}

/// The `bind:this={var}` instance binding on a component's props, if any.
fn component_instance_binding(
    props: &[mesh_core_component::template::Attribute],
) -> Option<String> {
    props.iter().find_map(|attr| {
        if attr.name != "bind:this" {
            return None;
        }
        match &attr.value {
            AttributeValue::InstanceBinding(value) if !value.is_empty() => Some(value.clone()),
            _ => None,
        }
    })
}

/// The public members a child component exposes across a `bind:this` boundary:
/// bare-assigned reactive variables and top-level `function` definitions. `local`
/// privates and lifecycle hooks are excluded, mirroring the runtime rule in
/// `ScriptContext::public_function_names` / `install_live_binding`. Returns
/// `(variables, functions)`.
pub fn public_component_members(source: &str) -> (Vec<String>, Vec<String>) {
    let Some((script_start, script_end)) = block_content_range(source, "script") else {
        return (Vec::new(), Vec::new());
    };

    let Some(script) = parse_luau_script(&source[script_start..script_end]).ok() else {
        return (Vec::new(), Vec::new());
    };
    (
        script.metadata.state_vars,
        script
            .metadata
            .public_functions
            .into_iter()
            .filter(|name| !is_reserved_component_hook(name))
            .collect(),
    )
}

/// Lifecycle hooks that stay host-private and do not cross `bind:this`.
/// Mirrors `is_lifecycle_handler` in the scripting runtime.
fn is_reserved_component_hook(name: &str) -> bool {
    matches!(name, "init" | "render" | "mount" | "unmount" | "onRender")
}

fn instance_binding_value(
    element: &mesh_core_component::template::ElementNode,
    attr_name: &str,
) -> Option<String> {
    element.attributes.iter().find_map(|attr| {
        if attr.name != attr_name {
            return None;
        }
        match &attr.value {
            AttributeValue::InstanceBinding(value) if !value.is_empty() => Some(value.clone()),
            _ => None,
        }
    })
}

fn static_attr_value(node: &TemplateNode, attr_name: &str) -> Option<String> {
    let TemplateNode::Element(element) = node else {
        return None;
    };

    element.attributes.iter().find_map(|attr| {
        if attr.name != attr_name {
            return None;
        }
        match &attr.value {
            AttributeValue::Static(value) if !value.is_empty() => Some(value.clone()),
            _ => None,
        }
    })
}

fn push_element_ref(refs: &mut Vec<ElementRef>, name: String, tag: &str, source: ElementRefSource) {
    if refs.iter().any(|existing| existing.name == name) {
        return;
    }
    refs.push(ElementRef {
        name,
        tag: tag.to_string(),
        element_type: element_type_for_tag(tag).type_name.to_string(),
        source,
    });
}

/// Extract state vars, service bindings, function names, and interface proxy
/// bindings from parser-owned Luau metadata.
fn extract_script_info(
    source: &str,
    parsed: Option<&ComponentFile>,
) -> (
    Vec<String>,
    Vec<(String, String)>,
    Vec<String>,
    Vec<ScriptSymbol>,
    Vec<ScriptMemberAccess>,
    HashMap<String, String>,
    Vec<ElementRefAlias>,
) {
    let script_range = parsed
        .and_then(|file| {
            file.blocks
                .iter()
                .find(|block| block.name == "script")
                .map(|block| (block.content.start, block.content.end))
        })
        .or_else(|| block_content_range(source, "script"));
    let Some((script_start, script_end)) = script_range else {
        return (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            HashMap::new(),
            Vec::new(),
        );
    };
    let script = parsed
        .and_then(|file| file.script.as_ref())
        .cloned()
        .or_else(|| parse_luau_script(&source[script_start..script_end]).ok());
    let Some(script) = script else {
        return (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            HashMap::new(),
            Vec::new(),
        );
    };
    let metadata = script.metadata;

    let symbols = metadata
        .symbols
        .into_iter()
        .map(|symbol| ScriptSymbol {
            name: symbol.name,
            kind: match symbol.kind {
                ComponentScriptSymbolKind::Function => ScriptSymbolKind::Function,
                ComponentScriptSymbolKind::Variable => ScriptSymbolKind::Variable,
            },
            span: ByteSpan {
                start: script_start + symbol.span.start,
                end: script_start + symbol.span.end,
            },
        })
        .collect();
    let aliases = metadata
        .element_ref_aliases
        .into_iter()
        .map(|alias| ElementRefAlias {
            alias: alias.alias,
            target: match alias.target {
                ScriptAliasTarget::Ref(name) => ElementRefAliasTarget::Ref(name),
                ScriptAliasTarget::CurrentTarget => ElementRefAliasTarget::CurrentTarget,
            },
        })
        .collect();
    let member_accesses = metadata
        .member_accesses
        .into_iter()
        .map(|access: ComponentScriptMemberAccess| ScriptMemberAccess {
            path: access.path,
            spans: access
                .spans
                .into_iter()
                .map(|span| ByteSpan {
                    start: script_start + span.start,
                    end: script_start + span.end,
                })
                .collect(),
        })
        .collect();

    (
        metadata.state_vars,
        metadata.service_bindings,
        metadata.functions,
        symbols,
        member_accesses,
        metadata.interface_proxies,
        aliases,
    )
}

/// Extract the raw text content inside `<block_name>...</block_name>`.
pub fn extract_block_text<'a>(source: &'a str, block_name: &str) -> &'a str {
    block_content_range(source, block_name)
        .map(|(start, end)| &source[start..end])
        .unwrap_or_default()
}

/// Extract the byte range `[start, end)` of a block's content in `source`.
pub fn block_content_range(source: &str, block_name: &str) -> Option<(usize, usize)> {
    let open = format!("<{}", block_name);
    let close = format!("</{}>", block_name);

    let tag_start = source.find(&open)?;
    let after_open = &source[tag_start..];
    let close_angle = after_open.find('>')?;
    let content_start = tag_start + close_angle + 1;
    let close_pos = source[content_start..]
        .find(&close)
        .unwrap_or_else(|| source.len().saturating_sub(content_start));
    Some((content_start, content_start + close_pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_element_refs_from_template() {
        let source = r#"
<template>
  <button ref="batteryButton">
    <icon ref="batteryIcon" name="battery-full" />
  </button>
</template>
"#;
        let doc = Document::new(
            Url::parse("file:///tmp/battery-button.mesh").unwrap(),
            source.to_string(),
        );

        assert_eq!(doc.element_refs.len(), 2);
        assert_eq!(doc.element_refs[0].name, "batteryButton");
        assert_eq!(doc.element_refs[0].element_type, "ButtonElement");
        assert_eq!(doc.element_refs[1].name, "batteryIcon");
        assert_eq!(doc.element_refs[1].element_type, "IconElement");
    }

    #[test]
    fn extracts_service_bindings_from_require_proxy() {
        let source = r#"
<script lang="luau">
local theme = require("mesh.theme")
theme:bind("is_dark", "theme_is_dark")
</script>
"#;
        let doc = Document::new(
            Url::parse("file:///tmp/theme-button.mesh").unwrap(),
            source.to_string(),
        );

        assert!(
            doc.service_bindings
                .contains(&("theme".to_string(), "theme_is_dark".to_string()))
        );
        assert_eq!(
            doc.interface_proxies.get("theme").map(String::as_str),
            Some("mesh.theme")
        );
    }
}
