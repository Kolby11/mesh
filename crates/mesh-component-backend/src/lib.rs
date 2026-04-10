use mesh_component::template::{Attribute, AttributeValue, ComponentRef, ElementNode, TemplateNode};
use mesh_component::{ComponentFile, parse_component};
use mesh_plugin::{Manifest, PluginType};
use mesh_theme::Theme;
use mesh_ui::accessibility::AccessibilityInfo;
use mesh_ui::style::{Display, FlexDirection};
use mesh_ui::{ComputedStyle, LayoutEngine, StyleResolver, VariableStore, WidgetNode};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CompiledFrontendPlugin {
    pub manifest: Manifest,
    pub source_path: PathBuf,
    pub component: ComponentFile,
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
        let mut root = WidgetNode::new("surface");
        root.attributes.insert("id".into(), self.manifest.package.id.clone());
        root.computed_style = surface_style(&self.manifest.package.id, width, height);
        if let Some(meta) = &self.component.meta {
            if let Some(role) = &meta.role {
                root.accessibility.role = role.clone();
            }
            root.accessibility.label = meta.label.clone();
            root.accessibility.description = meta.description.clone();
        }

        let resolver = StyleResolver::new(theme);
        let rules = self
            .component
            .style
            .as_ref()
            .map(|style| style.rules.as_slice())
            .unwrap_or(&[]);

        if let Some(template) = &self.component.template {
            root.children = template
                .root
                .iter()
                .map(|node| {
                    build_widget_node(node, rules, &resolver, Some(&root.computed_style), state)
                })
                .collect();
        }

        LayoutEngine::compute(&mut root, width as f32, height as f32);
        root
    }
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
    let component =
        parse_component(&source).map_err(|source| CompileFrontendError::ParseSource {
            path: source_path.clone(),
            source,
        })?;

    tracing::info!(
        "compiled frontend plugin '{}' from {}",
        manifest.package.id,
        source_path.display()
    );

    Ok(CompiledFrontendPlugin {
        manifest: manifest.clone(),
        source_path,
        component,
    })
}

fn build_widget_node(
    node: &TemplateNode,
    rules: &[mesh_component::style::StyleRule],
    resolver: &StyleResolver<'_>,
    parent_style: Option<&ComputedStyle>,
    state: Option<&dyn VariableStore>,
) -> WidgetNode {
    match node {
        TemplateNode::Element(element) => {
            build_element_node(element, rules, resolver, parent_style, state)
        }
        TemplateNode::Component(component) => {
            build_component_ref(component, rules, resolver, parent_style, state)
        }
        TemplateNode::Text(text) => {
            let mut node = WidgetNode::new("text");
            node.attributes.insert("content".into(), text.content.clone());
            node.computed_style = text_style();
            if let Some(parent_style) = parent_style {
                inherit_text_style(&mut node.computed_style, parent_style, InheritedStyleMask::default());
            }
            node
        }
        TemplateNode::Expr(expr) => {
            let mut node = WidgetNode::new("text");
            let content = state
                .and_then(|store| store.get(&expr.expression))
                .map(json_value_to_string)
                .unwrap_or_else(|| format!("{{{{ {} }}}}", expr.expression));
            node.attributes
                .insert("content".into(), content);
            node.computed_style = text_style();
            if let Some(parent_style) = parent_style {
                inherit_text_style(&mut node.computed_style, parent_style, InheritedStyleMask::default());
            }
            node
        }
        TemplateNode::If(if_node) => {
            let mut node = WidgetNode::new("column");
            node.attributes
                .insert("condition".into(), if_node.condition.clone());
            node.computed_style = container_style("column");
            node.children = if_node
                .then_children
                .iter()
                .map(|child| {
                    build_widget_node(child, rules, resolver, Some(&node.computed_style), state)
                })
                .collect();
            if let Some(parent_style) = parent_style {
                inherit_text_style(&mut node.computed_style, parent_style, InheritedStyleMask::default());
            }
            node
        }
        TemplateNode::For(for_node) => {
            let mut node = WidgetNode::new("column");
            node.attributes.insert(
                "content".into(),
                format!("for {} in {}", for_node.item_name, for_node.iterable),
            );
            node.computed_style = container_style("column");
            node.children = for_node
                .children
                .iter()
                .map(|child| {
                    build_widget_node(child, rules, resolver, Some(&node.computed_style), state)
                })
                .collect();
            if let Some(parent_style) = parent_style {
                inherit_text_style(&mut node.computed_style, parent_style, InheritedStyleMask::default());
            }
            node
        }
        TemplateNode::Slot(slot) => {
            let mut node = WidgetNode::new("box");
            node.attributes.insert(
                "content".into(),
                slot.name
                    .as_ref()
                    .map(|name| format!("slot:{name}"))
                    .unwrap_or_else(|| "slot".into()),
            );
            node.computed_style = default_leaf_style("box");
            if let Some(parent_style) = parent_style {
                inherit_text_style(&mut node.computed_style, parent_style, InheritedStyleMask::default());
            }
            node
        }
    }
}

