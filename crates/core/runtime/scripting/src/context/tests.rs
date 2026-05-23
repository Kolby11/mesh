use super::*;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_elements::VariableStore;
use mesh_core_service::{
    ContractCapabilities, InterfaceArgument, InterfaceCatalog, InterfaceContract, InterfaceEvent,
    InterfaceMethod, InterfaceProvider, parse_contract_version,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

fn audio_catalog() -> InterfaceCatalog {
    let mut catalog = InterfaceCatalog::default();
    catalog.register_contract(InterfaceContract {
        interface: "mesh.audio".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        // State fields are documented core reads — not callable methods.
        state_fields: Vec::new(),
        // Only mutating command methods belong here.
        methods: vec![
            InterfaceMethod {
                name: "set_volume".into(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "volume".into(),
                        arg_type: "float".into(),
                    },
                ],
                returns: Some("Result".into()),
                coalesce: false,
            },
            InterfaceMethod {
                name: "volume_up".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
            },
            InterfaceMethod {
                name: "volume_down".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
            },
            InterfaceMethod {
                name: "toggle_mute".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
            },
            InterfaceMethod {
                name: "set_muted".into(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "muted".into(),
                        arg_type: "boolean".into(),
                    },
                ],
                returns: Some("Result".into()),
                coalesce: false,
            },
        ],
        events: vec![InterfaceEvent {
            name: "VolumeChanged".into(),
            payload: Some("{ device_id: string, level: float }".into()),
        }],
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.audio".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/audio-interface".into()),
        provider_module: "@mesh/pipewire-audio".into(),
        backend_name: "PipeWire".into(),
        priority: 100,
    });
    catalog
}

fn theme_provider_only_catalog() -> InterfaceCatalog {
    let mut catalog = InterfaceCatalog::default();
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.theme".into(),
        version: Some("1.0".into()),
        base_module: None,
        provider_module: "@mesh/shell-theme".into(),
        backend_name: "Shell Theme".into(),
        priority: 100,
    });
    catalog
}

#[test]
fn require_import_installs_proxy() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio@>=1.0")
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
}

#[test]
fn explicit_interface_import_installs_proxy_global() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script_with_interface_imports(
        r#"
audio_percent = 0

function init()
    audio_percent = audio.percent or 0
end
"#,
        &[ScriptInterfaceImport {
            alias: "audio".into(),
            interface: "mesh.audio".into(),
            version: Some(">=1.0".into()),
        }],
    )
    .unwrap();
    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 72 }));
    ctx.call_init().unwrap();

    assert_eq!(
        ctx.interface_bindings
            .get("audio")
            .map(|resolution| resolution.requested.as_str()),
        Some("mesh.audio")
    );
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(72)));
    assert!(ctx.tracked_fields_for_service("audio").contains("percent"));
}

#[test]
fn module_object_exposes_state_and_exports() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.load_script(
        r#"
count = 1
module.exports.visible = true
module.exports.label = "Audio"

function onRender()
    count = count + 1
    latest_count = module.state.count
    exported_label = module.exports.label
end
"#,
    )
    .unwrap();

    assert_eq!(
        ctx.state.get("exports"),
        Some(serde_json::json!({ "visible": true, "label": "Audio" }))
    );

    ctx.call_handler("onRender", &[]).unwrap();

    assert_eq!(ctx.state.get("count"), Some(serde_json::json!(2)));
    assert_eq!(ctx.state.get("latest_count"), Some(serde_json::json!(1)));
    assert_eq!(
        ctx.state.get("exported_label"),
        Some(serde_json::json!("Audio"))
    );
}

#[test]
fn module_state_reflects_host_seeded_values_before_script_runs() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_global_state("seeded", serde_json::json!("ready"))
        .unwrap();
    ctx.load_script(
        r#"
seed_seen = module.state.seeded
"#,
    )
    .unwrap();

    assert_eq!(ctx.state.get("seed_seen"), Some(serde_json::json!("ready")));
}

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

#[test]
fn require_imports_interface_proxy() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));

    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio")
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
}

#[test]
fn rejects_missing_interface_contract() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio@>=1.0")
end
"#,
    )
    .unwrap();

    let err = ctx.call_init().unwrap_err();
    assert!(matches!(err, ScriptError::InterfaceUnavailable(_)));
}

