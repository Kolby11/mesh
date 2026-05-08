use super::*;

impl ShellComponent for FrontendSurfaceComponent {
    fn id(&self) -> &str {
        &self.compiled.manifest.package.id
    }

    fn surface_id(&self) -> &str {
        self.compiled.surface_id()
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(self.surface_layout.visible_on_start)
    }

    fn mount(&mut self, ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError> {
        self.diagnostics = Some(ctx.diagnostics);
        self.load_module_i18n();
        self.load_catalog_i18n();
        self.record_declared_missing_icon_diagnostics();
        self.init_root_runtime()?;
        self.render_hooks_pending = true;
        self.dirty = true;
        Ok(vec![CoreRequest::PublishDiagnostics {
            message: format!(
                "mounted frontend component '{}' from {}",
                self.id(),
                self.compiled.source_path.display()
            ),
        }])
    }

    fn handle_core_event(&mut self, event: &CoreEvent) -> Result<Vec<CoreRequest>, ComponentError> {
        if let CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        } = event
        {
            // Any surface hiding may have been a popover triggered from
            // this surface — drop its registration so a stale Tab doesn't
            // try to re-enter it.
            if !visible && surface_id != self.surface_id() {
                self.triggered_popovers
                    .retain(|_, target| target != surface_id);
            }
            // Sync portal bookkeeping when an OTHER surface's visibility
            // changes. This handles two cases:
            //   1. Shell hides a popover via Tab transfer — the trigger
            //      surface's Lua may still think the popover is open, so
            //      a click would emit a redundant Hide.
            //   2. Surface shown via a non-portal path (mesh.popover.activate)
            //      bypassing tick()'s bookkeeping — the next tick would
            //      otherwise re-emit a stale HideSurface from the previous
            //      paint's pending_surface_states.
            // Update last_surface_states whenever this component owns a
            // portal binding for the surface (not just when the key was
            // already present), and clear any stale pending state so the
            // next tick's diff is honest.
            if surface_id != self.surface_id() {
                let portal_tracks = self
                    .portal_hidden_bindings
                    .borrow()
                    .contains_key(surface_id);
                if portal_tracks || self.last_surface_states.contains_key(surface_id) {
                    self.last_surface_states
                        .insert(surface_id.clone(), *visible);
                    self.pending_surface_states.borrow_mut().remove(surface_id);
                    if let Some(binding) = self
                        .portal_hidden_bindings
                        .borrow()
                        .get(surface_id)
                        .cloned()
                    {
                        let component_id = self.id().to_string();
                        if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(&component_id)
                        {
                            runtime
                                .script_ctx
                                .set_global_state(&binding, serde_json::json!(!*visible))
                                .map_err(|source| ComponentError::Script {
                                    component_id: component_id.clone(),
                                    source,
                                })?;
                            self.dirty = true;
                        }
                    }
                }
            }
            if surface_id == self.surface_id() {
                let was_visible = self.visible;
                self.visible = *visible;
                if !visible {
                    self.clear_selection();
                    self.focused_key = None;
                    self.focus_visible_key = None;
                    self.pending_auto_focus = false;
                    self.return_focus = None;
                    self.close_on_focus_leave = false;
                    self.keyboard_mode_override = None;
                } else if !was_visible && self.surface_layout.keyboard_mode != KeyboardMode::None {
                    self.pending_auto_focus = true;
                }
                self.dirty = true;
            }
        }
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let ServiceEvent::Updated {
            service,
            source_module,
            payload,
        } = event;
        self.last_service_update = Some(format!("{service}:{source_module}"));
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            let service_name = crate::shell::service::service_name_from_interface(service);
            let required = format!("service.{service_name}.read");
            let has_read = runtime
                .script_ctx
                .capabilities
                .is_granted(&Capability::new(&required));
            let previous = runtime.script_ctx.state().get(&service_name);
            let tracked_fields = runtime.script_ctx.tracked_fields_for_service(&service_name);
            apply_service_update(
                runtime.script_ctx.state_mut(),
                has_read,
                service,
                source_module,
                payload.clone(),
            );
            let state_changed = runtime.script_ctx.state().is_dirty();
            if has_read {
                runtime
                    .script_ctx
                    .apply_service_payload(&service_name, payload);
            }
            let tracked_fields_changed = has_read
                && tracked_service_fields_changed(previous.as_ref(), payload, &tracked_fields);
            if state_changed || tracked_fields_changed {
                self.render_hooks_pending = true;
                self.dirty = true;
            }
        }
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        // Trigger a repaint once the tooltip delay has elapsed so the tooltip appears.
        if let Some(start) = self.hover_start {
            if start.elapsed() >= TOOLTIP_DELAY && !self.dirty {
                self.dirty = true;
            }
        }

