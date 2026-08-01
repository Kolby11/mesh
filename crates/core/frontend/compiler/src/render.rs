use crate::expr::eval_expr;
use crate::style::{
    InheritedStyleMask, child_style_context, inherit_text_style, inherited_style_mask, slot_style,
    synthetic_wrapper_style,
};
use crate::tags::lower_source_tag;
use crate::{FrontendCompositionResolver, LayeredStore};

use mesh_core_component::style::{StyleRule, StyleValue, prop_variable_key};
use mesh_core_component::template::{
    Attribute, AttributeValue, ComponentRef, ElementNode, ForNode, SourceTag, TemplateNode,
};
use mesh_core_component::{PropValue, PropsBlock};
use mesh_core_elements::accessibility::AccessibilityInfo;
use mesh_core_elements::{
    AttrKey, AttributeMap, ComponentCompositionProps, ComputedStyle, EventHandlerCall,
    HandlerTarget, StyleContext, StyleResolver, StyleRuleIndex, VariableStore, WidgetNode,
    element_contract_for_tag,
};
use mesh_core_module::Manifest;
use mesh_core_theme::Theme;
use serde_json;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub(crate) struct BuildStyleContext<'a, 'theme> {
    rules: &'a [StyleRule],
    index: BuildStyleRuleIndex<'a>,
    resolver: &'a StyleResolver<'theme>,
    namespace_handlers: bool,
}

enum BuildStyleRuleIndex<'a> {
    Owned(StyleRuleIndex),
    Borrowed(&'a StyleRuleIndex),
}

impl BuildStyleRuleIndex<'_> {
    fn as_ref(&self) -> &StyleRuleIndex {
        match self {
            Self::Owned(index) => index,
            Self::Borrowed(index) => index,
        }
    }
}

/// Owned, indexed style rules for a component embedded under a stable host.
///
/// Local component source and its host module's rules are immutable for the
/// lifetime of a compiled catalog. Preparing the combined rules once avoids
/// cloning both rule sets and rebuilding `StyleRuleIndex` on every cache miss.
#[derive(Debug)]
pub struct PreparedComponentStyleRules {
    rules: Vec<StyleRule>,
    index: StyleRuleIndex,
}

impl PreparedComponentStyleRules {
    pub fn new(component: &mesh_core_component::ComponentFile, host_rules: &[StyleRule]) -> Self {
        let component_rules = component
            .style
            .as_ref()
            .map(|style| style.rules.as_slice())
            .unwrap_or(&[]);
        let mut rules = Vec::with_capacity(host_rules.len() + component_rules.len());
        rules.extend_from_slice(host_rules);
        rules.extend_from_slice(component_rules);
        let index = StyleRuleIndex::new(&rules);
        Self { rules, index }
    }
}

impl<'a, 'theme> BuildStyleContext<'a, 'theme> {
    pub(crate) fn new(rules: &'a [StyleRule], resolver: &'a StyleResolver<'theme>) -> Self {
        Self {
            rules,
            index: BuildStyleRuleIndex::Owned(StyleRuleIndex::new(rules)),
            resolver,
            namespace_handlers: false,
        }
    }

    pub(crate) fn with_handler_namespacing(mut self, enabled: bool) -> Self {
        self.namespace_handlers = enabled;
        self
    }

    fn from_prepared(
        prepared: &'a PreparedComponentStyleRules,
        resolver: &'a StyleResolver<'theme>,
    ) -> Self {
        Self {
            rules: &prepared.rules,
            index: BuildStyleRuleIndex::Borrowed(&prepared.index),
            resolver,
            namespace_handlers: false,
        }
    }
}

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
    let Some(block) = block else {
        return HashMap::new();
    };
    let mut map = HashMap::with_capacity(block.props.len());
    // The shell publishes one `props` table in script state (the precedence-
    // resolved value per name); script writes round-trip back into it.
    let props_state_borrowed = state.and_then(|store| store.get_ref("props"));
    let props_state_owned = if props_state_borrowed.is_none() {
        state.and_then(|store| store.get("props"))
    } else {
        None
    };
    let props_state = props_state_borrowed.or(props_state_owned.as_ref());
    for def in &block.props {
        let value =
            props_state
                .and_then(|obj| obj.get(&def.name))
                .and_then(|value| {
                    mesh_core_component::json_to_prop_value_ref(value).and_then(|value| {
                        match mesh_core_component::prop_value_to_css(def, &value) {
                            Ok(css) => Some(css),
                            Err(err) => {
                                tracing::warn!(
                                    "invalid runtime value for prop `{}` ignored: {err}",
                                    def.name
                                );
                                None
                            }
                        }
                    })
                })
                .or_else(|| {
                    def.default.as_ref().and_then(|value| {
                        match mesh_core_component::prop_value_to_css(def, value) {
                            Ok(css) => Some(css),
                            Err(err) => {
                                tracing::warn!(
                                    "invalid default value for prop `{}` ignored: {err}",
                                    def.name
                                );
                                None
                            }
                        }
                    })
                });
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
        field.insert(
            "type".into(),
            serde_json::Value::String(def.ty.as_str().into()),
        );
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
    mesh_core_component::prop_value_to_json(value)
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
                obj.insert(
                    "fallback".into(),
                    serde_json::Value::String(fallback.clone()),
                );
            }
            serde_json::Value::Object(obj)
        }
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

    fn record_read(&self, name: &str) {
        let Some(dot_pos) = name.find('.') else {
            return;
        };
        let service = &name[..dot_pos];
        let field = &name[dot_pos + 1..];
        let mut reads = self.reads.borrow_mut();
        if reads.last().is_some_and(|(last_service, last_field)| {
            last_service == service && last_field == field
        }) {
            return;
        }
        if reads.iter().any(|(existing_service, existing_field)| {
            existing_service == service && existing_field == field
        }) {
            return;
        }
        reads.push((service.to_owned(), field.to_owned()));
    }
}

impl VariableStore for TrackingVariableStore<'_> {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        let result = self.inner.get(name);
        self.record_read(name);
        result
    }
    fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
        let result = self.inner.get_ref(name);
        self.record_read(name);
        result
    }
    fn keys(&self) -> Vec<String> {
        self.inner.keys()
    }
    fn translate(&self, key: &str) -> Option<String> {
        self.inner.translate(key)
    }
    fn template_locals(&self) -> serde_json::Map<String, serde_json::Value> {
        self.inner.template_locals()
    }
    fn record_template_service_reads(&self, reads: &[(String, String)]) {
        self.reads.borrow_mut().extend_from_slice(reads);
    }
}

