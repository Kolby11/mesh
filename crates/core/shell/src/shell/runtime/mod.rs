use super::*;

mod debug;
pub(crate) mod profiling;
mod reload;
mod render;
mod request;
mod service_state;
mod theme;
mod wayland;

const MAX_SHELL_MESSAGE_DRAIN_PER_FRAME: usize = 256;
const MAX_IDLE_SLEEP: Duration = Duration::from_millis(16);
const MIN_RUNTIME_SLEEP: Duration = Duration::from_millis(1);

impl Shell {
    pub(in crate::shell) fn rebuild_component_surface_index(&mut self) {
        self.component_by_surface.clear();
        self.component_by_surface.reserve(self.components.len());
        for (index, runtime) in self.components.iter().enumerate() {
            self.component_by_surface
                .insert(runtime.surface_id.clone(), index);
        }
    }

    pub(in crate::shell) fn component_index_for_surface(
        &mut self,
        surface_id: &str,
    ) -> Option<usize> {
        if self.component_by_surface.len() != self.components.len() {
            self.rebuild_component_surface_index();
        }

        if let Some(index) = self.component_by_surface.get(surface_id).copied()
            && self
                .components
                .get(index)
                .is_some_and(|runtime| runtime.surface_id == surface_id)
        {
            return Some(index);
        }

        self.rebuild_component_surface_index();
        self.component_by_surface.get(surface_id).copied()
    }

    fn next_runtime_sleep(&self, shell_message_backlog_likely: bool) -> Duration {
        if shell_message_backlog_likely
            || !self.pending_wayland_events.is_empty()
            || self.components_have_ready_render_work()
        {
            return Duration::ZERO;
        }

        let now = std::time::Instant::now();
        if now >= self.next_frontend_reload_check
            || now >= self.next_module_settings_reload_check
            || now >= self.next_theme_reload_check
            || now >= self.next_shell_settings_reload_check
        {
            return Duration::ZERO;
        }

        let mut next_deadline = self
            .next_frontend_reload_check
            .min(self.next_module_settings_reload_check)
            .min(self.next_theme_reload_check)
            .min(self.next_shell_settings_reload_check);

        for state in self.command_throttle.values() {
            if state.pending.is_none() {
                continue;
            }
            let command_due_at = state
                .last_send
                .checked_add(request::COMMAND_THROTTLE_INTERVAL)
                .unwrap_or(now);
            if command_due_at <= now {
                return Duration::ZERO;
            }
            next_deadline = next_deadline.min(command_due_at);
        }

        for surface in self.core.surfaces.values() {
            let Some(closing_until) = surface.closing_until else {
                continue;
            };
            if closing_until <= now {
                return Duration::ZERO;
            }
            next_deadline = next_deadline.min(closing_until);
        }

        let sleep_for = next_deadline
            .saturating_duration_since(now)
            .min(MAX_IDLE_SLEEP);
        if sleep_for < MIN_RUNTIME_SLEEP {
            Duration::ZERO
        } else {
            sleep_for
        }
    }

    fn components_have_ready_render_work(&self) -> bool {
        self.components.iter().any(|runtime| {
            if !runtime.component.wants_render() {
                return false;
            }
            let surface_id = runtime.surface_id.as_str();
            let visible = self
                .core
                .surfaces
                .get(surface_id)
                .map(|state| state.visible)
                .or_else(|| self.surfaces.get(surface_id).map(|surface| surface.visible))
                .unwrap_or(true);

            !visible
                || !self
                    .presentation_engine
                    .surface_waiting_for_frame_callback(surface_id)
        })
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
        pending.extend(self.sync_locale_service_state()?);
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

            let mut shell_messages = CoalescedShellMessages::default();
            let mut drained_shell_message_count = 0;
            for _ in 0..MAX_SHELL_MESSAGE_DRAIN_PER_FRAME {
                let Ok(message) = rx.try_recv() else {
                    break;
                };
                drained_shell_message_count += 1;
                shell_messages.push(message);
            }
            let shell_message_backlog_likely =
                drained_shell_message_count == MAX_SHELL_MESSAGE_DRAIN_PER_FRAME;
            for message in shell_messages.into_vec() {
                self.handle_shell_message(&mut pending, message)?;
            }

            pending.extend(self.tick_components()?);
            pending.extend(self.complete_due_surface_transitions()?);
            self.drain_requests(&mut pending)?;
            self.flush_throttled_commands();
            self.render_components()?;
            self.flush_wayland()?;
            self.presentation_engine.pump();

            let sleep_for = self.next_runtime_sleep(shell_message_backlog_likely);
            if !sleep_for.is_zero() {
                std::thread::sleep(sleep_for);
            }
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

#[derive(Default)]
struct CoalescedShellMessages {
    messages: Vec<ShellMessage>,
    backend_update_index: HashMap<String, HashMap<String, usize>>,
}

impl CoalescedShellMessages {
    fn push(&mut self, message: ShellMessage) {
        if let ShellMessage::BackendServiceUpdate {
            interface,
            provider_id,
            ..
        } = &message
        {
            if let Some(index) = self
                .backend_update_index
                .get(interface.as_str())
                .and_then(|providers| providers.get(provider_id.as_str()))
                .copied()
            {
                self.messages[index] = message;
                return;
            }
            self.backend_update_index
                .entry(interface.clone())
                .or_default()
                .insert(provider_id.clone(), self.messages.len());
        }

        self.messages.push(message);
    }

    fn into_vec(self) -> Vec<ShellMessage> {
        self.messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend_update(interface: &str, provider_id: &str, value: i64) -> ShellMessage {
        ShellMessage::BackendServiceUpdate {
            interface: interface.to_string(),
            provider_id: provider_id.to_string(),
            event: ServiceEvent::Updated {
                service: interface.to_string(),
                source_module: provider_id.to_string(),
                payload: serde_json::json!({ "value": value }),
            },
        }
    }

    #[test]
    fn coalesced_shell_messages_keep_latest_backend_update_per_provider() {
        let mut coalesced = CoalescedShellMessages::default();
        coalesced.push(backend_update("mesh.audio", "@mesh/pipewire-audio", 1));
        coalesced.push(backend_update("mesh.audio", "@mesh/pipewire-audio", 2));
        coalesced.push(backend_update("mesh.audio", "@mesh/pulseaudio-audio", 3));

        let messages = coalesced.into_vec();
        assert_eq!(messages.len(), 2);
        let ShellMessage::BackendServiceUpdate { event, .. } = &messages[0] else {
            panic!("expected backend service update");
        };
        let ServiceEvent::Updated { payload, .. } = event else {
            panic!("expected service update event");
        };
        assert_eq!(
            payload.get("value").and_then(|value| value.as_i64()),
            Some(2)
        );
    }
}
