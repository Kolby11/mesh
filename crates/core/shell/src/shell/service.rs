use super::types::CoreRequest;
use mesh_core_capability::Capability;
use mesh_core_scripting::{PublishedEvent, ScriptState};
pub(super) use mesh_core_service::service_name_from_interface;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// Bundle of interned capability values derived from an interface name.
///
/// The shell does a per-service-event capability check on every component
/// runtime. Constructing the three `Capability` values from formatted
/// strings showed up in profiling as a hot allocation. This struct lets the
/// caller compute them once per interface and pass borrowed refs through
/// the inner loop.
pub(super) struct ServiceCapabilities {
    pub service_name: String,
    pub read: Capability,
    pub theme: Option<Capability>,
    pub locale: Option<Capability>,
}

/// Get (or build) the interned capability bundle for a given interface.
///
/// Returns `Arc<ServiceCapabilities>` so the lock is released before the
/// caller iterates over runtimes. The set of interfaces is bounded and
/// stable in steady state, so the cache does not need eviction.
pub(super) fn service_capabilities(interface: &str) -> Arc<ServiceCapabilities> {
    static CACHE: OnceLock<RwLock<HashMap<String, Arc<ServiceCapabilities>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    if let Ok(guard) = cache.read() {
        if let Some(entry) = guard.get(interface) {
            return Arc::clone(entry);
        }
    }

    let service_name = service_name_from_interface(interface);
    let entry = Arc::new(ServiceCapabilities {
        read: Capability::new(format!("service.{service_name}.read")),
        theme: (interface == "mesh.theme").then(|| Capability::new("theme.read")),
        locale: (interface == "mesh.locale").then(|| Capability::new("locale.read")),
        service_name,
    });

    if let Ok(mut guard) = cache.write() {
        guard
            .entry(interface.to_string())
            .or_insert_with(|| Arc::clone(&entry));
    }

    entry
}

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
    payload: impl Borrow<serde_json::Value>,
) {
    let service_name = service_name_from_interface(service);
    if has_read {
        state.set(
            "last_service_update",
            serde_json::json!({ "name": service_name, "source_module": source_module }),
        );
        state.set(service_name, payload.borrow().clone());
    }
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
            "shell.hide-popover" => {
                let surface_id = event.payload.get("surface_id").and_then(|v| v.as_str())?;
                let defer_for_hover_bridge = event
                    .payload
                    .get("defer_for_hover_bridge")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Some(CoreRequest::HidePopover {
                    surface_id: surface_id.to_string(),
                    defer_for_hover_bridge,
                })
            }
            "shell.toggle-surface" => event
                .payload
                .get("surface_id")
                .and_then(|v| v.as_str())
                .map(|id| CoreRequest::ToggleSurface {
                    surface_id: id.to_string(),
                }),
            "shell.position-surface" => {
                let surface_id = event.payload.get("surface_id").and_then(|v| v.as_str())?;
                let margin_top = payload_i32(&event.payload, "margin_top").unwrap_or(0);
                let margin_left = payload_i32(&event.payload, "margin_left").unwrap_or(0);
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
            "shell.set-locale" => {
                event
                    .payload
                    .get("locale")
                    .and_then(|v| v.as_str())
                    .map(|locale| CoreRequest::SetLocale {
                        locale: locale.to_string(),
                    })
            }
            "shell.toggle-debug-overlay" => Some(CoreRequest::ToggleDebugOverlay),
            "shell.toggle-debug-layout-bounds" => Some(CoreRequest::ToggleDebugLayoutBounds),
            "shell.toggle-debug-profiling" => Some(CoreRequest::ToggleDebugProfiling),
            "shell.run-debug-benchmark" => {
                match event.payload.get("scenario_id").and_then(|v| v.as_str()) {
                    Some(scenario_id) if !scenario_id.is_empty() => {
                        Some(CoreRequest::RunDebugBenchmark {
                            scenario_id: scenario_id.to_string(),
                        })
                    }
                    _ => Some(CoreRequest::PublishDiagnostics {
                        message: "debug benchmark request missing scenario_id".to_string(),
                    }),
                }
            }
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

fn payload_i32(payload: &serde_json::Value, key: &str) -> Option<i32> {
    payload
        .get(key)
        .and_then(|value| value.as_i64())
        .and_then(|value| i32::try_from(value).ok())
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
    fn script_events_to_requests_maps_popover_hover_bridge_hide() {
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "shell.hide-popover".into(),
            payload: serde_json::json!({
                "surface_id": "@mesh/quick-settings",
                "defer_for_hover_bridge": true,
            }),
            source_module_id: "@mesh/quick-settings".into(),
            source_capabilities: mesh_core_capability::CapabilitySet::new(),
        }]);

        match requests.as_slice() {
            [
                CoreRequest::HidePopover {
                    surface_id,
                    defer_for_hover_bridge,
                },
            ] => {
                assert_eq!(surface_id, "@mesh/quick-settings");
                assert!(*defer_for_hover_bridge);
            }
            other => panic!("expected HidePopover request, got {other:?}"),
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
                channel: "shell.toggle-debug-layout-bounds".into(),
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
            Some(CoreRequest::ToggleDebugLayoutBounds)
        ));
        assert!(matches!(
            requests.get(2),
            Some(CoreRequest::ToggleDebugProfiling)
        ));
    }

    #[test]
    fn script_events_to_requests_keeps_position_margins_in_i32_range() {
        let requests = script_events_to_requests(vec![
            PublishedEvent {
                channel: "shell.position-surface".into(),
                payload: serde_json::json!({
                    "surface_id": "@mesh/popover",
                    "margin_top": i64::MAX,
                    "margin_left": i64::MIN,
                }),
                source_module_id: "@mesh/navigation-bar".into(),
                source_capabilities: mesh_core_capability::CapabilitySet::new(),
            },
            PublishedEvent {
                channel: "shell.position-surface".into(),
                payload: serde_json::json!({
                    "surface_id": "@mesh/popover",
                    "margin_top": "bad",
                    "margin_left": 24,
                }),
                source_module_id: "@mesh/navigation-bar".into(),
                source_capabilities: mesh_core_capability::CapabilitySet::new(),
            },
        ]);

        match &requests[0] {
            CoreRequest::PositionSurface {
                margin_top,
                margin_left,
                ..
            } => {
                assert_eq!(*margin_top, 0);
                assert_eq!(*margin_left, 0);
            }
            other => panic!("expected PositionSurface, got {other:?}"),
        }
        match &requests[1] {
            CoreRequest::PositionSurface {
                margin_top,
                margin_left,
                ..
            } => {
                assert_eq!(*margin_top, 0);
                assert_eq!(*margin_left, 24);
            }
            other => panic!("expected PositionSurface, got {other:?}"),
        }
    }
}
