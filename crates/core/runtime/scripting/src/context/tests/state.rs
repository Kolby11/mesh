use super::super::*;
use super::common::*;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_elements::VariableStore;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn unchanged_member_state_write_is_skipped() {
    let mut ctx = ScriptContext::new("@mesh/member-state", CapabilitySet::new()).unwrap();
    ctx.set_member_state("label", serde_json::json!("stable"))
        .unwrap();
    let generation = ctx.state().mutation_generation();

    assert!(
        !ctx.set_member_state_if_changed("label", serde_json::json!("stable"))
            .unwrap()
    );
    assert_eq!(ctx.state().mutation_generation(), generation);
    assert!(
        ctx.set_member_state_if_changed("label", serde_json::json!("changed"))
            .unwrap()
    );
    assert_ne!(ctx.state().mutation_generation(), generation);

    let generation = ctx.state().mutation_generation();
    let changed = serde_json::json!("changed");
    assert!(
        !ctx.set_member_state_if_changed_ref("label", &changed)
            .unwrap()
    );
    assert_eq!(ctx.state().mutation_generation(), generation);

    let changed_again = serde_json::json!({ "nested": [1, 2, 3] });
    assert!(
        ctx.set_member_state_if_changed_ref("label", &changed_again)
            .unwrap()
    );
    assert_eq!(ctx.state().get_ref("label"), Some(&changed_again));
}

// Run with:
// cargo test -p mesh-core-scripting --release -- unchanged_member_state_write_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only unchanged member-state write microbenchmark"]
fn unchanged_member_state_write_benchmark() {
    use std::time::Instant;

    let value = serde_json::json!({
        "title": "stable prop",
        "items": [1, 2, 3, 4],
        "enabled": true
    });
    let iterations = 100_000usize;
    let mut eager = ScriptContext::new("@mesh/eager-member", CapabilitySet::new()).unwrap();
    let mut gated = ScriptContext::new("@mesh/gated-member", CapabilitySet::new()).unwrap();
    let mut borrowed = ScriptContext::new("@mesh/borrowed-member", CapabilitySet::new()).unwrap();
    eager.set_member_state("config", value.clone()).unwrap();
    gated.set_member_state("config", value.clone()).unwrap();
    borrowed.set_member_state("config", value.clone()).unwrap();

    let eager_started = Instant::now();
    for _ in 0..iterations {
        eager
            .set_member_state("config", std::hint::black_box(value.clone()))
            .unwrap();
    }
    let eager_time = eager_started.elapsed();

    let gated_started = Instant::now();
    let mut changed = 0usize;
    for _ in 0..iterations {
        changed += gated
            .set_member_state_if_changed("config", std::hint::black_box(value.clone()))
            .unwrap() as usize;
    }
    let gated_time = gated_started.elapsed();

    let borrowed_started = Instant::now();
    let mut borrowed_changed = 0usize;
    for _ in 0..iterations {
        borrowed_changed += borrowed
            .set_member_state_if_changed_ref("config", std::hint::black_box(&value))
            .unwrap() as usize;
    }
    let borrowed_time = borrowed_started.elapsed();

    eprintln!(
        "unchanged member prop: eager {eager_time:?}; owned gate {gated_time:?}; borrowed gate {borrowed_time:?}; borrowed/owned ratio {:.1}x; changed={changed}/{borrowed_changed}",
        gated_time.as_secs_f64() / borrowed_time.as_secs_f64()
    );
    assert_eq!(changed, 0);
    assert_eq!(borrowed_changed, 0);
    assert!(gated_time < eager_time);
    assert!(borrowed_time < gated_time);
}

#[test]
fn host_seeded_global_is_visible_before_script_runs() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
    ctx.seed_context_global("seeded", serde_json::json!("ready"))
        .unwrap();
    ctx.load_script(
        r#"
seed_seen = seeded
"#,
    )
    .unwrap();

    assert_eq!(ctx.state.get("seed_seen"), Some(serde_json::json!("ready")));
}

