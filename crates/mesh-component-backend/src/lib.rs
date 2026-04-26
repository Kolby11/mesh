use mesh_component::template::{
    Attribute, AttributeValue, ComponentRef, ElementNode, TemplateNode,
};
use mesh_component::{AccessibilityRole, ComponentFile, parse_component};
use mesh_plugin::{Manifest, PluginType};
use mesh_theme::Theme;
use mesh_ui::accessibility::AccessibilityInfo;
use mesh_ui::style::{Display, FlexDirection};
use mesh_ui::{
    ComputedStyle, Dimension, LayoutEngine, StyleContext, StyleResolver, VariableStore, WidgetNode,
};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A `VariableStore` overlay used during `{#for}` iteration.
/// Shadows one variable name with the current loop item value while
/// delegating everything else to the underlying store.
struct LayeredStore<'a> {
    base: &'a dyn VariableStore,
    item_name: &'a str,
    item_value: serde_json::Value,
}

impl VariableStore for LayeredStore<'_> {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        if name == self.item_name {
            Some(self.item_value.clone())
        } else {
            self.base.get(name)
        }
    }

    fn keys(&self) -> Vec<String> {
        let mut keys = self.base.keys();
        if !keys.iter().any(|k| k == self.item_name) {
            keys.push(self.item_name.to_string());
        }
        keys
    }

    fn translate(&self, key: &str) -> Option<String> {
        self.base.translate(key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendRenderMode {
    Surface,
    Embedded,
}

pub trait FrontendCompositionResolver {
    fn render_import(
        &self,
        host: &Manifest,
        host_instance_key: &str,
        alias: &str,
        props: &HashMap<String, String>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode>;

    fn render_slot(
        &self,
        host: &Manifest,
        host_instance_key: &str,
        slot_name: Option<&str>,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode>;
}

#[derive(Debug, Clone)]
pub struct CompiledFrontendPlugin {
    pub manifest: Manifest,
    pub source_path: PathBuf,
    pub component: ComponentFile,
    /// Local single-file components shipped in `src/components/*.mesh` inside
    /// the plugin directory. Keyed by filename stem (e.g. `settings-button`).
    pub local_components: std::collections::HashMap<String, mesh_component::ComponentFile>,
}

impl CompiledFrontendPlugin {
    pub fn surface_id(&self) -> &str {
        &self.manifest.package.id
    }

    pub fn build_preview_tree(&self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
        self.build_preview_tree_with_state(theme, width, height, None)
    }

    pub fn build_preview_tree_with_state(
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        state: Option<&dyn VariableStore>,
    ) -> WidgetNode {
        self.build_tree_with_state(
            theme,
            width,
            height,
            state,
            FrontendRenderMode::Surface,
            &self.manifest.package.id,
            None,
            None,
        )
    }

    pub fn build_tree_with_state(
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        state: Option<&dyn VariableStore>,
        mode: FrontendRenderMode,
        instance_key: &str,
        composition: Option<&dyn FrontendCompositionResolver>,
        measurer: Option<&dyn mesh_ui::TextMeasurer>,
    ) -> WidgetNode {
        let mut root = WidgetNode::new("surface");
        root.attributes
            .insert("id".into(), self.manifest.package.id.clone());
        root.computed_style = match mode {
            FrontendRenderMode::Surface => surface_style(&self.manifest.package.id, width, height),
            FrontendRenderMode::Embedded => embedded_root_style(),
        };
        let manifest_has_role = self
            .manifest
            .accessibility
            .as_ref()
            .and_then(|accessibility| accessibility.role.as_ref())
            .is_some();
        if let Some(accessibility) = &self.manifest.accessibility {
            if let Some(role) = accessibility.role.as_deref() {
                root.accessibility.role = parse_accessibility_role(role);
            }
            root.accessibility.label = accessibility
                .label
                .clone()
                .or_else(|| self.manifest.package.name.clone());
            root.accessibility.description = accessibility
                .description
                .clone()
                .or_else(|| self.manifest.package.description.clone());
        }
        if let Some(meta) = &self.component.meta {
            if let Some(role) = &meta.role {
                if !manifest_has_role {
                    root.accessibility.role = role.clone();
                }
            }
            if root.accessibility.label.is_none() {
                root.accessibility.label = meta.label.clone();
            }
            if root.accessibility.description.is_none() {
                root.accessibility.description = meta.description.clone();
            }
        }

        let resolver = StyleResolver::new(theme);
        let rules = self
            .component
            .style
            .as_ref()
            .map(|style| style.rules.as_slice())
            .unwrap_or(&[]);

        if let Some(template) = &self.component.template {
            let root_context = child_style_context(
                &root.computed_style,
                StyleContext {
                    container_width: width as f32,
                    container_height: height as f32,
                },
            );
            root.children = template
                .root
                .iter()
                .map(|node| {
                    build_widget_node(
                        node,
                        &self.manifest,
                        rules,
                        &resolver,
                        Some(&root.computed_style),
                        root_context,
                        state,
                        instance_key,
                        composition,
                    )
                })
                .collect();
        }

        LayoutEngine::compute_with_measurer(&mut root, width as f32, height as f32, measurer);
        root
    }

    pub fn referenced_component_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        if let Some(template) = &self.component.template {
            collect_component_tags(&template.root, &mut tags);
        }
        tags.sort();
        tags.dedup();
        tags
    }
}

fn collect_component_tags(nodes: &[TemplateNode], tags: &mut Vec<String>) {
    for node in nodes {
        match node {
            TemplateNode::Component(component) => {
                tags.push(component.name.clone());
                collect_component_tags(&component.children, tags);
            }
            TemplateNode::Element(element) => collect_component_tags(&element.children, tags),
            TemplateNode::If(if_node) => {
                collect_component_tags(&if_node.then_children, tags);
                collect_component_tags(&if_node.else_children, tags);
            }
            TemplateNode::For(for_node) => collect_component_tags(&for_node.children, tags),
            TemplateNode::Text(_) | TemplateNode::Expr(_) | TemplateNode::Slot(_) => {}
        }
    }
}

/// Build a WidgetNode subtree from a parsed local ComponentFile.
/// This is a public helper so other crates (core) can render local
/// component templates without duplicating the template->widget logic.
///
/// `host_rules` are the parent plugin's CSS rules. They are merged before the
/// component's own rules so that parent-defined classes (e.g. `.battery-widget`)
/// apply inside child component templates as intended.
pub fn build_widget_tree_from_component(
    component: &mesh_component::ComponentFile,
    host_manifest: &Manifest,
    theme: &Theme,
    container_width: f32,
    container_height: f32,
    composition: Option<&dyn FrontendCompositionResolver>,
    instance_key: &str,
    state: Option<&dyn VariableStore>,
    host_rules: &[mesh_component::style::StyleRule],
) -> WidgetNode {
    let resolver = StyleResolver::new(theme);
    let component_rules: &[mesh_component::style::StyleRule] = component
        .style
        .as_ref()
        .map(|s| s.rules.as_slice())
        .unwrap_or(&[]);
    let merged: Vec<mesh_component::style::StyleRule>;
    let rules: &[mesh_component::style::StyleRule] = if host_rules.is_empty() {
        component_rules
    } else if component_rules.is_empty() {
        host_rules
    } else {
        merged = host_rules.iter().chain(component_rules.iter()).cloned().collect();
        &merged
    };

    if let Some(template) = &component.template {
        let child_context = StyleContext {
            container_width,
            container_height,
        };
        let children: Vec<WidgetNode> = template
            .root
            .iter()
            .map(|node| {
                build_widget_node(
                    node,
                    host_manifest,
                    rules,
                    &resolver,
                    None,
                    child_context,
                    state,
                    instance_key,
                    composition,
                )
            })
            .collect();
        let mut container = WidgetNode::new("box");
        container.children = children;
        container
    } else {
        WidgetNode::new("box")
    }
}

fn parse_accessibility_role(role: &str) -> AccessibilityRole {
    match role.trim().to_ascii_lowercase().as_str() {
        "button" => AccessibilityRole::Button,
        "slider" => AccessibilityRole::Slider,
        "label" => AccessibilityRole::Label,
        "text-input" | "textinput" | "text_input" => AccessibilityRole::TextInput,
        "checkbox" => AccessibilityRole::Checkbox,
        "switch" => AccessibilityRole::Switch,
        "region" => AccessibilityRole::Region,
        "list" => AccessibilityRole::List,
        "list-item" | "listitem" | "list_item" => AccessibilityRole::ListItem,
        "image" => AccessibilityRole::Image,
        "toolbar" => AccessibilityRole::Toolbar,
        "menu" => AccessibilityRole::Menu,
        "menu-item" | "menuitem" | "menu_item" => AccessibilityRole::MenuItem,
        "dialog" => AccessibilityRole::Dialog,
        "alert" => AccessibilityRole::Alert,
        "status" => AccessibilityRole::Status,
        "progress-bar" | "progressbar" | "progress_bar" => AccessibilityRole::ProgressBar,
        "tab" => AccessibilityRole::Tab,
        "tab-panel" | "tabpanel" | "tab_panel" => AccessibilityRole::TabPanel,
        "separator" => AccessibilityRole::Separator,
        custom => AccessibilityRole::Custom(custom.to_string()),
    }
}

pub fn root_accessibility_role(manifest: &Manifest, component: &ComponentFile) -> Option<String> {
    manifest
        .accessibility
        .as_ref()
        .and_then(|accessibility| accessibility.role.clone())
        .or_else(|| {
            component
                .meta
                .as_ref()
                .and_then(|meta| meta.role.as_ref().map(ToString::to_string))
        })
}

#[derive(Debug, thiserror::Error)]
pub enum CompileFrontendError {
    #[error("plugin '{plugin_id}' is not a frontend plugin")]
    NotFrontendPlugin { plugin_id: String },

    #[error("plugin '{plugin_id}' is missing a .mesh frontend entrypoint")]
    MissingMeshEntrypoint { plugin_id: String },

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
        source: mesh_component::ParseError,
    },
}

pub fn is_frontend_plugin(manifest: &Manifest) -> bool {
    matches!(
        manifest.package.plugin_type,
        PluginType::Surface | PluginType::Widget
    )
}

pub fn compile_frontend_plugin(
    manifest: &Manifest,
    plugin_dir: &Path,
) -> Result<CompiledFrontendPlugin, CompileFrontendError> {
    if !is_frontend_plugin(manifest) {
        return Err(CompileFrontendError::NotFrontendPlugin {
            plugin_id: manifest.package.id.clone(),
        });
    }

    let entrypoint = manifest
        .entrypoints
        .main
        .as_deref()
        .filter(|path| path.ends_with(".mesh"))
        .ok_or_else(|| CompileFrontendError::MissingMeshEntrypoint {
            plugin_id: manifest.package.id.clone(),
        })?;

    let source_path = plugin_dir.join(entrypoint);
    let source = std::fs::read_to_string(&source_path).map_err(|source| {
        CompileFrontendError::ReadSource {
            path: source_path.clone(),
            source,
        }
    })?;
    let mut component =
        parse_component(&source).map_err(|source| CompileFrontendError::ParseSource {
            path: source_path.clone(),
            source,
        })?;

    // Scan for local component files in `src/components/*.mesh` and parse them.
    let mut local_components: std::collections::HashMap<String, mesh_component::ComponentFile> =
        std::collections::HashMap::new();
    let components_dir = plugin_dir.join("src").join("components");
    if components_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&components_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("mesh") {
                    if let Ok(src) = std::fs::read_to_string(&path) {
                        match parse_component(&src) {
                            Ok(comp) => {
                                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                    // register by kebab-case stem
                                    local_components.insert(stem.to_string(), comp.clone());
                                    // also register PascalCase variant so <MyWidget/>
                                    // resolves when file is `my-widget.mesh`.
                                    let pascal = stem
                                        .split('-')
                                        .filter(|p| !p.is_empty())
                                        .map(|p| {
                                            let mut c = p.chars();
                                            match c.next() {
                                                Some(first) => {
                                                    first.to_ascii_uppercase().to_string()
                                                        + c.as_str()
                                                }
                                                None => String::new(),
                                            }
                                        })
                                        .collect::<String>();
                                    if !pascal.is_empty() {
                                        local_components.insert(pascal.clone(), comp.clone());
                                    }

                                    // Expose these local components implicitly by adding
                                    // import entries mapping alias -> host plugin id so
                                    // templates can reference them directly.
                                    component
                                        .imports
                                        .entry(stem.to_string())
                                        .or_insert_with(|| manifest.package.id.clone());
                                    if !pascal.is_empty() {
                                        component
                                            .imports
                                            .entry(pascal)
                                            .or_insert_with(|| manifest.package.id.clone());
                                    }
                                }
                            }
                            Err(err) => {
                                tracing::warn!(
                                    "plugin '{}': failed to parse local component {}: {}",
                                    manifest.package.id,
                                    path.display(),
                                    err
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Post-process imports: values starting with '@' are treated as
    // relative paths inside the plugin directory. Parse those files and
    // register them as local components keyed by the import alias so the
    // template may reference them using the alias name.
    let mut imports_to_fix: Vec<String> = Vec::new();
    for (alias, target) in component.imports.iter() {
        if target.starts_with('@') {
            imports_to_fix.push(alias.clone());
        }
    }

    for alias in imports_to_fix {
        if let Some(target) = component.imports.get(&alias) {
            let rel = target.trim_start_matches('@');
            let mut candidate = plugin_dir.join(rel);
            if candidate.extension().is_none() {
                candidate.set_extension("mesh");
            }
            if let Ok(src) = std::fs::read_to_string(&candidate) {
                match parse_component(&src) {
                    Ok(comp) => {
                        // register by alias so render_import can find it
                        local_components.insert(alias.clone(), comp);
                        // make scripts see the host plugin id under this alias
                        // so existing code that seeds aliases keeps working.
                        // (alias -> host plugin id)
                        // Note: mutate the component.imports map accordingly.
                        // We need a mutable reference; recreate a new map.
                    }
                    Err(err) => tracing::warn!(
                        "plugin '{}': failed to parse imported local component {}: {}",
                        manifest.package.id,
                        candidate.display(),
                        err
                    ),
                }
            }
        }
    }

    // Replace any import values that were local paths with the host plugin id
    // so the runtime alias seeding remains compatible with existing behavior.
    for (_alias, value) in component.imports.iter_mut() {
        if value.starts_with('@') {
            *value = manifest.package.id.clone();
        }
    }

    tracing::info!(
        "compiled frontend plugin '{}' from {}",
        manifest.package.id,
        source_path.display()
    );

    Ok(CompiledFrontendPlugin {
        manifest: manifest.clone(),
        source_path,
        component,
        local_components,
    })
}

fn build_widget_node(
    node: &TemplateNode,
    manifest: &Manifest,
    rules: &[mesh_component::style::StyleRule],
    resolver: &StyleResolver<'_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> WidgetNode {
    match node {
        TemplateNode::Element(element) => build_element_node(
            element,
            manifest,
            rules,
            resolver,
            parent_style,
            container_context,
            state,
            instance_key,
            composition,
        ),
        TemplateNode::Component(component) => build_component_ref(
            component,
            manifest,
            rules,
            resolver,
            parent_style,
            container_context,
            state,
            instance_key,
            composition,
        ),
        TemplateNode::Text(text) => {
            let mut node = WidgetNode::new("text");
            node.attributes
                .insert("content".into(), text.content.clone());
            node.computed_style = text_style();
            if let Some(parent_style) = parent_style {
                inherit_text_style(
                    &mut node.computed_style,
                    parent_style,
                    InheritedStyleMask::default(),
                );
            }
            node
        }
        TemplateNode::Expr(expr) => {
            let mut node = WidgetNode::new("text");
            let content = state
                .map(|store| eval_expr(&expr.expression, store))
                .unwrap_or_else(|| format!("{{ {} }}", expr.expression));
            node.attributes.insert("content".into(), content);
            node.computed_style = text_style();
            if let Some(parent_style) = parent_style {
                inherit_text_style(
                    &mut node.computed_style,
                    parent_style,
                    InheritedStyleMask::default(),
                );
            }
            node
        }
        TemplateNode::If(if_node) => {
            let show_then = match state {
                Some(store) => {
                    let result = eval_expr(&if_node.condition, store);
                    !matches!(result.as_str(), "false" | "nil" | "" | "0")
                }
                None => true,
            };
            let active_children = if show_then {
                &if_node.then_children
            } else {
                &if_node.else_children
            };
            let mut node = WidgetNode::new("column");
            node.computed_style = container_style("column");
            let child_context = child_style_context(&node.computed_style, container_context);
            node.children = active_children
                .iter()
                .map(|child| {
                    build_widget_node(
                        child,
                        manifest,
                        rules,
                        resolver,
                        Some(&node.computed_style),
                        child_context,
                        state,
                        instance_key,
                        composition,
                    )
                })
                .collect();
            if let Some(parent_style) = parent_style {
                inherit_text_style(
                    &mut node.computed_style,
                    parent_style,
                    InheritedStyleMask::default(),
                );
            }
            node
        }
        TemplateNode::For(for_node) => {
            let mut node = WidgetNode::new("column");
            node.computed_style = container_style("column");
            let child_context = child_style_context(&node.computed_style, container_context);

            if let Some(store) = state {
                if let Some(serde_json::Value::Array(items)) = store.get(&for_node.iterable) {
                    for item_val in items {
                        let item_store = LayeredStore {
                            base: store,
                            item_name: &for_node.item_name,
                            item_value: item_val,
                        };
                        for child in &for_node.children {
                            node.children.push(build_widget_node(
                                child,
                                manifest,
                                rules,
                                resolver,
                                Some(&node.computed_style),
                                child_context,
                                Some(&item_store as &dyn VariableStore),
                                instance_key,
                                composition,
                            ));
                        }
                    }
                }
            }

            if let Some(parent_style) = parent_style {
                inherit_text_style(
                    &mut node.computed_style,
                    parent_style,
                    InheritedStyleMask::default(),
                );
            }
            node
        }
        TemplateNode::Slot(slot) => {
            let slot_definition = slot
                .name
                .as_ref()
                .and_then(|name| manifest.provides_slots.get(name));
            let layout = slot_definition
                .and_then(|definition| definition.layout.as_deref())
                .unwrap_or("row");
            let tag = match layout {
                "column" => "column",
                "stack" => "box",
                _ => "row",
            };

            let mut node = WidgetNode::new(tag);
            node.attributes.insert(
                "slot".into(),
                slot.name.clone().unwrap_or_else(|| "default".into()),
            );
            node.computed_style = slot_style(tag);
            let child_context = child_style_context(&node.computed_style, container_context);
            if let Some(composition) = composition {
                let mut children = composition.render_slot(
                    manifest,
                    instance_key,
                    slot.name.as_deref(),
                    child_context.container_width,
                    child_context.container_height,
                );
                if let Some(max) = slot_definition.and_then(|definition| definition.max) {
                    children.truncate(max as usize);
                }
                node.children = children;
            }
            if let Some(parent_style) = parent_style {
                inherit_text_style(
                    &mut node.computed_style,
                    parent_style,
                    InheritedStyleMask::default(),
                );
            }
            node
        }
    }
}

fn build_element_node(
    element: &ElementNode,
    manifest: &Manifest,
    rules: &[mesh_component::style::StyleRule],
    resolver: &StyleResolver<'_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> WidgetNode {
    let tag = normalize_tag(&element.tag);
    let (classes, id, attributes, event_handlers) = parse_attributes(&element.attributes, state);
    let inherited_mask =
        inherited_style_mask(rules, &tag, &classes, id.as_deref(), container_context);

    let mut node = WidgetNode::new(tag.clone());
    node.attributes = attributes;
    node.event_handlers = event_handlers;
    node.computed_style = resolver.resolve_node_style(
        rules,
        &tag,
        &classes,
        id.as_deref(),
        container_context,
        Default::default(),
    );
    merge_missing_defaults(&tag, &mut node.computed_style);
    if let Some(parent_style) = parent_style {
        inherit_text_style(&mut node.computed_style, parent_style, inherited_mask);
    }
    node.accessibility = accessibility_for_tag(&tag);

    if let Some(id) = id {
        node.attributes.insert("id".into(), id);
    }
    if !classes.is_empty() {
        node.attributes.insert("class".into(), classes.join(" "));
    }

    // Inline text element: fold all Text/Expr children into a single "content" attribute
    // instead of building child nodes. This makes <span>{x}</span> and <p>Hello</p> work
    // as leaf text nodes so the painter can render them directly.
    if tag == "text"
        && !element.children.is_empty()
        && element.children.iter().all(is_inline_template_node)
    {
        let content: String = element
            .children
            .iter()
            .map(|c| resolve_inline_content(c, state))
            .collect();
        node.attributes.insert("content".into(), content);
        return node;
    }

    let child_context = child_style_context(&node.computed_style, container_context);
    node.children = element
        .children
        .iter()
        .map(|child| {
            build_widget_node(
                child,
                manifest,
                rules,
                resolver,
                Some(&node.computed_style),
                child_context,
                state,
                instance_key,
                composition,
            )
        })
        .collect();

    node
}

fn is_inline_template_node(node: &TemplateNode) -> bool {
    matches!(node, TemplateNode::Text(_) | TemplateNode::Expr(_))
}

fn resolve_inline_content(node: &TemplateNode, state: Option<&dyn VariableStore>) -> String {
    match node {
        TemplateNode::Text(t) => t.content.clone(),
        TemplateNode::Expr(e) => state
            .map(|store| eval_expr(&e.expression, store))
            .unwrap_or_else(|| format!("{{ {} }}", e.expression)),
        _ => String::new(),
    }
}

fn build_component_ref(
    component: &ComponentRef,
    manifest: &Manifest,
    rules: &[mesh_component::style::StyleRule],
    resolver: &StyleResolver<'_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    host_instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> WidgetNode {
    let (_, _, props, _) = parse_attributes(&component.props, state);
    if let Some(composition) = composition {
        if let Some(node) = composition.render_import(
            manifest,
            host_instance_key,
            &component.name,
            &props,
            container_context.container_width,
            container_context.container_height,
        ) {
            return node;
        }
    }

    let fake_element = ElementNode {
        tag: "box".into(),
        attributes: component.props.clone(),
        children: component.children.clone(),
    };
    let mut node = build_element_node(
        &fake_element,
        manifest,
        rules,
        resolver,
        parent_style,
        container_context,
        state,
        host_instance_key,
        composition,
    );
    node.attributes
        .insert("component".into(), component.name.clone());
    node
}

#[derive(Clone, Copy, Default)]
struct InheritedStyleMask {
    color: bool,
    font_family: bool,
    font_size: bool,
    font_weight: bool,
    line_height: bool,
}

fn inherit_text_style(
    style: &mut ComputedStyle,
    parent_style: &ComputedStyle,
    explicit: InheritedStyleMask,
) {
    if !explicit.color {
        style.color = parent_style.color;
    }
    if !explicit.font_family {
        style.font_family = parent_style.font_family.clone();
    }
    if !explicit.font_size {
        style.font_size = parent_style.font_size;
    }
    if !explicit.font_weight {
        style.font_weight = parent_style.font_weight;
    }
    if !explicit.line_height {
        style.line_height = parent_style.line_height;
    }
}

fn inherited_style_mask(
    rules: &[mesh_component::style::StyleRule],
    tag: &str,
    classes: &[String],
    id: Option<&str>,
    context: StyleContext,
) -> InheritedStyleMask {
    let mut mask = InheritedStyleMask::default();

    for rule in rules {
        if !selector_matches(&rule.selector, tag, classes, id)
            || rule.container_query.is_some_and(|query| {
                !query.matches(context.container_width, context.container_height)
            })
        {
            continue;
        }

        for decl in &rule.declarations {
            match decl.property.as_str() {
                "color" => mask.color = true,
                "font-family" => mask.font_family = true,
                "font-size" => mask.font_size = true,
                "font-weight" => mask.font_weight = true,
                "line-height" => mask.line_height = true,
                _ => {}
            }
        }
    }

    mask
}

fn selector_matches(
    selector: &mesh_component::style::Selector,
    tag: &str,
    classes: &[String],
    id: Option<&str>,
) -> bool {
    use mesh_component::style::Selector;

    match selector {
        Selector::Universal => true,
        Selector::Tag(t) => t == tag,
        Selector::Class(c) => classes.iter().any(|cls| cls == c),
        Selector::Id(i) => id == Some(i.as_str()),
        Selector::State(t, _state) => t == "*" || t == tag,
        Selector::Compound(parts) => parts
            .iter()
            .all(|part| selector_matches(part, tag, classes, id)),
    }
}

fn child_style_context(style: &ComputedStyle, parent_context: StyleContext) -> StyleContext {
    let width = (resolve_dimension_for_context(style.width, parent_context.container_width)
        - style.margin.horizontal())
    .max(0.0);
    let height = (resolve_dimension_for_context(style.height, parent_context.container_height)
        - style.margin.vertical())
    .max(0.0);

    StyleContext {
        container_width: (width - style.padding.horizontal()).max(0.0),
        container_height: (height - style.padding.vertical()).max(0.0),
    }
}

fn resolve_dimension_for_context(dimension: Dimension, available: f32) -> f32 {
    match dimension {
        Dimension::Px(px) => px,
        Dimension::Percent(percent) => available * percent / 100.0,
        Dimension::Auto | Dimension::Content => available.max(0.0),
    }
}

fn parse_attributes(
    attrs: &[Attribute],
    state: Option<&dyn VariableStore>,
) -> (
    Vec<String>,
    Option<String>,
    HashMap<String, String>,
    HashMap<String, String>,
) {
    let mut classes = Vec::new();
    let mut id = None;
    let mut resolved = HashMap::new();
    let mut event_handlers = HashMap::new();

    for attr in attrs {
        match &attr.value {
            AttributeValue::Static(value) => {
                if attr.name == "class" {
                    classes.extend(value.split_whitespace().map(str::to_string));
                } else if attr.name == "id" {
                    id = Some(value.clone());
                } else {
                    resolved.insert(attr.name.clone(), value.clone());
                }
            }
            AttributeValue::Binding(binding) | AttributeValue::TwoWayBinding(binding) => {
                let value = state
                    .map(|store| eval_expr(binding, store))
                    .unwrap_or_default();
                resolved.insert(attr.name.clone(), value);
            }
            AttributeValue::EventHandler(handler) => {
                event_handlers.insert(normalize_event_handler_name(&attr.name), handler.clone());
            }
        }
    }

    (classes, id, resolved, event_handlers)
}

fn normalize_event_handler_name(name: &str) -> String {
    name.strip_prefix("on").unwrap_or(name).to_string()
}

fn json_value_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    }
}

/// Evaluate a template expression against the current variable store.
///
/// Supports a subset of Luau expression syntax:
/// - `"string literal"` / `'string literal'`
/// - `not x` — boolean negation
/// - `cond and a or b` — ternary (Lua idiom)
/// - `x == y`, `x ~= y`, `x > y`, `x >= y`, `x < y`, `x <= y` — comparisons
/// - `x .. y` — string concatenation
/// - `t("key")` / `t(variable)` — translation
/// - `variable` / `a.b.c` — variable lookup
fn eval_expr(expr: &str, store: &dyn mesh_ui::VariableStore) -> String {
    let expr = expr.trim();

    // Parenthesized: (expr)
    if expr.starts_with('(') && expr.ends_with(')') && balanced_parens(expr) {
        return eval_expr(&expr[1..expr.len() - 1], store);
    }

    // #expr — Luau length operator: returns array/string/object length.
    if let Some(inner) = expr.strip_prefix('#') {
        let inner = inner.trim();
        return match store.get(inner) {
            Some(serde_json::Value::Array(arr)) => arr.len().to_string(),
            Some(serde_json::Value::String(s)) => s.len().to_string(),
            Some(serde_json::Value::Object(obj)) => obj.len().to_string(),
            _ => "0".into(),
        };
    }

    // not X — boolean negation
    if let Some(inner) = expr.strip_prefix("not ") {
        let value = eval_expr(inner.trim(), store);
        let is_truthy = !matches!(value.as_str(), "false" | "nil" | "" | "0");
        return if is_truthy {
            "false".into()
        } else {
            "true".into()
        };
    }

    // Ternary: cond and then_val or else_val
    // Boolean and: cond and expr  (no trailing 'or')
    if let Some((lhs, rest)) = split_op(expr, " and ") {
        if let Some((then_val, else_val)) = split_op(rest, " or ") {
            // Ternary idiom: A and B or C
            let cond_result = eval_expr(lhs, store);
            let truthy = !matches!(cond_result.as_str(), "false" | "nil" | "" | "0");
            return if truthy {
                eval_expr(then_val, store)
            } else {
                eval_expr(else_val, store)
            };
        }
        // Plain boolean AND: A and B
        let l = eval_expr(lhs, store);
        if matches!(l.as_str(), "false" | "nil" | "" | "0") {
            return "false".into();
        }
        let r = eval_expr(rest, store);
        return if matches!(r.as_str(), "false" | "nil" | "" | "0") {
            "false".into()
        } else {
            "true".into()
        };
    }

    // Boolean or: A or B
    if let Some((lhs, rhs)) = split_op(expr, " or ") {
        let l = eval_expr(lhs, store);
        if !matches!(l.as_str(), "false" | "nil" | "" | "0") {
            return "true".into();
        }
        let r = eval_expr(rhs, store);
        return if matches!(r.as_str(), "false" | "nil" | "" | "0") {
            "false".into()
        } else {
            "true".into()
        };
    }

    // Comparison operators (check multi-char first to avoid partial match)
    for op in &["~=", "==", ">=", "<=", ">", "<"] {
        if let Some((lhs, rhs)) = split_op(expr, op) {
            let l = eval_expr(lhs, store);
            let r = eval_expr(rhs, store);
            let result = if let (Ok(ln), Ok(rn)) = (l.parse::<f64>(), r.parse::<f64>()) {
                match *op {
                    "==" => (ln - rn).abs() < f64::EPSILON,
                    "~=" => (ln - rn).abs() >= f64::EPSILON,
                    ">=" => ln >= rn,
                    "<=" => ln <= rn,
                    ">" => ln > rn,
                    "<" => ln < rn,
                    _ => false,
                }
            } else {
                match *op {
                    "==" => l == r,
                    "~=" => l != r,
                    _ => false,
                }
            };
            return if result {
                "true".into()
            } else {
                "false".into()
            };
        }
    }

    // String concatenation: x .. y
    if let Some((lhs, rhs)) = split_op(expr, " .. ") {
        let l = eval_expr(lhs, store);
        let r = eval_expr(rhs, store);
        return format!("{l}{r}");
    }

    // t(...) — translation call
    if let Some(arg) = expr.strip_prefix("t(").and_then(|s| s.strip_suffix(')')) {
        let arg = arg.trim();
        if let Some(key) = strip_string_literal(arg) {
            return store.translate(&key).unwrap_or(key);
        }
        let resolved = eval_path(arg, store);
        return store.translate(&resolved).unwrap_or(resolved);
    }

    // String literal: "..." or '...'
    if let Some(s) = strip_string_literal(expr) {
        return s;
    }

    // Plain variable or dotted path
    eval_path(expr, store)
}

/// Split `expr` on the first occurrence of `op` (not inside parens or quotes).
fn split_op<'a>(expr: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut quote = b'"';
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == quote && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
            i += 1;
            continue;
        }
        if b == b'(' {
            depth += 1;
            i += 1;
            continue;
        }
        if b == b')' {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 && expr[i..].starts_with(op) {
            return Some((&expr[..i], &expr[i + op.len()..]));
        }
        i += 1;
    }
    None
}

/// Returns true if an expression's outer parens are balanced (matching).
fn balanced_parens(expr: &str) -> bool {
    let mut depth = 0i32;
    for (i, b) in expr.bytes().enumerate() {
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 && i < expr.len() - 1 {
                return false;
            }
        }
    }
    depth == 0
}

/// Resolve a variable name or dotted path from the store.
fn eval_path(expr: &str, store: &dyn mesh_ui::VariableStore) -> String {
    // Try exact key first (covers both flat and pre-serialized dotted keys)
    if let Some(value) = store.get(expr) {
        return json_value_to_string(value);
    }

    // Try dotted path traversal on a JSON object stored under the root key
    let parts: Vec<&str> = expr.splitn(2, '.').collect();
    if parts.len() == 2 {
        if let Some(root) = store.get(parts[0]) {
            if let Some(nested) = json_path(root, parts[1]) {
                return json_value_to_string(nested);
            }
        }
    }

    // No match — return the expression itself so it's visible during development
    expr.to_string()
}

/// Walk a dotted path into a JSON value.
fn json_path(mut value: serde_json::Value, path: &str) -> Option<serde_json::Value> {
    for key in path.split('.') {
        value = value.get(key)?.clone();
    }
    Some(value)
}

/// Strip surrounding `"..."` or `'...'` quotes, returning the inner string.
fn strip_string_literal(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() >= 2 {
        let q = s.chars().next()?;
        if (q == '"' || q == '\'') && s.ends_with(q) {
            return Some(s[1..s.len() - 1].to_string());
        }
    }
    None
}

fn normalize_tag(tag: &str) -> String {
    match tag {
        // Internal tags — pass through.
        "row" | "column" | "text" | "button" | "input" | "slider" | "scroll" | "icon" | "box" => {
            tag.to_string()
        }
        // HTML semantic / block containers.
        "nav" | "header" => "row".into(),
        "main" | "section" | "article" | "aside" => "column".into(),
        // Generic block container — direction controlled by CSS flex-direction.
        "div" => "box".into(),
        // List containers.
        "ul" | "ol" => "column".into(),
        "li" => "row".into(),
        // Inline and block text elements — folded to leaf text nodes.
        "span" | "label" | "em" | "strong" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            "text".into()
        }
        // Image.
        "img" => "icon".into(),
        // PascalCase → component placeholder.
        other if other.chars().next().is_some_and(char::is_uppercase) => "box".into(),
        // Unknown tags — generic container.
        _ => "box".into(),
    }
}

