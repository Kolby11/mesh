use crate::CompiledFrontendModule;

use mesh_core_component::{
    ComponentFile, ComponentImportTarget, PropDef, PropType, PropValue, SourceSpan,
    parse_component, referenced_identifiers,
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

    #[error("failed to parse component source {path} at bytes {span:?}: {source}")]
    ParseSource {
        path: PathBuf,
        span: SourceSpan,
        #[source]
        source: mesh_core_component::ParseError,
    },

    #[error("invalid component source path {path}: {message}")]
    InvalidSourcePath { path: PathBuf, message: String },

    #[error(
        "component import alias '{alias}' in {owner} is declared with multiple targets ({existing} and {incoming})"
    )]
    ConflictingImportAlias {
        alias: String,
        owner: PathBuf,
        existing: String,
        incoming: String,
    },

    #[error("component import cycle detected: {cycle}")]
    ImportCycle { cycle: ImportCyclePath },

    #[error("standalone component validation failed for {path} at bytes {span:?}: {message}")]
    StandaloneComponentViolation {
        path: PathBuf,
        message: String,
        span: SourceSpan,
    },

    #[error("unsupported pseudo-state :{state} in component source {path} at bytes {span:?}")]
    UnsupportedPseudoState {
        path: PathBuf,
        state: String,
        span: SourceSpan,
    },
}

impl CompileFrontendError {
    /// Return the source range associated with a component-level failure.
    /// Filesystem, module-graph, and import-cycle failures intentionally have
    /// no component source range.
    pub fn source_span(&self) -> Option<SourceSpan> {
        match self {
            Self::ParseSource { span, .. }
            | Self::StandaloneComponentViolation { span, .. }
            | Self::UnsupportedPseudoState { span, .. } => Some(*span),
            Self::NotFrontendModule { .. }
            | Self::MissingFrontendEntrypoint { .. }
            | Self::ReadSource { .. }
            | Self::InvalidSourcePath { .. }
            | Self::ConflictingImportAlias { .. }
            | Self::ImportCycle { .. } => None,
        }
    }
}

pub fn is_frontend_module(manifest: &Manifest) -> bool {
    matches!(
        manifest.package.module_type,
        ModuleType::Surface | ModuleType::Widget | ModuleType::Component
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

    compile_frontend_entrypoint(manifest, module_dir, entrypoint)
}

/// Compile a declared frontend `.mesh` entrypoint using the owning module's
/// manifest and import rules.  Besides a module's primary surface entrypoint,
/// the shell uses this for optional module-owned UI such as `settings_ui`.
pub fn compile_frontend_entrypoint(
    manifest: &Manifest,
    module_dir: &Path,
    entrypoint: &str,
) -> Result<CompiledFrontendModule, CompileFrontendError> {
    if !is_frontend_module(manifest) {
        return Err(CompileFrontendError::NotFrontendModule {
            module_id: manifest.package.id.clone(),
        });
    }
    if !entrypoint.ends_with(".mesh") {
        return Err(CompileFrontendError::MissingFrontendEntrypoint {
            module_id: manifest.package.id.clone(),
        });
    }

    let source_path = resolve_module_entrypoint_path(module_dir, entrypoint)?;
    let component = parse_component_file(&source_path)?;
    let mut local_components: HashMap<String, ComponentFile> = HashMap::new();
    let mut module_component_imports = HashMap::new();
    let mut seen_local_paths = HashSet::new();
    let mut parsed_components = HashMap::from([(source_path.clone(), component.clone())]);
    let mut import_bindings = HashMap::new();
    let mut ancestry = vec![source_path.clone()];
    collect_imports(
        &component,
        &source_path,
        module_dir,
        &mut local_components,
        &mut module_component_imports,
        &mut seen_local_paths,
        &mut parsed_components,
        &mut import_bindings,
        &mut ancestry,
    )?;
    validate_standalone_imports(&component, &source_path, module_dir, &local_components)?;
    validate_customizable_slots(manifest, &component, &source_path)?;

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

#[derive(Debug, Clone)]
pub struct ImportCyclePath(Vec<PathBuf>);

impl std::fmt::Display for ImportCyclePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, path) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(" -> ")?;
            }
            write!(f, "{}", path.display())?;
        }
        Ok(())
    }
}

