use super::types::CoreRequest;
use mesh_core_capability::Capability;
use mesh_core_scripting::{PublishedEvent, ScriptState};

/// Seed a component's script state with default values before the first
/// service update arrives. This prevents template crashes on first render.
pub(super) fn seed_service_state(state: &mut ScriptState) {
    state.set(
        "last_service_update",
        serde_json::json!({ "name": "", "source_plugin": "" }),
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
    source_plugin: &str,
    payload: serde_json::Value,
) {
    let service_name = service_name_from_interface(service);
    if has_read {
        state.set(
            "last_service_update",
            serde_json::json!({ "name": service_name, "source_plugin": source_plugin }),
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
            "shell.set-theme" => event
                .payload
                .get("theme_id")
                .and_then(|v| v.as_str())
                .map(|id| CoreRequest::SetTheme {
                    theme_id: id.to_string(),
                }),
            other => other.rfind('.').map(|pos| {
                let interface = other[..pos].to_string();
                let command = other[pos + 1..].to_string();
                let required = service_command_control_capability(&interface);
                if event.source_capabilities.is_granted(&required) {
                    CoreRequest::ServiceCommand {
                        interface,
                        command,
                        payload: event.payload,
                        source_plugin_id: event.source_plugin_id,
                        source_capabilities: event.source_capabilities,
                    }
                } else {
                    tracing::warn!(
                        source_plugin_id = %event.source_plugin_id,
                        required_capability = %required,
                        channel = %event.channel,
                        "denied frontend service command publication"
                    );
                    CoreRequest::PublishDiagnostics {
                        message: format!(
                            "Denied service command '{}' from '{}' without {}",
                            event.channel, event.source_plugin_id, required
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
            Some(serde_json::json!({ "name": "", "source_plugin": "" }))
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
                source_plugin_id: "@mesh/quick-settings".into(),
                source_capabilities: audio_caps,
            },
            PublishedEvent {
                channel: "mesh.network.set_wifi_enabled".into(),
                payload: serde_json::json!({ "enabled": true }),
                source_plugin_id: "@mesh/quick-settings".into(),
                source_capabilities: network_caps,
            },
        ]);

        assert_eq!(requests.len(), 2);
        match &requests[0] {
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                source_plugin_id,
                source_capabilities,
            } => {
                assert_eq!(interface, "mesh.audio");
                assert_eq!(command, "set_volume");
                assert_eq!(payload, &serde_json::json!({ "percent": 55 }));
                assert_eq!(source_plugin_id, "@mesh/quick-settings");
                assert!(source_capabilities.is_granted(&Capability::new("service.audio.control")));
            }
            other => panic!("expected audio ServiceCommand, got {other:?}"),
        }
        match &requests[1] {
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                source_plugin_id,
                source_capabilities,
            } => {
                assert_eq!(interface, "mesh.network");
                assert_eq!(command, "set_wifi_enabled");
                assert_eq!(payload, &serde_json::json!({ "enabled": true }));
                assert_eq!(source_plugin_id, "@mesh/quick-settings");
                assert!(
                    source_capabilities.is_granted(&Capability::new("service.network.control"))
                );
            }
            other => panic!("expected network ServiceCommand, got {other:?}"),
        }
    }

    #[test]
    fn script_events_to_requests_denies_uncontrolled_service_command() {
        let mut caps = mesh_core_capability::CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "mesh.audio.set_volume".into(),
            payload: serde_json::json!({ "percent": 55 }),
            source_plugin_id: "@mesh/panel".into(),
            source_capabilities: caps,
        }]);

        match requests.as_slice() {
            [CoreRequest::PublishDiagnostics { message }] => {
                assert!(message.contains("service.audio.control"));
            }
            other => panic!("expected denied diagnostic request, got {other:?}"),
        }
    }
}
