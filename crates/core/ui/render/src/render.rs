use crate::expr::eval_expr;
use crate::style::{
    InheritedStyleMask, child_style_context, container_style, inherit_text_style,
    inherited_style_mask, merge_missing_defaults, slot_style, text_style,
};
use crate::tags::lower_source_tag;
use crate::{FrontendCompositionResolver, LayeredStore};

use mesh_core_component::template::{
    Attribute, AttributeValue, ComponentRef, ElementNode, SourceTag, TemplateNode,
};
use mesh_core_elements::accessibility::AccessibilityInfo;
use mesh_core_elements::{ComputedStyle, StyleContext, StyleResolver, VariableStore, WidgetNode};
use mesh_core_plugin::Manifest;
use mesh_core_theme::Theme;

use std::collections::HashMap;

pub(crate) fn collect_component_tags(nodes: &[TemplateNode], tags: &mut Vec<String>) {
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
    component: &mesh_core_component::ComponentFile,
    host_manifest: &Manifest,
    theme: &Theme,
    container_width: f32,
    container_height: f32,
    composition: Option<&dyn FrontendCompositionResolver>,
    instance_key: &str,
    state: Option<&dyn VariableStore>,
    host_rules: &[mesh_core_component::style::StyleRule],
) -> WidgetNode {
    let resolver = StyleResolver::new(theme);
    let component_rules: &[mesh_core_component::style::StyleRule] = component
        .style
        .as_ref()
        .map(|s| s.rules.as_slice())
        .unwrap_or(&[]);
    let merged: Vec<mesh_core_component::style::StyleRule>;
    let rules: &[mesh_core_component::style::StyleRule] = if host_rules.is_empty() {
        component_rules
    } else if component_rules.is_empty() {
        host_rules
    } else {
        merged = host_rules
            .iter()
            .chain(component_rules.iter())
            .cloned()
            .collect();
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

pub(crate) fn build_widget_node(
    node: &TemplateNode,
    manifest: &Manifest,
    rules: &[mesh_core_component::style::StyleRule],
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
    rules: &[mesh_core_component::style::StyleRule],
    resolver: &StyleResolver<'_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> WidgetNode {
    let tag = lower_source_tag(&element.tag_kind).as_str().to_string();
    let (classes, id, mut attributes, event_handlers) =
        parse_attributes(&element.attributes, state);
    if tag == "input" && !attributes.contains_key("type") {
        if let Some(input_type) = default_input_type(&element.tag_kind) {
            attributes.insert("type".into(), input_type.into());
        }
    }
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

    if tag == "text"
        && !element.children.is_empty()
        && element.children.iter().all(is_inline_template_node)
    {
        let content: String = element
            .children
            .iter()
            .map(|child| resolve_inline_content(child, state))
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

fn default_input_type(source_tag: &SourceTag) -> Option<&'static str> {
    match source_tag {
        SourceTag::TextInput => Some("text"),
        SourceTag::PasswordInput => Some("password"),
        SourceTag::SearchInput => Some("search"),
        SourceTag::NumberInput => Some("number"),
        SourceTag::EmailInput => Some("email"),
        SourceTag::UrlInput => Some("url"),
        _ => None,
    }
}

fn resolve_inline_content(node: &TemplateNode, state: Option<&dyn VariableStore>) -> String {
    match node {
        TemplateNode::Text(text) => text.content.clone(),
        TemplateNode::Expr(expr) => state
            .map(|store| eval_expr(&expr.expression, store))
            .unwrap_or_else(|| format!("{{ {} }}", expr.expression)),
        _ => String::new(),
    }
}

fn build_component_ref(
    component: &ComponentRef,
    manifest: &Manifest,
    rules: &[mesh_core_component::style::StyleRule],
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
        tag_kind: SourceTag::Box,
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

pub(crate) fn parse_attributes(
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

fn accessibility_for_tag(tag: &str) -> AccessibilityInfo {
    let mut info = AccessibilityInfo::default();
    info.role = match tag {
        "button" => mesh_core_elements::AccessibilityRole::Button,
        "input" => mesh_core_elements::AccessibilityRole::TextInput,
        "slider" => mesh_core_elements::AccessibilityRole::Slider,
        "text" => mesh_core_elements::AccessibilityRole::Label,
        _ => mesh_core_elements::AccessibilityRole::Region,
    };
    info.focusable = matches!(tag, "button" | "input" | "slider");
    info
}
