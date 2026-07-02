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
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_storage_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "mesh-context-storage-{name}-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&root);
    root
}

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

function render()
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

    ctx.call_handler("render", &[]).unwrap();

    assert_eq!(ctx.state.get("count"), Some(serde_json::json!(2)));
    assert_eq!(ctx.state.get("latest_count"), Some(serde_json::json!(1)));
    assert_eq!(
        ctx.state.get("exported_label"),
        Some(serde_json::json!("Audio"))
    );
}

#[test]
fn lifecycle_self_meta_is_passed_to_init_and_render() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/test-component", caps).unwrap();
    ctx.load_script(
        r#"
init_module = ""
render_kind = ""
global_self_kind = self.meta.kind

function init(self)
    init_module = self.meta.module_id
end

function render(self)
    render_kind = self.meta.kind
    render_instance = self.meta.instance_id
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.call_render_lifecycle().unwrap();

    assert_eq!(
        ctx.state.get("init_module"),
        Some(serde_json::json!("@mesh/test-component"))
    );
    assert_eq!(
        ctx.state.get("render_kind"),
        Some(serde_json::json!("frontend"))
    );
    assert_eq!(
        ctx.state.get("render_instance"),
        Some(serde_json::json!("@mesh/test-component"))
    );
    assert_eq!(
        ctx.state.get("global_self_kind"),
        Some(serde_json::json!("frontend"))
    );
}

#[test]
fn lifecycle_self_storage_supports_json_values_snapshot_and_diagnostics() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/storage-component", caps).unwrap();
    ctx.load_script(
        r#"
storage_language = ""
storage_missing = false
snapshot_theme = ""
render_language = ""

function init(self)
    self.storage.language = "sk"
    self.storage.theme = { name = "dark", accents = { "blue", "green" } }
    self.storage.removed = true
    self.storage.removed = nil
    storage_language = self.storage.language
    storage_missing = self.storage.removed == nil
    storage_snapshot = self.storage:snapshot()
    snapshot_theme = storage_snapshot.theme.name
    self.storage.invalid = function() return true end
end

function render(self)
    render_language = self.storage.language
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.call_render_lifecycle().unwrap();

    assert_eq!(
        ctx.state.get("storage_language"),
        Some(serde_json::json!("sk"))
    );
    assert_eq!(
        ctx.state.get("storage_missing"),
        Some(serde_json::json!(true))
    );
    assert_eq!(
        ctx.state.get("snapshot_theme"),
        Some(serde_json::json!("dark"))
    );
    assert_eq!(
        ctx.state.get("render_language"),
        Some(serde_json::json!("sk"))
    );

    let diagnostics = ctx.drain_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].interface, "self.storage");
    assert!(diagnostics[0].reason.contains("unsupported storage value"));
}

#[test]
fn frontend_storage_flushes_on_unmount_and_loads_before_init() {
    let root = temp_storage_root("frontend-flush");
    let caps = CapabilitySet::new();
    let mut writer =
        ScriptContext::new_with_storage_root("@mesh/storage-lifecycle", caps.clone(), &root)
            .unwrap();
    writer
        .load_script(
            r#"
function init(self)
    self.storage.counter = 1
end

function render(self)
    self.storage.counter = 2
end

function unmount(self)
    self.storage.counter = 3
end
"#,
        )
        .unwrap();

    writer.call_init().unwrap();
    writer.call_render_lifecycle().unwrap();

    let mut before_flush =
        ScriptContext::new_with_storage_root("@mesh/storage-lifecycle", caps.clone(), &root)
            .unwrap();
    before_flush
        .load_script("function init(self)\nloaded = self.storage.counter\nend")
        .unwrap();
    before_flush.call_init().unwrap();
    assert_eq!(before_flush.state.get("loaded"), None);

    writer.call_handler("unmount", &[]).unwrap();

    let mut reader =
        ScriptContext::new_with_storage_root("@mesh/storage-lifecycle", caps, &root).unwrap();
    reader
        .load_script("function init(self)\nloaded = self.storage.counter\nend")
        .unwrap();
    reader.call_init().unwrap();
    assert_eq!(reader.state.get("loaded"), Some(serde_json::json!(3)));
}

