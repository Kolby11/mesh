use super::diagnostics::record_localized_miss as record_localized_miss_diagnostic;
use super::*;
use mesh_core_frontend_abi::{EffectScope, EffectSource, FrontendEffect};
use mesh_core_scripting::{ScriptDiagnosticCategory, ScriptError};

/// Drain a runtime's typed `mesh.shell` effects together with the caller
/// scope they are lowered under. The scope is only built when there is
/// something to lower.
fn take_shell_effects(
    runtime: &mut EmbeddedFrontendRuntime,
) -> Option<(EffectScope, Vec<FrontendEffect>)> {
    if !runtime.script_ctx.has_shell_effects() {
        return None;
    }
    let effects = runtime.script_ctx.drain_shell_effects();
    let scope = EffectScope::new(
        EffectSource::new(
            runtime.script_ctx.module_id.clone(),
            Some(runtime.script_ctx.instance_id.clone()),
        ),
        runtime.script_ctx.capabilities.clone(),
    );
    Some((scope, effects))
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
) -> Result<(), ScriptError> {
    if let Err(err) = runtime.script_ctx.set_member_states(props, skip_unchanged) {
        tracing::warn!(
            module_id = %runtime.module_id,
            error = %err,
            "failed to apply embedded component props to script members"
        );
        return Err(err);
    }
    Ok(())
}

impl Drop for FrontendSurfaceComponent {
    fn drop(&mut self) {
        if let Err(error) = self.unmount_runtimes() {
            tracing::warn!(
                component_id = %self.compiled.manifest.package.id,
                error = %error,
                "frontend component drop could not complete authored unmount"
            );
        }
    }
}

impl FrontendSurfaceComponent {
    /// Dispatch one authored frontend lifecycle hook for a runtime.
    ///
    /// Runtime creation calls `mount` after `init` has completed. Teardown
    /// calls `unmount` before the runtime is removed, and flushes storage even
    /// when the hook is absent or returns an error.
    pub(super) fn dispatch_runtime_hook(
        diagnostics: &Option<Diagnostics>,
        runtime: &mut EmbeddedFrontendRuntime,
        hook: &str,
    ) -> Result<(), ComponentError> {
        let result = if runtime.has_script && runtime.script_ctx.has_handler(hook) {
            runtime.script_ctx.call_handler(hook, &[])
        } else {
            Ok(())
        };
        if hook == "unmount" {
            runtime.script_ctx.flush_storage();
        }
        Self::normalize_script_props(diagnostics, runtime);
        Self::drain_script_diagnostics(diagnostics, runtime);
        if let Err(source) = result {
            let component_id = runtime.module_id.clone();
            let error_message = source.to_string();
            tracing::warn!(
                component_id = %component_id,
                handler = hook,
                error = %error_message,
                "frontend lifecycle hook failed"
            );
            if let Some(diagnostics) = diagnostics {
                diagnostics.record_handler_error_with_source(
                    component_id.clone(),
                    hook.to_string(),
                    error_message,
                    Some(runtime.source_path.display().to_string()),
                    runtime.script_source_span,
                );
            }
            return Err(ComponentError::Script {
                component_id,
                source,
            });
        }
        Ok(())
    }

    /// Run authored `unmount` hooks for every live runtime and return the
    /// detached map. Reload uses the returned map as a rollback candidate until
    /// the replacement environment has initialized successfully.
    pub(super) fn prepare_runtimes_for_reload(
        &mut self,
    ) -> (
        HashMap<Arc<str>, EmbeddedFrontendRuntime>,
        Option<ComponentError>,
    ) {
        self.failed_runtime_creations.get_mut().clear();
        self.child_settings_cache.get_mut().clear();
        self.component_callbacks.get_mut().clear();
        let runtimes = std::mem::take(&mut *self.runtimes.lock().unwrap());
        let mut runtimes = runtimes.into_iter().collect::<Vec<_>>();
        // Embedded children are torn down before their parents, while the
        // secondary key ordering keeps the result deterministic for equal
        // depths.
        runtimes.sort_by(|(left, _), (right, _)| {
            right.len().cmp(&left.len()).then_with(|| left.cmp(right))
        });

        let mut first_error = None;
        for (_, runtime) in &mut runtimes {
            if let Err(error) = Self::dispatch_runtime_hook(&self.diagnostics, runtime, "unmount") {
                first_error.get_or_insert(error);
            }
            // Lifecycle side channels cannot be applied after the surface is
            // removed. Drain them before the shared VM/runtime is dropped.
            runtime.script_ctx.drain_published_events();
            runtime.script_ctx.drain_shell_effects();
            runtime.script_ctx.drain_element_actions();
        }
        (runtimes.into_iter().collect(), first_error)
    }

