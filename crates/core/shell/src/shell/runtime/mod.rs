use super::types::COMPONENT_FAILURES_BEFORE_QUARANTINE;
use super::*;

use rustix::event::EventfdFlags;
use std::os::unix::io::AsFd;

mod debug;
pub(crate) mod profiling;
mod reload;
mod render;
mod request;
mod service_state;
mod theme;
mod wayland;

pub(in crate::shell) use request::EffectScheduler;
pub(in crate::shell) use theme::ControlPlaneSettingsCommit;

const MAX_SHELL_MESSAGE_DRAIN_PER_FRAME: usize = 256;
const DEV_WINDOW_POLL_SLEEP: Duration = Duration::from_millis(16);

impl Shell {
    /// Contain one component's failure at the shell boundary. Component
    /// effects are only appended after a successful callback, so an error
    /// discards the failed batch while sibling components continue running.
    pub(in crate::shell) fn contain_component_failure(
        &mut self,
        index: usize,
        phase: &str,
        error: impl std::fmt::Display,
    ) {
        let message = error.to_string();
        let Some((module_id, instance_id, was_quarantined, newly_quarantined, component_recorded)) =
            self.components.get_mut(index).map(|runtime| {
                let module_id = runtime.component.id().to_string();
                let instance_id = runtime.surface_id.clone();
                let was_quarantined = runtime.quarantined;
                let newly_quarantined = runtime.note_failure();
                let component_recorded = runtime.component.isolate_runtime_failure(phase, &message);
                (
                    module_id,
                    instance_id,
                    was_quarantined,
                    newly_quarantined,
                    component_recorded,
                )
            })
        else {
            return;
        };

        if !component_recorded {
            self.diagnostics.record_component_runtime_error(
                module_id.clone(),
                instance_id.clone(),
                phase,
                message.clone(),
            );
        }
        if let Some(module) = self.modules.get_mut(&module_id) {
            if !was_quarantined {
                let _ = module.mark_failed(message.clone());
            }
            if newly_quarantined {
                let quarantine_message = format!(
                    "component instance '{instance_id}' failed {COMPONENT_FAILURES_BEFORE_QUARANTINE} supervised runtime operations; quarantined until source or activation recovery"
                );
                let _ = module.mark_quarantined(quarantine_message.clone());
                self.diagnostics.record_component_runtime_error(
                    module_id.clone(),
                    instance_id.clone(),
                    "quarantine",
                    quarantine_message,
                );
            }
        } else if newly_quarantined {
            let quarantine_message = format!(
                "component instance '{instance_id}' failed {COMPONENT_FAILURES_BEFORE_QUARANTINE} supervised runtime operations; quarantined until source or activation recovery"
            );
            self.diagnostics.record_component_runtime_error(
                module_id.clone(),
                instance_id.clone(),
                "quarantine",
                quarantine_message,
            );
        }

        if newly_quarantined {
            tracing::warn!(
                component_id = %module_id,
                instance_id = %instance_id,
                "quarantined frontend component after repeated runtime failures"
            );
            self.destroy_all_child_surfaces(index);
            self.components[index].parent.force_full_present = true;
        }
    }

    pub(in crate::shell) fn clear_component_failure(&mut self, index: usize) {
        let Some(runtime) = self.components.get_mut(index) else {
            return;
        };
        let module_id = runtime.component.id().to_string();
        let was_unhealthy = runtime.quarantined || runtime.failure_count != 0;
        runtime.clear_failure_state();
        runtime.component.clear_runtime_failure();
        if was_unhealthy {
            let another_instance_unhealthy = self.components.iter().any(|other| {
                other.component.id() == module_id && (other.failure_count != 0 || other.quarantined)
            });
            if !another_instance_unhealthy && let Some(module) = self.modules.get_mut(&module_id) {
                module.clear_quarantine();
                let _ = module.mark_running();
            }
            tracing::info!(component_id = %module_id, "component runtime recovered");
        }
    }

