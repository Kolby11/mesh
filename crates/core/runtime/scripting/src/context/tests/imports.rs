use super::super::*;
use super::common::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use mesh_core_locale::{LocaleEngine, TranslationSet};
use serde_json::Value;
use std::collections::HashMap;

#[test]
fn require_import_installs_proxy() {
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
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
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
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
    let caps = CapabilitySet::from_ids(["service.audio.read"]);

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
    let caps = CapabilitySet::from_ids(["locale.read"]);
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
fn locale_host_exposes_locale_aware_formatters() {
    let caps = CapabilitySet::from_ids(["locale.read"]);
    let mut ctx = ScriptContext::new("@mesh/locale-format-test", caps).unwrap();
    let locale = LocaleEngine::new("en-US");
    let translator = locale.module_translator("@mesh/locale-format-test");
    ctx.set_i18n_translator(&translator);
    ctx.load_script(
        r#"
function init()
    local locale = require("mesh.locale")
    number = locale.format_number(1234567.89)
    date = locale.format_date(0, "short")
    duration = locale.format_duration(3675)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    assert_eq!(
        ctx.state.get("number"),
        Some(serde_json::json!("1,234,567.89"))
    );
    assert_eq!(ctx.state.get("date"), Some(serde_json::json!("01/01/1970")));
    assert_eq!(
        ctx.state.get("duration"),
        Some(serde_json::json!("1 h 1 min"))
    );
}

#[test]
fn locale_current_reads_the_host_owned_translation_snapshot() {
    let caps = CapabilitySet::from_ids(["locale.read"]);
    let mut ctx = ScriptContext::new("@mesh/locale-test", caps).unwrap();
    let mut locale = LocaleEngine::with_fallback_locale("sk-SK", "en");
    locale.load_module_translations(
        "@mesh/locale-test",
        TranslationSet {
            locale: "en".into(),
            messages: HashMap::from([(String::from("fallback"), String::from("Fallback"))]),
        },
    );
    locale.load_module_translations(
        "@mesh/locale-test",
        TranslationSet {
            locale: "sk-SK".into(),
            messages: HashMap::from([(String::from("greeting"), String::from("Ahoj"))]),
        },
    );
    let translator = locale.module_translator("@mesh/locale-test");
    let snapshot_revision = translator.snapshot_revision();
    ctx.set_i18n_translator(&translator);
    ctx.load_script(
        r#"
current_locale = ""
greeting = ""
fallback = ""
missing = ""
function read_locale()
    local i18n = require("mesh.i18n")
    current_locale = mesh.locale.current()
    greeting = i18n.t("greeting")
    fallback = i18n.t("fallback")
    missing = i18n.t("missing")
end
"#,
    )
    .unwrap();

    // The service payload is a projection for ordinary service consumers; it
    // must not be able to replace the locale/catalog snapshot used by host
    // locale and i18n APIs.
    ctx.apply_service_payload("locale", &serde_json::json!({ "current": "en" }));
    ctx.call_handler("read_locale", &[]).unwrap();
    assert_eq!(
        ctx.state.get("current_locale"),
        Some(serde_json::json!("sk-SK"))
    );
    assert_eq!(ctx.state.get("greeting"), Some(serde_json::json!("Ahoj")));
    assert_eq!(
        ctx.state.get("fallback"),
        Some(serde_json::json!("Fallback"))
    );
    assert_eq!(
        ctx.state.get("missing"),
        Some(serde_json::json!("!!missing"))
    );
    let misses = ctx.drain_localized_misses();
    assert_eq!(misses.len(), 1);
    assert_eq!(misses[0].snapshot_revision, snapshot_revision);
}

#[test]
fn locale_host_members_are_independently_capability_gated() {
    fn member_types(caps: CapabilitySet) -> (Value, Value) {
        let mut ctx = ScriptContext::new("@mesh/locale-capability-test", caps).unwrap();
        ctx.load_script(
            r#"
current_type = type(mesh.locale.current)
set_type = type(mesh.locale.set)
"#,
        )
        .unwrap();
        (
            ctx.state.get("current_type").unwrap().clone(),
            ctx.state.get("set_type").unwrap().clone(),
        )
    }

    assert_eq!(
        member_types(CapabilitySet::default()),
        (serde_json::json!("nil"), serde_json::json!("nil"))
    );

    let read = CapabilitySet::from_ids(["locale.read"]);
    assert_eq!(
        member_types(read),
        (serde_json::json!("function"), serde_json::json!("nil"))
    );

    let write = CapabilitySet::from_ids(["locale.write"]);
    assert_eq!(
        member_types(write),
        (serde_json::json!("nil"), serde_json::json!("function"))
    );
}

#[test]
fn locale_and_i18n_requires_are_denied_without_locale_capabilities() {
    let mut ctx =
        ScriptContext::new("@mesh/locale-capability-test", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
function init()
    i18n_ok = pcall(require, "mesh.i18n")
    locale_ok = pcall(require, "mesh.locale")
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("i18n_ok"), Some(serde_json::json!(false)));
    assert_eq!(ctx.state.get("locale_ok"), Some(serde_json::json!(false)));
}

#[test]
fn locale_write_capability_exposes_only_the_write_operation() {
    let caps = CapabilitySet::from_ids(["locale.write"]);
    let mut ctx = ScriptContext::new("@mesh/locale-write-test", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    require("mesh.locale").set("sk")
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();

    let events = ctx.drain_published_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].channel, "shell.set-locale");
    assert_eq!(events[0].payload, serde_json::json!({ "locale": "sk" }));
}

#[test]
fn named_locale_imports_enforce_read_and_write_members() {
    let write = CapabilitySet::from_ids(["locale.write"]);
    let mut write_ctx = ScriptContext::new("@mesh/locale-write-import", write).unwrap();
    write_ctx
        .load_script(
            r#"
function init()
    current_ok = pcall(import, "mesh.locale", "current")
    set_ok = pcall(import, "mesh.locale", "set")
end
"#,
        )
        .unwrap();
    write_ctx.call_init().unwrap();
    assert_eq!(
        write_ctx.state.get("current_ok"),
        Some(serde_json::json!(false))
    );
    assert_eq!(write_ctx.state.get("set_ok"), Some(serde_json::json!(true)));

    let read = CapabilitySet::from_ids(["locale.read"]);
    let mut read_ctx = ScriptContext::new("@mesh/locale-read-import", read).unwrap();
    read_ctx
        .load_script(
            r#"
function init()
    current_ok = pcall(import, "mesh.locale", "current")
    set_ok = pcall(import, "mesh.locale", "set")
end
"#,
        )
        .unwrap();
    read_ctx.call_init().unwrap();
    assert_eq!(
        read_ctx.state.get("current_ok"),
        Some(serde_json::json!(true))
    );
    assert_eq!(read_ctx.state.get("set_ok"), Some(serde_json::json!(false)));
}

#[test]
fn require_resolves_mesh_i18n_library_alias() {
    let caps = CapabilitySet::from_ids(["locale.read"]);
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
    assert!(ctx.drain_localized_misses().is_empty());
}

#[test]
fn missing_i18n_keys_are_visible_and_carry_owner_and_snapshot_metadata() {
    let caps = CapabilitySet::from_ids(["locale.read"]);
    let mut ctx = ScriptContext::new("@mesh/i18n-miss", caps).unwrap();
    ctx.set_i18n_translations(HashMap::new());
    ctx.load_script(
        r#"
function init()
    label = import("mesh.i18n", "t")("nav.missing")
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(
        ctx.state.get("label"),
        Some(serde_json::json!("!!nav.missing"))
    );
    let misses = ctx.drain_localized_misses();
    assert_eq!(misses.len(), 1);
    assert_eq!(misses[0].owner_module_id, "@mesh/i18n-miss");
    assert_eq!(misses[0].key.as_deref(), Some("nav.missing"));
    assert_eq!(misses[0].text, "!!nav.missing");
    assert_eq!(misses[0].snapshot_revision, 0);
    assert!(misses[0].missing);
}

#[test]
fn require_component_definition_specifier_returns_placeholder() {
    let caps = CapabilitySet::default();
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
    let caps = CapabilitySet::from_ids(["locale.read"]);
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
    let caps = CapabilitySet::from_ids(["locale.read", "locale.write"]);
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
    let caps = CapabilitySet::from_ids(["locale.read"]);
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
    let caps = CapabilitySet::from_ids(["locale.read"]);
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
    let caps = CapabilitySet::from_ids(["locale.read"]);
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
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
    let mut ctx = ScriptContext::new("@mesh/import-iface", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local VolumeChanged = import("mesh.audio@>=1.0", "VolumeChanged")
    seen_level = 0
    VolumeChanged:on(function(event) seen_level = event.level end)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.emit_interface_event(
        "audio",
        "VolumeChanged",
        &serde_json::json!({ "level": 71 }),
    )
    .unwrap();

    assert_eq!(ctx.state.get("seen_level"), Some(serde_json::json!(71)));
}

#[test]
fn import_component_definition_member() {
    let caps = CapabilitySet::default();
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
    let caps = CapabilitySet::default();
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
    let caps = CapabilitySet::default();
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
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
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
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
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
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
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
    let caps = CapabilitySet::from_ids(["theme.read"]);
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
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
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
