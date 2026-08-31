use crate::style::{
    child_style_context, inherit_text_style, inherited_style_mask, slot_style,
    synthetic_wrapper_style,
};
use crate::tags::lower_source_tag;
use crate::{FrontendCompositionResolver, LayeredStore};

use mesh_core_component::template::{AttributeValue, ElementNode, ForNode, TemplateNode};
use mesh_core_elements::style::StylePropertyMask;
use mesh_core_elements::{
    ComputedStyle, NodeId, StyleContext, StyleResolver, VariableStore, WidgetNode,
    authored_element_state, normalize_accessibility,
};
use mesh_core_module::Manifest;
use mesh_core_theme::Theme;
use serde_json;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

mod elements;
mod expr_eval;
mod style_context;

#[cfg(test)]
pub(crate) use elements::parse_attributes;
pub(crate) use elements::template_has_dynamic_structure;
pub(crate) use expr_eval::collect_component_tags;
pub(crate) use style_context::BuildStyleContext;
pub use style_context::{PreparedComponentStyleRules, props_settings_schema, resolve_css_props};

thread_local! {
    static CURRENT_COMPONENT_SOURCE_PATH: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) fn current_component_source_path() -> Option<PathBuf> {
    CURRENT_COMPONENT_SOURCE_PATH.with(|path| path.borrow().clone())
}

pub(crate) struct ComponentSourcePathGuard(Option<PathBuf>);

impl ComponentSourcePathGuard {
    pub(crate) fn enter(path: Option<&Path>) -> Self {
        Self(
            CURRENT_COMPONENT_SOURCE_PATH
                .with(|current| current.replace(path.map(Path::to_path_buf))),
        )
    }
}

impl Drop for ComponentSourcePathGuard {
    fn drop(&mut self) {
        CURRENT_COMPONENT_SOURCE_PATH.with(|current| {
            current.replace(self.0.take());
        });
    }
}

use elements::*;
use expr_eval::*;
use style_context::*;

/// Build a WidgetNode subtree from a parsed local ComponentFile.
/// This is a public helper so other crates (core) can render local
/// component templates without duplicating the template->widget logic.
///
/// `host_rules` are the parent module's CSS rules. They are merged before the
/// component's own rules so that parent-defined classes (e.g. `.battery-widget`)
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
        None,
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
        None,
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
        None,
    )
}

