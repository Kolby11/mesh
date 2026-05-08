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
            self.flush_throttled_commands();
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
}
