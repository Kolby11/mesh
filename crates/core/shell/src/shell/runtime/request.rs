use super::super::*;
use crate::shell::types::TabFocusTarget;
use mesh_core_debug::ProfilingBackendStage;

/// One main-loop tick (~60 Hz). Coalescable commands fire on the leading
/// edge; further calls within the interval park as `pending` and are
/// flushed on the next tick after the interval elapses. The slider's
/// visual position is rendered from cursor state independently of this
/// throttle, so dragging stays smooth.
const COMMAND_THROTTLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);
const DEBUG_INSPECTOR_SURFACE_ID: &str = "@mesh/debug-inspector";

impl Shell {
    pub(in crate::shell) fn claim_keyboard_focus_for_surface(&mut self, surface_id: &str) {
        if let Some(previous) = self.keyboard_focus_surface.clone()
            && previous != surface_id
        {
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == previous)
            {
                runtime.component.set_keyboard_mode_override(None);
            }
        }

        self.keyboard_focus_surface = Some(surface_id.to_string());
        if let Some(runtime) = self
            .components
            .iter_mut()
            .find(|runtime| runtime.surface_id == surface_id)
        {
            runtime.component.set_keyboard_mode_override(None);
        }
    }

    pub(in crate::shell) fn broadcast_core_event(
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

    pub(in crate::shell) fn drain_requests(
        &mut self,
        requests: &mut VecDeque<CoreRequest>,
    ) -> Result<(), ShellRunError> {
        while let Some(request) = requests.pop_front() {
            let emitted = self.apply_request(request)?;
            requests.extend(emitted);
        }
        Ok(())
    }

    pub(in crate::shell) fn apply_request(
        &mut self,
        request: CoreRequest,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let trigger_kind = profiling_trigger_for_request(&request);
        let profiling_started = self.profiling_enabled().then(std::time::Instant::now);
        let result = match request {
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
            CoreRequest::ActivatePopover {
                surface_id,
                trigger_surface,
                trigger_key,
                focus,
            } => {
                tracing::info!(
                    "apply_request ActivatePopover surface_id={surface_id} trigger_surface={trigger_surface} trigger_key={trigger_key} focus={focus}"
                );
                let trigger_runtime_found = self
                    .components
                    .iter_mut()
                    .find(|r| r.surface_id == trigger_surface)
                    .map(|runtime| {
                        if !trigger_key.is_empty() {
                            runtime
                                .component
                                .register_popover_trigger(trigger_key.clone(), surface_id.clone());
                        }
                        true
                    })
                    .unwrap_or(false);
                let target_runtime_found =
                    self.components.iter().any(|r| r.surface_id == surface_id);
                tracing::info!(
                    "apply_request ActivatePopover trigger_runtime_found={trigger_runtime_found} target_runtime_found={target_runtime_found}"
                );
                let mut emitted = VecDeque::new();
                emitted.push_back(CoreRequest::ShowSurface {
                    surface_id: surface_id.clone(),
                });
                if focus && !trigger_surface.is_empty() && !trigger_key.is_empty() {
                    emitted.push_back(CoreRequest::TransferTabFocus {
                        from_surface: trigger_surface.clone(),
                        to_surface: surface_id.clone(),
                        target: TabFocusTarget::First,
                        return_target: Some((trigger_surface, trigger_key)),
                        target_closes_on_leave: true,
                        close_source: None,
                    });
                }
                Ok(emitted)
            }
            CoreRequest::TransferTabFocus {
                from_surface,
                to_surface,
                target,
                return_target,
                target_closes_on_leave,
                close_source,
            } => self.apply_transfer_tab_focus(
                &from_surface,
                &to_surface,
                target,
                return_target,
                target_closes_on_leave,
                close_source,
            ),
            CoreRequest::SetTheme { theme_id } => self.apply_set_theme(&theme_id),
            CoreRequest::ToggleDebugOverlay => {
                self.debug.toggle();
                tracing::debug!(
                    "debug overlay: {}",
                    if self.debug.enabled { "on" } else { "off" }
                );
                self.set_surface_visibility(
                    DEBUG_INSPECTOR_SURFACE_ID.to_string(),
                    self.debug.enabled,
                )
            }
            CoreRequest::ToggleDebugProfiling => {
                let enabled = self.debug.toggle_profiling();
                if enabled {
                    self.profiling
                        .reset_for_new_session(self.debug.profiling_session_id);
                }
                tracing::debug!("debug profiling: {}", if enabled { "on" } else { "off" });
                Ok(VecDeque::new())
            }
            CoreRequest::RunDebugBenchmark { scenario_id } => {
                tracing::info!("diagnostic: unknown debug benchmark scenario: {scenario_id}");
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
        };
        if let Some(started) = profiling_started
            && result.is_ok()
        {
            self.record_shell_profiling_stage(
                mesh_core_debug::ProfilingStage::RuntimeUpdateHandling,
                started.elapsed(),
                Some(trigger_kind),
            );
        }
        result
    }

    pub(in crate::shell) fn dispatch_service_command(
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

        if self.service_handlers.contains_key(interface) {
            let coalesce = self.service_command_is_coalescable(interface, command);
            if coalesce {
                let key = (interface.to_string(), command.to_string());
                let now = std::time::Instant::now();
                let entry = self.command_throttle.get(&key);
                let allow_send = entry
                    .map(|state| now.duration_since(state.last_send) >= COMMAND_THROTTLE_INTERVAL)
                    .unwrap_or(true);
                if allow_send {
                    self.command_throttle.insert(
                        key,
                        CommandThrottleState {
                            last_send: now,
                            pending: None,
                        },
                    );
                    match self.send_service_command_message(interface, command, payload, coalesce) {
                        Some(Ok(())) => serde_json::json!({ "ok": true, "queued": true }),
                        Some(Err(())) => serde_json::json!({
                            "ok": false,
                            "error": "service_unavailable",
                            "status": "service_unavailable",
                        }),
                        None => serde_json::json!({
                            "ok": false,
                            "error": "service_unavailable",
                            "status": "service_unavailable",
                        }),
                    }
                } else {
                    let state =
                        self.command_throttle
                            .entry(key)
                            .or_insert_with(|| CommandThrottleState {
                                last_send: now,
                                pending: None,
                            });
                    state.pending = Some(payload.clone());
                    serde_json::json!({ "ok": true, "queued": true, "throttled": true })
                }
            } else {
                match self.send_service_command_message(interface, command, payload, coalesce) {
                    Some(Ok(())) => serde_json::json!({ "ok": true, "queued": true }),
                    Some(Err(())) => serde_json::json!({
                        "ok": false,
                        "error": "service_unavailable",
                        "status": "service_unavailable",
                    }),
                    None => serde_json::json!({
                        "ok": false,
                        "error": "service_unavailable",
                        "status": "service_unavailable",
                    }),
                }
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

    fn send_service_command_message(
        &mut self,
        interface: &str,
        command: &str,
        payload: &serde_json::Value,
        coalesce: bool,
    ) -> Option<Result<(), ()>> {
        let tx = self.service_handlers.get(interface).cloned()?;
        let active_provider_id = self
            .backend_runtimes
            .get(interface)
            .map(|slot| slot.provider_id.clone());
        let profiling_started = (self.profiling_enabled() && active_provider_id.is_some())
            .then(std::time::Instant::now);
        let result = tx
            .send(ServiceCommandMsg {
                command: command.to_string(),
                payload: payload.clone(),
                coalesce,
            })
            .map_err(|_| ());
        if let (Some(provider_id), Some(started)) = (active_provider_id, profiling_started) {
            self.record_backend_profiling_stage(
                interface,
                &provider_id,
                ProfilingBackendStage::CommandHandling,
                started.elapsed(),
                Some("service_command"),
            );
        }
        Some(result)
    }

    /// Apply a cross-surface tab focus transfer. Clears focus on the
    /// source surface, hands focus to the target with the requested
    /// position, swaps `keyboard_mode` so the compositor delivers keys to
    /// the new owner, and emits HideSurface for `close_source` if set.
    fn apply_transfer_tab_focus(
        &mut self,
        from_surface: &str,
        to_surface: &str,
        target: TabFocusTarget,
        return_target: Option<(SurfaceId, String)>,
        target_closes_on_leave: bool,
        close_source: Option<SurfaceId>,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        tracing::info!(
            "apply_transfer_tab_focus from={from_surface} to={to_surface} return_target={return_target:?} target_closes_on_leave={target_closes_on_leave} close_source={close_source:?}"
        );
        if let Some(runtime) = self
            .components
            .iter_mut()
            .find(|r| r.surface_id == from_surface)
        {
            runtime.component.release_focus_for_transfer();
            // Source: clear any prior override and force None so the
            // compositor can hand keyboard delivery to the target.
            runtime
                .component
                .set_keyboard_mode_override(Some(mesh_core_wayland::KeyboardMode::None));
        }
        if let Some(surface) = self.surfaces.get_mut(from_surface) {
            surface.keyboard_mode = mesh_core_wayland::KeyboardMode::None;
        }

        let target_found = if let Some(runtime) = self
            .components
            .iter_mut()
            .find(|r| r.surface_id == to_surface)
        {
            runtime.component.receive_focus_transfer(
                &target,
                return_target,
                target_closes_on_leave,
            );
            // Target: Exclusive while it owns cross-surface keyboard focus.
            // This includes the return leg from a closing popover; falling
            // back to OnDemand there leaves some compositors with no concrete
            // surface delivering subsequent key events.
            let target_owns_keyboard = target_closes_on_leave || close_source.is_some();
            let mode = if target_owns_keyboard {
                Some(mesh_core_wayland::KeyboardMode::Exclusive)
            } else {
                None
            };
            runtime.component.set_keyboard_mode_override(mode);
            true
        } else {
            tracing::warn!(
                to_surface,
                "TransferTabFocus target component not found; ignoring"
            );
            false
        };

        tracing::info!("apply_transfer_tab_focus target_found={target_found} to={to_surface}");
        if !target_found {
            return Ok(VecDeque::new());
        }

        self.keyboard_focus_surface = Some(to_surface.to_string());

        if let Some(surface) = self.surfaces.get_mut(to_surface) {
            surface.keyboard_mode = if target_closes_on_leave || close_source.is_some() {
                mesh_core_wayland::KeyboardMode::Exclusive
            } else {
                mesh_core_wayland::KeyboardMode::OnDemand
            };
        }

        let mut emitted = VecDeque::new();
        if let Some(close) = close_source {
            emitted.push_back(CoreRequest::HideSurface { surface_id: close });
        }
        Ok(emitted)
    }

    fn service_command_is_supported(&self, interface: &str, command: &str) -> bool {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract.as_ref() else {
            return true;
        };
        contract.methods.iter().any(|method| method.name == command)
    }

    /// Trailing-edge flush. Called once per main-loop tick: any throttled
    /// command whose interval has elapsed since its last send is dispatched
    /// now with the most recent payload. Stale entries (no pending payload
    /// and well past their interval) are pruned to keep the map bounded.
    pub(in crate::shell) fn flush_throttled_commands(&mut self) {
        if self.command_throttle.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let mut to_send: Vec<(String, String, serde_json::Value)> = Vec::new();
        let mut to_remove: Vec<(String, String)> = Vec::new();
        for (key, state) in self.command_throttle.iter_mut() {
            if now.duration_since(state.last_send) < COMMAND_THROTTLE_INTERVAL {
                continue;
            }
            if let Some(payload) = state.pending.take() {
                to_send.push((key.0.clone(), key.1.clone(), payload));
                state.last_send = now;
            } else if now.duration_since(state.last_send)
                >= COMMAND_THROTTLE_INTERVAL.saturating_mul(8)
            {
                to_remove.push(key.clone());
            }
        }
        for (interface, command, payload) in to_send {
            let _ = self.send_service_command_message(&interface, &command, &payload, true);
        }
        for key in to_remove {
            self.command_throttle.remove(&key);
        }
    }

    fn service_command_is_coalescable(&self, interface: &str, command: &str) -> bool {
        let resolution = self.interfaces.resolve(interface, None);
        resolution
            .contract
            .as_ref()
            .and_then(|contract| contract.methods.iter().find(|m| m.name == command))
            .is_some_and(|method| method.coalesce)
    }

    fn set_surface_visibility(
        &mut self,
        surface_id: SurfaceId,
        visible: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        tracing::info!("set_surface_visibility surface_id={surface_id} visible={visible}");
        if surface_id == DEBUG_INSPECTOR_SURFACE_ID {
            self.debug.enabled = visible;
        }
        self.core
            .surfaces
            .entry(surface_id.clone())
            .and_modify(|state| state.visible = visible)
            .or_insert(SurfaceState { visible });
        if !visible && self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
            self.keyboard_focus_surface = None;
        }

        self.broadcast_core_event(CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        })
    }
}

fn profiling_trigger_for_request(request: &CoreRequest) -> &'static str {
    match request {
        CoreRequest::PositionSurface { .. } => "position_surface",
        CoreRequest::ToggleSurface { .. } => "toggle_surface",
        CoreRequest::ShowSurface { .. } => "show_surface",
        CoreRequest::HideSurface { .. } => "hide_surface",
        CoreRequest::PublishDiagnostics { .. } => "publish_diagnostics",
        CoreRequest::ServiceCommand { .. } => "service_command",
        CoreRequest::WriteClipboard { .. } => "write_clipboard",
        CoreRequest::SetTheme { .. } => "set_theme",
        CoreRequest::ActivatePopover { .. } => "activate_popover",
        CoreRequest::TransferTabFocus { .. } => "transfer_tab_focus",
        CoreRequest::ToggleDebugOverlay => "toggle_debug_overlay",
        CoreRequest::ToggleDebugProfiling => "toggle_debug_profiling",
        CoreRequest::RunDebugBenchmark { .. } => "run_debug_benchmark",
        CoreRequest::CycleDebugTab => "cycle_debug_tab",
        CoreRequest::Shutdown => "shutdown",
    }
}