    pub(super) fn restore_runtimes_after_reload(
        &mut self,
        runtimes: HashMap<Arc<str>, EmbeddedFrontendRuntime>,
    ) {
        *self.runtimes.lock().unwrap() = runtimes;
        self.rebuild_runtime_generation_index();

        // The rollback candidates were cleanly detached before the
        // replacement was initialized. Re-enter them through the same
        // lifecycle boundary as a fresh runtime; otherwise a failed reload
        // would restore the Rust map while leaving authored resources and
        // subscriptions in their unmounted state.
        let mut instance_keys = self
            .runtimes
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        instance_keys
            .sort_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)));
        let mut runtimes = self.runtimes.lock().unwrap();
        for instance_key in instance_keys {
            let Some(runtime) = runtimes.get_mut(&instance_key) else {
                continue;
            };
            if let Err(error) = Self::dispatch_runtime_hook(&self.diagnostics, runtime, "mount") {
                tracing::warn!(
                    component_id = %runtime.module_id,
                    instance_key = %instance_key,
                    error = %error,
                    "frontend runtime rollback mount failed"
                );
            }
            runtime.script_ctx.drain_published_events();
            runtime.script_ctx.drain_shell_effects();
            runtime.script_ctx.drain_element_actions();
        }
        drop(runtimes);
        self.rebuild_runtime_generation_index();
    }

    /// Run authored `unmount` hooks for every live runtime, then remove the
    /// runtime map. Taking the map first makes this safe for replacement and
    /// teardown paths and guarantees that one failing hook cannot skip later
    /// runtimes.
    pub(super) fn unmount_runtimes(&mut self) -> Result<(), ComponentError> {
        let (_runtimes, first_error) = self.prepare_runtimes_for_reload();
        self.clear_runtime_generation_index();
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(())
        }
    }

    pub(super) fn deliver_service_call_result_to_instance(
        &mut self,
        instance_id: &str,
        call_id: u64,
        status: &str,
        result: &serde_json::Value,
    ) -> bool {
        let mut runtimes = self.runtimes.lock().unwrap();
        let Some(runtime) = runtimes.get_mut(instance_id) else {
            return false;
        };
        runtime
            .script_ctx
            .complete_service_call(call_id, status, result.clone())
    }

    fn drain_local_script_events(&mut self, events: Vec<PublishedEvent>) -> Vec<CoreRequest> {
        #[cfg(test)]
        {
            self.drained_script_events += events.len() as u64;
        }
        let mut shell_events = Vec::new();
        for event in events {
            if event.channel.starts_with("shell.")
                && let Err(rejection) = OperationRegistry::builtin().authorize_event(
                    &event.channel,
                    &event.payload,
                    &event.source_module_id,
                    &event.source_capabilities,
                )
            {
                tracing::warn!(
                    source_module_id = %event.source_module_id,
                    channel = %event.channel,
                    rejection = %rejection,
                    "shell operation rejected"
                );
                continue;
            }
            shell_events.push(event);
        }
        script_events_to_requests_at(shell_events, self.current_frontend_effect_revision())
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
        self.call_handler_target(handler, &merged_args)
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
        self.call_handler_target(handler, &merged_args)
    }

    /// Settle pending effects before choosing the template update path.
    /// Writes with no template consumers preserve the retained tree.
    pub(super) fn run_pending_effects(&mut self) {
        if !self.reactive_effects_pending {
            return;
        }
        self.flush_reactive_effects();
        self.reactive_effects_pending = false;
    }

    pub(super) fn flush_reactive_effects(&mut self) {
        let mut state_dirty = false;
        let mut written = Vec::new();
        let mut runtimes = self.runtimes.lock().unwrap();
        for (instance_key, runtime) in runtimes.iter_mut() {
            if Self::flush_runtime_effects(&self.diagnostics, runtime) {
                state_dirty = true;
                written.push(instance_key.to_string());
            }
        }
        let generations = runtimes
            .iter()
            .map(|(instance_key, runtime)| {
                (
                    instance_key.clone(),
                    runtime.script_ctx.state().mutation_generation(),
                )
            })
            .collect::<Vec<_>>();
        drop(runtimes);
        for (instance_key, generation) in generations {
            self.sync_runtime_generation(&instance_key, generation);
        }
        if (state_dirty
            || self.pending_service_template_nodes.is_some()
            || !self.pending_service_binding_readers.is_empty())
            && self.try_patch_template_writes(&written)
        {
            self.drop_script_narrow_after_patch();
            return;
        }
        if !state_dirty && self.script_narrow_is_unread_service_update() {
            self.drop_script_narrow_after_patch();
            return;
        }
        if state_dirty {
            self.effects_changed_templates = true;
            // Service projections preserve narrow invalidation so retained
            // reconciliation decides whether structural synchronization is needed.
            if self
                .dirty_types
                .contains(ComponentDirtyFlags::SCRIPT_NARROW)
            {
                self.invalidate_script_state_narrow();
            } else {
                self.invalidate_script_state();
            }
        }
    }

    #[cfg(test)]
    pub(super) fn has_reactive_effects(&self) -> bool {
        let mut runtimes = self.runtimes.lock().unwrap();
        runtimes
            .values_mut()
            .any(|runtime| runtime.script_ctx.has_reactive_effects())
    }

    pub(super) fn flush_runtime_effects(
        diagnostics: &Option<Diagnostics>,
        runtime: &mut EmbeddedFrontendRuntime,
    ) -> bool {
        if !runtime.script_ctx.has_reactive_effects() {
            return false;
        }
        let generation_before = runtime.script_ctx.state().mutation_generation();

        for error in runtime.script_ctx.run_reactive_effects() {
            Self::record_script_call_error(diagnostics, runtime, "effect", &error);
        }

        Self::normalize_script_props(diagnostics, runtime);
        Self::drain_script_diagnostics(diagnostics, runtime);
        if runtime.script_ctx.state().mutation_generation() == generation_before {
            return false;
        }
        // An effect that refreshed state no template expression reads has
        // no effect on the tree, exactly as for an event handler. Clear the
        // dirty flag so the frame is not rebuilt for an invisible write.
        if runtime.script_ctx.state().is_dirty()
            && !runtime.script_ctx.dirty_state_affects_template()
        {
            runtime.script_ctx.state_mut().clear_dirty();
            return false;
        }
        runtime.script_ctx.state().is_dirty()
    }

    pub(super) fn drain_script_diagnostics(
        diagnostics: &Option<Diagnostics>,
        runtime: &mut EmbeddedFrontendRuntime,
    ) {
        let localized_misses = runtime.script_ctx.drain_localized_misses();
        if diagnostics.is_some() {
            for resolution in localized_misses {
                record_localized_miss_diagnostic(diagnostics, &resolution, None);
            }
        }
        let Some(diagnostics) = diagnostics else {
            return;
        };
        for diagnostic in runtime.script_ctx.drain_diagnostics() {
            let (category, message) = match diagnostic.category {
                ScriptDiagnosticCategory::Lifecycle => (
                    mesh_core_diagnostics::DiagnosticCategory::Script,
                    diagnostic.reason.clone(),
                ),
                ScriptDiagnosticCategory::InterfaceUnavailable => (
                    mesh_core_diagnostics::DiagnosticCategory::Interface,
                    format!(
                        "interface '{}' unavailable for '{}': {}",
                        diagnostic.interface, diagnostic.module_id, diagnostic.reason
                    ),
                ),
                ScriptDiagnosticCategory::Storage => (
                    mesh_core_diagnostics::DiagnosticCategory::Storage,
                    format!(
                        "storage diagnostic for '{}': {}",
                        diagnostic.module_id, diagnostic.reason
                    ),
                ),
            };
            diagnostics.record_issue_with_source(
                format!(
                    "script:{}:{}:{}:{}",
                    runtime.module_id,
                    runtime.script_ctx.instance_id,
                    category.as_str(),
                    diagnostic.interface
                ),
                category,
                mesh_core_diagnostics::IssueSeverity::Error,
                message,
                Some(runtime.source_path.display().to_string()),
                runtime.script_source_span,
            );
        }
    }

    /// Keep an invalid script assignment from displacing the last valid host
    /// value. Host layers are resolved before the runtime is created and are
    /// retained in `host_props`, so a rejected script value falls back to the
    /// highest valid non-script layer rather than all the way to the declared
    /// default in the CSS projection.
    pub(super) fn normalize_script_props(
        diagnostics: &Option<Diagnostics>,
        runtime: &mut EmbeddedFrontendRuntime,
    ) {
        if runtime.prop_definitions.is_empty() {
            return;
        }
        let Some(current) = runtime.script_ctx.state().get_ref("props").cloned() else {
            return;
        };
        let Some(current) = current.as_object() else {
            let issue_code = format!(
                "prop-invalid:{}:{}:table",
                runtime.module_id, runtime.script_ctx.instance_id
            );
            let message =
                "component props state must be an object; restoring the last valid host values";
            tracing::warn!(
                module_id = %runtime.module_id,
                instance_id = %runtime.script_ctx.instance_id,
                "{message}"
            );
            if let Some(diagnostics) = diagnostics {
                diagnostics.record_issue(
                    issue_code,
                    mesh_core_diagnostics::IssueSeverity::Warning,
                    message,
                );
            }
            let _ = runtime
                .script_ctx
                .set_member_state("props", runtime.host_props.clone());
            return;
        };
        let host = runtime.host_props.as_object();
        let mut normalized = current.clone();
        let mut changed = false;

        for def in &runtime.prop_definitions {
            let issue_code = format!(
                "prop-invalid:{}:{}:{}",
                runtime.module_id, runtime.script_ctx.instance_id, def.name
            );
            match current.get(&def.name) {
                Some(value) => match validate_json_prop(def, value.clone()) {
                    Ok(value) => {
                        if current.get(&def.name) != Some(&value) {
                            normalized.insert(def.name.clone(), value);
                            changed = true;
                        }
                        if let Some(diagnostics) = diagnostics {
                            diagnostics.resolve_issue(&issue_code);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            module_id = %runtime.module_id,
                            instance_id = %runtime.script_ctx.instance_id,
                            prop = %def.name,
                            error = %error,
                            "invalid script prop override ignored; restoring the highest valid host value"
                        );
                        if let Some(diagnostics) = diagnostics {
                            diagnostics.record_issue(
                                issue_code,
                                mesh_core_diagnostics::IssueSeverity::Warning,
                                format!(
                                    "invalid script override for prop `{}`: {error}; using the highest valid host value",
                                    def.name
                                ),
                            );
                        }
                        match host.and_then(|host| host.get(&def.name)) {
                            Some(value) => {
                                normalized.insert(def.name.clone(), value.clone());
                            }
                            None => {
                                normalized.remove(&def.name);
                            }
                        }
                        changed = true;
                    }
                },
                None => {
                    if let Some(value) = host.and_then(|host| host.get(&def.name)) {
                        normalized.insert(def.name.clone(), value.clone());
                        changed = true;
                    }
                }
            }
        }

        if changed {
            if let Err(error) = runtime
                .script_ctx
                .set_member_state("props", serde_json::Value::Object(normalized))
            {
                tracing::warn!(
                    module_id = %runtime.module_id,
                    instance_id = %runtime.script_ctx.instance_id,
                    "failed to restore valid component props after an invalid script override: {error}"
                );
            }
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

    pub(super) fn localized_misses_handle(
        &self,
        instance_key: &str,
    ) -> Arc<Mutex<Vec<mesh_core_locale::LocalizedTextResolution>>> {
        self.runtimes
            .lock()
            .unwrap()
            .get(instance_key)
            .map(|runtime| runtime.script_ctx.localized_misses_handle())
            .unwrap_or_else(|| Arc::new(Mutex::new(Vec::new())))
    }

    pub(super) fn settings_json_for_component(
        &self,
        manifest: &mesh_core_module::Manifest,
        component: &mesh_core_component::ComponentFile,
        source_path: &Path,
    ) -> serde_json::Value {
        if manifest.package.id == self.compiled.manifest.package.id {
            return self.settings_json.clone();
        }
        if let Some(cached) = self
            .child_settings_cache
            .borrow()
            .get(&manifest.package.id)
            .and_then(|entries| entries.get(source_path))
        {
            return cached.clone();
        }
        let state = resolve_frontend_module_settings_with_props(
            &manifest.package.id,
            self.settings.namespace(&manifest.package.id),
            manifest,
            component.props.as_ref(),
        );
        mesh_core_config::log_settings_diagnostics(
            "embedded component settings",
            &state.diagnostics,
        );
        self.child_settings_cache
            .borrow_mut()
            .entry(manifest.package.id.clone())
            .or_default()
            .insert(source_path.to_path_buf(), state.effective.clone());
        state.effective
    }

    fn refresh_existing_runtime_props(
        diagnostics: &Option<Diagnostics>,
        runtime: &mut EmbeddedFrontendRuntime,
        component: &mesh_core_component::ComponentFile,
        props: &HashMap<String, serde_json::Value>,
        settings_json: &serde_json::Value,
        instance_key: &str,
    ) -> Result<(), ScriptError> {
        let ResolvedProps {
            effective: next_host_props,
            layers,
        } = resolve_props(component, props, settings_json, instance_key);
        let merged_props = merge_reloaded_props(
            runtime.script_ctx.state().get_ref("props"),
            &runtime.host_props,
            &next_host_props,
            &mut runtime.script_owned_props,
        );
        let mut publication = props.clone();
        publication.insert("props".to_string(), merged_props);
        runtime.script_ctx.set_member_states(&publication, true)?;
        runtime.script_ctx.set_prop_layers(layers)?;
        runtime.host_props = next_host_props;
        Self::normalize_script_props(diagnostics, runtime);
        Ok(())
    }

    pub(super) fn create_runtime_for_component(
        &self,
        instance_key: &str,
        component_id: String,
        manifest: &mesh_core_module::Manifest,
        component: &mesh_core_component::ComponentFile,
        source_path: &Path,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        let capabilities = match self.effective_capabilities.get(&manifest.package.id) {
            Some(effective) => effective.into_capability_set(),
            None => {
                #[cfg(test)]
                if self.test_capability_fallback {
                    grant_capabilities_from_manifest(manifest)
                } else {
                    return Err(ComponentError::Failed {
                        component_id,
                        message: format!(
                            "frontend component {} has no activation-resolved capability grant",
                            manifest.package.id
                        ),
                    });
                }
                #[cfg(not(test))]
                return Err(ComponentError::Failed {
                    component_id,
                    message: format!(
                        "frontend component {} has no activation-resolved capability grant",
                        manifest.package.id
                    ),
                });
            }
        };
        let mut script_ctx = ScriptContext::new_for_instance(
            manifest.package.id.clone(),
            component_id.clone(),
            instance_key.to_string(),
            capabilities,
        )
        .map_err(|source| ComponentError::Script {
            component_id: component_id.clone(),
            source,
        })?;
        // All components in this surface share one Lua realm so bind:this is a
        // live cross-component reference rather than a snapshot.
        script_ctx.attach_shared_vm(&self.surface_vm);
        script_ctx.set_interface_catalog(self.interface_catalog.clone());
        let translator = self.locale.module_translator(&manifest.package.id);
        script_ctx.set_i18n_translator(&translator);
        script_ctx.set_optional_interfaces(
            manifest
                .dependencies
                .interfaces
                .iter()
                .filter(|dep| !dep.required)
                .map(|dep| dep.name.clone())
                .collect(),
        );
        seed_service_context(&mut script_ctx).map_err(|source| ComponentError::Script {
            component_id: component_id.clone(),
            source,
        })?;
        script_ctx
            .seed_context_global("this", self.module_descriptor_from_manifest(manifest))
            .map_err(|source| ComponentError::Script {
                component_id: component_id.clone(),
                source,
            })?;
        if script_has_service_read(&script_ctx, "mesh.locale", "locale") {
            let payload = crate::shell::locale_service_payload(&self.locale, None, None);
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
        let settings_json = self.settings_json_for_component(manifest, component, source_path);
        let host_props = publish_resolved_props(
            &mut script_ctx,
            component,
            props,
            &settings_json,
            instance_key,
        )
        .map_err(|source| ComponentError::Script {
            component_id: component_id.clone(),
            source,
        })?;
        for (service_name, payload) in &self.cached_service_payloads {
            let interface = format!("mesh.{service_name}");
            if script_has_service_read(&script_ctx, &interface, service_name) {
                script_ctx.apply_service_payload_with_generation(
                    service_name,
                    payload.value.as_ref(),
                    payload.fingerprint,
                    Some(&payload.source_module),
                    payload.generation,
                );
                apply_service_update_with_name_and_fingerprint(
                    script_ctx.state_mut(),
                    true,
                    // A runtime being seeded has evaluated nothing yet, so
                    // nothing is known about what it reads.
                    true,
                    service_name,
                    "<cached>",
                    payload.value.as_ref(),
                    payload.fingerprint,
                );
            }
        }

        let callbacks = self
            .component_callbacks
            .borrow()
            .get(instance_key)
            .cloned()
            .unwrap_or_default();
        script_ctx.set_initial_callback_props(callbacks);
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
                .compile_and_execute_component_with_compiled(
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

        let mut runtime = EmbeddedFrontendRuntime {
            module_id: component_id,
            unpatchable_template_expressions:
                mesh_core_frontend::collect_unpatchable_template_expressions(component),
            source_path: source_path.to_path_buf(),
            script_source_span: component.script.as_ref().map(|script| {
                mesh_core_diagnostics::DiagnosticSourceSpan::new(script.span.start, script.span.end)
            }),
            script_ctx,
            has_script: component.script.is_some(),
            prop_definitions: component
                .props
                .as_ref()
                .map(|block| block.props.clone())
                .unwrap_or_default(),
            host_props,
            script_owned_props: HashSet::new(),
            cached_state_clone: None,
        };
        Self::dispatch_runtime_hook(&self.diagnostics, &mut runtime, "mount")?;
        Self::normalize_script_props(&self.diagnostics, &mut runtime);
        Ok(runtime)
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
            &compiled.source_path,
            props,
        )
    }

    pub(super) fn init_root_runtime(&self) -> Result<(), ComponentError> {
        let runtime =
            self.create_runtime(self.root_instance_key(), &self.compiled, &HashMap::new())?;
        let generation = runtime.script_ctx.state().mutation_generation();
        self.runtimes.lock().unwrap().insert(
            self.instance_keys
                .borrow_mut()
                .intern(self.root_instance_key()),
            runtime,
        );
        self.register_runtime_generation(self.root_instance_key(), generation);
        self.runtime_failure.borrow_mut().take();
        self.resolve_frontend_runtime_issue("initialization", self.root_instance_key());
        Ok(())
    }

    pub(super) fn ensure_runtime_for_compiled(
        &self,
        instance_key: &str,
        compiled: &mesh_core_frontend::CompiledFrontendModule,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        let settings_json = self.settings_json_for_component(
            &compiled.manifest,
            &compiled.component,
            &compiled.source_path,
        );
        let existing_generation = {
            let mut runtimes = self.runtimes.lock().unwrap();
            if let Some(runtime) = runtimes.get_mut(instance_key) {
                Self::refresh_existing_runtime_props(
                    &self.diagnostics,
                    runtime,
                    &compiled.component,
                    props,
                    &settings_json,
                    instance_key,
                )
                .map_err(|source| ComponentError::Script {
                    component_id: runtime.module_id.clone(),
                    source,
                })?;
                let callbacks = self
                    .component_callbacks
                    .borrow()
                    .get(instance_key)
                    .cloned()
                    .unwrap_or_default();
                runtime
                    .script_ctx
                    .set_callback_props(&callbacks)
                    .map_err(|source| ComponentError::Script {
                        component_id: runtime.module_id.clone(),
                        source,
                    })?;
                Some(runtime.script_ctx.state().mutation_generation())
            } else {
                None
            }
        };
        if let Some(generation) = existing_generation {
            self.sync_runtime_generation(instance_key, generation);
            self.resolve_frontend_runtime_issue("render", instance_key);
            return Ok(());
        }

        let mut runtime = self.create_runtime(instance_key, compiled, props)?;
        Self::flush_runtime_effects(&self.diagnostics, &mut runtime);
        apply_runtime_props(&mut runtime, props, false).map_err(|source| {
            ComponentError::Script {
                component_id: runtime.module_id.clone(),
                source,
            }
        })?;
        let generation = runtime.script_ctx.state().mutation_generation();
        self.runtimes.lock().unwrap().insert(
            self.instance_keys.borrow_mut().intern(instance_key),
            runtime,
        );
        self.register_runtime_generation(instance_key, generation);
        self.resolve_frontend_runtime_issue("render", instance_key);

        Ok(())
    }

    pub(super) fn build_error_widget(&self, message: impl Into<String>) -> WidgetNode {
        self.has_error_placeholders.set(true);
        self.error_placeholder_marks
            .set(self.error_placeholder_marks.get().wrapping_add(1));
        bounded_error_widget(message)
    }

    pub(super) fn has_template_expression_failure_for(&self, instance_key: &str) -> bool {
        self.template_expression_failure
            .borrow()
            .as_ref()
            .is_some_and(|(failed_instance_key, _)| failed_instance_key == instance_key)
    }

    pub(super) fn take_template_expression_failure_for(
        &self,
        instance_key: &str,
    ) -> Option<String> {
        let mut failure = self.template_expression_failure.borrow_mut();
        if failure
            .as_ref()
            .is_some_and(|(failed_instance_key, _)| failed_instance_key == instance_key)
        {
            failure.take().map(|(_, message)| message)
        } else {
            None
        }
    }

    pub(super) fn ensure_local_component_runtime(
        &self,
        instance_key: &str,
        host_module_id: &str,
        host_manifest: &mesh_core_module::Manifest,
        alias: &str,
        component: &mesh_core_component::ComponentFile,
        source_path: &Path,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        let settings_json = self.settings_json_for_component(host_manifest, component, source_path);
        let existing_generation = {
            let mut runtimes = self.runtimes.lock().unwrap();
            if let Some(runtime) = runtimes.get_mut(instance_key) {
                Self::refresh_existing_runtime_props(
                    &self.diagnostics,
                    runtime,
                    component,
                    props,
                    &settings_json,
                    instance_key,
                )
                .map_err(|source| ComponentError::Script {
                    component_id: runtime.module_id.clone(),
                    source,
                })?;
                let callbacks = self
                    .component_callbacks
                    .borrow()
                    .get(instance_key)
                    .cloned()
                    .unwrap_or_default();
                runtime
                    .script_ctx
                    .set_callback_props(&callbacks)
                    .map_err(|source| ComponentError::Script {
                        component_id: runtime.module_id.clone(),
                        source,
                    })?;
                Some(runtime.script_ctx.state().mutation_generation())
            } else {
                None
            }
        };
        if let Some(generation) = existing_generation {
            self.sync_runtime_generation(instance_key, generation);
            self.resolve_frontend_runtime_issue("render", instance_key);
            return Ok(());
        }

        let mut runtime = self.create_runtime_for_component(
            instance_key,
            local_component_runtime_id(host_module_id, alias),
            host_manifest,
            component,
            source_path,
            props,
        )?;
        Self::flush_runtime_effects(&self.diagnostics, &mut runtime);
        apply_runtime_props(&mut runtime, props, false).map_err(|source| {
            ComponentError::Script {
                component_id: runtime.module_id.clone(),
                source,
            }
        })?;
        let generation = runtime.script_ctx.state().mutation_generation();
        self.runtimes.lock().unwrap().insert(
            self.instance_keys.borrow_mut().intern(instance_key),
            runtime,
        );
        self.register_runtime_generation(instance_key, generation);
        self.resolve_frontend_runtime_issue("render", instance_key);

        Ok(())
    }

    /// A remembered deterministic creation failure for `instance_key`.
    fn known_runtime_creation_failure(&self, instance_key: &str) -> Option<String> {
        let failures = self.failed_runtime_creations.borrow();
        if failures.is_empty() {
            return None;
        }
        failures.get(instance_key).cloned()
    }

    /// Remember a creation failure that cannot recover while this component's
    /// interface catalog and capability grants stay the same. The diagnostic
    /// issue recorded for it stays open; later frames reuse its message.
    fn remember_runtime_creation_failure(
        &self,
        instance_key: &str,
        error: &ComponentError,
        message: &str,
    ) {
        let deterministic = matches!(
            error,
            ComponentError::Script {
                source: ScriptError::InterfaceUnavailable(_) | ScriptError::CapabilityDenied(_),
                ..
            }
        );
        if deterministic && !self.runtimes.lock().unwrap().contains_key(instance_key) {
            self.failed_runtime_creations
                .borrow_mut()
                .insert(Arc::from(instance_key), message.to_owned());
        }
    }

    pub(super) fn render_local_component(
        &self,
        host: &mesh_core_module::Manifest,
        alias: &str,
        component: &mesh_core_component::ComponentFile,
        source_path: &std::path::Path,
        instance_key: &str,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
        host_rules: &[mesh_core_component::style::StyleRule],
    ) -> WidgetNode {
        if let Some(message) = self.known_runtime_creation_failure(instance_key) {
            return self.build_error_widget(message);
        }
        if let Err(err) = self.ensure_local_component_runtime(
            instance_key,
            &host.package.id,
            host,
            alias,
            component,
            source_path,
            props,
        ) {
            let message = self.record_frontend_runtime_issue("render", instance_key, &err);
            self.remember_runtime_creation_failure(instance_key, &err, &message);
            return self.build_error_widget(message);
        }

        let theme = self.active_theme.borrow().clone();
        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::with_localized_misses(
            &state,
            self.locale.module_translator(&host.package.id),
            self.localized_misses_handle(instance_key),
        );
        let prepared_styles = {
            let cached = self.prepared_component_styles.borrow();
            cached
                .get(&host.package.id)
                .and_then(|components| {
                    components.get(&format!("{}::{alias}", source_path.display()))
                })
                .cloned()
        }
        .unwrap_or_else(|| {
            let prepared = Arc::new(mesh_core_frontend::PreparedComponentStyleRules::new(
                component, host_rules,
            ));
            self.prepared_component_styles
                .borrow_mut()
                .entry(host.package.id.clone())
                .or_default()
                .insert(
                    format!("{}::{alias}", source_path.display()),
                    Arc::clone(&prepared),
                );
            prepared
        });
        let node =
            mesh_core_frontend::build_embedded_widget_tree_from_component_with_prepared_styles_and_owner(
                component,
                host,
                &theme,
                container_width,
                container_height,
                Some(self),
                instance_key,
                Some(&bound),
                &prepared_styles,
                source_path,
            );
        self.take_template_expression_failure_for(instance_key)
            .map(|message| self.build_error_widget(message))
            .unwrap_or(node)
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

        let Some(entry) = self.frontend_catalog.modules.get(module_id) else {
            return self.build_error_widget(format!("missing embedded module '{module_id}'"));
        };
        self.render_embedded_compiled_instance(
            instance_key,
            module_id,
            &entry.compiled,
            props,
            container_width,
            container_height,
        )
    }

    pub(super) fn render_embedded_compiled_instance(
        &self,
        instance_key: &str,
        module_id: &str,
        compiled: &mesh_core_frontend::CompiledFrontendModule,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
    ) -> WidgetNode {
        if render_stack_contains_cycle(&self.render_stack.borrow(), module_id) {
            return self.build_error_widget(format!("composition cycle blocked for '{module_id}'"));
        }

        if let Some(message) = self.known_runtime_creation_failure(instance_key) {
            return self.build_error_widget(message);
        }
        if let Err(err) = self.ensure_runtime_for_compiled(instance_key, compiled, props) {
            let message = self.record_frontend_runtime_issue("render", instance_key, &err);
            self.remember_runtime_creation_failure(instance_key, &err, &message);
            return self.build_error_widget(message);
        }

        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::with_localized_misses(
            &state,
            self.locale.module_translator(module_id),
            self.localized_misses_handle(instance_key),
        );
        let active_theme = self.active_theme.borrow().clone();
        self.render_stack.borrow_mut().push(module_id.to_string());
        let measurer = SharedTextMeasurer;
        let mut tree = compiled.build_tree_with_state(
            &active_theme,
            container_width.max(0.0).ceil() as u32,
            container_height.max(0.0).ceil() as u32,
            Some(&bound),
            FrontendRenderMode::Embedded,
            instance_key,
            Some(self),
            Some(&measurer),
        );
        let expression_failure = self.take_template_expression_failure_for(instance_key);
        super::composition::annotate_source_file(
            &mut tree,
            &compiled.source_path.display().to_string(),
        );
        self.render_stack.borrow_mut().pop();
        expression_failure
            .map(|message| self.build_error_widget(message))
            .unwrap_or(tree)
    }

    #[cfg(test)]
    pub(super) fn call_namespaced_handler(
        &mut self,
        handler: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let target = HandlerTarget::from_legacy_serialized(handler);
        self.call_handler_target(&target, args)
    }

    pub(super) fn call_handler_target(
        &mut self,
        target: &HandlerTarget,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let _span = tracing::debug_span!(
            "call_handler",
            surface = %self.id(),
            handler = target.handler(),
            instance_key = target.instance_key(),
        )
        .entered();
        if let HandlerTarget::Function { callback, .. } = target {
            return self.run_script_call(target.instance_key(), target.handler(), |context| {
                context.call_callback(callback, args).map(|()| Vec::new())
            });
        }
        let (handler_name, merged_args) = unpack_handler_args(target.handler(), args);
        self.run_script_call(target.instance_key(), handler_name.as_ref(), |script_ctx| {
            script_ctx
                .call_handler(handler_name.as_ref(), merged_args.as_ref())
                .map(|()| Vec::new())
        })
    }

    /// The earliest pending `task` deadline across this surface's runtimes.
    pub(super) fn next_script_task_deadline(&self) -> Option<Instant> {
        self.runtimes
            .lock()
            .unwrap()
            .values()
            .filter_map(|runtime| runtime.script_ctx.next_task_deadline())
            .min()
    }

    /// Run every runtime's due `task.delay` / `task.spawn` work and process it
    /// as a handler call: state sync, invalidation, published events and
    /// element actions.
    pub(super) fn run_due_script_tasks(
        &mut self,
        now: Instant,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let due: Vec<Arc<str>> = self
            .runtimes
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, runtime)| {
                runtime
                    .script_ctx
                    .next_task_deadline()
                    .is_some_and(|deadline| deadline <= now)
            })
            .map(|(key, _)| Arc::clone(key))
            .collect();
        let mut requests = Vec::new();
        for instance_key in due {
            requests.extend(
                self.run_script_call(Some(&instance_key), "task", |script_ctx| {
                    Ok(script_ctx.run_due_tasks(now))
                })?,
            );
        }
        Ok(requests)
    }

    /// Run `call` against one runtime's script context, then apply what it
    /// did. `Err` means the call failed as a whole: it is reported and its
    /// effects are dropped. `Ok` carries errors from independent callbacks
    /// that failed while the rest of the call completed; each is reported and
    /// the call's effects are applied.
    pub(super) fn run_script_call(
        &mut self,
        instance_key: Option<&str>,
        label: &str,
        call: impl FnOnce(&mut ScriptContext) -> Result<Vec<ScriptError>, ScriptError>,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut runtimes = self.runtimes.lock().unwrap();
        let explicit_instance = instance_key;
        let instance_key = instance_key.unwrap_or_else(|| self.root_instance_key());
        let Some(runtime) = runtimes.get_mut(instance_key) else {
            return Ok(Vec::new());
        };
        // Bindings may have been installed by the last paint. Settle their
        // effects before the handler can write optimistic interaction state.
        for error in runtime.script_ctx.run_reactive_effects() {
            Self::record_script_call_error(&self.diagnostics, runtime, "effect", &error);
        }
        match call(&mut runtime.script_ctx) {
            Err(source) => {
                Self::record_script_call_error(&self.diagnostics, runtime, label, &source);
                Self::drain_script_diagnostics(&self.diagnostics, runtime);
                let generation = runtime.script_ctx.state().mutation_generation();
                let runtime_key = instance_key.to_string();
                drop(runtimes);
                self.sync_runtime_generation(&runtime_key, generation);
                return Ok(Vec::new());
            }
            Ok(partial_errors) => {
                for source in &partial_errors {
                    Self::record_script_call_error(&self.diagnostics, runtime, label, source);
                }
            }
        }
        Self::normalize_script_props(&self.diagnostics, runtime);
        Self::drain_script_diagnostics(&self.diagnostics, runtime);
        for error in runtime.script_ctx.run_reactive_effects() {
            Self::record_script_call_error(&self.diagnostics, runtime, "effect", &error);
        }
        let state_dirty = runtime.script_ctx.state().is_dirty();
        let state_affects_template = runtime.script_ctx.dirty_state_affects_template();
        if state_dirty && !state_affects_template {
            runtime.script_ctx.state_mut().clear_dirty();
        }
        let published = runtime.script_ctx.drain_published_events();
        let shell_effects = take_shell_effects(runtime);
        let generation = runtime.script_ctx.state().mutation_generation();
        let runtime_key = instance_key.to_string();
        drop(runtimes);
        self.sync_runtime_generation(&runtime_key, generation);

        let (mut events, neighbors_dirty) = match explicit_instance {
            Some(instance_key) => {
                let mut events = self.drain_local_script_events(published);
                events.extend(self.lower_shell_effects(shell_effects));
                let (neighbors_dirty, mut neighbor_events) =
                    self.resync_binding_neighbors(instance_key);
                events.append(&mut neighbor_events);
                (events, neighbors_dirty)
            }
            None => {
                // The root ID is borrowed from `self` for the runtime lookup above.
                // Most ordinary handlers neither publish events nor participate
                // in live bindings, so avoid cloning that ID before every
                // dispatch.
                let needs_neighbor_sync = self.has_binding_neighbors(self.root_instance_key());
                if published.is_empty() && shell_effects.is_none() && !needs_neighbor_sync {
                    (Vec::new(), false)
                } else {
                    let instance_key = self.root_instance_key().to_string();
                    let mut events = self.drain_local_script_events(published);
                    events.extend(self.lower_shell_effects(shell_effects));
                    let (neighbors_dirty, mut neighbor_events) = if needs_neighbor_sync {
                        self.resync_binding_neighbors(&instance_key)
                    } else {
                        (false, Vec::new())
                    };
                    events.append(&mut neighbor_events);
                    (events, neighbors_dirty)
                }
            }
        };
        let patched = state_affects_template
            && !neighbors_dirty
            && self.try_patch_template_writes(std::slice::from_ref(&runtime_key));
        if (state_affects_template || neighbors_dirty) && !patched {
            // The retained-tree/display-list diff is authoritative for both
            // structural fallback and pixel damage. Handler writes therefore
            // start on the narrow path: ordinary bound-value changes repaint
            // only their actual damage, while conditional/list changes still
            // promote naturally when the retained diff is structural.
            self.invalidate_script_state_narrow();
        }

        // Execute imperative element actions the handler queued via live
        // `refs.<name>` references (focus/blur), routed through the real focus path.
        let mut element_requests = self.apply_element_actions()?;
        events.append(&mut element_requests);
        if events
            .iter()
            .any(|request| matches!(request, CoreRequest::ShowSurface { .. }))
        {
            // A newly shown top-level surface receives compositor focus. Keep
            // the source surface's retained focus model in sync even when the
            // request originated in an isolated slot contribution runtime.
            self.clear_focus_for_transfer();
        }

        Ok(events)
    }

    /// Lower what effects queued outside a handler call: events they
    /// published and `mesh.shell` operations they requested.
    pub(super) fn drain_effect_side_channels(&mut self) -> Vec<CoreRequest> {
        let drained = {
            let mut runtimes = self.runtimes.lock().unwrap();
            runtimes
                .values_mut()
                .filter_map(|runtime| {
                    let published = runtime.script_ctx.drain_published_events();
                    let effects = take_shell_effects(runtime);
                    (!published.is_empty() || effects.is_some()).then_some((published, effects))
                })
                .collect::<Vec<_>>()
        };
        let mut requests = Vec::new();
        for (published, effects) in drained {
            requests.extend(self.drain_local_script_events(published));
            requests.extend(self.lower_shell_effects(effects));
        }
        requests
    }

    fn lower_shell_effects(
        &self,
        effects: Option<(EffectScope, Vec<FrontendEffect>)>,
    ) -> Vec<CoreRequest> {
        let Some((scope, effects)) = effects else {
            return Vec::new();
        };
        typed_effects_to_requests_at(scope, effects, self.current_frontend_effect_revision())
    }

    fn record_script_call_error(
        diagnostics: &Option<Diagnostics>,
        runtime: &EmbeddedFrontendRuntime,
        label: &str,
        source: &ScriptError,
    ) {
        let error_message = source.to_string();
        tracing::warn!(
            component_id = %runtime.module_id,
            handler = %label,
            error = %error_message,
            "frontend event handler failed"
        );
        if let Some(diagnostics) = diagnostics {
            diagnostics.record_handler_error_with_source(
                runtime.module_id.clone(),
                label.to_string(),
                error_message,
                Some(runtime.source_path.display().to_string()),
                runtime.script_source_span,
            );
        }
    }

    fn script_binding_neighbors(&self, instance_key: &str) -> Vec<Arc<str>> {
        let mut edges = Vec::new();
        for (parent, children) in self.bound_children.borrow().iter() {
            edges.extend(
                children
                    .iter()
                    .map(|(_, child)| (parent.clone(), child.clone())),
            );
        }
        for (child, callbacks) in self.component_callbacks.borrow().iter() {
            for target in callbacks.values() {
                let owner = target
                    .instance_key()
                    .unwrap_or_else(|| self.root_instance_key());
                if owner != child.as_ref() {
                    edges.push((Arc::from(owner), child.clone()));
                }
            }
        }
        let mut visited = HashSet::from([Arc::<str>::from(instance_key)]);
        let mut pending = vec![Arc::<str>::from(instance_key)];
        while let Some(current) = pending.pop() {
            for (parent, child) in &edges {
                let neighbor = if parent == &current {
                    child
                } else if child == &current {
                    parent
                } else {
                    continue;
                };
                if visited.insert(neighbor.clone()) {
                    pending.push(neighbor.clone());
                }
            }
        }
        visited.remove(instance_key);
        visited.into_iter().collect()
    }

    fn has_binding_neighbors(&self, instance_key: &str) -> bool {
        !self.script_binding_neighbors(instance_key).is_empty()
    }

    /// Re-sync every instance linked to `instance_key` by a live `bind:this`
    /// reference or callback, in either direction: children this instance binds (it may have
    /// called their functions) and parents that bind this instance (it may have
    /// fired `self.<Event>` to them). A live cross-call mutates the neighbor's
    /// `_ENV` in the shared VM without going through the shell's normal
    /// post-handler sync, so its Rust-side reactive state would otherwise stay
    /// stale until some other event re-rendered it.
    ///
    /// Both directions are gated on the neighbour's own
    /// `live_binding_external_accessed` flag, set by the live-binding proxy:
    /// calling a child function through the proxy flags the child, and a child
    /// firing a `self.<Event>` channel into a parent's subscription flags the
    /// parent. Neighbours nothing touched are skipped.
    fn resync_binding_neighbors(&mut self, instance_key: &str) -> (bool, Vec<CoreRequest>) {
        let neighbor_keys = self.script_binding_neighbors(instance_key);
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
            neighbor.script_ctx.invalidate_reactive_effects();
            neighbor.script_ctx.resync_state();
            for error in neighbor.script_ctx.run_reactive_effects() {
                Self::record_script_call_error(&self.diagnostics, neighbor, "effect", &error);
            }
            Self::normalize_script_props(&self.diagnostics, neighbor);
            Self::drain_script_diagnostics(&self.diagnostics, neighbor);
            if neighbor.script_ctx.state().is_dirty() {
                state_dirty = true;
            }
            let published = neighbor.script_ctx.drain_published_events();
            let shell_effects = take_shell_effects(neighbor);
            let generation = neighbor.script_ctx.state().mutation_generation();
            drop(runtimes);
            self.sync_runtime_generation(&neighbor_key, generation);
            events.extend(self.drain_local_script_events(published));
            events.extend(self.lower_shell_effects(shell_effects));
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
) -> Option<(&'a HandlerTarget, std::borrow::Cow<'a, [serde_json::Value]>)> {
    if let Some(call) = node.event_handler_calls.get(event_name) {
        if call.args.is_empty() {
            return Some((&call.handler, std::borrow::Cow::Borrowed(event_args)));
        }
        if event_args.is_empty() {
            return Some((
                &call.handler,
                std::borrow::Cow::Borrowed(call.args.as_slice()),
            ));
        }
        let mut merged = Vec::with_capacity(call.args.len() + event_args.len());
        merged.extend(call.args.iter().cloned());
        merged.extend_from_slice(event_args);
        return Some((&call.handler, std::borrow::Cow::Owned(merged)));
    }
    node.event_handlers
        .get(event_name)
        .map(|handler| (handler, std::borrow::Cow::Borrowed(event_args)))
}

#[cfg(test)]
mod handler_call_tests {
    use super::*;
    use mesh_core_elements::EventHandlerCall;

    #[test]
    fn node_handler_and_args_prefers_typed_call_args() {
        let mut node = WidgetNode::new("button");
        node.event_handlers
            .insert("click".into(), HandlerTarget::embedded("host", "legacy"));
        node.event_handler_calls.insert(
            "click".into(),
            EventHandlerCall {
                handler: HandlerTarget::embedded("host", "typed"),
                args: vec![serde_json::json!("prebound")],
            },
        );

        let event_args = [serde_json::json!({ "type": "click" })];
        let (handler, args) = node_handler_and_args(&node, "click", &event_args).expect("handler");

        assert_eq!(handler.handler(), "typed");
        assert_eq!(handler.instance_key(), Some("host"));
        assert_eq!(args[0], serde_json::json!("prebound"));
        assert_eq!(args[1], serde_json::json!({ "type": "click" }));
        assert!(matches!(args, std::borrow::Cow::Owned(_)));
    }

    #[test]
    fn node_handler_and_args_borrows_when_one_typed_arg_side_is_empty() {
        let mut node = WidgetNode::new("button");
        node.event_handler_calls.insert(
            "click".into(),
            EventHandlerCall {
                handler: HandlerTarget::embedded("host", "typed"),
                args: Vec::new(),
            },
        );

        let event_args = [serde_json::json!({ "type": "click" })];
        let (_, args) = node_handler_and_args(&node, "click", &event_args).expect("handler");
        assert_eq!(args.as_ref(), event_args.as_slice());
        assert!(matches!(args, std::borrow::Cow::Borrowed(_)));

        node.event_handler_calls.get_mut("click").unwrap().args =
            vec![serde_json::json!("prebound")];
        let (_, args) = node_handler_and_args(&node, "click", &[]).expect("handler");
        assert_eq!(args.as_ref(), [serde_json::json!("prebound")].as_slice());
        assert!(matches!(args, std::borrow::Cow::Borrowed(_)));
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

    #[test]
    fn handler_target_preserves_root_and_embedded_identity() {
        let root = HandlerTarget::root("onClick");
        assert_eq!(root.handler(), "onClick");
        assert_eq!(root.instance_key(), None);
        let embedded = HandlerTarget::embedded("@mesh/panel/local:Clock", "open");
        assert_eq!(embedded.handler(), "open");
        assert_eq!(embedded.instance_key(), Some("@mesh/panel/local:Clock"));
    }
}

/// Publish each declared prop's precedence-resolved value under `props.<name>`
/// in script state. The compiler reads these to project `prop(name)` into CSS,
/// and scripts read them as `props.name`. Every layer is validated before it
/// can win; an invalid higher-precedence value is ignored so the next valid
/// layer remains effective.
pub(super) fn publish_resolved_props(
    script_ctx: &mut ScriptContext,
    component: &mesh_core_component::ComponentFile,
    instance_props: &HashMap<String, serde_json::Value>,
    settings_json: &serde_json::Value,
    instance_key: &str,
) -> Result<serde_json::Value, ScriptError> {
    let resolved = resolve_props(component, instance_props, settings_json, instance_key);
    script_ctx.set_member_state("props", resolved.effective.clone())?;
    script_ctx.set_prop_layers(resolved.layers)?;
    Ok(resolved.effective)
}

/// Host-resolved props: the effective value of each declared prop, and the
/// per-layer snapshot behind `props.source(name)` / `props.at(name, scope)`.
pub(super) struct ResolvedProps {
    pub(super) effective: serde_json::Value,
    pub(super) layers: serde_json::Value,
}

#[cfg(test)]
pub(super) fn resolved_props_json(
    component: &mesh_core_component::ComponentFile,
    instance_props: &HashMap<String, serde_json::Value>,
    settings_json: &serde_json::Value,
    instance_key: &str,
) -> serde_json::Value {
    resolve_props(component, instance_props, settings_json, instance_key).effective
}

pub(super) fn resolve_props(
    component: &mesh_core_component::ComponentFile,
    instance_props: &HashMap<String, serde_json::Value>,
    settings_json: &serde_json::Value,
    instance_key: &str,
) -> ResolvedProps {
    let Some(block) = &component.props else {
        return ResolvedProps {
            effective: serde_json::json!({}),
            layers: serde_json::json!({}),
        };
    };
    let mut props = serde_json::Map::new();
    let mut layers = serde_json::Map::new();
    let global_settings = settings_json
        .pointer("/props/global")
        .and_then(serde_json::Value::as_object);
    let instance_settings = settings_json
        .pointer("/props/instances")
        .and_then(serde_json::Value::as_object)
        .and_then(|instances| instances.get(instance_key))
        .and_then(serde_json::Value::as_object);
    for def in &block.props {
        let mut candidates = Vec::with_capacity(4);
        if let Some(default) = def.default.as_ref().map(prop_default_to_json) {
            candidates.push((PropLayer::Default, default));
        }
        if let Some(value) = global_settings.and_then(|settings| settings.get(&def.name)) {
            candidates.push((PropLayer::Global, value.clone()));
        }
        if let Some(value) = instance_props.get(&def.name) {
            candidates.push((PropLayer::Instance, value.clone()));
        }
        if let Some(value) = instance_settings.and_then(|settings| settings.get(&def.name)) {
            candidates.push((PropLayer::PerInstance, value.clone()));
        }

        let raw_layers = candidates
            .iter()
            .map(|(layer, value)| (layer.as_str().to_string(), value.clone()))
            .collect::<serde_json::Map<_, _>>();
        let resolved = candidates.into_iter().rev().find_map(|(layer, value)| {
            match validate_json_prop(def, value) {
                Ok(value) => Some((layer, value)),
                Err(error) => {
                    tracing::warn!(
                        prop = %def.name,
                        layer = layer.as_str(),
                        error = %error,
                        "invalid prop override ignored; trying the next lower-precedence value"
                    );
                    None
                }
            }
        });
        let (winner, effective) = match resolved {
            Some((layer, value)) => (serde_json::Value::from(layer.as_str()), value),
            None => (serde_json::Value::Null, serde_json::Value::Null),
        };
        if !effective.is_null() {
            props.insert(def.name.clone(), effective.clone());
        }
        layers.insert(
            def.name.clone(),
            serde_json::json!({
                "layers": raw_layers,
                "winner": winner,
                "effective": effective,
            }),
        );
    }
    ResolvedProps {
        effective: serde_json::Value::Object(props),
        layers: serde_json::Value::Object(layers),
    }
}

#[derive(Debug, Clone, Copy)]
enum PropLayer {
    Default,
    Global,
    Instance,
    PerInstance,
}

impl PropLayer {
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Global => "global",
            Self::Instance => "instance",
            Self::PerInstance => "per_instance",
        }
    }
}

