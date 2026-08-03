use super::super::*;
use super::common::*;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_elements::VariableStore;
use serde_json::Value;
use std::collections::HashMap;

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
fn require_resolves_existing_host_api_tables() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("locale.read"));
    let mut ctx = ScriptContext::new("@mesh/host-api-test", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    local locale = require("mesh.locale")
    local ui = require("mesh.ui")
    local log = require("mesh.log")
    current_locale = locale.current()
    ui_type = type(ui.request_redraw)
    log_type = type(log.info)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(
        ctx.state.get("current_locale"),
        Some(serde_json::json!("en"))
    );
    assert_eq!(
        ctx.state.get("ui_type"),
        Some(serde_json::json!("function"))
    );
    assert_eq!(
        ctx.state.get("log_type"),
        Some(serde_json::json!("function"))
    );
}

#[test]
fn require_resolves_mesh_i18n_library_alias() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/i18n-test", caps).unwrap();
    ctx.set_i18n_translations(HashMap::from([(
        "nav.volume".to_string(),
        "Volume".to_string(),
    )]));
    ctx.load_script(
        r#"
function init()
    local i18n = require("mesh.i18n")
    label = i18n.t("nav.volume")
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("label"), Some(serde_json::json!("Volume")));
}

#[test]
fn require_component_definition_specifier_returns_placeholder() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/component-host", caps).unwrap();
    ctx.load_script(
        r#"
local LocalChild = require("./child.mesh")
local ModuleChild = require("@mesh/audio-popover")
local_ok = LocalChild.__mesh_component_definition == true
module_source = ModuleChild.source
"#,
    )
    .unwrap();

    assert_eq!(ctx.state.get("local_ok"), Some(serde_json::json!(true)));
    assert_eq!(
        ctx.state.get("module_source"),
        Some(serde_json::json!("@mesh/audio-popover"))
    );
}

#[test]
fn import_named_returns_selected_field() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/import-test", caps).unwrap();
    ctx.set_i18n_translations(HashMap::from([(
        "nav.volume".to_string(),
        "Volume".to_string(),
    )]));
    ctx.load_script(
        r#"
function init()
    local t = import("mesh.i18n", "t")
    label = t("nav.volume")
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("label"), Some(serde_json::json!("Volume")));
}

#[test]
fn import_multiple_named_returns_in_order() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("locale.read"));
    let mut ctx = ScriptContext::new("@mesh/import-multi", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    local current, set = import("mesh.locale", "current", "set")
    current_locale = current()
    set_type = type(set)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(
        ctx.state.get("current_locale"),
        Some(serde_json::json!("en"))
    );
    assert_eq!(
        ctx.state.get("set_type"),
        Some(serde_json::json!("function"))
    );
}

#[test]
fn import_with_no_names_is_equivalent_to_require() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/import-default", caps).unwrap();
    ctx.set_i18n_translations(HashMap::from([(
        "nav.audio".to_string(),
        "Audio".to_string(),
    )]));
    ctx.load_script(
        r#"
function init()
    local i18n = import("mesh.i18n")
    label = i18n.t("nav.audio")
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("label"), Some(serde_json::json!("Audio")));
}

#[test]
fn import_renames_freely() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/import-rename", caps).unwrap();
    ctx.set_i18n_translations(HashMap::from([(
        "nav.battery".to_string(),
        "Battery".to_string(),
    )]));
    ctx.load_script(
        r#"
function init()
    local translate = import("mesh.i18n", "t")
    label = translate("nav.battery")
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("label"), Some(serde_json::json!("Battery")));
}

#[test]
fn mesh_i18n_updates_existing_function_after_catalog_refresh() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/i18n-refresh", caps).unwrap();
    ctx.set_i18n_translations(HashMap::from([(
        "nav.volume".to_string(),
        "Volume".to_string(),
    )]));
    ctx.load_script(
        r#"
function init()
    t = import("mesh.i18n", "t")
    label = t("nav.volume")
end
function refresh()
    label = t("nav.volume")
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    ctx.set_i18n_translations(HashMap::from([(
        "nav.volume".to_string(),
        "Hlasitosť".to_string(),
    )]));
    ctx.call_handler("refresh", &[]).unwrap();

    assert_eq!(ctx.state.get("label"), Some(serde_json::json!("Hlasitosť")));
}

#[test]
fn import_interface_command_member_is_callable() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/import-iface", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local VolumeChanged = import("mesh.audio@>=1.0", "VolumeChanged")
    seen_level = 0
    VolumeChanged:on(function(event) seen_level = event.level end)
    VolumeChanged:emit({ level = 71 })
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("seen_level"), Some(serde_json::json!(71)));
}

#[test]
fn import_component_definition_member() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/import-component", caps).unwrap();
    ctx.load_script(
        r#"
local source = import("./child.mesh", "source")
child_source = source
"#,
    )
    .unwrap();

    assert_eq!(
        ctx.state.get("child_source"),
        Some(serde_json::json!("./child.mesh"))
    );
}

#[test]
fn import_requires_string_specifier() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/import-bad-spec", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    ok = pcall(import, 42)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("ok"), Some(serde_json::json!(false)));
}

#[test]
fn pcall_unsupported_require_is_false_without_diagnostic() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/unsupported-test", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    ok = pcall(require, "not-a-real-module")
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("ok"), Some(serde_json::json!(false)));
    assert!(ctx.drain_diagnostics().is_empty());
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
