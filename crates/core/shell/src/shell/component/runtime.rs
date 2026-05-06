use super::*;

impl FrontendSurfaceComponent {
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
        let mut runtimes = self.runtimes.lock().unwrap();
        for runtime in runtimes.values_mut() {
            if !runtime.script_ctx.has_handler("onRender") {
                continue;
            }

            if let Err(source) = runtime.script_ctx.call_handler("onRender", &[]) {
                let component_id = runtime.module_id.clone();
                let error_message = source.to_string();
                tracing::warn!(
                    component_id = %component_id,
                    handler = "onRender",
                    error = %error_message,
                    "frontend render hook failed"
                );
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.record_handler_error(
                        component_id,
                        "onRender".to_string(),
                        error_message,
                    );
                }
                Self::drain_script_diagnostics(&self.diagnostics, runtime);
                continue;
            }
            Self::drain_script_diagnostics(&self.diagnostics, runtime);

            if runtime.script_ctx.state().is_dirty() {
                self.dirty = true;
            }
        }
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

    pub(super) fn update_local_audio_percent(&self, percent: u32) {
        let percent = percent.min(100);
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            if !runtime
                .script_ctx
                .capabilities
                .is_granted(&Capability::new("service.audio.read"))
            {
                continue;
            }
            let mut audio = runtime
                .script_ctx
                .state()
                .get("audio")
                .unwrap_or_else(|| serde_json::json!({}));
            if let Some(obj) = audio.as_object_mut() {
                obj.insert("percent".into(), serde_json::Value::from(percent));
            }
            runtime.script_ctx.state_mut().set("audio", audio);
        }
    }

    pub(super) fn source_capabilities(&self) -> CapabilitySet {
        grant_capabilities_from_manifest(&self.compiled.manifest)
    }

    pub(super) fn runtime_state(
        &self,
        instance_key: &str,
    ) -> Option<mesh_core_scripting::ScriptState> {
        self.runtimes
            .lock()
            .unwrap()
            .get(instance_key)
            .map(|runtime| runtime.script_ctx.state().clone())
    }

    /// Load translation files from `config/i18n/{locale}.json` inside the module directory.

    pub(super) fn load_module_i18n_from_dir(&mut self, module_dir: &Path) {
        let i18n_dir = module_dir.join("config/i18n");
        let entries = match std::fs::read_dir(&i18n_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let messages: HashMap<String, String> = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(_) => {
                    tracing::warn!(
                        "module '{}': failed to parse i18n file {}",
                        self.id(),
                        path.display()
                    );
                    continue;
                }
            };
            tracing::debug!(
                "module '{}': loaded {} translations for locale '{}'",
                self.id(),
                messages.len(),
                stem
            );
            self.locale
                .load_translations(mesh_core_locale::TranslationSet {
                    locale: stem.to_string(),
                    messages,
                });
        }
    }

    pub(super) fn load_module_i18n(&mut self) {
        let module_dir = self.module_dir.clone();
        self.load_module_i18n_from_dir(&module_dir);
    }

    pub(super) fn load_catalog_i18n(&mut self) {
        let module_dirs: Vec<PathBuf> = self
            .frontend_catalog
            .modules
            .values()
            .map(|entry| entry.module_dir.clone())
            .collect();
        for module_dir in module_dirs {
            self.load_module_i18n_from_dir(&module_dir);
        }
    }

    pub(super) fn create_runtime_for_component(
        &self,
        component_id: String,
        manifest: &mesh_core_module::Manifest,
        component: &mesh_core_component::ComponentFile,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        let mut script_ctx = ScriptContext::new(
            component_id.clone(),
            grant_capabilities_from_manifest(manifest),
        )
        .map_err(|source| ComponentError::Script {
            component_id: component_id.clone(),
            source,
        })?;
        script_ctx.set_interface_catalog(self.interface_catalog.clone());
        seed_service_state(script_ctx.state_mut());

        for (key, value) in props {
            script_ctx.state_mut().set(key.clone(), value.clone());
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
                .load_script_with_interface_imports(&script.source, &interface_imports)
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
        })
    }

    pub(super) fn create_runtime(
        &self,
        compiled: &CompiledFrontendModule,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        self.create_runtime_for_component(
            compiled.manifest.package.id.clone(),
            &compiled.manifest,
            &compiled.component,
            props,
        )
    }

    pub(super) fn init_root_runtime(&self) -> Result<(), ComponentError> {
        let mut props = HashMap::new();
        props.insert("settings".into(), self.settings_json.clone());
        let runtime = self.create_runtime(&self.compiled, &props)?;
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
            let runtime = self.create_runtime(&entry.compiled, props)?;
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
        let message = message.into();
        let mut node = WidgetNode::new("box");
        let mut text = WidgetNode::new("text");
        text.attributes.insert("content".into(), message.clone());
        node.attributes.insert("content".into(), message);
        node.children.push(text);
        node
    }

    pub(super) fn ensure_local_component_runtime(
        &self,
        instance_key: &str,
        host_module_id: &str,
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
                format!("{host_module_id}::{alias}"),
                &entry.compiled.manifest,
                component,
                props,
            )?;
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
            self.ensure_local_component_runtime(instance_key, &host.package.id, alias, props)
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
        let mut node = mesh_core_render::build_widget_tree_from_component(
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
        let (instance_key, handler_name, component_id) =
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

        let mut runtimes = self.runtimes.lock().unwrap();
        let Some(runtime) = runtimes.get_mut(&instance_key) else {
            return Ok(Vec::new());
        };
        if let Err(source) = runtime.script_ctx.call_handler(&handler_name, args) {
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
        if runtime.script_ctx.state().is_dirty() {
            self.dirty = true;
        }

        Ok(script_events_to_requests(
            runtime.script_ctx.drain_published_events(),
        ))
    }
}
