use super::super::*;
use super::common::*;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_elements::VariableStore;

#[test]
fn interface_event_proxy_subscribes_and_emits() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
seen_level = 0

function init()
    local audio = require("mesh.audio@>=1.0")
    audio.events.VolumeChanged:subscribe(function(event)
        seen_level = event.level
    end)
    audio.events.VolumeChanged:emit({ level = 88 })
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("seen_level"), Some(serde_json::json!(88)));
}

#[test]
fn interface_event_proxy_receives_host_delivered_event() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
seen_level = 0
seen_device = ""

function init()
    local audio = require("mesh.audio@>=1.0")
    audio.events.VolumeChanged:subscribe(function(event)
        seen_level = event.level
        seen_device = event.device_id
    end)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.emit_interface_event(
        "audio",
        "VolumeChanged",
        &serde_json::json!({ "device_id": "default", "level": 42 }),
    )
    .unwrap();

    assert_eq!(ctx.state.get("seen_level"), Some(serde_json::json!(42)));
    assert_eq!(
        ctx.state.get("seen_device"),
        Some(serde_json::json!("default"))
    );
}

#[test]
fn event_only_subscription_cannot_read_service_state() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.events"));
    let mut ctx = ScriptContext::new("@mesh/event-only", caps).unwrap();
    ctx.set_interface_catalog(event_only_audio_catalog());
    ctx.load_script(
        r#"
seen_level = 0
seen_percent = -1

function init()
    local audio = require("mesh.audio@>=1.0")
    audio.events.VolumeChanged:subscribe(function(event)
        seen_level = event.level
        seen_percent = audio.percent or -1
    end)
end
"#,
    )
    .unwrap();

    assert!(ctx.can_subscribe_service_event("mesh.audio", "VolumeChanged"));
    assert!(!ctx.can_read_service_interface("mesh.audio"));
    ctx.call_init().unwrap();
    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 73 }));
    assert_eq!(ctx.service_context_generation(), 0);

    ctx.emit_interface_event(
        "audio",
        "VolumeChanged",
        &serde_json::json!({ "device_id": "default", "level": 42 }),
    )
    .unwrap();

    assert_eq!(ctx.state.get("seen_level"), Some(serde_json::json!(42)));
    assert_eq!(ctx.state.get("seen_percent"), Some(serde_json::json!(-1)));
}

#[test]
fn interface_named_event_channel_subscribes_with_on_alias() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
seen_level = 0

function init()
    local audio = require("mesh.audio@>=1.0")
    audio.VolumeChanged:on(function(event)
        seen_level = event.level
    end)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.emit_interface_event(
        "audio",
        "VolumeChanged",
        &serde_json::json!({ "level": 91 }),
    )
    .unwrap();

    assert_eq!(ctx.state.get("seen_level"), Some(serde_json::json!(91)));
}

#[test]
fn interface_event_subscription_registry_tracks_subscribe_and_unsubscribe() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio@>=1.0")
    unsubscribe = audio.events.VolumeChanged:subscribe(function(_event) end)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    assert!(ctx.is_subscribed_to_interface_event("audio", "VolumeChanged"));
    assert!(ctx.has_interface_event_subscription_for_service("audio"));

    ctx.call_handler("unsubscribe", &[]).unwrap();
    assert!(!ctx.is_subscribed_to_interface_event("audio", "VolumeChanged"));
    assert!(!ctx.has_interface_event_subscription_for_service("audio"));
}

#[test]
fn interface_event_subscription_registry_clears_on_reload() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio@>=1.0")
    audio.events.VolumeChanged:subscribe(function(_event) end)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    assert!(ctx.has_interface_event_subscription_for_service("audio"));

    ctx.load_script("function init() end").unwrap();
    assert!(!ctx.has_interface_event_subscription_for_service("audio"));
}

#[test]
fn self_named_event_channel_supports_on_and_fire() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.load_script(
        r#"
changed_count = 0

function init(self)
    self.Changed:on(function(event)
        changed_count = changed_count + event.count
    end)
    self.Changed:fire({ count = 2 })
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("changed_count"), Some(serde_json::json!(2)));
}

#[test]
fn module_events_subscribe_emit_and_unsubscribe() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.load_script(
        r#"
activation_count = 0

function init()
    local unsubscribe = module.events.ItemActivated:subscribe(function(event)
        activation_count = activation_count + event.count
    end)
    module.events.ItemActivated:emit({ count = 1 })
    unsubscribe()
    module.events.ItemActivated:emit({ count = 1 })
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(
        ctx.state.get("activation_count"),
        Some(serde_json::json!(1))
    );
}
