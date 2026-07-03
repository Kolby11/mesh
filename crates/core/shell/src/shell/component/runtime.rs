use super::*;

impl FrontendSurfaceComponent {
    fn drain_local_script_events(
        &mut self,
        instance_key: &str,
        events: Vec<PublishedEvent>,
    ) -> Vec<CoreRequest> {
        let mut shell_events = Vec::new();
        for event in events {
            match event.channel.as_str() {
                "shell.schedule-handler" => {
                    let Some(key) = event.payload.get("key").and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    let Some(handler) = event
                        .payload
                        .get("handler")
                        .and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    let delay_ms = event
                        .payload
                        .get("delay_ms")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0)
                        .min(5_000);
                    self.scheduled_handlers.insert(
                        key.to_string(),
                        ScheduledHandler {
                            instance_key: instance_key.to_string(),
                            handler: handler.to_string(),
                            deadline: Instant::now() + Duration::from_millis(delay_ms),
                        },
                    );
                }
                "shell.cancel-handler" => {
                    if let Some(key) = event.payload.get("key").and_then(|value| value.as_str()) {
                        self.scheduled_handlers.remove(key);
                    }
                }
                _ => shell_events.push(event),
            }
        }
        script_events_to_requests(shell_events)
    }

    pub(super) fn call_node_handler(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
        event_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(handler) = find_event_handler(tree, node_key, event_name) else {
            return Ok(Vec::new());
        };
        self.call_namespaced_handler(&handler, args)
    }

    pub(super) fn call_render_hooks(&mut self) {
        let mut state_dirty = false;
        let mut runtimes = self.runtimes.lock().unwrap();
        for runtime in runtimes.values_mut() {
            if Self::call_runtime_render_hook(&self.diagnostics, runtime) {
                state_dirty = true;
            }
        }
        drop(runtimes);
        if state_dirty {
            self.invalidate_script_state();
        }
    }

    pub(super) fn call_runtime_render_hook(
        diagnostics: &Option<Diagnostics>,
        runtime: &mut EmbeddedFrontendRuntime,
    ) -> bool {
        if !runtime.script_ctx.has_handler("render") {
            return false;
        }

        if let Err(source) = runtime.script_ctx.call_render_lifecycle() {
            let component_id = runtime.module_id.clone();
            let error_message = source.to_string();
            tracing::warn!(
                component_id = %component_id,
                handler = "render",
                error = %error_message,
                "frontend render hook failed"
            );
            if let Some(diagnostics) = diagnostics {
                diagnostics.record_handler_error(component_id, "render".to_string(), error_message);
            }
            Self::drain_script_diagnostics(diagnostics, runtime);
            return false;
        }
        Self::drain_script_diagnostics(diagnostics, runtime);
        runtime.script_ctx.state().is_dirty()
    }

    pub(super) fn drain_script_diagnostics(
        diagnostics: &Option<Diagnostics>,
        runtime: &mut EmbeddedFrontendRuntime,
    ) {
        let Some(diagnostics) = diagnostics else {
            return;
        };
        for diagnostic in runtime.script_ctx.drain_diagnostics() {
            diagnostics.error(format!(
                "interface '{}' unavailable for '{}': {}",
                diagnostic.interface, diagnostic.module_id, diagnostic.reason
            ));
        }
    }

    pub(super) fn runtime_state(
        &self,
        instance_key: &str,
    ) -> Option<Arc<mesh_core_scripting::ScriptState>> {
        let mut runtimes = self.runtimes.lock().unwrap();
        let runtime = runtimes.get_mut(instance_key)?;
        let generation = runtime.script_ctx.state().mutation_generation();
        if let Some((cached_generation, cached)) = runtime.cached_state_clone.as_ref()
            && *cached_generation == generation
        {
            return Some(Arc::clone(cached));
        }
        let snapshot = Arc::new(runtime.script_ctx.state().clone());
        runtime.cached_state_clone = Some((generation, Arc::clone(&snapshot)));
        Some(snapshot)
    }

    pub(super) fn load_graph_i18n_catalogs(&mut self) {
        for (module_id, locale, path) in &self.graph_i18n_catalogs {
            let Ok(content) = std::fs::read_to_string(path) else {
                tracing::warn!(
                    "module '{}': failed to read graph i18n catalog {}",
                    self.id(),
                    path.display()
                );
                continue;
            };
            let messages: HashMap<String, String> = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(_) => {
                    tracing::warn!(
                        "module '{}': failed to parse graph i18n catalog {}",
                        self.id(),
                        path.display()
                    );
                    continue;
                }
            };
            tracing::debug!(
                "module '{}': loaded {} graph translations for locale '{}'",
                self.id(),
                messages.len(),
                locale
            );
            self.locale.load_module_translations(
                module_id,
                mesh_core_locale::TranslationSet {
                    locale: locale.clone(),
                    messages,
                },
            );
        }
    }

    pub(super) fn create_runtime_for_component(
        &self,
        instance_key: &str,
        component_id: String,
        manifest: &mesh_core_module::Manifest,
        component: &mesh_core_component::ComponentFile,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        let mut script_ctx = ScriptContext::new_for_instance(
            manifest.package.id.clone(),
            component_id.clone(),
            instance_key.to_string(),
            grant_capabilities_from_manifest(manifest),
        )
        .map_err(|source| ComponentError::Script {
            component_id: component_id.clone(),
            source,
        })?;
        // All components in this surface share one Lua realm so bind:this is a
        // live cross-component reference rather than a snapshot.
        script_ctx.attach_shared_vm(&self.surface_vm);
        script_ctx.set_interface_catalog(self.interface_catalog.clone());
        script_ctx.set_optional_interfaces(
            manifest
                .dependencies
                .interfaces
                .iter()
                .filter(|dep| !dep.required)
                .map(|dep| dep.name.clone())
                .collect(),
        );
        seed_service_state(script_ctx.state_mut());
        script_ctx
            .set_global_state("this", self.module_descriptor_from_manifest(manifest))
            .map_err(|source| ComponentError::Script {
                component_id: component_id.clone(),
                source,
            })?;
        if script_has_service_read(&script_ctx, "mesh.locale", "locale") {
            let payload = serde_json::json!({
                "locale": self.locale.current(),
                "current": self.locale.current()
            });
            apply_service_update(
                script_ctx.state_mut(),
                true,
                "mesh.locale",
                "@mesh/shell",
                &payload,
            );
            script_ctx.apply_service_payload("locale", &payload);
        }
        for (key, value) in props {
            script_ctx.state_mut().set(key.clone(), value.clone());
        }
        publish_resolved_props(
            &mut script_ctx,
            component,
            props,
            &self.settings_json,
            instance_key,
        );
        for (service_name, payload) in &self.cached_service_payloads {
            let interface = format!("mesh.{service_name}");
            // Always seed the Lua-level service payload so interface proxies
            // can read state fields regardless of read capability.
            script_ctx.apply_service_payload(service_name, payload);
            if script_has_service_read(&script_ctx, &interface, service_name) {
                apply_service_update(
                    script_ctx.state_mut(),
                    true,
                    &interface,
                    "<cached>",
                    payload.clone(),
                );
            }
        }

        if let Some(script) = &component.script {
            let interface_imports = component
                .imports
                .iter()
                .filter_map(|import| match &import.target {
                    mesh_core_component::ComponentImportTarget::InterfaceApi {
                        interface,
                        version,
                    } => Some(ScriptInterfaceImport {
                        alias: import.alias.clone(),
                        interface: interface.clone(),
                        version: version.clone(),
                    }),
                    _ => None,
                })
                .collect::<Vec<_>>();
            script_ctx
                .compile_and_execute(&script.source, &interface_imports)
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
            script_ctx
                .call_init()
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
        }

        Ok(EmbeddedFrontendRuntime {
            module_id: component_id,
            script_ctx,
            cached_state_clone: None,
        })
    }

    pub(super) fn create_runtime(
        &self,
        instance_key: &str,
        compiled: &CompiledFrontendModule,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        self.create_runtime_for_component(
            instance_key,
            compiled.manifest.package.id.clone(),
            &compiled.manifest,
            &compiled.component,
            props,
        )
    }

    pub(super) fn init_root_runtime(&self) -> Result<(), ComponentError> {
        let mut props = HashMap::new();
        props.insert("settings".into(), self.settings_json.clone());
        let runtime = self.create_runtime(self.id(), &self.compiled, &props)?;
        self.runtimes
            .lock()
            .unwrap()
            .insert(self.id().to_string(), runtime);
        Ok(())
    }

    pub(super) fn ensure_runtime(
        &self,
        instance_key: &str,
        module_id: &str,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        if !self.runtimes.lock().unwrap().contains_key(instance_key) {
            let Some(entry) = self.frontend_catalog.modules.get(module_id) else {
                return Err(ComponentError::Failed {
                    component_id: self.id().to_string(),
                    message: format!("missing embedded frontend module '{module_id}'"),
                });
            };
            let runtime = self.create_runtime(instance_key, &entry.compiled, props)?;
            let mut runtime = runtime;
            Self::call_runtime_render_hook(&self.diagnostics, &mut runtime);
            self.runtimes
                .lock()
                .unwrap()
                .insert(instance_key.to_string(), runtime);
        }

        if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(instance_key) {
            for (key, value) in props {
                runtime
                    .script_ctx
                    .state_mut()
                    .set(key.clone(), value.clone());
            }
        }

        Ok(())
    }

    pub(super) fn build_error_widget(&self, message: impl Into<String>) -> WidgetNode {
        self.has_error_placeholders.set(true);
        bounded_error_widget(message)
    }

    pub(super) fn ensure_local_component_runtime(
        &self,
        instance_key: &str,
        host_module_id: &str,
        host_manifest: &mesh_core_module::Manifest,
        alias: &str,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        if !self.runtimes.lock().unwrap().contains_key(instance_key) {
            let Some(entry) = self.frontend_catalog.modules.get(host_module_id) else {
                return Err(ComponentError::Failed {
                    component_id: self.id().to_string(),
                    message: format!("missing host module '{host_module_id}'"),
                });
            };
            let Some(component) = entry.compiled.local_components.get(alias) else {
                return Err(ComponentError::Failed {
                    component_id: self.id().to_string(),
                    message: format!("missing local component import '{alias}'"),
                });
            };
            let runtime = self.create_runtime_for_component(
                instance_key,
                format!("{host_module_id}::{alias}"),
                host_manifest,
                component,
                props,
            )?;
            let mut runtime = runtime;
            Self::call_runtime_render_hook(&self.diagnostics, &mut runtime);
            self.runtimes
                .lock()
                .unwrap()
                .insert(instance_key.to_string(), runtime);
        }

        if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(instance_key) {
            for (key, value) in props {
                runtime
                    .script_ctx
                    .state_mut()
                    .set(key.clone(), value.clone());
            }
        }

        Ok(())
    }

    pub(super) fn render_local_component(
        &self,
        host: &mesh_core_module::Manifest,
        alias: &str,
        instance_key: &str,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
    ) -> WidgetNode {
        if let Err(err) =
            self.ensure_local_component_runtime(instance_key, &host.package.id, host, alias, props)
        {
            return self.build_error_widget(err.to_string());
        }

        let Some(entry) = self.frontend_catalog.modules.get(&host.package.id) else {
            return self.build_error_widget(format!("missing host module '{}'", host.package.id));
        };
        let Some(component) = entry.compiled.local_components.get(alias) else {
            return self.build_error_widget(format!("missing local component import '{alias}'"));
        };

        let theme = self.active_theme.borrow().clone();
        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::new(&state, &self.locale);
        let host_rules = entry
            .compiled
            .component
            .style
            .as_ref()
            .map(|style| style.rules.as_slice())
            .unwrap_or(&[]);
        let mut node = mesh_core_frontend::build_widget_tree_from_component(
            component,
            host,
            &theme,
            container_width,
            container_height,
            Some(self),
            instance_key,
            Some(&bound),
            host_rules,
        );
        namespace_event_handlers(&mut node, instance_key);
        node
    }

    pub(super) fn render_embedded_instance(
        &self,
        instance_key: &str,
        module_id: &str,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
    ) -> WidgetNode {
        if self
            .render_stack
            .borrow()
            .iter()
            .filter(|ancestor| ancestor.as_str() == module_id)
            .count()
            >= 2
        {
            return self.build_error_widget(format!("composition cycle blocked for '{module_id}'"));
        }

        if let Err(err) = self.ensure_runtime(instance_key, module_id, props) {
            return self.build_error_widget(err.to_string());
        }

        let Some(entry) = self.frontend_catalog.modules.get(module_id) else {
            return self.build_error_widget(format!("missing embedded module '{module_id}'"));
        };

        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::new(&state, &self.locale);
        let active_theme = self.active_theme.borrow().clone();
        self.render_stack.borrow_mut().push(module_id.to_string());
        let measurer = SharedTextMeasurer;
        let mut tree = entry.compiled.build_tree_with_state(
            &active_theme,
            container_width.max(0.0).ceil() as u32,
            container_height.max(0.0).ceil() as u32,
            Some(&bound),
            FrontendRenderMode::Embedded,
            instance_key,
            Some(self),
            Some(&measurer),
        );
        self.render_stack.borrow_mut().pop();
        namespace_event_handlers(&mut tree, instance_key);
        tree
    }

    pub(super) fn call_namespaced_handler(
        &mut self,
        handler: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let (instance_key, raw_handler_name, component_id) =
            if let Some((instance_key, handler_name)) = parse_namespaced_handler(handler) {
                let component_id = self
                    .runtimes
                    .lock()
                    .unwrap()
                    .get(instance_key)
                    .map(|runtime| runtime.module_id.clone())
                    .unwrap_or_else(|| self.id().to_string());
                (
                    instance_key.to_string(),
                    handler_name.to_string(),
                    component_id,
                )
            } else {
                (
                    self.id().to_string(),
                    handler.to_string(),
                    self.id().to_string(),
                )
            };

        let (handler_name, merged_args) = unpack_handler_args(&raw_handler_name, args);

        let mut runtimes = self.runtimes.lock().unwrap();
        let Some(runtime) = runtimes.get_mut(&instance_key) else {
            return Ok(Vec::new());
        };
        if let Err(source) = runtime.script_ctx.call_handler(&handler_name, &merged_args) {
            let error_message = source.to_string();
            tracing::warn!(
                component_id = %component_id,
                handler = %handler_name,
                error = %error_message,
                "frontend event handler failed"
            );
            if let Some(diagnostics) = &self.diagnostics {
                diagnostics.record_handler_error(
                    component_id.clone(),
                    handler_name.clone(),
                    error_message,
                );
            }
            Self::drain_script_diagnostics(&self.diagnostics, runtime);
            return Ok(Vec::new());
        }
        Self::drain_script_diagnostics(&self.diagnostics, runtime);
        let state_dirty = runtime.script_ctx.state().is_dirty();
        let published = runtime.script_ctx.drain_published_events();
        drop(runtimes);
        let mut events = self.drain_local_script_events(&instance_key, published);
        // A live `bind:this` cross-call mutated another instance's `_ENV` directly
        // during this handler — a parent calling `child.fn()` (parent→child) or a
        // child firing `self.Event` to a subscribed parent (child→parent). Re-sync
        // every instance linked to this one so its reactive state catches up.
        let (neighbors_dirty, mut neighbor_events) = self.resync_binding_neighbors(&instance_key);
        events.append(&mut neighbor_events);
        if state_dirty || neighbors_dirty {
            self.invalidate_script_state();
        }

        // Execute imperative element actions the handler queued via live
        // `refs.<name>` references (focus/blur), routed through the real focus path.
        let mut element_requests = self.apply_element_actions()?;
        events.append(&mut element_requests);

        Ok(events)
    }

    /// Re-sync every instance linked to `instance_key` by a live `bind:this`
    /// reference, in either direction: children this instance binds (it may have
    /// called their functions) and parents that bind this instance (it may have
    /// fired `self.<Event>` to them). A live cross-call mutates the neighbor's
    /// `_ENV` in the shared VM without going through the shell's normal
    /// post-handler sync, so its Rust-side reactive state would otherwise stay
    /// stale until some other event re-rendered it.
    fn resync_binding_neighbors(&mut self, instance_key: &str) -> (bool, Vec<CoreRequest>) {
        let neighbor_keys: Vec<String> = {
            let bound_children = self.bound_children.borrow();
            let mut keys: Vec<String> = bound_children
                .get(instance_key)
                .map(|links| links.iter().map(|(_, key)| key.clone()).collect())
                .unwrap_or_default();
            for (parent_key, links) in bound_children.iter() {
                if parent_key != instance_key
                    && links.iter().any(|(_, child)| child == instance_key)
                {
                    keys.push(parent_key.clone());
                }
            }
            keys.sort();
            keys.dedup();
            keys
        };
        if neighbor_keys.is_empty() {
            return (false, Vec::new());
        }

        let mut state_dirty = false;
        let mut events = Vec::new();
        for neighbor_key in neighbor_keys {
            let mut runtimes = self.runtimes.lock().unwrap();
            let Some(neighbor) = runtimes.get_mut(&neighbor_key) else {
                continue;
            };
            neighbor.script_ctx.resync_state();
            Self::drain_script_diagnostics(&self.diagnostics, neighbor);
            if neighbor.script_ctx.state().is_dirty() {
                state_dirty = true;
            }
            let published = neighbor.script_ctx.drain_published_events();
            drop(runtimes);
            events.extend(self.drain_local_script_events(&neighbor_key, published));
        }
        (state_dirty, events)
    }
}

