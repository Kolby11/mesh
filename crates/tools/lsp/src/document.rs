use mesh_core_component::{
    ComponentFile, ComponentImport,
    parser::ParseError,
    parser::parse_component,
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
            Err(err) => (None, Some(err)),
        };

        let (
            state_vars,
            service_bindings,
            script_functions,
            script_symbols,
            interface_proxies,
            element_ref_aliases,
        ) = extract_script_info(&source);

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
    let mut variables: Vec<String> = Vec::new();
    let mut functions: Vec<String> = Vec::new();

    let Some((script_start, script_end)) = block_content_range(source, "script") else {
        return (variables, functions);
    };
    let script = &source[script_start..script_end];

    for line in script.lines() {
        let t = line.trim();

        // Public function: `function Name(...)` — `local function` is private.
        if let Some(rest) = t.strip_prefix("function ") {
            if let Some(name) = rest.split('(').next() {
                let name = name.trim();
                if !name.is_empty()
                    && !name.contains(['.', ':'])
                    && !is_reserved_component_hook(name)
                    && !functions.iter().any(|existing| existing == name)
                {
                    functions.push(name.to_string());
                }
            }
            continue;
        }

        // Public reactive variable: bare `name = value` (rejects `local`,
        // comparisons, and compound LHS — see `parse_public_assignment`).
        if let Some(name) = parse_public_assignment(t) {
            if !variables.contains(&name) {
                variables.push(name);
            }
        }
    }

    (variables, functions)
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
/// bindings from the script block via line-by-line pattern matching.
fn extract_script_info(
    source: &str,
) -> (
    Vec<String>,
    Vec<(String, String)>,
    Vec<String>,
    Vec<ScriptSymbol>,
    HashMap<String, String>,
    Vec<ElementRefAlias>,
) {
    let mut state_vars: Vec<String> = Vec::new();
    let mut service_bindings: Vec<(String, String)> = Vec::new();
    let mut functions: Vec<String> = Vec::new();
    let mut script_symbols: Vec<ScriptSymbol> = Vec::new();
    let mut interface_proxies: HashMap<String, String> = HashMap::new();
    let mut element_ref_aliases: Vec<ElementRefAlias> = Vec::new();

    let Some((script_start, script_end)) = block_content_range(source, "script") else {
        return (
            state_vars,
            service_bindings,
            functions,
            script_symbols,
            interface_proxies,
            element_ref_aliases,
        );
    };
    let script = &source[script_start..script_end];
    let mut line_start = script_start;

    for line in script.lines() {
        let t = line.trim();

        if let Some(rest) = t.strip_prefix("mesh.state.set(") {
            if let Some(key) = parse_first_string_arg(rest) {
                if !state_vars.contains(&key) {
                    state_vars.push(key);
                }
            }
        }

        if let Some(rest) = t.strip_prefix("mesh.service.bind(") {
            if let Some((svc, local)) = parse_two_string_args(rest) {
                service_bindings.push((svc, local));
            }
        }

        if let Some((svc, local)) = parse_proxy_bind_args(t, &interface_proxies) {
            service_bindings.push((svc, local));
        }

        if let Some(rest) = t.strip_prefix("function ") {
            if let Some(name) = rest.split('(').next() {
                let name = name.trim().to_string();
                if !name.is_empty() && !functions.contains(&name) {
                    functions.push(name);
                }
            }
        }

        if let Some((name, span)) = parse_function_definition(line, line_start) {
            push_script_symbol(&mut script_symbols, name, ScriptSymbolKind::Function, span);
        }

        // Bare non-local assignment (`name = value`) is a public reactive member.
        if let Some(name) = parse_public_assignment(t) {
            if !state_vars.contains(&name) {
                state_vars.push(name.clone());
            }
            if let Some(span) = assignment_name_span(line, line_start, &name) {
                push_script_symbol(&mut script_symbols, name, ScriptSymbolKind::Variable, span);
            }
        }

        if let Some(rest) = t.strip_prefix("local ") {
            if rest.contains("= function") || rest.contains("=function") {
                if let Some(name) = rest.split('=').next() {
                    let name = name.trim().to_string();
                    if !name.is_empty() && !functions.contains(&name) {
                        functions.push(name.clone());
                    }
                    if let Some(span) = assignment_name_span(line, line_start, &name) {
                        push_script_symbol(
                            &mut script_symbols,
                            name,
                            ScriptSymbolKind::Function,
                            span,
                        );
                    }
                }
            }

            // Detect: local <var> = require("mesh....")
            //     or: local <var> = import("mesh....")  (default import, no names)
            // A default `import(...)` with no extra arguments resolves to the
            // same proxy table as `require(...)`, so it binds the same shape.
            let binder = ["= require(", "= import("]
                .into_iter()
                .find_map(|kw| rest.find(kw).map(|pos| (kw, pos)));
            if let Some((kw, req_pos)) = binder {
                let var_name = rest[..req_pos].trim().to_string();
                let after_req = &rest[req_pos + kw.len()..];
                // Strip opening quote
                let after_quote = after_req.trim_start_matches(|c| c == '"' || c == '\'');
                // Extract the module string up to closing quote
                let module = after_quote
                    .split(|c| c == '"' || c == '\'')
                    .next()
                    .unwrap_or("");
                // Only a single-argument import maps to one proxy variable;
                // `import("spec", "a", "b")` returns several values, not a proxy.
                let rest_after_module = after_quote
                    .get(module.len()..)
                    .unwrap_or("")
                    .trim_start_matches(|c| c == '"' || c == '\'');
                let is_default_import =
                    kw == "= require(" || !rest_after_module.trim_start().starts_with(',');
                let iface = canonicalize_interface_name(module);
                if is_default_import && !var_name.is_empty() && !iface.is_empty() {
                    interface_proxies.insert(var_name, iface);
                }
            }
        }

        if let Some((alias, target)) = parse_element_ref_alias(t) {
            if !element_ref_aliases
                .iter()
                .any(|existing| existing.alias == alias)
            {
                element_ref_aliases.push(ElementRefAlias { alias, target });
            }
        }

        line_start += line.len() + 1;
    }

    (
        state_vars,
        service_bindings,
        functions,
        script_symbols,
        interface_proxies,
        element_ref_aliases,
    )
}

