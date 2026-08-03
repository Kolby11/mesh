use super::super::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use serde_json::Value;

#[test]
fn mesh_request_redraw_marks_dirty_without_global_change() {
    let caps = CapabilitySet::new();
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

// Run with:
// cargo test -p mesh-core-scripting --release -- atomic_redraw_idle_check_beats_lua_global_read --ignored --nocapture
#[test]
#[ignore]
fn atomic_redraw_idle_check_beats_lua_global_read() {
    use std::time::Instant;

    let iterations = 1_000_000usize;
    let mut old_ctx = ScriptContext::new("@test/redraw-old", CapabilitySet::new()).unwrap();
    old_ctx.load_script("function noop() end").unwrap();
    let old_started = Instant::now();
    for _ in 0..iterations {
        old_ctx.old_global_redraw_flag_sync_for_benchmark();
    }
    let old_time = old_started.elapsed();

    let new_ctx = ScriptContext::new("@test/redraw-new", CapabilitySet::new()).unwrap();
    let new_started = Instant::now();
    let mut pending_count = 0usize;
    for _ in 0..iterations {
        pending_count += usize::from(new_ctx.pending_redraw_for_benchmark());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "idle redraw sync check: Lua global read {old_time:?}; atomic check {new_time:?}; ratio {:.1}x; pending={pending_count}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(pending_count, 0);
    assert!(new_time < old_time);
}

// Run with:
// cargo test -p mesh-core-scripting --release -- assigned_global_pending_flag_beats_empty_mutex_drain --ignored --nocapture
#[test]
#[ignore]
fn assigned_global_pending_flag_beats_empty_mutex_drain() {
    use std::time::Instant;

    let iterations = 1_000_000usize;
    let mut ctx = ScriptContext::new("@test/assigned-empty", CapabilitySet::new()).unwrap();
    ctx.load_script("value = 1\nfunction noop() end").unwrap();

    let drain_started = Instant::now();
    let mut drain_count = 0usize;
    for _ in 0..iterations {
        drain_count += ctx.old_empty_assigned_globals_drain_for_benchmark();
    }
    let drain_time = drain_started.elapsed();

    let pending_started = Instant::now();
    let mut pending_count = 0usize;
    for _ in 0..iterations {
        pending_count += usize::from(ctx.pending_assigned_globals_for_benchmark());
    }
    let pending_time = pending_started.elapsed();

    eprintln!(
        "assigned globals empty check: mutex drain {drain_time:?}; atomic pending {pending_time:?}; ratio {:.1}x; counts={drain_count}/{pending_count}",
        drain_time.as_secs_f64() / pending_time.as_secs_f64()
    );
    assert_eq!(drain_count, pending_count);
    assert!(pending_time < drain_time);
}

#[test]
fn sync_state_from_lua_discovers_new_globals_from_write_log() {
    let caps = CapabilitySet::new();
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
    let mut ctx = ScriptContext::new("@test/handler-only-write-log", CapabilitySet::new()).unwrap();
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

// cargo test -p mesh-core-scripting --release -- handler_only_discovery_flag_beats_repeated_env_scan --ignored --nocapture
#[test]
#[ignore = "release-only handler-only sync microbenchmark"]
fn handler_only_discovery_flag_beats_repeated_env_scan() {
    use std::time::Instant;

    let mut source = String::new();
    for index in 0..256 {
        source.push_str(&format!("function handler_{index}() return {index} end\n"));
    }
    source.push_str("function noop() end\n");
    let iterations = 20_000;

    let mut repeated_scan =
        ScriptContext::new("@test/handler-only-scan", CapabilitySet::new()).unwrap();
    repeated_scan.load_script(&source).unwrap();
    let scan_started = Instant::now();
    for _ in 0..iterations {
        repeated_scan
            .call_lua_function_without_sync_for_test("noop")
            .unwrap();
        repeated_scan.old_sync_state_from_lua_scan_for_benchmark();
    }
    let scan_time = scan_started.elapsed();

    let mut discovered =
        ScriptContext::new("@test/handler-only-discovered", CapabilitySet::new()).unwrap();
    discovered.load_script(&source).unwrap();
    let discovered_started = Instant::now();
    for _ in 0..iterations {
        discovered.call_handler("noop", &[]).unwrap();
    }
    let discovered_time = discovered_started.elapsed();

    eprintln!(
        "handler-only sync: repeated env scan {scan_time:?}; explicit discovery flag {discovered_time:?}; ratio {:.1}x",
        scan_time.as_secs_f64() / discovered_time.as_secs_f64()
    );
    assert!(discovered_time < scan_time);
}

// Run with:
// cargo test -p mesh-core-scripting --release -- sync_state_write_log_beats_full_env_scan --ignored --nocapture
#[test]
#[ignore]
fn sync_state_write_log_beats_full_env_scan() {
    use std::time::Instant;

    let mut source = String::new();
    for index in 0..1_000usize {
        source.push_str(&format!("value_{index} = {index}\n"));
    }
    source.push_str(
        r#"
function tick()
    value_500 = value_500 + 1
end
"#,
    );

    let iterations = 2_000usize;
    let mut old_ctx = ScriptContext::new("@test/old-sync", CapabilitySet::new()).unwrap();
    old_ctx.load_script(&source).unwrap();
    let old_start = Instant::now();
    for _ in 0..iterations {
        old_ctx
            .call_lua_function_without_sync_for_test("tick")
            .unwrap();
        old_ctx.old_sync_state_from_lua_scan_for_benchmark();
    }
    let old_ns = old_start.elapsed().as_nanos().max(1);

    let mut new_ctx = ScriptContext::new("@test/new-sync", CapabilitySet::new()).unwrap();
    new_ctx.load_script(&source).unwrap();
    let new_start = Instant::now();
    for _ in 0..iterations {
        new_ctx.call_handler("tick", &[]).unwrap();
    }
    let new_ns = new_start.elapsed().as_nanos();

    eprintln!("old_env_scan={old_ns}ns write_log_sync={new_ns}ns");
    assert!(
        new_ns < old_ns,
        "write-log sync should beat the old full _ENV scan path"
    );
}

// cargo test -p mesh-core-scripting --release -- scalar_write_log_beats_known_global_reads --ignored --nocapture
#[test]
#[ignore = "release-only scalar write-log sync microbenchmark"]
fn scalar_write_log_beats_known_global_reads() {
    use std::time::Instant;

    let mut source = String::new();
    for index in 0..512usize {
        source.push_str(&format!("value_{index} = {index}\n"));
    }
    source.push_str("function noop() end\n");
    let iterations = 5_000usize;

    let mut known_reads =
        ScriptContext::new("@test/scalar-known-reads", CapabilitySet::new()).unwrap();
    known_reads.load_script(&source).unwrap();
    let known_reads_started = Instant::now();
    for _ in 0..iterations {
        known_reads
            .call_lua_function_without_sync_for_test("noop")
            .unwrap();
        known_reads.sync_known_globals_with_scalar_gate_for_benchmark();
    }
    let known_reads_time = known_reads_started.elapsed();

    let mut write_logged =
        ScriptContext::new("@test/scalar-write-log", CapabilitySet::new()).unwrap();
    write_logged.load_script(&source).unwrap();
    let write_logged_started = Instant::now();
    for _ in 0..iterations {
        write_logged.call_handler("noop", &[]).unwrap();
    }
    let write_logged_time = write_logged_started.elapsed();

    eprintln!(
        "unchanged scalar sync: known-global reads {known_reads_time:?}; write-log proxy {write_logged_time:?}; ratio {:.1}x",
        known_reads_time.as_secs_f64() / write_logged_time.as_secs_f64()
    );
    assert!(write_logged_time < known_reads_time);
}

#[test]
fn side_channel_pending_flag_drains_published_events() {
    let mut ctx = ScriptContext::new("@test/side-channel-flag", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
function publish()
    mesh.events.publish("test.channel", { ok = true })
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
    assert_eq!(events[0].channel, "test.channel");
    assert!(!ctx.pending_side_channels_for_test());
}

// Run with:
// cargo test -p mesh-core-scripting --release -- empty_side_channel_pending_flag_beats_lock_drains --ignored --nocapture
#[test]
#[ignore]
fn empty_side_channel_pending_flag_beats_lock_drains() {
    use std::time::Instant;

    let iterations = 1_000_000usize;
    let mut old_ctx = ScriptContext::new("@test/old-side", CapabilitySet::new()).unwrap();
    let old_start = Instant::now();
    for _ in 0..iterations {
        old_ctx.old_sync_side_channels_for_benchmark();
    }
    let old_ns = old_start.elapsed().as_nanos().max(1);

    let mut new_ctx = ScriptContext::new("@test/new-side", CapabilitySet::new()).unwrap();
    let new_start = Instant::now();
    for _ in 0..iterations {
        new_ctx.sync_side_channels_for_benchmark();
    }
    let new_ns = new_start.elapsed().as_nanos();

    eprintln!("old_empty_side_channel_locks={old_ns}ns pending_flag_skip={new_ns}ns");
    assert!(
        new_ns < old_ns,
        "pending flag should beat locking every empty side-channel queue"
    );
}
