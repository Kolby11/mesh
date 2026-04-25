use mesh_scripting::{PublishedEvent, ScriptState};
use super::types::CoreRequest;

/// Seed a component's script state with default values before the first
/// service update arrives. This prevents template crashes on first render.
pub(super) fn seed_service_state(state: &mut ScriptState) {
    state.set(
        "last_service_update",
        serde_json::json!({ "name": "", "source_plugin": "" }),
    );
    // Default unavailable stub — replaced by the audio backend plugin's first emission.
    state.set(
        "audio",
        serde_json::json!({
            "available": false,
            "percent": 0,
            "label": "Unavailable",
            "glyph": "VOL",
            "icon_name": "audio-volume-muted",
            "source_plugin": "",
        }),
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
    let mut payload = payload;
    if has_read && service_name_from_interface(service) == "audio" {
        if let Some(obj) = payload.as_object_mut() {
            let percent = obj.get("percent").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let muted = obj.get("muted").and_then(|v| v.as_bool()).unwrap_or(false);
            let icon_name = audio_icon_name(percent, muted);
            obj.entry("icon_name")
                .or_insert_with(|| serde_json::Value::String(icon_name.to_string()));
        }
    }

    let service_name = service_name_from_interface(service);
    state.set(
        "last_service_update",
        serde_json::json!({ "name": service_name, "source_plugin": source_plugin }),
    );
    if has_read {
        state.set(service_name, payload);
    }
}

fn audio_icon_name(percent: u32, muted: bool) -> &'static str {
    if muted || percent == 0 {
        "audio-volume-muted"
    } else if percent < 34 {
        "audio-volume-low"
    } else if percent < 67 {
        "audio-volume-medium"
    } else {
        "audio-volume-high"
    }
}

pub(super) fn service_name_from_interface(interface: &str) -> String {
    interface
        .strip_prefix("mesh.")
        .unwrap_or(interface)
        .to_string()
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
            other => other.rfind('.').map(|pos| CoreRequest::ServiceCommand {
                interface: other[..pos].to_string(),
                command: other[pos + 1..].to_string(),
                payload: event.payload,
            }),
        })
        .collect()
}
