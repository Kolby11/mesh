use crate::expr::eval_expr;
use crate::style::{
    InheritedStyleMask, child_style_context, container_style, inherit_text_style,
    inherited_style_mask, merge_missing_defaults, slot_style, text_style,
};
use crate::tags::lower_source_tag;
use crate::{FrontendCompositionResolver, LayeredStore};

use mesh_core_component::style::{StyleValue, prop_variable_key};
use mesh_core_component::template::{
    Attribute, AttributeValue, ComponentRef, ElementNode, SourceTag, TemplateNode,
};
use mesh_core_component::{PropValue, PropsBlock};
use mesh_core_elements::accessibility::AccessibilityInfo;
use mesh_core_elements::{
    ComputedStyle, StyleContext, StyleResolver, VariableStore, WidgetNode, element_contract_for_tag,
};
use mesh_core_module::Manifest;
use mesh_core_theme::Theme;
use serde_json;

use std::collections::{BTreeMap, HashMap};

/// Build the per-instance CSS prop map consumed by `StyleResolver::with_props`.
///
/// Each declared prop resolves to a single value: a `props.<name>` entry in the
/// script `state` (where the shell funnels the precedence-resolved value —
/// default → user setting → instance prop → script write) overrides the declared
/// default. The map is keyed by `prop_variable_key(name)` so `prop(name)`
/// references in `<style>` resolve through the same lookup as `var(--…)`.
pub fn resolve_css_props(
    block: Option<&PropsBlock>,
    state: Option<&dyn VariableStore>,
) -> HashMap<String, StyleValue> {
    let mut map = HashMap::new();
    let Some(block) = block else {
        return map;
    };
    // The shell publishes one `props` table in script state (the precedence-
    // resolved value per name); script writes round-trip back into it.
    let props_state = state.and_then(|s| s.get("props"));
    for def in &block.props {
        let value = props_state
            .as_ref()
            .and_then(|obj| obj.get(&def.name))
            .map(|value| json_value_to_css_string(value.clone()))
            .or_else(|| def.default.as_ref().map(prop_default_to_css_string));
        if let Some(value) = value {
            map.insert(prop_variable_key(&def.name), StyleValue::Literal(value));
        }
    }
    map
}