fn evaluate_template_expression(
    expression: &str,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> serde_json::Value {
    if crate::expr::uses_translation(expression) {
        return state
            .map(|store| serde_json::Value::String(eval_expr(expression, store)))
            .unwrap_or(serde_json::Value::Null);
    }
    if let (Some(state), Some(composition)) = (state, composition)
        && let Some(result) = composition.evaluate_template_expression(
            instance_key,
            expression,
            &state.template_locals(),
        )
    {
        state.record_template_service_reads(&result.service_reads);
        return result.value;
    }
    state
        .map(|store| serde_json::Value::String(eval_expr(expression, store)))
        .unwrap_or(serde_json::Value::Null)
}

fn template_value_to_string(value: serde_json::Value) -> String {
    crate::expr::json_value_to_string(value)
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
    build_widget_tree_from_component_inner(
        component,
        host_manifest,
        theme,
        container_width,
        container_height,
        composition,
        instance_key,
        state,
        host_rules,
        None,
        false,
    )
}

/// Build a local component subtree for insertion into a composed surface.
///
/// Unlike the generic helper, event handlers are namespaced as they are
/// created so the shell does not need a second recursive tree pass.
pub fn build_embedded_widget_tree_from_component(
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
    build_widget_tree_from_component_inner(
        component,
        host_manifest,
        theme,
        container_width,
        container_height,
        composition,
        instance_key,
        state,
        host_rules,
        None,
        true,
    )
}

/// Build an embedded component using an already merged and indexed host/local
/// style-rule set.
#[allow(clippy::too_many_arguments)]
pub fn build_embedded_widget_tree_from_component_with_prepared_styles(
    component: &mesh_core_component::ComponentFile,
    host_manifest: &Manifest,
    theme: &Theme,
    container_width: f32,
    container_height: f32,
    composition: Option<&dyn FrontendCompositionResolver>,
    instance_key: &str,
    state: Option<&dyn VariableStore>,
    prepared_styles: &PreparedComponentStyleRules,
) -> WidgetNode {
    build_widget_tree_from_component_inner(
        component,
        host_manifest,
        theme,
        container_width,
        container_height,
        composition,
        instance_key,
        state,
        &[],
        Some(prepared_styles),
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_widget_tree_from_component_inner(
    component: &mesh_core_component::ComponentFile,
    host_manifest: &Manifest,
    theme: &Theme,
    container_width: f32,
    container_height: f32,
    composition: Option<&dyn FrontendCompositionResolver>,
    instance_key: &str,
    state: Option<&dyn VariableStore>,
    host_rules: &[mesh_core_component::style::StyleRule],
    prepared_styles: Option<&PreparedComponentStyleRules>,
    namespace_handlers: bool,
) -> WidgetNode {
    let resolver =
        StyleResolver::new(theme).with_props(resolve_css_props(component.props.as_ref(), state));
    let component_rules = component
        .style
        .as_ref()
        .map(|style| style.rules.as_slice())
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
        let build_style = match prepared_styles {
            Some(prepared) => BuildStyleContext::from_prepared(prepared, &resolver),
            None => BuildStyleContext::new(rules, &resolver),
        }
        .with_handler_namespacing(namespace_handlers);
        let children: Vec<WidgetNode> = template
            .root
            .iter()
            .map(|node| {
                build_widget_node(
                    node,
                    host_manifest,
                    &build_style,
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
        container.children = children.into();
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
    build_style: &BuildStyleContext<'_, '_>,
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
            build_style,
            parent_style,
            container_context,
            state,
            instance_key,
            composition,
        ),
        TemplateNode::Component(component) => build_component_ref(
            component,
            manifest,
            build_style,
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
            node.computed_style = build_style.resolver.resolve_node_style_for_module_indexed(
                build_style.rules,
                build_style.index.as_ref(),
                "text",
                &[],
                None,
                container_context,
                Default::default(),
                Some(&manifest.package.id),
            );
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
                .map(|store| {
                    template_value_to_string(evaluate_template_expression(
                        &expr.expression,
                        Some(store),
                        instance_key,
                        composition,
                    ))
                })
                .unwrap_or_else(|| format!("{{ {} }}", expr.expression));
            node.attributes.insert("content".into(), content);
            node.service_field_reads = tracking_store.map(|t| t.into_reads()).unwrap_or_default();
            node.computed_style = build_style.resolver.resolve_node_style_for_module_indexed(
                build_style.rules,
                build_style.index.as_ref(),
                "text",
                &[],
                None,
                container_context,
                Default::default(),
                Some(&manifest.package.id),
            );
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
                Some(store) => !matches!(
                    evaluate_template_expression(
                        &if_node.condition,
                        Some(store),
                        instance_key,
                        composition,
                    ),
                    serde_json::Value::Null | serde_json::Value::Bool(false)
                ),
                None => true,
            };
            let active_children = if show_then {
                &if_node.then_children
            } else {
                &if_node.else_children
            };
            let mut node = WidgetNode::new("column");
            attach_module_id(&mut node, &manifest.package.id);
            node.computed_style = synthetic_wrapper_style();
            let child_context = child_style_context(&node.computed_style, container_context);
            node.children = active_children
                .iter()
                .map(|child| {
                    build_widget_node(
                        child,
                        manifest,
                        build_style,
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
            node.computed_style = synthetic_wrapper_style();
            let child_context = child_style_context(&node.computed_style, container_context);

            if let Some(store) = state {
                if let Some(composition) = composition {
                    let iterable = evaluate_template_expression(
                        &for_node.iterable,
                        Some(store),
                        instance_key,
                        Some(composition),
                    );
                    if let serde_json::Value::Array(items) = iterable {
                        node.children.extend(build_for_children(
                            &items,
                            for_node,
                            manifest,
                            build_style,
                            &node.computed_style,
                            child_context,
                            store,
                            instance_key,
                            Some(composition),
                        ));
                    }
                } else {
                    let borrowed_items = store.get_ref(&for_node.iterable).and_then(|value| {
                        if let serde_json::Value::Array(items) = value {
                            Some(items.as_slice())
                        } else {
                            None
                        }
                    });
                    if let Some(items) = borrowed_items {
                        node.children.extend(build_for_children(
                            items,
                            for_node,
                            manifest,
                            build_style,
                            &node.computed_style,
                            child_context,
                            store,
                            instance_key,
                            composition,
                        ));
                    } else {
                        let iterable = store
                            .get(&for_node.iterable)
                            .unwrap_or(serde_json::Value::Null);
                        if let serde_json::Value::Array(items) = iterable {
                            node.children.extend(build_for_children(
                                &items,
                                for_node,
                                manifest,
                                build_style,
                                &node.computed_style,
                                child_context,
                                store,
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
                node.children = children.into();
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

fn build_for_children<'items, I>(
    items: I,
    for_node: &ForNode,
    manifest: &Manifest,
    build_style: &BuildStyleContext<'_, '_>,
    parent_style: &ComputedStyle,
    child_context: StyleContext,
    store: &dyn VariableStore,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> Vec<WidgetNode>
where
    I: IntoIterator<Item = &'items serde_json::Value>,
    I::IntoIter: ExactSizeIterator,
{
    // `WidgetNode` is a large struct, so growing this vector re-copies every
    // node built so far. The exact child count is known before the first push.
    let items = items.into_iter();
    let mut children = Vec::with_capacity(items.len().saturating_mul(for_node.children.len()));
    for item_val in items {
        let item_store = LayeredStore {
            base: store,
            item_name: &for_node.item_name,
            item_value: item_val,
        };
        for child in &for_node.children {
            children.push(build_widget_node(
                child,
                manifest,
                build_style,
                Some(parent_style),
                child_context,
                Some(&item_store as &dyn VariableStore),
                instance_key,
                composition,
            ));
        }
    }
    children
}

fn build_element_node(
    element: &ElementNode,
    manifest: &Manifest,
    build_style: &BuildStyleContext<'_, '_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> WidgetNode {
    let source_tag = element.tag.as_str();
    // Runtime tags are static strings; keep them borrowed and let the node own
    // the single allocation instead of building an owned copy to clone from.
    let tag: &str = lower_source_tag(&element.tag_kind).as_str();

    // Per-node tracking: intercept service field reads for THIS node's attribute evaluation only.
    // Children each get their own tracker via the original `state` parameter below.
    let tracking_store: Option<TrackingVariableStore> = state.map(TrackingVariableStore::new);
    let tracking_state: Option<&dyn VariableStore> =
        tracking_store.as_ref().map(|t| t as &dyn VariableStore);
    let effective_state = tracking_state.or(state);

    let (classes, id, mut attributes, event_handlers, event_handler_calls) =
        parse_attributes_runtime(
            &element.attributes,
            effective_state,
            instance_key,
            composition,
            build_style.namespace_handlers,
        );
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
    let resolved_classes = attributes.get("class").map(|value| {
        value
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>()
    });
    let style_classes = resolved_classes.as_deref().unwrap_or(&classes);
    let style_id = id
        .as_deref()
        .or_else(|| attributes.get("id").map(String::as_str));
    let inherited_mask = inherited_style_mask(
        build_style.rules,
        tag,
        style_classes,
        style_id,
        container_context,
    );

    let computed_style = build_style
        .resolver
        .resolve_node_style_for_module_indexed_with_inline_style(
            build_style.rules,
            build_style.index.as_ref(),
            tag,
            style_classes,
            style_id,
            attributes.get("style").map(String::as_str),
            container_context,
            Default::default(),
            Some(&manifest.package.id),
        );

    let mut node = WidgetNode::new(tag);
    attach_module_id(&mut node, &manifest.package.id);
    node.attributes = attributes;
    node.event_handlers = event_handlers;
    node.event_handler_calls = event_handler_calls;
    node.computed_style = computed_style;
    if let Some(parent_style) = parent_style {
        inherit_text_style(&mut node.computed_style, parent_style, inherited_mask);
    }
    node.accessibility = accessibility_for_element(source_tag, tag, &node.attributes);

    if let Some(id) = id {
        node.attributes.insert("id".into(), id);
    }
    if !classes.is_empty() && !node.attributes.contains_key("class") {
        node.attributes.insert("class".into(), classes.join(" "));
    }

    if tag == "text"
        && !element.children.is_empty()
        && element.children.iter().all(is_inline_template_node)
    {
        let content: String = element
            .children
            .iter()
            .map(|child| resolve_inline_content(child, effective_state, instance_key, composition))
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
                build_style,
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

fn apply_source_tag_defaults(source_tag: &SourceTag, attributes: &mut AttributeMap) {
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

fn resolve_inline_content(
    node: &TemplateNode,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> String {
    match node {
        TemplateNode::Text(text) => text.content.clone(),
        TemplateNode::Expr(expr) => state
            .map(|store| {
                template_value_to_string(evaluate_template_expression(
                    &expr.expression,
                    Some(store),
                    instance_key,
                    composition,
                ))
            })
            .unwrap_or_else(|| format!("{{ {} }}", expr.expression)),
        _ => String::new(),
    }
}

fn build_component_ref(
    component: &ComponentRef,
    manifest: &Manifest,
    build_style: &BuildStyleContext<'_, '_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    host_instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> WidgetNode {
    let (_, _, props, _, parsed_handler_calls) = parse_attributes_runtime(
        &component.props,
        state,
        host_instance_key,
        composition,
        false,
    );
    let mut composition_props = ComponentCompositionProps {
        values: props,
        ..ComponentCompositionProps::default()
    };
    let mut prop_handler_calls = BTreeMap::new();
    for attr in &component.props {
        if let AttributeValue::EventHandler(handler) = &attr.value {
            let mut target =
                HandlerTarget::from_legacy_serialized(resolve_event_handler_value(state, handler));
            target.namespace(host_instance_key);
            composition_props.values.insert(
                AttrKey::new(&attr.name),
                component_prop_handler_token(host_instance_key, &attr.name),
            );
            prop_handler_calls.insert(
                attr.name.clone(),
                EventHandlerCall {
                    handler: target,
                    args: Vec::new(),
                },
            );
        } else if matches!(attr.value, AttributeValue::EventHandlerCall { .. }) {
            // Attribute parsing normalizes event-looking names (`onselect` →
            // `select`) for element dispatch. Component props must retain the
            // authored prop name so two props bound to the same handler remain
            // distinguishable at the composition boundary.
            let event_name = normalize_event_handler_name(&attr.name);
            if let Some(call) = parsed_handler_calls.get(&event_name) {
                let mut call = call.clone();
                call.handler.namespace(host_instance_key);
                composition_props.values.insert(
                    AttrKey::new(&attr.name),
                    component_prop_handler_token(host_instance_key, &attr.name),
                );
                prop_handler_calls.insert(attr.name.clone(), call);
            }
        } else if let AttributeValue::Binding(binding) = &attr.value {
            composition_props
                .bindings
                .insert(AttrKey::new(&attr.name), binding.clone());
        } else if let AttributeValue::InstanceBinding(binding) = &attr.value {
            composition_props.bind_this = Some(binding.clone());
        }
    }
    if let Some(composition) = composition {
        if let Some(node) = composition.render_import(
            manifest,
            host_instance_key,
            &component.name,
            component.source_ordinal,
            component.duplicate_ordinal,
            component.repeated_by_loop,
            &composition_props,
            &prop_handler_calls,
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
        build_style,
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

/// Opaque per-prop value used while rendering a component subtree.
///
/// Handler props are represented as strings in the child script state. A
/// token keeps two props that target the same handler distinct until the shell
/// attaches their typed call arguments, at which point it is replaced by the
/// real namespaced handler and never reaches dispatch.
fn component_prop_handler_token(host_instance_key: &str, prop_name: &str) -> String {
    let mut token = String::with_capacity(
        "__mesh_prop_handler__::".len() + host_instance_key.len() + 2 + prop_name.len(),
    );
    token.push_str("__mesh_prop_handler__::");
    token.push_str(host_instance_key);
    token.push_str("::");
    token.push_str(prop_name);
    token
}

thread_local! {
    /// Module identities seen while building trees on this thread, most
    /// recently used first. A tree draws from very few modules (its own, plus
    /// one per embedded component), so a short move-to-front list resolves
    /// every node after the first.
    static SHARED_MODULE_IDS: RefCell<Vec<Arc<str>>> = const { RefCell::new(Vec::new()) };
}

const SHARED_MODULE_ID_CACHE_LIMIT: usize = 16;

/// Resolve a module id to a shared allocation.
///
/// Every node built from one module carries the same identity string, so
/// cloning an `Arc` here replaces one malloc-and-copy per node with a
/// refcount bump. The lookup is a short string comparison, not a hash.
fn shared_module_id(module_id: &str) -> Arc<str> {
    SHARED_MODULE_IDS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(position) = cache.iter().position(|entry| entry.as_ref() == module_id) {
            if position != 0 {
                cache.swap(0, position);
            }
            return cache[0].clone();
        }
        let shared: Arc<str> = Arc::from(module_id);
        if cache.len() >= SHARED_MODULE_ID_CACHE_LIMIT {
            cache.pop();
        }
        cache.insert(0, shared.clone());
        shared
    })
}

fn attach_module_id(node: &mut WidgetNode, module_id: &str) {
    node.set_module_id(shared_module_id(module_id));
}

#[cfg(test)]
pub(crate) fn parse_attributes(
    attrs: &[Attribute],
    state: Option<&dyn VariableStore>,
) -> (
    Vec<String>,
    Option<String>,
    AttributeMap,
    BTreeMap<String, HandlerTarget>,
    BTreeMap<String, EventHandlerCall>,
) {
    parse_attributes_runtime(attrs, state, "", None, false)
}

fn parse_attributes_runtime(
    attrs: &[Attribute],
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
    namespace_handlers: bool,
) -> (
    Vec<String>,
    Option<String>,
    AttributeMap,
    BTreeMap<String, HandlerTarget>,
    BTreeMap<String, EventHandlerCall>,
) {
    let mut classes = Vec::new();
    let mut id = None;
    // One allocation for the whole map: the source attributes plus the handful
    // the builder adds afterwards (`data-mesh-element`, source-tag defaults,
    // `class`/`id` write-back, `content`).
    let mut resolved = AttributeMap::with_capacity(attrs.len() + 4);
    let mut event_handlers = BTreeMap::new();
    let mut event_handler_calls = BTreeMap::new();

    for attr in attrs {
        match &attr.value {
            AttributeValue::Static(value) => {
                if attr.name == "class" {
                    classes.extend(value.split_whitespace().map(str::to_string));
                } else if attr.name == "id" {
                    id = Some(value.clone());
                } else {
                    resolved.insert(AttrKey::new(&attr.name), value.clone());
                }
            }
            AttributeValue::Binding(binding) | AttributeValue::TwoWayBinding(binding) => {
                if is_event_handler_attribute(&attr.name) {
                    let handler = resolve_event_handler_value(state, binding);
                    event_handlers.insert(
                        normalize_event_handler_name(&attr.name),
                        namespace_handler_if_needed(instance_key, handler, namespace_handlers),
                    );
                    continue;
                }
                let value = state
                    .map(|store| {
                        evaluate_template_expression(
                            binding,
                            Some(store),
                            instance_key,
                            composition,
                        )
                    })
                    .unwrap_or(serde_json::Value::Null);
                resolved.insert_value(AttrKey::new(&attr.name), value);
            }
            AttributeValue::InstanceBinding(_) => {}
            AttributeValue::EventHandler(handler) => {
                let resolved_handler = resolve_event_handler_value(state, handler);
                event_handlers.insert(
                    normalize_event_handler_name(&attr.name),
                    namespace_handler_if_needed(instance_key, resolved_handler, namespace_handlers),
                );
            }
            AttributeValue::EventHandlerCall { handler, args } => {
                let resolved_handler = namespace_handler_if_needed(
                    instance_key,
                    resolve_event_handler_value(state, handler),
                    namespace_handlers,
                );
                let resolved_args: Vec<serde_json::Value> = args
                    .iter()
                    .map(|arg| {
                        state
                            .map(|store| {
                                evaluate_template_expression(
                                    arg,
                                    Some(store),
                                    instance_key,
                                    composition,
                                )
                            })
                            .unwrap_or(serde_json::Value::Null)
                    })
                    .collect();
                event_handler_calls.insert(
                    normalize_event_handler_name(&attr.name),
                    EventHandlerCall {
                        handler: resolved_handler.clone(),
                        args: resolved_args,
                    },
                );
                event_handlers.insert(normalize_event_handler_name(&attr.name), resolved_handler);
            }
        }
    }

    (classes, id, resolved, event_handlers, event_handler_calls)
}

fn namespace_handler_if_needed(
    instance_key: &str,
    handler: String,
    namespace_handlers: bool,
) -> HandlerTarget {
    let mut target = HandlerTarget::from_legacy_serialized(handler);
    if namespace_handlers {
        target.namespace(instance_key);
    }
    target
}

fn resolve_event_handler_value(state: Option<&dyn VariableStore>, handler: &str) -> String {
    state
        .and_then(|store| match store.get_ref(handler) {
            Some(value) => value.as_str().map(str::to_string),
            None => store
                .get(handler)
                .and_then(|value| value.as_str().map(str::to_string)),
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| handler.to_string())
}

fn normalize_event_handler_name(name: &str) -> String {
    normalized_event_handler_name(name).to_owned()
}

fn normalized_event_handler_name(name: &str) -> &str {
    name.strip_prefix("on").unwrap_or(name)
}

fn is_event_handler_attribute(name: &str) -> bool {
    if !name.starts_with("on") {
        return false;
    }
    matches!(
        normalized_event_handler_name(name),
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
            | "twofingerscroll"
            | "swipe"
            | "pinch"
            | "hold"
            | "touchstart"
            | "touchmove"
            | "touchend"
            | "touchcancel"
            | "tap"
            | "doubletap"
            | "longpress"
            | "keydown"
            | "keyup"
            | "keybind"
    )
}

fn accessibility_for_element(
    source_tag: &str,
    runtime_tag: &str,
    attributes: &AttributeMap,
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
    // Synthetic layout wrappers ({#if}/{#for} column nodes) and many plain
    // structural elements carry no attributes at all; every field below
    // already defaults to what an all-`None`/`false` attribute lookup chain
    // would produce, so skip the ~15 BTreeMap probes entirely in that case.
    if attributes.is_empty() {
        return info;
    }

    // One ordered pass over the node's own attributes instead of ~19 map
    // descents for keys that are mostly absent: real elements carry a handful
    // of attributes, so walking them and matching the key is cheaper than
    // searching the tree once per accessibility field. Alternative spellings
    // are collected separately and resolved by precedence afterwards.
    let mut aria_label = None;
    let mut label = None;
    let mut alt = None;
    let mut title = None;
    let mut tooltip = None;
    let mut key = None;
    let mut keybind = None;
    let mut shortcut = None;
    let mut expanded = None;
    let mut open = None;
    for (name, value) in attributes.iter_values() {
        match name.as_str() {
            "aria-label" => aria_label = Some(value),
            "label" => label = Some(value),
            "alt" => alt = Some(value),
            "title" => title = Some(value),
            "tooltip" => tooltip = Some(value),
            "key" => key = Some(value),
            "keybind" => keybind = Some(value),
            "shortcut" => shortcut = Some(value),
            "expanded" => expanded = Some(value),
            "open" => open = Some(value),
            "disabled" => info.state.disabled = value.legacy_bool(),
            "checked" => info.state.checked = Some(value.legacy_bool()),
            "selected" => info.state.selected = value.legacy_bool(),
            "pressed" => info.state.pressed = value.legacy_bool(),
            "busy" => info.state.busy = value.legacy_bool(),
            "invalid" => info.state.invalid = value.legacy_bool(),
            "required" => info.state.required = value.legacy_bool(),
            "value" => info.state.value = Some(value.to_legacy_string()),
            "min" => info.state.value_min = value.parse_f32(),
            "max" => info.state.value_max = value.parse_f32(),
            _ => {}
        }
    }
    info.label = aria_label
        .or(label)
        .or(alt)
        .map(|value| value.to_legacy_string());
    info.description = title.or(tooltip).map(|value| value.to_legacy_string());
    info.keyboard_shortcut = key
        .or(keybind)
        .or(shortcut)
        .map(|value| value.to_legacy_string());
    info.state.expanded = expanded.or(open).map(|value| value.legacy_bool());
    info
}

#[cfg(test)]
fn accessibility_for_tag(tag: &str) -> AccessibilityInfo {
    accessibility_for_element(tag, tag, &AttributeMap::new())
}

/// Pre-guard behavior: always runs the full attribute lookup chain, even for
/// an empty attribute map. Kept only for the release benchmark comparison
/// below; production code takes the early-return path in
/// `accessibility_for_element`.
#[cfg(test)]
fn accessibility_for_element_unguarded(
    source_tag: &str,
    runtime_tag: &str,
    attributes: &AttributeMap,
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
fn bool_attr(attributes: &AttributeMap, name: &str) -> bool {
    attributes.get(name).is_some_and(|value| bool_value(value))
}

#[cfg(test)]
fn bool_value(value: &str) -> bool {
    matches!(value.trim(), "" | "true" | "1")
}

#[cfg(test)]
fn number_attr(attributes: &AttributeMap, name: &str) -> Option<f32> {
    attributes.get(name)?.trim().parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TemplateExpressionResult;

    struct TranslatingStore;

    impl VariableStore for TranslatingStore {
        fn get(&self, _name: &str) -> Option<serde_json::Value> {
            None
        }

        fn keys(&self) -> Vec<String> {
            Vec::new()
        }

        fn translate(&self, key: &str) -> Option<String> {
            (key == "nav.open_settings").then(|| "Open settings".to_string())
        }
    }

    struct IdentityTranslationComposition;

    impl FrontendCompositionResolver for IdentityTranslationComposition {
        fn evaluate_template_expression(
            &self,
            _instance_key: &str,
            expression: &str,
            _locals: &serde_json::Map<String, serde_json::Value>,
        ) -> Option<TemplateExpressionResult> {
            Some(TemplateExpressionResult {
                value: serde_json::Value::String(
                    expression
                        .strip_prefix("t('")
                        .and_then(|value| value.strip_suffix("')"))
                        .unwrap_or(expression)
                        .to_string(),
                ),
                service_reads: Vec::new(),
            })
        }

        fn render_import(
            &self,
            _host: &Manifest,
            _host_instance_key: &str,
            _alias: &str,
            _source_ordinal: usize,
            _duplicate_ordinal: Option<usize>,
            _repeated_by_loop: bool,
            _props: &ComponentCompositionProps,
            _prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
            _container_width: f32,
            _container_height: f32,
        ) -> Option<WidgetNode> {
            None
        }

        fn render_slot(
            &self,
            _host: &Manifest,
            _host_instance_key: &str,
            _slot_name: Option<&str>,
            _container_width: f32,
            _container_height: f32,
        ) -> Vec<WidgetNode> {
            Vec::new()
        }
    }

    struct TypedExpressionComposition;

    impl FrontendCompositionResolver for TypedExpressionComposition {
        fn evaluate_template_expression(
            &self,
            _instance_key: &str,
            expression: &str,
            _locals: &serde_json::Map<String, serde_json::Value>,
        ) -> Option<TemplateExpressionResult> {
            let value = match expression {
                "enabled" => serde_json::json!(true),
                "minimum" => serde_json::json!(1.5),
                "metadata" => serde_json::json!({"source": "runtime"}),
                _ => serde_json::Value::Null,
            };
            Some(TemplateExpressionResult {
                value,
                service_reads: Vec::new(),
            })
        }

        fn render_import(
            &self,
            _host: &Manifest,
            _host_instance_key: &str,
            _alias: &str,
            _source_ordinal: usize,
            _duplicate_ordinal: Option<usize>,
            _repeated_by_loop: bool,
            _props: &ComponentCompositionProps,
            _prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
            _container_width: f32,
            _container_height: f32,
        ) -> Option<WidgetNode> {
            None
        }

        fn render_slot(
            &self,
            _host: &Manifest,
            _host_instance_key: &str,
            _slot_name: Option<&str>,
            _container_width: f32,
            _container_height: f32,
        ) -> Vec<WidgetNode> {
            Vec::new()
        }
    }

    #[test]
    fn translated_expression_uses_locale_store_before_composition_runtime() {
        let value = evaluate_template_expression(
            "t('nav.open_settings')",
            Some(&TranslatingStore),
            "test",
            Some(&IdentityTranslationComposition),
        );

        assert_eq!(value, serde_json::json!("Open settings"));
    }

    #[test]
    fn component_handler_calls_preserve_authored_prop_identity() {
        #[derive(Default)]
        struct CapturingComposition {
            props: std::cell::RefCell<ComponentCompositionProps>,
            calls: std::cell::RefCell<BTreeMap<String, EventHandlerCall>>,
        }

        impl FrontendCompositionResolver for CapturingComposition {
            fn evaluate_template_expression(
                &self,
                _instance_key: &str,
                _expression: &str,
                _locals: &serde_json::Map<String, serde_json::Value>,
            ) -> Option<TemplateExpressionResult> {
                None
            }

            fn render_import(
                &self,
                _host: &Manifest,
                _host_instance_key: &str,
                _alias: &str,
                _source_ordinal: usize,
                _duplicate_ordinal: Option<usize>,
                _repeated_by_loop: bool,
                props: &ComponentCompositionProps,
                prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
                _container_width: f32,
                _container_height: f32,
            ) -> Option<WidgetNode> {
                self.props.replace(props.clone());
                self.calls.replace(prop_handler_calls.clone());
                Some(WidgetNode::new("box"))
            }

            fn render_slot(
                &self,
                _host: &Manifest,
                _host_instance_key: &str,
                _slot_name: Option<&str>,
                _container_width: f32,
                _container_height: f32,
            ) -> Vec<WidgetNode> {
                Vec::new()
            }
        }

        let component = mesh_core_component::parse_component(
            r#"
<template>
  <Child hidden="{isHidden}" bind:this={child}
         onprimary={onShared("primary")} onsecondary={onShared("secondary")} />
</template>
<script lang="luau">
import Child from "./child.mesh"
</script>
"#,
        )
        .unwrap();
        let store = MapStore(
            [
                ("onShared".to_string(), serde_json::json!("onShared")),
                ("isHidden".to_string(), serde_json::json!(true)),
            ]
            .into_iter()
            .collect(),
        );
        let composition = CapturingComposition::default();

        build_widget_tree_from_component(
            &component,
            &test_manifest(),
            &mesh_core_theme::default_theme(),
            200.0,
            80.0,
            Some(&composition),
            "root",
            Some(&store),
            &[],
        );

        let props = composition.props.borrow();
        assert!(props.values.contains_key("onprimary"));
        assert!(props.values.contains_key("onsecondary"));
        assert_ne!(
            props.values.get("onprimary"),
            props.values.get("onsecondary")
        );
        assert_eq!(
            props.bindings.get("hidden").map(String::as_str),
            Some("isHidden")
        );
        assert_eq!(props.bind_this.as_deref(), Some("child"));
        assert!(
            props
                .values
                .keys()
                .all(|key| !key.as_str().starts_with("__mesh_"))
        );
        let calls = composition.calls.borrow();
        assert_eq!(calls["onprimary"].handler, "onShared");
        assert_eq!(calls["onprimary"].handler.instance_key(), Some("root"));
        assert_eq!(calls["onsecondary"].handler, calls["onprimary"].handler);
        assert_eq!(calls["onprimary"].args, vec![serde_json::json!("primary")]);
        assert_eq!(
            calls["onsecondary"].args,
            vec![serde_json::json!("secondary")]
        );
    }

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
            keybinds: Default::default(),
            i18n: None,
            theme: None,
            service: None,
            provides: vec![],
            interface: None,
            interfaces: Vec::new(),
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
    fn prepared_component_styles_match_per_build_rule_merging() {
        let component = mesh_core_component::parse_component(
            r#"
<template><box class="target" /></template>
<style>.target { height: 12px; color: #abcdef; }</style>
"#,
        )
        .unwrap();
        let host = mesh_core_component::parse_component(
            r#"
<template><box /></template>
<style>.target { width: 34px; padding: 2px; }</style>
"#,
        )
        .unwrap();
        let host_rules = &host.style.as_ref().unwrap().rules;
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();

        let legacy = build_embedded_widget_tree_from_component(
            &component,
            &manifest,
            &theme,
            200.0,
            80.0,
            None,
            "root/local:target",
            None,
            host_rules,
        );
        let prepared = PreparedComponentStyleRules::new(&component, host_rules);
        let cached = build_embedded_widget_tree_from_component_with_prepared_styles(
            &component,
            &manifest,
            &theme,
            200.0,
            80.0,
            None,
            "root/local:target",
            None,
            &prepared,
        );

        assert_eq!(cached.tag, legacy.tag);
        assert_eq!(cached.attributes, legacy.attributes);
        assert_eq!(cached.children.len(), legacy.children.len());
        let cached_target = cached.children.first().expect("prepared target");
        let legacy_target = legacy.children.first().expect("legacy target");
        assert_eq!(cached_target.tag, legacy_target.tag);
        assert_eq!(cached_target.attributes, legacy_target.attributes);
        assert_eq!(
            format!("{:?}", cached_target.computed_style),
            format!("{:?}", legacy_target.computed_style)
        );
    }

    // cargo test -p mesh-core-frontend --release -- prepared_component_style_rules_avoid_remerge_and_reindex --ignored --nocapture
    #[test]
    #[ignore = "release-only prepared component-style benchmark"]
    fn prepared_component_style_rules_avoid_remerge_and_reindex() {
        use std::time::Instant;

        let rules = (0..32)
            .map(|index| {
                format!(
                    ".item-{index} {{ width: {}px; height: {}px; color: #abcdef; }}",
                    index + 1,
                    index + 2
                )
            })
            .collect::<String>();
        let component = mesh_core_component::parse_component(&format!(
            "<template><box class=\"item-31\" /></template><style>{rules}</style>"
        ))
        .unwrap();
        let host = mesh_core_component::parse_component(&format!(
            "<template><box /></template><style>{rules}</style>"
        ))
        .unwrap();
        let host_rules = &host.style.as_ref().unwrap().rules;
        let iterations = 20_000usize;

        let rebuild_started = Instant::now();
        let mut rebuilt_rules = 0usize;
        for _ in 0..iterations {
            let prepared = PreparedComponentStyleRules::new(
                std::hint::black_box(&component),
                std::hint::black_box(host_rules),
            );
            rebuilt_rules += std::hint::black_box(prepared.rules.len());
        }
        let rebuild_time = rebuild_started.elapsed();

        let prepared = PreparedComponentStyleRules::new(&component, host_rules);
        let reuse_started = Instant::now();
        let mut reused_rules = 0usize;
        for _ in 0..iterations {
            reused_rules += std::hint::black_box(&prepared).rules.len();
        }
        let reuse_time = reuse_started.elapsed();

        eprintln!(
            "component style preparation: rebuild {rebuild_time:?}; cached reuse {reuse_time:?}; ratio {:.1}x; rules={rebuilt_rules}/{reused_rules}",
            rebuild_time.as_secs_f64() / reuse_time.as_secs_f64()
        );
        assert_eq!(rebuilt_rules, reused_rules);
        assert!(reuse_time < rebuild_time);
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
    fn dynamic_class_participates_in_initial_style_resolution() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <box class="{active_class}"/>
</template>
<style>
.active { width: 42px; }
</style>
"#,
        )
        .unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();
        let state = MapStore(std::collections::HashMap::from([(
            "active_class".to_string(),
            serde_json::json!("active"),
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

        let active_box = tree.children.first().expect("template root");
        assert_eq!(
            active_box.attributes.get("class"),
            Some(&"active".to_string())
        );
        assert_eq!(
            active_box.computed_style.width,
            mesh_core_elements::Dimension::Px(42.0)
        );
    }

    #[test]
    fn dynamic_inline_style_overrides_stylesheet_layout() {
        let component = mesh_core_component::parse_component(
            r#"
<template>
  <box class="positioned" style={position_style}/>
</template>
<style>
.positioned {
  position: absolute;
  left: 1px;
  top: 2px;
}
</style>
"#,
        )
        .unwrap();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();
        let state = MapStore(std::collections::HashMap::from([(
            "position_style".to_string(),
            serde_json::json!("left: 38px; top: 32px;"),
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

        let positioned = tree.children.first().expect("template root");
        assert_eq!(
            positioned.attributes.get("style").map(String::as_str),
            Some("left: 38px; top: 32px;")
        );
        assert_eq!(positioned.computed_style.inset_left, Some(38.0));
        assert_eq!(positioned.computed_style.inset_top, Some(32.0));
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

        let (_, _, _, handlers, _) = parse_attributes(&attrs, None);

        assert_eq!(
            handlers.get("input").map(HandlerTarget::as_str),
            Some("onInput")
        );
        assert_eq!(
            handlers.get("change").map(HandlerTarget::as_str),
            Some("onChange")
        );
        assert_eq!(
            handlers.get("select").map(HandlerTarget::as_str),
            Some("onSelect")
        );
        assert_eq!(
            handlers.get("activate").map(HandlerTarget::as_str),
            Some("onActivate")
        );
        assert_eq!(
            handlers.get("openchange").map(HandlerTarget::as_str),
            Some("onOpenChange")
        );
    }

    #[test]
    fn event_handler_call_attrs_store_typed_args_not_json_handler_string() {
        let attrs = vec![Attribute {
            name: "onclick".into(),
            value: AttributeValue::EventHandlerCall {
                handler: "selectItem".into(),
                args: vec!["item_id".into(), "\"fallback\"".into()],
            },
        }];
        let store = MapStore(
            [
                ("item_id".to_string(), serde_json::json!("alpha")),
                ("selectItem".to_string(), serde_json::json!("onSelectItem")),
            ]
            .into_iter()
            .collect(),
        );

        let (_, _, _, handlers, handler_calls) = parse_attributes(&attrs, Some(&store));

        assert_eq!(
            handlers.get("click").map(HandlerTarget::as_str),
            Some("onSelectItem")
        );
        let call = handler_calls.get("click").expect("typed call");
        assert_eq!(call.handler, "onSelectItem");
        assert_eq!(
            call.args,
            vec![
                serde_json::Value::String("alpha".into()),
                serde_json::Value::String("fallback".into())
            ]
        );
        assert!(
            !handlers
                .get("click")
                .is_some_and(|handler| handler.starts_with('{')),
            "event handler should no longer be JSON-in-a-string"
        );
    }

    #[test]
    fn two_way_value_binding_still_resolves_attribute_value() {
        let attrs = vec![Attribute {
            name: "value".into(),
            value: AttributeValue::TwoWayBinding("current_value".into()),
        }];

        let (_, _, resolved, _, _) = parse_attributes(&attrs, None);

        assert_eq!(resolved.get("value"), Some(&String::new()));
    }

    #[test]
    fn bound_attributes_retain_runtime_types_through_accessibility_lowering() {
        let attrs = [
            ("disabled", "enabled"),
            ("min", "minimum"),
            ("data-metadata", "metadata"),
        ]
        .map(|(name, binding)| Attribute {
            name: name.into(),
            value: AttributeValue::Binding(binding.into()),
        });
        let store = MapStore(std::collections::HashMap::new());

        let (_, _, resolved, _, _) = parse_attributes_runtime(
            &attrs,
            Some(&store),
            "typed-attributes",
            Some(&TypedExpressionComposition),
            false,
        );

        assert!(resolved.get_value("disabled").unwrap().legacy_bool());
        assert_eq!(resolved.get_value("min").unwrap().parse_f32(), Some(1.5));
        assert_eq!(
            resolved
                .get_value("data-metadata")
                .unwrap()
                .to_legacy_string(),
            r#"{"source":"runtime"}"#
        );

        let accessibility = accessibility_for_element("input", "input", &resolved);
        assert!(accessibility.state.disabled);
        assert_eq!(accessibility.state.value_min, Some(1.5));
        assert_eq!(resolved.get("disabled").map(String::as_str), Some("true"));
        assert_eq!(resolved.get("min").map(String::as_str), Some("1.5"));
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
        fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
            self.0.get(name)
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
    fn tracking_store_coalesces_consecutive_duplicate_reads() {
        let inner = MapStore(std::collections::HashMap::new());
        let tracker = TrackingVariableStore::new(&inner);
        for _ in 0..4 {
            let _ = mesh_core_elements::VariableStore::get(&tracker, "audio.percent");
        }
        assert_eq!(
            tracker.into_reads(),
            vec![("audio".to_string(), "percent".to_string())]
        );
    }

    #[test]
    fn tracking_store_coalesces_nonconsecutive_duplicate_reads() {
        let inner = MapStore(std::collections::HashMap::new());
        let tracker = TrackingVariableStore::new(&inner);
        let _ = mesh_core_elements::VariableStore::get(&tracker, "audio.percent");
        let _ = mesh_core_elements::VariableStore::get(&tracker, "network.ssid");
        let _ = mesh_core_elements::VariableStore::get(&tracker, "audio.percent");

        assert_eq!(
            tracker.into_reads(),
            vec![
                ("audio".to_string(), "percent".to_string()),
                ("network".to_string(), "ssid".to_string())
            ]
        );
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

    // cargo test -p mesh-core-frontend --release -- repeated_service_read_coalescing_avoids_string_allocations --ignored --nocapture
    #[test]
    #[ignore = "release-only repeated service-read tracking microbenchmark"]
    fn repeated_service_read_coalescing_avoids_string_allocations() {
        use std::time::Instant;

        let iterations = 1_000_000usize;
        let name = "audio.percent";

        let eager_started = Instant::now();
        let mut eager = Vec::new();
        for _ in 0..iterations {
            let dot = std::hint::black_box(name).find('.').unwrap();
            eager.push((
                std::hint::black_box(name[..dot].to_owned()),
                std::hint::black_box(name[dot + 1..].to_owned()),
            ));
        }
        let eager_time = eager_started.elapsed();

        let inner = MapStore(std::collections::HashMap::new());
        let tracker = TrackingVariableStore::new(&inner);
        let coalesced_started = Instant::now();
        for _ in 0..iterations {
            tracker.record_read(std::hint::black_box(name));
        }
        let coalesced_time = coalesced_started.elapsed();
        let tracked = tracker.into_reads();

        eprintln!(
            "repeated service reads: eager allocations {eager_time:?}; coalesced {coalesced_time:?}; ratio {:.1}x; entries={}/{}",
            eager_time.as_secs_f64() / coalesced_time.as_secs_f64(),
            eager.len(),
            tracked.len()
        );
        assert_eq!(tracked.len(), 1);
        assert!(coalesced_time < eager_time);
    }

    // cargo test -p mesh-core-frontend --release -- nonconsecutive_service_read_coalescing_avoids_duplicate_allocations --ignored --nocapture
    #[test]
    #[ignore = "release-only nonconsecutive service-read tracking microbenchmark"]
    fn nonconsecutive_service_read_coalescing_avoids_duplicate_allocations() {
        use std::time::Instant;

        fn old_record_read(reads: &mut Vec<(String, String)>, name: &str) {
            let Some(dot_pos) = name.find('.') else {
                return;
            };
            let service = &name[..dot_pos];
            let field = &name[dot_pos + 1..];
            if reads.last().is_some_and(|(last_service, last_field)| {
                last_service == service && last_field == field
            }) {
                return;
            }
            reads.push((service.to_owned(), field.to_owned()));
        }

        let iterations = 250_000usize;
        let names = [
            "audio.percent",
            "network.ssid",
            "audio.percent",
            "power.percent",
        ];

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let mut reads = Vec::new();
            for name in names {
                old_record_read(&mut reads, std::hint::black_box(name));
            }
            old_total = old_total.wrapping_add(std::hint::black_box(reads.len()));
        }
        let old_time = old_started.elapsed();

        let inner = MapStore(std::collections::HashMap::new());
        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let tracker = TrackingVariableStore::new(&inner);
            for name in names {
                tracker.record_read(std::hint::black_box(name));
            }
            new_total = new_total.wrapping_add(std::hint::black_box(tracker.into_reads().len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "nonconsecutive service reads: consecutive-only {old_time:?}; duplicate scan {new_time:?}; ratio {:.1}x; entries={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_total < old_total);
        assert!(new_time < old_time);
    }

    #[test]
    fn event_handler_resolution_prefers_borrowed_store_lookup() {
        struct BorrowOnlyStore(serde_json::Value);

        impl VariableStore for BorrowOnlyStore {
            fn get(&self, _name: &str) -> Option<serde_json::Value> {
                panic!("owned lookup should not run when a borrowed value exists");
            }

            fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
                (name == "handler").then_some(&self.0)
            }

            fn keys(&self) -> Vec<String> {
                Vec::new()
            }
        }

        let store = BorrowOnlyStore(serde_json::json!("onResolved"));
        assert_eq!(
            resolve_event_handler_value(Some(&store), "handler"),
            "onResolved"
        );
    }

    // cargo test -p mesh-core-frontend --release -- borrowed_event_handler_lookup_beats_owned_json_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only event-handler lookup microbenchmark"]
    fn borrowed_event_handler_lookup_beats_owned_json_clone() {
        use std::time::Instant;

        struct HandlerStore(serde_json::Value);

        impl VariableStore for HandlerStore {
            fn get(&self, name: &str) -> Option<serde_json::Value> {
                (name == "handler").then(|| self.0.clone())
            }

            fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
                (name == "handler").then_some(&self.0)
            }

            fn keys(&self) -> Vec<String> {
                Vec::new()
            }
        }

        let store = HandlerStore(serde_json::json!("onPointerMove"));
        let iterations = 1_000_000usize;

        let owned_started = Instant::now();
        let mut owned_bytes = 0usize;
        for _ in 0..iterations {
            let handler = store
                .get(std::hint::black_box("handler"))
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap();
            owned_bytes = owned_bytes.wrapping_add(handler.len());
        }
        let owned_time = owned_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_bytes = 0usize;
        for _ in 0..iterations {
            let handler =
                resolve_event_handler_value(Some(&store), std::hint::black_box("handler"));
            borrowed_bytes = borrowed_bytes.wrapping_add(handler.len());
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "event handler state lookup: owned JSON clone {owned_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; bytes={owned_bytes}/{borrowed_bytes}",
            owned_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(owned_bytes, borrowed_bytes);
        assert!(borrowed_time < owned_time);
    }

    #[test]
    fn css_prop_resolution_prefers_borrowed_props_table() {
        struct BorrowOnlyProps(serde_json::Value);

        impl VariableStore for BorrowOnlyProps {
            fn get(&self, _name: &str) -> Option<serde_json::Value> {
                panic!("owned props lookup should not run when borrowing is supported");
            }

            fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
                (name == "props").then_some(&self.0)
            }

            fn keys(&self) -> Vec<String> {
                Vec::new()
            }
        }

        let component = mesh_core_component::parse_component(
            r#"
<props>
  width: { type: "size", default: "10px" }
</props>
<template><box/></template>
"#,
        )
        .unwrap();
        let store = BorrowOnlyProps(serde_json::json!({"width": "42px"}));
        let props = resolve_css_props(component.props.as_ref(), Some(&store));
        assert!(matches!(
            props.get(&prop_variable_key("width")),
            Some(StyleValue::Literal(value)) if value == "42px"
        ));
    }

    // cargo test -p mesh-core-frontend --release -- borrowed_css_props_table_beats_deep_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only CSS props table lookup microbenchmark"]
    fn borrowed_css_props_table_beats_deep_clone() {
        use std::time::Instant;

        struct OwnedProps(serde_json::Value);
        impl VariableStore for OwnedProps {
            fn get(&self, name: &str) -> Option<serde_json::Value> {
                (name == "props").then(|| self.0.clone())
            }
            fn keys(&self) -> Vec<String> {
                Vec::new()
            }
        }

        let component = mesh_core_component::parse_component(
            r#"
<props>
  width: { type: "size", default: "10px" }
</props>
<template><box/></template>
"#,
        )
        .unwrap();
        let mut values = serde_json::Map::new();
        values.insert("width".into(), serde_json::json!("42px"));
        for index in 0..128 {
            values.insert(
                format!("unused_{index}"),
                serde_json::json!({"payload": "x".repeat(1_024), "enabled": true}),
            );
        }
        let value = serde_json::Value::Object(values);
        let owned = OwnedProps(value.clone());
        let borrowed = MapStore(HashMap::from([("props".into(), value)]));
        let iterations = 10_000usize;

        let owned_started = Instant::now();
        let mut owned_total = 0usize;
        for _ in 0..iterations {
            owned_total +=
                resolve_css_props(component.props.as_ref(), Some(std::hint::black_box(&owned)))
                    .len();
        }
        let owned_time = owned_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_total = 0usize;
        for _ in 0..iterations {
            borrowed_total += resolve_css_props(
                component.props.as_ref(),
                Some(std::hint::black_box(&borrowed)),
            )
            .len();
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "CSS props table: owned deep clone {owned_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; totals={owned_total}/{borrowed_total}",
            owned_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(owned_total, borrowed_total);
        assert!(borrowed_time < owned_time);
    }

    #[test]
    fn namespaced_handler_preserves_typed_owner() {
        let mut target = HandlerTarget::root("onToggle");
        target.namespace("@mesh/panel/local:Toolbar");
        assert_eq!(target.handler(), "onToggle");
        assert_eq!(target.instance_key(), Some("@mesh/panel/local:Toolbar"));
        target.namespace("@mesh/other");
        assert_eq!(target.instance_key(), Some("@mesh/panel/local:Toolbar"));
    }

    #[test]
    fn gesture_and_touch_attributes_are_runtime_event_handlers() {
        for name in [
            "ontwofingerscroll",
            "onswipe",
            "onpinch",
            "onhold",
            "ontouchstart",
            "ontouchmove",
            "ontouchend",
            "ontouchcancel",
            "ontap",
            "ondoubletap",
            "onlongpress",
        ] {
            assert!(is_event_handler_attribute(name), "{name}");
        }
    }

    // cargo test -p mesh-core-frontend --release -- borrowed_event_attribute_classification_beats_owned_normalization --ignored --nocapture
    #[test]
    #[ignore = "release-only event attribute classification microbenchmark"]
    fn borrowed_event_attribute_classification_beats_owned_normalization() {
        use std::hint::black_box;
        use std::time::Instant;

        let names = [
            "class",
            "onclick",
            "ontap",
            "ontwofingerscroll",
            "onlongpress",
            "oncustom",
            "style",
            "onchange",
        ];
        let iterations = 2_000_000;

        let owned_started = Instant::now();
        let mut owned_matches = 0usize;
        for index in 0..iterations {
            let name = black_box(names[index % names.len()]);
            if name.starts_with("on")
                && matches!(
                    name.strip_prefix("on").unwrap_or(name).to_string().as_str(),
                    "click" | "change" | "tap" | "twofingerscroll" | "longpress"
                )
            {
                owned_matches += 1;
            }
        }
        let owned = owned_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_matches = 0usize;
        for index in 0..iterations {
            if is_event_handler_attribute(black_box(names[index % names.len()])) {
                borrowed_matches += 1;
            }
        }
        let borrowed = borrowed_started.elapsed();

        // The production matcher recognizes more event names; all extra names
        // in this workload are deliberately non-events.
        assert_eq!(owned_matches, borrowed_matches);
        let speedup = owned.as_secs_f64() / borrowed.as_secs_f64();
        eprintln!(
            "MESH_PERF metric=borrowed_event_attribute_classification_speedup value={speedup:.3} owned={owned:?} borrowed={borrowed:?}"
        );
        assert!(borrowed < owned);
    }

    // cargo test -p mesh-core-frontend --release -- compiler_handler_namespace_presizing_beats_format_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only compiler handler namespace microbenchmark"]
    fn compiler_handler_namespace_presizing_beats_format_benchmark() {
        use std::time::Instant;

        let instance_key = "@mesh/panel/local:StatusCluster/import:NetworkControls";
        let handler = "onConnectionStateChanged";
        let iterations = 1_000_000;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total ^=
                std::hint::black_box(format!("__mesh_embed__::{instance_key}::{handler}").len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let target = std::hint::black_box(HandlerTarget::embedded(instance_key, handler));
            new_total ^= target.dynamic_heap_bytes();
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "compiler handler namespace: format {old_time:?}; typed {new_time:?}; ratio {:.2}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(new_total, 0);
        assert!(new_time < old_time);
    }

    // `AccessibilityInfo`/`AccessibilityState` intentionally don't derive
    // `PartialEq` (they're not otherwise compared for equality), so compare
    // the fields this benchmark pair actually produces.
    fn accessibility_info_eq(a: &AccessibilityInfo, b: &AccessibilityInfo) -> bool {
        a.role == b.role
            && a.label == b.label
            && a.description == b.description
            && a.focusable == b.focusable
            && a.focused == b.focused
            && a.keyboard_shortcut == b.keyboard_shortcut
            && a.state.disabled == b.state.disabled
            && a.state.checked == b.state.checked
            && a.state.expanded == b.state.expanded
            && a.state.selected == b.state.selected
            && a.state.pressed == b.state.pressed
            && a.state.busy == b.state.busy
            && a.state.invalid == b.state.invalid
            && a.state.required == b.state.required
            && a.state.value == b.state.value
            && a.state.value_min == b.state.value_min
            && a.state.value_max == b.state.value_max
    }

    #[test]
    fn accessibility_for_element_empty_attributes_matches_unguarded_chain() {
        let empty = AttributeMap::new();
        for tag in ["box", "row", "column", "button", "text", "custom-widget"] {
            assert!(accessibility_info_eq(
                &accessibility_for_element(tag, tag, &empty),
                &accessibility_for_element_unguarded(tag, tag, &empty)
            ));
        }
    }

    #[test]
    fn accessibility_for_element_non_empty_attributes_matches_unguarded_chain() {
        let mut attributes = AttributeMap::new();
        attributes.insert("class".into(), "active".to_string());
        attributes.insert("disabled".into(), "true".to_string());
        attributes.insert("min".into(), "1".to_string());
        assert!(accessibility_info_eq(
            &accessibility_for_element("input", "input", &attributes),
            &accessibility_for_element_unguarded("input", "input", &attributes)
        ));
    }

    #[test]
    fn accessibility_single_pass_matches_lookup_chain_including_precedence() {
        // Every attribute the chain reads, in every alternative spelling, plus
        // unrelated attributes that must be ignored.
        let all: Vec<(&str, &str)> = vec![
            ("aria-label", "aria"),
            ("label", "label"),
            ("alt", "alt"),
            ("title", "title"),
            ("tooltip", "tooltip"),
            ("key", "ctrl+k"),
            ("keybind", "ctrl+b"),
            ("shortcut", "ctrl+s"),
            ("expanded", "true"),
            ("open", "false"),
            ("disabled", "1"),
            ("checked", "false"),
            ("selected", ""),
            ("pressed", "true"),
            ("busy", "nope"),
            ("invalid", "true"),
            ("required", "1"),
            ("value", " 42 "),
            ("min", " 1.5 "),
            ("max", "not-a-number"),
            ("class", "primary"),
            ("data-mesh-element", "button"),
        ];

        // Full set, then every single-attribute map, then each attribute with
        // its higher-precedence sibling removed.
        let mut cases: Vec<AttributeMap> = vec![
            all.iter()
                .map(|(k, v)| (AttrKey::new(k), v.to_string()))
                .collect(),
        ];
        for (name, value) in &all {
            cases.push(AttributeMap::from([(
                AttrKey::new(name),
                value.to_string(),
            )]));
        }
        for skip in ["aria-label", "label", "title", "key", "keybind", "expanded"] {
            cases.push(
                all.iter()
                    .filter(|(name, _)| *name != skip)
                    .map(|(k, v)| (AttrKey::new(k), v.to_string()))
                    .collect(),
            );
        }

        for attributes in cases {
            for tag in ["input", "button", "box", "custom-widget"] {
                let single_pass = accessibility_for_element(tag, tag, &attributes);
                let chain = accessibility_for_element_unguarded(tag, tag, &attributes);
                assert!(
                    accessibility_info_eq(&single_pass, &chain),
                    "mismatch for <{tag}> with {attributes:?}"
                );
                assert_eq!(single_pass.label, chain.label);
                assert_eq!(single_pass.description, chain.description);
                assert_eq!(single_pass.keyboard_shortcut, chain.keyboard_shortcut);
            }
        }
    }

    // cargo test -p mesh-core-frontend --release -- accessibility_attribute_pass_beats_lookup_chain --ignored --nocapture
    #[test]
    #[ignore = "release-only accessibility attribute-pass microbenchmark"]
    fn accessibility_attribute_pass_beats_lookup_chain() {
        use std::time::Instant;

        // A representative populated element: a few real attributes, none of
        // which most of the accessibility chain is looking for.
        let attributes: AttributeMap = [
            ("class", "entry-action primary"),
            ("data-mesh-element", "button"),
            ("style", "padding: 4px"),
            ("aria-label", "Open entry"),
        ]
        .into_iter()
        .map(|(name, value)| (AttrKey::new(name), value.to_string()))
        .collect();
        let iterations = 2_000_000usize;

        let chain_started = Instant::now();
        let mut chain_checksum = 0usize;
        for _ in 0..iterations {
            let info = accessibility_for_element_unguarded(
                std::hint::black_box("button"),
                std::hint::black_box("button"),
                std::hint::black_box(&attributes),
            );
            chain_checksum ^= info.label.map(|label| label.len()).unwrap_or(0);
        }
        let chain_time = chain_started.elapsed();

        let pass_started = Instant::now();
        let mut pass_checksum = 0usize;
        for _ in 0..iterations {
            let info = accessibility_for_element(
                std::hint::black_box("button"),
                std::hint::black_box("button"),
                std::hint::black_box(&attributes),
            );
            pass_checksum ^= info.label.map(|label| label.len()).unwrap_or(0);
        }
        let pass_time = pass_started.elapsed();

        eprintln!(
            "accessibility ({} attributes) over {iterations} elements: lookup chain {chain_time:?}, single pass {pass_time:?}, ratio {:.2}x",
            attributes.len(),
            chain_time.as_secs_f64() / pass_time.as_secs_f64()
        );
        println!(
            "MESH_PERF metric=accessibility_attribute_pass_speedup value={:.6}",
            chain_time.as_secs_f64() / pass_time.as_secs_f64()
        );
        assert_eq!(chain_checksum, pass_checksum);
        assert!(pass_time < chain_time);
    }

    // cargo test -p mesh-core-frontend --release -- typed_attribute_storage_beats_stringify_then_parse --ignored --nocapture
    #[test]
    #[ignore = "release-only typed attribute storage microbenchmark"]
    fn typed_attribute_storage_beats_stringify_then_parse() {
        use std::time::Instant;

        const ITERATIONS: usize = 500_000;
        let values = [
            ("checked", serde_json::json!(false)),
            ("disabled", serde_json::json!(true)),
            ("expanded", serde_json::json!(true)),
            ("max", serde_json::json!(100.5)),
            ("min", serde_json::json!(1.5)),
            ("selected", serde_json::json!(false)),
        ];
        let legacy_map = || {
            let mut attributes = AttributeMap::with_capacity(values.len());
            for (name, value) in &values {
                attributes.insert(AttrKey::new(name), template_value_to_string(value.clone()));
            }
            attributes
        };
        let typed_map = || {
            let mut attributes = AttributeMap::with_capacity(values.len());
            for (name, value) in &values {
                attributes.insert_value(AttrKey::new(name), value.clone());
            }
            attributes
        };

        let legacy = legacy_map();
        let typed = typed_map();
        assert!(accessibility_info_eq(
            &accessibility_for_element("input", "input", &legacy),
            &accessibility_for_element("input", "input", &typed)
        ));

        let legacy_started = Instant::now();
        let mut legacy_checksum = 0f32;
        for _ in 0..ITERATIONS {
            let attributes = legacy_map();
            let info = accessibility_for_element(
                std::hint::black_box("input"),
                std::hint::black_box("input"),
                std::hint::black_box(&attributes),
            );
            legacy_checksum += std::hint::black_box(
                info.state.disabled as u8 as f32
                    + info.state.expanded.unwrap_or(false) as u8 as f32
                    + info.state.value_min.unwrap_or_default()
                    + info.state.value_max.unwrap_or_default(),
            );
        }
        let legacy_time = legacy_started.elapsed();

        let typed_started = Instant::now();
        let mut typed_checksum = 0f32;
        for _ in 0..ITERATIONS {
            let attributes = typed_map();
            let info = accessibility_for_element(
                std::hint::black_box("input"),
                std::hint::black_box("input"),
                std::hint::black_box(&attributes),
            );
            typed_checksum += std::hint::black_box(
                info.state.disabled as u8 as f32
                    + info.state.expanded.unwrap_or(false) as u8 as f32
                    + info.state.value_min.unwrap_or_default()
                    + info.state.value_max.unwrap_or_default(),
            );
        }
        let typed_time = typed_started.elapsed();

        assert_eq!(legacy_checksum, typed_checksum);
        eprintln!(
            "six bound attributes across {ITERATIONS} nodes: stringify + parse {legacy_time:?}, typed storage {typed_time:?}, ratio {:.2}x",
            legacy_time.as_secs_f64() / typed_time.as_secs_f64()
        );
        println!(
            "MESH_PERF metric=typed_attribute_storage_speedup value={:.6}",
            legacy_time.as_secs_f64() / typed_time.as_secs_f64()
        );
        assert!(typed_time < legacy_time);
    }

    // cargo test -p mesh-core-frontend --release -- accessibility_empty_attribute_guard_beats_full_lookup_chain --ignored --nocapture
    #[test]
    #[ignore = "release-only accessibility attribute-guard microbenchmark"]
    fn accessibility_empty_attribute_guard_beats_full_lookup_chain() {
        use std::time::Instant;

        // Synthetic {#if}/{#for} column wrappers and many plain structural
        // elements carry no attributes; this is the realistic empty case.
        let empty = AttributeMap::new();
        let iterations = 2_000_000usize;

        let unguarded_started = Instant::now();
        let mut unguarded_checksum = 0usize;
        for _ in 0..iterations {
            let info = accessibility_for_element_unguarded(
                std::hint::black_box("column"),
                std::hint::black_box("column"),
                std::hint::black_box(&empty),
            );
            unguarded_checksum ^= info.focusable as usize;
        }
        let unguarded_time = unguarded_started.elapsed();

        let guarded_started = Instant::now();
        let mut guarded_checksum = 0usize;
        for _ in 0..iterations {
            let info = accessibility_for_element(
                std::hint::black_box("column"),
                std::hint::black_box("column"),
                std::hint::black_box(&empty),
            );
            guarded_checksum ^= info.focusable as usize;
        }
        let guarded_time = guarded_started.elapsed();

        eprintln!(
            "accessibility (empty attrs) over {iterations} nodes: unguarded {unguarded_time:?}, guarded {guarded_time:?}, ratio {:.2}x",
            unguarded_time.as_secs_f64() / guarded_time.as_secs_f64()
        );
        println!(
            "MESH_PERF metric=accessibility_empty_attribute_guard_speedup value={:.6}",
            unguarded_time.as_secs_f64() / guarded_time.as_secs_f64()
        );
        assert_eq!(unguarded_checksum, guarded_checksum);
        assert!(guarded_time < unguarded_time);
    }

    // cargo test -p mesh-core-frontend --release -- shared_module_id_beats_per_node_string --ignored --nocapture
    #[test]
    #[ignore = "release-only shared module-identity microbenchmark"]
    fn shared_module_id_beats_per_node_string() {
        use std::time::Instant;

        // Every node in a built tree carries its module's identity.
        let module_id = "@mesh/navigation-bar";
        let nodes = 2_000usize;
        let passes = 500usize;

        // Only the identity assignment is timed: constructing the ~900-byte
        // node around it is unchanged by this work and would swamp the signal.
        let mut node = WidgetNode::new("row");

        let owned_started = Instant::now();
        let mut owned_total = 0usize;
        for _ in 0..passes {
            for _ in 0..nodes {
                node.set_module_id(std::hint::black_box(module_id).to_string());
                owned_total = owned_total.wrapping_add(node.module_id().unwrap().len());
            }
        }
        let owned_time = owned_started.elapsed();

        // Warm the shared cache so the benchmark measures steady-state lookups.
        let _ = shared_module_id(module_id);

        let shared_started = Instant::now();
        let mut shared_total = 0usize;
        for _ in 0..passes {
            for _ in 0..nodes {
                attach_module_id(&mut node, std::hint::black_box(module_id));
                shared_total = shared_total.wrapping_add(node.module_id().unwrap().len());
            }
        }
        let shared_time = shared_started.elapsed();

        eprintln!(
            "module identity over {} nodes: per-node string {owned_time:?}, shared Arc {shared_time:?}, ratio {:.2}x",
            nodes * passes,
            owned_time.as_secs_f64() / shared_time.as_secs_f64()
        );
        println!(
            "MESH_PERF metric=shared_module_id_speedup value={:.6}",
            owned_time.as_secs_f64() / shared_time.as_secs_f64()
        );
        assert_eq!(owned_total, shared_total);
        assert!(shared_time < owned_time);
    }

    #[test]
    fn shared_module_ids_reuse_one_allocation_per_module() {
        let mut first = WidgetNode::new("row");
        let mut second = WidgetNode::new("column");
        attach_module_id(&mut first, "@mesh/panel");
        attach_module_id(&mut second, "@mesh/panel");
        assert_eq!(first.module_id(), Some("@mesh/panel"));
        assert!(std::sync::Arc::ptr_eq(
            first.shared_module_id().unwrap(),
            second.shared_module_id().unwrap()
        ));

        let mut other = WidgetNode::new("box");
        attach_module_id(&mut other, "@mesh/launcher");
        assert_eq!(other.module_id(), Some("@mesh/launcher"));
        assert!(!std::sync::Arc::ptr_eq(
            first.shared_module_id().unwrap(),
            other.shared_module_id().unwrap()
        ));

        // The bounded cache must keep returning correct ids after eviction.
        for index in 0..(SHARED_MODULE_ID_CACHE_LIMIT * 2) {
            let id = format!("@mesh/module-{index}");
            let mut node = WidgetNode::new("row");
            attach_module_id(&mut node, &id);
            assert_eq!(node.module_id(), Some(id.as_str()));
        }
        let mut again = WidgetNode::new("row");
        attach_module_id(&mut again, "@mesh/panel");
        assert_eq!(again.module_id(), Some("@mesh/panel"));
    }

    // cargo test -p mesh-core-frontend --release -- for_children_capacity_beats_growing --ignored --nocapture
    #[test]
    #[ignore = "release-only {#for} child-vector capacity microbenchmark"]
    fn for_children_capacity_beats_growing() {
        use std::time::Instant;

        // A `{#for}` over N items with M children per iteration pushes N*M
        // large `WidgetNode` values; growing re-copies everything pushed so far.
        let items = 200usize;
        let children_per_item = 3usize;
        let passes = 2_000usize;
        let total = items * children_per_item;

        let growing_started = Instant::now();
        let mut growing_total = 0usize;
        for _ in 0..passes {
            let mut children: Vec<WidgetNode> = Vec::new();
            for _ in 0..total {
                children.push(WidgetNode::new(std::hint::black_box("row")));
            }
            growing_total = growing_total.wrapping_add(children.len());
        }
        let growing_time = growing_started.elapsed();

        let reserved_started = Instant::now();
        let mut reserved_total = 0usize;
        for _ in 0..passes {
            let mut children: Vec<WidgetNode> = Vec::with_capacity(total);
            for _ in 0..total {
                children.push(WidgetNode::new(std::hint::black_box("row")));
            }
            reserved_total = reserved_total.wrapping_add(children.len());
        }
        let reserved_time = reserved_started.elapsed();

        eprintln!(
            "{{#for}} children ({total} nodes of {} bytes, {passes} passes): growing {growing_time:?}, reserved {reserved_time:?}, ratio {:.2}x",
            std::mem::size_of::<WidgetNode>(),
            growing_time.as_secs_f64() / reserved_time.as_secs_f64()
        );
        println!(
            "MESH_PERF metric=for_children_capacity_speedup value={:.6}",
            growing_time.as_secs_f64() / reserved_time.as_secs_f64()
        );
        assert_eq!(growing_total, reserved_total);
        assert!(reserved_time < growing_time);
    }

    /// A component shaped like a real shell surface: a styled root, nested
    /// rows/columns with classes, text and expression nodes, a conditional,
    /// and a loop over a list of items.
    fn end_to_end_bench_component() -> mesh_core_component::ComponentFile {
        mesh_core_component::parse_component(
            r#"
<template>
  <column class="root">
    <row class="header">
      <text class="title">Panel</text>
      <text class="subtitle">{subtitle}</text>
    </row>
    {#if expanded}
      <column class="body">
        {#for item in items}
          <row class="entry">
            <icon class="entry-icon" name="star" />
            <column class="entry-text">
              <text class="entry-title">{item.title}</text>
              <text class="entry-detail">{item.detail}</text>
            </column>
            <button class="entry-action" onclick={onActivate}>
              <text>Open</text>
            </button>
          </row>
        {/for}
      </column>
    {/if}
  </column>
</template>
<style>
  .root { color: #ffffff; font-family: "Inter"; padding: 8px; }
  .header { font-size: 18px; padding-bottom: 4px; }
  .title { font-weight: 700; }
  .subtitle { color: #b0b0b0; font-size: 12px; }
  .body { padding: 4px; }
  .entry { padding: 6px; border-radius: 8px; }
  .entry:hover { color: #ffffff; }
  .entry-icon { width: 16px; height: 16px; }
  .entry-text { padding-left: 8px; }
  .entry-title { font-weight: 600; font-size: 14px; }
  .entry-detail { color: #9a9a9a; font-size: 11px; }
  .entry-action { padding: 4px; }
  button { font-weight: 600; }
  text { line-height: 1.4; }
  row { padding-left: 2px; }
  column { padding-top: 2px; }
</style>
"#,
        )
        .unwrap()
    }

    // cargo test -p mesh-core-frontend --release -- widget_tree_build_end_to_end_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only end-to-end widget-tree build benchmark"]
    fn widget_tree_build_end_to_end_benchmark() {
        use std::time::Instant;

        let component = end_to_end_bench_component();
        let manifest = test_manifest();
        let theme = mesh_core_theme::default_theme();
        let items: Vec<serde_json::Value> = (0..64)
            .map(|index| {
                serde_json::json!({
                    "title": format!("Entry {index}"),
                    "detail": format!("detail line {index}"),
                })
            })
            .collect();
        let store = MapStore(
            [
                ("subtitle".to_string(), serde_json::json!("all systems")),
                ("expanded".to_string(), serde_json::json!(true)),
                ("items".to_string(), serde_json::Value::Array(items)),
                ("onActivate".to_string(), serde_json::json!("onActivate")),
            ]
            .into_iter()
            .collect(),
        );

        fn count(node: &WidgetNode) -> usize {
            1 + node.children.iter().map(count).sum::<usize>()
        }
        let node_count = count(&build_widget_tree_from_component(
            &component,
            &manifest,
            &theme,
            480.0,
            640.0,
            None,
            "root",
            Some(&store),
            &[],
        ));

        let builds = 2_000usize;
        let started = Instant::now();
        let mut checksum = 0usize;
        for _ in 0..builds {
            let tree = build_widget_tree_from_component(
                std::hint::black_box(&component),
                &manifest,
                &theme,
                480.0,
                640.0,
                None,
                "root",
                Some(&store),
                &[],
            );
            checksum = checksum.wrapping_add(tree.children.len());
        }
        let elapsed = started.elapsed();

        eprintln!(
            "widget tree build: {builds} builds of a {node_count}-node tree in {elapsed:?} ({:.3}ms per build, checksum {checksum})",
            elapsed.as_secs_f64() * 1000.0 / builds as f64
        );
    }
}
