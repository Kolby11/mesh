use super::super::*;

impl Shell {
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
}