#[test]
fn require_missing_interface_emits_visible_diagnostic() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/diagnostic-test", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    require("mesh.audio@>=1.0")
end
"#,
    )
    .unwrap();

    let err = ctx.call_init().unwrap_err();
    assert!(matches!(err, ScriptError::InterfaceUnavailable(_)));
    let diagnostics = ctx.drain_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].module_id, "@mesh/diagnostic-test");
    assert_eq!(diagnostics[0].interface, "mesh.audio");
    assert_eq!(diagnostics[0].requested_version.as_deref(), Some(">=1.0"));
    assert!(diagnostics[0].reason.contains("missing contract"));
}

#[test]
fn pcall_require_still_emits_interface_diagnostic() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/pcall-test", caps).unwrap();
    ctx.load_script(
        r#"
ok = true

function init()
    ok = pcall(require, "mesh.audio")
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    assert_eq!(ctx.state.get("ok"), Some(Value::Bool(false)));
    let diagnostics = ctx.drain_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].module_id, "@mesh/pcall-test");
    assert_eq!(diagnostics[0].interface, "mesh.audio");
}

#[test]
fn unknown_method_reads_state_field_as_nil() {
    // Unknown keys fall through to the live service state table (__mesh_svc_audio).
    // When no service has emitted yet the table doesn't exist, so the result is nil
    // and the call succeeds without error.
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio@>=1.0")
    local val = audio.mute_all  -- unknown key: should return nil, not error
    assert(val == nil)
end
"#,
    )
    .unwrap();

    // Should succeed — no error for unknown keys.
    ctx.call_init().unwrap();
}

#[test]
fn globals_are_reactive_state() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@test/local", caps).unwrap();
    ctx.load_script(
        r#"
volumeHidden = true
count = 0

function toggle()
    volumeHidden = not volumeHidden
end
"#,
    )
    .unwrap();

    assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(true)));
    assert_eq!(ctx.state.get("count"), Some(Value::Number(0.into())));

    ctx.call_handler("toggle", &[]).unwrap();
    assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(false)));

    ctx.call_handler("toggle", &[]).unwrap();
    assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(true)));
}

#[test]
fn reactive_global_marks_dirty_only_when_value_changes() {
    let mut state = ScriptState::new();
    state.set("count", serde_json::json!(1));
    assert!(state.is_dirty());

    state.clear_dirty();
    state.set("count", serde_json::json!(1));
    assert!(!state.is_dirty());

    state.set("count", serde_json::json!(2));
    assert!(state.is_dirty());
}

#[test]
fn reactive_table_compares_nested_values() {
    let mut state = ScriptState::new();
    state.set(
        "settings",
        serde_json::json!({
            "enabled": true,
            "label": "primary",
            "nested": { "value": 1 }
        }),
    );
    assert!(state.is_dirty());

    state.clear_dirty();
    state.set(
        "settings",
        serde_json::json!({
            "enabled": true,
            "label": "primary",
            "nested": { "value": 1 }
        }),
    );
    assert!(!state.is_dirty());

    state.set(
        "settings",
        serde_json::json!({
            "enabled": false,
            "label": "primary",
            "nested": { "value": 1 }
        }),
    );
    assert!(state.is_dirty());

    state.clear_dirty();
    state.set(
        "settings",
        serde_json::json!({
            "enabled": false,
            "label": "primary",
            "nested": { "value": 2 }
        }),
    );
    assert!(state.is_dirty());

    state.clear_dirty();
    state.set(
        "settings",
        serde_json::json!({
            "enabled": false,
            "label": "primary",
            "nested": { "value": 2 },
            "levels": [1, 2, 3]
        }),
    );
    assert!(state.is_dirty());

    state.clear_dirty();
    state.set(
        "settings",
        serde_json::json!({
            "enabled": false,
            "label": "primary",
            "nested": { "value": 2 },
            "levels": [1, 3, 3]
        }),
    );
    assert!(state.is_dirty());

    state.clear_dirty();
    state.set(
        "wifi_networks",
        serde_json::json!([
            { "connection_id": "home", "ssid": "Home", "strength": 70, "active": false },
            { "connection_id": "office", "ssid": "Office", "strength": 60, "active": true }
        ]),
    );
    assert!(state.is_dirty());

    state.clear_dirty();
    state.set(
        "wifi_networks",
        serde_json::json!([
            { "connection_id": "home", "ssid": "Home", "strength": 71, "active": true },
            { "connection_id": "office", "ssid": "Office", "strength": 60, "active": false }
        ]),
    );
    assert!(state.is_dirty());
}

#[test]
fn host_value_update_does_not_mark_dirty() {
    let mut state = ScriptState::new();
    state.set_host_value("elements", serde_json::json!({ "root": true }));
    assert!(!state.is_dirty());
}

