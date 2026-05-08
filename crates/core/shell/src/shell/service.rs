use super::types::CoreRequest;
use mesh_core_capability::Capability;
use mesh_core_scripting::{PublishedEvent, ScriptState};

/// Seed a component's script state with default values before the first
/// service update arrives. This prevents template crashes on first render.
pub(super) fn seed_service_state(state: &mut ScriptState) {
    state.set(
        "last_service_update",
        serde_json::json!({ "name": "", "source_module": "" }),
    );
}

/// Apply a service update payload into a component's script state.
///
/// The payload is set directly as `state[service_name]` — no parsing in core.
/// `has_read` must be pre-computed by the caller from the component's capability set
/// (`service.<name>.read`) to avoid simultaneous mutable/immutable borrows.
pub(super) fn apply_service_update(
    state: &mut ScriptState,
    has_read: bool,
    service: &str,
    source_module: &str,
    payload: serde_json::Value,
) {
    let service_name = service_name_from_interface(service);
    if has_read {
        state.set(
            "last_service_update",
            serde_json::json!({ "name": service_name, "source_module": source_module }),
        );
        state.set(service_name, payload);
    }
}

pub(super) fn service_name_from_interface(interface: &str) -> String {
    interface
        .strip_prefix("mesh.")
        .unwrap_or(interface)
        .to_string()
}

pub(super) fn service_command_control_capability(interface: &str) -> Capability {
    Capability::new(format!(
        "service.{}.control",
        service_name_from_interface(interface)
    ))
}

