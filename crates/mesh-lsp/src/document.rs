use mesh_component::{
    ComponentFile, ComponentImport,
    parser::ParseError,
    parser::parse_component,
    template::{AttributeValue, TemplateNode},
};
use mesh_elements::element_type_for_tag;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Clone)]
pub struct ElementRef {
    pub name: String,
    pub tag: String,
    pub element_type: String,
    pub source: ElementRefSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementRefSource {
    Ref,
    Id,
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
    /// Template element bindings exposed to Luau through `refs.<name>`.
    pub element_refs: Vec<ElementRef>,
}

impl Document {
    pub fn new(uri: Url, source: String) -> Self {
        let (parsed, parse_error) = match parse_component(&source) {
            Ok(file) => (Some(file), None),
            Err(err) => (None, Some(err)),
        };

        let (state_vars, service_bindings, script_functions) = extract_script_info(&source);

        let element_refs = parsed
            .as_ref()
            .map(extract_element_refs)
            .unwrap_or_default();
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
            element_refs,
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

/// Extract state vars, service bindings, and function names from the script block via
/// line-by-line pattern matching (no Luau AST required).
fn extract_script_info(source: &str) -> (Vec<String>, Vec<(String, String)>, Vec<String>) {
    let mut state_vars: Vec<String> = Vec::new();
    let mut service_bindings: Vec<(String, String)> = Vec::new();
    let mut functions: Vec<String> = Vec::new();

    let script = extract_block_text(source, "script");

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

        if let Some(rest) = t.strip_prefix("function ") {
            if let Some(name) = rest.split('(').next() {
                let name = name.trim().to_string();
                if !name.is_empty() && !functions.contains(&name) {
                    functions.push(name);
                }
            }
        }

        if let Some(rest) = t.strip_prefix("local ") {
            if rest.contains("= function") || rest.contains("=function") {
                if let Some(name) = rest.split('=').next() {
                    let name = name.trim().to_string();
                    if !name.is_empty() && !functions.contains(&name) {
                        functions.push(name);
                    }
                }
            }
        }
    }

    (state_vars, service_bindings, functions)
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
}
