use super::super::*;
use crate::shell::types::TabFocusTarget;
use mesh_core_debug::{
    BenchmarkScenarioId, BenchmarkScenarioStatus, DebugBenchmarkRunState, ProfilingBackendStage,
};
use mesh_core_presentation::{
    LayerSurfaceSizePolicy, PopupAnchor, PopupConfig, PopupConstraint, PopupGravity, PopupPlacement,
};

/// One main-loop tick (~60 Hz). Coalescable commands fire on the leading
/// edge; further calls within the interval park as `pending` and are
/// flushed on the next tick after the interval elapses. The slider's
/// visual position is rendered from cursor state independently of this
/// throttle, so dragging stays smooth.
pub(in crate::shell) const COMMAND_THROTTLE_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(16);
const POPOVER_HOVER_BRIDGE_DELAY: std::time::Duration = std::time::Duration::from_millis(180);
const DEBUG_INSPECTOR_SURFACE_ID: &str = "@mesh/debug-inspector";

/// Canonical failure payload when a service command cannot be delivered (no
/// backend channel, send failure, or unregistered interface).
fn service_unavailable_response() -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": "service_unavailable",
        "status": "service_unavailable",
    })
}

impl Shell {
    fn invalidate_debug_layout_bounds_targets(&mut self) {
        for runtime in &mut self.components {
            runtime.component.request_paint();
            runtime.parent.force_full_present = true;
            for child in &mut runtime.children {
                child.target.force_full_present = true;
            }
        }
    }

    fn clear_transfer_owned_keyboard_mode(
        &mut self,
        surface_id: &str,
    ) -> Option<mesh_core_wayland::KeyboardMode> {
        let previous_mode = self
            .transfer_owned_keyboard_modes
            .remove(surface_id)
            .or_else(|| {
                self.surfaces
                    .get(surface_id)
                    .map(|surface| surface.keyboard_mode)
            });
        let Some(previous_mode) = previous_mode else {
            return None;
        };
        if previous_mode == mesh_core_wayland::KeyboardMode::OnDemand {
            self.configure_surface_keyboard_mode(surface_id, mesh_core_wayland::KeyboardMode::None);
        }
        if let Some(surface) = self.surfaces.get_mut(surface_id) {
            surface.keyboard_mode = previous_mode;
        }
        if let Some(index) = self.component_index_for_surface(surface_id) {
            let runtime = &mut self.components[index];
            runtime.component.set_keyboard_mode_override(None);
        }
        Some(previous_mode)
    }

    fn configure_surface_keyboard_mode(
        &mut self,
        surface_id: &str,
        keyboard_mode: mesh_core_wayland::KeyboardMode,
    ) {
        let size_policy = self
            .component_index_for_surface(surface_id)
            .map(|index| self.components[index].parent.surface_size_policy)
            .unwrap_or(LayerSurfaceSizePolicy::Fixed);

        let (surface, visible) = match self.surfaces.get(surface_id) {
            Some(surface) => {
                let visible = self
                    .core
                    .surfaces
                    .get(surface_id)
                    .map(|state| state.visible)
                    .unwrap_or(surface.visible);
                (surface, visible)
            }
            None => return,
        };

        let cfg = if visible {
            LayerSurfaceConfig {
                edge: surface.edge,
                layer: surface.layer.unwrap_or(Layer::Top),
                size_policy,
                width: surface.width,
                height: surface.height,
                exclusive_zone: surface.exclusive_zone,
                keyboard_mode,
                namespace: surface_id.to_string(),
                margin_top: surface.margin_top,
                margin_right: surface.margin_right,
                margin_bottom: surface.margin_bottom,
                margin_left: surface.margin_left,
            }
        } else {
            LayerSurfaceConfig {
                edge: surface.edge,
                layer: surface.layer.unwrap_or(Layer::Top),
                size_policy: LayerSurfaceSizePolicy::Fixed,
                width: 1,
                height: 1,
                exclusive_zone: 0,
                keyboard_mode: mesh_core_wayland::KeyboardMode::None,
                namespace: surface_id.to_string(),
                margin_top: 0,
                margin_right: 0,
                margin_bottom: 0,
                margin_left: 0,
            }
        };

        self.presentation_engine.configure(surface_id, cfg.clone());
        if let Some(index) = self.component_index_for_surface(surface_id) {
            self.components[index].parent.last_surface_config = Some(cfg);
        }
    }

