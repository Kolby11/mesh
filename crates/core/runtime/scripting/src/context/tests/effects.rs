use super::common::audio_catalog;
use crate::ScriptContext;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use serde_json::json;

#[test]
fn effects_track_service_fields_and_skip_unrelated_changes() {
    let mut ctx = ScriptContext::new(
        "@test/effect-fields",
        CapabilitySet::from_ids(["service.audio.read"]),
    )
    .unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.apply_service_payload("audio", &json!({"percent": 10, "muted": false}));
    ctx.load_script(
        r#"
local audio = require("mesh.audio@>=1.0")
local runs = 0
telemetry = 0
observed = 0
effect(function(self)
    runs = runs + 1
    observed = audio.percent
    executions = runs
    owner = self.meta.instance_id
end)
function unrelated() telemetry = telemetry + 1 end
"#,
    )
    .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("executions"), Some(json!(1)));
    assert!(ctx.run_reactive_effects().is_empty());
    ctx.call_handler("unrelated", &[]).unwrap();
    ctx.apply_service_payload("audio", &json!({"percent": 10, "muted": true}));
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("executions"), Some(json!(1)));
    ctx.apply_service_payload("audio", &json!({"percent": 20, "muted": true}));
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("executions"), Some(json!(2)));
    assert_eq!(ctx.state().get("observed"), Some(json!(20)));
}

#[test]
fn effects_settle_dependencies_without_running_independent_effects() {
    let mut ctx = ScriptContext::new("@test/effect-state", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
left = 1
right = 2
local a, b = 0, 0
effect(function() derived = left * 2; a = a + 1; left_runs = a end)
effect(function() shown = derived + 1 end)
effect(function() other = right * 3; b = b + 1; right_runs = b end)
function update() left = 3 end
"#,
    )
    .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    ctx.call_handler("update", &[]).unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("shown"), Some(json!(7)));
    assert_eq!(ctx.state().get("left_runs"), Some(json!(2)));
    assert_eq!(ctx.state().get("right_runs"), Some(json!(1)));
}

#[test]
fn effects_cleanup_before_rerun_and_cancel_exactly_once() {
    let mut ctx = ScriptContext::new("@test/effect-cleanup", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
value = 1
cleaned = ""
local handle = effect(function()
    local captured = value
    return function() cleaned = cleaned .. tostring(captured) end
end)
function update() value = 2 end
function stop() first_cancel = handle:cancel(); second_cancel = handle:cancel() end
"#,
    )
    .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    ctx.call_handler("update", &[]).unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("cleaned"), Some(json!("1")));
    ctx.call_handler("stop", &[]).unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("cleaned"), Some(json!("12")));
    assert_eq!(ctx.state().get("first_cancel"), Some(json!(true)));
    assert_eq!(ctx.state().get("second_cancel"), Some(json!(false)));
}

#[test]
fn failed_effect_retains_dependencies_and_does_not_stop_other_effects() {
    let mut ctx = ScriptContext::new("@test/effect-failure", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
fail = true
effect(function() if fail then error("local effect failure") end; recovered = true end)
effect(function() independent = "ready" end)
function recover() fail = false end
"#,
    )
    .unwrap();
    assert_eq!(ctx.run_reactive_effects().len(), 1);
    assert_eq!(ctx.state().get("independent"), Some(json!("ready")));
    assert!(ctx.run_reactive_effects().is_empty());
    ctx.call_handler("recover", &[]).unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("recovered"), Some(json!(true)));
}

#[test]
fn effect_cleanup_runs_on_source_reload_and_unmount() {
    let mut ctx = ScriptContext::new("@test/effect-lifetime", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
cleaned = 0
effect(function() return function() cleaned = cleaned + 1 end end)
"#,
    )
    .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    ctx.load_script("effect(function() return function() cleaned = cleaned + 10 end end)")
        .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("cleaned"), Some(json!(1)));
    let _ = ctx.call_handler("unmount", &[]);
    assert_eq!(ctx.state().get("cleaned"), Some(json!(11)));
}

#[test]
fn equal_props_replacement_does_not_reset_optimistic_state() {
    let mut ctx = ScriptContext::new("@test/effect-equal-props", CapabilitySet::default()).unwrap();
    ctx.state_mut().set("props", json!({"step": 5}));
    ctx.load_script(
        r#"
level = 50
local runs = 0
effect(function() runs = runs + 1; executions = runs; level = 50 end)
function scroll() level = level - 5 end
"#,
    )
    .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    ctx.call_handler("scroll", &[]).unwrap();
    ctx.state_mut().set("props", json!({"step": 5.0}));
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("level"), Some(json!(45)));
    assert_eq!(ctx.state().get("executions"), Some(json!(1)));
    ctx.state_mut().set("props", json!({"step": 10}));
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("executions"), Some(json!(2)));
}

#[test]
fn self_cancel_runs_returned_cleanup_once() {
    let mut ctx = ScriptContext::new("@test/effect-self-cancel", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
cleaned = 0
local handle
handle = effect(function()
    handle:cancel()
    return function() cleaned = cleaned + 1 end
end)
"#,
    )
    .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    assert!(ctx.run_reactive_effects().is_empty());
    assert_eq!(ctx.state().get("cleaned"), Some(json!(1)));
    assert!(!ctx.has_reactive_effects());
}

#[test]
fn compiled_legacy_render_emits_a_migration_diagnostic() {
    let mut ctx = ScriptContext::new("@test/legacy-render", CapabilitySet::default()).unwrap();
    ctx.load_script(
        r#"
__mesh_template_expressions = {}
function render() error("legacy hook must not run") end
"#,
    )
    .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    let diagnostics = ctx.drain_diagnostics();
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.category
        == crate::ScriptDiagnosticCategory::Lifecycle
        && diagnostic.reason.contains("effect(fn)")));
}

#[test]
#[ignore = "release-only unchanged reactive-effect workload"]
fn unchanged_effect_inputs_skip_callback_work_benchmark() {
    use std::time::{Duration, Instant};
    let mut ctx = ScriptContext::new(
        "@test/effect-idle-budget",
        CapabilitySet::from_ids(["service.audio.read"]),
    )
    .unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.apply_service_payload("audio", &json!({"percent": 50, "muted": false}));
    ctx.load_script(
        r#"
local audio = require("mesh.audio@>=1.0")
for index = 1, 32 do
    local label
    effect(function() label = string.format("Channel %d: %d%%", index, audio.percent) end)
end
"#,
    )
    .unwrap();
    assert!(ctx.run_reactive_effects().is_empty());
    let mut skipped = Duration::ZERO;
    let mut forced = Duration::ZERO;
    for iteration in 0..1_000 {
        let mut measure = |force: bool| {
            if force {
                ctx.invalidate_reactive_effects();
            }
            let started = Instant::now();
            assert!(ctx.run_reactive_effects().is_empty());
            if force {
                forced += started.elapsed();
            } else {
                skipped += started.elapsed();
            }
        };
        if iteration % 2 == 0 {
            measure(false);
            measure(true);
        } else {
            measure(true);
            measure(false);
        }
    }
    let ratio = forced.as_secs_f64() / skipped.as_secs_f64();
    eprintln!(
        "32 service-reading label effects, 1000 alternating unchanged/forced flushes: skipped {skipped:?}, forced {forced:?}, ratio {ratio:.3}"
    );
    eprintln!("MESH_PERF metric=unchanged_effect_inputs_speedup value={ratio:.6}");
    assert!(
        ratio >= 2.0,
        "unchanged inputs should avoid callback work: {ratio}"
    );
}
