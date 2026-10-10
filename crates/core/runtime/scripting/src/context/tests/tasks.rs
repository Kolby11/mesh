use super::super::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use std::time::{Duration, Instant};

fn context(source: &str) -> ScriptContext {
    let mut ctx = ScriptContext::new("@mesh/test", CapabilitySet::default()).unwrap();
    ctx.load_script(source).unwrap();
    ctx
}

fn later(ms: u64) -> Instant {
    Instant::now() + Duration::from_millis(ms)
}

#[test]
fn delay_runs_its_callback_once_the_deadline_passes() {
    let mut ctx = context(
        r#"
fired = 0
function schedule()
    task.delay(50, function()
        fired += 1
    end)
end
"#,
    );
    ctx.call_handler("schedule", &[]).unwrap();
    assert_eq!(ctx.pending_task_count(), 1);
    let deadline = ctx.next_task_deadline().expect("pending deadline");

    assert!(ctx.run_due_tasks(Instant::now()).is_empty());
    assert_eq!(ctx.state.get("fired"), Some(serde_json::json!(0)));

    assert!(ctx.run_due_tasks(deadline).is_empty());
    assert_eq!(ctx.state.get("fired"), Some(serde_json::json!(1)));
    assert_eq!(ctx.pending_task_count(), 0);
    assert_eq!(ctx.next_task_deadline(), None);
}

#[test]
fn cancel_is_local_and_publishes_nothing() {
    let mut ctx = context(
        r#"
fired = false
local pending
function schedule()
    pending = task.delay(10, function()
        fired = true
    end)
end
function cancel()
    cancelled = pending:cancel()
    cancelled_again = task.cancel(pending)
end
"#,
    );
    ctx.call_handler("schedule", &[]).unwrap();
    ctx.call_handler("cancel", &[]).unwrap();
    assert!(ctx.drain_published_events().is_empty());
    assert_eq!(ctx.state.get("cancelled"), Some(serde_json::json!(true)));
    assert_eq!(
        ctx.state.get("cancelled_again"),
        Some(serde_json::json!(false))
    );
    assert_eq!(ctx.next_task_deadline(), None);
    assert!(ctx.run_due_tasks(later(1_000)).is_empty());
    assert_eq!(ctx.state.get("fired"), Some(serde_json::json!(false)));
}

#[test]
fn due_tasks_run_in_deadline_order() {
    let mut ctx = context(
        r#"
order = ""
function schedule()
    task.delay(30, function() order ..= "c" end)
    task.delay(10, function() order ..= "a" end)
    task.delay(20, function() order ..= "b" end)
end
"#,
    );
    ctx.call_handler("schedule", &[]).unwrap();
    assert!(ctx.run_due_tasks(later(1_000)).is_empty());
    assert_eq!(ctx.state.get("order"), Some(serde_json::json!("abc")));
}

#[test]
fn spawn_and_sleep_resume_a_coroutine_at_its_deadline() {
    let mut ctx = context(
        r#"
step = 0
function start()
    task.spawn(function(first)
        step = first
        task.sleep(25)
        step = 2
        task.sleep(25)
        step = 3
    end, 1)
end
"#,
    );
    ctx.call_handler("start", &[]).unwrap();
    assert_eq!(ctx.state.get("step"), Some(serde_json::json!(1)));
    assert_eq!(ctx.pending_task_count(), 1);

    let first = ctx.next_task_deadline().unwrap();
    assert!(ctx.run_due_tasks(first).is_empty());
    assert_eq!(ctx.state.get("step"), Some(serde_json::json!(2)));
    // A resumption that sleeps again is queued for later, not re-run now.
    assert_eq!(ctx.pending_task_count(), 1);

    let second = ctx.next_task_deadline().unwrap();
    assert!(second > first);
    assert!(ctx.run_due_tasks(second).is_empty());
    assert_eq!(ctx.state.get("step"), Some(serde_json::json!(3)));
    assert_eq!(ctx.pending_task_count(), 0);
}

#[test]
fn a_spawned_task_can_be_cancelled_while_it_sleeps() {
    let mut ctx = context(
        r#"
finished = false
local handle
function start()
    handle = task.spawn(function()
        task.sleep(10)
        finished = true
    end)
end
function stop()
    handle:cancel()
end
"#,
    );
    ctx.call_handler("start", &[]).unwrap();
    ctx.call_handler("stop", &[]).unwrap();
    assert!(ctx.run_due_tasks(later(1_000)).is_empty());
    assert_eq!(ctx.state.get("finished"), Some(serde_json::json!(false)));
}

#[test]
fn sleep_outside_a_task_and_bad_delays_raise_at_the_call() {
    let mut ctx = context(
        r#"
function sleep_in_handler()
    task.sleep(10)
end
function negative()
    task.delay(-1, function() end)
end
function not_a_number()
    task.delay("soon", function() end)
end
function infinite()
    task.delay(math.huge, function() end)
end
function stray_yield()
    task.spawn(function()
        coroutine.yield("not a sleep")
    end)
end
"#,
    );
    for handler in [
        "sleep_in_handler",
        "negative",
        "not_a_number",
        "infinite",
        "stray_yield",
    ] {
        assert!(
            ctx.call_handler(handler, &[]).is_err(),
            "{handler} should raise"
        );
    }
    assert_eq!(ctx.pending_task_count(), 0);
}

#[test]
fn a_failing_task_is_reported_and_the_rest_still_run() {
    let mut ctx = context(
        r#"
ran = false
function schedule()
    task.delay(1, function() error("boom") end)
    task.delay(2, function() ran = true end)
end
"#,
    );
    ctx.call_handler("schedule", &[]).unwrap();
    let errors = ctx.run_due_tasks(later(1_000));
    assert_eq!(errors.len(), 1);
    assert!(errors[0].to_string().contains("boom"), "{}", errors[0]);
    assert_eq!(ctx.state.get("ran"), Some(serde_json::json!(true)));
}

#[test]
fn pending_tasks_are_bounded() {
    let mut ctx = context(
        r#"
function flood()
    for _ = 1, 10000 do
        task.delay(1000, function() end)
    end
end
"#,
    );
    let error = ctx.call_handler("flood", &[]).unwrap_err();
    assert!(
        error.to_string().contains("too many pending tasks"),
        "{error}"
    );
}

#[test]
fn tasks_are_dropped_with_the_context() {
    let mut ctx = context(
        r#"
function schedule()
    task.delay(10, function() end)
end
"#,
    );
    ctx.call_handler("schedule", &[]).unwrap();
    assert_eq!(ctx.pending_task_count(), 1);
    ctx.uninit();
    assert_eq!(ctx.pending_task_count(), 0);
    assert_eq!(ctx.next_task_deadline(), None);
}

#[test]
fn removed_scheduler_channels_name_their_replacement() {
    let mut ctx = context(
        r#"
function old()
    mesh.events.publish("shell.schedule-handler", { key = "k", handler = "h", delay_ms = 10 })
end
"#,
    );
    let error = ctx.call_handler("old", &[]).unwrap_err();
    assert!(error.to_string().contains("task.delay"), "{error}");
}
