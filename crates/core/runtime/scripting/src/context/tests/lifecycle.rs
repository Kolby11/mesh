use super::super::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use serde_json::Value;

#[test]
fn module_object_keeps_events_without_legacy_state_and_exports() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.load_script(
        r#"
has_state = module.state ~= nil
has_exports = module.exports ~= nil
module.events.changed:subscribe(function(value)
    seen = value
end)
module.events.changed:emit("ready")
"#,
    )
    .unwrap();

    assert_eq!(ctx.state.get("has_state"), Some(serde_json::json!(false)));
    assert_eq!(ctx.state.get("has_exports"), Some(serde_json::json!(false)));
    assert_eq!(ctx.state.get("seen"), Some(serde_json::json!("ready")));
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
fn lifecycle_handlers_reuse_self_table() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@test/self-cache", caps).unwrap();
    ctx.load_script(
        r#"
first = nil
second = nil

function render(self)
    if first == nil then
        first = tostring(self)
    else
        second = tostring(self)
    end
end
"#,
    )
    .unwrap();

    ctx.call_render_lifecycle().unwrap();
    ctx.call_render_lifecycle().unwrap();

    assert_eq!(ctx.state.get("first"), ctx.state.get("second"));
}

// Run with:
// cargo test -p mesh-core-scripting --release -- cached_lifecycle_self_table_beats_rebuilding --ignored --nocapture
#[test]
#[ignore]
fn cached_lifecycle_self_table_beats_rebuilding() {
    use std::time::Instant;

    let source = r#"
function render(self)
    local id = self.meta.module_id
end
"#;
    let iterations = 20_000usize;

    let mut rebuild_ctx = ScriptContext::new("@test/self-rebuild", CapabilitySet::new()).unwrap();
    rebuild_ctx.load_script(source).unwrap();
    let rebuild_start = Instant::now();
    for _ in 0..iterations {
        rebuild_ctx.clear_cached_self_table_for_benchmark();
        rebuild_ctx.call_render_lifecycle().unwrap();
    }
    let rebuild_ns = rebuild_start.elapsed().as_nanos().max(1);

    let mut cached_ctx = ScriptContext::new("@test/self-cached", CapabilitySet::new()).unwrap();
    cached_ctx.load_script(source).unwrap();
    let cached_start = Instant::now();
    for _ in 0..iterations {
        cached_ctx.call_render_lifecycle().unwrap();
    }
    let cached_ns = cached_start.elapsed().as_nanos();

    eprintln!("rebuild_self_table={rebuild_ns}ns cached_self_table={cached_ns}ns");
    assert!(
        cached_ns < rebuild_ns,
        "cached lifecycle self table should beat rebuilding it per render"
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
