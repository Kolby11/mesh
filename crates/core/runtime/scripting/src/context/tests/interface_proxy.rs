use super::super::*;
use super::common::*;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_elements::VariableStore;
use mesh_core_frontend_abi::{FrontendEffect, SurfaceEffect, SurfaceRole};
use serde_json::Value;

#[test]
fn interface_proxy_tracks_top_level_field_reads() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
icon_name = "audio-volume-muted"

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    local percent = audio.percent or 0
    if audio.muted then
        icon_name = "audio-volume-muted"
    else
        if percent < 34 then
            icon_name = "audio-volume-low"
        else
            if percent < 67 then
                icon_name = "audio-volume-medium"
            else
                icon_name = "audio-volume-high"
            end
        end
    end
end
"#,
    )
    .unwrap();

    let payload = serde_json::json!({ "percent": 65, "muted": false });
    ctx.apply_service_payload("audio", &payload);
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(
        ctx.state.get("icon_name"),
        Some(Value::String("audio-volume-medium".into()))
    );

    let tracked = ctx.tracked_fields_for_service("audio");
    assert!(tracked.contains("percent"));
    assert!(tracked.contains("muted"));
    assert!(!ctx.tracked_service_fields_changed(
        "audio",
        Some(&serde_json::json!({ "percent": 65, "muted": false })),
        &serde_json::json!({ "percent": 65, "muted": false }),
    ));
    assert!(ctx.tracked_service_fields_changed(
        "audio",
        Some(&serde_json::json!({ "percent": 65, "muted": false })),
        &serde_json::json!({ "percent": 66, "muted": false }),
    ));
}

#[test]
fn interface_proxy_repeated_field_reads_track_once_per_proxy() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
total = 0

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    for i = 1, 50 do
        total = total + (audio.percent or 0)
        total = total + (audio.state.percent or 0)
    end
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 1 }));
    ctx.call_handler("sync_audio_state", &[]).unwrap();

    let tracked = ctx.tracked_fields_for_service("audio");
    assert_eq!(tracked.len(), 1);
    assert!(tracked.contains("percent"));
}

#[test]
fn interface_proxy_exposes_state_table() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_state_type = ""

function init()
    local audio = require("mesh.audio@>=1.0")
    audio_state_type = type(audio.state)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(
        ctx.state.get("audio_state_type"),
        Some(serde_json::json!("table"))
    );
}

#[test]
fn interface_proxy_state_reads_latest_payload() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_percent = 0

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    audio_percent = audio.state.percent or 0
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 31 }));
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(31)));

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 88 }));
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(88)));
    assert!(ctx.tracked_fields_for_service("audio").contains("percent"));
}

#[test]
fn interface_proxy_direct_field_read_remains_compatibility_alias() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
state_percent = 0
direct_percent = 0

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    state_percent = audio.state.percent or 0
    direct_percent = audio.percent or 0
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 57 }));
    ctx.call_handler("sync_audio_state", &[]).unwrap();

    assert_eq!(ctx.state.get("state_percent"), Some(serde_json::json!(57)));
    assert_eq!(ctx.state.get("direct_percent"), Some(serde_json::json!(57)));
}

#[test]
fn interface_method_returns_ticket_and_receives_completion() {
    let caps = CapabilitySet::from_ids(["service.audio.control"]);
    let mut ctx = ScriptContext::new_for_instance(
        "@test/audio-widget",
        "@test/audio-widget",
        "@test/audio-widget#bottom",
        caps,
    )
    .unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function invoke()
    local audio = require("mesh.audio@>=1.0")
    ticket = audio.set_volume("default", 55)
end

function inspect()
    local snapshot = ticket:poll()
    completed = snapshot.done
    completion_status = snapshot.status
    completion_value = snapshot.result and snapshot.result.percent or -1
end
"#,
    )
    .unwrap();

    ctx.call_handler("invoke", &[]).unwrap();
    let events = ctx.drain_published_events();
    let call_id = events
        .iter()
        .find_map(|event| event.call_id)
        .expect("proxy invocation should publish a call id");
    assert_eq!(events.len(), 1);

    ctx.complete_service_call(
        call_id,
        "completed",
        serde_json::json!({ "ok": true, "percent": 55 }),
    );
    ctx.call_handler("inspect", &[]).unwrap();
    assert_eq!(ctx.state.get("completed"), Some(serde_json::json!(true)));
    assert_eq!(
        ctx.state.get("completion_status"),
        Some(serde_json::json!("completed"))
    );
    assert_eq!(
        ctx.state.get("completion_value"),
        Some(serde_json::json!(55))
    );
}

