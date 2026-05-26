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
use mesh_core_elements::{
    ComputedStyle, ElementDiagnostic, StyleContext, StyleResolver, VariableStore, WidgetNode,
    element_contract_for_tag, validate_element_attribute, validate_element_event,
};
use mesh_core_module::Manifest;
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
/// `host_rules` are the parent module's CSS rules. They are merged before the
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
        attach_module_id(&mut container, &host_manifest.package.id);
        container.children = children;
        container
    } else {
        let mut container = WidgetNode::new("box");
        attach_module_id(&mut container, &host_manifest.package.id);
        container
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
            attach_module_id(&mut node, &manifest.package.id);
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
            attach_module_id(&mut node, &manifest.package.id);
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
            attach_module_id(&mut node, &manifest.package.id);
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
            attach_module_id(&mut node, &manifest.package.id);
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
            attach_module_id(&mut node, &manifest.package.id);
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
    let source_tag = element.tag.as_str();
    let tag = lower_source_tag(&element.tag_kind).as_str().to_string();
    let _element_diagnostics = collect_element_diagnostics(element);
    let (classes, id, mut attributes, event_handlers) =
        parse_attributes(&element.attributes, state);
    attributes
        .entry("data-mesh-element".into())
        .or_insert_with(|| source_tag.to_string());
    if tag == "input" && !attributes.contains_key("type") {
        if let Some(input_type) = default_input_type(&element.tag_kind) {
            attributes.insert("type".into(), input_type.into());
        }
    }
    apply_source_tag_defaults(&element.tag_kind, &mut attributes);
    let inherited_mask =
        inherited_style_mask(rules, &tag, &classes, id.as_deref(), container_context);

    let mut node = WidgetNode::new(tag.clone());
    attach_module_id(&mut node, &manifest.package.id);
    node.attributes = attributes;
    node.event_handlers = event_handlers;
    node.computed_style = resolver.resolve_node_style_for_module(
        rules,
        &tag,
        &classes,
        id.as_deref(),
        container_context,
        Default::default(),
        Some(&manifest.package.id),
    );
    merge_missing_defaults(&tag, &mut node.computed_style);
    if let Some(parent_style) = parent_style {
        inherit_text_style(&mut node.computed_style, parent_style, inherited_mask);
    }
    node.accessibility = accessibility_for_element(source_tag, &tag, &node.attributes);

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

pub fn collect_element_diagnostics(element: &ElementNode) -> Vec<ElementDiagnostic> {
    let tag = element.tag.as_str();
    let mut diagnostics = Vec::new();
    for attribute in &element.attributes {
        if attribute.name == "class" || attribute.name == "id" || attribute.name == "bind:this" {
            continue;
        }
        if is_event_handler_attribute(&attribute.name) {
            let event_name = normalize_event_handler_name(&attribute.name);
            if let Some(diagnostic) = validate_element_event(tag, &event_name) {
                diagnostics.push(diagnostic);
            }
        } else if let Some(diagnostic) =
            validate_element_attribute(tag, &attribute.name, attribute_static_value(attribute))
        {
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}

fn attribute_static_value(attribute: &Attribute) -> &str {
    match &attribute.value {
        AttributeValue::Static(value) => value,
        _ => "",
    }
}

fn is_inline_template_node(node: &TemplateNode) -> bool {
    matches!(node, TemplateNode::Text(_) | TemplateNode::Expr(_))
}

fn default_input_type(source_tag: &SourceTag) -> Option<&'static str> {
    match source_tag {
        SourceTag::TextArea => Some("textarea"),
        SourceTag::Search => Some("search"),
        SourceTag::Password => Some("password"),
        SourceTag::NumberInput => Some("number"),
        SourceTag::Stepper => Some("number"),
        SourceTag::TextInput => Some("text"),
        SourceTag::PasswordInput => Some("password"),
        SourceTag::SearchInput => Some("search"),
        SourceTag::EmailInput => Some("email"),
        SourceTag::UrlInput => Some("url"),
        _ => None,
    }
}

fn apply_source_tag_defaults(source_tag: &SourceTag, attributes: &mut HashMap<String, String>) {
    match source_tag {
        SourceTag::TextArea => {
            attributes
                .entry("multiline".into())
                .or_insert_with(|| "true".into());
        }
        SourceTag::Password | SourceTag::PasswordInput => {
            attributes
                .entry("masked".into())
                .or_insert_with(|| "true".into());
        }
        SourceTag::Stepper => {
            attributes
                .entry("step".into())
                .or_insert_with(|| "1".into());
        }
        _ => {}
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
    let (_, _, mut props, _) = parse_attributes(&component.props, state);
    for attr in &component.props {
        if let AttributeValue::EventHandler(handler) = &attr.value {
            props.insert(
                attr.name.clone(),
                resolve_component_prop_handler_value(state, host_instance_key, handler),
            );
        } else if let AttributeValue::Binding(binding) = &attr.value {
            props.insert(format!("__mesh_binding_{}", attr.name), binding.clone());
        } else if let AttributeValue::InstanceBinding(binding) = &attr.value {
            props.insert("__mesh_bind_this".to_string(), binding.clone());
        }
    }
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

fn attach_module_id(node: &mut WidgetNode, module_id: &str) {
    node.attributes
        .insert("_mesh_module_id".into(), module_id.to_string());
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
                if is_event_handler_attribute(&attr.name) {
                    event_handlers.insert(normalize_event_handler_name(&attr.name), value);
                } else {
                    resolved.insert(attr.name.clone(), value);
                }
            }
            AttributeValue::InstanceBinding(_) => {}
            AttributeValue::EventHandler(handler) => {
                let resolved_handler = resolve_event_handler_value(state, handler);
                event_handlers.insert(normalize_event_handler_name(&attr.name), resolved_handler);
            }
        }
    }

    (classes, id, resolved, event_handlers)
}

fn resolve_event_handler_value(state: Option<&dyn VariableStore>, handler: &str) -> String {
    state
        .and_then(|store| store.get(handler))
        .and_then(|value| value.as_str().map(str::to_string))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| handler.to_string())
}