fn merge_missing_defaults(tag: &str, style: &mut ComputedStyle) {
    let defaults = default_leaf_style(tag);

    if tag == "icon" {
        style.background_color = mesh_ui::Color::TRANSPARENT;
        style.border_radius = mesh_ui::Corners::zero();
        style.padding = mesh_ui::Edges::zero();
    }

    if style.background_color.a == 0 && defaults.background_color.a > 0 {
        style.background_color = defaults.background_color;
    }
    if style.color.a == 0 {
        style.color = defaults.color;
    }
    if style.padding.top == 0.0
        && style.padding.right == 0.0
        && style.padding.bottom == 0.0
        && style.padding.left == 0.0
    {
        style.padding = defaults.padding;
    }
    if style.gap == 0.0 {
        style.gap = defaults.gap;
    }
    if style.border_radius.top_left == 0.0 {
        style.border_radius = defaults.border_radius;
    }
    if style.overflow_x == ComputedStyle::default().overflow_x {
        style.overflow_x = defaults.overflow_x;
    }
    if style.overflow_y == ComputedStyle::default().overflow_y {
        style.overflow_y = defaults.overflow_y;
    }
    if style.font_size == ComputedStyle::default().font_size {
        style.font_size = defaults.font_size;
    }
    if tag == "column" || tag == "row" {
        if style.direction != defaults.direction {
            style.direction = defaults.direction;
        }
    }
}