#[test]
fn frontend_storage_is_isolated_by_component_instance() {
    let root = temp_storage_root("frontend-instance-scope");
    let caps = CapabilitySet::new();
    let mut first = ScriptContext::new_with_storage_scope(
        "@mesh/module",
        "@mesh/component",
        "panel/first",
        caps.clone(),
        &root,
    )
    .unwrap();
    first
        .load_script("function unmount(self) self.storage.value = 'first' end")
        .unwrap();
    first.call_handler("unmount", &[]).unwrap();

    let mut second = ScriptContext::new_with_storage_scope(
        "@mesh/module",
        "@mesh/component",
        "panel/second",
        caps,
        &root,
    )
    .unwrap();
    second
        .load_script("function init(self) loaded = self.storage.value end")
        .unwrap();
    second.call_init().unwrap();

    assert_eq!(second.state.get("loaded"), None);
}

#[test]
fn frontend_storage_persistence_failure_is_diagnostic_and_keeps_memory_state() {
    let root = temp_storage_root("frontend-failure");
    std::fs::write(&root, "not a directory").unwrap();
    let caps = CapabilitySet::new();
    let mut ctx =
        ScriptContext::new_with_storage_root("@mesh/storage-failure", caps, &root).unwrap();
    ctx.load_script(
        r#"
function init(self)
    self.storage.value = "kept"
end

function render(self)
    latest = self.storage.value
end

function unmount(self)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    ctx.call_handler("unmount", &[]).unwrap();
    ctx.call_render_lifecycle().unwrap();

    assert_eq!(ctx.state.get("latest"), Some(serde_json::json!("kept")));
    let diagnostics = ctx.drain_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.reason.contains("storage persistence failed"))
    );
}

#[test]
fn frontend_storage_render_reads_track_only_watched_key_writes() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/storage-watch", caps).unwrap();
    ctx.load_script(
        r#"
function render(self)
    rendered_theme = self.storage.theme
end

function set_watched()
    self.storage.theme = "dark"
end

function set_unwatched()
    self.storage.locale = "sk"
end
"#,
    )
    .unwrap();

    ctx.state_mut().clear_dirty();
    ctx.call_render_lifecycle().unwrap();
    assert!(ctx.tracked_storage_keys().contains("theme"));

    ctx.state_mut().clear_dirty();
    ctx.call_handler("set_unwatched", &[]).unwrap();
    assert!(!ctx.state().is_dirty());

    ctx.call_handler("set_watched", &[]).unwrap();
    assert!(ctx.state().is_dirty());
}

#[test]
fn legacy_on_render_is_not_a_render_lifecycle_fallback() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/legacy-component", caps).unwrap();
    ctx.load_script(
        r#"
render_count = 0

function init()
    initialized = true
end

function onRender()
    render_count = render_count + 1
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();
    assert!(!ctx.call_render_lifecycle().unwrap());

    assert_eq!(ctx.state.get("initialized"), Some(serde_json::json!(true)));
    assert_eq!(ctx.state.get("render_count"), Some(serde_json::json!(0)));
}

#[test]
fn public_member_inspection_keeps_locals_private_and_hooks_reserved() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/member-test", caps).unwrap();
    ctx.load_script(
        r#"
local private_count = 1
local function private_helper()
end

public_count = 2

function public_action()
    public_count = public_count + 1
end

function render(self)
end

function render()
end
"#,
    )
    .unwrap();

    assert_eq!(ctx.public_field_names(), vec!["public_count".to_string()]);
    assert_eq!(
        ctx.public_function_names(),
        vec!["public_action".to_string()]
    );
    assert!(ctx.state.get("private_count").is_none());

    ctx.call_handler("public_action", &[]).unwrap();
    assert_eq!(ctx.state.get("public_count"), Some(serde_json::json!(3)));
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

    assert_eq!(
        ctx.state.get("label"),
        Some(serde_json::json!("nav.volume"))
    );
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

    assert_eq!(
        ctx.state.get("label"),
        Some(serde_json::json!("nav.volume"))
    );
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

    assert_eq!(ctx.state.get("label"), Some(serde_json::json!("nav.audio")));
}

