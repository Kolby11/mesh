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
    /// Presentation reports the compositor-facing buffer size. Parent layer
    /// surfaces may include transparent tooltip reserve in that size, while
    /// component layout and input always use the content rectangle.
    pub(in crate::shell) fn content_size_for_target(
        &self,
        index: usize,
        target: TargetRef,
        padded: (u32, u32),
    ) -> (u32, u32) {
        let padding = self.components[index]
            .target(target)
            .last_surface_config
            .as_ref()
            .map(|config| config.padding())
            .unwrap_or_default();
        (
            padded
                .0
                .saturating_sub(padding.left.saturating_add(padding.right))
                .max(1),
            padded
                .1
                .saturating_sub(padding.top.saturating_add(padding.bottom))
                .max(1),
        )
    }

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
        if self.pending_resource_preparation.is_some() {
            return Duration::from_millis(1);
        }
        if mesh_core_render::icon_resolution_jobs_pending()
            || mesh_core_render::icon_raster_jobs_pending()
            || mesh_core_render::glyph_raster_jobs_pending()
            || mesh_core_render::image_decode_jobs_pending()
        {
            return Duration::from_millis(1);
        }

        let now = std::time::Instant::now();
        if now >= self.next_frontend_reload_check
            || now >= self.next_theme_reload_check
            || now >= self.next_shell_settings_reload_check
        {
            return Duration::ZERO;
        }

        let mut next_deadline = self
            .next_frontend_reload_check
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

    pub(in crate::shell) fn components_have_ready_render_work(&self) -> bool {
        let resource_revision = mesh_core_resources::resource_revision();
        self.components.iter().any(|runtime| {
            let resource_revision_changed = runtime.targets().any(|target| {
                self.surface_is_effectively_visible(target.surface_id.as_str())
                    && target
                        .last_paint_resource_revision
                        .is_some_and(|seen| seen != resource_revision)
            });
            if !runtime.component.wants_render() && !resource_revision_changed {
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
                    && !self
                        .presentation_engine
                        .surface_waiting_for_buffer_release(surface_id)
                    && self
                        .presentation_engine
                        .surface_ready_to_present(surface_id)
            })
        })
    }

    pub fn run(&mut self) -> Result<(), ShellRunError> {
        if self.shutdown_complete {
            return Ok(());
        }
        self.discover_modules();
        if let Some(graph) = self.installed_module_graph.as_ref() {
            for descriptor in graph.theme_catalog().iter() {
                let source = descriptor.default_source();
                match mesh_core_theme::load_theme_from_source(source) {
                    Ok(mut theme) => {
                        // The graph identity, not CSS metadata, is the
                        // activation identity. CSS labels remain content,
                        // while ownership and mode selection come from the
                        // authorized descriptor.
                        theme.id = descriptor.id.clone();
                        if let Some(label) = &descriptor.label {
                            theme.name = label.clone();
                        }
                        tracing::debug!(
                            "registering graph-authorized theme '{}' mode '{}'",
                            descriptor.id,
                            descriptor.default_mode
                        );
                        if let Err(error) = self.theme.register_theme(theme) {
                            tracing::warn!(
                                "skipping graph-authorized theme '{}' due to duplicate identity: {error}",
                                descriptor.id
                            );
                        }
                    }
                    Err(error) => tracing::warn!(
                        "failed to load graph-authorized theme '{}' mode '{}': {error}",
                        descriptor.id,
                        descriptor.default_mode
                    ),
                }
            }
        }
        if let Some(graph) = self.installed_module_graph.clone()
            && !graph.theme_catalog().is_empty()
        {
            match prepare_theme_for_graph(&self.settings, &graph) {
                Ok((theme, watch)) => {
                    self.theme.replace_active(theme);
                    self.theme_watch = watch;
                }
                Err(error) => tracing::warn!(
                    "failed to compose selected graph theme '{}': {error}; retaining recovery theme",
                    self.settings.theme.active
                ),
            }
        }
        self.resolve_modules()?;
        self.load_frontend_components()?;

        let runtime = Runtime::new().map_err(ShellRunError::RuntimeInit)?;
        let (tx, mut rx) = mpsc::unbounded_channel::<ShellMessage>();

        let eventfd = rustix::event::eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)
            .map_err(|e| ShellRunError::EventfdCreate(format!("eventfd: {e}")))?;
        let eventfd_raw = eventfd.as_raw_fd();
        self.eventfd_fd = Some(eventfd);

        self.file_watcher =
            file_watch::spawn_file_watcher(self.file_watch_paths(), tx.clone(), eventfd_raw);
        self.file_watcher_active = self.file_watcher.is_some();
        self.backend_respawn = Some(backend::BackendRespawnContext {
            handle: runtime.handle().clone(),
            tx: tx.clone(),
            eventfd_fd: eventfd_raw,
        });
        self.spawn_backend_modules(runtime.handle(), tx.clone(), eventfd_raw);
        let ipc_socket_path = default_ipc_socket_path();
        let run_result = (|| -> Result<(), ShellRunError> {
            self.ipc_server = Some(
                spawn_ipc_server(&runtime, ipc_socket_path.clone(), tx, eventfd_raw).map_err(
                    |source| ShellRunError::IpcInit {
                        path: ipc_socket_path.clone(),
                        source,
                    },
                )?,
            );

            let mut pending = VecDeque::new();
            pending.extend(self.mount_components()?);
            pending.extend(self.replay_cached_service_events()?);
            pending.extend(self.sync_theme_service_state()?);
            pending.extend(self.mark_components_locale_changed()?);
            pending.extend(self.sync_locale_service_state()?);
            pending.extend(self.broadcast_core_event(CoreEvent::Started)?);
            if let Some(request) = shell_sound_request(SoundKind::Startup, &self.settings.sounds) {
                pending.push_back(request);
            }

            tracing::info!(
                "MESH shell core is running with {} frontend component(s)",
                self.components.len()
            );

            while !self.core.shutting_down {
                pending.extend(self.reload_theme_if_changed()?);
                pending.extend(self.reload_locale_if_settings_changed()?);
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
                pending.extend(std::mem::take(&mut self.deferred_requests));
                pending.extend(self.complete_due_surface_transitions()?);
                if !pending.is_empty() {
                    self.presented_last_frame = true;
                }
                self.drain_requests(&mut pending)?;
                pending.extend(self.poll_pending_resource_preparation());
                self.drain_requests(&mut pending)?;
                self.flush_throttled_commands();
                self.render_components()?;
                self.presentation_engine
                    .finish_frame()
                    .map_err(ShellRunError::Presentation)?;
                self.flush_wayland()?;

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
            Ok(())
        })();

        let shutdown_result = self.shutdown_runtime(&runtime, &ipc_socket_path);
        match (run_result, shutdown_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    /// Execute the one teardown path for normal shutdown and every setup or
    /// event-loop error. Each phase is safe to repeat, and later phases still
    /// run after an earlier phase reports an error.
    fn shutdown_runtime(
        &mut self,
        runtime: &Runtime,
        ipc_socket_path: &std::path::Path,
    ) -> Result<(), ShellRunError> {
        if self.shutdown_complete {
            return Ok(());
        }
        self.shutdown_started = true;
        self.core.shutting_down = true;
        self.backend_respawn = None;
        for task in self.backend_restart_tasks.drain(..) {
            task.abort();
        }
        for state in self.backend_supervision.values_mut() {
            state.invalidate_pending_restart();
        }

        if let Some(ipc_server) = self.ipc_server.take() {
            runtime.block_on(ipc_server.shutdown());
        }
        if let Some(file_watcher) = self.file_watcher.take() {
            file_watcher.stop_and_join();
        }
        self.file_watcher_active = false;

        let mut first_error = None;
        let mut shutdown_requests = match self.broadcast_core_event(CoreEvent::ShuttingDown) {
            Ok(requests) => requests,
            Err(error) => {
                first_error = Some(error);
                VecDeque::new()
            }
        };
        if let Err(error) = self.drain_requests(&mut shutdown_requests) {
            first_error.get_or_insert(error);
        }
        let mut unmount_requests = self.unmount_components();
        if let Err(error) = self.drain_requests(&mut unmount_requests) {
            first_error.get_or_insert(error);
        }

        if let Some(mut pending) = self.pending_resource_preparation.take() {
            pending.resource_job.cancel();
            if let Err(error) = pending.resource_job.wait() {
                tracing::warn!("resource preparation cleanup failed: {error}");
            }
            if let Some(rollback) = pending.rollback {
                let _ = rollback.restore();
            }
        }
        self.shutdown_backend_runtimes(runtime);
        self.pending_profile_switch = None;
        self.backend_supervision.clear();
        self.eventfd_fd.take();
        let _ = std::fs::remove_file(ipc_socket_path);
        self.shutdown_complete = true;
        tracing::info!("shell event loop stopped");
        first_error.map_or(Ok(()), Err)
    }

    pub(in crate::shell) fn handle_shell_message(
        &mut self,
        pending: &mut VecDeque<CoreRequest>,
        message: ShellMessage,
    ) -> Result<(), ShellRunError> {
        let message_started = self.profiling_enabled().then(std::time::Instant::now);
        let trigger_kind = match &message {
            ShellMessage::BackendServiceUpdate { .. } => "backend_service_update",
            ShellMessage::BackendLifecycle { .. } => "backend_lifecycle",
            ShellMessage::BackendCommandResult { .. } => "backend_command_result",
            ShellMessage::BackendInterfaceEvent { .. } => "backend_interface_event",
            ShellMessage::BackendRestartDue { .. } => "backend_restart_due",
            ShellMessage::FilesystemChanged => "filesystem_changed",
            ShellMessage::FileWatcherStopped => "file_watcher_stopped",
            ShellMessage::Ipc(_) => "ipc",
        };
        match message {
            ShellMessage::BackendServiceUpdate {
                interface,
                provider_id,
                identity,
                event,
            } => {
                if self.capture_pending_backend_update_at_identity(
                    &interface,
                    &provider_id,
                    identity,
                    event.clone(),
                ) || self.capture_profile_backend_update_at_identity(
                    &interface,
                    &provider_id,
                    identity,
                    event.clone(),
                ) {
                    return Ok(());
                }
                let provider_is_active = if identity == BackendIdentity::default() {
                    self.backend_provider_is_active(&interface, &provider_id)
                } else {
                    self.backend_provider_is_active_at_identity(&interface, &provider_id, identity)
                };
                if !provider_is_active {
                    tracing::debug!(
                        interface,
                        provider_id,
                        "ignored service update from a prepared or obsolete provider"
                    );
                    return Ok(());
                }
                let profiling_started = self.profiling_enabled().then(std::time::Instant::now);
                let event = self.normalize_service_event(event);
                if self.record_latest_service_state_at_identity(&event, identity) {
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
                identity,
                stage,
                status,
                message,
            } => self.handle_backend_lifecycle_at_identity(
                interface,
                provider_id,
                identity,
                stage,
                status,
                message,
            ),
            ShellMessage::BackendCommandResult {
                interface,
                provider_id,
                identity,
                generation,
                call_id,
                command,
                result,
                outcome,
            } => {
                let provider_is_active = (if identity == BackendIdentity::default() {
                    self.backend_provider_is_active(&interface, &provider_id)
                } else {
                    self.backend_provider_is_active_at_identity(&interface, &provider_id, identity)
                }) && self
                    .backend_runtimes
                    .get(&interface)
                    .is_some_and(|slot| slot.generation == generation);
                if provider_is_active {
                    let contract = self.interfaces.resolve(&interface, None).contract;
                    let warnings = contract.as_ref().map_or_else(Vec::new, |contract| {
                        service_state::service_method_result_contract_warnings(
                            contract, &command, &result,
                        )
                    });
                    if warnings.is_empty() {
                        self.record_backend_method_result(
                            interface,
                            provider_id,
                            call_id,
                            command,
                            result.clone(),
                            outcome,
                        );
                        self.complete_service_call_route(call_id, outcome.as_str(), &result);
                    } else {
                        let message = warnings.join("; ");
                        tracing::warn!(
                            interface,
                            provider_id,
                            command,
                            error = %message,
                            "rejected service command result with invalid contract payload"
                        );
                        self.diagnostics.record_lifecycle_error(
                            provider_id.clone(),
                            "invalid_service_command_result",
                            message.clone(),
                        );
                        let invalid_result = serde_json::json!({
                            "ok": false,
                            "status": "invalid_service_command_result",
                            "error": message,
                        });
                        self.record_backend_method_result(
                            interface,
                            provider_id,
                            call_id,
                            command,
                            invalid_result.clone(),
                            mesh_core_backend::BackendCommandOutcome::Failed,
                        );
                        self.complete_service_call_route(
                            call_id,
                            "invalid_service_command_result",
                            &invalid_result,
                        );
                    }
                } else {
                    let stale_status = if self
                        .backend_runtimes
                        .get(&interface)
                        .is_some_and(|slot| slot.generation != generation)
                    {
                        "stale_generation"
                    } else if self.backend_runtimes.get(&interface).is_some_and(|slot| {
                        slot.identity
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .activation_generation
                            != identity.activation_generation
                    }) {
                        "stale_activation_generation"
                    } else if self.backend_runtimes.get(&interface).is_some_and(|slot| {
                        slot.identity
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .provider_epoch
                            != identity.provider_epoch
                    }) {
                        "stale_provider_epoch"
                    } else {
                        "stale_provider"
                    };
                    self.complete_service_call_route(
                        call_id,
                        stale_status,
                        &serde_json::json!({
                            "ok": false,
                            "status": stale_status,
                            "error": "backend activation or provider epoch is no longer active",
                        }),
                    );
                }
            }
            ShellMessage::BackendInterfaceEvent {
                interface,
                provider_id,
                identity,
                name,
                payload,
                generation,
            } => {
                pending.extend(self.broadcast_backend_interface_event_at_identity(
                    interface,
                    provider_id,
                    identity,
                    name,
                    payload,
                    generation,
                )?);
            }
            ShellMessage::BackendRestartDue {
                interface,
                provider_id,
                identity,
                restart_generation,
            } => {
                self.handle_backend_restart_due_at_identity(
                    &interface,
                    &provider_id,
                    identity,
                    restart_generation,
                );
            }
            ShellMessage::FilesystemChanged => {
                self.schedule_reload_checks_now();
                pending.extend(self.reconcile_installed_graph());
            }
            ShellMessage::FileWatcherStopped => {
                // Reload checks were parked at FILE_WATCHER_RELOAD_PARK
                // (24h) on the assumption inotify would report every
                // change. With the thread gone that assumption is false;
                // fall back to short-interval polling starting now instead
                // of leaving the shell blind until the park expires.
                if self.file_watcher_active {
                    tracing::warn!(
                        "file watcher stopped; falling back to short-interval reload polling"
                    );
                    self.file_watcher_active = false;
                    self.schedule_reload_checks_now();
                }
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
        // Graph and profile edits are activation inputs, not just ordinary
        // source reloads. Watching their containing directories also catches
        // atomic replacement of the graph/profile files and creation of a
        // newly installed module directory.
        paths.push(self.installed_module_graph_path());
        paths.extend(self.module_dirs.iter().cloned());
        paths.extend(self.modules.values().map(|module| module.path.clone()));
        for runtime in &self.components {
            paths.extend(runtime.source_paths.iter().map(|(path, _)| path.clone()));
        }
        paths
    }

    fn schedule_reload_checks_now(&mut self) {
        let now = std::time::Instant::now();
        self.next_theme_reload_check = now;
        self.next_shell_settings_reload_check = now;
        self.next_frontend_reload_check = now;
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
            identity,
            ..
        } = &message
        {
            let provider_key = format!("{provider_id}:{identity:?}");
            if let Some(index) = self
                .backend_update_index
                .get(interface.as_str())
                .and_then(|providers| providers.get(&provider_key))
                .copied()
            {
                self.messages[index] = message;
                return;
            }
            self.backend_update_index
                .entry(interface.clone())
                .or_default()
                .insert(provider_key, self.messages.len());
        } else {
            // Lifecycle, command-result, named interface-event, IPC, and
            // filesystem messages are ordering barriers. Updates after one
            // must not replace a slot before it, otherwise consumers can see
            // the newest state before an event that was emitted against the
            // older state.
            self.backend_update_index.clear();
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
            identity: BackendIdentity::default(),
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

    #[test]
    fn named_interface_event_is_a_backend_update_coalescing_barrier() {
        let mut coalesced = CoalescedShellMessages::default();
        coalesced.push(backend_update("mesh.audio", "@mesh/pipewire-audio", 1));
        coalesced.push(ShellMessage::BackendInterfaceEvent {
            interface: "mesh.audio".to_string(),
            provider_id: "@mesh/pipewire-audio".to_string(),
            identity: BackendIdentity::default(),
            name: "VolumeChanged".to_string(),
            payload: serde_json::json!({ "value": 1 }),
            generation: 0,
        });
        coalesced.push(backend_update("mesh.audio", "@mesh/pipewire-audio", 2));

        let messages = coalesced.into_vec();
        assert_eq!(messages.len(), 3);
        assert!(matches!(
            messages[0],
            ShellMessage::BackendServiceUpdate { .. }
        ));
        assert!(matches!(
            messages[1],
            ShellMessage::BackendInterfaceEvent { .. }
        ));
        assert!(matches!(
            messages[2],
            ShellMessage::BackendServiceUpdate { .. }
        ));
    }

    #[test]
    fn backend_updates_still_coalesce_independently_before_a_barrier() {
        let mut coalesced = CoalescedShellMessages::default();
        for value in 0..1_000 {
            coalesced.push(backend_update("mesh.audio", "@mesh/pipewire-audio", value));
        }

        let messages = coalesced.into_vec();
        assert_eq!(messages.len(), 1, "999 redundant deliveries are removed");
        let ShellMessage::BackendServiceUpdate { event, .. } = &messages[0] else {
            panic!("expected backend update");
        };
        let ServiceEvent::Updated { payload, .. } = event else {
            panic!("expected state update");
        };
        assert_eq!(payload["value"], serde_json::json!(999));
    }

    // cargo test -p mesh-core-shell --release -- wake_coalescing_beats_repeated_shell_delivery --ignored --nocapture
    #[test]
    #[ignore = "release-only service-update coalescing microbenchmark"]
    fn wake_coalescing_beats_repeated_shell_delivery() {
        use std::time::Instant;

        let updates = 10_000;
        let mut baseline_shell = Shell::new();
        let mut baseline_pending = VecDeque::new();
        let baseline_started = Instant::now();
        for value in 0..updates {
            baseline_shell
                .handle_shell_message(
                    &mut baseline_pending,
                    backend_update("mesh.audio", "@mesh/pipewire-audio", value),
                )
                .unwrap();
        }
        let baseline = baseline_started.elapsed();

        let mut optimized_shell = Shell::new();
        let mut optimized_pending = VecDeque::new();
        let optimized_started = Instant::now();
        let mut coalesced = CoalescedShellMessages::default();
        for value in 0..updates {
            coalesced.push(backend_update("mesh.audio", "@mesh/pipewire-audio", value));
        }
        for message in coalesced.into_vec() {
            optimized_shell
                .handle_shell_message(&mut optimized_pending, message)
                .unwrap();
        }
        let optimized = optimized_started.elapsed();

        eprintln!(
            "10k repeated deliveries: {baseline:?}; wake coalescing + one delivery: {optimized:?}; ratio: {:.1}x",
            baseline.as_secs_f64() / optimized.as_secs_f64()
        );
        assert!(optimized < baseline);
        assert_eq!(
            baseline_shell.latest_service_state["mesh.audio"].state,
            optimized_shell.latest_service_state["mesh.audio"].state
        );
    }
}