/// Merge a host prop republication into the script's current props.
///
/// A prop whose current value differs from the host value last published is
/// a script override (03 §4, layer 5). Ownership is recorded in
/// `script_owned` and kept even if a later host value happens to coincide, so
/// a newer host value never replaces it. A script releases a prop by
/// assigning `nil`; the host value then applies again.
pub(super) fn merge_reloaded_props(
    current: Option<&serde_json::Value>,
    previous_host: &serde_json::Value,
    next_host: &serde_json::Value,
    script_owned: &mut HashSet<String>,
) -> serde_json::Value {
    let mut merged = current
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    let previous = previous_host.as_object().cloned().unwrap_or_default();
    let next = next_host.as_object().cloned().unwrap_or_default();

    for name in previous.keys() {
        match merged.get(name) {
            None => {
                script_owned.remove(name);
            }
            Some(value) if Some(value) != previous.get(name) => {
                script_owned.insert(name.clone());
            }
            Some(_) => {}
        }
    }
    for (name, value) in &next {
        if !previous.contains_key(name) && merged.get(name).is_some_and(|current| current != value)
        {
            script_owned.insert(name.clone());
        }
    }
    for name in previous.keys().chain(next.keys()) {
        if script_owned.contains(name) {
            continue;
        }
        match next.get(name) {
            Some(value) => {
                merged.insert(name.clone(), value.clone());
            }
            None => {
                merged.remove(name);
            }
        }
    }
    serde_json::Value::Object(merged)
}