    pub(in crate::shell) fn claim_keyboard_focus_for_surface(&mut self, surface_id: &str) {
        let previous_focus = self.keyboard_focus_surface.clone();
        if let Some(previous) = previous_focus.as_deref()
            && previous != surface_id
        {
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == previous)
            {
                runtime.component.set_keyboard_mode_override(None);
            }
            if self.transfer_owned_keyboard_modes.contains_key(previous) {
                self.clear_transfer_owned_keyboard_mode(previous);
            }
        } else if previous_focus.as_deref() == Some(surface_id)
            && self.transfer_owned_keyboard_modes.contains_key(surface_id)
        {
            self.clear_transfer_owned_keyboard_mode(surface_id);
        }

        self.keyboard_focus_surface = Some(surface_id.to_string());
        if previous_focus.as_deref() != Some(surface_id) {
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == surface_id)
            {
                runtime.component.set_keyboard_mode_override(None);
            }
            if self.transfer_owned_keyboard_modes.contains_key(surface_id) {
                self.clear_transfer_owned_keyboard_mode(surface_id);
            }
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

    pub(in crate::shell) fn drain_request(
        &mut self,
        request: CoreRequest,
    ) -> Result<(), ShellRunError> {
        let mut emitted = self.apply_request(request)?;
        self.drain_requests(&mut emitted)
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
                self.pending_popover_hides.remove(&surface_id);
                self.set_surface_visibility(surface_id, false)
            }
            CoreRequest::HidePopover {
                surface_id,
                defer_for_hover_bridge,
            } => self.hide_popover(surface_id, defer_for_hover_bridge),
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
                self.cancel_pending_popover_hide(&surface_id);
                if let Some(index) = self.component_index_for_surface(&trigger_surface) {
                    let runtime = &mut self.components[index];
                    if !trigger_key.is_empty() {
                        runtime
                            .component
                            .register_popover_trigger(trigger_key.clone(), surface_id.clone());
                    }
                }
                let target_runtime_found = self.component_index_for_surface(&surface_id).is_some();

                // Promote to xdg_popup when the compositor supports it and the
                // trigger surface is known. The anchor rect is built from the
                // trigger surface's exclusive-zone (bar height) and the popover's
                // current margin-left (set by the preceding shell.position-surface
                // event), both of which are in the parent surface's coordinate
                // space and available before the next render frame.
                // A popover re-activates itself from its own `onpointerenter`
                // (a keep-alive that cancels the trigger's pending close while
                // the cursor travels onto it). That re-entrant call must not
                // re-promote the popup: `trigger_surface` would be the popover
                // itself, so it would be re-parented to itself and its
                // anchor_rect (in the original parent's space) would be
                // reinterpreted against the tiny popover surface, sliding it to
                // a screen edge. Only promote when the trigger is a *different*
                // surface (the real anchor).
                if self.presentation_engine.popup_supported()
                    && !trigger_surface.is_empty()
                    && trigger_surface != surface_id
                    && target_runtime_found
                {
                    let trigger_exclusive_zone = self
                        .surfaces
                        .get(&trigger_surface)
                        .map(|s| s.exclusive_zone)
                        .unwrap_or(40);
                    // Anchor the popup to the trigger element's *real* rect in
                    // the parent surface, then let the compositor center it
                    // (anchor = bottom-center of the trigger, gravity = down).
                    // This is width-agnostic: the popover may measure wider than
                    // any value the component could predict, and the compositor
                    // keeps it centered and on-screen (flip/slide) as it grows.
                    // The component must NOT compute its own left edge from an
                    // assumed popover width — that hardcoding caused near-edge
                    // popovers to overflow and get slid sideways once their
                    // measured size exceeded the assumption.
                    let trigger_rect =
                        self.component_index_for_surface(&trigger_surface)
                            .and_then(|idx| {
                                self.components[idx]
                                    .component
                                    .node_bounds_by_key(&trigger_key)
                            });
                    let (anchor_x, anchor_w) = match trigger_rect {
                        Some((left, _top, right, _bottom)) => {
                            (left.round() as i32, ((right - left).round() as i32).max(1))
                        }
                        // Fall back to the component-reported margin-left (the
                        // legacy single-edge anchor) when the trigger rect is
                        // unavailable, e.g. activation without an event.
                        None => (
                            self.component_index_for_surface(&surface_id)
                                .map(|idx| self.components[idx].component.popover_margin_left())
                                .unwrap_or(0),
                            1,
                        ),
                    };
                    let popup_config = PopupConfig {
                        parent_surface_id: trigger_surface.clone(),
                        placement: PopupPlacement {
                            anchor_rect: (anchor_x, 0, anchor_w, trigger_exclusive_zone),
                            size: (1, 1),
                            anchor: PopupAnchor::Bottom,
                            gravity: PopupGravity::Bottom,
                            constraint: PopupConstraint::default(),
                            offset: (0, 0),
                        },
                        grab: false,
                        grab_serial: None,
                    };
                    // Legacy path: this promotes a *separate* popover module's
                    // own parent surface into an xdg_popup. The newer model
                    // (auto-derived child surfaces from in-tree `<popover open>`
                    // nodes of a single component VM) supersedes this; kept for
                    // shipped separate-module popovers during the transition.
                    if let Some(idx) = self.component_index_for_surface(&surface_id) {
                        self.components[idx].parent.popup_parent_surface =
                            Some(trigger_surface.clone());
                        self.components[idx].parent.popup_config = Some(popup_config);
                        self.components[idx].parent.last_popup_size = None;
                        self.components[idx].component.set_popup_promoted(true);
                    }
                    tracing::info!(
                        "ActivatePopover: promoting {surface_id} as xdg_popup child of {trigger_surface} trigger_rect=({anchor_x},{anchor_w}) bar_h={trigger_exclusive_zone}"
                    );
                }