#[test]
fn import_renames_freely() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/import-rename", caps).unwrap();
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

    assert_eq!(
        ctx.state.get("label"),
        Some(serde_json::json!("nav.battery"))
    );
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
fn host_value_update_refreshes_snapshot_without_dirty_generation() {
    let mut state = ScriptState::new();
    assert_eq!(state.snapshot(), serde_json::json!({}));
    let initial_generation = state.snapshot_generation();

    state.set_host_value("elements", serde_json::json!({ "root": true }));

    assert_eq!(state.snapshot_generation(), initial_generation);
    assert_eq!(
        state.snapshot(),
        serde_json::json!({ "elements": { "root": true } })
    );
    assert!(!state.is_dirty());
}

#[test]
fn snapshot_updates_after_cached_read() {
    let mut state = ScriptState::new();
    state.set("count", serde_json::json!(1));
    assert_eq!(state.snapshot(), serde_json::json!({ "count": 1 }));

    state.set("count", serde_json::json!(2));
    assert_eq!(state.snapshot(), serde_json::json!({ "count": 2 }));
}

#[test]
fn snapshot_reads_fresh_proxy_values() {
    let value = Arc::new(AtomicUsize::new(1));
    let proxy_value = Arc::clone(&value);
    let mut state = ScriptState::new();
    state.register_proxy(
        "count",
        Box::new(move || serde_json::json!(proxy_value.load(Ordering::SeqCst))),
        None,
    );

    assert_eq!(state.snapshot(), serde_json::json!({ "count": 1 }));
    value.store(2, Ordering::SeqCst);
    assert_eq!(state.snapshot(), serde_json::json!({ "count": 2 }));
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
fn mesh_popover_hide_can_request_hover_bridge_deferral() {
    let caps = CapabilitySet::new();
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

    let published = ctx.drain_published_events();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].channel, "shell.hide-popover");
    assert_eq!(
        published[0].payload,
        serde_json::json!({
            "surface_id": "@test/popover",
            "defer_for_hover_bridge": true,
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

#[test]
fn components_sharing_one_vm_keep_isolated_public_members() {
    // Two component instances on a single shared surface VM must keep their
    // public members private to their own _ENV — sharing the VM does not share
    // bare globals (that only happens through an explicit bind:this reference).
    let vm = SurfaceVm::new();

    let mut ctx_a = ScriptContext::new("@mesh/comp-a", CapabilitySet::new()).unwrap();
    ctx_a.attach_shared_vm(&vm);
    ctx_a
        .load_script("secret = \"a-value\"\nfunction init() end")
        .unwrap();
    ctx_a.call_init().unwrap();

    let mut ctx_b = ScriptContext::new("@mesh/comp-b", CapabilitySet::new()).unwrap();
    ctx_b.attach_shared_vm(&vm);
    ctx_b
        .load_script("secret = \"b-value\"\nfunction init() end")
        .unwrap();
    ctx_b.call_init().unwrap();

    assert_eq!(
        ctx_a.state.get("secret"),
        Some(serde_json::json!("a-value"))
    );
    assert_eq!(
        ctx_b.state.get("secret"),
        Some(serde_json::json!("b-value"))
    );
}

#[test]
fn interface_event_subscriptions_are_independent_on_shared_vm() {
    // The interface-event channel registry lives on each instance's _ENV, so a
    // subscription on one component must not register on another sharing the VM.
    let vm = SurfaceVm::new();

    let mut subscriber_caps = CapabilitySet::new();
    subscriber_caps.grant(Capability::new("service.audio.read"));
    let mut subscriber = ScriptContext::new("@mesh/subscriber", subscriber_caps).unwrap();
    subscriber.attach_shared_vm(&vm);
    subscriber.set_interface_catalog(audio_catalog());
    subscriber
        .load_script(
            r#"
function init()
    local audio = require("mesh.audio@>=1.0")
    audio.events.VolumeChanged:subscribe(function(_event) end)
end
"#,
        )
        .unwrap();
    subscriber.call_init().unwrap();

    let mut idle_caps = CapabilitySet::new();
    idle_caps.grant(Capability::new("service.audio.read"));
    let mut idle = ScriptContext::new("@mesh/idle", idle_caps).unwrap();
    idle.attach_shared_vm(&vm);
    idle.set_interface_catalog(audio_catalog());
    idle.load_script("function init() end").unwrap();
    idle.call_init().unwrap();

    assert!(subscriber.is_subscribed_to_interface_event("audio", "VolumeChanged"));
    assert!(!idle.has_interface_event_subscription_for_service("audio"));
}

#[test]
fn same_component_on_shared_vm_has_independent_self_channels() {
    // Two instances of the SAME component (same module_id) on one VM must not
    // share `self.Changed` — the regression that motivated moving the self-event
    // registry from globals to the per-instance _ENV.
    let vm = SurfaceVm::new();

    fn instance(vm: &SurfaceVm) -> ScriptContext {
        let mut ctx = ScriptContext::new("@mesh/item-row", CapabilitySet::new()).unwrap();
        ctx.attach_shared_vm(vm);
        ctx.load_script(
            r#"
hits = 0
function init(self)
    self.Changed:on(function(_event) hits = hits + 1 end)
    self.Changed:fire({})
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();
        ctx
    }

    let first = instance(&vm);
    let second = instance(&vm);

    // Each instance's own fire incremented only its own counter. If the channels
    // collided, the second instance's fire would also run the first's handler.
    assert_eq!(first.state.get("hits"), Some(serde_json::json!(1)));
    assert_eq!(second.state.get("hits"), Some(serde_json::json!(1)));
}

#[test]
fn live_binding_reads_and_calls_child_in_same_tick() {
    // A live `bind:this` proxy forwards straight to the child's `_ENV` in the
    // shared VM: the parent reads the child's current value and calls its real
    // function synchronously, with the real return value — no snapshot, no queue.
    let vm = SurfaceVm::new();

    let mut child = ScriptContext::new("@mesh/slider", CapabilitySet::new()).unwrap();
    child.attach_shared_vm(&vm);
    child
        .load_script(
            r#"
percent = 10
function set_volume(value)
    percent = value
    return percent
end
function init() end
"#,
        )
        .unwrap();
    child.call_init().unwrap();

    let mut parent = ScriptContext::new("@mesh/host", CapabilitySet::new()).unwrap();
    parent.attach_shared_vm(&vm);
    parent
        .load_script(
            r#"
returned = 0
observed = 0
function bump()
    returned = slider.set_volume(77)
    observed = slider.percent
end
function init() end
"#,
        )
        .unwrap();
    parent.call_init().unwrap();

    parent.install_live_binding("slider", &child).unwrap();
    parent.call_handler("bump", &[]).unwrap();

    // The bound call ran the child's real function and returned its real value.
    assert_eq!(parent.state.get("returned"), Some(serde_json::json!(77)));
    // The parent read the value the child mutated within the same tick (liveness).
    assert_eq!(parent.state.get("observed"), Some(serde_json::json!(77)));

    // Re-syncing the child surfaces the live `_ENV` mutation into its own state.
    child.resync_state();
    assert_eq!(child.state.get("percent"), Some(serde_json::json!(77)));
}

#[test]
fn live_binding_does_not_expose_host_internals() {
    // The live proxy is curated: host internals (`self`, `module`, `mesh`,
    // `require`, `__mesh_*`) and lifecycle hooks must not cross the boundary,
    // only the child's public members do.
    let vm = SurfaceVm::new();

    let mut child = ScriptContext::new("@mesh/child", CapabilitySet::new()).unwrap();
    child.attach_shared_vm(&vm);
    child
        .load_script("public_value = 5\nfunction init() end")
        .unwrap();
    child.call_init().unwrap();

    let mut parent = ScriptContext::new("@mesh/parent", CapabilitySet::new()).unwrap();
    parent.attach_shared_vm(&vm);
    parent
        .load_script(
            r#"
has_public = false
has_self = true
has_require = true
has_mesh = true
function probe()
    has_public = child.public_value == 5
    has_self = child.self ~= nil
    has_require = child.require ~= nil
    has_mesh = child.mesh ~= nil
end
function init() end
"#,
        )
        .unwrap();
    parent.call_init().unwrap();

    parent.install_live_binding("child", &child).unwrap();
    parent.call_handler("probe", &[]).unwrap();

    assert_eq!(
        parent.state.get("has_public"),
        Some(serde_json::json!(true))
    );
    assert_eq!(parent.state.get("has_self"), Some(serde_json::json!(false)));
    assert_eq!(
        parent.state.get("has_require"),
        Some(serde_json::json!(false))
    );
    assert_eq!(parent.state.get("has_mesh"), Some(serde_json::json!(false)));
}

#[test]
fn live_binding_routes_child_self_event_to_parent_in_same_tick() {
    // Child→parent events: the live proxy exposes the child's `self.<Event>`
    // channel, so a parent subscribes with `child.Event:on(fn)` and the child's
    // `self.Event:fire(...)` runs the parent's closure synchronously — same
    // channel table in the shared VM, no marshalling.
    let vm = SurfaceVm::new();

    let mut child = ScriptContext::new("@mesh/emitter", CapabilitySet::new()).unwrap();
    child.attach_shared_vm(&vm);
    child
        .load_script(
            r#"
local changed
function init(self)
    changed = self.Changed
end
function announce()
    changed:fire({ value = 42 })
end
"#,
        )
        .unwrap();
    child.call_init().unwrap();

    let mut parent = ScriptContext::new("@mesh/listener", CapabilitySet::new()).unwrap();
    parent.attach_shared_vm(&vm);
    parent
        .load_script(
            r#"
received = 0
function listen()
    emitter.Changed:on(function(event) received = event.value end)
end
function init() end
"#,
        )
        .unwrap();
    parent.call_init().unwrap();

    parent.install_live_binding("emitter", &child).unwrap();

    // Parent registers a real Lua closure on the child's live self-event channel.
    parent.call_handler("listen", &[]).unwrap();
    assert_eq!(parent.state.get("received"), Some(serde_json::json!(0)));

    // The child fires; the parent's closure runs synchronously in the same tick.
    child.call_handler("announce", &[]).unwrap();

    // The parent's `_ENV` was mutated by the fired callback; re-syncing surfaces
    // it into the parent's reactive state (what the shell does after the handler).
    parent.resync_state();
    assert_eq!(parent.state.get("received"), Some(serde_json::json!(42)));
}

#[test]
fn refs_read_live_element_geometry_from_published_metrics() {
    // `refs.<name>.<field>` reads the latest published metrics, so a handler sees
    // the geometry of the most recent paint — and re-reads pick up new values
    // without re-binding (a live reference, not a one-shot snapshot).
    let mut ctx = ScriptContext::new("@test/refs", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
width = -1
present = false
function measure()
    width = refs.panel.width
    present = refs.panel.present
end
"#,
    )
    .unwrap();

    ctx.apply_element_metrics(&serde_json::json!({
        "panel": { "width": 320.0, "height": 48.0 }
    }));
    ctx.call_handler("measure", &[]).unwrap();
    assert_eq!(ctx.state.get("width"), Some(serde_json::json!(320)));
    assert_eq!(ctx.state.get("present"), Some(serde_json::json!(true)));

    // A new paint publishes new metrics; the same `refs.panel` reads the update.
    ctx.apply_element_metrics(&serde_json::json!({
        "panel": { "width": 200.0, "height": 48.0 }
    }));
    ctx.call_handler("measure", &[]).unwrap();
    assert_eq!(ctx.state.get("width"), Some(serde_json::json!(200)));
}

#[test]
fn refs_absent_element_reads_nil_and_reports_not_present() {
    let mut ctx = ScriptContext::new("@test/refs-absent", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
width_state = "unknown"
missing_present = true
function probe()
    width_state = refs.ghost.width == nil and "absent" or "present"
    missing_present = refs.ghost.present
end
"#,
    )
    .unwrap();

    ctx.apply_element_metrics(&serde_json::json!({ "panel": { "width": 10.0 } }));
    ctx.call_handler("probe", &[]).unwrap();

    // A field on an element not in the current tree reads nil; `present` is false.
    assert_eq!(
        ctx.state.get("width_state"),
        Some(serde_json::json!("absent"))
    );
    assert_eq!(
        ctx.state.get("missing_present"),
        Some(serde_json::json!(false))
    );
}

#[test]
fn refs_methods_queue_element_actions_for_the_shell() {
    // `refs.<name>:focus()` / `:blur()` enqueue imperative actions the shell
    // drains and applies to the real widget tree — both call styles work.
    let mut ctx = ScriptContext::new("@test/refs-actions", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
function activate()
    refs.search_input:focus()
    refs.search_input.blur()
end
"#,
    )
    .unwrap();

    ctx.call_handler("activate", &[]).unwrap();
    let actions = ctx.drain_element_actions();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].target, "search_input");
    assert_eq!(actions[0].action, "focus");
    assert_eq!(actions[1].target, "search_input");
    assert_eq!(actions[1].action, "blur");

    // Draining is one-shot.
    assert!(ctx.drain_element_actions().is_empty());
}

#[test]
fn refs_scroll_into_view_queues_an_element_action() {
    // `refs.<name>:scroll_into_view()` is the third imperative method; the shell
    // turns it into scroll-offset adjustments on the real widget tree.
    let mut ctx = ScriptContext::new("@test/refs-scroll", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
function reveal()
    refs.row_42:scroll_into_view()
end
"#,
    )
    .unwrap();

    ctx.call_handler("reveal", &[]).unwrap();
    let actions = ctx.drain_element_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].target, "row_42");
    assert_eq!(actions[0].action, "scroll_into_view");
}