fn surface_style(surface_id: &str, width: u32, height: u32) -> ComputedStyle {
    let mut style = container_style("column");
    style.padding = mesh_ui::Edges::all(0.0);
    style.gap = 0.0;
    style.width = mesh_ui::Dimension::Px(width as f32);
    style.height = mesh_ui::Dimension::Px(height as f32);
    style.background_color = match surface_id {
        "@mesh/panel" => mesh_ui::Color::from_hex("#1f1a24").unwrap_or(mesh_ui::Color::BLACK),
        "@mesh/launcher" => mesh_ui::Color::from_hex("#141218").unwrap_or(mesh_ui::Color::BLACK),
        "@mesh/notification-center" => {
            mesh_ui::Color::from_hex("#18161d").unwrap_or(mesh_ui::Color::BLACK)
        }
        "@mesh/quick-settings" => {
            mesh_ui::Color::from_hex("#1b1b22").unwrap_or(mesh_ui::Color::BLACK)
        }
        _ => mesh_ui::Color::from_hex("#16131a").unwrap_or(mesh_ui::Color::BLACK),
    };
    style
}

fn container_style(tag: &str) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.direction = if tag == "column" {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    };
    style.padding = mesh_ui::Edges::all(12.0);
    style.gap = 8.0;
    style.color = mesh_ui::Color::WHITE;
    style
}

