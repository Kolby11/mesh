use crate::FrontendCompositionResolver;

use mesh_core_component::template::{
    Attribute, AttributeValue, ComponentRef, ElementNode, SourceTag, TemplateNode,
};
use mesh_core_elements::accessibility::AccessibilityInfo;
use mesh_core_elements::{
    AttrKey, AttributeMap, ComponentCompositionProps, ComputedStyle, EventHandlerCall,
    HandlerTarget, StyleContext, VariableStore, WidgetNode, element_contract_for_tag,
};
use mesh_core_module::Manifest;
use serde_json;

use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) fn template_has_dynamic_structure(nodes: &[TemplateNode]) -> bool {
    nodes.iter().any(|node| match node {
        TemplateNode::If(_) | TemplateNode::For(_) | TemplateNode::Slot(_) => true,
        TemplateNode::Element(element) => template_has_dynamic_structure(&element.children),
        TemplateNode::Component(_) | TemplateNode::Text(_) | TemplateNode::Expr(_) => false,
    })
}

pub(super) fn template_subtree_is_native(node: &TemplateNode) -> bool {
    match node {
        TemplateNode::Element(element) => element.children.iter().all(template_subtree_is_native),
        TemplateNode::Text(_) | TemplateNode::Expr(_) => true,
        TemplateNode::Component(_)
        | TemplateNode::If(_)
        | TemplateNode::For(_)
        | TemplateNode::Slot(_) => false,
    }
}

pub(super) fn is_inline_template_node(node: &TemplateNode) -> bool {
    matches!(node, TemplateNode::Text(_) | TemplateNode::Expr(_))
}

pub(super) fn default_input_type(source_tag: &SourceTag) -> Option<&'static str> {
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

pub(super) fn apply_source_tag_defaults(source_tag: &SourceTag, attributes: &mut AttributeMap) {
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

pub(super) fn resolve_inline_content(
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

pub(super) fn build_component_ref(
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
            state.and_then(VariableStore::loop_identity),
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
        None,
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
pub(super) fn component_prop_handler_token(host_instance_key: &str, prop_name: &str) -> String {
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

pub(super) const SHARED_MODULE_ID_CACHE_LIMIT: usize = 16;

/// Resolve a module id to a shared allocation.
///
/// Every node built from one module carries the same identity string, so
/// cloning an `Arc` here replaces one malloc-and-copy per node with a
/// refcount bump. The lookup is a short string comparison, not a hash.
pub(super) fn shared_module_id(module_id: &str) -> Arc<str> {
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

pub(super) fn attach_module_id(node: &mut WidgetNode, module_id: &str) {
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

pub(super) fn parse_attributes_runtime(
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

pub(super) fn namespace_handler_if_needed(
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

pub(super) fn resolve_event_handler_value(
    state: Option<&dyn VariableStore>,
    handler: &str,
) -> String {
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

pub(super) fn normalize_event_handler_name(name: &str) -> String {
    normalized_event_handler_name(name).to_owned()
}

pub(super) fn normalized_event_handler_name(name: &str) -> &str {
    name.strip_prefix("on").unwrap_or(name)
}

pub(super) fn is_event_handler_attribute(name: &str) -> bool {
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

pub(super) fn accessibility_for_element(
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
pub(super) fn accessibility_for_tag(tag: &str) -> AccessibilityInfo {
    accessibility_for_element(tag, tag, &AttributeMap::new())
}

/// Pre-guard behavior: always runs the full attribute lookup chain, even for
/// an empty attribute map. Kept only for the release benchmark comparison
/// below; production code takes the early-return path in
/// `accessibility_for_element`.
#[cfg(test)]
pub(super) fn accessibility_for_element_unguarded(
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
pub(super) fn bool_attr(attributes: &AttributeMap, name: &str) -> bool {
    attributes.get(name).is_some_and(|value| bool_value(value))
}

#[cfg(test)]
pub(super) fn bool_value(value: &str) -> bool {
    matches!(value.trim(), "" | "true" | "1")
}

#[cfg(test)]
pub(super) fn number_attr(attributes: &AttributeMap, name: &str) -> Option<f32> {
    attributes.get(name)?.trim().parse::<f32>().ok()
}