fn prop_default_to_json(value: &mesh_core_component::PropValue) -> serde_json::Value {
    mesh_core_component::prop_value_to_json(value)
}

fn validate_json_prop(
    def: &mesh_core_component::PropDef,
    value: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let prop_value = mesh_core_component::json_to_prop_value_ref(&value).map_err(|error| {
        format!(
            "prop `{}` expects a {}, {error}",
            def.name,
            def.ty.lua_type()
        )
    })?;
    let normalized = mesh_core_component::normalize_prop_value(def, prop_value)
        .map_err(|error| error.message)?;
    Ok(mesh_core_component::prop_value_to_json(&normalized))
}

pub(super) fn script_has_service_read(
    script_ctx: &ScriptContext,
    interface: &str,
    _service_name: &str,
) -> bool {
    script_ctx.can_read_service_interface(interface)
}

#[cfg(test)]
mod prop_resolution_tests {
    use super::*;

    fn component_with_props() -> mesh_core_component::ComponentFile {
        mesh_core_component::parse_component(
            r#"
<props>
{
    width = { type = "size", default = "20px" },
}
</props>
<template><box /></template>
"#,
        )
        .expect("component with props")
    }

    #[test]
    fn invalid_higher_layers_fall_back_to_the_highest_valid_value() {
        let component = component_with_props();

        let settings = serde_json::json!({
            "props": {
                "global": { "width": "28px" },
                "instances": { "instance": { "width": "not-a-size" } }
            }
        });
        let instance_props =
            HashMap::from([("width".to_string(), serde_json::json!("also-not-a-size"))]);

        assert_eq!(
            resolved_props_json(&component, &instance_props, &settings, "instance"),
            serde_json::json!({ "width": "28px" })
        );
    }

