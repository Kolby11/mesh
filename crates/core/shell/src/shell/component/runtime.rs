use super::*;

fn scheduled_handler_name(instance_key: &str, handler: &str) -> String {
    if handler.starts_with("__mesh_embed__::") {
        handler.to_string()
    } else {
        let mut namespaced = String::with_capacity(
            "__mesh_embed__::".len() + instance_key.len() + "::".len() + handler.len(),
        );
        namespaced.push_str("__mesh_embed__::");
        namespaced.push_str(instance_key);
        namespaced.push_str("::");
        namespaced.push_str(handler);
        namespaced
    }
}

fn render_stack_contains_cycle(stack: &[String], module_id: &str) -> bool {
    stack
        .iter()
        .filter(|ancestor| ancestor.as_str() == module_id)
        .nth(1)
        .is_some()
}

fn local_component_runtime_id(host_module_id: &str, alias: &str) -> String {
    let mut component_id = String::with_capacity(host_module_id.len() + 2 + alias.len());
    component_id.push_str(host_module_id);
    component_id.push_str("::");
    component_id.push_str(alias);
    component_id
}

fn apply_runtime_props(
    runtime: &mut EmbeddedFrontendRuntime,
    props: &HashMap<String, serde_json::Value>,
    skip_unchanged: bool,
) {
    for (key, value) in props {
        let result = if skip_unchanged {
            runtime
                .script_ctx
                .set_member_state_if_changed_ref(key, value)
                .map(|_| ())
        } else {
            runtime.script_ctx.set_member_state(key, value.clone())
        };
        if let Err(err) = result {
            tracing::warn!(
                module_id = %runtime.module_id,
                prop = %key,
                error = %err,
                "failed to apply embedded component prop to script member"
            );
            runtime
                .script_ctx
                .state_mut()
                .set(key.clone(), value.clone());
        }
    }
}

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
                            namespaced_handler: scheduled_handler_name(instance_key, handler),
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
        let Some(node) = find_node_by_key(tree, node_key) else {
            return Ok(Vec::new());
        };
        let Some((handler, merged_args)) = node_handler_and_args(node, event_name, args) else {
            return Ok(Vec::new());
        };
        self.call_namespaced_handler(&handler, &merged_args)
    }

    /// Same as `call_node_handler`, but reads the handler off an
    /// already-resolved node instead of re-walking the tree by key. Used by
    /// callers (hover-transition dispatch) that resolved several nodes in one
    /// pass via `find_nodes_by_keys`.
    pub(super) fn call_resolved_node_handler(
        &mut self,
        node: &WidgetNode,
        event_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some((handler, merged_args)) = node_handler_and_args(node, event_name, args) else {
            return Ok(Vec::new());
        };
        self.call_namespaced_handler(&handler, &merged_args)
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
            apply_service_update_with_name(
                script_ctx.state_mut(),
                true,
                "locale",
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
                apply_service_update_with_name(
                    script_ctx.state_mut(),
                    true,
                    service_name,
                    "<cached>",
                    payload.clone(),
                );
            }
        }

        let template_expressions = mesh_core_frontend::collect_template_expressions(component);
        if component.script.is_some() || !template_expressions.is_empty() {
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
                .compile_and_execute_component(
                    component
                        .script
                        .as_ref()
                        .map_or("", |script| script.source.as_str()),
                    &interface_imports,
                    &template_expressions,
                )
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
            if component.script.is_some() {
                script_ctx
                    .call_init()
                    .map_err(|source| ComponentError::Script {
                        component_id: component_id.clone(),
                        source,
                    })?;
            }
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
        {
            let mut runtimes = self.runtimes.lock().unwrap();
            if let Some(runtime) = runtimes.get_mut(instance_key) {
                apply_runtime_props(runtime, props, true);
                return Ok(());
            }
        }

        let Some(entry) = self.frontend_catalog.modules.get(module_id) else {
            return Err(ComponentError::Failed {
                component_id: self.id().to_string(),
                message: format!("missing embedded frontend module '{module_id}'"),
            });
        };
        let mut runtime = self.create_runtime(instance_key, &entry.compiled, props)?;
        Self::call_runtime_render_hook(&self.diagnostics, &mut runtime);
        apply_runtime_props(&mut runtime, props, false);
        self.runtimes
            .lock()
            .unwrap()
            .insert(instance_key.to_string(), runtime);

        Ok(())
    }

    pub(super) fn build_error_widget(&self, message: impl Into<String>) -> WidgetNode {
        self.has_error_placeholders.set(true);
        self.error_placeholder_marks
            .set(self.error_placeholder_marks.get().wrapping_add(1));
        bounded_error_widget(message)
    }

    pub(super) fn ensure_local_component_runtime(
        &self,
        instance_key: &str,
        host_module_id: &str,
        host_manifest: &mesh_core_module::Manifest,
        alias: &str,
        component: &mesh_core_component::ComponentFile,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        {
            let mut runtimes = self.runtimes.lock().unwrap();
            if let Some(runtime) = runtimes.get_mut(instance_key) {
                apply_runtime_props(runtime, props, true);
                return Ok(());
            }
        }

        let mut runtime = self.create_runtime_for_component(
            instance_key,
            local_component_runtime_id(host_module_id, alias),
            host_manifest,
            component,
            props,
        )?;
        Self::call_runtime_render_hook(&self.diagnostics, &mut runtime);
        apply_runtime_props(&mut runtime, props, false);
        self.runtimes
            .lock()
            .unwrap()
            .insert(instance_key.to_string(), runtime);

        Ok(())
    }

    pub(super) fn render_local_component(
        &self,
        host: &mesh_core_module::Manifest,
        alias: &str,
        component: &mesh_core_component::ComponentFile,
        instance_key: &str,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
        host_rules: &[mesh_core_component::style::StyleRule],
    ) -> WidgetNode {
        if let Err(err) = self.ensure_local_component_runtime(
            instance_key,
            &host.package.id,
            host,
            alias,
            component,
            props,
        ) {
            return self.build_error_widget(err.to_string());
        }

        let theme = self.active_theme.borrow().clone();
        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::new(&state, &self.locale);
        let node = mesh_core_frontend::build_embedded_widget_tree_from_component(
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
        if render_stack_contains_cycle(&self.render_stack.borrow(), module_id) {
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
        let tree = entry.compiled.build_tree_with_state(
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
        tree
    }

    pub(super) fn call_namespaced_handler(
        &mut self,
        handler: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let _span = tracing::debug_span!("call_handler", surface = %self.id(), handler).entered();
        let root_instance_key;
        let (instance_key, raw_handler_name) =
            if let Some((instance_key, handler_name)) = parse_namespaced_handler(handler) {
                (instance_key, handler_name)
            } else {
                root_instance_key = self.id().to_string();
                (root_instance_key.as_str(), handler)
            };

        let (handler_name, merged_args) = unpack_handler_args(raw_handler_name, args);

        let mut runtimes = self.runtimes.lock().unwrap();
        let Some(runtime) = runtimes.get_mut(instance_key) else {
            return Ok(Vec::new());
        };
        if let Err(source) = runtime
            .script_ctx
            .call_handler(handler_name.as_ref(), merged_args.as_ref())
        {
            let error_message = source.to_string();
            let component_id = runtime.module_id.clone();
            tracing::warn!(
                component_id = %component_id,
                handler = %handler_name,
                error = %error_message,
                "frontend event handler failed"
            );
            if let Some(diagnostics) = &self.diagnostics {
                diagnostics.record_handler_error(
                    component_id.clone(),
                    handler_name.to_string(),
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
        let mut events = self.drain_local_script_events(instance_key, published);
        // A live `bind:this` cross-call mutated another instance's `_ENV` directly
        // during this handler — a parent calling `child.fn()` (parent→child) or a
        // child firing `self.Event` to a subscribed parent (child→parent). Re-sync
        // every instance linked to this one so its reactive state catches up.
        let (neighbors_dirty, mut neighbor_events) = self.resync_binding_neighbors(instance_key);
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
            if !neighbor.script_ctx.take_live_binding_external_accessed() {
                continue;
            }
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

fn unpack_handler_args<'a>(
    handler_name: &'a str,
    event_args: &'a [serde_json::Value],
) -> (
    std::borrow::Cow<'a, str>,
    std::borrow::Cow<'a, [serde_json::Value]>,
) {
    (
        std::borrow::Cow::Borrowed(handler_name),
        std::borrow::Cow::Borrowed(event_args),
    )
}

fn node_handler_and_args<'a>(
    node: &'a WidgetNode,
    event_name: &str,
    event_args: &'a [serde_json::Value],
) -> Option<(&'a str, std::borrow::Cow<'a, [serde_json::Value]>)> {
    if let Some(call) = node.event_handler_calls.get(event_name) {
        let mut merged = call.args.clone();
        merged.extend_from_slice(event_args);
        return Some((call.handler.as_str(), std::borrow::Cow::Owned(merged)));
    }
    node.event_handlers
        .get(event_name)
        .map(|handler| (handler.as_str(), std::borrow::Cow::Borrowed(event_args)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::shell::component) struct ResolvedManifestText {
    pub(in crate::shell::component) text: String,
    pub(in crate::shell::component) key: Option<String>,
    pub(in crate::shell::component) fallback: Option<String>,
}

#[cfg(test)]
mod handler_call_tests {
    use super::*;
    use mesh_core_elements::EventHandlerCall;

    #[test]
    fn node_handler_and_args_prefers_typed_call_args() {
        let mut node = WidgetNode::new("button");
        node.event_handlers
            .insert("click".into(), "__mesh_embed__::host::legacy".into());
        node.event_handler_calls.insert(
            "click".into(),
            EventHandlerCall {
                handler: "__mesh_embed__::host::typed".into(),
                args: vec![serde_json::json!("prebound")],
            },
        );

        let event_args = [serde_json::json!({ "type": "click" })];
        let (handler, args) = node_handler_and_args(&node, "click", &event_args).expect("handler");

        assert_eq!(handler, "__mesh_embed__::host::typed");
        assert_eq!(args[0], serde_json::json!("prebound"));
        assert_eq!(args[1], serde_json::json!({ "type": "click" }));
    }

    #[test]
    fn unpack_handler_args_borrows_plain_handler_forms() {
        let event_args = [serde_json::json!({ "type": "click" })];
        let (handler, args) = unpack_handler_args("onClick", &event_args);
        assert_eq!(handler.as_ref(), "onClick");
        assert!(matches!(handler, std::borrow::Cow::Borrowed(_)));
        assert_eq!(args.as_ref(), event_args.as_slice());
        assert!(matches!(args, std::borrow::Cow::Borrowed(_)));
    }

    // cargo test -p mesh-core-shell --release -- plain_handler_syntax_gate_beats_failed_json_parse --ignored --nocapture
    #[test]
    #[ignore = "release-only plain handler dispatch microbenchmark"]
    fn plain_handler_syntax_gate_beats_failed_json_parse() {
        fn old_unpack(
            handler_name: &str,
            event_args: &[serde_json::Value],
        ) -> (String, Vec<serde_json::Value>) {
            let _ = serde_json::from_str::<serde_json::Value>(handler_name);
            (handler_name.to_string(), event_args.to_vec())
        }

        let iterations = 500_000;
        let event_args = [serde_json::json!({ "type": "pointermove", "x": 12, "y": 34 })];
        let old_started = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(old_unpack("onPointerMove", &event_args));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(unpack_handler_args("onPointerMove", &event_args));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "plain handler unpack: failed JSON parse {old_time:?}; syntax gate {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- plain_handler_borrowing_beats_clone_unpack --ignored --nocapture
    #[test]
    #[ignore = "release-only plain handler borrowing microbenchmark"]
    fn plain_handler_borrowing_beats_clone_unpack() {
        fn old_plain_unpack(
            handler_name: &str,
            event_args: &[serde_json::Value],
        ) -> (String, Vec<serde_json::Value>) {
            (handler_name.to_string(), event_args.to_vec())
        }

        let iterations = 500_000;
        let event_args = [serde_json::json!({ "type": "pointermove", "x": 12, "y": 34 })];
        let old_started = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(old_plain_unpack("onPointerMove", &event_args));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        for _ in 0..iterations {
            let (handler, args) = unpack_handler_args("onPointerMove", &event_args);
            std::hint::black_box((handler.as_ref(), args.as_ref()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "plain handler arg transfer: clone {old_time:?}; borrow {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- namespaced_handler_resolution_borrows_parts --ignored --nocapture
    #[test]
    #[ignore = "release-only namespaced handler target microbenchmark"]
    fn namespaced_handler_resolution_borrows_parts() {
        fn old_resolve(handler: &str, fallback_id: &str) -> (String, String, String) {
            if let Some((instance_key, handler_name)) = parse_namespaced_handler(handler) {
                (
                    instance_key.to_string(),
                    handler_name.to_string(),
                    format!("{instance_key}:component"),
                )
            } else {
                (
                    fallback_id.to_string(),
                    handler.to_string(),
                    fallback_id.to_string(),
                )
            }
        }

        fn new_resolve<'a>(
            handler: &'a str,
            fallback_id: &'a str,
        ) -> (&'a str, &'a str, Option<String>) {
            if let Some((instance_key, handler_name)) = parse_namespaced_handler(handler) {
                (instance_key, handler_name, None)
            } else {
                (fallback_id, handler, None)
            }
        }

        let iterations = 500_000;
        let handler = "__mesh_embed__::@mesh/settings/local:theme-selector::onThemeLight";
        let fallback = "@mesh/settings";

        let old_started = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(old_resolve(handler, fallback));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        for _ in 0..iterations {
            std::hint::black_box(new_resolve(handler, fallback));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "namespaced handler target: clone {old_time:?}; borrow {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    // Run with:
    // nix develop --command cargo test -p mesh-core-shell --release -- typed_handler_call_args_beat_json_unpack --ignored --nocapture
    #[test]
    #[ignore]
    fn typed_handler_call_args_beat_json_unpack() {
        use std::time::Instant;

        fn old_json_descriptor_unpack(
            handler_name: &str,
            event_args: &[serde_json::Value],
        ) -> (String, Vec<serde_json::Value>) {
            let parsed = serde_json::from_str::<serde_json::Value>(handler_name).unwrap();
            let handler = parsed
                .get("h")
                .and_then(serde_json::Value::as_str)
                .unwrap()
                .to_string();
            let mut args = parsed
                .get("a")
                .and_then(serde_json::Value::as_array)
                .unwrap()
                .clone();
            args.extend_from_slice(event_args);
            (handler, args)
        }

        let iterations = 200_000usize;
        let legacy_handler = serde_json::json!({
            "h": "__mesh_embed__::host::selectItem",
            "a": ["alpha", "beta", "gamma"]
        })
        .to_string();
        let event_args = [serde_json::json!({ "type": "click", "x": 12, "y": 34 })];

        let legacy_start = Instant::now();
        for _ in 0..iterations {
            let (handler, args) = old_json_descriptor_unpack(&legacy_handler, &event_args);
            assert_eq!(handler, "__mesh_embed__::host::selectItem");
            assert_eq!(args.len(), 4);
        }
        let legacy_ns = legacy_start.elapsed().as_nanos().max(1);

        let mut node = WidgetNode::new("button");
        node.event_handler_calls.insert(
            "click".into(),
            EventHandlerCall {
                handler: "__mesh_embed__::host::selectItem".into(),
                args: vec![
                    serde_json::json!("alpha"),
                    serde_json::json!("beta"),
                    serde_json::json!("gamma"),
                ],
            },
        );

        let typed_start = Instant::now();
        for _ in 0..iterations {
            let (handler, args) =
                node_handler_and_args(&node, "click", &event_args).expect("handler");
            assert_eq!(handler, "__mesh_embed__::host::selectItem");
            assert_eq!(args.len(), 4);
        }
        let typed_ns = typed_start.elapsed().as_nanos();

        eprintln!("legacy_json_unpack={legacy_ns}ns typed_handler_call={typed_ns}ns");
        assert!(
            typed_ns < legacy_ns,
            "typed handler calls should avoid per-dispatch JSON parsing"
        );
    }
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

#[cfg(test)]
mod scheduled_handler_tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn scheduled_handler_name_namespaces_plain_handlers_once() {
        assert_eq!(
            scheduled_handler_name("@mesh/panel/local:Clock", "close"),
            "__mesh_embed__::@mesh/panel/local:Clock::close"
        );
        assert_eq!(
            scheduled_handler_name("@mesh/panel", "__mesh_embed__::@mesh/panel::close"),
            "__mesh_embed__::@mesh/panel::close"
        );
    }

    #[test]
    fn runtime_key_helpers_match_legacy_behavior() {
        assert_eq!(
            local_component_runtime_id("@mesh/panel", "Toolbar"),
            "@mesh/panel::Toolbar"
        );
        let stack = vec![
            "@mesh/panel".to_string(),
            "@mesh/audio".to_string(),
            "@mesh/panel".to_string(),
        ];
        assert!(render_stack_contains_cycle(&stack, "@mesh/panel"));
        assert!(!render_stack_contains_cycle(&stack, "@mesh/audio"));
    }

    // cargo test -p mesh-core-shell --release -- embedded_runtime_helpers_beat_format_and_full_cycle_scan_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only embedded runtime helper microbenchmark"]
    fn embedded_runtime_helpers_beat_format_and_full_cycle_scan_benchmark() {
        let host = "@mesh/panel/local:StatusCluster/import:NetworkControls";
        let alias = "BatteryIndicator";
        let module = "@mesh/panel";
        let mut stack = vec![module.to_string(), module.to_string()];
        stack.extend((0..62).map(|index| format!("@mesh/other-{index}")));
        let iterations = 1_000_000;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let id = format!("{host}::{alias}");
            let is_cycle = stack
                .iter()
                .filter(|ancestor| ancestor.as_str() == module)
                .count()
                >= 2;
            old_total ^= std::hint::black_box(id.len() + usize::from(is_cycle));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let id = local_component_runtime_id(host, alias);
            let is_cycle = render_stack_contains_cycle(&stack, module);
            new_total ^= std::hint::black_box(id.len() + usize::from(is_cycle));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "embedded runtime helpers: format/full scan {old_time:?}; presized/short-circuit {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- scheduled_handler_name_presizing_beats_format_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only scheduled handler namespace microbenchmark"]
    fn scheduled_handler_name_presizing_beats_format_benchmark() {
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
            new_total ^= std::hint::black_box(scheduled_handler_name(instance_key, handler).len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "scheduled handler namespace: format {old_time:?}; presized {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- scheduled_handler_cached_name_beats_tick_format --ignored --nocapture
    #[test]
    #[ignore = "release-only scheduled handler dispatch microbenchmark"]
    fn scheduled_handler_cached_name_beats_tick_format() {
        let now = Instant::now();
        let old_handlers = (0..256)
            .map(|index| {
                (
                    format!("handler-{index}"),
                    (
                        format!("@mesh/panel/local:Child{index}"),
                        "closeBridge".to_string(),
                        now - Duration::from_millis(1),
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
        let new_handlers = old_handlers
            .iter()
            .map(|(key, (instance_key, handler, deadline))| {
                (
                    key.clone(),
                    ScheduledHandler {
                        namespaced_handler: scheduled_handler_name(instance_key, handler),
                        deadline: *deadline,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let iterations = 20_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let due = old_handlers
                .iter()
                .filter(|(_, (_, _, deadline))| *deadline <= now)
                .map(|(key, (instance_key, handler, _))| {
                    (
                        key.clone(),
                        format!("__mesh_embed__::{instance_key}::{handler}"),
                    )
                })
                .collect::<Vec<_>>();
            old_total = old_total.wrapping_add(std::hint::black_box(due.len()));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let due = new_handlers
                .iter()
                .filter(|(_, scheduled)| scheduled.deadline <= now)
                .map(|(key, scheduled)| (key.clone(), scheduled.namespaced_handler.clone()))
                .collect::<Vec<_>>();
            new_total = new_total.wrapping_add(std::hint::black_box(due.len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "scheduled handler dispatch prep: format-on-tick {old_time:?}; cached-name {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }
}
