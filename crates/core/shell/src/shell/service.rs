use super::types::CoreRequest;
use mesh_core_capability::Capability;
use mesh_core_scripting::{PublishedEvent, ScriptState};
pub(super) use mesh_core_service::service_name_from_interface;
pub(super) use mesh_core_service::service_name_from_interface_cow;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
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
    pub control: Capability,
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
        control: Capability::new(format!("service.{service_name}.control")),
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
    let service_name = service_name_from_interface_cow(service);
    apply_service_update_with_name(
        state,
        has_read,
        service_name.as_ref(),
        source_module,
        payload,
    );
}

pub(super) fn apply_service_update_with_name(
    state: &mut ScriptState,
    has_read: bool,
    service_name: &str,
    source_module: &str,
    payload: impl Borrow<serde_json::Value>,
) {
    if has_read {
        state.set(
            "last_service_update",
            serde_json::json!({ "name": service_name, "source_module": source_module }),
        );
        state.set(service_name, payload.borrow().clone());
    }
}

pub(super) fn apply_service_update_with_name_and_fingerprint(
    state: &mut ScriptState,
    has_read: bool,
    service_name: &str,
    source_module: &str,
    payload: &serde_json::Value,
    fingerprint: u64,
) {
    if has_read {
        let metadata_fingerprint = service_update_metadata_fingerprint(service_name, source_module);
        state.set_with_fingerprint_lazy(
            "last_service_update",
            metadata_fingerprint,
            || serde_json::json!({ "name": service_name, "source_module": source_module }),
        );
        state.set_with_fingerprint(service_name, payload, fingerprint);
    }
}

