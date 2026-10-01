use super::*;

/// `drain_requests` must always terminate, even given a batch larger than any
/// single legitimate frame could produce (whether from one huge initial
/// queue or a component/backend that keeps re-emitting the same request). A
/// no-op request (`PositionSurface` for a surface id with no live component)
/// is enough to exercise the ceiling without depending on component/service
/// wiring.
#[test]
fn drain_requests_terminates_and_diagnoses_a_batch_over_the_drain_budget() {
    let mut shell = Shell::new();
    let mut requests: VecDeque<CoreRequest> = (0..10_000)
        .map(|_| CoreRequest::PositionSurface {
            surface_id: "@mesh/does-not-exist".to_string(),
            margin_top: 0,
            margin_left: 0,
        })
        .collect();

    shell
        .drain_requests(&mut requests)
        .expect("an over-budget batch must be dropped, not returned as an error");

    assert!(
        requests.is_empty(),
        "the dropped remainder must not be left for a later drain pass"
    );
    assert!(
        shell
            .diagnostics
            .snapshot()
            .iter()
            .flat_map(|module| module.instances.iter())
            .flat_map(|instance| instance.issues.iter())
            .any(|issue| issue.issue_code.contains("request_drain_budget_exceeded")),
        "exceeding the drain budget must record a diagnosable lifecycle error"
    );
}

/// A batch at or under the budget still drains completely and records no
/// budget diagnostic.
#[test]
fn drain_requests_processes_a_normal_batch_without_dropping_anything() {
    let mut shell = Shell::new();
    let mut requests: VecDeque<CoreRequest> = (0..8)
        .map(|_| CoreRequest::PositionSurface {
            surface_id: "@mesh/does-not-exist".to_string(),
            margin_top: 0,
            margin_left: 0,
        })
        .collect();

    shell.drain_requests(&mut requests).unwrap();

    assert!(requests.is_empty());
    assert!(
        !shell
            .diagnostics
            .snapshot()
            .iter()
            .flat_map(|module| module.instances.iter())
            .flat_map(|instance| instance.issues.iter())
            .any(|issue| issue.issue_code.contains("request_drain_budget_exceeded"))
    );
}

fn set_prop_call(call_id: u64, value: i64) -> CoreRequest {
    CoreRequest::ServiceCall {
        interface: "mesh.settings".to_string(),
        command: "set_prop".to_string(),
        payload: serde_json::json!({ "module_id": "@test/none", "prop": "size", "value": value }),
        call_id,
        source_instance_id: "@test/caller".to_string(),
        source_module_id: "@test/caller".to_string(),
        source_capabilities: mesh_core_capability::CapabilitySet::from_ids([
            "service.settings.control",
        ]),
    }
}

/// A core service call runs as a follow-up effect. When that effect fails,
/// the failure is answered to the call and recorded against its source; it
/// must not leave the effect loop with an error that stops the shell.
#[test]
fn a_failing_core_service_effect_answers_its_call_without_stopping_the_shell() {
    let mut shell = Shell::new();
    let mut requests = VecDeque::from([set_prop_call(7, 1)]);

    shell
        .drain_requests(&mut requests)
        .expect("a failed effect is contained, not returned");

    assert!(!shell.pending_service_call_routes.contains_key(&7));
    assert!(shell.diagnostics.snapshot().iter().any(|entry| {
        entry.module_id == "@test/caller" && entry.health.to_string().contains("shell_effect_failed")
    }));
}

/// A durable write that arrives while another is pending waits for it
/// instead of failing, and only the latest write to one setting survives.
#[test]
fn durable_writes_wait_for_the_pending_write_and_the_latest_wins() {
    let mut shell = Shell::new();
    shell.pending_profile_write = Some(crate::shell::runtime::PendingProfileWrite {
        worker: std::thread::spawn(|| {}),
        call_ids: Vec::new(),
    });
    let mut requests = VecDeque::from([set_prop_call(1, 1), set_prop_call(2, 2)]);

    shell.drain_requests(&mut requests).unwrap();

    assert_eq!(shell.parked_durable_writes.len(), 1);
    assert!(
        !shell.pending_service_call_routes.contains_key(&1),
        "the superseded call is answered"
    );
    assert!(shell.pending_service_call_routes.contains_key(&2));

    shell.pending_profile_write.take().unwrap().worker.join().unwrap();
    shell.release_parked_durable_writes();
    shell.drain_requests(&mut VecDeque::new()).unwrap();

    assert!(shell.parked_durable_writes.is_empty());
    assert!(!shell.pending_service_call_routes.contains_key(&2));
}