/// If `handler_name` is a JSON object with `h` and `a` fields, unpack it into
/// the real handler name and pre-bound arguments. Otherwise, return as-is.
/// Pre-bound args are prepended to the event args.
fn unpack_handler_args(
    handler_name: &str,
    event_args: &[serde_json::Value],
) -> (String, Vec<serde_json::Value>) {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(handler_name) {
        if let (Some(h), Some(a)) = (
            parsed.get("h").and_then(serde_json::Value::as_str),
            parsed.get("a").and_then(serde_json::Value::as_array),
        ) {
            let mut merged: Vec<serde_json::Value> = a.clone();
            merged.extend_from_slice(event_args);
            return (h.to_string(), merged);
        }
    }
    (handler_name.to_string(), event_args.to_vec())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::shell::component) struct ResolvedManifestText {
    pub(in crate::shell::component) text: String,
    pub(in crate::shell::component) key: Option<String>,
    pub(in crate::shell::component) fallback: Option<String>,
}

/// Publish each declared prop's precedence-resolved value under `props.<name>`
/// in script state. The compiler reads these to project `prop(name)` into CSS,
/// and scripts read them as `props.name`. Precedence applied here: declared
/// default, overridden by an instance prop passed at the embed site.
///
/// User-settings layers (global / per-instance) fold into this same map once the
/// settings projection lands; the funnel key (`props.<name>`) stays the same.
pub(super) fn publish_resolved_props(
    script_ctx: &mut ScriptContext,
    component: &mesh_core_component::ComponentFile,
    instance_props: &HashMap<String, serde_json::Value>,
    settings_json: &serde_json::Value,
    instance_key: &str,
) {
    let Some(block) = &component.props else {
        return;
    };
    let mut props = serde_json::Map::new();
    let global_settings = settings_json
        .pointer("/props/global")
        .and_then(serde_json::Value::as_object);
    let instance_settings = settings_json
        .pointer("/props/instances")
        .and_then(serde_json::Value::as_object)
        .and_then(|instances| instances.get(instance_key))
        .and_then(serde_json::Value::as_object);
    for def in &block.props {
        let resolved = def
            .default
            .as_ref()
            .map(prop_default_to_json)
            .into_iter()
            .chain(
                global_settings
                    .and_then(|settings| settings.get(&def.name))
                    .cloned(),
            )
            .chain(instance_props.get(&def.name).cloned())
            .chain(
                instance_settings
                    .and_then(|settings| settings.get(&def.name))
                    .cloned(),
            )
            .last()
            .and_then(|value| validate_json_prop(def, value));
        if let Some(value) = resolved {
            props.insert(def.name.clone(), value);
        }
    }
    // Publish one reactive `props` table: readable as `props.name` in script and,
    // via `state["props"]`, projected into CSS `prop(name)`. `set_member_state`
    // installs it on the component's own `_ENV`, so script writes (`props.x = y`)
    // round-trip back through `sync_state_from_lua` and repaint.
    if let Err(err) = script_ctx.set_member_state("props", serde_json::Value::Object(props)) {
        tracing::warn!("failed to publish component props: {err}");
    }
}