fn embedded_root_style() -> ComputedStyle {
    let mut style = container_style("column");
    style.padding = mesh_ui::Edges::all(0.0);
    style.gap = 0.0;
    style.background_color = mesh_ui::Color::TRANSPARENT;
    style.width = mesh_ui::Dimension::Auto;
    style.height = mesh_ui::Dimension::Auto;
    style
}

fn slot_style(tag: &str) -> ComputedStyle {
    let mut style = container_style(tag);
    style.padding = mesh_ui::Edges::all(0.0);
    style.background_color = mesh_ui::Color::TRANSPARENT;
    style.border_radius = mesh_ui::Corners::all(0.0);
    style.width = mesh_ui::Dimension::Auto;
    style.height = mesh_ui::Dimension::Auto;
    style
}

fn text_style() -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.display = Display::Flex;
    style.color = mesh_ui::Color::WHITE;
    style.font_size = 14.0;
    style.background_color = mesh_ui::Color::TRANSPARENT;
    style
}

fn default_leaf_style(tag: &str) -> ComputedStyle {
    let mut style = match tag {
        "column" | "row" => container_style(tag),
        "button" => {
            let mut style = container_style("row");
            style.background_color =
                mesh_ui::Color::from_hex("#2b2633").unwrap_or(mesh_ui::Color::BLACK);
            style.border_radius = mesh_ui::Corners::all(12.0);
            style.padding = mesh_ui::Edges::all(10.0);
            style
        }
        "input" => {
            let mut style = container_style("row");
            style.background_color =
                mesh_ui::Color::from_hex("#221f28").unwrap_or(mesh_ui::Color::BLACK);
            style.border_radius = mesh_ui::Corners::all(10.0);
            style.padding = mesh_ui::Edges::all(10.0);
            style.height = mesh_ui::Dimension::Px(44.0);
            style.border_width = mesh_ui::Edges::all(1.0);
            style.border_color =
                mesh_ui::Color::from_hex("#3b3644").unwrap_or(mesh_ui::Color::WHITE);
            style
        }
        "slider" => {
            let mut style = container_style("row");
            style.height = mesh_ui::Dimension::Px(36.0);
            style.padding = mesh_ui::Edges::all(8.0);
            style
        }
        "scroll" => {
            let mut style = container_style("column");
            style.background_color = mesh_ui::Color::TRANSPARENT;
            style.height = mesh_ui::Dimension::Px(220.0);
            style.padding = mesh_ui::Edges::all(0.0);
            style.overflow_x = mesh_ui::Overflow::Hidden;
            style.overflow_y = mesh_ui::Overflow::Auto;
            style
        }
        "icon" => {
            let mut style = ComputedStyle::default();
            style.width = mesh_ui::Dimension::Px(18.0);
            style.height = mesh_ui::Dimension::Px(18.0);
            style.background_color = mesh_ui::Color::TRANSPARENT;
            style
        }
        "box" => {
            let mut style = ComputedStyle::default();
            style.background_color = mesh_ui::Color::TRANSPARENT;
            style
        }
        "text" => text_style(),
        _ => container_style("column"),
    };

    if tag == "text" {
        style.height = mesh_ui::Dimension::Px(22.0);
    }

    style
}

