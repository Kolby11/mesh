use super::*;

use rustix::event::EventfdFlags;
use std::os::unix::io::{AsFd, AsRawFd};

mod debug;
pub(crate) mod profiling;
mod reload;
mod render;
mod request;
mod service_state;
mod theme;
mod wayland;

const MAX_SHELL_MESSAGE_DRAIN_PER_FRAME: usize = 256;
const DEV_WINDOW_POLL_SLEEP: Duration = Duration::from_millis(16);
pub(in crate::shell) const FILE_WATCHER_RELOAD_PARK: Duration = Duration::from_secs(24 * 60 * 60);

impl Shell {
    fn surface_is_effectively_visible(&self, surface_id: &str) -> bool {
        self.core
            .surfaces
            .get(surface_id)
            .map(|state| state.visible)
            .or_else(|| self.surfaces.get(surface_id).map(|surface| surface.visible))
            .unwrap_or(true)
    }

    pub(in crate::shell) fn rebuild_component_surface_index(&mut self) {
        self.component_by_surface.clear();
        // A component owns its parent surface plus any auto-derived child
        // surfaces, so map *every* target's surface id back to the component.
        for (index, runtime) in self.components.iter().enumerate() {
            for target in runtime.targets() {
                self.component_by_surface
                    .insert(target.surface_id.clone(), index);
            }
        }
    }

    /// Resolve a surface id to the owning component and which of its surface
    /// targets (parent or a child popup) it refers to. Rebuilds the index
    /// lazily on a miss or a stale mapping (e.g. after hot reload or after a
    /// child surface was added/removed), so the map may hold more entries than
    /// there are components.
    pub(in crate::shell) fn component_target_for_surface(
        &mut self,
        surface_id: &str,
    ) -> Option<(usize, TargetRef)> {
        if let Some(index) = self.component_by_surface.get(surface_id).copied()
            && let Some(target) = self
                .components
                .get(index)
                .and_then(|runtime| runtime.target_ref_for_surface(surface_id))
        {
            return Some((index, target));
        }

        self.rebuild_component_surface_index();
        let index = self.component_by_surface.get(surface_id).copied()?;
        let target = self
            .components
            .get(index)?
            .target_ref_for_surface(surface_id)?;
        Some((index, target))
    }

    pub(in crate::shell) fn component_index_for_surface(
        &mut self,
        surface_id: &str,
    ) -> Option<usize> {
        self.component_target_for_surface(surface_id)
            .map(|(index, _)| index)
    }

    pub(in crate::shell) fn next_runtime_sleep(
        &self,
        shell_message_backlog_likely: bool,
    ) -> Duration {
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

        for hide_at in self.pending_popover_hides.values() {
            if *hide_at <= now {
                return Duration::ZERO;
            }
            next_deadline = next_deadline.min(*hide_at);
        }

        for runtime in &self.components {
            if !self.surface_is_effectively_visible(runtime.surface_id.as_str()) {
                continue;
            }
            if !runtime.component.wants_tick() {
                continue;
            }
            let Some(tick_deadline) = runtime.component.next_tick_deadline() else {
                continue;
            };
            if tick_deadline <= now {
                return Duration::ZERO;
            }
            next_deadline = next_deadline.min(tick_deadline);
        }

        let sleep_for = next_deadline.saturating_duration_since(now);
        sleep_for
    }

