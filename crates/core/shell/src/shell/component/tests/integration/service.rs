use super::*;
use crate::shell::service::script_events_to_requests;

#[test]
fn frontend_component_observes_only_subscribed_interface_events() {
    let component = test_frontend_component_with_catalog(
        r#"
<template>
  <box />
</template>
<script lang="luau">
function init()
    local audio = require("mesh.audio@>=1.0")
    audio.events.VolumeChanged:subscribe(function(_event) end)
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );

    assert!(ShellComponent::observes_service_event(
        &component,
        &ServiceEvent::InterfaceEvent {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            name: "VolumeChanged".into(),
            payload: serde_json::json!({ "device_id": "default", "level": 42 }),
        },
    ));
    assert!(!ShellComponent::observes_service_event(
        &component,
        &ServiceEvent::InterfaceEvent {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            name: "OtherEvent".into(),
            payload: serde_json::json!({}),
        },
    ));
    assert!(!ShellComponent::observes_service_event(
        &component,
        &ServiceEvent::InterfaceEvent {
            service: "mesh.network".into(),
            source_module: "@mesh/networkmanager-network".into(),
            name: "VolumeChanged".into(),
            payload: serde_json::json!({}),
        },
    ));
}

#[test]
fn frontend_component_keeps_service_updates_for_subscribed_event_services_only() {
    let subscribed = test_frontend_component_with_catalog(
        r#"
<template>
  <box />
</template>
<script lang="luau">
function init()
    local audio = require("mesh.audio@>=1.0")
    audio.events.VolumeChanged:subscribe(function(_event) end)
end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );
    let idle = test_frontend_component_with_catalog(
        r#"
<template>
  <box />
</template>
<script lang="luau">
function init() end
</script>
"#,
        audio_network_catalog(),
        &["service.audio.read"],
    );

    let audio_update = ServiceEvent::Updated {
        service: "mesh.audio".into(),
        source_module: "@mesh/pipewire-audio".into(),
        payload: serde_json::json!({ "percent": 42, "muted": false }),
    };

    assert!(ShellComponent::observes_service_event(
        &subscribed,
        &audio_update,
    ));
    assert!(!ShellComponent::observes_service_event(
        &idle,
        &audio_update
    ));
}

#[test]
fn frontend_proxy_update_reaches_panel_or_quick_settings_render_state() {
    let mut ctx = make_audio_ctx();
    ctx.load_script(
        r#"
-- Panel-style: read audio.percent and audio.muted directly on rerender.
volumeIcon = "audio-volume-muted"
volumeLevel = 0

function render()
    local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
    if not audio_ok or not audio then return end
    local pct = audio.percent or 0
    local muted = audio.muted or false
    volumeLevel = pct
    if muted or pct == 0 then
        volumeIcon = "audio-volume-muted"
    elseif pct < 34 then
        volumeIcon = "audio-volume-low"
    elseif pct < 67 then
        volumeIcon = "audio-volume-medium"
    else
        volumeIcon = "audio-volume-high"
    end
end
"#,
    )
    .unwrap();

    // Simulate a ServiceEvent::Updated payload arriving (as apply_service_payload does).
    ctx.apply_service_payload(
        "audio",
        &serde_json::json!({ "percent": 75, "muted": false }),
    );

    // The runtime calls the script's render handler on each rerender.
    ctx.call_handler("render", &[]).unwrap();

    // Verify that the template-visible reactive globals reflect the emitted payload,
    // proving rerender-visible service state without any callback registration.
    assert_eq!(
        ctx.state.get("volumeIcon"),
        Some(serde_json::json!("audio-volume-high")),
        "volumeIcon should be high for 75% unmuted"
    );
    assert_eq!(
        ctx.state.get("volumeLevel"),
        Some(serde_json::json!(75)),
        "volumeLevel should equal the emitted percent"
    );
    // Confirm the proxy read was tracked (needed for shell invalidation).
    let tracked = ctx.tracked_fields_for_service("audio");
    assert!(
        tracked.contains("percent"),
        "audio.percent should be in tracked fields"
    );
    assert!(
        tracked.contains("muted"),
        "audio.muted should be in tracked fields"
    );
}

// ---------- integration test 2: proxy command becomes ServiceCommand ----