fn push_script_symbol(
    symbols: &mut Vec<ScriptSymbol>,
    name: String,
    kind: ScriptSymbolKind,
    span: ByteSpan,
) {
    if symbols
        .iter()
        .any(|symbol| symbol.name == name && symbol.kind == kind)
    {
        return;
    }
    symbols.push(ScriptSymbol { name, kind, span });
}

fn parse_function_definition(line: &str, line_start: usize) -> Option<(String, ByteSpan)> {
    let trimmed = line.trim_start();
    let prefix = if trimmed.starts_with("local function ") {
        "local function "
    } else if trimmed.starts_with("function ") {
        "function "
    } else {
        return None;
    };

    let name = trimmed
        .strip_prefix(prefix)?
        .split('(')
        .next()?
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }

    let column = line.find(&name)?;
    Some((
        name.clone(),
        ByteSpan {
            start: line_start + column,
            end: line_start + column + name.len(),
        },
    ))
}

fn assignment_name_span(line: &str, line_start: usize, name: &str) -> Option<ByteSpan> {
    let column = line.find(name)?;
    Some(ByteSpan {
        start: line_start + column,
        end: line_start + column + name.len(),
    })
}

fn parse_element_ref_alias(line: &str) -> Option<(String, ElementRefAliasTarget)> {
    let line = line.strip_prefix("local ").unwrap_or(line);
    let (alias, value) = line.split_once('=')?;
    let alias = alias.trim();
    if alias.is_empty()
        || !alias
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }

    let value = value.trim();
    if value.starts_with("event.current_target") {
        return Some((alias.to_string(), ElementRefAliasTarget::CurrentTarget));
    }

    let rest = value.strip_prefix("refs.")?;
    let ref_name: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    (!ref_name.is_empty()).then(|| (alias.to_string(), ElementRefAliasTarget::Ref(ref_name)))
}