#[test]
fn refs_scroll_to_forwards_positional_args_without_self() {
    // `refs.x:scroll_to(top, left)` forwards its numeric args (in order, with the
    // `:`-call self table stripped) as a JSON array the shell reads.
    let mut ctx = ScriptContext::new("@test/refs-scroll-to", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
function jump()
    refs.list:scroll_to(120, 40)
end
function jump_top_only()
    refs.list:scroll_to(80)
end
"#,
    )
    .unwrap();

    ctx.call_handler("jump", &[]).unwrap();
    let actions = ctx.drain_element_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, "scroll_to");
    // Integer Lua literals serialize as JSON integers; the shell reads them via
    // `as_f64`, so assert on the numeric values rather than the JSON number kind.
    let nums: Vec<f64> = actions[0]
        .args
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect();
    assert_eq!(nums, vec![120.0, 40.0]);

    ctx.call_handler("jump_top_only", &[]).unwrap();
    let actions = ctx.drain_element_actions();
    let nums: Vec<f64> = actions[0]
        .args
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect();
    assert_eq!(nums, vec![80.0]);
}

#[test]
fn refs_method_options_table_is_separated_from_positional_args() {
    // A DOM-style options table (`{ smooth = true }`) is captured into `options`,
    // distinct from positional numeric args and from the stripped `self` table.
    let mut ctx = ScriptContext::new("@test/refs-options", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
function smooth_jump()
    refs.list:scroll_to(100, { smooth = true, duration = 300 })
end
function smooth_reveal()
    refs.row:scroll_into_view({ smooth = true })
end
"#,
    )
    .unwrap();

    ctx.call_handler("smooth_jump", &[]).unwrap();
    let actions = ctx.drain_element_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, "scroll_to");
    assert_eq!(actions[0].args.as_array().unwrap().len(), 1);
    assert_eq!(actions[0].args[0].as_f64(), Some(100.0));
    assert_eq!(
        actions[0].options.get("smooth").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        actions[0].options.get("duration").and_then(|v| v.as_f64()),
        Some(300.0)
    );

    ctx.call_handler("smooth_reveal", &[]).unwrap();
    let actions = ctx.drain_element_actions();
    // No positional args, options-only — `self` table must not leak into either.
    assert!(actions[0].args.as_array().unwrap().is_empty());
    assert_eq!(
        actions[0].options.get("smooth").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn refs_value_write_queues_set_value_via_assignment_and_method() {
    // `refs.x.value = "..."` (assignment) and `refs.x:set_value("...")` (method)
    // both queue a set_value action carrying the new text.
    let mut ctx = ScriptContext::new("@test/refs-set-value", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
function assign()
    refs.field.value = "hello"
end
function call_method()
    refs.field:set_value("world")
end
"#,
    )
    .unwrap();

    ctx.call_handler("assign", &[]).unwrap();
    let actions = ctx.drain_element_actions();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].target, "field");
    assert_eq!(actions[0].action, "set_value");
    assert_eq!(actions[0].args[0].as_str(), Some("hello"));

    ctx.call_handler("call_method", &[]).unwrap();
    let actions = ctx.drain_element_actions();
    assert_eq!(actions[0].action, "set_value");
    assert_eq!(actions[0].args[0].as_str(), Some("world"));
}

#[test]
fn refs_write_to_readonly_field_errors() {
    // Only `value` is writable; assigning to any other field is a hard error.
    let mut ctx = ScriptContext::new("@test/refs-readonly", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
function bad()
    refs.field.width = 50
end
"#,
    )
    .unwrap();

    assert!(ctx.call_handler("bad", &[]).is_err());
}
