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
    let prev = serde_json::json!({"percent": 65, "muted": false, "balance": 0.5});
    let next = serde_json::json!({"percent": 70, "muted": true, "balance": 0.5});
    let diff = json_field_diff("audio", &prev, &next);
    assert_eq!(
        diff.len(),
        2,
        "percent and muted changed, balance unchanged"
    );
}

#[test]
fn json_diff_non_object_payloads_return_empty() {
    let prev = serde_json::json!("not an object");
    let next = serde_json::json!(42);
    let diff = json_field_diff("audio", &prev, &next);
    assert!(diff.is_empty(), "non-object payloads produce empty diff");
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