    #[test]
    fn scripts_read_the_winning_layer_and_user_intent_beneath_an_override() {
        let component = component_with_props();
        let settings = serde_json::json!({
            "props": {
                "global": { "width": "28px" },
                "instances": { "instance": { "width": "36px" } }
            }
        });
        let mut script_ctx = ScriptContext::new(
            "@test/props",
            mesh_core_capability::CapabilitySet::default(),
        )
        .expect("script context");
        script_ctx
            .load_script(
                r#"
function report()
  observed = {
    before = props.source("width"),
    user = props.at("width", "per_instance"),
    unset = props.at("width", "instance"),
    undeclared = props.source("height"),
  }
  props.width = "44px"
  observed.after = props.source("width")
  observed.script = props.at("width", "script")
  observed.global = props.at("width", "global")
  observed.default = props.at("width", "default")
  observed.user_after = props.at("width", "per_instance")
  observed.bad_scope = pcall(props.at, "width", "session")
end
"#,
            )
            .expect("load script");
        publish_resolved_props(
            &mut script_ctx,
            &component,
            &HashMap::new(),
            &settings,
            "instance",
        )
        .expect("publish props");
        script_ctx.call_handler("report", &[]).expect("report");

        let observed = script_ctx
            .state()
            .get_ref("observed")
            .cloned()
            .expect("observed state");
        assert_eq!(
            observed,
            serde_json::json!({
                "before": "per_instance",
                "user": "36px",
                "after": "script",
                "script": "44px",
                "global": "28px",
                "default": "20px",
                "user_after": "36px",
                "bad_scope": false,
            }),
            "unset layers and undeclared props answer nil"
        );
    }