    pub(in crate::shell) fn component_is_quarantined(&self, index: usize) -> bool {
        self.components
            .get(index)
            .is_some_and(|runtime| runtime.quarantined)
    }

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
            if runtime.quarantined || !runtime.component.wants_tick() {
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
        if self.composition_mode.is_recovery() {
            let reason = self
                .composition_mode
                .recovery_reason()
                .unwrap_or("configured shell state is invalid");
            tracing::error!(
                "starting explicit shell recovery without configured modules: {reason}"
            );
        } else {
            if let Some(graph) = self.installed_module_graph.as_ref() {
                match prepare_theme_state_for_graph(&self.settings, graph) {
                    Ok(Some(prepared)) => {
                        self.install_prepared_theme(prepared);
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!(
                        "failed to compose selected graph theme '{}': {error}; retaining recovery theme",
                        self.settings.theme.active
                    ),
                }
            }
            if let Err(error) = self
                .resolve_modules()
                .and_then(|_| self.load_frontend_components())
            {
                let message = format!(
                    "configured shell graph/profile could not prepare an active composition: {error}"
                );
                tracing::error!("{message}");
                self.enter_composition_recovery(message);
            }
        }

        let runtime = Runtime::new().map_err(ShellRunError::RuntimeInit)?;
        let (tx, mut rx) = mpsc::unbounded_channel::<ShellMessage>();

        let eventfd = rustix::event::eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)
            .map_err(|e| ShellRunError::EventfdCreate(format!("eventfd: {e}")))?;
        let wake = WakeHandle::from_fd(&eventfd)
            .map_err(|e| ShellRunError::EventfdCreate(format!("eventfd wake handle: {e}")))?;
        self.eventfd_fd = Some(eventfd);
        self.wake_handle = Some(wake.clone());

        self.file_watcher_tx = Some(tx.clone());
        self.reconcile_file_watcher();
        self.backend_respawn = Some(backend::BackendRespawnContext {
            handle: runtime.handle().clone(),
            tx: tx.clone(),
            wake: wake.clone(),
        });
        self.spawn_backend_modules(runtime.handle(), tx.clone(), wake.clone());
        let ipc_socket_path = default_ipc_socket_path();
        let run_result = (|| -> Result<(), ShellRunError> {
            self.ipc_server = Some(
                spawn_ipc_server(&runtime, ipc_socket_path.clone(), tx, wake.clone()).map_err(
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
            self.enqueue_effects(std::mem::take(&mut pending));

            tracing::info!(
                "MESH shell core is running with {} frontend component(s)",
                self.components.len()
            );

            while !self.core.shutting_down {
                self.reconcile_file_watcher();
                pending.extend(self.reload_theme_if_changed()?);
                pending.extend(self.reload_locale_if_settings_changed()?);
                self.reload_frontend_components_if_changed()?;
                self.dispatch_wayland_inner()?;

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
                pending.extend(self.poll_pending_resource_preparation());
                if !pending.is_empty() {
                    self.presented_last_frame = true;
                }
                self.enqueue_effects(std::mem::take(&mut pending));
                self.process_effects()?;
                self.flush_throttled_commands();
                self.render_components_inner()?;
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
        if self.shutdown_complete || self.shutdown_phase.is_stopped() {
            return Ok(());
        }
        self.begin_shutdown();
        let mut first_error = None;
        let discarded_before_shutdown = self.discard_scheduled_effects();

        self.advance_shutdown_phase(ShellShutdownPhase::StoppingComponents);
        self.shutdown_effects_allowed = true;
        let shutdown_requests = match self.broadcast_core_event(CoreEvent::ShuttingDown) {
            Ok(requests) => requests,
            Err(error) => {
                first_error = Some(error);
                VecDeque::new()
            }
        };
        self.enqueue_effects(shutdown_requests);
        let unmount_requests = self.unmount_components();
        self.enqueue_effects(unmount_requests);
        if let Err(error) = self.process_effects() {
            first_error.get_or_insert(error);
        }
        self.shutdown_effects_allowed = false;

        if let Some(mut pending) = self.pending_resource_preparation.take() {
            pending.resource_job.cancel();
            if let Err(error) = pending.resource_job.wait() {
                tracing::warn!("resource preparation cleanup failed: {error}");
            }
            if let Some(rollback) = pending.rollback {
                let _ = rollback.restore();
            }
            super::profile::abort_package_transaction(
                pending.package_transaction,
                pending.package_rollback,
                self,
            );
        }
        if let Some(pending) = self.pending_profile_switch.take() {
            self.abort_profile_candidate(pending, "shell runtime is shutting down".into());
        }

        self.advance_shutdown_phase(ShellShutdownPhase::StoppingProviders);
        self.shutdown_backend_runtimes(runtime);
        self.backend_supervision.clear();

        self.advance_shutdown_phase(ShellShutdownPhase::DestroyingPresentation);
        self.destroy_presentation_surfaces();

        self.advance_shutdown_phase(ShellShutdownPhase::StoppingWorkers);
        self.backend_respawn = None;
        let restart_tasks = self.backend_restart_tasks.drain(..).collect::<Vec<_>>();
        for task in &restart_tasks {
            task.abort();
        }
        for state in self.backend_supervision.values_mut() {
            state.invalidate_pending_restart();
        }
        if !restart_tasks.is_empty() {
            runtime.block_on(async move {
                for task in restart_tasks {
                    let _ = task.await;
                }
            });
        }
        if let Some(ipc_server) = self.ipc_server.take() {
            runtime.block_on(ipc_server.shutdown());
        }
        if let Some(file_watcher) = self.file_watcher.take() {
            file_watcher.stop_and_join();
        }
        self.file_watcher_tx = None;
        self.file_watcher_active = false;

        self.advance_shutdown_phase(ShellShutdownPhase::Flushing);
        let dropped_effects =
            discarded_before_shutdown.saturating_add(self.discard_scheduled_effects());
        if dropped_effects > 0 {
            self.diagnostics.record_lifecycle_error(
                "@mesh/shell",
                "shutdown_effects_discarded",
                format!(
                    "discarded {dropped_effects} shell effects after shutdown entered quiescing"
                ),
            );
        }
        self.pending_service_call_routes.clear();
        self.pending_bound_service_state.clear();
        self.bound_service_state_transactions.clear();
        self.command_throttle.clear();
        self.candidate_preview = None;
        self.wake_handle.take();
        self.eventfd_fd.take();
        let _ = std::fs::remove_file(ipc_socket_path);
        self.advance_shutdown_phase(ShellShutdownPhase::Stopped);
        self.shutdown_complete = true;
        tracing::info!("shell event loop stopped");
        first_error.map_or(Ok(()), Err)
    }

    fn destroy_presentation_surfaces(&mut self) {
        for index in (0..self.components.len()).rev() {
            self.destroy_all_child_surfaces(index);
            let surface_id = self.components[index].surface_id.clone();
            let module_id = self.components[index].component.id().to_string();
            self.presentation_engine.destroy_surface(&surface_id);
            self.diagnostics.unregister(&module_id, &surface_id);
            self.core.surfaces.remove(&surface_id);
            self.surfaces.remove(&surface_id);
            self.pending_popover_hides.remove(&surface_id);
            self.transfer_owned_keyboard_modes.remove(&surface_id);
        }
        for surface_id in self.core.surfaces.keys().cloned().collect::<Vec<_>>() {
            self.presentation_engine.destroy_surface(&surface_id);
        }
        self.components.clear();
        self.component_by_surface.clear();
        self.core.surfaces.clear();
        self.surfaces.clear();
        self.pending_popover_hides.clear();
        self.transfer_owned_keyboard_modes.clear();
        self.keyboard_focus_surface = None;
        self.service_delivery_index.mark_dirty();
    }

    pub(in crate::shell) fn handle_shell_message(
        &mut self,
        pending: &mut VecDeque<CoreRequest>,
        message: ShellMessage,
    ) -> Result<(), ShellRunError> {
        if !self.shutdown_phase.accepts_external_work() {
            tracing::debug!(phase = ?self.shutdown_phase, "rejected shell work after shutdown quiescing");
            if let ShellMessage::IpcProfileSwitch {
                profile_id,
                response,
            } = message
            {
                let _ = response.send(IpcProfileSwitchResponse::Rejected {
                    profile_id,
                    generation: self.activation_generation,
                    reason: format!("shell is shutting down ({:?})", self.shutdown_phase),
                });
            }
            return Ok(());
        }
        let message_started = self.profiling_enabled().then(std::time::Instant::now);
        let trigger_kind = match &message {
            ShellMessage::BackendServiceUpdate { .. } => "backend_service_update",
            ShellMessage::BackendLifecycle { .. } => "backend_lifecycle",
            ShellMessage::BackendCommandResult { .. } => "backend_command_result",
            ShellMessage::BackendInterfaceEvent { .. } => "backend_interface_event",
            ShellMessage::BackendRestartDue { .. } => "backend_restart_due",
            ShellMessage::FilesystemChanged { .. } => "filesystem_changed",
            ShellMessage::FileWatcherStatus { .. } => "file_watcher_status",
            ShellMessage::FileWatcherStopped { .. } => "file_watcher_stopped",
            ShellMessage::Ipc(_) => "ipc",
            ShellMessage::IpcProfileSwitch { .. } => "ipc_profile_switch",
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
            ShellMessage::FilesystemChanged { generation } => {
                if generation != self.file_watch_set.generation {
                    tracing::debug!(
                        generation,
                        current_generation = self.file_watch_set.generation,
                        "ignored filesystem event from retired watch generation"
                    );
                } else {
                    self.schedule_reload_checks_now();
                    pending.extend(self.reconcile_installed_graph());
                }
            }
            ShellMessage::FileWatcherStatus {
                generation,
                active,
                watched_paths,
            } => {
                if generation == self.file_watch_set.generation {
                    self.file_watcher_active = active;
                    if active {
                        self.diagnostics
                            .resolve_lifecycle_error("@mesh/shell", "file_watcher");
                        tracing::debug!(generation, watched_paths, "file watcher is healthy");
                    } else {
                        let message = format!(
                            "watch generation {generation} has no existing directories; bounded metadata polling is active"
                        );
                        self.diagnostics.record_lifecycle_error(
                            "@mesh/shell",
                            "file_watcher",
                            message,
                        );
                        tracing::warn!(generation, "file watcher has no active directories");
                    }
                }
            }
            ShellMessage::FileWatcherStopped { generation } => {
                if generation == self.file_watch_set.generation {
                    let was_active = self.file_watcher_active;
                    tracing::warn!(
                        generation,
                        "file watcher stopped; bounded reload polling remains active"
                    );
                    self.file_watcher_active = false;
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/shell",
                        "file_watcher",
                        format!(
                            "watch generation {generation} stopped; using bounded metadata polling"
                        ),
                    );
                    if was_active {
                        self.schedule_reload_checks_now();
                    }
                }
            }
            ShellMessage::Ipc(request) => {
                pending.push_back(request);
            }
            ShellMessage::IpcProfileSwitch {
                profile_id,
                response,
            } => {
                pending.extend(self.apply_switch_profile_with_ack(&profile_id, Some(response)));
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
        let mut push_path = |path: PathBuf| {
            if !path.as_os_str().is_empty() {
                paths.push(path);
            }
        };

        let graph_path = self.installed_module_graph_path();
        push_path(graph_path.clone());
        push_path(graph_path.with_file_name("mesh.lock"));
        if let Ok(profile_paths) =
            mesh_core_module::package::ProfilePaths::from_root_graph(&graph_path)
        {
            push_path(profile_paths.active_profile_path());
            push_path(profile_paths.profiles_dir());
            if let Some(profile_id) = &self.active_profile_id
                && let Ok(path) = profile_paths.profile_path(profile_id)
            {
                push_path(path);
            }
        }
        push_path(self.theme_watch.path.clone());
        push_path(self.settings_watch.path.clone());
        for path in &self.module_dirs {
            push_path(path.clone());
        }
        for module in self.modules.values() {
            push_path(module.path.clone());
            push_path(module.manifest_path.clone());
        }
        for runtime in &self.components {
            for (path, _) in &runtime.source_paths {
                push_path(path.clone());
            }
        }

        if let Some(graph) = &self.installed_module_graph {
            for module in graph.modules() {
                push_path(PathBuf::from(&module.path));
                push_path(module.manifest_path.clone());
            }
            for theme in graph.theme_catalog().iter() {
                for mode in theme.modes.values() {
                    push_path(mode.source.candidate_path());
                }
            }
            for resource in graph
                .contributed_icons()
                .iter()
                .chain(graph.contributed_fonts())
            {
                push_path(resource.source.manifest_path.clone());
                if let Some(root) = resource.source.manifest_path.parent() {
                    push_path(root.join(&resource.path));
                }
            }
            for catalog in graph.contributed_i18n() {
                push_path(catalog.source.manifest_path.clone());
                if let Some(root) = catalog.source.manifest_path.parent() {
                    push_path(root.join(&catalog.path));
                }
            }
            for schema in graph.settings_schemas() {
                push_path(schema.source.manifest_path.clone());
                if let Some(entry) = &schema.settings_page
                    && let Some(root) = schema.source.manifest_path.parent()
                {
                    push_path(root.join(entry));
                }
            }
            for source in graph
                .frontend_entrypoints()
                .iter()
                .map(|entry| (&entry.source, entry.path.as_str()))
                .chain(
                    graph
                        .frontend_surfaces()
                        .iter()
                        .map(|surface| (&surface.source, surface.path.as_str())),
                )
                .chain(
                    graph
                        .contributed_layouts()
                        .iter()
                        .map(|layout| (&layout.source, layout.path.as_str())),
                )
                .chain(
                    graph
                        .contributed_libraries()
                        .iter()
                        .map(|library| (&library.source, library.path.as_str())),
                )
            {
                push_path(source.0.manifest_path.clone());
                if let Some(root) = source.0.manifest_path.parent() {
                    push_path(root.join(source.1));
                }
            }
            if let Ok((catalogs, _)) = graph.locale_catalog_sources() {
                for catalog in catalogs {
                    push_path(catalog.path);
                }
            }
        }

        for path in self
            .resource_snapshot
            .host_catalog
            .data_dirs
            .iter()
            .chain(self.resource_snapshot.host_catalog.icon_dirs.iter())
            .chain(self.resource_snapshot.host_catalog.font_dirs.iter())
        {
            push_path(path.clone());
        }
        for asset in self
            .resource_snapshot
            .icon_assets
            .iter()
            .chain(self.resource_snapshot.font_assets.iter())
        {
            push_path(asset.handle.candidate_path());
        }
        paths
    }

    /// Reconcile the worker with the inputs of the latest active catalog,
    /// profile, resource snapshot, theme, and mounted component graph. The
    /// worker owns the inotify descriptors; this shell-side generation is the
    /// authority used to reject delayed events from a retired set.
    pub(in crate::shell) fn reconcile_file_watcher(&mut self) {
        let paths = file_watch::WatchSet::new(0, self.file_watch_paths()).paths;
        let paths_changed = self.file_watch_set.paths != paths;
        let generation = if !paths_changed {
            self.file_watch_set.generation
        } else {
            self.file_watch_set.generation.saturating_add(1)
        };
        let watch_set = file_watch::WatchSet::new(generation, paths);
        let Some(tx) = self.file_watcher_tx.clone() else {
            self.file_watch_set = watch_set;
            return;
        };
        let Some(wake) = self.wake_handle.clone() else {
            self.file_watch_set = watch_set;
            return;
        };

        if !paths_changed {
            return;
        }

        self.file_watch_set = watch_set.clone();
        self.file_watcher_active = false;
        if paths_changed
            && let Some(watcher) = self.file_watcher.as_ref()
            && watcher.replace(watch_set.clone())
        {
            self.schedule_reload_checks_now();
            return;
        }
        if let Some(watcher) = self.file_watcher.take() {
            watcher.stop_and_join();
        }
        self.file_watcher = file_watch::spawn_file_watcher(watch_set, tx, wake);
        if self.file_watcher.is_none() {
            self.diagnostics.record_lifecycle_error(
                "@mesh/shell",
                "file_watcher",
                "could not start managed file watcher; using bounded metadata polling",
            );
        }
        self.schedule_reload_checks_now();
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
    filesystem_change_generations: std::collections::HashSet<u64>,
}

impl CoalescedShellMessages {
    fn push(&mut self, message: ShellMessage) {
        if let ShellMessage::FilesystemChanged { generation } = &message {
            if !self.filesystem_change_generations.insert(*generation) {
                return;
            }
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
        coalesced.push(ShellMessage::FilesystemChanged { generation: 1 });
        coalesced.push(ShellMessage::FilesystemChanged { generation: 1 });

        let messages = coalesced.into_vec();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0],
            ShellMessage::FilesystemChanged { generation: 1 }
        ));
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

    #[test]
    fn shutdown_quiescing_rejects_new_messages_and_requests() {
        let mut shell = Shell::new();
        assert_eq!(shell.shutdown_phase(), ShellShutdownPhase::Running);
        assert!(shell.begin_shutdown());
        assert_eq!(shell.shutdown_phase(), ShellShutdownPhase::Quiescing);
        assert!(!shell.begin_shutdown());

        let mut pending = VecDeque::new();
        shell
            .handle_shell_message(
                &mut pending,
                ShellMessage::Ipc(CoreRequest::ToggleDebugOverlay),
            )
            .unwrap();
        assert!(pending.is_empty());
        shell
            .apply_request(CoreRequest::ToggleDebugOverlay)
            .unwrap();
    }

    #[test]
    fn shutdown_runtime_advances_all_phases_and_is_idempotent() {
        let mut shell = Shell::new();
        let runtime = Runtime::new().unwrap();
        let socket_path =
            std::env::temp_dir().join(format!("mesh-shell-shutdown-test-{}", std::process::id()));

        shell.shutdown_runtime(&runtime, &socket_path).unwrap();
        assert_eq!(shell.shutdown_phase(), ShellShutdownPhase::Stopped);
        assert!(shell.shutdown_complete);
        shell.shutdown_runtime(&runtime, &socket_path).unwrap();
        assert_eq!(shell.shutdown_phase(), ShellShutdownPhase::Stopped);
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
