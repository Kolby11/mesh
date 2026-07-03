use std::collections::{BTreeMap, HashMap};

use mesh_core_elements::style::Dimension;
use mesh_core_elements::{EventHandlerCall, WidgetNode};
use mesh_core_frontend::FrontendCompositionResolver;
use mesh_core_interaction::source_element_tag;
use mesh_core_module::ModuleType;

use super::{FrontendSurfaceComponent, PROMOTED_POPOVER_MARKER};

impl FrontendCompositionResolver for FrontendSurfaceComponent {
    fn render_import(
        &self,
        host: &mesh_core_module::Manifest,
        host_instance_key: &str,
        alias: &str,
        props: &BTreeMap<String, String>,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode> {
        if let Some(entry) = self.frontend_catalog.modules.get(&host.package.id) {
            if entry.compiled.local_components.contains_key(alias) {
                let bind_this = props.get("__mesh_bind_this").cloned();
                let props_json: HashMap<String, serde_json::Value> = props
                    .iter()
                    .filter(|(key, _)| {
                        !key.starts_with("__mesh_binding_") && key.as_str() != "__mesh_bind_this"
                    })
                    .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
                    .collect();
                let instance_key = format!("{host_instance_key}/local:{alias}");
                let mut node = self.render_local_component(
                    &entry.compiled.manifest,
                    alias,
                    &instance_key,
                    &props_json,
                    container_width,
                    container_height,
                );
                apply_prop_handler_calls(&mut node, props, prop_handler_calls);
                if let Some(binding) = bind_this.and_then(|value| simple_state_binding(&value)) {
                    self.bind_child_instance(host_instance_key, &binding, &instance_key);
                }
                return Some(node);
            }
        }

        let module_id = match self
            .frontend_catalog
            .imported_component_module_id(host, alias)
        {
            Ok(id) => id,
            Err(message) => return Some(self.build_error_widget(message)),
        };

        // Surface modules are portals: their visibility is tracked via pending_surface_states
        // and translated to ShowSurface/HideSurface requests in tick(). They render nothing inline.
        let is_surface = self
            .frontend_catalog
            .modules
            .get(&module_id)
            .map(|e| e.compiled.manifest.package.module_type == ModuleType::Surface)
            .unwrap_or(false);
        if is_surface {
            let hidden = props
                .get("hidden")
                .map(|v| v == "true" || v == "True")
                .unwrap_or(false);
            if let Some(binding) = props
                .get("__mesh_binding_hidden")
                .and_then(|binding| simple_state_binding(binding))
            {
                self.portal_hidden_bindings
                    .borrow_mut()
                    .insert(module_id.clone(), (host_instance_key.to_string(), binding));
            }
            self.pending_surface_states
                .borrow_mut()
                .insert(module_id, !hidden);
            let mut placeholder = WidgetNode::new("box");
            placeholder.computed_style.width = Dimension::Px(0.0);
            placeholder.computed_style.height = Dimension::Px(0.0);
            placeholder
                .attributes
                .insert("hidden".into(), "true".into());
            return Some(placeholder);
        }

        let props_json: HashMap<String, serde_json::Value> = props
            .iter()
            .filter(|(key, _)| {
                !key.starts_with("__mesh_binding_") && key.as_str() != "__mesh_bind_this"
            })
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect();
        let bind_this = props.get("__mesh_bind_this").cloned();
        let instance_key = format!("{host_instance_key}/import:{alias}");
        let mut node = self.render_embedded_instance(
            &instance_key,
            &module_id,
            &props_json,
            container_width,
            container_height,
        );
        apply_prop_handler_calls(&mut node, props, prop_handler_calls);
        if let Some(binding) = bind_this.and_then(|value| simple_state_binding(&value)) {
            self.bind_child_instance(host_instance_key, &binding, &instance_key);
        }
        // Inline component modules whose root element is a `<popover>` are never
        // painted inline: the popover is realized as a promoted child `xdg_popup`
        // surface. Mark the embedded wrapper as hidden (so it is skipped by parent
        // painting and hit-testing) and tag it for out-of-flow collapse. The actual
        // `position: absolute` geometry is applied in `finalize_tree` AFTER the
        // restyle pass, because restyle re-resolves `computed_style` purely from CSS
        // and would otherwise wipe any geometry set here. Taking the wrapper out of
        // flow keeps its (full-size) popover subtree intact for
        // `collect_child_surface_requests()` and child-surface painting while
        // preventing it from contributing to the trigger row's layout — otherwise
        // the resting popover would widen the control cluster and overlap
        // neighbouring buttons. Open and closed popovers collapse identically so
        // toggling a hover popover never relayouts its trigger; only open popovers
        // are additionally promoted to a child surface.
        if embedded_root_is_popover(&node) {
            let mut node = node;
            self.has_promoted_popover_wrappers.set(true);
            node.attributes.insert("hidden".into(), "true".into());
            node.attributes
                .insert(PROMOTED_POPOVER_MARKER.into(), "true".into());
            return Some(node);
        }
        Some(node)
    }

    fn render_slot(
        &self,
        host: &mesh_core_module::Manifest,
        host_instance_key: &str,
        slot_name: Option<&str>,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode> {
        let Some(slot_name) = slot_name else {
            return Vec::new();
        };

        let slot_id = format!("{}:{slot_name}", host.package.id);
        let accepts_widget = host
            .provides_slots
            .get(slot_name)
            .and_then(|definition| definition.accepts.as_deref())
            .map(|accepts| accepts == "widget")
            .unwrap_or(false);

        let mut nodes = Vec::new();
        for contribution in self.frontend_catalog.slot_contributions_for(&slot_id) {
            let Some(entry) = self.frontend_catalog.modules.get(&contribution.widget_id) else {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' references missing module '{}'",
                    contribution.widget_id
                )));
                continue;
            };

            let module_type = entry.compiled.manifest.package.module_type;
            if accepts_widget && !matches!(module_type, ModuleType::Widget | ModuleType::Component)
            {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' accepts widgets, but '{}' is {}",
                    contribution.widget_id, module_type
                )));
                continue;
            }

            let props_json: HashMap<String, serde_json::Value> = contribution
                .props
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            let instance_key = format!(
                "{host_instance_key}/slot:{slot_name}/{}",
                contribution.contribution_id
            );
            let mut node = self.render_embedded_instance(
                &instance_key,
                &contribution.widget_id,
                &props_json,
                container_width,
                container_height,
            );
            node.attributes.insert(
                "_mesh_slot_source".into(),
                contribution.source_module_id.clone(),
            );
            nodes.push(node);
        }

        nodes
    }
}