fn resolve_component_prop_handler_value(
    state: Option<&dyn VariableStore>,
    host_instance_key: &str,
    handler: &str,
) -> String {
    let resolved = resolve_event_handler_value(state, handler);
    if resolved.starts_with("__mesh_embed__::") {
        resolved
    } else {
        format!("__mesh_embed__::{host_instance_key}::{resolved}")
    }
}

fn normalize_event_handler_name(name: &str) -> String {
    name.strip_prefix("on").unwrap_or(name).to_string()
}

fn is_event_handler_attribute(name: &str) -> bool {
    if !name.starts_with("on") {
        return false;
    }
    matches!(
        normalize_event_handler_name(name).as_str(),
        "click"
            | "input"
            | "change"
            | "select"
            | "activate"
            | "openchange"
            | "release"
            | "focus"
            | "blur"
            | "keydown"
            | "keyup"
            | "keybind"
    )
}

fn accessibility_for_element(
    source_tag: &str,
    runtime_tag: &str,
    attributes: &HashMap<String, String>,
) -> AccessibilityInfo {
    let mut info = AccessibilityInfo::default();
    if let Some(contract) = element_contract_for_tag(source_tag) {
        info.role = contract.accessibility.role.clone();
        info.focusable = contract.accessibility.focusable;
    } else {
        info.role = match runtime_tag {
            "button" => mesh_core_elements::AccessibilityRole::Button,
            "input" => mesh_core_elements::AccessibilityRole::TextInput,
            "slider" => mesh_core_elements::AccessibilityRole::Slider,
            "checkbox" => mesh_core_elements::AccessibilityRole::Checkbox,
            "switch" => mesh_core_elements::AccessibilityRole::Switch,
            "text" => mesh_core_elements::AccessibilityRole::Label,
            _ => mesh_core_elements::AccessibilityRole::Region,
        };
        info.focusable = matches!(
            runtime_tag,
            "button" | "input" | "slider" | "checkbox" | "switch"
        );
    }
    info.label = attributes
        .get("aria-label")
        .or_else(|| attributes.get("label"))
        .or_else(|| attributes.get("alt"))
        .cloned();
    info.description = attributes
        .get("title")
        .or_else(|| attributes.get("tooltip"))
        .cloned();
    info.keyboard_shortcut = attributes
        .get("key")
        .or_else(|| attributes.get("keybind"))
        .or_else(|| attributes.get("shortcut"))
        .cloned();
    info.state.disabled = bool_attr(attributes, "disabled");
    info.state.checked = attributes.get("checked").map(|value| bool_value(value));
    info.state.expanded = attributes.get("expanded").map(|value| bool_value(value));
    info.state.selected = bool_attr(attributes, "selected");
    info.state.pressed = bool_attr(attributes, "pressed");
    info.state.busy = bool_attr(attributes, "busy");
    info.state.invalid = bool_attr(attributes, "invalid");
    info.state.required = bool_attr(attributes, "required");
    info.state.value = attributes.get("value").cloned();
    info.state.value_min = number_attr(attributes, "min");
    info.state.value_max = number_attr(attributes, "max");
    info
}