                let mut emitted = self.sibling_popover_hides(&surface_id, &trigger_surface);
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
            CoreRequest::SetLocale { locale } => self.apply_set_locale(&locale),
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
            CoreRequest::ToggleDebugLayoutBounds => {
                self.debug.toggle_layout_bounds();
                tracing::debug!(
                    "debug layout bounds: {}",
                    if self.debug.show_layout_bounds {
                        "on"
                    } else {
                        "off"
                    }
                );
                self.invalidate_debug_layout_bounds_targets();
                Ok(VecDeque::new())
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
                self.apply_run_debug_benchmark(&scenario_id)
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
        let interface_canonical = canonical_interface_name_cow(interface);
        let service_caps = service_capabilities(interface_canonical.as_ref());
        let required = &service_caps.control;
        if !source_capabilities.is_granted(required) {
            tracing::warn!(
                source_module_id,
                interface,
                command,
                required_capability = %required,
                "denied unauthorized service command dispatch"
            );
            self.record_method_call(mesh_core_debug::MethodCallEntry {
                interface: interface_canonical.to_string(),
                provider_id: None,
                source_module_id: source_module_id.to_string(),
                command: command.to_string(),
                status: "capability_denied".to_string(),
                queued: false,
                result: Some(serde_json::json!({
                    "ok": false,
                    "error": "capability_denied",
                    "status": "capability_denied",
                })),
                error: Some("capability_denied".to_string()),
            });
            return serde_json::json!({
                "ok": false,
                "error": "capability_denied",
                "status": "capability_denied",
            });
        }

        if !self.service_command_is_supported(interface_canonical.as_ref(), command) {
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
            self.record_method_call(mesh_core_debug::MethodCallEntry {
                interface: interface_canonical.to_string(),
                provider_id: None,
                source_module_id: source_module_id.to_string(),
                command: command.to_string(),
                status: "unsupported_service_command".to_string(),
                queued: false,
                result: Some(serde_json::json!({
                    "ok": false,
                    "error": message,
                    "status": "unsupported_service_command",
                })),
                error: Some(message.clone()),
            });
            return serde_json::json!({
                "ok": false,
                "error": message,
                "status": "unsupported_service_command",
            });
        }

        let interface = interface_canonical.as_ref();
        let mut dispatch_result = if self.service_handlers.contains_key(interface) {
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
                        Some(Err(())) | None => service_unavailable_response(),
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
                    Some(Err(())) | None => service_unavailable_response(),
                }
            }
        } else {
            tracing::debug!("no handler registered for service: {interface}");
            service_unavailable_response()
        };

        if dispatch_result
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
            && interface == "mesh.audio"
            && command == "set_muted"
            && let Some(muted) = payload.get("muted").and_then(|value| value.as_bool())
        {
            self.apply_optimistic_audio_muted_state(muted);
            dispatch_result["optimistic"] = serde_json::json!(true);
        }

        let provider_id = self
            .backend_runtimes
            .get(interface)
            .map(|slot| slot.provider_id.clone());
        let queued = dispatch_result
            .get("queued")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let status = dispatch_result
            .get("status")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                if queued {
                    "queued".to_string()
                } else {
                    "failed".to_string()
                }
            });
        let error = dispatch_result
            .get("error")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        self.record_method_call(mesh_core_debug::MethodCallEntry {
            interface: interface.to_string(),
            provider_id,
            source_module_id: source_module_id.to_string(),
            command: command.to_string(),
            status,
            queued,
            result: Some(dispatch_result.clone()),
            error,
        });

        dispatch_result
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

    fn apply_run_debug_benchmark(
        &mut self,
        scenario_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let Some(scenario) = benchmark_scenario_id(scenario_id) else {
            let mut emitted = VecDeque::new();
            emitted.push_back(CoreRequest::PublishDiagnostics {
                message: format!("unknown debug benchmark scenario: {scenario_id}"),
            });
            return Ok(emitted);
        };

        self.debug.latest_benchmark_run = Some(DebugBenchmarkRunState {
            scenario_id: scenario,
            status: BenchmarkScenarioStatus::WaitingForSamples,
        });

        let mut emitted = VecDeque::new();
        if scenario == BenchmarkScenarioId::SurfaceOpenClose {
            emitted.push_back(CoreRequest::ToggleSurface {
                surface_id: "@mesh/audio-popover".to_string(),
            });
        }
        Ok(emitted)
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
        if let Some(index) = self.component_index_for_surface(from_surface) {
            {
                let runtime = &mut self.components[index];
                runtime.component.release_focus_for_transfer();
                runtime
                    .component
                    .set_keyboard_mode_override(Some(mesh_core_wayland::KeyboardMode::None));
            }
            // Source: clear any prior override and force None so the
            // compositor can hand keyboard delivery to the target.
            self.clear_transfer_owned_keyboard_mode(from_surface);
        }
        if let Some(surface) = self.surfaces.get_mut(from_surface) {
            surface.keyboard_mode = mesh_core_wayland::KeyboardMode::None;
        }

        let target_owns_keyboard = target_closes_on_leave || close_source.is_some();
        let target_restore_keyboard_mode = self
            .transfer_owned_keyboard_modes
            .remove(to_surface)
            .or_else(|| {
                self.surfaces
                    .get(to_surface)
                    .map(|surface| surface.keyboard_mode)
            });

        let target_found = if let Some(index) = self.component_index_for_surface(to_surface) {
            let runtime = &mut self.components[index];
            runtime.component.receive_focus_transfer(
                &target,
                return_target,
                target_closes_on_leave,
            );
            // Target: Exclusive while it owns cross-surface keyboard focus.
            // This includes the return leg from a closing popover; falling
            // back to OnDemand there leaves some compositors with no concrete
            // surface delivering subsequent key events.
            let mode = if target_owns_keyboard {
                Some(mesh_core_wayland::KeyboardMode::Exclusive)
            } else {
                None
            };
            runtime.component.set_keyboard_mode_override(mode);
            if target_owns_keyboard {
                if let Some(restore_mode) = target_restore_keyboard_mode {
                    self.transfer_owned_keyboard_modes
                        .insert(to_surface.to_string(), restore_mode);
                } else {
                    self.transfer_owned_keyboard_modes.insert(
                        to_surface.to_string(),
                        mesh_core_wayland::KeyboardMode::None,
                    );
                }
                if let Some(surface) = self.surfaces.get_mut(to_surface) {
                    surface.keyboard_mode = mesh_core_wayland::KeyboardMode::Exclusive;
                }
            } else {
                self.transfer_owned_keyboard_modes.remove(to_surface);
                if let Some(surface) = self.surfaces.get_mut(to_surface) {
                    surface.keyboard_mode = mesh_core_wayland::KeyboardMode::OnDemand;
                }
            }
            true
        } else {
            tracing::warn!(
                to_surface,
                "TransferTabFocus target component not found; ignoring"
            );
            if let Some(restore_mode) = target_restore_keyboard_mode {
                if let Some(surface) = self.surfaces.get_mut(to_surface) {
                    surface.keyboard_mode = restore_mode;
                }
            } else {
                self.transfer_owned_keyboard_modes.remove(to_surface);
            }
            false
        };

        tracing::info!("apply_transfer_tab_focus target_found={target_found} to={to_surface}");
        if !target_found {
            return Ok(VecDeque::new());
        }

        self.keyboard_focus_surface = Some(to_surface.to_string());

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
        if visible {
            self.pending_popover_hides.remove(&surface_id);
            if let Some(state) = self.core.surfaces.get_mut(&surface_id) {
                state.closing_until = None;
            }
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == surface_id.as_str())
            {
                runtime.component.set_surface_exiting(false);
            }
            return self.set_surface_visibility_now(surface_id, true);
        }

        let hide_transition = self
            .components
            .iter()
            .find(|runtime| runtime.surface_id == surface_id.as_str())
            .map(|runtime| runtime.component.hide_transition_ms())
            .unwrap_or(0);
        let is_visible = self
            .core
            .surfaces
            .get(&surface_id)
            .map(|state| state.visible)
            .unwrap_or_else(|| {
                self.surfaces
                    .get(&surface_id)
                    .map(|surface| surface.visible)
                    .unwrap_or(true)
            });
        let already_closing = self
            .core
            .surfaces
            .get(&surface_id)
            .and_then(|state| state.closing_until)
            .is_some();
        if hide_transition > 0 && is_visible && !already_closing {
            let until =
                std::time::Instant::now() + std::time::Duration::from_millis(hide_transition);
            self.core
                .surfaces
                .entry(surface_id.clone())
                .and_modify(|state| {
                    state.visible = true;
                    state.closing_until = Some(until);
                })
                .or_insert(SurfaceState {
                    visible: true,
                    closing_until: Some(until),
                });
            if self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
                self.keyboard_focus_surface = None;
            }
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == surface_id.as_str())
            {
                runtime.component.set_keyboard_mode_override(None);
                runtime.component.set_surface_exiting(true);
            }
            if let Some(previous_mode) = self.transfer_owned_keyboard_modes.remove(&surface_id) {
                if let Some(surface) = self.surfaces.get_mut(&surface_id) {
                    surface.keyboard_mode = previous_mode;
                }
            }
            return Ok(VecDeque::new());
        }

        self.set_surface_visibility_now(surface_id, false)
    }

    fn sibling_popover_hides(
        &self,
        surface_id: &str,
        trigger_surface: &str,
    ) -> VecDeque<CoreRequest> {
        if trigger_surface.is_empty() {
            return VecDeque::new();
        }
        self.components
            .iter()
            .filter(|runtime| {
                runtime.surface_id != surface_id
                    && runtime.parent.popup_parent_surface.as_deref() == Some(trigger_surface)
                    && self.surface_is_effectively_visible(runtime.surface_id.as_str())
            })
            .map(|runtime| CoreRequest::HidePopover {
                surface_id: runtime.surface_id.clone(),
                defer_for_hover_bridge: false,
            })
            .collect()
    }

    pub(in crate::shell) fn hide_popover(
        &mut self,
        surface_id: SurfaceId,
        defer_for_hover_bridge: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if defer_for_hover_bridge && self.surface_is_promoted_popover(&surface_id) {
            self.pending_popover_hides.insert(
                surface_id.clone(),
                std::time::Instant::now() + POPOVER_HOVER_BRIDGE_DELAY,
            );
            if let Some(state) = self.core.surfaces.get_mut(&surface_id) {
                state.visible = true;
                state.closing_until = None;
            }
            if let Some(index) = self.component_index_for_surface(&surface_id) {
                self.components[index].component.set_surface_exiting(false);
            }
            return Ok(VecDeque::new());
        }

        self.pending_popover_hides.remove(&surface_id);
        self.set_surface_visibility(surface_id, false)
    }

    pub(in crate::shell) fn cancel_pending_popover_hide(&mut self, surface_id: &str) -> bool {
        let cancelled = self.pending_popover_hides.remove(surface_id).is_some();
        if cancelled {
            if let Some(state) = self.core.surfaces.get_mut(surface_id) {
                state.closing_until = None;
                state.visible = true;
            }
            // Only clear surface_exiting on top-level promoted surfaces; in-tree child
            // surfaces don't have their own ComponentRuntime so calling set_surface_exiting
            // on the parent would incorrectly affect the parent surface's animation state.
            let target = self.component_target_for_surface(surface_id);
            if let Some((index, crate::shell::types::TargetRef::Parent)) = target {
                self.components[index].component.set_surface_exiting(false);
            }
        }
        cancelled
    }

    pub(in crate::shell) fn defer_child_popover_hides_for_parent(
        &mut self,
        parent_surface_id: &str,
    ) {
        let Some(index) = self.component_index_for_surface(parent_surface_id) else {
            return;
        };
        let hide_at = std::time::Instant::now() + POPOVER_HOVER_BRIDGE_DELAY;
        let child_surface_ids: Vec<_> = self.components[index]
            .children
            .iter()
            .filter(|child| child.target.popup_parent_surface.as_deref() == Some(parent_surface_id))
            .map(|child| child.target.surface_id.clone())
            .collect();
        for surface_id in child_surface_ids {
            self.pending_popover_hides.insert(surface_id, hide_at);
        }
    }

    pub(in crate::shell) fn cancel_pending_child_popover_hides_at(
        &mut self,
        parent_surface_id: &str,
        x: f32,
        y: f32,
    ) {
        let Some(index) = self.component_index_for_surface(parent_surface_id) else {
            return;
        };
        let child_surface_ids: Vec<_> = self.components[index]
            .children
            .iter()
            .filter(|child| {
                child.target.popup_parent_surface.as_deref() == Some(parent_surface_id)
                    && point_in_rect(x, y, child.anchor_rect)
            })
            .map(|child| child.target.surface_id.clone())
            .collect();
        for surface_id in child_surface_ids {
            self.cancel_pending_popover_hide(&surface_id);
        }
    }

    fn child_popover_pointer_leave_requests(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<VecDeque<CoreRequest>>, ShellRunError> {
        let Some((index, crate::shell::types::TargetRef::Child(child_index))) =
            self.component_target_for_surface(surface_id)
        else {
            return Ok(None);
        };
        if self.components[index].children[child_index]
            .target
            .popup_parent_surface
            .is_none()
        {
            return Ok(None);
        }

        let node_key = self.components[index].children[child_index]
            .node_key
            .clone();
        let target_surface_size = self.components[index].children[child_index]
            .target
            .known_surface_size
            .or_else(|| {
                self.components[index].children[child_index]
                    .target
                    .paint_buffer
                    .as_ref()
                    .map(|buffer| (buffer.width.max(1), buffer.height.max(1)))
            })
            .or_else(|| self.presentation_engine.surface_size_if_known(surface_id))
            .unwrap_or((1, 1));
        let component_surface_size = self.components[index]
            .parent
            .known_surface_size
            .or_else(|| {
                self.surfaces
                    .get(&self.components[index].surface_id)
                    .map(|surface| (surface.width.max(1), surface.height.max(1)))
            })
            .unwrap_or(target_surface_size);
        self.components[index]
            .target_mut(crate::shell::types::TargetRef::Child(child_index))
            .known_surface_size = Some(target_surface_size);

        let emitted = self.components[index]
            .component
            .handle_child_surface_input(
                &node_key,
                self.theme.active(),
                component_surface_size.0,
                component_surface_size.1,
                ComponentInput::PointerLeave,
            )
            .map_err(ShellRunError::Component)?;
        Ok(Some(VecDeque::from(emitted)))
    }

    fn surface_is_promoted_popover(&mut self, surface_id: &str) -> bool {
        let Some((index, target)) = self.component_target_for_surface(surface_id) else {
            return false;
        };
        match target {
            crate::shell::types::TargetRef::Parent => {
                self.components[index].parent.popup_parent_surface.is_some()
            }
            crate::shell::types::TargetRef::Child(child_index) => self.components[index].children
                [child_index]
                .target
                .popup_parent_surface
                .is_some(),
        }
    }

    pub(in crate::shell) fn set_surface_visibility_now(
        &mut self,
        surface_id: SurfaceId,
        visible: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if !visible {
            if let Some(index) = self
                .components
                .iter()
                .position(|runtime| runtime.surface_id == surface_id.as_str())
            {
                {
                    let runtime = &mut self.components[index];
                    runtime.component.set_keyboard_mode_override(None);
                    runtime.component.set_surface_exiting(false);
                    if runtime.parent.popup_parent_surface.is_some() {
                        // Tear down the xdg_popup surface so it can be recreated
                        // fresh on the next ActivatePopover call.
                        runtime.parent.popup_parent_surface = None;
                        runtime.parent.popup_config = None;
                        runtime.parent.last_popup_size = None;
                        runtime.component.set_popup_promoted(false);
                        self.presentation_engine.destroy_popup(&surface_id);
                        tracing::info!(
                            "set_surface_visibility_now: destroyed xdg_popup for {surface_id}"
                        );
                    }
                }
                self.destroy_all_child_surfaces(index);
            }
            if let Some(previous_mode) = self.transfer_owned_keyboard_modes.remove(&surface_id) {
                if let Some(surface) = self.surfaces.get_mut(&surface_id) {
                    surface.keyboard_mode = previous_mode;
                }
            }
        }
        if surface_id == DEBUG_INSPECTOR_SURFACE_ID {
            self.debug.enabled = visible;
        }
        self.core
            .surfaces
            .entry(surface_id.clone())
            .and_modify(|state| {
                state.visible = visible;
                state.closing_until = None;
            })
            .or_insert(SurfaceState {
                visible,
                closing_until: None,
            });
        if !visible && self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
            self.keyboard_focus_surface = None;
        }

        self.broadcast_core_event(CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        })
    }

    pub(in crate::shell) fn complete_due_surface_transitions(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let now = std::time::Instant::now();
        let due_popovers: Vec<_> = self
            .pending_popover_hides
            .iter()
            .filter_map(|(surface_id, hide_at)| (*hide_at <= now).then(|| surface_id.clone()))
            .collect();
        let due: Vec<_> = self
            .core
            .surfaces
            .iter()
            .filter_map(|(surface_id, state)| {
                state
                    .closing_until
                    .is_some_and(|until| until <= now)
                    .then(|| surface_id.clone())
            })
            .collect();
        let mut emitted = VecDeque::new();
        for surface_id in due_popovers {
            self.pending_popover_hides.remove(&surface_id);
            if let Some(mut requests) = self.child_popover_pointer_leave_requests(&surface_id)? {
                self.drain_requests(&mut requests)?;
                emitted.extend(requests);
            } else {
                emitted.extend(self.set_surface_visibility(surface_id, false)?);
            }
        }
        for surface_id in due {
            emitted.extend(self.set_surface_visibility_now(surface_id, false)?);
        }
        Ok(emitted)
    }
}

