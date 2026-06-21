use std::collections::{BTreeMap, HashMap};

use mesh_core_elements::WidgetNode;
use mesh_core_frontend::FrontendCompositionResolver;
use mesh_core_module::ModuleType;

use super::FrontendSurfaceComponent;

impl FrontendCompositionResolver for FrontendSurfaceComponent {
    fn render_import(
        &self,
        host: &mesh_core_module::Manifest,
        host_instance_key: &str,
        alias: &str,
        props: &BTreeMap<String, String>,
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
                let node = self.render_local_component(
                    &entry.compiled.manifest,
                    alias,
                    &instance_key,
                    &props_json,
                    container_width,
                    container_height,
                );
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
                    .insert(module_id.clone(), binding);
            }
            self.pending_surface_states
                .borrow_mut()
                .insert(module_id, !hidden);
            return Some(WidgetNode::new("box"));
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
        let node = self.render_embedded_instance(
            &instance_key,
            &module_id,
            &props_json,
            container_width,
            container_height,
        );
        if let Some(binding) = bind_this.and_then(|value| simple_state_binding(&value)) {
            self.bind_child_instance(host_instance_key, &binding, &instance_key);
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

            if accepts_widget && entry.compiled.manifest.package.module_type != ModuleType::Widget {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' accepts widgets, but '{}' is {}",
                    contribution.widget_id, entry.compiled.manifest.package.module_type
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