#[test]
fn shared_vm_keeps_host_seeded_globals_local_across_templates_and_handlers() {
    let vm = SurfaceVm::new();
    let expression = "this.package.id".to_string();

    let mut first = ScriptContext::new("@mesh/first", CapabilitySet::new()).unwrap();
    first.attach_shared_vm(&vm);
    first
        .seed_context_global(
            "this",
            serde_json::json!({ "package": { "id": "@mesh/first" } }),
        )
        .unwrap();
    first
        .compile_and_execute_component(
            "handler_seen = ''\nfunction capture() handler_seen = this.package.id end",
            &[],
            std::slice::from_ref(&expression),
        )
        .unwrap();

    let mut second = ScriptContext::new("@mesh/second", CapabilitySet::new()).unwrap();
    second.attach_shared_vm(&vm);
    second
        .seed_context_global(
            "this",
            serde_json::json!({ "package": { "id": "@mesh/second" } }),
        )
        .unwrap();
    second
        .compile_and_execute_component(
            "handler_seen = ''\nfunction capture() handler_seen = this.package.id end",
            &[],
            std::slice::from_ref(&expression),
        )
        .unwrap();

    let empty_locals = serde_json::Map::new();
    assert_eq!(
        first
            .evaluate_template_expression(&expression, &empty_locals)
            .unwrap()
            .0,
        serde_json::json!("@mesh/first")
    );
    assert_eq!(
        second
            .evaluate_template_expression(&expression, &empty_locals)
            .unwrap()
            .0,
        serde_json::json!("@mesh/second")
    );

    first.call_handler("capture", &[]).unwrap();
    second.call_handler("capture", &[]).unwrap();
    assert_eq!(
        first.state.get("handler_seen"),
        Some(serde_json::json!("@mesh/first"))
    );
    assert_eq!(
        second.state.get("handler_seen"),
        Some(serde_json::json!("@mesh/second"))
    );
}

