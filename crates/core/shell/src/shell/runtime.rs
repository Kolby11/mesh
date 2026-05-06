use super::*;

impl Shell {
    pub(super) fn build_debug_snapshot(&self) -> DebugSnapshot {
        let modules = self
            .modules
            .values()
            .map(|inst| ModuleEntry {
                id: inst.manifest.package.id.clone(),
                module_type: format!("{:?}", inst.manifest.package.module_type).to_lowercase(),
                state: inst.state.to_string(),
                error_count: inst.error_count,
                last_error: inst.last_error.clone(),
            })
            .collect();

        let catalog = self.interfaces.catalog();
        let mut interfaces: Vec<InterfaceEntry> = catalog
            .providers
            .iter()
            .map(|(name, providers)| {
                let providers = providers
                    .iter()
                    .map(|p| ProviderEntry {
                        backend_name: p.backend_name.clone(),
                        priority: p.priority,
                    })
                    .collect();
                InterfaceEntry {
                    name: name.clone(),
                    providers,
                }
            })
            .collect();
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));

        let health = self
            .diagnostics
            .snapshot()
            .into_iter()
            .map(|(id, status)| HealthEntry {
                module_id: id,
                status: status.to_string(),
            })
            .collect();

        let mut backend_runtimes: Vec<BackendRuntimeEntry> = self
            .backend_runtime_statuses
            .values()
            .map(|entry| BackendRuntimeEntry {
                interface: entry.interface.clone(),
                provider_id: entry.provider_id.clone(),
                status: entry.status.as_str().to_string(),
                message: entry.message.clone(),
                failure_count: entry.failure_count,
            })
            .collect();
        backend_runtimes.sort_by(|a, b| {
            a.interface
                .cmp(&b.interface)
                .then_with(|| a.provider_id.cmp(&b.provider_id))
        });

        let active_surfaces = self
            .core
            .surfaces
            .iter()
            .filter(|(_, s)| s.visible)
            .map(|(id, _)| id.clone())
            .collect();

        DebugSnapshot {
            modules,
            interfaces,
            backend_runtimes,
            health,
            active_surfaces,
        }
    }

    pub fn run(&mut self) -> Result<(), ShellRunError> {
        self.discover_modules();
        for theme in mesh_core_theme::load_themes_from_dir(&mesh_core_theme::theme_dir_path()) {
            tracing::debug!("registering theme '{}'", theme.id);
            self.theme.register_theme(theme);
        }
        self.resolve_modules()?;
        self.load_frontend_components()?;

        let runtime = Runtime::new().map_err(ShellRunError::RuntimeInit)?;
        let (tx, mut rx) = mpsc::unbounded_channel::<ShellMessage>();
        self.spawn_backend_modules(&runtime, tx.clone());
        let ipc_socket_path = default_ipc_socket_path();
        spawn_ipc_server(&runtime, ipc_socket_path.clone(), tx).map_err(|source| {
            ShellRunError::IpcInit {
                path: ipc_socket_path.clone(),
                source,
            }
        })?;

        let mut pending = VecDeque::new();
        pending.extend(self.mount_components()?);
        pending.extend(self.replay_cached_service_events()?);
        self.mark_components_locale_changed()?;
        pending.extend(self.broadcast_core_event(CoreEvent::Started)?);
        play_shell_sound(
            SoundKind::Startup,
            &self.settings.sounds,
            self.service_handlers.get("mesh.audio"),
        );

        tracing::info!(
            "MESH shell core is running with {} frontend component(s)",
            self.components.len()
        );

        while !self.core.shutting_down {
            pending.extend(self.reload_theme_if_changed()?);
            pending.extend(self.reload_locale_if_settings_changed()?);
            self.reload_module_settings_if_changed()?;
            self.reload_frontend_components_if_changed()?;
            self.dispatch_wayland()?;

            while let Ok(message) = rx.try_recv() {
                match message {
                    ShellMessage::Service(event) => {
                        pending.extend(self.broadcast_service_event(event)?);
                    }
                    ShellMessage::BackendLifecycle {
                        interface,
                        provider_id,
                        stage,
                        status,
                        message,
                    } => self.handle_backend_lifecycle(
                        interface,
                        provider_id,
                        stage,
                        status,
                        message,
                    ),
                    ShellMessage::Ipc(request) => {
                        pending.push_back(request);
                    }
                }
            }

            pending.extend(self.tick_components()?);
            self.drain_requests(&mut pending)?;
            self.render_components()?;
            self.flush_wayland()?;
            self.render_engine.pump();

            std::thread::sleep(Duration::from_millis(16));
        }

        let mut shutdown_requests = self.broadcast_core_event(CoreEvent::ShuttingDown)?;
        self.drain_requests(&mut shutdown_requests)?;
        let _ = std::fs::remove_file(&ipc_socket_path);
        tracing::info!("shell event loop stopped");
        Ok(())
    }

    pub(super) fn reload_theme_if_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let Ok(metadata) = std::fs::metadata(&self.theme_watch.path) else {
            return Ok(VecDeque::new());
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(VecDeque::new());
        };

        if self.theme_watch.modified_at == Some(modified_at) {
            return Ok(VecDeque::new());
        }

        let old_theme_id = self.theme.active().id.clone();
        let theme = mesh_core_theme::load_theme_from_path(&self.theme_watch.path)
            .map_err(ShellRunError::Theme)?;
        tracing::info!(
            "reloaded active theme '{}' from {}",
            theme.id,
            self.theme_watch.path.display()
        );
        self.theme.replace_active(theme);
        self.theme_watch.modified_at = Some(modified_at);
        self.mark_components_theme_changed()?;
        let new_theme_id = self.theme.active().id.clone();
        if new_theme_id != old_theme_id {
            return self.sync_theme_service_state(&new_theme_id);
        }
        Ok(VecDeque::new())
    }

    fn reload_frontend_components_if_changed(&mut self) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            let Some(source_path) = runtime.source_path.as_ref() else {
                continue;
            };

            let Ok(metadata) = std::fs::metadata(source_path) else {
                continue;
            };
            let Ok(modified_at) = metadata.modified() else {
                continue;
            };

            if runtime.source_modified_at == Some(modified_at) {
                continue;
            }

            let reloaded = runtime
                .component
                .reload_source()
                .map_err(ShellRunError::Component)?;
            runtime.source_modified_at = Some(modified_at);

            if reloaded {
                tracing::info!(
                    "recompiled frontend component '{}' from {}",
                    runtime.component.id(),
                    source_path.display()
                );
            }
        }

        Ok(())
    }

    fn mark_components_theme_changed(&mut self) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            runtime
                .component
                .theme_changed()
                .map_err(ShellRunError::Component)?;
        }
        Ok(())
    }

    fn apply_set_theme(&mut self, theme_id: &str) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if self.theme.set_active(theme_id).is_err() {
            let path = mesh_core_theme::theme_path_for_id(theme_id);
            match mesh_core_theme::load_theme_from_path(&path) {
                Ok(theme) => {
                    self.theme.register_theme(theme);
                    if let Err(e) = self.theme.set_active(theme_id) {
                        tracing::warn!("failed to activate theme '{theme_id}': {e}");
                        return Ok(VecDeque::new());
                    }
                }
                Err(e) => {
                    tracing::warn!("cannot load theme '{theme_id}': {e}");
                    return Ok(VecDeque::new());
                }
            }
        }
        tracing::info!("active theme changed to '{theme_id}'");
        let path = mesh_core_theme::theme_path_for_id(theme_id);
        let modified_at = std::fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        self.theme_watch = ThemeWatchState { path, modified_at };
        self.mark_components_theme_changed()?;
        self.sync_theme_service_state(theme_id)
    }

    pub(super) fn sync_theme_service_state(
        &mut self,
        theme_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let is_dark = theme_id.contains("dark");
        let payload =
            serde_json::json!({ "current": theme_id, "theme_id": theme_id, "is_dark": is_dark });
        if let Some(tx) = self.service_handlers.get("mesh.theme") {
            let _ = tx.send(ServiceCommandMsg {
                command: "set-current".to_string(),
                payload: payload.clone(),
            });
        }
        self.broadcast_service_event(ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: "@mesh/shell".into(),
            payload,
        })
    }

    pub(super) fn reload_locale_if_settings_changed(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        let Ok(metadata) = std::fs::metadata(&self.settings_watch.path) else {
            return Ok(requests);
        };
        let Ok(modified_at) = metadata.modified() else {
            return Ok(requests);
        };

        if self.settings_watch.modified_at == Some(modified_at) {
            return Ok(requests);
        }

        self.settings_watch.modified_at = Some(modified_at);

        let new_settings = match load_shell_settings() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("failed to reload shell settings: {e}");
                return Ok(requests);
            }
        };

        let old_theme = self.settings.theme.clone();
        let old_i18n = self.settings.i18n.clone();
        let new_i18n = &new_settings.i18n;
        let locale_changed = old_i18n.locale != new_i18n.locale
            || old_i18n.fallback_locale != new_i18n.fallback_locale;

        let theme_changed = old_theme.active != new_settings.theme.active;
        if theme_changed {
            let (theme, theme_watch) = load_active_theme(&new_settings);
            let active_theme_id = theme.active().id.clone();
            tracing::info!(
                "active theme changed: {} -> {}",
                old_theme.active,
                active_theme_id
            );
            self.theme = theme;
            self.theme_watch = theme_watch;
            self.mark_components_theme_changed()?;
            requests.extend(self.sync_theme_service_state(&active_theme_id)?);
        }

        if locale_changed {
            tracing::info!(
                "locale changed: {} (fallback: {}) -> {} (fallback: {})",
                old_i18n.locale,
                old_i18n.fallback_locale,
                new_i18n.locale,
                new_i18n.fallback_locale,
            );
            self.locale = LocaleEngine::with_fallback_locale(
                new_i18n.locale.clone(),
                new_i18n.fallback_locale.clone(),
            );
            self.mark_components_locale_changed()?;
        }

        self.settings = new_settings;

        Ok(requests)
    }

    fn reload_module_settings_if_changed(&mut self) -> Result<(), ShellRunError> {
        for runtime in &mut self.components {
            let current_settings_path = runtime.component.module_settings_path().map(PathBuf::from);
            if runtime.module_settings_path != current_settings_path {
                runtime.module_settings_path = current_settings_path.clone();
                runtime.module_settings_modified_at = None;
            }

            let Some(settings_path) = current_settings_path
                .as_ref()
                .or(runtime.module_settings_path.as_ref())
            else {
                continue;
            };

            let Ok(metadata) = std::fs::metadata(settings_path) else {
                continue;
            };
            let Ok(modified_at) = metadata.modified() else {
                continue;
            };

            if runtime.module_settings_modified_at == Some(modified_at) {
                continue;
            }

            runtime.module_settings_modified_at = Some(modified_at);

            let changed = runtime
                .component
                .reload_module_settings()
                .map_err(ShellRunError::Component)?;

            if changed {
                tracing::info!(
                    "module settings changed for component '{}'",
                    runtime.component.id()
                );
            }
        }
        Ok(())
    }

    fn mark_components_locale_changed(&mut self) -> Result<(), ShellRunError> {
        let locale = self.locale.clone();
        for runtime in &mut self.components {
            runtime
                .component
                .locale_changed(&locale)
                .map_err(ShellRunError::Component)?;
        }
        Ok(())
    }

    fn tick_components(&mut self) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            requests.extend(runtime.component.tick().map_err(ShellRunError::Component)?);
        }
        Ok(requests)
    }

    fn broadcast_core_event(
        &mut self,
        event: CoreEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            requests.extend(
                runtime
                    .component
                    .handle_core_event(&event)
                    .map_err(ShellRunError::Component)?,
            );
        }
        Ok(requests)
    }

    pub(super) fn broadcast_service_event(
        &mut self,
        event: ServiceEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if !self.record_latest_service_state(&event) {
            return Ok(VecDeque::new());
        }
        let mut requests = VecDeque::new();
        for runtime in &mut self.components {
            requests.extend(
                runtime
                    .component
                    .handle_service_event(&event)
                    .map_err(ShellRunError::Component)?,
            );
        }
        Ok(requests)
    }

    pub(super) fn record_latest_service_state(&mut self, event: &ServiceEvent) -> bool {
        let ServiceEvent::Updated {
            service,
            source_module,
            payload,
        } = event;
        let interface = canonical_interface_name(service);
        let shell_authoritative_theme_update =
            interface == "mesh.theme" && source_module == "@mesh/shell";
        if let Some(slot) = self.backend_runtimes.get(&interface) {
            if slot.provider_id != *source_module && !shell_authoritative_theme_update {
                tracing::debug!(
                    interface,
                    source_module,
                    active_provider = %slot.provider_id,
                    "ignoring stale service update from inactive provider"
                );
                return false;
            }
        } else if self
            .backend_runtime_statuses
            .get(&(interface.clone(), source_module.clone()))
            .is_some_and(|entry| {
                matches!(
                    entry.status,
                    BackendRuntimeStatus::InitFailed
                        | BackendRuntimeStatus::Failed
                        | BackendRuntimeStatus::Stopped
                )
            })
        {
            tracing::debug!(
                interface,
                source_module,
                "ignoring service update from terminal backend provider"
            );
            return false;
        }
        self.validate_service_state_shape(&interface, source_module, payload);
        self.latest_service_state.insert(
            interface.clone(),
            LatestServiceState {
                interface,
                provider_id: source_module.clone(),
                state: payload.clone(),
            },
        );
        true
    }

    fn validate_service_state_shape(
        &mut self,
        interface: &str,
        provider_id: &str,
        payload: &serde_json::Value,
    ) {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract.as_ref() else {
            return;
        };
        for warning in service_state_contract_warnings(contract, payload) {
            self.record_service_contract_warning(interface, provider_id, warning);
        }
    }

    fn record_service_contract_warning(
        &mut self,
        interface: &str,
        provider_id: &str,
        message: String,
    ) {
        let message = format!("service_contract_warning: {interface}: {message}");
        tracing::warn!(interface, provider_id, "{message}");
        self.diagnostics.record_lifecycle_error(
            provider_id.to_string(),
            "service_contract_warning",
            message,
        );
    }

    fn replay_cached_service_events(&mut self) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        let events = self
            .latest_service_state
            .values()
            .map(|latest| ServiceEvent::Updated {
                service: latest.interface.clone(),
                source_module: latest.provider_id.clone(),
                payload: latest.state.clone(),
            })
            .collect::<Vec<_>>();
        for event in events {
            requests.extend(self.broadcast_service_event(event)?);
        }
        Ok(requests)
    }

    fn drain_requests(
        &mut self,
        requests: &mut VecDeque<CoreRequest>,
    ) -> Result<(), ShellRunError> {
        while let Some(request) = requests.pop_front() {
            let emitted = self.apply_request(request)?;
            requests.extend(emitted);
        }
        Ok(())
    }

    pub(super) fn apply_request(
        &mut self,
        request: CoreRequest,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        match request {
            CoreRequest::PositionSurface {
                surface_id,
                margin_top,
                margin_left,
            } => {
                if let Some(runtime) = self
                    .components
                    .iter_mut()
                    .find(|r| r.surface_id == surface_id)
                {
                    runtime.component.apply_position(margin_top, margin_left);
                }
                Ok(VecDeque::new())
            }
            CoreRequest::ToggleSurface { surface_id } => {
                let visible = self
                    .core
                    .surfaces
                    .get(&surface_id)
                    .map(|state| !state.visible)
                    .unwrap_or(true);
                self.set_surface_visibility(surface_id, visible)
            }
            CoreRequest::ShowSurface { surface_id } => {
                self.set_surface_visibility(surface_id, true)
            }
            CoreRequest::HideSurface { surface_id } => {
                self.set_surface_visibility(surface_id, false)
            }
            CoreRequest::PublishDiagnostics { message } => {
                tracing::info!("diagnostic: {message}");
                Ok(VecDeque::new())
            }
            CoreRequest::WriteClipboard { text } => {
                if let Err(err) = self.clipboard.write_text(&text) {
                    tracing::warn!(error = %err, "failed to write selection to clipboard");
                }
                Ok(VecDeque::new())
            }
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                source_module_id,
                source_capabilities,
            } => {
                let _ = self.dispatch_service_command(
                    &interface,
                    &command,
                    &payload,
                    &source_module_id,
                    &source_capabilities,
                );
                Ok(VecDeque::new())
            }
            CoreRequest::SetTheme { theme_id } => self.apply_set_theme(&theme_id),
            CoreRequest::ToggleDebugOverlay => {
                self.debug.toggle();
                tracing::debug!(
                    "debug overlay: {}",
                    if self.debug.enabled { "on" } else { "off" }
                );
                Ok(VecDeque::new())
            }
            CoreRequest::CycleDebugTab => {
                self.debug.cycle_tab();
                Ok(VecDeque::new())
            }
            CoreRequest::Shutdown => {
                self.core.shutting_down = true;
                Ok(VecDeque::new())
            }
        }
    }

    pub(super) fn dispatch_service_command(
        &mut self,
        interface: &str,
        command: &str,
        payload: &serde_json::Value,
        source_module_id: &str,
        source_capabilities: &mesh_core_capability::CapabilitySet,
    ) -> serde_json::Value {
        let required = service_command_control_capability(interface);
        if !source_capabilities.is_granted(&required) {
            tracing::warn!(
                source_module_id,
                interface,
                command,
                required_capability = %required,
                "denied unauthorized service command dispatch"
            );
            return serde_json::json!({
                "ok": false,
                "error": "capability_denied",
                "status": "capability_denied",
            });
        }

        if !self.service_command_is_supported(interface, command) {
            let message = format!("unsupported_service_command: {interface}.{command}");
            tracing::warn!(
                source_module_id,
                interface,
                command,
                "unsupported_service_command"
            );
            self.diagnostics.record_lifecycle_error(
                source_module_id.to_string(),
                "unsupported_service_command",
                message.clone(),
            );
            return serde_json::json!({
                "ok": false,
                "error": message,
                "status": "unsupported_service_command",
            });
        }

        if let Some(tx) = self.service_handlers.get(interface) {
            match tx.send(ServiceCommandMsg {
                command: command.to_string(),
                payload: payload.clone(),
            }) {
                Ok(()) => serde_json::json!({ "ok": true, "queued": true }),
                Err(_) => serde_json::json!({
                    "ok": false,
                    "error": "service_unavailable",
                    "status": "service_unavailable",
                }),
            }
        } else {
            tracing::debug!("no handler registered for service: {interface}");
            serde_json::json!({
                "ok": false,
                "error": "service_unavailable",
                "status": "service_unavailable",
            })
        }
    }

    fn service_command_is_supported(&self, interface: &str, command: &str) -> bool {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract.as_ref() else {
            return true;
        };
        contract.methods.iter().any(|method| method.name == command)
    }

    fn set_surface_visibility(
        &mut self,
        surface_id: SurfaceId,
        visible: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        self.core
            .surfaces
            .entry(surface_id.clone())
            .and_modify(|state| state.visible = visible)
            .or_insert(SurfaceState { visible });

        self.broadcast_core_event(CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        })
    }

    fn render_components(&mut self) -> Result<(), ShellRunError> {
        let debug_snapshot = self.debug.enabled.then(|| self.build_debug_snapshot());

        for runtime in &mut self.components {
            let surface_size = {
                let surface = self
                    .surfaces
                    .get(&runtime.surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
                if surface.width == 0 || surface.height == 0 {
                    self.render_engine.surface_size(&runtime.surface_id)?
                } else {
                    Some((surface.width.max(1), surface.height.max(1)))
                }
            };
            if let Some((width, height)) = surface_size {
                runtime.component.surface_size_changed(width, height);
            }
            if !runtime.component.wants_render() {
                continue;
            }

            let mut rerender_attempts = 0;
            let mut buffer = loop {
                let surface = self
                    .surfaces
                    .get_mut(&runtime.surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
                runtime
                    .component
                    .render(surface)
                    .map_err(ShellRunError::Component)?;

                let visible = self
                    .core
                    .surfaces
                    .get(&runtime.surface_id)
                    .map(|state| state.visible)
                    .unwrap_or(surface.visible);
                let cfg = if visible {
                    LayerSurfaceConfig {
                        edge: surface.edge,
                        layer: surface.layer.unwrap_or(Layer::Top),
                        width: surface.width,
                        height: surface.height,
                        exclusive_zone: surface.exclusive_zone,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: runtime.surface_id.clone(),
                        margin_top: surface.margin_top,
                        margin_right: surface.margin_right,
                        margin_bottom: surface.margin_bottom,
                        margin_left: surface.margin_left,
                    }
                } else {
                    LayerSurfaceConfig {
                        edge: surface.edge,
                        layer: surface.layer.unwrap_or(Layer::Top),
                        width: 1,
                        height: 1,
                        exclusive_zone: 0,
                        keyboard_mode: surface.keyboard_mode,
                        namespace: runtime.surface_id.clone(),
                        margin_top: 0,
                        margin_right: 0,
                        margin_bottom: 0,
                        margin_left: 0,
                    }
                };
                self.render_engine.configure(&runtime.surface_id, cfg);

                if !visible {
                    break PixelBuffer::new(1, 1);
                }

                let configured_size = if surface.width == 0 || surface.height == 0 {
                    self.render_engine.surface_size(&runtime.surface_id)?
                } else {
                    None
                };
                let width = if surface.width == 0 {
                    configured_size.map(|(width, _)| width).unwrap_or(1)
                } else {
                    surface.width.max(1)
                };
                let height = if surface.height == 0 {
                    configured_size.map(|(_, height)| height).unwrap_or(1)
                } else {
                    surface.height.max(1)
                };
                let mut buffer = PixelBuffer::new(width, height);
                runtime
                    .component
                    .paint(self.theme.active(), width, height, &mut buffer)
                    .map_err(ShellRunError::Component)?;

                if !runtime.component.wants_render() || rerender_attempts >= 1 {
                    break buffer;
                }

                rerender_attempts += 1;
            };

            let visible = self
                .core
                .surfaces
                .get(&runtime.surface_id)
                .map(|state| state.visible)
                .unwrap_or_else(|| {
                    self.surfaces
                        .get(&runtime.surface_id)
                        .map(|surface| surface.visible)
                        .unwrap_or(true)
                });

            if visible && let Some(snapshot) = &debug_snapshot {
                if self.debug.show_layout_bounds {
                    if let Some(tree) = runtime.component.last_widget_tree() {
                        self.debug_overlay
                            .paint_layout_bounds(tree, &mut buffer, 1.0);
                    }
                }
                self.debug_overlay
                    .paint_panel(snapshot, self.debug.active_tab, &mut buffer, 1.0);
            }

            self.render_engine
                .present(&runtime.surface_id, runtime.component.id(), visible, &buffer)
                .map_err(ShellRunError::Render)?;
        }
        Ok(())
    }

    fn dispatch_wayland(&mut self) -> Result<(), ShellRunError> {
        let events = coalesce_pointer_moves(self.render_engine.poll_events());
        for event in events {
            tracing::trace!(
                "[hover] dispatch_wayland: got event {:?}",
                std::mem::discriminant(&event)
            );
            let surface_id = event_surface_id(&event);

            let Some(index) = self
                .components
                .iter()
                .position(|runtime| runtime.surface_id == *surface_id)
            else {
                continue;
            };

            let runtime_surface_id = self.components[index].surface_id.clone();
            let Some(surface) = self.surfaces.get(&runtime_surface_id) else {
                continue;
            };
            let fixed_surface_size = if surface.width == 0 || surface.height == 0 {
                None
            } else {
                Some((surface.width.max(1), surface.height.max(1)))
            };
            let _ = surface;
            let surface_size = fixed_surface_size
                .or(self.render_engine.surface_size(&runtime_surface_id)?)
                .unwrap_or((1, 1));

            if let WindowEvent::Key {
                event: WindowKeyEvent::Pressed(key, mods),
                ..
            } = &event
            {
                if let Some(request) =
                    shell_global_shortcut_request(key, mods.ctrl, mods.shift, self.debug.enabled)
                {
                    let mut pending = VecDeque::from([request]);
                    self.drain_requests(&mut pending)?;
                    continue;
                }
            }

            let input = match event {
                WindowEvent::PointerMove { x, y, .. } => ComponentInput::PointerMove { x, y },
                WindowEvent::PointerButton { x, y, pressed, .. } => {
                    ComponentInput::PointerButton { x, y, pressed }
                }
                WindowEvent::Scroll { x, y, dx, dy, .. } => ComponentInput::Scroll { x, y, dx, dy },
                WindowEvent::Key {
                    event: WindowKeyEvent::Pressed(key, mods),
                    ..
                } => {
                    self.active_key_modifiers = KeyModifiers {
                        ctrl: mods.ctrl,
                        shift: mods.shift,
                        alt: mods.alt,
                    };
                    component_key_pressed_input(key, mods.ctrl, mods.shift, mods.alt)
                }
                WindowEvent::Key {
                    event: WindowKeyEvent::Released(key),
                    ..
                } => {
                    update_modifiers_for_key_release(&mut self.active_key_modifiers, &key);
                    component_key_released_input(key, self.active_key_modifiers)
                }
                WindowEvent::Char { ch, .. } => ComponentInput::Char { ch },
            };

            tracing::trace!(
                "[hover] dispatch_wayland: routing event to surface_id={}",
                runtime_surface_id
            );
            let emitted = {
                let runtime = &mut self.components[index];
                runtime
                    .component
                    .surface_size_changed(surface_size.0, surface_size.1);
                runtime.component.handle_input(
                    self.theme.active(),
                    surface_size.0,
                    surface_size.1,
                    input,
                )
            }
            .map_err(ShellRunError::Component)?;

            for request in emitted {
                let mut pending = VecDeque::from([request]);
                self.drain_requests(&mut pending)?;
            }
        }

        Ok(())
    }

    fn flush_wayland(&mut self) -> Result<(), ShellRunError> {
        for runtime in &self.components {
            let surface = self
                .surfaces
                .get(&runtime.surface_id)
                .ok_or_else(|| ShellRunError::MissingSurface(runtime.surface_id.clone()))?;
            tracing::trace!(
                "flushing surface '{}' size={}x{} visible={}",
                runtime.surface_id,
                surface.width,
                surface.height,
                surface.visible
            );
        }
        Ok(())
    }
}

fn service_state_contract_warnings(
    contract: &InterfaceContract,
    payload: &serde_json::Value,
) -> Vec<String> {
    let Some(object) = payload.as_object() else {
        return vec![format!(
            "state for {} must be a JSON object, got {}",
            contract.interface,
            json_type_name(payload)
        )];
    };

    let mut warnings = Vec::new();
    for field in &contract.state_fields {
        if is_runtime_metadata_state_field(&field.name) {
            continue;
        }
        let Some(value) = object.get(&field.name) else {
            warnings.push(format!(
                "missing required state field '{}' for {}",
                field.name, contract.interface
            ));
            continue;
        };
        if !json_value_matches_contract_type(value, &field.field_type) {
            warnings.push(format!(
                "state field '{}' for {} expected {}, got {}",
                field.name,
                contract.interface,
                field.field_type,
                json_type_name(value)
            ));
        }
    }
    warnings
}

fn is_runtime_metadata_state_field(name: &str) -> bool {
    name == "source_module"
}

fn json_value_matches_contract_type(value: &serde_json::Value, field_type: &str) -> bool {
    let normalized = field_type.trim().to_ascii_lowercase();
    if normalized.starts_with('[') && normalized.ends_with(']') {
        return value.is_array();
    }

    match normalized.as_str() {
        "bool" | "boolean" => value.is_boolean(),
        "float" | "double" | "number" => value.is_number(),
        "int" | "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "string" => value.is_string(),
        "object" | "table" | "map" => value.is_object(),
        _ => true,
    }
}

fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}
