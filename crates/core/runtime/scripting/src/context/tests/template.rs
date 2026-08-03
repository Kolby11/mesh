use super::super::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use serde_json::Value;

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
fn template_expressions_use_component_lexical_scope_and_full_luau() {
    let mut ctx = ScriptContext::new("@test/template-expressions", CapabilitySet::new()).unwrap();
    let expressions = vec![
        "add(secret, 2)".to_string(),
        "0 or 5".to_string(),
        "add(item.value, secret)".to_string(),
    ];
    ctx.compile_and_execute_component(
        "local secret = 40\nlocal function add(a, b) return a + b end",
        &[],
        &expressions,
    )
    .unwrap();

    assert_eq!(
        ctx.evaluate_template_expression("add(secret, 2)", &serde_json::Map::new())
            .unwrap()
            .0,
        serde_json::json!(42)
    );
    assert_eq!(
        ctx.evaluate_template_expression("0 or 5", &serde_json::Map::new())
            .unwrap()
            .0,
        serde_json::json!(0)
    );
    let mut locals = serde_json::Map::new();
    locals.insert("item".into(), serde_json::json!({ "value": 2 }));
    assert_eq!(
        ctx.evaluate_template_expression("add(item.value, secret)", &locals)
            .unwrap()
            .0,
        serde_json::json!(42)
    );
}

#[test]
fn template_expression_reads_gate_unbound_public_member_writes() {
    let mut ctx = ScriptContext::new("@test/template-member-reads", CapabilitySet::new()).unwrap();
    ctx.compile_and_execute_component(
        "label = 'ready'\ntelemetry = 0\nfunction update_label() label = 'done' end\nfunction update_telemetry() telemetry = telemetry + 1 end",
        &[],
        &["label".to_string()],
    )
    .unwrap();
    ctx.evaluate_template_expression("label", &serde_json::Map::new())
        .unwrap();
    ctx.mark_template_dependencies_ready();
    ctx.state_mut().clear_dirty();

    ctx.call_handler("update_telemetry", &[]).unwrap();
    assert!(ctx.state().is_dirty());
    assert!(!ctx.dirty_state_affects_template());
    ctx.state_mut().clear_dirty();

    ctx.call_handler("update_label", &[]).unwrap();
    assert!(ctx.dirty_state_affects_template());
}

// cargo test -p mesh-core-scripting --release -- template_dependency_rust_gate_beats_lua_table_lookup --ignored --nocapture
#[test]
#[ignore = "release-only template dependency gate microbenchmark"]
fn template_dependency_rust_gate_beats_lua_table_lookup() {
    let mut ctx = ScriptContext::new("@mesh/dependency-gate", CapabilitySet::new()).unwrap();
    ctx.load_script("label = 'visible'\ntelemetry = 0").unwrap();
    let iterations = 1_000_000;
    let (lua_time, rust_time, lua_hits, rust_hits) =
        ctx.benchmark_template_dependency_gate(iterations);

    eprintln!(
        "{iterations} template dependency probes: Lua table {lua_time:?}; Rust cache {rust_time:?}; ratio {:.2}x",
        lua_time.as_secs_f64() / rust_time.as_secs_f64()
    );
    eprintln!(
        "MESH_PERF metric=template_dependency_gate_speedup value={:.6}",
        lua_time.as_secs_f64() / rust_time.as_secs_f64()
    );
    assert_eq!(lua_hits, rust_hits);
    assert!(rust_time < lua_time);
}

#[test]
fn template_member_dependencies_are_conservative_until_first_evaluation_finishes() {
    let mut ctx =
        ScriptContext::new("@test/template-dependency-readiness", CapabilitySet::new()).unwrap();
    ctx.compile_and_execute_component(
        "label = 'ready'; function update() label = label == 'ready' and 'done' or 'ready' end",
        &[],
        &[],
    )
    .unwrap();
    ctx.state_mut().clear_dirty();

    ctx.call_handler("update", &[]).unwrap();
    assert!(ctx.dirty_state_affects_template());

    ctx.state_mut().clear_dirty();
    ctx.mark_template_dependencies_ready();
    ctx.call_handler("update", &[]).unwrap();
    assert!(
        !ctx.dirty_state_affects_template(),
        "an authoritative empty dependency set may skip unrelated writes"
    );
}

#[test]
fn pure_public_member_expressions_reuse_unchanged_values() {
    let mut ctx = ScriptContext::new("@test/template-value-cache", CapabilitySet::new()).unwrap();
    let expressions = vec![
        "left".to_string(),
        "right".to_string(),
        "item.value".to_string(),
        "format(left)".to_string(),
    ];
    ctx.compile_and_execute_component(
        "left = 'a'; right = 'b'; function format(value) return '[' .. value .. ']' end; function update_left() left = 'c' end",
        &[],
        &expressions,
    )
    .unwrap();
    ctx.evaluate_template_expression("left", &serde_json::Map::new())
        .unwrap();
    ctx.evaluate_template_expression("right", &serde_json::Map::new())
        .unwrap();
    let mut locals = serde_json::Map::new();
    locals.insert("item".into(), serde_json::json!({ "value": 7 }));
    ctx.evaluate_template_expression("item.value", &locals)
        .unwrap();
    ctx.evaluate_template_expression("format(left)", &serde_json::Map::new())
        .unwrap();
    assert!(ctx.template_expression_cache_contains("left"));
    assert!(ctx.template_expression_cache_contains("right"));
    assert!(!ctx.template_expression_cache_contains("item.value"));
    assert!(!ctx.template_expression_cache_contains("format(left)"));
    ctx.mark_template_dependencies_ready();
    ctx.state_mut().clear_dirty();

    ctx.call_handler("update_left", &[]).unwrap();
    assert_eq!(
        ctx.evaluate_template_expression("right", &serde_json::Map::new())
            .unwrap()
            .0,
        serde_json::json!("b")
    );
    assert_eq!(ctx.template_expression_cache_hits(), 1);
    assert_eq!(
        ctx.evaluate_template_expression("left", &serde_json::Map::new())
            .unwrap()
            .0,
        serde_json::json!("c")
    );
    assert_eq!(ctx.template_expression_cache_hits(), 1);
}

#[test]
fn template_expression_cache_accumulates_changes_until_template_evaluation() {
    let mut ctx = ScriptContext::new("@test/template-value-cache", CapabilitySet::new()).unwrap();
    let expressions = vec!["left".to_string(), "right".to_string()];
    ctx.compile_and_execute_component(
        "left = 'a'; right = 'b'; function update_left() left = 'c' end; function update_right() right = 'd' end",
        &[],
        &expressions,
    )
    .unwrap();
    ctx.evaluate_template_expression("left", &serde_json::Map::new())
        .unwrap();
    ctx.evaluate_template_expression("right", &serde_json::Map::new())
        .unwrap();
    ctx.mark_template_dependencies_ready();
    ctx.state_mut().clear_dirty();

    ctx.call_handler("update_left", &[]).unwrap();
    ctx.call_handler("update_right", &[]).unwrap();

    assert_eq!(
        ctx.evaluate_template_expression("left", &serde_json::Map::new())
            .unwrap()
            .0,
        serde_json::json!("c")
    );
    assert_eq!(
        ctx.evaluate_template_expression("right", &serde_json::Map::new())
            .unwrap()
            .0,
        serde_json::json!("d")
    );
}