fn validate_customizable_slots(
    manifest: &Manifest,
    component: &ComponentFile,
    source_path: &Path,
) -> Result<(), CompileFrontendError> {
    fn visit<'a>(
        nodes: &'a [TemplateNode],
        out: &mut Vec<&'a mesh_core_component::template::SlotNode>,
    ) {
        for node in nodes {
            match node {
                TemplateNode::Slot(slot) => out.push(slot),
                TemplateNode::Element(node) => visit(&node.children, out),
                TemplateNode::Component(node) => visit(&node.children, out),
                TemplateNode::If(node) => {
                    visit(&node.then_children, out);
                    visit(&node.else_children, out);
                }
                TemplateNode::For(node) => visit(&node.children, out),
                TemplateNode::Text(_) | TemplateNode::Expr(_) => {}
            }
        }
    }

    let mut slots = Vec::new();
    if let Some(template) = &component.template {
        visit(&template.root, &mut slots);
    }
    let mut names = HashSet::new();
    for slot in slots.into_iter().filter(|slot| slot.customizable) {
        let name = slot.name.as_deref().unwrap_or_default();
        if !names.insert(name) {
            return Err(CompileFrontendError::StandaloneComponentViolation {
                path: source_path.to_path_buf(),
                message: format!("duplicate customizable slot name '{name}'"),
                span: slot.span,
            });
        }
        let Some(point) = slot.extension_point.as_deref() else {
            return Err(CompileFrontendError::StandaloneComponentViolation {
                path: source_path.to_path_buf(),
                message: format!("customizable slot '{name}' requires an extension-point"),
                span: slot.span,
            });
        };
        let Some(hosted) = manifest.hosted_extension_points.get(point) else {
            return Err(CompileFrontendError::StandaloneComponentViolation {
                path: source_path.to_path_buf(),
                message: format!(
                    "customizable slot '{name}' hosts undeclared extension point '{point}'"
                ),
                span: slot.span,
            });
        };
        if !hosted.slots.contains_key(name) {
            return Err(CompileFrontendError::StandaloneComponentViolation {
                path: source_path.to_path_buf(),
                message: format!(
                    "customizable slot '{name}' has no mesh.hosts.{point}.slots.{name} declaration"
                ),
                span: slot.span,
            });
        }
    }
    Ok(())
}

fn parse_component_file(path: &Path) -> Result<ComponentFile, CompileFrontendError> {
    let source =
        std::fs::read_to_string(path).map_err(|source| CompileFrontendError::ReadSource {
            path: path.to_path_buf(),
            source,
        })?;
    let component =
        parse_component(&source).map_err(|source| CompileFrontendError::ParseSource {
            path: path.to_path_buf(),
            span: source.span(),
            source,
        })?;
    validate_component_pseudo_states(path, &component)?;
    Ok(component)
}

fn validate_component_pseudo_states(
    path: &Path,
    component: &ComponentFile,
) -> Result<(), CompileFrontendError> {
    fn visit(
        selector: &mesh_core_component::style::Selector,
        path: &Path,
        span: SourceSpan,
    ) -> Result<(), CompileFrontendError> {
        match selector {
            mesh_core_component::style::Selector::State(_, state) => {
                if mesh_core_elements::PseudoState::from_name(state).is_none() {
                    return Err(CompileFrontendError::UnsupportedPseudoState {
                        path: path.to_path_buf(),
                        state: state.clone(),
                        span,
                    });
                }
                Ok(())
            }
            mesh_core_component::style::Selector::Compound(parts) => {
                for part in parts {
                    visit(part, path, span)?;
                }
                Ok(())
            }
            mesh_core_component::style::Selector::Universal
            | mesh_core_component::style::Selector::Tag(_)
            | mesh_core_component::style::Selector::Class(_)
            | mesh_core_component::style::Selector::Id(_) => Ok(()),
        }
    }

    let Some(style) = &component.style else {
        return Ok(());
    };
    for rule in &style.rules {
        visit(&rule.selector, path, style.span)?;
    }
    Ok(())
}