        // Emit Show/HideSurface requests for surface portals whose desired visibility changed.
        let pending = std::mem::take(&mut *self.pending_surface_states.borrow_mut());
        let mut requests = Vec::new();
        for (surface_id, visible) in pending {
            let was_visible = self.last_surface_states.get(&surface_id).copied();
            if was_visible != Some(visible) {
                self.last_surface_states.insert(surface_id.clone(), visible);
                if visible {
                    requests.push(CoreRequest::ShowSurface { surface_id });
                } else {
                    requests.push(CoreRequest::HideSurface { surface_id });
                }
            }
        }
        Ok(requests)
    }

    fn wants_render(&self) -> bool {
        self.dirty || !self.style_animations.is_empty() || self.has_active_keyframe_animation
    }

    fn surface_size_changed(&mut self, width: u32, height: u32) -> bool {
        self.observe_surface_size(width, height)
    }

    fn render(&mut self, surface: &mut dyn ShellSurface) -> Result<(), ComponentError> {
        self.render_layout(surface);

        if self.visible {
            surface.show();
        } else {
            surface.hide();
        }

        let template_nodes = self
            .compiled
            .component
            .template
            .as_ref()
            .map(|template| template.root.len())
            .unwrap_or(0);
        let role =
            root_accessibility_role(&self.compiled.manifest).unwrap_or_else(|| "unknown".into());

        tracing::debug!(
            "rendered frontend '{}' visible={} nodes={} role={}{}",
            self.id(),
            self.visible,
            template_nodes,
            role,
            self.last_service_update
                .as_deref()
                .map(|summary| format!(" service={summary}"))
                .unwrap_or_default()
        );

        self.dirty = false;
        Ok(())
    }

    fn paint(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        buffer: &mut PixelBuffer,
    ) -> Result<(), ComponentError> {
        let (requested_width, requested_height) = self.requested_layout_size();
        let content_width = if requested_width == 0 {
            width.max(1)
        } else {
            requested_width.max(1)
        };
        let content_height = if requested_height == 0 {
            height.max(1)
        } else {
            requested_height.max(1)
        };
        self.observe_surface_size(content_width, content_height);
        let mut tree = self.build_tree(theme, content_width, content_height);
        self.prune_stale_interaction_targets(&tree);
        self.apply_pending_auto_focus(&tree);
        self.apply_style_animations(&mut tree);
        if self.surface_layout.size_policy == SurfaceSizePolicy::ContentMeasured {
            let surface_layout_manifest = self.compiled.manifest.surface_layout.as_ref();
            let measured_size = measure_content_size(
                &tree,
                content_width,
                content_height,
                surface_layout_manifest,
            );
            if self.measured_size != Some(measured_size) {
                self.measured_size = Some(measured_size);
                self.dirty = true;
            }
        }
        self.publish_element_metrics(&tree);
        buffer.clear(mesh_core_elements::style::Color::TRANSPARENT);

        let tooltip = if let (Some(start), Some(hovered_key)) =
            (self.hover_start, self.hovered_key.as_ref())
        {
            if start.elapsed() >= TOOLTIP_DELAY {
                find_tooltip_text_by_key(&tree, hovered_key).map(|text| {
                    let (cx, cy) = self.hovered_pos;
                    (text, cx, cy)
                })
            } else {
                None
            }
        } else {
            None
        };

        let paint_started = std::time::Instant::now();
        paint_frontend_tree_at_for_module(
            &tree,
            buffer,
            1.0,
            0.0,
            0.0,
            tooltip
                .as_ref()
                .map(|(text, cx, cy)| (text.as_str(), *cx, *cy)),
            Some(self.compiled.manifest.package.id.as_str()),
        );
        if self.profiling_enabled {
            self.profiling_records.push(ComponentProfilingRecord {
                stage: mesh_core_debug::ProfilingStage::Paint,
                duration: paint_started.elapsed(),
                module_id: Some(self.compiled.manifest.package.id.clone()),
                trigger_kind: Some("rebuild".to_string()),
            });
        }
        self.last_tree = Some(tree);
        self.clear_runtime_dirty_states();

        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), ComponentError> {
        self.dirty = true;
        Ok(())
    }

    fn locale_changed(&mut self, locale: &LocaleEngine) -> Result<(), ComponentError> {
        self.locale.set_locale(locale.current());
        self.runtimes.lock().unwrap().clear();
        self.init_root_runtime()?;
        self.render_hooks_pending = true;
        self.dirty = true;
        Ok(())
    }

    fn source_path(&self) -> Option<&Path> {
        Some(self.compiled.source_path.as_path())
    }

    fn watched_source_paths(&self) -> Vec<PathBuf> {
        self.compiled.watched_paths.clone()
    }

    fn module_settings_path(&self) -> Option<&Path> {
        if self.module_settings_file.exists() {
            Some(self.module_settings_file.as_path())
        } else {
            None
        }
    }

    fn reload_module_settings(&mut self) -> Result<bool, ComponentError> {
        let settings_state =
            load_frontend_module_settings(&self.module_settings_file, &self.compiled.manifest);
        let layout_changed = self.surface_layout != settings_state.layout;
        let settings_changed = self.settings_json != settings_state.raw;

        self.surface_layout = settings_state.layout;
        self.settings_json = settings_state.raw;

        if settings_changed {
            if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
                runtime
                    .script_ctx
                    .state_mut()
                    .set("settings", self.settings_json.clone());
            }
        }

        let Some(locale) = self
            .settings_json
            .get("i18n")
            .and_then(|i18n| i18n.get("default_locale"))
            .and_then(|l| l.as_str())
        else {
            if layout_changed || settings_changed {
                self.dirty = true;
            }
            return Ok(layout_changed || settings_changed);
        };

        if self.locale.current() != locale {
            tracing::info!(
                "module '{}': applying locale '{}' from module settings",
                self.id(),
                locale
            );
            self.locale.set_locale(locale);
        }

        if layout_changed || settings_changed {
            self.dirty = true;
        }
        Ok(layout_changed || settings_changed)
    }

    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        let manifest = self.compiled.manifest.clone();
        let recompiled = compile_frontend_module(&manifest, &self.module_dir).map_err(|err| {
            ComponentError::Failed {
                component_id: self.id().to_string(),
                message: format!("frontend recompile failed: {err}"),
            }
        })?;

        let component_id = self.id().to_string();
        self.compiled = recompiled;
        if let Some(entry) = self.frontend_catalog.modules.get_mut(&component_id) {
            entry.compiled = self.compiled.clone();
        }
        self.runtimes.lock().unwrap().clear();
        self.init_root_runtime()?;
        self.render_hooks_pending = true;
        self.dirty = true;
        Ok(true)
    }

    fn handle_input(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        self.handle_component_input(theme, width, height, input)
    }

    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        self.last_tree.as_ref()
    }

    fn apply_position(&mut self, margin_top: i32, margin_left: i32) {
        self.surface_layout.edge = Edge::Left;
        self.surface_layout.margin_top = margin_top;
        self.surface_layout.margin_left = margin_left;
        self.dirty = true;
    }

    fn allows_shrink_to_fit(&self) -> bool {
        self.surface_layout.size_policy == SurfaceSizePolicy::ContentMeasured
    }

    fn set_profiling_enabled(&mut self, enabled: bool) {
        self.profiling_enabled = enabled;
        if !enabled {
            self.profiling_records.clear();
        }
    }

    fn take_profiling_records(&mut self) -> Vec<ComponentProfilingRecord> {
        std::mem::take(&mut self.profiling_records)
    }

    fn receive_focus_transfer(
        &mut self,
        target: &TabFocusTarget,
        return_focus: Option<(String, String)>,
        close_on_focus_leave: bool,
    ) {
        if let Some(tree) = self.last_tree.clone() {
            self.apply_focus_transfer(&tree, target, return_focus, close_on_focus_leave);
        } else {
            // No tree yet — defer via pending_auto_focus and keep return target.
            self.pending_auto_focus = true;
            self.return_focus = return_focus;
            self.close_on_focus_leave = close_on_focus_leave;
        }
    }

    fn release_focus_for_transfer(&mut self) {
        self.clear_focus_for_transfer();
    }

    fn register_popover_trigger(&mut self, trigger_key: String, popover_surface: String) {
        self.triggered_popovers.insert(trigger_key, popover_surface);
    }

    fn unregister_popover_trigger(&mut self, popover_surface: &str) {
        self.triggered_popovers
            .retain(|_, surface| surface != popover_surface);
    }

    fn set_keyboard_mode_override(&mut self, mode: Option<KeyboardMode>) {
        self.keyboard_mode_override = mode;
        self.dirty = true;
    }
}