    #[test]
    fn a_valid_instance_layer_beats_an_invalid_global_override() {
        let component = component_with_props();
        let settings = serde_json::json!({
            "props": { "global": { "width": "not-a-size" } }
        });
        let instance_props = HashMap::from([("width".to_string(), serde_json::json!("32px"))]);

        assert_eq!(
            resolved_props_json(&component, &instance_props, &settings, "instance"),
            serde_json::json!({ "width": "32px" })
        );
    }

    #[test]
    fn invalid_overrides_leave_the_declared_default_as_the_effective_value() {
        let component = component_with_props();
        let settings = serde_json::json!({
            "props": {
                "global": { "width": "not-a-size" },
                "instances": { "instance": { "width": null } }
            }
        });
        let instance_props = HashMap::from([("width".to_string(), serde_json::json!(false))]);

        assert_eq!(
            resolved_props_json(&component, &instance_props, &settings, "instance"),
            serde_json::json!({ "width": "20px" })
        );
    }

    #[test]
    fn duration_string_overrides_are_normalized_before_precedence_resolution() {
        let component = mesh_core_component::parse_component(
            r#"
<props>
{
    delay = { type = "duration", default = "120ms", min = 0, max = 600 },
}
</props>
<template><box /></template>
"#,
        )
        .expect("component with duration prop");

        assert_eq!(
            resolved_props_json(
                &component,
                &HashMap::new(),
                &serde_json::json!({ "props": { "global": { "delay": "240ms" } } }),
                "instance",
            ),
            serde_json::json!({ "delay": 240.0 })
        );
        assert_eq!(
            resolved_props_json(
                &component,
                &HashMap::new(),
                &serde_json::json!({ "props": { "global": { "delay": "900ms" } } }),
                "instance",
            ),
            serde_json::json!({ "delay": 120.0 }),
            "an invalid later value leaves the highest valid lower layer"
        );
    }
}