fn profiling_trigger_for_request(request: &CoreRequest) -> &'static str {
    match request {
        CoreRequest::PositionSurface { .. } => "position_surface",
        CoreRequest::ToggleSurface { .. } => "toggle_surface",
        CoreRequest::ShowSurface { .. } => "show_surface",
        CoreRequest::HideSurface { .. } => "hide_surface",
        CoreRequest::HidePopover { .. } => "hide_popover",
        CoreRequest::PublishDiagnostics { .. } => "publish_diagnostics",
        CoreRequest::ServiceCommand { .. } => "service_command",
        CoreRequest::WriteClipboard { .. } => "write_clipboard",
        CoreRequest::SetTheme { .. } => "set_theme",
        CoreRequest::SetLocale { .. } => "set_locale",
        CoreRequest::ActivatePopover { .. } => "activate_popover",
        CoreRequest::TransferTabFocus { .. } => "transfer_tab_focus",
        CoreRequest::ToggleDebugOverlay => "toggle_debug_overlay",
        CoreRequest::ToggleDebugLayoutBounds => "toggle_debug_layout_bounds",
        CoreRequest::ToggleDebugProfiling => "toggle_debug_profiling",
        CoreRequest::RunDebugBenchmark { .. } => "run_debug_benchmark",
        CoreRequest::CycleDebugTab => "cycle_debug_tab",
        CoreRequest::Shutdown => "shutdown",
    }
}

fn point_in_rect(x: f32, y: f32, rect: (i32, i32, i32, i32)) -> bool {
    let (left, top, width, height) = rect;
    let right = left.saturating_add(width.max(0));
    let bottom = top.saturating_add(height.max(0));
    x >= left as f32 && x < right as f32 && y >= top as f32 && y < bottom as f32
}

fn benchmark_scenario_id(scenario_id: &str) -> Option<BenchmarkScenarioId> {
    match scenario_id {
        "hover" => Some(BenchmarkScenarioId::Hover),
        "surface_open_close" => Some(BenchmarkScenarioId::SurfaceOpenClose),
        "pointer_update" => Some(BenchmarkScenarioId::PointerUpdate),
        "keyboard_traversal" => Some(BenchmarkScenarioId::KeyboardTraversal),
        "backend_update" => Some(BenchmarkScenarioId::BackendUpdate),
        _ => None,
    }
}