fn prop_default_to_json(value: &mesh_core_component::PropValue) -> serde_json::Value {
    mesh_core_component::prop_value_to_json(value)
}

fn validate_json_prop(
    def: &mesh_core_component::PropDef,
    value: serde_json::Value,
) -> Option<serde_json::Value> {
    let prop_value = mesh_core_component::json_to_prop_value(value.clone())?;
    match mesh_core_component::validate_prop_value(def, &prop_value) {
        Ok(()) => Some(value),
        Err(err) => {
            tracing::warn!(
                "invalid value for prop `{}` from settings/instance state ignored: {err}",
                def.name
            );
            None
        }
    }
}

pub(super) fn script_has_service_read(
    script_ctx: &ScriptContext,
    interface: &str,
    service_name: &str,
) -> bool {
    let capabilities = &script_ctx.capabilities;
    capabilities.is_granted(&Capability::new(format!("service.{service_name}.read")))
        || (interface == "mesh.theme" && capabilities.is_granted(&Capability::new("theme.read")))
        || (interface == "mesh.locale" && capabilities.is_granted(&Capability::new("locale.read")))
}

impl FrontendSurfaceComponent {
    pub(in crate::shell::component) fn resolve_manifest_text(
        &self,
        module_id: &str,
        field_path: &str,
        text: &mesh_core_module::LocalizedText,
    ) -> ResolvedManifestText {
        match text {
            mesh_core_module::LocalizedText::Literal(value) => ResolvedManifestText {
                text: value.clone(),
                key: None,
                fallback: None,
            },
            mesh_core_module::LocalizedText::Translation { key, fallback } => {
                let resolved = self
                    .locale
                    .translate_for_module(key, module_id)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        if let Some(diagnostics) = &self.diagnostics {
                            diagnostics.degraded(format!(
                                "missing localized manifest text: module_id='{module_id}' field_path='{field_path}' key='{key}' fallback='{fallback}'"
                            ));
                        }
                        text.fallback_text().to_string()
                    });
                ResolvedManifestText {
                    text: resolved,
                    key: Some(key.clone()),
                    fallback: Some(fallback.clone()),
                }
            }
        }
    }

    fn module_descriptor_from_manifest(
        &self,
        manifest: &mesh_core_module::Manifest,
    ) -> serde_json::Value {
        let keybinds = manifest
            .keybinds
            .actions
            .iter()
            .map(|(keybind_id, action)| {
                let mut descriptor = serde_json::Map::new();
                descriptor.insert("id".into(), serde_json::json!(keybind_id));
                descriptor.insert(
                    "scope".into(),
                    serde_json::json!(match action.scope {
                        mesh_core_module::KeybindScope::Surface => "surface",
                        mesh_core_module::KeybindScope::Access => "access",
                    }),
                );
                let label = action.label.clone().unwrap_or_else(|| {
                    mesh_core_module::LocalizedText::Literal(format!("keybind.{keybind_id}.label"))
                });
                insert_resolved_manifest_text(
                    &mut descriptor,
                    "label",
                    self.resolve_manifest_text(
                        &manifest.package.id,
                        &format!("mesh.keybinds.{keybind_id}.label"),
                        &label,
                    ),
                );
                let description = action.description.clone().unwrap_or_else(|| {
                    mesh_core_module::LocalizedText::Literal(format!(
                        "keybind.{keybind_id}.description"
                    ))
                });
                insert_resolved_manifest_text(
                    &mut descriptor,
                    "description",
                    self.resolve_manifest_text(
                        &manifest.package.id,
                        &format!("mesh.keybinds.{keybind_id}.description"),
                        &description,
                    ),
                );
                if let Some(category) = &action.category {
                    insert_resolved_manifest_text(
                        &mut descriptor,
                        "category",
                        self.resolve_manifest_text(
                            &manifest.package.id,
                            &format!("mesh.keybinds.{keybind_id}.category"),
                            category,
                        ),
                    );
                }
                descriptor.insert(
                    "trigger".into(),
                    keybind_trigger_descriptor(&action.trigger),
                );
                descriptor.insert(
                    "localized_triggers".into(),
                    serde_json::Value::Object(
                        action
                            .localized_triggers
                            .iter()
                            .map(|(locale, trigger)| {
                                (locale.clone(), keybind_trigger_descriptor(trigger))
                            })
                            .collect(),
                    ),
                );
                (keybind_id.clone(), serde_json::Value::Object(descriptor))
            })
            .collect::<serde_json::Map<_, _>>();

        serde_json::json!({
            "id": manifest.package.id.clone(),
            "name": manifest.package.name.clone(),
            "version": manifest.package.version.clone(),
            "type": manifest.package.module_type,
            "api_version": manifest.package.api_version.clone(),
            "description": manifest.package.description.clone(),
            "keybinds": keybinds,
        })
    }
}

