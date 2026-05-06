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
        self.load_plugin_i18n();
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
            if surface_id == self.surface_id() {
                self.visible = *visible;
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
            source_plugin,
            payload,
        } = event;
        self.last_service_update = Some(format!("{service}:{source_plugin}"));
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
                source_plugin,
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
        self.dirty || !self.style_animations.is_empty()
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

        paint_frontend_tree_at(
            &tree,
            buffer,
            1.0,
            0.0,
            0.0,
            tooltip
                .as_ref()
                .map(|(text, cx, cy)| (text.as_str(), *cx, *cy)),
        );
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

    fn plugin_settings_path(&self) -> Option<&Path> {
        if self.plugin_settings_file.exists() {
            Some(self.plugin_settings_file.as_path())
        } else {
            None
        }
    }

    fn reload_plugin_settings(&mut self) -> Result<bool, ComponentError> {
        let settings_state =
            load_frontend_plugin_settings(&self.plugin_settings_file, &self.compiled.manifest);
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
                "plugin '{}': applying locale '{}' from plugin settings",
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
        let recompiled = compile_frontend_plugin(&manifest, &self.plugin_dir).map_err(|err| {
            ComponentError::Failed {
                component_id: self.id().to_string(),
                message: format!("frontend recompile failed: {err}"),
            }
        })?;

        let component_id = self.id().to_string();
        self.compiled = recompiled;
        if let Some(entry) = self.frontend_catalog.plugins.get_mut(&component_id) {
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
}