    fn components_have_ready_render_work(&self) -> bool {
        if !self.presented_last_frame {
            return false;
        }
        self.components.iter().any(|runtime| {
            if !runtime.component.wants_render() {
                return false;
            }
            // A component drives its parent surface plus any child popups from
            // one VM; it has ready work if any of its targets can present now.
            runtime.targets().any(|target| {
                let surface_id = target.surface_id.as_str();
                self.surface_is_effectively_visible(surface_id)
                    && !self
                        .presentation_engine
                        .surface_waiting_for_frame_callback(surface_id)
            })
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

        let eventfd = rustix::event::eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)
            .map_err(|e| ShellRunError::EventfdCreate(format!("eventfd: {e}")))?;
        let eventfd_raw = eventfd.as_raw_fd();
        self.eventfd_fd = Some(eventfd);

        self.file_watcher_active =
            file_watch::spawn_file_watcher(self.file_watch_paths(), tx.clone(), eventfd_raw);
        self.spawn_backend_modules(&runtime, tx.clone(), eventfd_raw);
        let ipc_socket_path = default_ipc_socket_path();
        spawn_ipc_server(&runtime, ipc_socket_path.clone(), tx, eventfd_raw).map_err(|source| {
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
            if drained_shell_message_count > 0 {
                self.presented_last_frame = true;
            }
            for message in shell_messages.into_vec() {
                self.handle_shell_message(&mut pending, message)?;
            }

            pending.extend(self.tick_components()?);
            pending.extend(self.complete_due_surface_transitions()?);
            if !pending.is_empty() {
                self.presented_last_frame = true;
            }
            self.drain_requests(&mut pending)?;
            self.flush_throttled_commands();
            self.render_components()?;
            self.flush_wayland()?;
            self.presentation_engine.pump();

            let deadline = self.next_runtime_sleep(shell_message_backlog_likely);
            if self.presentation_engine.supports_blocking_dispatch() {
                let wait_started = self.profiling_enabled().then(std::time::Instant::now);
                let eventfd_borrowed = self
                    .eventfd_fd
                    .as_ref()
                    .expect("eventfd must be created before shell loop")
                    .as_fd();
                let result = self
                    .presentation_engine
                    .wait_for_events(deadline, eventfd_borrowed)
                    .map_err(ShellRunError::Presentation)?;
                if let Some(started) = wait_started {
                    self.record_shell_profiling_stage(
                        mesh_core_debug::ProfilingStage::SchedulerIdle,
                        started.elapsed(),
                        Some(result.reason.as_str()),
                    );
                }
            } else {
                let sleep_for = if deadline.is_zero() {
                    DEV_WINDOW_POLL_SLEEP
                } else if self.presentation_engine.needs_polling_dispatch() {
                    deadline.min(DEV_WINDOW_POLL_SLEEP)
                } else {
                    deadline
                };
                let eventfd_borrowed = self
                    .eventfd_fd
                    .as_ref()
                    .expect("eventfd must be created before shell loop")
                    .as_fd();
                wait_for_eventfd(sleep_for, eventfd_borrowed);
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
            ShellMessage::FilesystemChanged => "filesystem_changed",
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
            ShellMessage::FilesystemChanged => {
                self.schedule_reload_checks_now();
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

    fn file_watch_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        paths.push(self.theme_watch.path.clone());
        paths.push(self.settings_watch.path.clone());
        for runtime in &self.components {
            paths.extend(runtime.source_paths.iter().map(|(path, _)| path.clone()));
            if let Some(path) = &runtime.module_settings_path {
                paths.push(path.clone());
            }
        }
        paths
    }

    fn schedule_reload_checks_now(&mut self) {
        let now = std::time::Instant::now();
        self.next_theme_reload_check = now;
        self.next_shell_settings_reload_check = now;
        self.next_frontend_reload_check = now;
        self.next_module_settings_reload_check = now;
    }
}

fn wait_for_eventfd(timeout: Duration, eventfd_fd: std::os::unix::io::BorrowedFd<'_>) {
    use rustix::event::{PollFd, PollFlags, poll};
    use rustix::io::read as eventfd_read;

    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    let mut fds = [PollFd::new(
        &eventfd_fd,
        PollFlags::IN | PollFlags::ERR | PollFlags::HUP,
    )];
    let ready = match poll(&mut fds, timeout_ms) {
        Ok(0) | Err(rustix::io::Errno::INTR) => false,
        Ok(_) => fds[0]
            .revents()
            .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
        Err(err) => {
            tracing::warn!("eventfd wait failed: {err}");
            false
        }
    };
    if ready {
        let mut counter = [0u8; 8];
        let _ = eventfd_read(&eventfd_fd, &mut counter);
    }
}

#[derive(Default)]
struct CoalescedShellMessages {
    messages: Vec<ShellMessage>,
    backend_update_index: HashMap<String, HashMap<String, usize>>,
    has_filesystem_changed: bool,
}

impl CoalescedShellMessages {
    fn push(&mut self, message: ShellMessage) {
        if matches!(message, ShellMessage::FilesystemChanged) {
            if self.has_filesystem_changed {
                return;
            }
            self.has_filesystem_changed = true;
        }

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

    #[test]
    fn coalesced_shell_messages_keep_single_filesystem_change() {
        let mut coalesced = CoalescedShellMessages::default();
        coalesced.push(ShellMessage::FilesystemChanged);
        coalesced.push(ShellMessage::FilesystemChanged);

        let messages = coalesced.into_vec();
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0], ShellMessage::FilesystemChanged));
    }
}