/// Convert a require module string to a canonical interface name.
/// "mesh.audio@>=1.0" → "mesh.audio", "mesh.audio" → "mesh.audio"
fn canonicalize_interface_name(module: &str) -> String {
    // Strip version suffix
    let module = if let Some((left, _)) = module.rsplit_once('@') {
        if left.starts_with("mesh.") {
            left
        } else {
            module
        }
    } else {
        module
    };
    if module.starts_with("mesh.") {
        module.to_string()
    } else {
        String::new()
    }
}

/// Extract the raw text content inside `<block_name>...</block_name>`.
pub fn extract_block_text<'a>(source: &'a str, block_name: &str) -> &'a str {
    let open = format!("<{}", block_name);
    let close = format!("</{}>", block_name);

    let Some(tag_start) = source.find(&open) else {
        return "";
    };
    let after_open = &source[tag_start..];
    let Some(close_angle) = after_open.find('>') else {
        return "";
    };
    let content_start = tag_start + close_angle + 1;
    let Some(close_pos) = source[content_start..].find(&close) else {
        return "";
    };
    &source[content_start..content_start + close_pos]
}

/// Extract the byte range `[start, end)` of a block's content in `source`.
pub fn block_content_range(source: &str, block_name: &str) -> Option<(usize, usize)> {
    let open = format!("<{}", block_name);
    let close = format!("</{}>", block_name);

    let tag_start = source.find(&open)?;
    let after_open = &source[tag_start..];
    let close_angle = after_open.find('>')?;
    let content_start = tag_start + close_angle + 1;
    let close_pos = source[content_start..].find(&close)?;
    Some((content_start, content_start + close_pos))
}

/// Detects a bare public assignment `name = value` (a public reactive script
/// member). Returns the member name. Rejects `local`/keyword lines, comparisons
/// (`==`, `~=`, `<=`, `>=`), compound LHS (`t.x`, `t:m`, `t[i]`), and multi-assign.
fn parse_public_assignment(line: &str) -> Option<String> {
    let line = line.trim();
    let eq = line.find('=')?;
    // Reject comparison / inequality operators.
    let before = line.as_bytes()[eq.checked_sub(1)?];
    let after = line.as_bytes().get(eq + 1).copied();
    if matches!(before, b'=' | b'~' | b'<' | b'>' | b'!') || after == Some(b'=') {
        return None;
    }

    let name = line[..eq].trim();
    if name.is_empty() {
        return None;
    }
    // LHS must be a single plain identifier.
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if is_lua_keyword(name) {
        return None;
    }
    Some(name.to_string())
}

fn is_lua_keyword(word: &str) -> bool {
    matches!(
        word,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

fn parse_first_string_arg(s: &str) -> Option<String> {
    let s = s.trim_start();
    let quote = if s.starts_with('"') {
        '"'
    } else if s.starts_with('\'') {
        '\''
    } else {
        return None;
    };
    let inner = &s[1..];
    let end = inner.find(quote)?;
    Some(inner[..end].to_string())
}

fn parse_two_string_args(s: &str) -> Option<(String, String)> {
    let first = parse_first_string_arg(s)?;
    // Advance past the first quoted string + comma
    let s = s.trim_start();
    let first_quoted_len = first.len() + 2; // quotes
    let after_first = s.get(first_quoted_len..)?.trim_start_matches([',', ' ']);
    let second = parse_first_string_arg(after_first)?;
    Some((first, second))
}

fn parse_proxy_bind_args(
    line: &str,
    interface_proxies: &HashMap<String, String>,
) -> Option<(String, String)> {
    let bind_pos = line.find(":bind(").or_else(|| line.find(".bind("))?;
    let proxy_expr = line[..bind_pos].trim();
    let proxy_name = proxy_expr
        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .find(|segment| !segment.is_empty())?;
    let iface = interface_proxies.get(proxy_name)?;
    let service = iface.strip_prefix("mesh.").unwrap_or(iface).to_string();
    let (_, local) = parse_two_string_args(&line[bind_pos + 6..])?;
    Some((service, local))
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
