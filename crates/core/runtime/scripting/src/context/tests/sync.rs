use super::super::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use serde_json::Value;

#[test]
fn mesh_request_redraw_marks_dirty_without_global_change() {
    let caps = CapabilitySet::default();
    let mut ctx = ScriptContext::new("@test/redraw", caps).unwrap();
    ctx.load_script(
        r#"
function request()
    mesh.ui.request_redraw()
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
fn sync_state_from_lua_discovers_new_globals_from_write_log() {
    let caps = CapabilitySet::default();
    let mut ctx = ScriptContext::new("@test/write-log", caps).unwrap();
    ctx.load_script(
        r#"
count = 1

function add_later()
    late_value = count + 41
end
"#,
    )
    .unwrap();

    assert_eq!(ctx.state.get("late_value"), None);
    ctx.call_handler("add_later", &[]).unwrap();
    assert_eq!(ctx.state.get("late_value"), Some(Value::from(42)));
    assert!(ctx.has_user_global_key_for_test("late_value"));
}

#[test]
fn handler_only_context_discovers_late_global_without_repeating_full_scan() {
    let mut ctx =
        ScriptContext::new("@test/handler-only-write-log", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
function add_later()
    late_value = 42
end
"#,
    )
    .unwrap();

    assert!(ctx.state().keys().is_empty());
    ctx.call_handler("add_later", &[]).unwrap();
    assert_eq!(ctx.state.get("late_value"), Some(Value::from(42)));
    assert!(ctx.has_user_global_key_for_test("late_value"));
}

#[test]
fn nil_writes_remove_public_state_and_invalidate_template_dependencies() {
    let mut ctx = ScriptContext::new("@test/nil-deletion", CapabilitySet::default()).unwrap();
    ctx.compile_and_execute_component(
        "visible = 'shown'\ndetails = { title = 'ready' }\nfunction clear() visible = nil; details = nil end\nfunction restore() visible = 'restored' end",
        &[],
        &["visible".to_string(), "details".to_string()],
    )
    .unwrap();

    ctx.evaluate_template_expression("visible", &serde_json::Map::new())
        .unwrap();
    ctx.evaluate_template_expression("details", &serde_json::Map::new())
        .unwrap();
    ctx.mark_template_dependencies_ready();
    ctx.state_mut().clear_dirty();
    let previous_generation = ctx.state().mutation_generation();

    ctx.call_handler("clear", &[]).unwrap();

    assert_eq!(ctx.state().get("visible"), None);
    assert_eq!(ctx.state().get("details"), None);
    assert!(!ctx.state().keys().contains(&"visible".to_string()));
    assert!(!ctx.state().keys().contains(&"details".to_string()));
    assert!(ctx.state().is_dirty());
    assert!(ctx.dirty_state_affects_template());
    assert!(ctx.state().mutation_generation() > previous_generation);

    ctx.state_mut().clear_dirty();
    ctx.call_handler("restore", &[]).unwrap();
    assert_eq!(ctx.state().get("visible"), Some(Value::from("restored")));
}

#[test]
fn side_channel_pending_flag_drains_published_events() {
    let capabilities = CapabilitySet::from_ids(["service.debug.control"]);
    let mut ctx = ScriptContext::new("@mesh/debug-inspector", capabilities).unwrap();
    ctx.load_script(
        r#"
function publish()
    mesh.events.publish("shell.toggle-debug-overlay", {})
end
"#,
    )
    .unwrap();

    assert!(!ctx.pending_side_channels_for_test());
    ctx.call_lua_function_without_sync_for_test("publish")
        .unwrap();
    assert!(ctx.pending_side_channels_for_test());

    let events = ctx.drain_published_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].channel, "shell.toggle-debug-overlay");
    assert!(!ctx.pending_side_channels_for_test());
}