/// Returns true when an embedded component's rendered tree has a `<popover>` as its
/// top-level content, regardless of open state. The root node from
/// `build_tree_with_state` is always a "surface" wrapper, so the actual element is the
/// first child. The popover element paints as a generic `box`, carrying its identity on
/// `data-mesh-element`, so match on `source_element_tag` rather than the raw render tag.
fn embedded_root_is_popover(node: &WidgetNode) -> bool {
    node.children
        .first()
        .is_some_and(|child| source_element_tag(child) == "popover")
}

fn apply_prop_handler_calls(
    node: &mut WidgetNode,
    props: &BTreeMap<String, String>,
    prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
) {
    if prop_handler_calls.is_empty() {
        return;
    }
    for (event_name, handler) in node.event_handlers.clone() {
        let Some((_, call)) = prop_handler_calls
            .iter()
            .find(|(prop_name, _)| props.get(*prop_name) == Some(&handler))
        else {
            continue;
        };
        node.event_handler_calls.insert(
            event_name,
            EventHandlerCall {
                handler,
                args: call.args.clone(),
            },
        );
    }
    for child in &mut node.children {
        apply_prop_handler_calls(child, props, prop_handler_calls);
    }
}

fn simple_state_binding(binding: &str) -> Option<String> {
    let trimmed = binding.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        || trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    Some(trimmed.to_string())
}

impl FrontendSurfaceComponent {
    pub(super) fn bind_child_instance(
        &self,
        host_instance_key: &str,
        binding: &str,
        child_instance_key: &str,
    ) {
        // Live `bind:this`: parent and child share one surface VM, so the parent
        // env holds a proxy table forwarding straight to the child's live `_ENV`.
        // Reads see current values; calls run the child's real function and return
        // its real value synchronously — no snapshot, no queued call stubs.
        let runtimes = self.runtimes.lock().unwrap();
        let (Some(parent), Some(child)) = (
            runtimes.get(host_instance_key),
            runtimes.get(child_instance_key),
        ) else {
            return;
        };
        if let Err(source) = parent
            .script_ctx
            .install_live_binding(binding, &child.script_ctx)
        {
            tracing::warn!(
                component_id = %parent.module_id,
                binding = %binding,
                child_instance_key = %child_instance_key,
                error = %source,
                "failed to install live bound child instance proxy"
            );
            return;
        }
        drop(runtimes);

        // Record the link so the parent's event handlers can re-sync this child
        // after a live cross-call mutates its `_ENV` directly.
        let mut bound_children = self.bound_children.borrow_mut();
        let links = bound_children
            .entry(host_instance_key.to_string())
            .or_default();
        if !links
            .iter()
            .any(|(b, key)| b == binding && key == child_instance_key)
        {
            links.push((binding.to_string(), child_instance_key.to_string()));
        }
    }
}