fn build_element_node(
    element: &ElementNode,
    rules: &[mesh_component::style::StyleRule],
    resolver: &StyleResolver<'_>,
    parent_style: Option<&ComputedStyle>,
    state: Option<&dyn VariableStore>,
) -> WidgetNode {
    let tag = normalize_tag(&element.tag);
    let (classes, id, attributes, event_handlers) = parse_attributes(&element.attributes, state);
    let inherited_mask = inherited_style_mask(rules, &tag, &classes, id.as_deref());

    let mut node = WidgetNode::new(tag.clone());
    node.attributes = attributes;
    node.event_handlers = event_handlers;
    node.computed_style = resolver.resolve_node_style(rules, &tag, &classes, id.as_deref());
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

    node.children = element
        .children
        .iter()
        .map(|child| build_widget_node(child, rules, resolver, Some(&node.computed_style), state))
        .collect();

    node
}

fn build_component_ref(
    component: &ComponentRef,
    rules: &[mesh_component::style::StyleRule],
    resolver: &StyleResolver<'_>,
    parent_style: Option<&ComputedStyle>,
    state: Option<&dyn VariableStore>,
) -> WidgetNode {
    let fake_element = ElementNode {
        tag: "box".into(),
        attributes: component.props.clone(),
        children: component.children.clone(),
    };
    let mut node = build_element_node(&fake_element, rules, resolver, parent_style, state);
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
) -> InheritedStyleMask {
    let mut mask = InheritedStyleMask::default();

    for rule in rules {
        if !selector_matches(&rule.selector, tag, classes, id) {
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
            AttributeValue::Binding(binding) => {
                let value = state
                    .and_then(|store| store.get(binding))
                    .map(json_value_to_string)
                    .unwrap_or_default();
                resolved.insert(attr.name.clone(), value);
            }
            AttributeValue::EventHandler(handler) => {
                event_handlers.insert(attr.name.clone(), handler.clone());
            }
        }
    }

    (classes, id, resolved, event_handlers)
}

fn json_value_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    }
}

fn normalize_tag(tag: &str) -> String {
    match tag {
        "row" | "column" | "text" | "button" | "input" | "slider" | "scroll" | "icon" | "box" => {
            tag.to_string()
        }
        other => {
            if other.chars().next().is_some_and(char::is_uppercase) {
                "box".into()
            } else {
                other.to_string()
            }
        }
    }
}

fn merge_missing_defaults(tag: &str, style: &mut ComputedStyle) {
    let defaults = default_leaf_style(tag);

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
            style
        }
        "icon" => {
            let mut style = ComputedStyle::default();
            style.width = mesh_ui::Dimension::Px(18.0);
            style.height = mesh_ui::Dimension::Px(18.0);
            style.background_color =
                mesh_ui::Color::from_hex("#7f67be").unwrap_or(mesh_ui::Color::WHITE);
            style.border_radius = mesh_ui::Corners::all(9.0);
            style
        }
        "box" => {
            let mut style = container_style("column");
            style.background_color =
                mesh_ui::Color::from_hex("#24202b").unwrap_or(mesh_ui::Color::BLACK);
            style.border_radius = mesh_ui::Corners::all(16.0);
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