#[test]
fn interface_proxy_validates_method_arity_and_types_without_confusing_table_args() {
    let caps = CapabilitySet::from_ids(["service.audio.control"]);
    let mut ctx = ScriptContext::new_for_instance(
        "@test/audio-widget",
        "@test/audio-widget",
        "@test/audio-widget#bottom",
        caps,
    )
    .unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function valid_colon_call()
    local audio = require("mesh.audio@>=1.0")
    ticket = audio:set_volume("default", 55)
end

function invalid_type_call()
    local audio = require("mesh.audio@>=1.0")
    audio.set_volume("default", "loud")
end

function invalid_arity_call()
    local audio = require("mesh.audio@>=1.0")
    audio.set_volume("default")
end
"#,
    )
    .unwrap();

    ctx.call_handler("valid_colon_call", &[]).unwrap();
    assert_eq!(ctx.drain_published_events().len(), 1);
    assert!(ctx.call_handler("invalid_type_call", &[]).is_err());
    assert!(ctx.call_handler("invalid_arity_call", &[]).is_err());
    assert!(ctx.drain_published_events().is_empty());
}

#[test]
fn service_ticket_cancel_publishes_correlated_cancellation() {
    let caps = CapabilitySet::from_ids(["service.audio.control"]);
    let mut ctx = ScriptContext::new_for_instance(
        "@test/audio-widget",
        "@test/audio-widget",
        "@test/audio-widget#bottom",
        caps,
    )
    .unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function invoke()
    local audio = require("mesh.audio@>=1.0")
    ticket = audio.set_volume("default", 55)
    cancelled = ticket:cancel()
end
"#,
    )
    .unwrap();

    ctx.call_handler("invoke", &[]).unwrap();
    assert_eq!(ctx.state.get("cancelled"), Some(serde_json::json!(true)));
    let events = ctx.drain_published_events();
    assert_eq!(events.len(), 2);
    let call_id = events[0].call_id.expect("service call id");
    assert_eq!(events[1].channel, "mesh.service.cancel");
    assert_eq!(events[1].call_id, Some(call_id));
    assert_eq!(
        events[1].source_instance_id.as_deref(),
        Some("@test/audio-widget#bottom")
    );
}

#[test]
fn service_state_is_context_local_capability_filtered_and_generation_stamped() {
    let read_caps = CapabilitySet::from_ids(["service.audio.read"]);
    let vm = SurfaceVm::new();

    let mut first = ScriptContext::new("@test/first-audio", read_caps.clone()).unwrap();
    first.attach_shared_vm(&vm);
    first.set_interface_catalog(audio_catalog());
    first
        .load_script(
            r#"
audio = require("mesh.audio@>=1.0")
observed = ""
function read_audio()
    observed = audio.devices[1].name
end
function mutate_copy()
    local devices = audio.devices
    devices[1].name = "mutated"
end
function write_state()
    audio.percent = 100
end
function direct_global_present()
    return __mesh_svc_audio ~= nil
end
"#,
        )
        .unwrap();

    let mut second = ScriptContext::new("@test/second-audio", read_caps).unwrap();
    second.attach_shared_vm(&vm);
    second.set_interface_catalog(audio_catalog());
    second
        .load_script(
            r#"
audio = require("mesh.audio@>=1.0")
observed = ""
function read_audio()
    observed = audio.devices[1].name
end
"#,
        )
        .unwrap();

    let mut denied = ScriptContext::new("@test/denied-audio", CapabilitySet::default()).unwrap();
    denied.attach_shared_vm(&vm);
    denied
        .load_script(
            r#"
direct_global_present = false
function probe()
    direct_global_present = __mesh_svc_audio ~= nil
end
"#,
        )
        .unwrap();

    first.apply_service_payload(
        "audio",
        &serde_json::json!({ "percent": 31, "devices": [{ "name": "first" }] }),
    );
    second.apply_service_payload(
        "audio",
        &serde_json::json!({ "percent": 82, "devices": [{ "name": "second" }] }),
    );
    denied.apply_service_payload("audio", &serde_json::json!({ "percent": 99 }));

    assert_eq!(first.service_context_generation(), 1);
    assert_eq!(first.service_payload_generation("audio"), Some(1));
    assert_eq!(second.service_context_generation(), 1);
    assert_eq!(denied.service_context_generation(), 0);

    first.call_handler("read_audio", &[]).unwrap();
    second.call_handler("read_audio", &[]).unwrap();
    denied.call_handler("probe", &[]).unwrap();
    assert_eq!(
        first.state.get("observed"),
        Some(serde_json::json!("first"))
    );
    assert_eq!(
        second.state.get("observed"),
        Some(serde_json::json!("second"))
    );
    assert_eq!(
        denied.state.get("direct_global_present"),
        Some(serde_json::json!(false))
    );

    first.call_handler("mutate_copy", &[]).unwrap();
    first.call_handler("read_audio", &[]).unwrap();
    second.call_handler("read_audio", &[]).unwrap();
    assert_eq!(
        first.state.get("observed"),
        Some(serde_json::json!("first"))
    );
    assert_eq!(
        second.state.get("observed"),
        Some(serde_json::json!("second"))
    );

    let error = first
        .call_handler("write_state", &[])
        .expect_err("service state must not be writable from Luau");
    assert!(error.to_string().contains("read-only"));
}