fn collect_imports(
    component: &ComponentFile,
    component_path: &Path,
    module_dir: &Path,
    local_components: &mut HashMap<String, ComponentFile>,
    module_component_imports: &mut HashMap<String, String>,
    seen_local_paths: &mut HashSet<PathBuf>,
    parsed_components: &mut HashMap<PathBuf, ComponentFile>,
    import_bindings: &mut HashMap<(PathBuf, String), ImportBindingTarget>,
    ancestry: &mut Vec<PathBuf>,
) -> Result<(), CompileFrontendError> {
    for import in &component.imports {
        match &import.target {
            ComponentImportTarget::ComponentLocal(source) => {
                let target_path = resolve_local_component_file(source, component_path, module_dir)?;
                insert_import_binding(
                    component_path,
                    &import.alias,
                    ImportBindingTarget::Local(target_path.clone()),
                    import_bindings,
                )?;
                let parsed = if let Some(parsed) = parsed_components.get(&target_path) {
                    parsed.clone()
                } else {
                    let parsed = parse_component_file(&target_path)?;
                    parsed_components.insert(target_path.clone(), parsed.clone());
                    parsed
                };
                insert_local_component(
                    component_path,
                    &import.alias,
                    &target_path,
                    parsed.clone(),
                    local_components,
                );
                if ancestry.iter().any(|path| path == &target_path) {
                    let start = ancestry
                        .iter()
                        .position(|path| path == &target_path)
                        .unwrap_or(0);
                    let mut cycle = ancestry[start..].to_vec();
                    cycle.push(target_path);
                    return Err(CompileFrontendError::ImportCycle {
                        cycle: ImportCyclePath(cycle),
                    });
                }
                if seen_local_paths.insert(target_path.clone()) {
                    ancestry.push(target_path.clone());
                    collect_imports(
                        &parsed,
                        &target_path,
                        module_dir,
                        local_components,
                        module_component_imports,
                        seen_local_paths,
                        parsed_components,
                        import_bindings,
                        ancestry,
                    )?;
                    ancestry.pop();
                }
            }
            ComponentImportTarget::ComponentModule(module_id) => {
                insert_import_binding(
                    component_path,
                    &import.alias,
                    ImportBindingTarget::Module(module_id.clone()),
                    import_bindings,
                )?;
                insert_module_component_import(
                    component_path,
                    &import.alias,
                    module_id,
                    ancestry.len() == 1,
                    module_component_imports,
                )?;
            }
            ComponentImportTarget::InterfaceApi { interface, version } => {
                insert_import_binding(
                    component_path,
                    &import.alias,
                    ImportBindingTarget::Interface {
                        interface: interface.clone(),
                        version: version.clone(),
                    },
                    import_bindings,
                )?;
            }
        }
    }
    Ok(())
}