#[cfg(test)]
fn accessibility_for_tag(tag: &str) -> AccessibilityInfo {
    accessibility_for_element(tag, tag, &HashMap::new())
}

fn bool_attr(attributes: &HashMap<String, String>, name: &str) -> bool {
    attributes.get(name).is_some_and(|value| bool_value(value))
}

fn bool_value(value: &str) -> bool {
    matches!(value.trim(), "" | "true" | "1")
}

fn number_attr(attributes: &HashMap<String, String>, name: &str) -> Option<f32> {
    attributes.get(name)?.trim().parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::ElementDiagnosticKind;

    fn test_manifest() -> Manifest {
        Manifest {
            package: mesh_core_module::ModuleSection {
                id: "test".into(),
                version: "0.1.0".into(),
                module_type: mesh_core_module::ModuleType::Widget,
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
            keybinds: Default::default(),
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
            icons: None,
            icon_pack: None,
            icon_requirements: Default::default(),
            translations: Default::default(),
        }
    }

    fn find_tag<'a>(node: &'a WidgetNode, tag: &str) -> Option<&'a WidgetNode> {
        if node.tag == tag {
            return Some(node);
        }
        node.children.iter().find_map(|child| find_tag(child, tag))
    }

    #[test]
    fn event_handler_attributes_normalize_to_widget_event_keys() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <box>
    <button onclick={onTap}>Tap</button>
    <input onchange={onInputChange} onfocus={onInputFocus} />
    <slider onrelease={onSliderRelease} />
  </box>
</template>
"#,
        )
        .unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();

        let tree = build_widget_tree_from_component(
            &component,
            &manifest,
            &theme,
            200.0,
            80.0,
            None,
            "root",
            None,
            &[],
        );

        let button = find_tag(&tree, "button").expect("button node");
        assert_eq!(button.event_handlers.get("click"), Some(&"onTap".into()));

        let input = find_tag(&tree, "input").expect("input node");
        assert_eq!(
            input.event_handlers.get("change"),
            Some(&"onInputChange".into())
        );
        assert_eq!(
            input.event_handlers.get("focus"),
            Some(&"onInputFocus".into())
        );

        let slider = find_tag(&tree, "slider").expect("slider node");
        assert_eq!(
            slider.event_handlers.get("release"),
            Some(&"onSliderRelease".into())
        );
    }

    #[test]
    fn accessibility_for_tag_marks_switch_and_checkbox_focusable() {
        let checkbox = accessibility_for_tag("checkbox");
        assert_eq!(
            checkbox.role,
            mesh_core_elements::AccessibilityRole::Checkbox
        );
        assert!(checkbox.focusable);

        let switch = accessibility_for_tag("switch");
        assert_eq!(switch.role, mesh_core_elements::AccessibilityRole::Switch);
        assert!(switch.focusable);
    }

    #[test]
    fn shared_value_change_handlers_are_normalized() {
        let attrs = vec![
            Attribute {
                name: "oninput".into(),
                value: AttributeValue::EventHandler("onInput".into()),
            },
            Attribute {
                name: "onchange".into(),
                value: AttributeValue::EventHandler("onChange".into()),
            },
            Attribute {
                name: "onselect".into(),
                value: AttributeValue::EventHandler("onSelect".into()),
            },
            Attribute {
                name: "onactivate".into(),
                value: AttributeValue::EventHandler("onActivate".into()),
            },
            Attribute {
                name: "onopenchange".into(),
                value: AttributeValue::EventHandler("onOpenChange".into()),
            },
        ];

        let (_, _, _, handlers) = parse_attributes(&attrs, None);

        assert_eq!(handlers.get("input"), Some(&"onInput".to_string()));
        assert_eq!(handlers.get("change"), Some(&"onChange".to_string()));
        assert_eq!(handlers.get("select"), Some(&"onSelect".to_string()));
        assert_eq!(handlers.get("activate"), Some(&"onActivate".to_string()));
        assert_eq!(
            handlers.get("openchange"),
            Some(&"onOpenChange".to_string())
        );
    }

    #[test]
    fn two_way_value_binding_still_resolves_attribute_value() {
        let attrs = vec![Attribute {
            name: "value".into(),
            value: AttributeValue::TwoWayBinding("current_value".into()),
        }];

        let (_, _, resolved, _) = parse_attributes(&attrs, None);

        assert_eq!(resolved.get("value"), Some(&String::new()));
    }

    #[test]
    fn frontend_element_diagnostics_collect_unsupported_attribute() {
        let element = ElementNode {
            tag: "button".into(),
            tag_kind: SourceTag::Button,
            attributes: vec![Attribute {
                name: "browser-form-action".into(),
                value: AttributeValue::Static("submit".into()),
            }],
            children: vec![],
        };

        let diagnostics = collect_element_diagnostics(&element);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].tag, "button");
        assert_eq!(diagnostics[0].name, "browser-form-action");
    }

    #[test]
    fn frontend_element_diagnostics_preserve_shipped_pass_through_attributes() {
        let element = ElementNode {
            tag: "button".into(),
            tag_kind: SourceTag::Button,
            attributes: vec![
                Attribute {
                    name: "data-test-id".into(),
                    value: AttributeValue::Static("ok".into()),
                },
                Attribute {
                    name: "aria-label".into(),
                    value: AttributeValue::Static("OK".into()),
                },
            ],
            children: vec![],
        };

        assert!(collect_element_diagnostics(&element).is_empty());
    }

    #[test]
    fn phase87_layout_display_source_semantics_survive_lowering() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <grid columns="120px auto" label="Main grid">
    <progress value="50" min="0" max="100" label="Loading" />
    <section label="Details">
      <badge>Ready</badge>
    </section>
  </grid>