#[test]
fn equal_service_payloads_do_not_advance_context_generation() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-generation", caps).unwrap();
    let payload = serde_json::json!({ "percent": 64 });

    ctx.apply_service_payload("audio", &payload);
    ctx.apply_service_payload("audio", &payload);
    assert_eq!(ctx.service_context_generation(), 1);

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 65 }));
    assert_eq!(ctx.service_context_generation(), 2);
    assert_eq!(ctx.service_payload_generation("audio"), Some(2));
}

#[test]
fn stale_versioned_service_snapshots_cannot_replace_context_state() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-stale-generation", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
observed = 0
function read_audio()
    local audio = require("mesh.audio@>=1.0")
    observed = audio.percent
end
"#,
    )
    .unwrap();

    let first = serde_json::json!({ "percent": 20 });
    let stale = serde_json::json!({ "percent": 99 });
    let first_fingerprint = ScriptContext::service_payload_fingerprint(&first);
    let stale_fingerprint = ScriptContext::service_payload_fingerprint(&stale);
    assert!(ctx.apply_service_payload_with_generation(
        "audio",
        &first,
        first_fingerprint,
        Some("@mesh/pipewire-audio"),
        7,
    ));
    assert!(!ctx.apply_service_payload_with_generation(
        "audio",
        &stale,
        stale_fingerprint,
        Some("@mesh/pipewire-audio"),
        6,
    ));
    assert_eq!(ctx.service_context_generation(), 1);

    ctx.call_handler("read_audio", &[]).unwrap();
    assert_eq!(ctx.state.get("observed"), Some(serde_json::json!(20)));
}

#[test]
fn interface_proxy_reads_state_fields_without_callbacks() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
icon_name = "audio-volume-muted"

function init()
    local audio = require("mesh.audio@>=1.0")
    if audio.muted then
        icon_name = "audio-volume-muted"
    elseif audio.percent < 50 then
        icon_name = "audio-volume-low"
    else
        icon_name = "audio-volume-high"
    end
end
"#,
    )
    .unwrap();
    let payload = serde_json::json!({ "percent": 80, "muted": false });
    ctx.apply_service_payload("audio", &payload);
    ctx.call_init().unwrap();
    assert_eq!(
        ctx.state.get("icon_name"),
        Some(Value::String("audio-volume-high".into()))
    );
}

#[test]
fn interface_proxy_reads_latest_emitted_fields_after_repeated_updates() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_percent = 0
audio_muted = false
audio_source = ""

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    audio_percent = audio.percent or 0
    audio_muted = audio.muted or false
    audio_source = audio.source_module or ""
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload(
        "audio",
        &serde_json::json!({
            "percent": 25,
            "muted": false,
            "source_module": "@mesh/pulse"
        }),
    );
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(25)));
    assert_eq!(ctx.state.get("audio_muted"), Some(serde_json::json!(false)));
    assert_eq!(
        ctx.state.get("audio_source"),
        Some(serde_json::json!("@mesh/pulse"))
    );

    ctx.apply_service_payload(
        "audio",
        &serde_json::json!({
            "percent": 82,
            "muted": true,
            "source_module": "@mesh/pipewire"
        }),
    );
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(82)));
    assert_eq!(ctx.state.get("audio_muted"), Some(serde_json::json!(true)));
    assert_eq!(
        ctx.state.get("audio_source"),
        Some(serde_json::json!("@mesh/pipewire"))
    );
}