fn accessibility_for_tag(tag: &str) -> AccessibilityInfo {
    let mut info = AccessibilityInfo::default();
    info.role = match tag {
        "button" => mesh_component::meta::AccessibilityRole::Button,
        "input" => mesh_component::meta::AccessibilityRole::TextInput,
        "slider" => mesh_component::meta::AccessibilityRole::Slider,
        "text" => mesh_component::meta::AccessibilityRole::Label,
        _ => mesh_component::meta::AccessibilityRole::Region,
    };
    info.focusable = matches!(tag, "button" | "input" | "slider");
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_component::template::{Attribute, AttributeValue};

    #[test]
    fn normalizes_html_style_event_handler_names() {
        let attrs = vec![Attribute {
            name: "onclick".into(),
            value: AttributeValue::EventHandler("openPanel".into()),
        }];

        let (_, _, _, handlers) = parse_attributes(&attrs, None);

        assert_eq!(handlers.get("click").map(String::as_str), Some("openPanel"));
        assert!(!handlers.contains_key("onclick"));
    }

    struct MapStore(HashMap<String, serde_json::Value>);
    impl mesh_ui::VariableStore for MapStore {
        fn get(&self, name: &str) -> Option<serde_json::Value> { self.0.get(name).cloned() }
        fn keys(&self) -> Vec<String> { self.0.keys().cloned().collect() }
    }

    #[test]
    fn eval_expr_length_operator() {
        let store = MapStore(
            [("items".to_string(), serde_json::json!(["a", "b", "c"]))]
                .into_iter()
                .collect(),
        );
        assert_eq!(eval_expr("#items", &store), "3");
        assert_eq!(eval_expr("#missing", &store), "0");
    }

    #[test]
    fn eval_expr_boolean_and() {
        let store = MapStore(
            [
                ("a".to_string(), serde_json::json!(true)),
                ("b".to_string(), serde_json::json!(false)),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(eval_expr("a and b", &store), "false");
        assert_eq!(eval_expr("a and a", &store), "true");
        assert_eq!(eval_expr("b and a", &store), "false");
    }

    #[test]
    fn for_node_iterates_over_list() {
        let source = r#"
<template>
  <div>
    {#for item in items}
      <span>{item.name}</span>
    {/for}
  </div>
</template>
"#;
        let plugin = mesh_component::parse_component(source).unwrap();
        let manifest = mesh_plugin::Manifest {
            package: mesh_plugin::PackageSection {
                id: "test".into(),
                version: "0.1.0".into(),
                plugin_type: mesh_plugin::PluginType::Widget,
                api_version: "0.1.0".into(),
                name: None,
                license: None,
                description: None,
                authors: vec![],
                repository: None,
            },
            compatibility: Default::default(),
            dependencies: Default::default(),
            capabilities: Default::default(),
            entrypoints: Default::default(),
            accessibility: None,
            settings: None,
            i18n: None,
            theme: None,
            service: None,
            provides: vec![],
            interface: None,
            extensions: vec![],
            exports: Default::default(),
            provides_slots: Default::default(),
            slot_contributions: Default::default(),
            surface_layout: None,
            assets: None,
            translations: Default::default(),
        };
        let theme = mesh_theme::default_theme();
        let store = MapStore(
            [(
                "items".to_string(),
                serde_json::json!([{"name": "Alice"}, {"name": "Bob"}]),
            )]
            .into_iter()
            .collect(),
        );
        let compiled = CompiledFrontendPlugin {
            manifest,
            source_path: std::path::PathBuf::from("test.mesh"),
            component: plugin,
            local_components: Default::default(),
        };
        let tree = compiled.build_preview_tree_with_state(&theme, 400, 300, Some(&store));
        let texts = collect_text_content(&tree);
        assert!(
            texts.contains(&"Alice".to_string()),
            "expected Alice in {texts:?}"
        );
        assert!(
            texts.contains(&"Bob".to_string()),
            "expected Bob in {texts:?}"
        );
    }

    fn collect_text_content(node: &mesh_ui::WidgetNode) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(c) = node.attributes.get("content") {
            if !c.is_empty() {
                out.push(c.clone());
            }
        }
        for child in &node.children {
            out.extend(collect_text_content(child));
        }
        out
    }
}
