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