fn service_update_metadata_fingerprint(service_name: &str, source_module: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    service_name.hash(&mut hasher);
    source_module.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
pub(super) fn service_command_control_capability(interface: &str) -> Capability {
    service_capabilities(interface).control.clone()
}

pub(super) fn script_events_to_requests(events: Vec<PublishedEvent>) -> Vec<CoreRequest> {
    events
        .into_iter()
        .filter_map(script_event_to_request)
        .collect()
}

fn script_event_to_request(event: PublishedEvent) -> Option<CoreRequest> {
    match event.channel.as_str() {
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
        "shell.set-locale" => event
            .payload
            .get("locale")
            .and_then(|v| v.as_str())
            .map(|locale| CoreRequest::SetLocale {
                locale: locale.to_string(),
            }),
        "shell.set-provider" if event.source_module_id == "@mesh/settings" => {
            let interface = event
                .payload
                .get("interface")
                .and_then(|value| value.as_str())?;
            let provider_id = event
                .payload
                .get("provider_id")
                .and_then(|value| value.as_str())?;
            Some(CoreRequest::SetProvider {
                interface: interface.to_string(),
                provider_id: provider_id.to_string(),
            })
        }
        "shell.toggle-debug-overlay" => Some(CoreRequest::ToggleDebugOverlay),
        "shell.toggle-debug-layout-bounds" => Some(CoreRequest::ToggleDebugLayoutBounds),
        "shell.toggle-debug-element-picker" => Some(CoreRequest::ToggleDebugElementPicker),
        "shell.open-debug-source" if event.source_module_id == "@mesh/debug-inspector" => {
            let path = event.payload.get("path").and_then(|value| value.as_str())?;
            let line = event
                .payload
                .get("line")
                .and_then(|value| value.as_u64())
                .unwrap_or(1)
                .clamp(1, u64::from(u32::MAX)) as u32;
            Some(CoreRequest::OpenDebugSource {
                path: path.to_string(),
                line,
            })
        }
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
        other if other.starts_with("shell.") => {
            tracing::warn!(
                source_module_id = %event.source_module_id,
                channel = %event.channel,
                "unknown shell channel published by frontend module"
            );
            Some(CoreRequest::PublishDiagnostics {
                message: format!(
                    "Unknown shell channel '{}' published by '{}'; shell.* is reserved for core requests and service commands use mesh.<interface>.<command> channels",
                    event.channel, event.source_module_id
                ),
            })
        }
        other => other.rfind('.').map(|pos| {
            let interface_name = &other[..pos];
            let capabilities = service_capabilities(interface_name);
            let required = &capabilities.control;
            if event.source_capabilities.is_granted(required) {
                CoreRequest::ServiceCommand {
                    interface: interface_name.to_string(),
                    command: other[pos + 1..].to_string(),
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
    }
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
    fn settings_provider_event_maps_to_provider_selection_request() {
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "shell.set-provider".into(),
            payload: serde_json::json!({
                "interface": "mesh.audio",
                "provider_id": "@mesh/pulseaudio-audio",
            }),
            source_module_id: "@mesh/settings".into(),
            source_capabilities: Default::default(),
        }]);

        assert!(matches!(
            requests.as_slice(),
            [CoreRequest::SetProvider { interface, provider_id }]
                if interface == "mesh.audio" && provider_id == "@mesh/pulseaudio-audio"
        ));
    }

    #[test]
    fn non_settings_module_cannot_select_provider() {
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "shell.set-provider".into(),
            payload: serde_json::json!({
                "interface": "mesh.audio",
                "provider_id": "@mesh/pulseaudio-audio",
            }),
            source_module_id: "@mesh/navigation-bar".into(),
            source_capabilities: Default::default(),
        }]);

        assert!(matches!(
            requests.as_slice(),
            [CoreRequest::PublishDiagnostics { message }]
                if message.contains("shell.set-provider")
        ));
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
    fn script_events_to_requests_rejects_unknown_shell_channels() {
        let mut caps = mesh_core_capability::CapabilitySet::new();
        caps.grant(Capability::new("service.shell.control"));
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "shell.brightness-down".into(),
            payload: serde_json::json!({ "step": 10 }),
            source_module_id: "@mesh/navigation-bar".into(),
            source_capabilities: caps,
        }]);

        match requests.as_slice() {
            [CoreRequest::PublishDiagnostics { message }] => {
                assert!(message.contains("Unknown shell channel"));
                assert!(message.contains("shell.brightness-down"));
            }
            other => {
                panic!("unknown shell.* channels must never become service commands, got {other:?}")
            }
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
                channel: "shell.toggle-debug-element-picker".into(),
                payload: serde_json::json!({}),
                source_module_id: "@mesh/debug-inspector".into(),
                source_capabilities: mesh_core_capability::CapabilitySet::new(),
            },
            PublishedEvent {
                channel: "shell.open-debug-source".into(),
                payload: serde_json::json!({ "path": "/tmp/example.mesh", "line": 42 }),
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
            Some(CoreRequest::ToggleDebugElementPicker)
        ));
        assert!(matches!(
            requests.get(3),
            Some(CoreRequest::OpenDebugSource { path, line })
                if path == "/tmp/example.mesh" && *line == 42
        ));
        assert!(matches!(
            requests.get(4),
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

    #[test]
    fn service_capabilities_include_control_capability() {
        let caps = service_capabilities("mesh.audio");

        assert_eq!(caps.service_name, "audio");
        assert_eq!(caps.read.id(), "service.audio.read");
        assert_eq!(caps.control.id(), "service.audio.control");
    }

    #[test]
    fn fingerprinted_service_metadata_preserves_update_shape() {
        let payload = serde_json::json!({ "available": true });
        let fingerprint = mesh_core_scripting::ScriptContext::service_payload_fingerprint(&payload);
        let mut state = ScriptState::new();

        apply_service_update_with_name_and_fingerprint(
            &mut state,
            true,
            "audio",
            "@mesh/pipewire",
            &payload,
            fingerprint,
        );

        assert_eq!(state.get("audio"), Some(payload));
        assert_eq!(
            state.get("last_service_update"),
            Some(serde_json::json!({
                "name": "audio",
                "source_module": "@mesh/pipewire"
            }))
        );
    }

    // cargo test -p mesh-core-shell --release -- lazy_service_update_metadata_beats_json_rebuild --ignored --nocapture
    #[test]
    #[ignore = "release-only service update metadata microbenchmark"]
    fn lazy_service_update_metadata_beats_json_rebuild() {
        let payload = serde_json::json!({ "available": true });
        let fingerprint = mesh_core_scripting::ScriptContext::service_payload_fingerprint(&payload);
        let iterations = 200_000usize;

        let mut rebuilt = ScriptState::new();
        apply_service_update_with_name(&mut rebuilt, true, "audio", "@mesh/pipewire", &payload);
        rebuilt.clear_dirty();
        let rebuilt_started = std::time::Instant::now();
        for _ in 0..iterations {
            apply_service_update_with_name(
                &mut rebuilt,
                true,
                "audio",
                "@mesh/pipewire",
                std::hint::black_box(&payload),
            );
        }
        let rebuilt_time = rebuilt_started.elapsed();

        let mut lazy = ScriptState::new();
        apply_service_update_with_name_and_fingerprint(
            &mut lazy,
            true,
            "audio",
            "@mesh/pipewire",
            &payload,
            fingerprint,
        );
        lazy.clear_dirty();
        let lazy_started = std::time::Instant::now();
        for _ in 0..iterations {
            apply_service_update_with_name_and_fingerprint(
                &mut lazy,
                true,
                "audio",
                "@mesh/pipewire",
                std::hint::black_box(&payload),
                fingerprint,
            );
        }
        let lazy_time = lazy_started.elapsed();

        eprintln!(
            "unchanged service update over {iterations} writes: rebuild metadata {rebuilt_time:?}; lazy fingerprint {lazy_time:?}; ratio {:.2}x",
            rebuilt_time.as_secs_f64() / lazy_time.as_secs_f64()
        );
        assert_eq!(rebuilt.snapshot(), lazy.snapshot());
        assert!(!rebuilt.is_dirty());
        assert!(!lazy.is_dirty());
        assert!(lazy_time < rebuilt_time);
    }

    // cargo test -p mesh-core-shell --release -- cached_service_control_capability_avoids_formatting --ignored --nocapture
    #[test]
    #[ignore = "release-only service command capability microbenchmark"]
    fn cached_service_control_capability_avoids_formatting() {
        use std::hint::black_box;
        use std::time::Instant;

        fn old_control_capability(interface: &str) -> mesh_core_capability::Capability {
            mesh_core_capability::Capability::new(format!(
                "service.{}.control",
                service_name_from_interface(interface)
            ))
        }

        let iterations = 1_000_000usize;
        let interface = "mesh.audio";

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let cap = old_control_capability(black_box(interface));
            old_total = old_total.wrapping_add(cap.id().len());
        }
        let old_time = old_started.elapsed();

        let cached_started = Instant::now();
        let mut cached_total = 0usize;
        for _ in 0..iterations {
            let cap = service_command_control_capability(black_box(interface));
            cached_total = cached_total.wrapping_add(cap.id().len());
        }
        let cached_time = cached_started.elapsed();

        eprintln!(
            "service control capability over {iterations} iterations: format {old_time:?}; cached clone {cached_time:?}; ratio {:.1}x; totals={old_total}/{cached_total}",
            old_time.as_secs_f64() / cached_time.as_secs_f64()
        );
        assert_eq!(old_total, cached_total);
        assert!(cached_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- borrowed_service_control_capability_beats_cached_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only service command capability borrow microbenchmark"]
    fn borrowed_service_control_capability_beats_cached_clone() {
        use std::hint::black_box;
        use std::time::Instant;

        let iterations = 1_000_000usize;
        let interface = "mesh.audio";

        let clone_started = Instant::now();
        let mut clone_total = 0usize;
        for _ in 0..iterations {
            let cap = service_command_control_capability(black_box(interface));
            clone_total = clone_total.wrapping_add(cap.id().len());
        }
        let clone_time = clone_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_total = 0usize;
        for _ in 0..iterations {
            let caps = service_capabilities(black_box(interface));
            borrowed_total = borrowed_total.wrapping_add(caps.control.id().len());
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "service control capability over {iterations} iterations: cached clone {clone_time:?}; borrowed cached Arc {borrowed_time:?}; ratio {:.1}x; totals={clone_total}/{borrowed_total}",
            clone_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(clone_total, borrowed_total);
        assert!(borrowed_time < clone_time);
    }

    // cargo test -p mesh-core-shell --release -- borrowed_service_update_name_avoids_projection --ignored --nocapture
    #[test]
    #[ignore = "release-only service update name microbenchmark"]
    fn borrowed_service_update_name_avoids_projection() {
        use std::hint::black_box;
        use std::time::Instant;

        fn old_apply_service_update(
            state: &mut ScriptState,
            has_read: bool,
            service: &str,
            source_module: &str,
            payload: impl Borrow<serde_json::Value>,
        ) {
            let service_name = service_name_from_interface(service);
            apply_service_update_with_name(state, has_read, &service_name, source_module, payload);
        }

        let iterations = 200_000usize;
        let payload = serde_json::json!({ "available": true, "percent": 42, "muted": false });

        let projected_started = Instant::now();
        let mut projected_total = 0usize;
        for _ in 0..iterations {
            let mut state = ScriptState::new();
            old_apply_service_update(
                &mut state,
                true,
                black_box("mesh.audio"),
                "@mesh/pipewire-audio",
                black_box(&payload),
            );
            projected_total = projected_total.wrapping_add(
                state
                    .get("audio")
                    .and_then(|value| value.as_object().map(serde_json::Map::len))
                    .unwrap_or_default(),
            );
        }
        let projected_time = projected_started.elapsed();

        let cow_started = Instant::now();
        let mut cow_total = 0usize;
        for _ in 0..iterations {
            let mut state = ScriptState::new();
            apply_service_update(
                &mut state,
                true,
                black_box("mesh.audio"),
                "@mesh/pipewire-audio",
                black_box(&payload),
            );
            cow_total = cow_total.wrapping_add(
                state
                    .get("audio")
                    .and_then(|value| value.as_object().map(serde_json::Map::len))
                    .unwrap_or_default(),
            );
        }
        let cow_time = cow_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_total = 0usize;
        for _ in 0..iterations {
            let mut state = ScriptState::new();
            apply_service_update_with_name(
                &mut state,
                true,
                black_box("audio"),
                "@mesh/pipewire-audio",
                black_box(&payload),
            );
            borrowed_total = borrowed_total.wrapping_add(
                state
                    .get("audio")
                    .and_then(|value| value.as_object().map(serde_json::Map::len))
                    .unwrap_or_default(),
            );
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "service update state write over {iterations} iterations: owned-project {projected_time:?}; cow-project {cow_time:?}; borrowed-name {borrowed_time:?}; owned/cow {:.1}x; owned/borrowed {:.1}x; totals={projected_total}/{cow_total}/{borrowed_total}",
            projected_time.as_secs_f64() / cow_time.as_secs_f64(),
            projected_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(projected_total, cow_total);
        assert_eq!(projected_total, borrowed_total);
        assert!(cow_time < projected_time);
        assert!(borrowed_time < projected_time);
    }

    // cargo test -p mesh-core-shell --release -- denied_service_command_defers_command_allocation --ignored --nocapture
    #[test]
    #[ignore = "release-only denied service command allocation microbenchmark"]
    fn denied_service_command_defers_command_allocation() {
        use std::hint::black_box;
        use std::time::Instant;

        fn old_denied_command_request(event: PublishedEvent) -> Option<CoreRequest> {
            let other = event.channel.as_str();
            other.rfind('.').map(|pos| {
                let interface = other[..pos].to_string();
                let command = other[pos + 1..].to_string();
                black_box(&command);
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
                    CoreRequest::PublishDiagnostics {
                        message: format!(
                            "Denied service command '{}' from '{}' without {}",
                            event.channel, event.source_module_id, required
                        ),
                    }
                }
            })
        }

        let event = PublishedEvent {
            channel: "mesh.audio.set_volume".into(),
            payload: serde_json::json!({ "percent": 55 }),
            source_module_id: "@mesh/benchmark".into(),
            source_capabilities: mesh_core_capability::CapabilitySet::new(),
        };
        let iterations = 200_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            if let Some(CoreRequest::PublishDiagnostics { message }) =
                old_denied_command_request(black_box(event.clone()))
            {
                old_total += message.len();
            }
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            if let Some(CoreRequest::PublishDiagnostics { message }) =
                script_event_to_request(black_box(event.clone()))
            {
                new_total += message.len();
            }
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "denied service command over {iterations} events: eager command allocation {old_time:?}; deferred {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }
}