</template>
"#,
        )
        .unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();

        let tree = build_widget_tree_from_component(
            &component,
            &manifest,
            &theme,
            320.0,
            120.0,
            None,
            "root",
            None,
            &[],
        );

        let grid = tree.children.first().expect("grid node");
        assert_eq!(grid.tag, "box");
        assert_eq!(
            grid.attributes.get("data-mesh-element"),
            Some(&"grid".to_string())
        );
        assert_eq!(
            grid.accessibility.role,
            mesh_core_elements::AccessibilityRole::Region
        );
        assert_eq!(grid.accessibility.label.as_deref(), Some("Main grid"));

        let progress = grid
            .children
            .iter()
            .find(|node| {
                node.attributes
                    .get("data-mesh-element")
                    .is_some_and(|value| value == "progress")
            })
            .expect("progress node");
        assert_eq!(progress.tag, "text");
        assert_eq!(
            progress.accessibility.role,
            mesh_core_elements::AccessibilityRole::ProgressBar
        );
        assert_eq!(progress.accessibility.state.value.as_deref(), Some("50"));
        assert_eq!(progress.accessibility.state.value_min, Some(0.0));
        assert_eq!(progress.accessibility.state.value_max, Some(100.0));
    }

    #[test]
    fn phase87_collects_source_tag_diagnostics_before_lowering() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <grid columns="1fr 2fr" />
  <progress value="half" />
</template>
"#,
        )
        .unwrap();
        let template = component.template.expect("template");
        let diagnostics: Vec<_> = template
            .root
            .iter()
            .filter_map(|node| match node {
                TemplateNode::Element(element) => Some(collect_element_diagnostics(element)),
                _ => None,
            })
            .flatten()
            .collect();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.tag == "grid"
                && diagnostic.name == "columns"
                && diagnostic.kind == ElementDiagnosticKind::InvalidAttributeValue
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.tag == "progress"
                && diagnostic.name == "value"
                && diagnostic.kind == ElementDiagnosticKind::InvalidAttributeValue
        }));
    }

    #[test]
    fn phase88_button_aliases_preserve_source_semantics_without_icon_shortcuts() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <icon-button onclick={onTap} pressed="true" busy="true" keybind="media.play">
    <icon name="media-playback-start" />
    <text>Play</text>
  </icon-button>
</template>
"#,
        )
        .unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();

        let tree = build_widget_tree_from_component(
            &component,
            &manifest,
            &theme,
            240.0,
            80.0,
            None,
            "root",
            None,
            &[],
        );

        let button = tree.children.first().expect("button alias");
        assert_eq!(button.tag, "button");
        assert_eq!(
            button.attributes.get("data-mesh-element"),
            Some(&"icon-button".to_string())
        );
        assert_eq!(button.event_handlers.get("click"), Some(&"onTap".into()));
        assert_eq!(
            button.accessibility.role,
            mesh_core_elements::AccessibilityRole::Button
        );
        assert!(button.accessibility.state.pressed);
        assert!(button.accessibility.state.busy);
        assert_eq!(
            button.accessibility.keyboard_shortcut.as_deref(),
            Some("media.play")
        );
        assert!(button.children.iter().any(|child| child.tag == "icon"));
    }

    #[test]
    fn phase88_input_variants_configure_single_runtime_input_path() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <textarea value="hello" placeholder="Notes" required="true" />
  <password value="secret" />
  <number-input value="5" min="0" max="10" step="1" />
  <stepper value="2" min="0" max="5" />