impl FrontendSurfaceComponent {
    pub(in crate::shell::component) fn resolve_manifest_text(
        &self,
        module_id: &str,
        field_path: &str,
        text: &mesh_core_module::LocalizedText,
    ) -> mesh_core_locale::LocalizedTextResolution {
        let resolution =
            crate::shell::resolve_manifest_text(&self.locale, module_id, field_path, text);
        self.record_localized_miss(&resolution, None);
        resolution
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
                        &format!("mesh.contributes.keybinds.{keybind_id}.label"),
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
                        &format!("mesh.contributes.keybinds.{keybind_id}.description"),
                        &description,
                    ),
                );
                if let Some(category) = &action.category {
                    insert_resolved_manifest_text(
                        &mut descriptor,
                        "category",
                        self.resolve_manifest_text(
                            &manifest.package.id,
                            &format!("mesh.contributes.keybinds.{keybind_id}.category"),
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
    resolved: mesh_core_locale::LocalizedTextResolution,
) {
    descriptor.insert(field.into(), serde_json::json!(resolved.text));
    descriptor.insert(
        format!("{field}_owner_module"),
        serde_json::json!(resolved.owner_module_id),
    );
    descriptor.insert(
        format!("{field}_missing"),
        serde_json::json!(resolved.missing),
    );
    descriptor.insert(
        format!("{field}_snapshot_revision"),
        serde_json::json!(resolved.snapshot_revision),
    );
    if let Some(source) = resolved.source {
        descriptor.insert(format!("{field}_source"), serde_json::json!(source));
    }
    if let Some(key) = resolved.key {
        descriptor.insert(format!("{field}_key"), serde_json::json!(key));
    }
    if let Some(fallback) = resolved.fallback {
        descriptor.insert(format!("{field}_fallback"), serde_json::json!(fallback));
    }
    if let Some(field_path) = resolved.field_path {
        descriptor.insert(format!("{field}_field_path"), serde_json::json!(field_path));
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
mod runtime_key_tests {
    use super::*;

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
}
