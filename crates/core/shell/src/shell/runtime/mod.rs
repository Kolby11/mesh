use super::*;

mod debug;
pub(crate) mod profiling;
mod reload;
mod render;
mod request;
mod service_state;
mod theme;
mod wayland;

impl Shell {
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
                self.handle_shell_message(&mut pending, message)?;
            }

            pending.extend(self.tick_components()?);
            pending.extend(self.complete_due_surface_transitions()?);
            self.drain_requests(&mut pending)?;
            self.flush_throttled_commands();
            self.render_components()?;
            self.flush_wayland()?;
            self.presentation_engine.pump();

            std::thread::sleep(Duration::from_millis(16));
        }

        let mut shutdown_requests = self.broadcast_core_event(CoreEvent::ShuttingDown)?;
        self.drain_requests(&mut shutdown_requests)?;
        let _ = std::fs::remove_file(&ipc_socket_path);
        tracing::info!("shell event loop stopped");
        Ok(())
    }

    pub(in crate::shell) fn handle_shell_message(
        &mut self,
        pending: &mut VecDeque<CoreRequest>,
        message: ShellMessage,
    ) -> Result<(), ShellRunError> {
        let message_started = self.profiling_enabled().then(std::time::Instant::now);
        let trigger_kind = match &message {
            ShellMessage::Service(_) => "service_event",
            ShellMessage::BackendServiceUpdate { .. } => "backend_service_update",
            ShellMessage::BackendLifecycle { .. } => "backend_lifecycle",
            ShellMessage::BackendCommandResult { .. } => "backend_command_result",
            ShellMessage::BackendInterfaceEvent { .. } => "backend_interface_event",
            ShellMessage::Ipc(_) => "ipc",
        };
        match message {
            ShellMessage::Service(event) => {
                pending.extend(self.broadcast_service_event(event)?);
            }
            ShellMessage::BackendServiceUpdate {
                interface,
                provider_id,
                event,
            } => {
                let profiling_started = self.profiling_enabled().then(std::time::Instant::now);
                if self.record_latest_service_state(&event) {
                    pending.extend(self.deliver_service_event(&event)?);
                    if let Some(started) = profiling_started {
                        self.record_backend_profiling_stage(
                            &interface,
                            &provider_id,
                            mesh_core_debug::ProfilingBackendStage::PollUpdate,
                            started.elapsed(),
                            Some("service_update"),
                        );
                    }
                }
            }
            ShellMessage::BackendLifecycle {
                interface,
                provider_id,
                stage,
                status,
                message,
            } => self.handle_backend_lifecycle(interface, provider_id, stage, status, message),
            ShellMessage::BackendCommandResult {
                interface,
                provider_id,
                command,
                result,
            } => self.record_backend_method_result(interface, provider_id, command, result),
            ShellMessage::BackendInterfaceEvent {
                interface,
                provider_id,
                name,
                payload,
            } => {
                pending.extend(self.broadcast_backend_interface_event(
                    interface,
                    provider_id,
                    name,
                    payload,
                )?);
            }
            ShellMessage::Ipc(request) => {
                pending.push_back(request);
            }
        }
        if let Some(started) = message_started {
            self.record_shell_profiling_stage(
                mesh_core_debug::ProfilingStage::RuntimeUpdateHandling,
                started.elapsed(),
                Some(trigger_kind),
            );
        }
        Ok(())
    }
}