/// Proves that a bundled control handler (e.g. quick-settings onToggleWiFi)
/// calling a named proxy command method publishes a `CoreRequest::ServiceCommand`
/// through the `script_events_to_requests` routing layer.
#[test]
fn frontend_proxy_command_from_bundled_handler_becomes_service_command_request() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
-- Quick-settings style: read wifi_enabled from proxy, then send the command.
wifi_enabled = false

function onToggleWiFi()
    local network_ok, network = pcall(require, "mesh.network@>=1.0")
    if network_ok and network then
        local enabled = network.wifi_enabled or false
        network.set_wifi_enabled(not enabled)
    end
end
"#,
    )
    .unwrap();

    // Seed proxy state so wifi_enabled read returns false.
    ctx.apply_service_payload("network", &serde_json::json!({ "wifi_enabled": false }));

    ctx.call_handler("onToggleWiFi", &[]).unwrap();
    let events = ctx.drain_published_events();

    // Route published events through the same path the shell uses.
    let requests = script_events_to_requests(events);

    assert!(
        !requests.is_empty(),
        "onToggleWiFi should publish at least one request"
    );
    match &requests[0] {
        CoreRequest::ServiceCommand {
            interface,
            command,
            payload,
            ..
        } => {
            assert_eq!(
                interface, "mesh.network",
                "interface should be mesh.network"
            );
            assert_eq!(
                command, "set_wifi_enabled",
                "command should be set_wifi_enabled"
            );
            assert_eq!(
                payload.get("enabled").and_then(|v| v.as_bool()),
                Some(true),
                "enabled should be true (toggled from false)"
            );
        }
        other => panic!("expected ServiceCommand for network.set_wifi_enabled, got {other:?}"),
    }
}

// ---------- integration test 3: missing service keeps fallback copy -----

/// Proves that when `pcall(require, "mesh.audio@>=1.0")` fails (e.g. the
/// interface contract is not registered in the catalog), the script still
/// produces user-visible explanatory text rather than a blank or nil surface.
#[test]
fn frontend_missing_service_keeps_visible_fallback_copy() {
    // Intentionally use an empty catalog so the require will fail.
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
    // No interface registered → pcall(require, ...) will catch the error.

    ctx.load_script(
        r#"
-- Panel-style degraded path: pcall catches the missing interface.
volumeLevel = "0"
volumeIcon = "audio-volume-muted"
batteryText = "N/A"

function render()
    local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
    if not audio_ok or not audio then
        volumeLevel = "0"
        volumeIcon = "audio-volume-muted"
        -- Explicit user-visible copy — not blank.
        batteryText = "N/A"
        return
    end
    volumeLevel = tostring(audio.percent or 0)
end
"#,
    )
    .unwrap();

    // No service payload applied — provider is absent.
    ctx.call_handler("render", &[]).unwrap();

    // Template-visible globals must be non-empty explanatory copy.
    assert_eq!(
        ctx.state.get("batteryText"),
        Some(serde_json::json!("N/A")),
        "batteryText should be 'N/A' when service is unavailable"
    );
    assert_eq!(
        ctx.state.get("volumeLevel"),
        Some(serde_json::json!("0")),
        "volumeLevel should be '0' when service is unavailable"
    );
    assert_eq!(
        ctx.state.get("volumeIcon"),
        Some(serde_json::json!("audio-volume-muted")),
        "volumeIcon should fall back to muted when service is unavailable"
    );
}