pub(super) fn bounded_error_widget(message: impl Into<String>) -> WidgetNode {
    let message = message.into();
    let mut node = WidgetNode::new("box");
    let mut text = WidgetNode::new("text");
    text.attributes.insert("content".into(), message.clone());
    text.attributes
        .insert(ERROR_PLACEHOLDER_MARKER.into(), "true".into());
    node.attributes.insert("content".into(), message);
    node.attributes
        .insert(ERROR_PLACEHOLDER_MARKER.into(), "true".into());
    node.children.push(text);
    node
}

fn insert_resolved_manifest_text(
    descriptor: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    resolved: ResolvedManifestText,
) {
    descriptor.insert(field.into(), serde_json::json!(resolved.text));
    if let Some(key) = resolved.key {
        descriptor.insert(format!("{field}_key"), serde_json::json!(key));
    }
    if let Some(fallback) = resolved.fallback {
        descriptor.insert(format!("{field}_fallback"), serde_json::json!(fallback));
    }
}

fn keybind_trigger_descriptor(trigger: &mesh_core_module::KeybindTrigger) -> serde_json::Value {
    serde_json::json!({
        "kind": match trigger.kind {
            mesh_core_module::KeybindTriggerKind::Shortcut => "shortcut",
            mesh_core_module::KeybindTriggerKind::AccessKey => "access_key",
        },
        "key": trigger.key.clone(),
        "modifiers": trigger.modifiers.clone(),
    })
}