#[test]
fn shared_vm_reuses_equal_service_payload_conversion_marker() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let vm = SurfaceVm::new();
    let payload = serde_json::json!({ "percent": 64, "muted": false });

    let mut first = ScriptContext::new("@mesh/first", caps.clone()).unwrap();
    first.attach_shared_vm(&vm);
    first.set_interface_catalog(audio_catalog());
    first
        .load_script(
            r#"
audio = require("mesh.audio@>=1.0")
first_percent = 0
function read()
    first_percent = audio.percent
end
"#,
        )
        .unwrap();
    first.apply_service_payload("audio", &payload);
    let first_marker = first.service_payload_marker_for_test("audio").unwrap();
    assert_eq!(first_marker.len(), std::mem::size_of::<u64>());

    let mut second = ScriptContext::new("@mesh/second", caps).unwrap();
    second.attach_shared_vm(&vm);
    second.set_interface_catalog(audio_catalog());
    second
        .load_script(
            r#"
audio = require("mesh.audio@>=1.0")
second_percent = 0
function read()
    second_percent = audio.percent
end
"#,
        )
        .unwrap();
    second.apply_service_payload("audio", &payload.clone());
    assert_eq!(
        second.service_payload_marker_for_test("audio"),
        Some(first_marker)
    );

    first.call_handler("read", &[]).unwrap();
    second.call_handler("read", &[]).unwrap();
    assert_eq!(
        first.state.get("first_percent"),
        Some(serde_json::json!(64))
    );
    assert_eq!(
        second.state.get("second_percent"),
        Some(serde_json::json!(64))
    );
}

#[test]
fn interface_proxy_method_publishes_service_command() {
    let caps = CapabilitySet::from_ids(["service.audio.read", "service.audio.control"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio@>=1.0")
    audio:set_volume("default", 50)
    audio.set_volume("default", 50)
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    let published = ctx.drain_published_events();
    assert_eq!(published.len(), 2);
    for event in published {
        assert_eq!(event.channel, "mesh.audio.set_volume");
        assert_eq!(event.source_module_id, "@test/audio-widget");
        assert!(
            event
                .source_capabilities
                .is_granted(&Capability::new("service.audio.control"))
        );
        assert_eq!(
            event.payload,
            serde_json::json!({ "device_id": "default", "percent": 50 })
        );
    }
}

#[test]
fn control_only_proxy_can_command_without_receiving_service_state() {
    let caps = CapabilitySet::from_ids(["service.audio.control"]);
    let mut ctx = ScriptContext::new("@test/control-only-audio", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_percent = -1
command_queued = false

function invoke()
    local audio = require("mesh.audio@>=1.0")
    audio_percent = audio.percent or -1
    command_queued = audio.set_volume("default", 50).queued
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 64 }));
    assert_eq!(ctx.service_context_generation(), 0);
    ctx.call_handler("invoke", &[]).unwrap();

    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(-1)));
    assert_eq!(
        ctx.state.get("command_queued"),
        Some(serde_json::json!(true))
    );
    assert_eq!(ctx.drain_published_events().len(), 1);
}