pub(super) fn script_events_to_requests(events: Vec<PublishedEvent>) -> Vec<CoreRequest> {
    events
        .into_iter()
        .filter_map(|event| match event.channel.as_str() {
            "shell.show-surface" => event
                .payload
                .get("surface_id")
                .and_then(|v| v.as_str())
                .map(|id| CoreRequest::ShowSurface {
                    surface_id: id.to_string(),
                }),
            "shell.hide-surface" => event
                .payload
                .get("surface_id")
                .and_then(|v| v.as_str())
                .map(|id| CoreRequest::HideSurface {
                    surface_id: id.to_string(),
                }),
            "shell.toggle-surface" => event
                .payload
                .get("surface_id")
                .and_then(|v| v.as_str())
                .map(|id| CoreRequest::ToggleSurface {
                    surface_id: id.to_string(),
                }),
            "shell.position-surface" => {
                let surface_id = event.payload.get("surface_id").and_then(|v| v.as_str())?;
                let margin_top = event
                    .payload
                    .get("margin_top")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                let margin_left = event
                    .payload
                    .get("margin_left")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                Some(CoreRequest::PositionSurface {
                    surface_id: surface_id.to_string(),
                    margin_top,
                    margin_left,
                })
            }
            "shell.activate-popover" => {
                let surface_id = event.payload.get("surface_id").and_then(|v| v.as_str())?;
                let trigger_surface = event
                    .payload
                    .get("trigger_surface")
                    .and_then(|v| v.as_str())?;
                let trigger_key = event.payload.get("trigger_key").and_then(|v| v.as_str())?;
                let focus = event
                    .payload
                    .get("focus")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                Some(CoreRequest::ActivatePopover {
                    surface_id: surface_id.to_string(),
                    trigger_surface: trigger_surface.to_string(),
                    trigger_key: trigger_key.to_string(),
                    focus,
                })
            }
            "shell.set-theme" => event
                .payload
                .get("theme_id")
                .and_then(|v| v.as_str())
                .map(|id| CoreRequest::SetTheme {
                    theme_id: id.to_string(),
                }),
            "shell.toggle-debug-overlay" => Some(CoreRequest::ToggleDebugOverlay),
            "shell.toggle-debug-profiling" => Some(CoreRequest::ToggleDebugProfiling),
            other => other.rfind('.').map(|pos| {
                let interface = other[..pos].to_string();
                let command = other[pos + 1..].to_string();
                let required = service_command_control_capability(&interface);
                if event.source_capabilities.is_granted(&required) {
                    CoreRequest::ServiceCommand {
                        interface,
                        command,
                        payload: event.payload,
                        source_module_id: event.source_module_id,
                        source_capabilities: event.source_capabilities,
                    }
                } else {
                    tracing::warn!(
                        source_module_id = %event.source_module_id,
                        required_capability = %required,
                        channel = %event.channel,
                        "denied frontend service command publication"
                    );
                    CoreRequest::PublishDiagnostics {
                        message: format!(
                            "Denied service command '{}' from '{}' without {}",
                            event.channel, event.source_module_id, required
                        ),
                    }
                }
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::VariableStore;

    #[test]
    fn apply_service_update_does_not_leak_metadata_without_read_capability() {
        let mut state = ScriptState::new();
        seed_service_state(&mut state);

        apply_service_update(
            &mut state,
            false,
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "percent": 42 }),
        );

        assert_eq!(
            state.get("last_service_update"),
            Some(serde_json::json!({ "name": "", "source_module": "" }))
        );
        assert_eq!(state.get("audio"), None);
    }

    #[test]
    fn script_events_to_requests_maps_named_proxy_commands() {
        let mut audio_caps = mesh_core_capability::CapabilitySet::new();
        audio_caps.grant(Capability::new("service.audio.control"));
        let mut network_caps = mesh_core_capability::CapabilitySet::new();
        network_caps.grant(Capability::new("service.network.control"));
        let requests = script_events_to_requests(vec![
            PublishedEvent {
                channel: "mesh.audio.set_volume".into(),
                payload: serde_json::json!({ "percent": 55 }),
                source_module_id: "@mesh/quick-settings".into(),
                source_capabilities: audio_caps,
            },
            PublishedEvent {
                channel: "mesh.network.set_wifi_enabled".into(),
                payload: serde_json::json!({ "enabled": true }),
                source_module_id: "@mesh/quick-settings".into(),
                source_capabilities: network_caps,
            },
        ]);

        assert_eq!(requests.len(), 2);
        match &requests[0] {
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                source_module_id,
                source_capabilities,
            } => {
                assert_eq!(interface, "mesh.audio");
                assert_eq!(command, "set_volume");
                assert_eq!(payload, &serde_json::json!({ "percent": 55 }));
                assert_eq!(source_module_id, "@mesh/quick-settings");
                assert!(source_capabilities.is_granted(&Capability::new("service.audio.control")));
            }
            other => panic!("expected audio ServiceCommand, got {other:?}"),
        }
        match &requests[1] {
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                source_module_id,
                source_capabilities,
            } => {
                assert_eq!(interface, "mesh.network");
                assert_eq!(command, "set_wifi_enabled");
                assert_eq!(payload, &serde_json::json!({ "enabled": true }));
                assert_eq!(source_module_id, "@mesh/quick-settings");
                assert!(
                    source_capabilities.is_granted(&Capability::new("service.network.control"))
                );
            }
            other => panic!("expected network ServiceCommand, got {other:?}"),
        }
    }

    #[test]
    fn script_events_to_requests_maps_popover_focus_option() {
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "shell.activate-popover".into(),
            payload: serde_json::json!({
                "surface_id": "@mesh/audio-popover",
                "trigger_surface": "@mesh/navigation-bar",
                "trigger_key": "volume-button",
                "focus": false,
            }),
            source_module_id: "@mesh/navigation-bar".into(),
            source_capabilities: mesh_core_capability::CapabilitySet::new(),
        }]);

        match requests.as_slice() {
            [
                CoreRequest::ActivatePopover {
                    surface_id,
                    trigger_surface,
                    trigger_key,
                    focus,
                },
            ] => {
                assert_eq!(surface_id, "@mesh/audio-popover");
                assert_eq!(trigger_surface, "@mesh/navigation-bar");
                assert_eq!(trigger_key, "volume-button");
                assert!(!focus);
            }
            other => panic!("expected ActivatePopover request, got {other:?}"),
        }
    }

    #[test]
    fn script_events_to_requests_denies_uncontrolled_service_command() {
        let mut caps = mesh_core_capability::CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "mesh.audio.set_volume".into(),
            payload: serde_json::json!({ "percent": 55 }),
            source_module_id: "@mesh/panel".into(),
            source_capabilities: caps,
        }]);

        match requests.as_slice() {
            [CoreRequest::PublishDiagnostics { message }] => {
                assert!(message.contains("service.audio.control"));
            }
            other => panic!("expected denied diagnostic request, got {other:?}"),
        }
    }

    #[test]
    fn script_events_to_requests_maps_debug_control_events() {
        let requests = script_events_to_requests(vec![
            PublishedEvent {
                channel: "shell.toggle-debug-overlay".into(),
                payload: serde_json::json!({}),
                source_module_id: "@mesh/debug-inspector".into(),
                source_capabilities: mesh_core_capability::CapabilitySet::new(),
            },
            PublishedEvent {
                channel: "shell.toggle-debug-profiling".into(),
                payload: serde_json::json!({}),
                source_module_id: "@mesh/debug-inspector".into(),
                source_capabilities: mesh_core_capability::CapabilitySet::new(),
            },
        ]);

        assert!(matches!(
            requests.first(),
            Some(CoreRequest::ToggleDebugOverlay)
        ));
        assert!(matches!(
            requests.get(1),
            Some(CoreRequest::ToggleDebugProfiling)
        ));
    }
}