#[test]
fn mesh_request_redraw_marks_dirty_without_global_change() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@test/redraw", caps).unwrap();
    ctx.load_script(
        r#"
function request()
    __mesh_request_redraw = true
end
"#,
    )
    .unwrap();

    ctx.state.clear_dirty();
    ctx.call_handler("request", &[]).unwrap();
    assert!(ctx.state.is_dirty());

    ctx.state.clear_dirty();
    ctx.sync_state_from_lua();
    assert!(!ctx.state.is_dirty());
}

#[test]
fn if_then_end_executes_conditionally() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@test/if", caps).unwrap();
    ctx.load_script(
        r#"
a = true
b = false

function run()
    a = not a
    if not a then
        b = true
    end
end
"#,
    )
    .unwrap();

    ctx.call_handler("run", &[]).unwrap();
    assert_eq!(ctx.state.get("a"), Some(Value::Bool(false)));
    assert_eq!(ctx.state.get("b"), Some(Value::Bool(true)));

    ctx.call_handler("run", &[]).unwrap();
    assert_eq!(ctx.state.get("a"), Some(Value::Bool(true)));
    // b stays true — the if branch doesn't reset it
    assert_eq!(ctx.state.get("b"), Some(Value::Bool(true)));
}

#[test]
fn interface_proxy_tracks_top_level_field_reads() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
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
}

#[test]
fn interface_proxy_exposes_state_table() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
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
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
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
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
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
fn interface_proxy_reads_state_fields_without_callbacks() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
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
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
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
fn provider_only_require_creates_read_only_proxy() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("theme.read"));
    let mut ctx = ScriptContext::new("@test/theme-widget", caps).unwrap();
    ctx.set_interface_catalog(theme_provider_only_catalog());
    ctx.load_script(
        r#"
theme_icon = "weather-clear-night"

function sync_theme_state()
    local theme = require("mesh.theme")
    if theme.is_dark then
        theme_icon = "weather-clear-night"
    else
        theme_icon = "weather-clear"
    end
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("theme", &serde_json::json!({ "is_dark": false }));
    ctx.call_handler("sync_theme_state", &[]).unwrap();
    assert_eq!(
        ctx.state.get("theme_icon"),
        Some(Value::String("weather-clear".into()))
    );
    assert!(ctx.drain_diagnostics().is_empty());
}

#[test]
fn rejects_legacy_mesh_require_syntax() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    require("@mesh/audio@>=1.0")
end
"#,
    )
    .unwrap();

    let err = ctx.call_init().unwrap_err();
    assert!(
        matches!(err, ScriptError::LuaError(message) if message.contains("unsupported require: @mesh/audio@>=1.0"))
    );
}

#[test]
fn interface_proxy_method_publishes_service_command() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.audio.control"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio@>=1.0")
    audio:set_volume("default", 0.5)
    audio.set_volume("default", 0.5)
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
            serde_json::json!({ "device_id": "default", "volume": 0.5 })
        );
    }
}

#[test]
fn popover_activate_publishes_focus_option_and_trigger_target() {
    let caps = CapabilitySet::new();
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

    let published = ctx.drain_published_events();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].channel, "shell.activate-popover");
    assert_eq!(
        published[0].payload,
        serde_json::json!({
            "surface_id": "@test/popover",
            "trigger_surface": "@test/nav",
            "trigger_key": "volume-button",
            "focus": false,
        })
    );
}

#[test]
fn interface_proxy_method_returns_queued_result() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.audio.control"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
queued_ok = false
queued = false

function init()
    local audio = require("mesh.audio@>=1.0")
    local result = audio.set_volume("default", 0.5)
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
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
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
    local result = audio.set_volume("default", 0.5)
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

#[test]
fn handler_receives_event_payload_argument() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@test/click", caps).unwrap();
    ctx.load_script(
        r#"
last_margin_left = 0
last_pointer_x = 0

function on_click(event)
    last_margin_left = event.current_target.position.margin_left
    last_pointer_x = event.pointer.x
end
"#,
    )
    .unwrap();

    ctx.call_handler(
        "on_click",
        &[serde_json::json!({
            "pointer": { "x": 42.0, "y": 10.0 },
            "current_target": {
                "position": {
                    "margin_left": 128,
                    "margin_top": 8
                }
            }
        })],
    )
    .unwrap();

    assert_eq!(
        ctx.state.get("last_margin_left"),
        Some(Value::Number(128.into()))
    );
    assert_eq!(ctx.state.get("last_pointer_x"), Some(serde_json::json!(42)));
}