</template>
"#,
        )
        .unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();

        let tree = build_widget_tree_from_component(
            &component,
            &manifest,
            &theme,
            240.0,
            160.0,
            None,
            "root",
            None,
            &[],
        );

        let textarea = &tree.children[0];
        assert_eq!(textarea.tag, "input");
        assert_eq!(
            textarea.attributes.get("data-mesh-element"),
            Some(&"textarea".to_string())
        );
        assert_eq!(
            textarea.attributes.get("type"),
            Some(&"textarea".to_string())
        );
        assert_eq!(
            textarea.attributes.get("multiline"),
            Some(&"true".to_string())
        );
        assert!(textarea.accessibility.state.required);

        let password = &tree.children[1];
        assert_eq!(password.tag, "input");
        assert_eq!(
            password.attributes.get("type"),
            Some(&"password".to_string())
        );
        assert_eq!(password.attributes.get("masked"), Some(&"true".to_string()));

        let number = &tree.children[2];
        assert_eq!(number.tag, "input");
        assert_eq!(
            number.attributes.get("data-mesh-element"),
            Some(&"number-input".to_string())
        );
        assert_eq!(number.attributes.get("type"), Some(&"number".to_string()));
        assert_eq!(number.accessibility.state.value.as_deref(), Some("5"));
        assert_eq!(number.accessibility.state.value_min, Some(0.0));
        assert_eq!(number.accessibility.state.value_max, Some(10.0));

        let stepper = &tree.children[3];
        assert_eq!(stepper.tag, "input");
        assert_eq!(stepper.attributes.get("type"), Some(&"number".to_string()));
        assert_eq!(stepper.attributes.get("step"), Some(&"1".to_string()));
    }

    #[test]
    fn phase88_collects_button_and_numeric_source_diagnostics() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <button name="media-playback-start" />
  <number-input step="0" />
</template>
"#,
        )
        .unwrap();
        let template = component.template.expect("template");
        let diagnostics: Vec<_> = template
            .root
            .iter()
            .filter_map(|node| match node {
                TemplateNode::Element(element) => Some(collect_element_diagnostics(element)),
                _ => None,
            })
            .flatten()
            .collect();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.tag == "button"
                && diagnostic.name == "name"
                && diagnostic.kind == ElementDiagnosticKind::InvalidAttributeValue
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.tag == "number-input"
                && diagnostic.name == "step"
                && diagnostic.kind == ElementDiagnosticKind::InvalidAttributeValue
        }));
    }

    #[test]
    fn phase89_choice_menu_source_semantics_survive_lowering() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <select value="en" onchange={onLocaleChange} label="Language">
    <option value="en">English</option>
    <option value="sk" selected="true">Slovak</option>
  </select>
  <menu label="Commands">
    <menu-item onactivate={onCommand} keybind="ctrl+k">Command</menu-item>
  </menu>
</template>
"#,
        )
        .unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();

        let tree = build_widget_tree_from_component(
            &component,
            &manifest,
            &theme,
            320.0,
            160.0,
            None,
            "root",
            None,
            &[],
        );

        let select = &tree.children[0];
        assert_eq!(select.tag, "input");
        assert_eq!(
            select.attributes.get("data-mesh-element"),
            Some(&"select".to_string())
        );
        assert_eq!(
            select.event_handlers.get("change"),
            Some(&"onLocaleChange".into())
        );
        assert_eq!(
            select.accessibility.role,
            mesh_core_elements::AccessibilityRole::Menu
        );
        assert_eq!(select.accessibility.state.value.as_deref(), Some("en"));
        assert_eq!(select.children.len(), 2);

        let option = &select.children[1];
        assert_eq!(
            option.attributes.get("data-mesh-element"),
            Some(&"option".to_string())
        );
        assert!(option.accessibility.state.selected);

        let menu = &tree.children[1];
        assert_eq!(
            menu.attributes.get("data-mesh-element"),
            Some(&"menu".into())
        );
        assert_eq!(
            menu.accessibility.role,
            mesh_core_elements::AccessibilityRole::Menu
        );
        let item = &menu.children[0];
        assert_eq!(
            item.attributes.get("data-mesh-element"),
            Some(&"menu-item".to_string())
        );
        assert_eq!(
            item.event_handlers.get("activate"),
            Some(&"onCommand".into())
        );
        assert_eq!(
            item.accessibility.keyboard_shortcut.as_deref(),
            Some("ctrl+k")
        );
    }
}