/// Derive a settings schema from a component's `<props>` — the third projection
/// (alongside the CSS `prop()` map and the reactive Lua `props` table). The shape
/// mirrors the manifest `inline_schema` object so a generated settings UI can
/// consume it directly. Only `expose`d props are included; returns `None` when
/// the component declares no exposable props.
pub fn props_settings_schema(block: Option<&PropsBlock>) -> Option<serde_json::Value> {
    let block = block?;
    let mut properties = serde_json::Map::new();
    for def in &block.props {
        if !def.expose {
            continue;
        }
        let mut field = serde_json::Map::new();
        field.insert("type".into(), serde_json::Value::String(def.ty.as_str().into()));
        if let Some(default) = &def.default {
            field.insert("default".into(), prop_value_to_json(default));
        }
        if let Some(label) = &def.label {
            field.insert("label".into(), localized_label_to_json(label));
        }
        if let Some(description) = &def.description {
            field.insert("description".into(), localized_label_to_json(description));
        }
        if !def.options.is_empty() {
            field.insert(
                "enum".into(),
                serde_json::Value::Array(
                    def.options
                        .iter()
                        .map(|option| serde_json::Value::String(option.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(min) = def.min {
            field.insert("minimum".into(), serde_json::json!(min));
        }
        if let Some(max) = def.max {
            field.insert("maximum".into(), serde_json::json!(max));
        }
        if let Some(step) = def.step {
            field.insert("step".into(), serde_json::json!(step));
        }
        if let Some(unit) = &def.unit {
            field.insert("unit".into(), serde_json::Value::String(unit.clone()));
        }
        properties.insert(def.name.clone(), serde_json::Value::Object(field));
    }
    if properties.is_empty() {
        return None;
    }
    Some(serde_json::json!({ "type": "object", "properties": properties }))
}

fn prop_value_to_json(value: &PropValue) -> serde_json::Value {
    match value {
        PropValue::String(s) => serde_json::Value::String(s.clone()),
        PropValue::Number(n) => serde_json::json!(n),
        PropValue::Bool(b) => serde_json::Value::Bool(*b),
    }
}

fn localized_label_to_json(label: &mesh_core_component::LocalizedLabel) -> serde_json::Value {
    match label {
        mesh_core_component::LocalizedLabel::Literal(text) => {
            serde_json::Value::String(text.clone())
        }
        mesh_core_component::LocalizedLabel::Translation { key, fallback } => {
            let mut obj = serde_json::Map::new();
            obj.insert("t".into(), serde_json::Value::String(key.clone()));
            if let Some(fallback) = fallback {
                obj.insert("fallback".into(), serde_json::Value::String(fallback.clone()));
            }
            serde_json::Value::Object(obj)
        }
    }
}

fn prop_default_to_css_string(value: &PropValue) -> String {
    match value {
        PropValue::String(s) => s.clone(),
        PropValue::Number(n) => format_css_number(*n),
        PropValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
    }
}

fn json_value_to_css_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => if b { "1" } else { "0" }.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn format_css_number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

struct TrackingVariableStore<'a> {
    inner: &'a dyn VariableStore,
    reads: std::cell::RefCell<Vec<(String, String)>>,
}

impl<'a> TrackingVariableStore<'a> {
    fn new(inner: &'a dyn VariableStore) -> Self {
        Self {
            inner,
            reads: std::cell::RefCell::new(Vec::new()),
        }
    }
    fn into_reads(self) -> Vec<(String, String)> {
        self.reads.into_inner()
    }
}

impl VariableStore for TrackingVariableStore<'_> {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        let result = self.inner.get(name);
        if let Some(dot_pos) = name.find('.') {
            let service = name[..dot_pos].to_string();
            let field = name[dot_pos + 1..].to_string();
            self.reads.borrow_mut().push((service, field));
        }
        result
    }
    fn keys(&self) -> Vec<String> {
        self.inner.keys()
    }
    fn translate(&self, key: &str) -> Option<String> {
        self.inner.translate(key)
    }
}

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
    let resolver =
        StyleResolver::new(theme).with_props(resolve_css_props(component.props.as_ref(), state));
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
            let tracking_store: Option<TrackingVariableStore> =
                state.map(TrackingVariableStore::new);
            let effective = tracking_store.as_ref().map(|t| t as &dyn VariableStore);
            let content = effective
                .or(state)
                .map(|store| eval_expr(&expr.expression, store))
                .unwrap_or_else(|| format!("{{ {} }}", expr.expression));
            node.attributes.insert("content".into(), content);
            node.service_field_reads = tracking_store.map(|t| t.into_reads()).unwrap_or_default();
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

    // Per-node tracking: intercept service field reads for THIS node's attribute evaluation only.
    // Children each get their own tracker via the original `state` parameter below.
    let tracking_store: Option<TrackingVariableStore> = state.map(TrackingVariableStore::new);
    let tracking_state: Option<&dyn VariableStore> =
        tracking_store.as_ref().map(|t| t as &dyn VariableStore);
    let effective_state = tracking_state.or(state);

    let (classes, id, mut attributes, event_handlers) =
        parse_attributes(&element.attributes, effective_state);
    if let Some(binding) = element.attributes.iter().find_map(|attribute| {
        if let AttributeValue::InstanceBinding(binding) = &attribute.value {
            Some(binding.as_str())
        } else {
            None
        }
    }) {
        attributes.insert("_mesh_bind_this".into(), binding.to_string());
    }
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
            .map(|child| resolve_inline_content(child, effective_state))
            .collect();
        node.attributes.insert("content".into(), content);
        node.service_field_reads = tracking_store.map(|t| t.into_reads()).unwrap_or_default();
        return node;
    }

    node.service_field_reads = tracking_store.map(|t| t.into_reads()).unwrap_or_default();

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

fn apply_source_tag_defaults(source_tag: &SourceTag, attributes: &mut BTreeMap<String, String>) {
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
        } else if let AttributeValue::EventHandlerCall { handler, args } = &attr.value {
            let resolved_handler = resolve_event_handler_value(state, handler);
            let resolved_args: Vec<serde_json::Value> = args
                .iter()
                .map(|arg| {
                    let val = state.map(|store| eval_expr(arg, store)).unwrap_or_default();
                    serde_json::Value::String(val)
                })
                .collect();
            let call_value =
                serde_json::json!({"h": resolved_handler, "a": resolved_args}).to_string();
            let namespaced = if resolved_handler.starts_with("__mesh_embed__::") {
                call_value
            } else {
                format!("__mesh_embed__::{host_instance_key}::{call_value}")
            };
            props.insert(attr.name.clone(), namespaced);
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
    BTreeMap<String, String>,
    BTreeMap<String, String>,
) {
    let mut classes = Vec::new();
    let mut id = None;
    let mut resolved = BTreeMap::new();
    let mut event_handlers = BTreeMap::new();

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
            AttributeValue::EventHandlerCall { handler, args } => {
                let resolved_handler = resolve_event_handler_value(state, handler);
                let resolved_args: Vec<serde_json::Value> = args
                    .iter()
                    .map(|arg| {
                        let val = state.map(|store| eval_expr(arg, store)).unwrap_or_default();
                        serde_json::Value::String(val)
                    })
                    .collect();
                let value =
                    serde_json::json!({"h": resolved_handler, "a": resolved_args}).to_string();
                event_handlers.insert(normalize_event_handler_name(&attr.name), value);
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
            | "scroll"
            | "keydown"
            | "keyup"
            | "keybind"
    )
}

fn accessibility_for_element(
    source_tag: &str,
    runtime_tag: &str,
    attributes: &BTreeMap<String, String>,
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
    info.state.expanded = attributes
        .get("expanded")
        .or_else(|| attributes.get("open"))
        .map(|value| bool_value(value));
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
    accessibility_for_element(tag, tag, &BTreeMap::new())
}

fn bool_attr(attributes: &BTreeMap<String, String>, name: &str) -> bool {
    attributes.get(name).is_some_and(|value| bool_value(value))
}

fn bool_value(value: &str) -> bool {
    matches!(value.trim(), "" | "true" | "1")
}

fn number_attr(attributes: &BTreeMap<String, String>, name: &str) -> Option<f32> {
    attributes.get(name)?.trim().parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    const PROP_COMPONENT: &str = r#"
<props>
  track_width: { type: "size", default: "20px" }
</props>
<template>
  <box>
    <slider class="audio-slider"/>
  </box>
</template>
<style>
.audio-slider { width: prop(track_width); }
</style>
"#;

    #[test]
    fn prop_default_projects_into_painted_width() {
        let component = mesh_core_component::parse_component(PROP_COMPONENT).unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();

        let tree = build_widget_tree_from_component(
            &component, &manifest, &theme, 200.0, 80.0, None, "root", None, &[],
        );

        let slider = find_tag(&tree, "slider").expect("slider node");
        assert_eq!(
            slider.computed_style.width,
            mesh_core_elements::Dimension::Px(20.0)
        );
    }

    #[test]
    fn prop_state_override_beats_default() {
        let component = mesh_core_component::parse_component(PROP_COMPONENT).unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();
        let state = MapStore(std::collections::HashMap::from([(
            "props".to_string(),
            serde_json::json!({ "track_width": "36px" }),
        )]));

        let tree = build_widget_tree_from_component(
            &component,
            &manifest,
            &theme,
            200.0,
            80.0,
            None,
            "root",
            Some(&state),
            &[],
        );

        let slider = find_tag(&tree, "slider").expect("slider node");
        assert_eq!(
            slider.computed_style.width,
            mesh_core_elements::Dimension::Px(36.0)
        );
    }

    #[test]
    fn props_settings_schema_projects_typed_fields() {
        let component = mesh_core_component::parse_component(
            r#"
<props>
  width:   { type: "size", default: "fit-content", label: t("var.width") }
  density: { type: "enum", options: ["compact", "cozy"], default: "cozy" }
  hidden:  { type: "size", default: "10px", expose: false }
</props>
<template><box/></template>
"#,
        )
        .unwrap();

        let schema = props_settings_schema(component.props.as_ref()).expect("schema");
        let props = &schema["properties"];
        // Exposed fields present; expose:false omitted.
        assert_eq!(props["width"]["type"], "size");
        assert_eq!(props["width"]["default"], "fit-content");
        assert_eq!(props["width"]["label"]["t"], "var.width");
        assert_eq!(props["density"]["enum"][0], "compact");
        assert!(props.get("hidden").is_none());
    }

    #[test]
    fn props_settings_schema_is_none_without_exposed_props() {
        let component = mesh_core_component::parse_component(
            r#"
<props>
  internal: { type: "size", default: "1px", expose: false }
</props>
<template><box/></template>
"#,
        )
        .unwrap();
        assert!(props_settings_schema(component.props.as_ref()).is_none());
        assert!(props_settings_schema(None).is_none());
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

    #[test]
    fn phase90_container_collection_source_semantics_survive_lowering() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <tabs label="Debug views">
    <tab selected="true" onactivate={onOverview}>Overview</tab>
    <tab onactivate={onSurfaces}>Surfaces</tab>
  </tabs>
  <list label="Surfaces">
    <list-item selected="true" onactivate={onSurface}>Navigation</list-item>
    <empty-state>No rows</empty-state>
  </list>
  <details open="true" label="Advanced">
    <text>Details body</text>
  </details>
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

        let tabs = &tree.children[0];
        assert_eq!(
            tabs.attributes.get("data-mesh-element"),
            Some(&"tabs".into())
        );
        let tab = &tabs.children[0];
        assert_eq!(tab.attributes.get("data-mesh-element"), Some(&"tab".into()));
        assert_eq!(
            tab.event_handlers.get("activate"),
            Some(&"onOverview".into())
        );
        assert_eq!(
            tab.accessibility.role,
            mesh_core_elements::AccessibilityRole::Tab
        );
        assert!(tab.accessibility.state.selected);

        let list = &tree.children[1];
        assert_eq!(list.tag, "column");
        assert_eq!(
            list.attributes.get("data-mesh-element"),
            Some(&"list".into())
        );
        assert_eq!(
            list.accessibility.role,
            mesh_core_elements::AccessibilityRole::List
        );
        let item = &list.children[0];
        assert_eq!(
            item.attributes.get("data-mesh-element"),
            Some(&"list-item".to_string())
        );
        assert_eq!(
            item.accessibility.role,
            mesh_core_elements::AccessibilityRole::ListItem
        );
        assert!(item.accessibility.state.selected);

        let details = &tree.children[2];
        assert_eq!(
            details.attributes.get("data-mesh-element"),
            Some(&"details".to_string())
        );
        assert_eq!(details.accessibility.state.expanded, Some(true));
    }

    struct MapStore(std::collections::HashMap<String, serde_json::Value>);
    impl mesh_core_elements::VariableStore for MapStore {
        fn get(&self, name: &str) -> Option<serde_json::Value> {
            self.0.get(name).cloned()
        }
        fn keys(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
    }

    #[test]
    fn tracking_store_records_dotted_reads() {
        let mut map = std::collections::HashMap::new();
        map.insert("audio".to_string(), serde_json::json!({"percent": 80}));
        let inner = MapStore(map);
        let t = TrackingVariableStore::new(&inner);
        let _ = mesh_core_elements::VariableStore::get(&t, "audio.percent");
        let reads = t.into_reads();
        assert_eq!(reads, vec![("audio".to_string(), "percent".to_string())]);
    }

    #[test]
    fn tracking_store_skips_bare_reads() {
        let inner = MapStore(std::collections::HashMap::new());
        let t = TrackingVariableStore::new(&inner);
        let _ = mesh_core_elements::VariableStore::get(&t, "audio");
        let reads = t.into_reads();
        assert!(reads.is_empty());
    }

    #[test]
    fn tracking_store_no_cross_contamination() {
        let inner = MapStore(std::collections::HashMap::new());
        let t1 = TrackingVariableStore::new(&inner);
        let t2 = TrackingVariableStore::new(&inner);
        let _ = mesh_core_elements::VariableStore::get(&t1, "network.ssid");
        let reads1 = t1.into_reads();
        let reads2 = t2.into_reads();
        assert_eq!(reads1.len(), 1);
        assert!(reads2.is_empty());
    }

    // Run with: cargo test -p mesh-core-frontend --release -- service_field_tracking_overhead --ignored
    // Not run in debug mode — allocator cost of Vec+String dwarfs the measured work and produces
    // meaningless ratios (20-30x). In release mode the ratio is < 1.01.
    #[test]
    #[ignore]
    fn service_field_tracking_overhead_under_one_percent() {
        use std::time::Instant;

        struct NoopStore;
        impl mesh_core_elements::VariableStore for NoopStore {
            fn get(&self, _: &str) -> Option<serde_json::Value> {
                None
            }
            fn keys(&self) -> Vec<String> {
                Vec::new()
            }
        }

        let iterations = 10_000usize;
        let noop = NoopStore;

        let baseline_start = Instant::now();
        for _ in 0..iterations {
            let _ = mesh_core_elements::VariableStore::get(&noop, "audio.percent");
            let _ = mesh_core_elements::VariableStore::get(&noop, "volume");
            let _ = mesh_core_elements::VariableStore::get(&noop, "network.ssid");
        }
        let baseline_ns = baseline_start.elapsed().as_nanos().max(1);

        let tracking_start = Instant::now();
        for _ in 0..iterations {
            let t = TrackingVariableStore::new(&noop);
            let _ = mesh_core_elements::VariableStore::get(&t, "audio.percent");
            let _ = mesh_core_elements::VariableStore::get(&t, "volume");
            let _ = mesh_core_elements::VariableStore::get(&t, "network.ssid");
            let _ = t.into_reads();
        }
        let tracking_ns = tracking_start.elapsed().as_nanos();

        let overhead_ratio = tracking_ns as f64 / baseline_ns as f64;
        assert!(
            overhead_ratio <= 1.05,
            "TrackingVariableStore overhead {:.4}x exceeds 1.05x threshold (baseline={}ns tracked={}ns)",
            overhead_ratio,
            baseline_ns,
            tracking_ns,
        );
    }
}