// cargo test -p mesh-core-scripting --release -- removed_legacy_module_state_mirror_avoids_proxy_snapshot_serialization --ignored --nocapture
#[test]
#[ignore = "release-only legacy module-state mirror microbenchmark"]
fn removed_legacy_module_state_mirror_avoids_proxy_snapshot_serialization() {
    use std::time::Instant;

    let mut ctx =
        ScriptContext::new("@mesh/module-mirror-benchmark", CapabilitySet::new()).unwrap();
    ctx.load_script("").unwrap();
    for index in 0..64 {
        ctx.state_mut().set(
            format!("value_{index}"),
            serde_json::json!({
                "label": format!("value {index}"),
                "samples": [1, 2, 3, 4, 5, 6, 7, 8]
            }),
        );
    }
    ctx.state_mut().register_proxy(
        "service",
        Box::new(|| serde_json::json!({"percent": 72, "muted": false})),
        None,
    );
    let iterations = 20_000usize;

    let mirrored_started = Instant::now();
    let mut mirrored_total = 0usize;
    for _ in 0..iterations {
        mirrored_total =
            mirrored_total.wrapping_add(ctx.legacy_module_state_mirror_for_benchmark());
    }
    let mirrored_time = mirrored_started.elapsed();

    let removed_started = Instant::now();
    let mut removed_total = 0u64;
    for _ in 0..iterations {
        removed_total =
            removed_total.wrapping_add(std::hint::black_box(ctx.state().mutation_generation()));
    }
    let removed_time = removed_started.elapsed();

    eprintln!(
        "legacy module.state mirror: serialized {mirrored_time:?}; removed-path bookkeeping {removed_time:?}; ratio {:.1}x; totals={mirrored_total}/{removed_total}",
        mirrored_time.as_secs_f64() / removed_time.as_secs_f64()
    );
    assert!(mirrored_total > 0);
    assert!(removed_time < mirrored_time);
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
fn reactive_globals_preserve_scalar_table_transitions() {
    let mut ctx =
        ScriptContext::new("@test/reactive-type-transition", CapabilitySet::new()).unwrap();
    ctx.load_script(
        r#"
value = 1

function to_table()
    value = { nested = 2 }
end

function mutate_table()
    value.nested = 3
end

function to_scalar()
    value = 4
end
"#,
    )
    .unwrap();

    ctx.call_handler("to_table", &[]).unwrap();
    assert_eq!(
        ctx.state.get("value"),
        Some(serde_json::json!({ "nested": 2 }))
    );

    ctx.call_handler("mutate_table", &[]).unwrap();
    assert_eq!(
        ctx.state.get("value"),
        Some(serde_json::json!({ "nested": 3 }))
    );

    ctx.call_handler("to_scalar", &[]).unwrap();
    assert_eq!(ctx.state.get("value"), Some(serde_json::json!(4)));

    ctx.load_script("value = value + 1\nfunction noop() end")
        .unwrap();
    assert_eq!(ctx.state.get("value"), Some(serde_json::json!(5)));
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
fn host_value_fingerprint_skips_unchanged_large_snapshot() {
    let mut state = ScriptState::new();
    let value = serde_json::json!({
        "root": {
            "x": 0,
            "y": 0,
            "width": 1280,
            "height": 56
        }
    });

    state.set_host_value_with_fingerprint("elements", value.clone(), 42);
    let generation = state.mutation_generation();

    state.set_host_value_with_fingerprint("elements", value.clone(), 42);
    assert_eq!(
        state.mutation_generation(),
        generation,
        "same producer fingerprint should skip host-value replacement"
    );
    assert_eq!(state.get("elements"), Some(value.clone()));

    let changed = serde_json::json!({
        "root": {
            "x": 0,
            "y": 0,
            "width": 960,
            "height": 56
        }
    });
    state.set_host_value_with_fingerprint("elements", changed.clone(), 43);
    assert_ne!(state.mutation_generation(), generation);
    assert_eq!(state.get("elements"), Some(changed));
}

#[test]
fn reactive_fingerprint_setter_preserves_dirty_semantics() {
    let mut state = ScriptState::new();
    let initial = serde_json::json!({ "available": true, "percent": 42 });
    let changed = serde_json::json!({ "available": true, "percent": 73 });

    state.set_with_fingerprint("audio", &initial, 11);
    assert!(state.is_dirty());
    assert_eq!(state.get("audio"), Some(initial.clone()));
    state.clear_dirty();
    let generation = state.mutation_generation();

    state.set_with_fingerprint("audio", &initial, 11);
    assert!(!state.is_dirty());
    assert_eq!(state.mutation_generation(), generation);

    state.set_with_fingerprint("audio", &changed, 12);
    assert!(state.is_dirty());
    assert!(state.mutation_generation() > generation);
    assert_eq!(state.get("audio"), Some(changed));
}

#[test]
fn lazy_reactive_fingerprint_skips_value_construction() {
    let mut state = ScriptState::new();
    let constructions = std::cell::Cell::new(0usize);

    state.set_with_fingerprint_lazy("last_service_update", 41, || {
        constructions.set(constructions.get() + 1);
        serde_json::json!({ "name": "audio", "source_module": "@mesh/pipewire" })
    });
    state.clear_dirty();
    state.set_with_fingerprint_lazy("last_service_update", 41, || {
        constructions.set(constructions.get() + 1);
        serde_json::json!({ "name": "audio", "source_module": "@mesh/pipewire" })
    });

    assert_eq!(constructions.get(), 1);
    assert!(!state.is_dirty());
}

// cargo test -p mesh-core-scripting --release -- reactive_fingerprint_setter_beats_clone_and_deep_compare --ignored --nocapture
#[test]
#[ignore = "release-only reactive service-state fingerprint microbenchmark"]
fn reactive_fingerprint_setter_beats_clone_and_deep_compare() {
    let payload = serde_json::json!({
        "available": true,
        "devices": (0..32)
            .map(|index| serde_json::json!({
                "id": format!("sink-{index}"),
                "name": format!("Audio device {index}"),
                "volume": 64
            }))
            .collect::<Vec<_>>()
    });
    let fingerprint = ScriptContext::service_payload_fingerprint(&payload);
    let iterations = 50_000usize;

    let mut compared = ScriptState::new();
    compared.set("audio", payload.clone());
    compared.clear_dirty();
    let compared_started = std::time::Instant::now();
    for _ in 0..iterations {
        compared.set("audio", std::hint::black_box(&payload).clone());
    }
    let compared_time = compared_started.elapsed();

    let mut fingerprinted = ScriptState::new();
    fingerprinted.set_with_fingerprint("audio", &payload, fingerprint);
    fingerprinted.clear_dirty();
    let fingerprinted_started = std::time::Instant::now();
    for _ in 0..iterations {
        fingerprinted.set_with_fingerprint("audio", std::hint::black_box(&payload), fingerprint);
    }
    let fingerprinted_time = fingerprinted_started.elapsed();

    eprintln!(
        "unchanged reactive payload over {iterations} writes: clone/deep-compare {compared_time:?}; fingerprint {fingerprinted_time:?}; ratio {:.2}x",
        compared_time.as_secs_f64() / fingerprinted_time.as_secs_f64()
    );
    assert_eq!(compared.get("audio"), fingerprinted.get("audio"));
    assert!(!compared.is_dirty());
    assert!(!fingerprinted.is_dirty());
    assert!(fingerprinted_time < compared_time);
}

// Run with:
// cargo test -p mesh-core-scripting --release -- host_value_fingerprint_beats_repeated_deep_compare --ignored --nocapture
#[test]
#[ignore]
fn host_value_fingerprint_beats_repeated_deep_compare() {
    use std::time::Instant;

    let mut large_map = serde_json::Map::new();
    for index in 0..1_000usize {
        large_map.insert(
            format!("node_{index}"),
            serde_json::json!({
                "x": index,
                "y": index + 1,
                "width": 20,
                "height": 12,
                "label": format!("node {index}")
            }),
        );
    }
    let large_value = serde_json::Value::Object(large_map);
    let iterations = 20_000usize;

    let mut deep_state = ScriptState::new();
    deep_state.set_host_value("elements", large_value.clone());
    let deep_start = Instant::now();
    for _ in 0..iterations {
        deep_state.set_host_value("elements", large_value.clone());
    }
    let deep_ns = deep_start.elapsed().as_nanos().max(1);

    let mut fingerprint_state = ScriptState::new();
    fingerprint_state.set_host_value_with_fingerprint("elements", large_value.clone(), 99);
    let fingerprint_start = Instant::now();
    for _ in 0..iterations {
        fingerprint_state.set_host_value_with_fingerprint("elements", large_value.clone(), 99);
    }
    let fingerprint_ns = fingerprint_start.elapsed().as_nanos();

    eprintln!("deep_compare={deep_ns}ns fingerprint_skip={fingerprint_ns}ns");
    assert!(
        fingerprint_ns < deep_ns,
        "fingerprint host writes should be faster for unchanged large values"
    );
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
fn proxy_snapshot_reuses_cached_variables_but_keeps_proxy_fresh() {
    let value = Arc::new(AtomicUsize::new(1));
    let proxy_value = Arc::clone(&value);
    let mut state = ScriptState::new();
    state.set(
        "local",
        serde_json::json!({
            "nested": [1, 2, 3],
            "label": "cached"
        }),
    );
    state.register_proxy(
        "live",
        Box::new(move || serde_json::json!(proxy_value.load(Ordering::SeqCst))),
        None,
    );

    assert_eq!(
        state.snapshot(),
        serde_json::json!({
            "local": {
                "nested": [1, 2, 3],
                "label": "cached"
            },
            "live": 1
        })
    );
    value.store(2, Ordering::SeqCst);
    assert_eq!(
        state.snapshot(),
        serde_json::json!({
            "local": {
                "nested": [1, 2, 3],
                "label": "cached"
            },
            "live": 2
        })
    );
}

// cargo test -p mesh-core-scripting --release -- cached_proxy_snapshot_variables_beat_rebuilding_locals --ignored --nocapture
#[test]
#[ignore = "release-only proxy snapshot variable-cache microbenchmark"]
fn cached_proxy_snapshot_variables_beat_rebuilding_locals() {
    let mut state = ScriptState::new();
    for index in 0..128 {
        state.set(
            format!("value_{index}"),
            serde_json::json!({
                "label": format!("value {index}"),
                "samples": [1, 2, 3, 4, 5, 6, 7, 8]
            }),
        );
    }
    state.register_proxy(
        "service",
        Box::new(|| serde_json::json!({"percent": 72, "muted": false})),
        None,
    );
    let iterations = 20_000usize;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let mut object = serde_json::Map::with_capacity(state.variables.len() + 1);
        for (key, value) in &state.variables {
            object.insert(key.clone(), value.as_ref().clone());
        }
        object.insert(
            "service".to_string(),
            serde_json::json!({"percent": 72, "muted": false}),
        );
        old_total = old_total.wrapping_add(std::hint::black_box(object.len()));
    }
    let old_time = old_started.elapsed();

    let _ = state.snapshot();
    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let snapshot = state.snapshot();
        new_total = new_total.wrapping_add(std::hint::black_box(
            snapshot.as_object().map_or(0, serde_json::Map::len),
        ));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "proxy snapshot locals: rebuild {old_time:?}; cached {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

#[test]
fn script_state_clone_shares_variable_values() {
    let mut state = ScriptState::new();
    state.set(
        "elements",
        serde_json::json!({
            "root": {
                "x": 0,
                "y": 0,
                "width": 1280,
                "height": 56
            }
        }),
    );

    let cloned = state.clone();

    assert_eq!(
        state.value_arc_ptr("elements"),
        cloned.value_arc_ptr("elements"),
        "cloning ScriptState should not recursively clone JSON variable values"
    );
    assert_eq!(cloned.get("elements"), state.get("elements"));
}

// Run with:
// cargo test -p mesh-core-scripting --release -- script_state_clone_is_shallow_for_large_values --ignored --nocapture
#[test]
#[ignore]
fn script_state_clone_is_shallow_for_large_values() {
    use std::time::Instant;

    let mut large_map = serde_json::Map::new();
    for index in 0..1_000usize {
        large_map.insert(
            format!("node_{index}"),
            serde_json::json!({
                "x": index,
                "y": index + 1,
                "width": 20,
                "height": 12,
                "label": format!("node {index}")
            }),
        );
    }
    let large_value = serde_json::Value::Object(large_map);

    let mut deep_map = HashMap::new();
    deep_map.insert("elements".to_string(), large_value.clone());

    let mut state = ScriptState::new();
    state.set("elements", large_value);

    let iterations = 20_000usize;
    let deep_start = Instant::now();
    for _ in 0..iterations {
        let cloned = deep_map.clone();
        assert!(cloned.contains_key("elements"));
    }
    let deep_ns = deep_start.elapsed().as_nanos().max(1);

    let shallow_start = Instant::now();
    for _ in 0..iterations {
        let cloned = state.clone();
        assert!(cloned.value_arc_ptr("elements").is_some());
    }
    let shallow_ns = shallow_start.elapsed().as_nanos();

    eprintln!("deep_hashmap_clone={deep_ns}ns shallow_script_state_clone={shallow_ns}ns");
    assert!(
        shallow_ns.saturating_mul(2) <= deep_ns,
        "ScriptState clone should be at least 2x faster for large JSON values"
    );
}