fn insert_import_binding(
    owner: &Path,
    alias: &str,
    incoming: ImportBindingTarget,
    import_bindings: &mut HashMap<(PathBuf, String), ImportBindingTarget>,
) -> Result<(), CompileFrontendError> {
    let key = (owner.to_path_buf(), alias.to_string());
    if let Some(existing) = import_bindings.get(&key)
        && existing != &incoming
    {
        return Err(CompileFrontendError::ConflictingImportAlias {
            alias: alias.to_string(),
            owner: owner.to_path_buf(),
            existing: existing.describe(),
            incoming: incoming.describe(),
        });
    }
    import_bindings.insert(key, incoming);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ImportBindingTarget {
    Local(PathBuf),
    Module(String),
    Interface {
        interface: String,
        version: Option<String>,
    },
}

impl ImportBindingTarget {
    fn describe(&self) -> String {
        match self {
            Self::Local(path) => format!("local target {}", path.display()),
            Self::Module(module_id) => format!("module target {module_id}"),
            Self::Interface { interface, version } => format!(
                "interface target {}{}",
                interface,
                version
                    .as_deref()
                    .map(|version| format!("@{version}"))
                    .unwrap_or_default()
            ),
        }
    }
}

fn insert_local_component(
    owner: &Path,
    alias: &str,
    target: &Path,
    component: ComponentFile,
    local_components: &mut HashMap<String, ComponentFile>,
) {
    let key = crate::scoped_local_component_key(owner, alias, target);
    local_components.entry(key).or_insert(component);
}

fn insert_module_component_import(
    owner: &Path,
    alias: &str,
    module_id: &str,
    is_root: bool,
    module_component_imports: &mut HashMap<String, String>,
) -> Result<(), CompileFrontendError> {
    if is_root {
        module_component_imports.insert(alias.to_string(), module_id.to_string());
    } else {
        module_component_imports.insert(
            crate::scoped_module_import_key(owner, alias),
            module_id.to_string(),
        );
    }
    Ok(())
}

fn resolve_module_entrypoint_path(
    module_dir: &Path,
    entrypoint: &str,
) -> Result<PathBuf, CompileFrontendError> {
    mesh_core_module::package::resolve_contained_module_file(
        module_dir,
        entrypoint,
        "frontend entrypoint",
    )
    .map_err(|source| CompileFrontendError::InvalidSourcePath {
        path: module_dir.join(entrypoint),
        message: source.to_string(),
    })
}

fn resolve_local_component_file(
    source: &str,
    component_path: &Path,
    module_dir: &Path,
) -> Result<PathBuf, CompileFrontendError> {
    mesh_core_module::package::resolve_contained_component_file(
        module_dir,
        component_path,
        source,
        "local component import",
    )
    .map_err(|error| CompileFrontendError::InvalidSourcePath {
        path: component_path.parent().unwrap_or(module_dir).join(source),
        message: error.to_string(),
    })
}

fn resolve_local_component_path(
    source: &str,
    component_path: &Path,
    module_dir: &Path,
) -> Result<PathBuf, CompileFrontendError> {
    mesh_core_module::package::resolve_contained_component_path(
        module_dir,
        component_path,
        source,
        "local component import",
    )
    .map_err(|error| CompileFrontendError::InvalidSourcePath {
        path: component_path.parent().unwrap_or(module_dir).join(source),
        message: error.to_string(),
    })
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
        true,
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
                    validate_expression(
                        &expr.expression,
                        path,
                        allowed_symbols,
                        loop_locals,
                        expr.expression_span,
                    )?;
                }
            }
            TemplateNode::If(if_node) => {
                if strict_scope {
                    validate_expression(
                        &if_node.condition,
                        path,
                        allowed_symbols,
                        loop_locals,
                        if_node.condition_span,
                    )?;
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
                    validate_expression(
                        &for_node.iterable,
                        path,
                        allowed_symbols,
                        loop_locals,
                        for_node.iterable_span,
                    )?;
                }
                let mut child_loop_locals = loop_locals.clone();
                child_loop_locals.insert(for_node.item_name.clone());
                if strict_scope && let Some(key) = &for_node.key {
                    validate_expression(
                        key,
                        path,
                        allowed_symbols,
                        &child_loop_locals,
                        for_node.key_span.unwrap_or(for_node.span),
                    )?;
                }
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
                    let child_path = resolve_local_component_path(source, path, module_dir)?;
                    let child_component = local_components
                        .get(&crate::scoped_local_component_key(
                            path,
                            &component_ref.name,
                            &child_path,
                        ))
                        .or_else(|| local_components.get(&component_ref.name));
                    let Some(child_component) = child_component else {
                        continue;
                    };
                    validate_child_component_props(component_ref, child_component, path)?;
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

fn validate_child_component_props(
    reference: &mesh_core_component::template::ComponentRef,
    child: &ComponentFile,
    parent_path: &Path,
) -> Result<(), CompileFrontendError> {
    let declarations = child
        .props
        .as_ref()
        .map(|block| {
            block
                .props
                .iter()
                .map(|definition| (definition.name.as_str(), definition))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let public_fields = public_component_fields(child);

    for attribute in &reference.props {
        if attribute.name == "bind:this" {
            continue;
        }

        let Some(definition) = declarations.get(attribute.name.as_str()) else {
            if public_fields.contains(&attribute.name) {
                continue;
            }
            return Err(component_prop_error(
                parent_path,
                &reference.name,
                &attribute.name,
                attribute.span.unwrap_or(reference.span),
                format!(
                    "child component `{}` has no public prop or field named `{}`",
                    reference.name, attribute.name
                ),
            ));
        };

        if !definition.expose {
            return Err(component_prop_error(
                parent_path,
                &reference.name,
                &attribute.name,
                attribute.span.unwrap_or(reference.span),
                format!(
                    "child component `{}` prop `{}` is private (`expose: false`)",
                    reference.name, attribute.name
                ),
            ));
        }

        if let AttributeValue::Static(value) = &attribute.value {
            let parsed = static_prop_value(definition, value).map_err(|message| {
                component_prop_error(
                    parent_path,
                    &reference.name,
                    &attribute.name,
                    attribute.span.unwrap_or(reference.span),
                    message,
                )
            })?;
            mesh_core_component::validate_prop_value(definition, &parsed).map_err(|error| {
                component_prop_error(
                    parent_path,
                    &reference.name,
                    &attribute.name,
                    attribute.span.unwrap_or(reference.span),
                    format!("invalid value for child prop `{}`: {error}", attribute.name),
                )
            })?;
        }
    }
    Ok(())
}

fn public_component_fields(component: &ComponentFile) -> HashSet<String> {
    let mut fields = HashSet::new();
    if let Some(script) = &component.script {
        fields.extend(script.metadata.state_vars.iter().cloned());
        fields.extend(script.metadata.public_functions.iter().cloned());
    }
    fields
}

fn static_prop_value(definition: &PropDef, value: &str) -> Result<PropValue, String> {
    match definition.ty {
        PropType::Bool => match value.trim() {
            "" | "true" | "1" => Ok(PropValue::Bool(true)),
            "false" | "0" => Ok(PropValue::Bool(false)),
            other => Err(format!(
                "child prop `{}` expects a boolean literal, got `{other}`",
                definition.name
            )),
        },
        PropType::Number | PropType::Int => value
            .trim()
            .parse::<f64>()
            .map(PropValue::Number)
            .map_err(|_| {
                format!(
                    "child prop `{}` expects a numeric literal, got `{value}`",
                    definition.name
                )
            }),
        PropType::Duration if value.trim().parse::<f64>().is_ok() => Ok(PropValue::Number(
            value
                .trim()
                .parse::<f64>()
                .expect("checked numeric duration"),
        )),
        _ => Ok(PropValue::String(value.to_string())),
    }
}

fn component_prop_error(
    path: &Path,
    component: &str,
    prop: &str,
    span: SourceSpan,
    message: String,
) -> CompileFrontendError {
    CompileFrontendError::StandaloneComponentViolation {
        path: path.to_path_buf(),
        message: format!("component `{component}` attribute `{prop}`: {message}"),
        span,
    }
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
                validate_expression(
                    expr,
                    path,
                    allowed_symbols,
                    loop_locals,
                    attr.span.unwrap_or_default(),
                )?;
            }
            // bind:this targets a local variable by design — skip public-symbol validation.
            AttributeValue::InstanceBinding(_) => {}
            AttributeValue::EventHandler(handler) => {
                validate_identifier(
                    handler,
                    path,
                    allowed_symbols,
                    loop_locals,
                    attr.span.unwrap_or_default(),
                )?;
            }
            AttributeValue::EventHandlerCall { handler, args } => {
                validate_identifier(
                    handler,
                    path,
                    allowed_symbols,
                    loop_locals,
                    attr.span.unwrap_or_default(),
                )?;
                for arg in args {
                    validate_expression(
                        arg,
                        path,
                        allowed_symbols,
                        loop_locals,
                        attr.span.unwrap_or_default(),
                    )?;
                }
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
    span: SourceSpan,
) -> Result<(), CompileFrontendError> {
    for identifier in referenced_identifiers(expr) {
        if allowed_symbols.contains(&identifier) || loop_locals.contains(&identifier) {
            continue;
        }
        return Err(CompileFrontendError::StandaloneComponentViolation {
            path: path.to_path_buf(),
            message: format!("unknown standalone component symbol `{identifier}` in `{expr}`"),
            span,
        });
    }
    Ok(())
}

fn validate_identifier(
    identifier: &str,
    path: &Path,
    allowed_symbols: &HashSet<String>,
    loop_locals: &HashSet<String>,
    span: SourceSpan,
) -> Result<(), CompileFrontendError> {
    if allowed_symbols.contains(identifier) || loop_locals.contains(identifier) {
        return Ok(());
    }

    Err(CompileFrontendError::StandaloneComponentViolation {
        path: path.to_path_buf(),
        message: format!("unknown standalone component symbol `{identifier}`"),
        span,
    })
}

fn component_allowed_symbols(
    component: &ComponentFile,
    explicit_props: &HashSet<String>,
) -> HashSet<String> {
    let mut allowed = HashSet::from([
        "t".to_string(),
        "this".to_string(),
        "props".to_string(),
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
        allowed.extend(script.metadata.state_vars.iter().cloned());
        allowed.extend(
            script
                .metadata
                .service_bindings
                .iter()
                .map(|(_, local)| local.clone()),
        );
        allowed.extend(script.metadata.public_functions.iter().cloned());
        allowed.extend(script.metadata.required_aliases.iter().cloned());
        allowed.extend(script.metadata.interface_proxies.keys().cloned());
    }

    allowed
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
    fn known_pseudo_states_are_accepted_by_compiler_validation() {
        let source = r#"
<template><box /></template>
<style>
box:hover { opacity: 0.9; }
box:required { opacity: 0.8; }
box:selected { opacity: 0.7; }
box:expanded { opacity: 0.6; }
box:pressed { opacity: 0.5; }
box:invalid { opacity: 0.4; }
box:value { opacity: 0.3; }
</style>
"#;
        let component = component(source);
        validate_component_pseudo_states(Path::new("/tmp/states.mesh"), &component).unwrap();
    }

    #[test]
    fn unknown_pseudo_states_have_a_source_owned_compiler_diagnostic() {
        let source = r#"
<template><box /></template>
<style>box:pressedly { opacity: 0.5; }</style>
"#;
        let component = component(source);
        let error = validate_component_pseudo_states(Path::new("/tmp/states.mesh"), &component)
            .unwrap_err();
        assert!(matches!(
            error,
            CompileFrontendError::UnsupportedPseudoState { ref state, span, .. }
                if state == "pressedly" && span == component.style.as_ref().unwrap().span
        ));
        assert!(error.source_span().is_some());
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
        assert!(err.source_span().is_some_and(|span| span.start < span.end));
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
<props>
theme_icon: { type: "icon" }
</props>
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
    fn child_component_props_validate_publicity_and_static_types() {
        let root = component(
            r#"
<template>
  <Child mode="wrong" />
</template>
<script lang="luau">
import Child from "./child.mesh"
</script>
"#,
        );
        let child = component(
            r#"
<props>
mode: { type: "enum", options: ["compact", "cozy"] }
</props>
<template><box /></template>
"#,
        );

        let error = validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .expect_err("invalid child enum value accepted");
        assert!(
            error
                .to_string()
                .contains("invalid value for child prop `mode`")
        );

        let root = component(
            r#"
<template>
  <Child secret="value" />
</template>
<script lang="luau">
import Child from "./child.mesh"
</script>
"#,
        );
        let child = component(
            r#"
<props>
secret: { type: "string", expose: false }
</props>
<template><box /></template>
"#,
        );
        let error = validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .expect_err("private child prop accepted");
        assert!(error.to_string().contains("is private"));

        let root = component(
            r#"
<template>
  <Child missing="value" />
</template>
<script lang="luau">
import Child from "./child.mesh"
</script>
"#,
        );
        let child = component(r#"<template><box /></template>"#);
        let error = validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .expect_err("unknown child prop accepted");
        assert!(error.to_string().contains("has no public prop or field"));
    }

    #[test]
    fn child_public_script_fields_remain_valid_component_inputs() {
        let root = component(
            r#"
<template>
  <Child title="From parent" />
</template>
<script lang="luau">
import Child from "./child.mesh"
</script>
"#,
        );
        let child = component(
            r#"
<template><text>{title}</text></template>
<script lang="luau">
title = "Default"
</script>
"#,
        );

        validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .expect("public script field rejected as child input");
    }

    #[test]
    fn root_unknown_expression_reports_its_expression_span() {
        let root = component(r#"<template><text>{missing_root}</text></template>"#);
        let expected_span = match &root.template.as_ref().unwrap().root[0] {
            TemplateNode::Element(element) => match &element.children[0] {
                TemplateNode::Expr(expression) => expression.expression_span,
                other => panic!("expected expression child, got {other:?}"),
            },
            other => panic!("expected element root, got {other:?}"),
        };

        let error = validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::new(),
        )
        .expect_err("unknown root expression symbol accepted");

        assert!(error.to_string().contains("missing_root"));
        assert_eq!(error.source_span(), Some(expected_span));
    }

    #[test]
    fn root_expression_scope_allows_props_and_keyed_loop_locals() {
        let root = component(
            r#"
<props>
title: { type: "string", default: "Items" }
</props>
<template>
  <text>{props.title}</text>
  {#for item in items key={item.id}}<text>{item.name}</text>{/for}
</template>
<script lang="luau">
items = {}
</script>
"#,
        );

        validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::new(),
        )
        .expect("root props and keyed loop locals should be in scope");
    }

    #[test]
    fn nested_keyed_loop_expression_uses_the_component_scope() {
        let root = component(
            r#"
<template><Child /></template>
<script lang="luau">
import Child from "./child.mesh"
</script>
"#,
        );
        let child = component(
            r#"
<template>
  {#for item in items key={missing_key}}<text>{item.name}</text>{/for}
</template>
<script lang="luau">
items = {}
</script>
"#,
        );
        let expected_span = match &child.template.as_ref().unwrap().root[0] {
            TemplateNode::For(for_node) => for_node.key_span.unwrap(),
            other => panic!("expected keyed loop root, got {other:?}"),
        };

        let error = validate_standalone_imports(
            &root,
            &path("main.mesh"),
            Path::new("/tmp"),
            &HashMap::from([("Child".to_string(), child)]),
        )
        .expect_err("unknown nested keyed-loop symbol accepted");

        assert!(error.to_string().contains("missing_key"));
        assert_eq!(error.source_span(), Some(expected_span));
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
    fn local_component_source_paths_are_contained_and_canonical() {
        let root = std::env::temp_dir().join(format!(
            "mesh-frontend-component-paths-{}",
            std::process::id()
        ));
        let owner = root.join("src/main.mesh");
        let child = root.join("src/components/child.mesh");
        std::fs::create_dir_all(child.parent().unwrap()).unwrap();
        std::fs::write(&owner, "").unwrap();
        std::fs::write(&child, "").unwrap();

        let resolved = resolve_local_component_file("./components/child", &owner, &root)
            .expect("contained component path");
        assert_eq!(resolved, child.canonicalize().unwrap());

        let error = resolve_local_component_file("/tmp/outside.mesh", &owner, &root)
            .expect_err("absolute component path must fail");
        assert!(matches!(
            error,
            CompileFrontendError::InvalidSourcePath { .. }
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frontend_entrypoint_paths_are_contained_and_canonical() {
        let workspace = tempfile::tempdir().unwrap();
        let directory = workspace.path().join("module");
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("src/main.mesh");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "<template><box /></template>").unwrap();

        let resolved = resolve_module_entrypoint_path(&directory, "src/main.mesh")
            .expect("contained frontend entrypoint");
        assert_eq!(resolved, source.canonicalize().unwrap());

        let outside = workspace.path().join("outside.mesh");
        std::fs::write(&outside, "<template><box /></template>").unwrap();
        for entrypoint in ["../outside.mesh", outside.to_str().unwrap()] {
            assert!(
                resolve_module_entrypoint_path(&directory, entrypoint).is_err(),
                "entrypoint {entrypoint:?} must remain inside the module root"
            );
        }

        #[cfg(unix)]
        {
            let linked = directory.join("linked.mesh");
            std::os::unix::fs::symlink(&outside, &linked).unwrap();
            assert!(
                resolve_module_entrypoint_path(&directory, "linked.mesh").is_err(),
                "symlinked entrypoints must be rejected"
            );
        }
    }

    #[test]
    fn recursive_component_imports_reject_external_paths() {
        let workspace = tempfile::tempdir().unwrap();
        let directory = workspace.path().join("module");
        std::fs::create_dir_all(&directory).unwrap();
        let owner_path = directory.join("src/main.mesh");
        std::fs::create_dir_all(owner_path.parent().unwrap()).unwrap();
        let outside = workspace.path().join("outside.mesh");
        std::fs::write(&outside, "<template><box /></template>").unwrap();

        let assert_rejected = |source: &str| {
            std::fs::write(
                &owner_path,
                format!(
                    "<template><Item /></template><script lang=\"luau\">import Item from \"{source}\"</script>"
                ),
            )
            .unwrap();
            let root = parse_component_file(&owner_path).unwrap();
            let mut local_components = HashMap::new();
            let mut module_component_imports = HashMap::new();
            let mut seen_local_paths = HashSet::new();
            let mut parsed_components = HashMap::from([(owner_path.clone(), root.clone())]);
            let mut import_bindings = HashMap::new();
            let mut ancestry = vec![owner_path.clone()];
            let error = collect_imports(
                &root,
                &owner_path,
                &directory,
                &mut local_components,
                &mut module_component_imports,
                &mut seen_local_paths,
                &mut parsed_components,
                &mut import_bindings,
                &mut ancestry,
            )
            .expect_err("external component import must be rejected");
            assert!(
                matches!(error, CompileFrontendError::InvalidSourcePath { .. }),
                "unexpected error for {source:?}: {error:?}"
            );
        };

        assert_rejected("../../outside.mesh");
        assert_rejected(outside.to_str().unwrap());

        #[cfg(unix)]
        {
            let linked = directory.join("linked.mesh");
            std::os::unix::fs::symlink(&outside, &linked).unwrap();
            assert_rejected("../linked.mesh");
        }
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

    #[test]
    fn recursive_imports_are_resolved_by_canonical_owner_and_alias() {
        let directory = tempfile::tempdir().unwrap();
        let root_path = directory.path().join("main.mesh");
        let branch_a = directory.path().join("branch-a.mesh");
        let branch_b = directory.path().join("branch-b.mesh");
        let item_a = directory.path().join("item-a.mesh");
        let item_b = directory.path().join("item-b.mesh");
        std::fs::write(
            &root_path,
            r#"
<template><BranchA /><BranchB /></template>
<script lang="luau">
import BranchA from "./branch-a.mesh"
import BranchB from "./branch-b.mesh"
</script>
"#,
        )
        .unwrap();
        std::fs::write(
            &branch_a,
            r#"
<template><Item /></template>
<script lang="luau">import Item from "./item-a.mesh"</script>
"#,
        )
        .unwrap();
        std::fs::write(
            &branch_b,
            r#"
<template><Item /></template>
<script lang="luau">import Item from "./item-b.mesh"</script>
"#,
        )
        .unwrap();
        std::fs::write(&item_a, "<template><text>A</text></template>").unwrap();
        std::fs::write(&item_b, "<template><text>B</text></template>").unwrap();

        let root = parse_component_file(&root_path).unwrap();
        let mut local_components = HashMap::new();
        let mut module_component_imports = HashMap::new();
        let mut seen_local_paths = HashSet::new();
        let mut parsed_components = HashMap::from([(root_path.clone(), root.clone())]);
        let mut import_bindings = HashMap::new();
        let mut ancestry = vec![root_path.clone()];
        collect_imports(
            &root,
            &root_path,
            directory.path(),
            &mut local_components,
            &mut module_component_imports,
            &mut seen_local_paths,
            &mut parsed_components,
            &mut import_bindings,
            &mut ancestry,
        )
        .unwrap();

        let root_path = root_path.canonicalize().unwrap();
        let branch_a = branch_a.canonicalize().unwrap();
        let branch_b = branch_b.canonicalize().unwrap();
        let item_a = item_a.canonicalize().unwrap();
        let item_b = item_b.canonicalize().unwrap();
        assert!(
            local_components.contains_key(&crate::scoped_local_component_key(
                &branch_a, "Item", &item_a,
            ))
        );
        assert!(
            local_components.contains_key(&crate::scoped_local_component_key(
                &branch_b, "Item", &item_b,
            ))
        );
        assert!(
            !local_components.contains_key("Item"),
            "recursive aliases must not leak into an unscoped index"
        );
        assert_eq!(seen_local_paths.len(), 4);

        validate_standalone_imports(&root, &root_path, directory.path(), &local_components)
            .unwrap();
    }

    #[test]
    fn recursive_import_cycle_reports_canonical_path_chain() {
        let directory = tempfile::tempdir().unwrap();
        let root_path = directory.path().join("main.mesh");
        let first_path = directory.path().join("first.mesh");
        let second_path = directory.path().join("second.mesh");
        std::fs::write(
            &root_path,
            "<template><First /></template><script lang=\"luau\">import First from \"./first.mesh\"</script>",
        )
        .unwrap();
        std::fs::write(
            &first_path,
            "<template><Second /></template><script lang=\"luau\">import Second from \"./second.mesh\"</script>",
        )
        .unwrap();
        std::fs::write(
            &second_path,
            "<template><First /></template><script lang=\"luau\">import First from \"./first.mesh\"</script>",
        )
        .unwrap();

        let root = parse_component_file(&root_path).unwrap();
        let mut local_components = HashMap::new();
        let mut module_component_imports = HashMap::new();
        let mut seen_local_paths = HashSet::new();
        let mut parsed_components = HashMap::from([(root_path.clone(), root.clone())]);
        let mut import_bindings = HashMap::new();
        let mut ancestry = vec![root_path.clone()];
        let error = collect_imports(
            &root,
            &root_path,
            directory.path(),
            &mut local_components,
            &mut module_component_imports,
            &mut seen_local_paths,
            &mut parsed_components,
            &mut import_bindings,
            &mut ancestry,
        )
        .unwrap_err();

        let first = first_path.canonicalize().unwrap();
        let second = second_path.canonicalize().unwrap();
        assert!(matches!(error, CompileFrontendError::ImportCycle { .. }));
        let message = error.to_string();
        assert!(message.contains(&first.display().to_string()));
        assert!(message.contains(&second.display().to_string()));
    }

    #[test]
    fn import_alias_collision_reports_owner_and_targets() {
        let owner = PathBuf::from("/module/src/owner.mesh");
        let mut bindings = HashMap::new();
        insert_import_binding(
            &owner,
            "Item",
            ImportBindingTarget::Local(PathBuf::from("/module/src/one.mesh")),
            &mut bindings,
        )
        .unwrap();
        let error = insert_import_binding(
            &owner,
            "Item",
            ImportBindingTarget::Module("@mesh/item".into()),
            &mut bindings,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            CompileFrontendError::ConflictingImportAlias { ref alias, .. } if alias == "Item"
        ));
        assert!(error.to_string().contains("owner.mesh"));
        assert!(error.to_string().contains("@mesh/item"));
    }
}
