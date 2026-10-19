use super::super::*;
use super::common::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn unchanged_member_state_write_is_skipped() {
    let mut ctx = ScriptContext::new("@mesh/member-state", CapabilitySet::default()).unwrap();
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

#[test]
fn host_seeded_global_is_visible_before_script_runs() {
    let caps = CapabilitySet::default();
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

    let mut first = ScriptContext::new("@mesh/first", CapabilitySet::default()).unwrap();
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

    let mut second = ScriptContext::new("@mesh/second", CapabilitySet::default()).unwrap();
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

#[test]
fn unknown_method_reads_state_field_as_nil() {
    // Unknown keys fall through to the service proxy. When no service has
    // emitted yet the Rust-owned context store is empty, so the result is nil.
    let caps = CapabilitySet::from_ids(["service.audio.read"]);
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
    let caps = CapabilitySet::default();
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
        ScriptContext::new("@test/reactive-type-transition", CapabilitySet::default()).unwrap();
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
