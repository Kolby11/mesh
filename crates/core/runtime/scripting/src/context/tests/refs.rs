use super::super::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;

#[test]
fn refs_read_live_element_geometry_from_published_metrics() {
    // `refs.<name>.<field>` reads the latest published metrics, so a handler sees
    // the geometry of the most recent paint — and re-reads pick up new values
    // without re-binding (a live reference, not a one-shot snapshot).
    let mut ctx = ScriptContext::new("@test/refs", CapabilitySet::default()).unwrap();
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
fn refs_cache_element_proxies_without_stale_metrics() {
    let mut ctx = ScriptContext::new("@test/refs-cache", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
same_proxy = false
same_method = false
width = -1
function measure()
    local first = refs.panel
    local second = refs.panel
    same_proxy = first == second
    same_method = first.focus == second.focus
    width = second.width
end
"#,
    )
    .unwrap();

    ctx.apply_element_metrics(&serde_json::json!({
        "panel": { "width": 320.0, "height": 48.0 }
    }));
    ctx.call_handler("measure", &[]).unwrap();
    assert_eq!(ctx.state.get("same_proxy"), Some(serde_json::json!(true)));
    assert_eq!(ctx.state.get("same_method"), Some(serde_json::json!(true)));
    assert_eq!(ctx.state.get("width"), Some(serde_json::json!(320)));

    ctx.apply_element_metrics(&serde_json::json!({
        "panel": { "width": 200.0, "height": 48.0 }
    }));
    ctx.call_handler("measure", &[]).unwrap();
    assert_eq!(ctx.state.get("width"), Some(serde_json::json!(200)));
}

#[test]
fn refs_share_live_metrics_across_surface_component_contexts() {
    let surface_vm = SurfaceVm::new();
    let mut root = ScriptContext::new("@test/root", CapabilitySet::default()).unwrap();
    let mut child = ScriptContext::new("@test/child", CapabilitySet::default()).unwrap();
    root.attach_shared_vm(&surface_vm);
    child.attach_shared_vm(&surface_vm);
    root.load_script("function init() end").unwrap();
    child
        .load_script(
            r#"
width = -1
function measure()
    width = refs.panel.width
end
"#,
        )
        .unwrap();

    root.apply_element_metrics(&serde_json::json!({
        "panel": { "width": 280.0 }
    }));
    child.call_handler("measure", &[]).unwrap();

    assert_eq!(child.state.get("width"), Some(serde_json::json!(280)));
}

#[test]
fn element_metrics_fingerprint_skips_unchanged_lua_publication() {
    let mut ctx = ScriptContext::new("@test/refs-fingerprint", CapabilitySet::default()).unwrap();
    ctx.load_script("function init() end").unwrap();
    let first = serde_json::json!({ "panel": { "width": 320.0 } });
    let changed = serde_json::json!({ "panel": { "width": 200.0 } });

    ctx.apply_element_metrics_with_fingerprint(&first, 41);
    ctx.apply_element_metrics_with_fingerprint(&changed, 41);
    ctx.load_script(
        r#"
width = -1
function measure()
    width = refs.panel.width
end
"#,
    )
    .unwrap();
    ctx.call_handler("measure", &[]).unwrap();

    assert_eq!(ctx.state.get("width"), Some(serde_json::json!(320)));

    ctx.apply_element_metrics_with_fingerprint(&changed, 42);
    ctx.call_handler("measure", &[]).unwrap();
    assert_eq!(ctx.state.get("width"), Some(serde_json::json!(200)));
}

#[test]
fn refs_absent_element_reads_nil_and_reports_not_present() {
    let mut ctx = ScriptContext::new("@test/refs-absent", CapabilitySet::default()).unwrap();
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
    let mut ctx = ScriptContext::new("@test/refs-actions", CapabilitySet::default()).unwrap();
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
    let mut ctx = ScriptContext::new("@test/refs-scroll", CapabilitySet::default()).unwrap();
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
    let mut ctx = ScriptContext::new("@test/refs-scroll-to", CapabilitySet::default()).unwrap();
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
    let mut ctx = ScriptContext::new("@test/refs-options", CapabilitySet::default()).unwrap();
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
    let mut ctx = ScriptContext::new("@test/refs-set-value", CapabilitySet::default()).unwrap();
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
    let mut ctx = ScriptContext::new("@test/refs-readonly", CapabilitySet::default()).unwrap();
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
