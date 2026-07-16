use super::*;

#[test]
fn json_diff_detects_changed_fields() {
    let prev = serde_json::json!({"percent": 65, "muted": false});
    let next = serde_json::json!({"percent": 66, "muted": false});
    let diff = json_field_diff("audio", &prev, &next);
    assert_eq!(diff.len(), 1, "only percent changed");
    assert_eq!(diff[0].0, "audio");
    assert_eq!(diff[0].1, "percent");
}

#[test]
fn json_diff_detects_added_fields() {
    let prev = serde_json::json!({"percent": 65});
    let next = serde_json::json!({"percent": 65, "muted": false});
    let diff = json_field_diff("audio", &prev, &next);
    assert_eq!(diff.len(), 1, "muted was added");
    assert_eq!(diff[0].1, "muted");
}

#[test]
fn json_diff_detects_removed_fields() {
    let prev = serde_json::json!({"percent": 65, "muted": false});
    let next = serde_json::json!({"percent": 65});
    let diff = json_field_diff("audio", &prev, &next);
    assert_eq!(diff.len(), 1, "muted was removed");
    assert_eq!(diff[0].1, "muted");
}

#[test]
fn json_diff_unchanged_returns_empty() {
    let prev = serde_json::json!({"percent": 65, "muted": false});
    let next = serde_json::json!({"percent": 65, "muted": false});
    let diff = json_field_diff("audio", &prev, &next);
    assert!(diff.is_empty(), "no fields changed");
}

#[test]
fn json_diff_multiple_changes() {
    let prev = serde_json::json!({"percent": 65, "muted": false, "volume": 0.5});
    let next = serde_json::json!({"percent": 70, "muted": true, "volume": 0.5});
    let diff = json_field_diff("audio", &prev, &next);
    assert_eq!(diff.len(), 2, "percent and muted changed, volume unchanged");
}

#[test]
fn json_diff_non_object_payloads_return_empty() {
    let prev = serde_json::json!("not an object");
    let next = serde_json::json!(42);
    let diff = json_field_diff("audio", &prev, &next);
    assert!(diff.is_empty(), "non-object payloads produce empty diff");
}

// cargo test -p mesh-core-shell --release -- dirty_service_state_short_circuits_tracked_field_scan --ignored --nocapture
#[test]
#[ignore = "release-only service invalidation short-circuit microbenchmark"]
fn dirty_service_state_short_circuits_tracked_field_scan() {
    let iterations = 500_000usize;
    let tracked_fields = (0..32)
        .map(|index| format!("field_{index}"))
        .collect::<HashSet<_>>();
    let mut previous = serde_json::Map::new();
    let mut next = serde_json::Map::new();
    for index in 0..32 {
        previous.insert(format!("field_{index}"), serde_json::json!(index));
        next.insert(format!("field_{index}"), serde_json::json!(index));
    }
    previous.insert("generation".into(), serde_json::json!(1));
    next.insert("generation".into(), serde_json::json!(2));
    let previous = serde_json::Value::Object(previous);
    let next = serde_json::Value::Object(next);

    let eager_started = std::time::Instant::now();
    let mut eager_total = 0usize;
    for _ in 0..iterations {
        let state_changed = std::hint::black_box(true);
        let tracked_changed = tracked_service_fields_changed(
            Some(std::hint::black_box(&previous)),
            std::hint::black_box(&next),
            std::hint::black_box(&tracked_fields),
        );
        eager_total += std::hint::black_box(state_changed || tracked_changed) as usize;
    }
    let eager_time = eager_started.elapsed();

    let short_circuit_started = std::time::Instant::now();
    let mut short_circuit_total = 0usize;
    for _ in 0..iterations {
        let needs_rebuild = std::hint::black_box(true)
            || tracked_service_fields_changed(
                Some(std::hint::black_box(&previous)),
                std::hint::black_box(&next),
                std::hint::black_box(&tracked_fields),
            );
        short_circuit_total += std::hint::black_box(needs_rebuild) as usize;
    }
    let short_circuit_time = short_circuit_started.elapsed();

    eprintln!(
        "dirty service invalidation over {iterations} updates with 32 tracked fields: eager {eager_time:?}; short-circuit {short_circuit_time:?}; ratio {:.1}x",
        eager_time.as_secs_f64() / short_circuit_time.as_secs_f64()
    );
    assert_eq!(eager_total, short_circuit_total);
    assert!(short_circuit_time < eager_time);
}

#[test]
fn fingerprint_equal_service_update_retains_exact_field_fallback() {
    let previous = serde_json::json!({ "percent": 42 });
    let next = serde_json::json!({ "percent": 73 });
    let tracked_fields = HashSet::from(["percent".to_owned()]);
    let mut state = ScriptState::new();
    state.set_with_fingerprint("audio", &previous, 41);
    state.clear_dirty();

    state.set_with_fingerprint("audio", &next, 41);
    assert!(!state.is_dirty());
    let retained = state.get("audio");
    assert!(tracked_service_fields_changed(
        retained.as_ref(),
        &next,
        &tracked_fields,
    ));
}

// cargo test -p mesh-core-shell --release -- dirty_service_state_skips_previous_payload_clone --ignored --nocapture
#[test]
#[ignore = "release-only previous service payload clone microbenchmark"]
fn dirty_service_state_skips_previous_payload_clone() {
    let iterations = 50_000usize;
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
    let mut state = ScriptState::new();
    state.set("audio", payload);

    let eager_started = std::time::Instant::now();
    let mut eager_total = 0usize;
    for _ in 0..iterations {
        let previous = std::hint::black_box(&state).get("audio");
        eager_total += std::hint::black_box(previous.is_some()) as usize;
    }
    let eager_time = eager_started.elapsed();

    let short_circuit_started = std::time::Instant::now();
    let mut short_circuit_total = 0usize;
    for _ in 0..iterations {
        let needs_rebuild = std::hint::black_box(true) || {
            let previous = std::hint::black_box(&state).get("audio");
            previous.is_some()
        };
        short_circuit_total += std::hint::black_box(needs_rebuild) as usize;
    }
    let short_circuit_time = short_circuit_started.elapsed();

    eprintln!(
        "previous 32-device payload over {iterations} dirty updates: eager clone {eager_time:?}; lazy fallback {short_circuit_time:?}; ratio {:.1}x",
        eager_time.as_secs_f64() / short_circuit_time.as_secs_f64()
    );
    assert_eq!(eager_total, short_circuit_total);
    assert!(short_circuit_time < eager_time);
}