/// Build a local component while retaining its canonical source identity for
/// recursive owner-scoped import resolution.
#[allow(clippy::too_many_arguments)]
pub fn build_embedded_widget_tree_from_component_with_prepared_styles_and_owner(
    component: &mesh_core_component::ComponentFile,
    host_manifest: &Manifest,
    theme: &Theme,
    container_width: f32,
    container_height: f32,
    composition: Option<&dyn FrontendCompositionResolver>,
    instance_key: &str,
    state: Option<&dyn VariableStore>,
    prepared_styles: &PreparedComponentStyleRules,
    owner_source_path: &Path,
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
        Some(owner_source_path),
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
    owner_source_path: Option<&Path>,
) -> WidgetNode {
    let _source_path_guard = ComponentSourcePathGuard::enter(owner_source_path);
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
            .flat_map(|node| {
                build_widget_nodes(
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
        normalize_accessibility(&mut container);
        container
    } else {
        let mut container = WidgetNode::new("box");
        attach_module_id(&mut container, &host_manifest.package.id);
        normalize_accessibility(&mut container);
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
    let mut node = build_widget_node_inner(
        node,
        manifest,
        build_style,
        parent_style,
        container_context,
        state,
        instance_key,
        composition,
        None,
    );
    normalize_accessibility(&mut node);
    node
}

/// Build a template node as zero or more layout children. Control-flow nodes
/// are fragments: their active children join the surrounding parent instead of
/// introducing an author-invisible flex container.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_widget_nodes(
    node: &TemplateNode,
    manifest: &Manifest,
    build_style: &BuildStyleContext<'_, '_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> Vec<WidgetNode> {
    match node {
        TemplateNode::If(if_node) => {
            let show_then = state.is_none_or(|store| {
                !matches!(
                    evaluate_template_expression(
                        &if_node.condition,
                        Some(store),
                        instance_key,
                        composition,
                    ),
                    serde_json::Value::Null | serde_json::Value::Bool(false)
                )
            });
            let children = if show_then {
                &if_node.then_children
            } else {
                &if_node.else_children
            };
            children
                .iter()
                .flat_map(|child| {
                    build_widget_nodes(
                        child,
                        manifest,
                        build_style,
                        parent_style,
                        container_context,
                        state,
                        instance_key,
                        composition,
                    )
                })
                .collect()
        }
        TemplateNode::For(for_node) => state
            .map(|store| {
                if let Some(composition) = composition {
                    match evaluate_template_expression(
                        &for_node.iterable,
                        Some(store),
                        instance_key,
                        Some(composition),
                    ) {
                        serde_json::Value::Array(items) => build_for_children(
                            &items,
                            for_node,
                            manifest,
                            build_style,
                            parent_style,
                            container_context,
                            store,
                            instance_key,
                            Some(composition),
                        ),
                        _ => Vec::new(),
                    }
                } else if let Some(serde_json::Value::Array(items)) =
                    store.get_ref(&for_node.iterable)
                {
                    build_for_children(
                        items,
                        for_node,
                        manifest,
                        build_style,
                        parent_style,
                        container_context,
                        store,
                        instance_key,
                        None,
                    )
                } else if let Some(serde_json::Value::Array(items)) = store.get(&for_node.iterable)
                {
                    build_for_children(
                        &items,
                        for_node,
                        manifest,
                        build_style,
                        parent_style,
                        container_context,
                        store,
                        instance_key,
                        None,
                    )
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default(),
        TemplateNode::Slot(slot) if slot.customizable => {
            let Some(composition) = composition else {
                return Vec::new();
            };
            let mut children = composition.render_slot(
                manifest,
                instance_key,
                slot.extension_point.as_deref(),
                slot.name.as_deref(),
                true,
                container_context.container_width,
                container_context.container_height,
            );
            if let Some(max) = slot
                .extension_point
                .as_ref()
                .and_then(|point| manifest.hosted_extension_points.get(point))
                .and_then(|hosted| hosted.max)
            {
                children.truncate(max as usize);
            }
            children
        }
        _ => vec![build_widget_node(
            node,
            manifest,
            build_style,
            parent_style,
            container_context,
            state,
            instance_key,
            composition,
        )],
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_widget_node_selective(
    node: &TemplateNode,
    manifest: &Manifest,
    build_style: &BuildStyleContext<'_, '_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
    previous: Option<&WidgetNode>,
    rebuild_node_ids: &HashSet<NodeId>,
) -> WidgetNode {
    let mut node = build_widget_node_inner(
        node,
        manifest,
        build_style,
        parent_style,
        container_context,
        state,
        instance_key,
        composition,
        previous.map(|previous| (previous, rebuild_node_ids)),
    );
    normalize_accessibility(&mut node);
    node
}

#[allow(clippy::too_many_arguments)]
fn build_widget_node_inner(
    node: &TemplateNode,
    manifest: &Manifest,
    build_style: &BuildStyleContext<'_, '_>,
    parent_style: Option<&ComputedStyle>,
    container_context: StyleContext,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
    selective: Option<(&WidgetNode, &HashSet<NodeId>)>,
) -> WidgetNode {
    if let Some((previous, rebuild_node_ids)) = selective
        && !rebuild_node_ids.contains(&previous.id)
        && template_subtree_is_native(node)
    {
        return previous.clone();
    }
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
            selective,
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
            node.computed_style = build_style
                .resolver
                .resolve_node_style_for_module_indexed_with_parent_style(
                    build_style.rules,
                    build_style.index.as_ref(),
                    "text",
                    &[],
                    None,
                    container_context,
                    Default::default(),
                    Some(&manifest.package.id),
                    parent_style,
                );
            if let Some(parent_style) = parent_style {
                inherit_text_style(
                    &mut node.computed_style,
                    parent_style,
                    StylePropertyMask::default(),
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
            node.computed_style = build_style
                .resolver
                .resolve_node_style_for_module_indexed_with_parent_style(
                    build_style.rules,
                    build_style.index.as_ref(),
                    "text",
                    &[],
                    None,
                    container_context,
                    Default::default(),
                    Some(&manifest.package.id),
                    parent_style,
                );
            if let Some(parent_style) = parent_style {
                inherit_text_style(
                    &mut node.computed_style,
                    parent_style,
                    StylePropertyMask::default(),
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
                    StylePropertyMask::default(),
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
                            Some(&node.computed_style),
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
                            Some(&node.computed_style),
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
                                Some(&node.computed_style),
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
                    StylePropertyMask::default(),
                );
            }
            node
        }
        TemplateNode::Slot(slot) => {
            let hosted = slot
                .extension_point
                .as_ref()
                .and_then(|point| manifest.hosted_extension_points.get(point));
            let layout = hosted
                .and_then(|hosted| hosted.layout.as_deref())
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
                slot.extension_point
                    .clone()
                    .unwrap_or_else(|| "default".into()),
            );
            node.computed_style = slot_style(tag);
            let child_context = child_style_context(&node.computed_style, container_context);
            if let Some(composition) = composition {
                let mut children = composition.render_slot(
                    manifest,
                    instance_key,
                    slot.extension_point.as_deref(),
                    slot.name.as_deref(),
                    slot.customizable,
                    child_context.container_width,
                    child_context.container_height,
                );
                if let Some(max) = hosted.and_then(|hosted| hosted.max) {
                    children.truncate(max as usize);
                }
                node.children = children.into();
            }
            if let Some(parent_style) = parent_style {
                inherit_text_style(
                    &mut node.computed_style,
                    parent_style,
                    StylePropertyMask::default(),
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
    parent_style: Option<&ComputedStyle>,
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
    let mut seen_keyed_identities = HashSet::new();
    for (item_ordinal, item_val) in items.enumerate() {
        let loop_identity = for_node
            .key
            .as_ref()
            .map(|key| {
                let item_store = LayeredStore {
                    base: store,
                    item_name: &for_node.item_name,
                    item_value: item_val,
                    loop_identity: store.loop_identity().map(str::to_owned),
                };
                let key_value =
                    evaluate_template_expression(key, Some(&item_store), instance_key, composition);
                let key =
                    serde_json::to_string(&key_value).unwrap_or_else(|_| item_ordinal.to_string());
                match store.loop_identity() {
                    Some(parent) => format!("{parent}/{key}"),
                    None => key,
                }
            })
            .or_else(|| store.loop_identity().map(str::to_owned));
        let item_store = LayeredStore {
            base: store,
            item_name: &for_node.item_name,
            item_value: item_val,
            loop_identity,
        };
        for (child_index, child) in for_node.children.iter().enumerate() {
            let mut built = build_widget_nodes(
                child,
                manifest,
                build_style,
                parent_style,
                child_context,
                Some(&item_store as &dyn VariableStore),
                instance_key,
                composition,
            );
            for (fragment_index, child) in built.iter_mut().enumerate() {
                if let Some(identity) = item_store.loop_identity() {
                    let identity = format!("{identity}/{child_index}/{fragment_index}");
                    if seen_keyed_identities.insert(identity.clone()) {
                        child.set_loop_identity(identity);
                    } else {
                        tracing::warn!(key = %for_node.key.as_deref().unwrap_or_default(), identity, "duplicate keyed loop value; retaining positional identity for this occurrence");
                    }
                }
            }
            children.extend(built);
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
    selective: Option<(&WidgetNode, &HashSet<NodeId>)>,
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
    let element_state = authored_element_state(&attributes);
    let inherited_mask = inherited_style_mask(
        build_style.rules,
        tag,
        style_classes,
        style_id,
        element_state,
        container_context,
    );

    let computed_style = build_style
        .resolver
        .resolve_node_style_for_module_indexed_with_inline_style_and_parent_style(
            build_style.rules,
            build_style.index.as_ref(),
            tag,
            style_classes,
            style_id,
            attributes.get("style").map(String::as_str),
            container_context,
            element_state,
            Some(&manifest.package.id),
            parent_style,
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
    let mut accessibility_baseline = mesh_core_elements::AccessibilityInfo::default();
    accessibility_baseline.role = node.accessibility.role.clone();
    accessibility_baseline.focusable = node.accessibility.focusable;
    node.set_accessibility_baseline(accessibility_baseline);

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
    let previous_children: Option<&[WidgetNode]> = selective
        .map(|(previous, _)| -> &[WidgetNode] { &previous.children })
        .filter(|children: &&[WidgetNode]| children.len() == element.children.len());
    node.children = element
        .children
        .iter()
        .enumerate()
        .flat_map(|(index, child)| {
            if matches!(
                child,
                TemplateNode::If(_)
                    | TemplateNode::For(_)
                    | TemplateNode::Slot(mesh_core_component::template::SlotNode {
                        customizable: true,
                        ..
                    })
            ) {
                return build_widget_nodes(
                    child,
                    manifest,
                    build_style,
                    Some(&node.computed_style),
                    child_context,
                    state,
                    instance_key,
                    composition,
                );
            }
            if let Some((_, rebuild_node_ids)) = selective {
                vec![build_widget_node_inner(
                    child,
                    manifest,
                    build_style,
                    Some(&node.computed_style),
                    child_context,
                    state,
                    instance_key,
                    composition,
                    previous_children
                        .and_then(|children| children.get(index))
                        .map(|previous| (previous, rebuild_node_ids)),
                )]
            } else {
                vec![build_widget_node(
                    child,
                    manifest,
                    build_style,
                    Some(&node.computed_style),
                    child_context,
                    state,
                    instance_key,
                    composition,
                )]
            }
        })
        .collect();

    node
}

#[cfg(test)]
mod tests;