#[test]
fn popover_activate_queues_focus_option_and_trigger_target() {
    let caps = CapabilitySet::from_ids(["shell.surface"]);
    let mut ctx = ScriptContext::new("@test/nav", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    mesh.popover.activate("@test/popover", {
        surface = { id = "@test/nav" },
        current = { key = "volume-button" }
    }, { focus = false })
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert!(ctx.drain_published_events().is_empty());
    let effects = ctx.drain_shell_effects();
    assert!(
        matches!(
            effects.as_slice(),
            [FrontendEffect::Surface(SurfaceEffect::ActivatePopover {
                surface_id,
                trigger_surface,
                trigger_key,
                focus: false,
            })] if surface_id == "@test/popover"
                && trigger_surface == "@test/nav"
                && trigger_key == "volume-button"
        ),
        "{effects:?}"
    );
}

#[test]
fn shell_calls_require_the_surface_capability_at_the_call() {
    let mut ctx = ScriptContext::new("@test/unprivileged", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
function typed()
    mesh.shell.show_surface("@mesh/settings")
end
function raw()
    mesh.events.publish("shell.show-surface", { surface_id = "@mesh/settings" })
end
"#,
    )
    .unwrap();

    let error = ctx
        .call_handler("typed", &[])
        .expect_err("unprivileged shell call must fail");
    assert!(error.to_string().contains("shell.surface"), "{error}");
    let error = ctx
        .call_handler("raw", &[])
        .expect_err("removed shell channel must fail");
    assert!(
        error.to_string().contains("mesh.shell.show_surface"),
        "{error}"
    );
    assert!(ctx.drain_published_events().is_empty());
    assert!(ctx.drain_shell_effects().is_empty());
}

#[test]
fn shell_calls_validate_arguments_at_the_call() {
    let caps = CapabilitySet::from_ids(["shell.surface"]);
    let mut ctx = ScriptContext::new("@test/panel", caps).unwrap();
    ctx.load_script(
        r#"
function missing_id() mesh.shell.show_surface() end
function empty_id() mesh.shell.hide_surface("") end
function bad_role() mesh.shell.set_surface_role("@test/panel", "floating") end
function bad_margin() mesh.shell.position_surface("@test/panel", "top", 0) end
function huge_margin() mesh.shell.position_surface("@test/panel", 2^40, 0) end
function ok()
    mesh.shell.set_surface_role("@test/panel", "TopLevel")
    mesh.shell.position_surface("@test/panel", 4, -8)
    mesh.shell.promote_widget("@test/panel", "root/widgets/media")
end
"#,
    )
    .unwrap();

    for handler in [
        "missing_id",
        "empty_id",
        "bad_role",
        "bad_margin",
        "huge_margin",
    ] {
        assert!(
            ctx.call_handler(handler, &[]).is_err(),
            "{handler} should raise"
        );
    }
    assert!(ctx.drain_shell_effects().is_empty());

    ctx.call_handler("ok", &[]).unwrap();
    let effects = ctx.drain_shell_effects();
    assert!(
        matches!(
            effects.as_slice(),
            [
                FrontendEffect::Surface(SurfaceEffect::SetRole {
                    role: SurfaceRole::Window,
                    ..
                }),
                FrontendEffect::Surface(SurfaceEffect::Position {
                    margin_top: 4,
                    margin_left: -8,
                    ..
                }),
                FrontendEffect::Surface(SurfaceEffect::SetChildRole {
                    node_key,
                    role: SurfaceRole::Window,
                    ..
                }),
            ] if node_key == "root/widgets/media"
        ),
        "{effects:?}"
    );
}

#[test]
fn mesh_popover_hide_can_request_hover_bridge_deferral() {
    let caps = CapabilitySet::from_ids(["shell.surface"]);
    let mut ctx = ScriptContext::new("@test/popover", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    mesh.popover.hide("@test/popover", { bridge = true })
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    let effects = ctx.drain_shell_effects();
    assert!(
        matches!(
            effects.as_slice(),
            [FrontendEffect::Surface(SurfaceEffect::HidePopover {
                surface_id,
                defer_for_hover_bridge: true,
            })] if surface_id == "@test/popover"
        ),
        "{effects:?}"
    );
}

#[test]
fn interface_proxy_method_returns_queued_result() {
    let caps = CapabilitySet::from_ids(["service.audio.read", "service.audio.control"]);
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
queued_ok = false
queued = false

function init()
    local audio = require("mesh.audio@>=1.0")
    local result = audio.set_volume("default", 50)
    queued_ok = result.ok
    queued = result.queued
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("queued_ok"), Some(serde_json::json!(true)));
    assert_eq!(ctx.state.get("queued"), Some(serde_json::json!(true)));
    let published = ctx.drain_published_events();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].channel, "mesh.audio.set_volume");
}

#[test]
fn read_only_interface_proxy_returns_capability_denied_result() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@test/read-only-audio", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_percent = 0
denied_ok = true
denied_error = ""

function read_state()
    local audio = require("mesh.audio@>=1.0")
    audio_percent = audio.percent or 0
end

function change_volume()
    local audio = require("mesh.audio@>=1.0")
    local result = audio.set_volume("default", 50)
    denied_ok = result.ok
    denied_error = result.error or ""
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 64 }));
    ctx.call_handler("read_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(64)));

    ctx.call_handler("change_volume", &[]).unwrap();
    assert_eq!(ctx.state.get("denied_ok"), Some(serde_json::json!(false)));
    assert_eq!(
        ctx.state.get("denied_error"),
        Some(serde_json::json!("capability denied"))
    );
    assert!(
        ctx.drain_published_events().is_empty(),
        "read-only audio proxy must not publish mesh.audio.set_volume"
    );
}
