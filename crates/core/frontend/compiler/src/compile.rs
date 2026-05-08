use crate::CompiledFrontendModule;

use mesh_core_component::{
    ComponentFile, ComponentImportTarget, parse_component,
    template::{Attribute, AttributeValue, TemplateNode},
};
use mesh_core_module::{Manifest, ModuleType};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum CompileFrontendError {
    #[error("module '{module_id}' is not a frontend module")]
    NotFrontendModule { module_id: String },

    #[error("module '{module_id}' is missing a .mesh frontend entrypoint")]
    MissingFrontendEntrypoint { module_id: String },

    #[error("failed to read component source {path}: {source}")]
    ReadSource {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse component source {path}: {source}")]
    ParseSource {
        path: PathBuf,
        #[source]
        source: mesh_core_component::ParseError,
    },

    #[error("component import alias '{alias}' is declared with multiple targets")]
    ConflictingImportAlias { alias: String },

    #[error("standalone component validation failed for {path}: {message}")]
    StandaloneComponentViolation { path: PathBuf, message: String },
}

pub fn is_frontend_module(manifest: &Manifest) -> bool {
    matches!(
        manifest.package.module_type,
        ModuleType::Surface | ModuleType::Widget
    )
}

pub fn compile_frontend_module(
    manifest: &Manifest,
    module_dir: &Path,
) -> Result<CompiledFrontendModule, CompileFrontendError> {
    if !is_frontend_module(manifest) {
        return Err(CompileFrontendError::NotFrontendModule {
            module_id: manifest.package.id.clone(),
        });
    }

    let entrypoint = manifest
        .entrypoints
        .main
        .as_deref()
        .filter(|path| path.ends_with(".mesh"))
        .ok_or_else(|| CompileFrontendError::MissingFrontendEntrypoint {
            module_id: manifest.package.id.clone(),
        })?;

    let source_path = module_dir.join(entrypoint);
    let component = parse_component_file(&source_path)?;
    let mut local_components: HashMap<String, ComponentFile> = HashMap::new();
    let mut module_component_imports = HashMap::new();
    let mut seen_local_paths = HashSet::new();
    collect_imports(
        &component,
        &source_path,
        module_dir,
        &mut local_components,
        &mut module_component_imports,
        &mut seen_local_paths,
    )?;
    validate_standalone_imports(&component, &source_path, module_dir, &local_components)?;

    tracing::info!(
        "compiled frontend module '{}' from {}",
        manifest.package.id,
        source_path.display()
    );

    // The entrypoint plus every locally-imported component's source path —
    // dedup'd via `seen_local_paths`. This is what the hot-reload watcher
    // mtimes so editing any constituent .mesh file triggers a recompile.
    let mut watched_paths = Vec::with_capacity(seen_local_paths.len() + 1);
    watched_paths.push(source_path.clone());
    for path in &seen_local_paths {
        if path != &source_path {
            watched_paths.push(path.clone());
        }
    }

    Ok(CompiledFrontendModule {
        manifest: manifest.clone(),
        source_path,
        component,
        local_components,
        module_component_imports,
        watched_paths,
    })
}

fn parse_component_file(path: &Path) -> Result<ComponentFile, CompileFrontendError> {
    let source =
        std::fs::read_to_string(path).map_err(|source| CompileFrontendError::ReadSource {
            path: path.to_path_buf(),
            source,
        })?;
    parse_component(&source).map_err(|source| CompileFrontendError::ParseSource {
        path: path.to_path_buf(),
        source,
    })
}

fn collect_imports(
    component: &ComponentFile,
    component_path: &Path,
    module_dir: &Path,
    local_components: &mut HashMap<String, ComponentFile>,
    module_component_imports: &mut HashMap<String, String>,
    seen_local_paths: &mut HashSet<PathBuf>,
) -> Result<(), CompileFrontendError> {
    for import in &component.imports {
        match &import.target {
            ComponentImportTarget::ComponentLocal(source) => {
                let target_path = resolve_local_component_path(source, component_path, module_dir);
                let parsed = parse_component_file(&target_path)?;
                insert_local_component(
                    &import.alias,
                    target_path.clone(),
                    parsed.clone(),
                    local_components,
                )?;
                let canonical = target_path.canonicalize().unwrap_or(target_path.clone());
                if seen_local_paths.insert(canonical) {
                    collect_imports(
                        &parsed,
                        &target_path,
                        module_dir,
                        local_components,
                        module_component_imports,
                        seen_local_paths,
                    )?;
                }
            }
            ComponentImportTarget::ComponentModule(module_id) => {
                insert_module_component_import(&import.alias, module_id, module_component_imports)?;
            }
            ComponentImportTarget::InterfaceApi { .. } => {}
        }
    }
    Ok(())
}

fn insert_local_component(
    alias: &str,
    path: PathBuf,
    component: ComponentFile,
    local_components: &mut HashMap<String, ComponentFile>,
) -> Result<(), CompileFrontendError> {
    local_components.insert(alias.to_string(), component);
    tracing::debug!(
        "registered local component import {alias} from {}",
        path.display()
    );
    Ok(())
}

fn insert_module_component_import(
    alias: &str,
    module_id: &str,
    module_component_imports: &mut HashMap<String, String>,
) -> Result<(), CompileFrontendError> {
    if let Some(existing) = module_component_imports.get(alias) {
        if existing != module_id {
            return Err(CompileFrontendError::ConflictingImportAlias {
                alias: alias.to_string(),
            });
        }
    }
    module_component_imports.insert(alias.to_string(), module_id.to_string());
    Ok(())
}

fn resolve_local_component_path(source: &str, component_path: &Path, module_dir: &Path) -> PathBuf {
    let mut path = if let Some(rest) = source.strip_prefix("@src/") {
        module_dir.join("src").join(rest)
    } else if source.starts_with('/') {
        PathBuf::from(source)
    } else {
        component_path.parent().unwrap_or(module_dir).join(source)
    };
    if path.extension().is_none() {
        path.set_extension("mesh");
    }
    path
}

fn validate_standalone_imports(
    root: &ComponentFile,
    root_path: &Path,
    module_dir: &Path,
    local_components: &HashMap<String, ComponentFile>,
) -> Result<(), CompileFrontendError> {
    let mut ancestry = Vec::new();
    validate_component_template(
        root,
        root_path,
        module_dir,
        local_components,
        false,
        &HashSet::new(),
        &mut ancestry,
    )
}

fn validate_component_template(
    component: &ComponentFile,
    path: &Path,
    module_dir: &Path,
    local_components: &HashMap<String, ComponentFile>,
    strict_scope: bool,
    explicit_props: &HashSet<String>,
    ancestry: &mut Vec<PathBuf>,
) -> Result<(), CompileFrontendError> {
    if ancestry.iter().any(|existing| existing == path) {
        return Ok(());
    }
    ancestry.push(path.to_path_buf());

    let allowed_symbols = component_allowed_symbols(component, explicit_props);
    let local_imports = component
        .imports
        .iter()
        .filter_map(|import| match &import.target {
            ComponentImportTarget::ComponentLocal(source) => Some((import.alias.as_str(), source)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();

    if let Some(template) = &component.template {
        validate_template_nodes(
            &template.root,
            path,
            module_dir,
            local_components,
            strict_scope,
            &allowed_symbols,
            &HashSet::new(),
            &local_imports,
            ancestry,
        )?;
    }

    ancestry.pop();
    Ok(())
}

fn validate_template_nodes(
    nodes: &[TemplateNode],
    path: &Path,
    module_dir: &Path,
    local_components: &HashMap<String, ComponentFile>,
    strict_scope: bool,
    allowed_symbols: &HashSet<String>,
    loop_locals: &HashSet<String>,
    local_imports: &HashMap<&str, &String>,
    ancestry: &mut Vec<PathBuf>,
) -> Result<(), CompileFrontendError> {
    for node in nodes {
        match node {
            TemplateNode::Element(element) => {
                if strict_scope {
                    validate_attributes(&element.attributes, path, allowed_symbols, loop_locals)?;
                }
                validate_template_nodes(
                    &element.children,
                    path,
                    module_dir,
                    local_components,
                    strict_scope,
                    allowed_symbols,
                    loop_locals,
                    local_imports,
                    ancestry,
                )?;
            }
            TemplateNode::Text(_) | TemplateNode::Slot(_) => {}
            TemplateNode::Expr(expr) => {
                if strict_scope {
                    validate_expression(&expr.expression, path, allowed_symbols, loop_locals)?;
                }
            }
            TemplateNode::If(if_node) => {
                if strict_scope {
                    validate_expression(&if_node.condition, path, allowed_symbols, loop_locals)?;
                }
                validate_template_nodes(
                    &if_node.then_children,
                    path,
                    module_dir,
                    local_components,
                    strict_scope,
                    allowed_symbols,
                    loop_locals,
                    local_imports,
                    ancestry,
                )?;
                validate_template_nodes(
                    &if_node.else_children,
                    path,
                    module_dir,
                    local_components,
                    strict_scope,
                    allowed_symbols,
                    loop_locals,
                    local_imports,
                    ancestry,
                )?;
            }
            TemplateNode::For(for_node) => {
                if strict_scope {
                    validate_expression(&for_node.iterable, path, allowed_symbols, loop_locals)?;
                }
                let mut child_loop_locals = loop_locals.clone();
                child_loop_locals.insert(for_node.item_name.clone());
                validate_template_nodes(
                    &for_node.children,
                    path,
                    module_dir,
                    local_components,
                    strict_scope,
                    allowed_symbols,
                    &child_loop_locals,
                    local_imports,
                    ancestry,
                )?;
            }
            TemplateNode::Component(component_ref) => {
                if strict_scope {
                    validate_attributes(&component_ref.props, path, allowed_symbols, loop_locals)?;
                }
                validate_template_nodes(
                    &component_ref.children,
                    path,
                    module_dir,
                    local_components,
                    strict_scope,
                    allowed_symbols,
                    loop_locals,
                    local_imports,
                    ancestry,
                )?;

                if let Some(source) = local_imports.get(component_ref.name.as_str()) {
                    let child_path = resolve_local_component_path(source, path, module_dir);
                    let Some(child_component) = local_components.get(&component_ref.name) else {
                        continue;
                    };
                    let explicit_props = component_ref
                        .props
                        .iter()
                        .map(|attr| attr.name.clone())
                        .collect::<HashSet<_>>();
                    validate_component_template(
                        child_component,
                        &child_path,
                        module_dir,
                        local_components,
                        true,
                        &explicit_props,
                        ancestry,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn validate_attributes(
    attrs: &[Attribute],
    path: &Path,
    allowed_symbols: &HashSet<String>,
    loop_locals: &HashSet<String>,
) -> Result<(), CompileFrontendError> {
    for attr in attrs {
        match &attr.value {
            AttributeValue::Binding(expr) | AttributeValue::TwoWayBinding(expr) => {
                validate_expression(expr, path, allowed_symbols, loop_locals)?;
            }
            AttributeValue::EventHandler(handler) => {
                validate_identifier(handler, path, allowed_symbols, loop_locals)?;
            }
            AttributeValue::Static(_) => {}
        }
    }
    Ok(())
}

fn validate_expression(
    expr: &str,
    path: &Path,
    allowed_symbols: &HashSet<String>,
    loop_locals: &HashSet<String>,
) -> Result<(), CompileFrontendError> {
    for identifier in referenced_identifiers(expr) {
        if allowed_symbols.contains(&identifier) || loop_locals.contains(&identifier) {
            continue;
        }
        return Err(CompileFrontendError::StandaloneComponentViolation {
            path: path.to_path_buf(),
            message: format!("unknown standalone component symbol `{identifier}` in `{expr}`"),
        });
    }
    Ok(())
}

fn validate_identifier(
    identifier: &str,
    path: &Path,
    allowed_symbols: &HashSet<String>,
    loop_locals: &HashSet<String>,
) -> Result<(), CompileFrontendError> {
    if allowed_symbols.contains(identifier) || loop_locals.contains(identifier) {
        return Ok(());
    }

    Err(CompileFrontendError::StandaloneComponentViolation {
        path: path.to_path_buf(),
        message: format!("unknown standalone component symbol `{identifier}`"),
    })
}

fn component_allowed_symbols(
    component: &ComponentFile,
    explicit_props: &HashSet<String>,
) -> HashSet<String> {
    let mut allowed = HashSet::from([
        "t".to_string(),
        "refs".to_string(),
        "settings".to_string(),
        "elements".to_string(),
    ]);
    allowed.extend(explicit_props.iter().cloned());

    for import in &component.imports {
        if matches!(import.target, ComponentImportTarget::InterfaceApi { .. }) {
            allowed.insert(import.alias.clone());
        }
    }

    if let Some(script) = &component.script {
        let (state_vars, service_bindings, functions, interface_proxies) =
            extract_script_symbols(&script.source);
        allowed.extend(state_vars);
        allowed.extend(service_bindings);
        allowed.extend(functions);
        allowed.extend(interface_proxies);
    }

    allowed
}

fn extract_script_symbols(source: &str) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let mut state_vars = Vec::new();
    let mut service_bindings = Vec::new();
    let mut functions = Vec::new();
    let mut interface_proxies = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("mesh.state.set(") {
            if let Some(key) = parse_first_string_arg(rest) {
                push_unique(&mut state_vars, key);
            }
        }

        if let Some(rest) = trimmed.strip_prefix("mesh.service.bind(") {
            if let Some((_, local)) = parse_two_string_args(rest) {
                push_unique(&mut service_bindings, local);
            }
        }

        if let Some((_, local)) = parse_proxy_bind_args(trimmed) {
            push_unique(&mut service_bindings, local);
        }

        if let Some(rest) = trimmed.strip_prefix("function ") {
            if let Some(name) = rest.split('(').next() {
                let name = name.trim();
                if is_identifier(name) {
                    push_unique(&mut functions, name.to_string());
                }
            }
        }

        if let Some(rest) = trimmed.strip_prefix("local ") {
            if let Some(req_pos) = rest.find("= require(") {
                let var_name = rest[..req_pos].trim();
                if is_identifier(var_name) {
                    push_unique(&mut interface_proxies, var_name.to_string());
                }
            }

            if let Some(use_pos) = rest.find("= mesh.service.use(") {
                let var_name = rest[..use_pos].trim();
                if is_identifier(var_name) {
                    push_unique(&mut interface_proxies, var_name.to_string());
                }
            }
        }

        if let Some(name) = parse_global_assignment(trimmed) {
            push_unique(&mut state_vars, name);
        }
    }

    (state_vars, service_bindings, functions, interface_proxies)
}

fn parse_global_assignment(line: &str) -> Option<String> {
    if line.is_empty()
        || line.starts_with("--")
        || line.starts_with("local ")
        || line.starts_with("function ")
        || line.starts_with("if ")
        || line.starts_with("elseif ")
        || line.starts_with("for ")
        || line.starts_with("while ")
        || line.starts_with("return ")
    {
        return None;
    }

    let eq = line.find('=')?;
    let lhs = line[..eq].trim_end();
    if lhs.ends_with('>') || lhs.ends_with('<') || lhs.ends_with('~') || lhs.ends_with('=') {
        return None;
    }
    let name = lhs.split_whitespace().next()?;
    if is_identifier(name) {
        Some(name.to_string())
    } else {
        None
    }
}

fn parse_first_string_arg(source: &str) -> Option<String> {
    let s = source.trim_start();
    let quote = s.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn parse_two_string_args(source: &str) -> Option<(String, String)> {
    let first = parse_first_string_arg(source)?;
    let s = source.trim_start();
    let quote = s.chars().next()?;
    let first_quoted_len = 1 + s[1..].find(quote)? + 1;
    let after_first = s.get(first_quoted_len..)?.trim_start_matches([',', ' ']);
    let second = parse_first_string_arg(after_first)?;
    Some((first, second))
}

fn parse_proxy_bind_args(source: &str) -> Option<(String, String)> {
    let bind_pos = source.find(":bind(").or_else(|| source.find(".bind("))?;
    parse_two_string_args(&source[bind_pos + 6..])
}

fn referenced_identifiers(expr: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let bytes = expr.as_bytes();
    let mut i = 0usize;
    let mut prev_non_ws: Option<u8> = None;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'"' || byte == b'\'' {
            let quote = byte;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == quote && bytes.get(i.wrapping_sub(1)) != Some(&b'\\') {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        if is_ident_start(byte) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                i += 1;
            }
            let ident = &expr[start..i];
            let next_non_ws = next_non_ws(bytes, i);
            let is_member = prev_non_ws == Some(b'.');
            let is_keyword = matches!(ident, "and" | "or" | "not" | "true" | "false" | "nil");
            let is_builtin_call = ident == "t" && next_non_ws == Some(b'(');
            if !is_member && !is_keyword && !is_builtin_call && !refs.iter().any(|r| r == ident) {
                refs.push(ident.to_string());
            }
            prev_non_ws = Some(bytes[i - 1]);
            continue;
        }

        if !byte.is_ascii_whitespace() {
            prev_non_ws = Some(byte);
        }
        i += 1;
    }

    refs
}

fn next_non_ws(bytes: &[u8], mut index: usize) -> Option<u8> {
    while index < bytes.len() {
        if !bytes[index].is_ascii_whitespace() {
            return Some(bytes[index]);
        }
        index += 1;
    }
    None
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("/tmp/{name}"))
    }

    fn component(source: &str) -> ComponentFile {
        parse_component(source).unwrap()
    }

    #[test]
    fn imported_component_cannot_read_parent_variable_implicitly() {
        let root = component(
            r#"
<template>
  <Child />
</template>
<script lang="luau">
import Child from "./child.mesh"
theme_icon = "weather-clear"
</script>
"#,
        );
        let child = component(
            r#"
<template>
  <icon name="{theme_icon}" />
</template>
"#,
        );

        let err = validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            CompileFrontendError::StandaloneComponentViolation { .. }
        ));
        assert!(err.to_string().contains("theme_icon"));
    }

    #[test]
    fn imported_component_cannot_read_parent_handler_implicitly() {
        let root = component(
            r#"
<template>
  <Child />
</template>
<script lang="luau">
import Child from "./child.mesh"
function onThemeToggle()
end
</script>
"#,
        );
        let child = component(
            r#"
<template>
  <button onclick={onThemeToggle}>Toggle</button>
</template>
"#,
        );

        let err = validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .unwrap_err();

        assert!(err.to_string().contains("onThemeToggle"));
    }

    #[test]
    fn imported_component_can_use_explicit_props() {
        let root = component(
            r#"
<template>
  <Child theme_icon="{theme_icon}" />
</template>
<script lang="luau">
import Child from "./child.mesh"
theme_icon = "weather-clear"
</script>
"#,
        );
        let child = component(
            r#"
<template>
  <icon name="{theme_icon}" />
</template>
"#,
        );

        validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .unwrap();
    }

    #[test]
    fn imported_component_can_use_translation_builtin() {
        let root = component(
            r#"
<template>
  <Child />
</template>
<script lang="luau">
import Child from "./child.mesh"
</script>
"#,
        );
        let child = component(
            r#"
<template>
  <text>{t("nav.current")}</text>
</template>
"#,
        );

        validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .unwrap();
    }

    #[test]
    fn root_component_keeps_its_own_scope() {
        let root = component(
            r#"
<template>
  <button onclick={onTap}>{label}</button>
</template>
<script lang="luau">
label = "Hello"
function onTap()
end
</script>
"#,
        );

        validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::new(),
        )
        .unwrap();
    }
}