#[test]
fn quick_settings_audio_render_state_uses_seeded_payload() {
    let mut ctx = make_audio_ctx();
    ctx.load_script(
        r#"
 local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end

audio_label = "0%"
audio_backend = "Unavailable"
audio_tooltip = "Volume unavailable"
icon_name = "audio-volume-muted"

function render()
    if not audio_ok or not audio or audio.available == false then
        audio_label = "Audio unavailable"
        audio_backend = "Unavailable"
        audio_tooltip = "Audio unavailable"
        icon_name = "audio-volume-muted"
        return
    end

    local percent = math.floor((tonumber(audio.percent) or 0) + 0.5)
    local muted = audio.muted or false
    audio_label = string.format("%d%%", percent)
    audio_backend = audio.source_module or "Unavailable"
    if muted then
        audio_tooltip = string.format("Volume muted at %d%%", percent)
    else
        audio_tooltip = string.format("Volume %d%%", percent)
    end

    if muted or percent == 0 then
        icon_name = "audio-volume-muted"
    elseif percent < 34 then
        icon_name = "audio-volume-low"
    elseif percent < 67 then
        icon_name = "audio-volume-medium"
    else
        icon_name = "audio-volume-high"
    end
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload(
        "audio",
        &serde_json::json!({
            "available": true,
            "percent": 42,
            "muted": false,
            "source_module": "@mesh/pipewire-audio"
        }),
    );

    ctx.call_handler("render", &[]).unwrap();

    assert_eq!(ctx.state.get("audio_label"), Some(serde_json::json!("42%")));
    assert_eq!(
        ctx.state.get("audio_backend"),
        Some(serde_json::json!("@mesh/pipewire-audio"))
    );
    assert_eq!(
        ctx.state.get("audio_tooltip"),
        Some(serde_json::json!("Volume 42%"))
    );
    assert_eq!(
        ctx.state.get("icon_name"),
        Some(serde_json::json!("audio-volume-medium"))
    );
}

#[test]
fn quick_settings_audio_slider_publishes_set_volume_service_command() {
    let mut ctx = make_audio_ctx();
    ctx.load_script(
        r#"
 local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end

audio_percent = 0
audio_status = ""

local function clamp_percent(value)
    local numeric = tonumber(value) or 0
    if numeric < 0 then return 0 end
    if numeric > 100 then return 100 end
    return math.floor(numeric + 0.5)
end

function onVolumeChange(value)
    local percent = clamp_percent(value)
    audio_percent = percent
    if audio_ok and audio and audio.available ~= false then
        audio.set_volume("default", percent / 100)
    else
        audio_status = "Audio controls unavailable"
    end
end
"#,
    )
    .unwrap();
    ctx.apply_service_payload("audio", &serde_json::json!({ "available": true }));

    ctx.call_handler("onVolumeChange", &[serde_json::json!(42)])
        .unwrap();
    let requests = script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.audio");
            assert_eq!(command, "set_volume");
            assert_eq!(
                payload,
                &serde_json::json!({ "device_id": "default", "volume": 0.42 })
            );
        }
        other => panic!("expected one mesh.audio set_volume command, got {other:?}"),
    }
}

#[test]
fn quick_settings_network_toggle_publishes_set_wifi_enabled_service_command() {
    let mut ctx = make_network_ctx();
    ctx.load_script(
        r#"
 local network_ok, network = pcall(require, "mesh.network@>=1.0")
if not network_ok then network = nil end

network_status = ""

function onToggleWiFi()
    if not network_ok or not network or network.available == false then
        network_status = "Network unavailable"
        return
    end
    if network.controls_available == false or network.permission_denied == true then
        network_status = "Network controls unavailable"
        return
    end
    network.set_wifi_enabled(not (network.wifi_enabled or false))
end
"#,
    )
    .unwrap();
    ctx.apply_service_payload(
        "network",
        &serde_json::json!({ "available": true, "wifi_enabled": false }),
    );

    ctx.call_handler("onToggleWiFi", &[]).unwrap();
    let requests = script_events_to_requests(ctx.drain_published_events());

    match requests.as_slice() {
        [
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            },
        ] => {
            assert_eq!(interface, "mesh.network");
            assert_eq!(command, "set_wifi_enabled");
            assert_eq!(payload, &serde_json::json!({ "enabled": true }));
        }
        other => panic!("expected one mesh.network set_wifi_enabled command, got {other:?}"),
    }
}

#[test]
fn quick_settings_missing_services_keep_visible_fallback_copy() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.network.read"));
    let mut ctx = ScriptContext::new("@mesh/quick-settings", caps).unwrap();

    ctx.load_script(
        r#"
local audio_ok, audio = pcall(require, "mesh.audio@>=1.0")
if not audio_ok then audio = nil end
local network_ok, network = pcall(require, "mesh.network@>=1.0")
if not network_ok then network = nil end

audio_status = ""
network_status = ""

function render()
    if not audio_ok or not audio or audio.available == false then
        audio_status = "Audio unavailable"
    end
    if not network_ok or not network or network.available == false then
        network_status = "Network unavailable"
    end
end
"#,
    )
    .unwrap();

    ctx.call_handler("render", &[]).unwrap();

    assert_eq!(
        ctx.state.get("audio_status"),
        Some(serde_json::json!("Audio unavailable"))
    );
    assert_eq!(
        ctx.state.get("network_status"),
        Some(serde_json::json!("Network unavailable"))
    );
}
