use super::common::*;
use super::*;

#[test]
fn latest_service_state_is_keyed_by_interface() {
    let mut shell = Shell::new();

    shell
        .broadcast_service_event(service_update(
            "audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0 }),
        ))
        .unwrap();

    assert!(shell.latest_service_state.contains_key("mesh.audio"));
    assert!(!shell.latest_service_state.contains_key("audio"));
    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(latest.interface, "mesh.audio");
    assert_eq!(latest.state["percent"], serde_json::json!(42.0));
}

#[test]
fn service_delivery_index_routes_updates_without_scanning_unrelated_components() {
    let audio_summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: vec!["audio".to_string()],
        cached_update_services: Vec::new(),
        interface_events: Vec::new(),
    })));
    let power_summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: vec!["power".to_string()],
        cached_update_services: Vec::new(),
        interface_events: Vec::new(),
    })));
    let fallback_summary = Arc::new(Mutex::new(None));
    let audio_state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let power_state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let fallback_state = Arc::new(Mutex::new(IndexedRecordingState::default()));

    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/audio-observer",
        audio_summary,
        Arc::clone(&audio_state),
    )));
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/power-observer",
        power_summary,
        Arc::clone(&power_state),
    )));
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/fallback-observer",
        fallback_summary,
        Arc::clone(&fallback_state),
    )));

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();

    assert_eq!(
        audio_state.lock().unwrap().observed,
        0,
        "indexed subscribers dispatch without repeating the observation gate"
    );
    assert_eq!(audio_state.lock().unwrap().handled.len(), 1);
    assert_eq!(
        power_state.lock().unwrap().observed,
        0,
        "summarized components for other services should not be scanned"
    );
    assert!(power_state.lock().unwrap().handled.is_empty());
    assert_eq!(
        fallback_state.lock().unwrap().observed,
        1,
        "unknown-summary components keep the legacy observation gate"
    );
    assert_eq!(fallback_state.lock().unwrap().handled.len(), 1);
}

#[test]
fn service_updates_are_cached_by_components_that_do_not_observe_them() {
    let audio_summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: vec!["audio".to_string()],
        cached_update_services: Vec::new(),
        interface_events: Vec::new(),
    })));
    let idle_summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        cached_update_services: vec!["audio".to_string()],
        ..Default::default()
    })));
    let audio_state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let idle_state = Arc::new(Mutex::new(IndexedRecordingState::default()));

    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/audio-observer",
        audio_summary,
        Arc::clone(&audio_state),
    )));
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/idle-surface",
        idle_summary,
        Arc::clone(&idle_state),
    )));

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();

    assert_eq!(audio_state.lock().unwrap().handled.len(), 1);
    assert!(
        audio_state.lock().unwrap().cached.is_empty(),
        "components that handle an update already own the payload"
    );
    assert!(
        idle_state.lock().unwrap().handled.is_empty(),
        "a surface that reads no audio field is not a delivery target"
    );
    assert_eq!(
        idle_state.lock().unwrap().cached.len(),
        1,
        "non-observing surfaces still cache the payload for runtimes created later"
    );
}

#[test]
fn service_delivery_index_routes_interface_events_by_name() {
    let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: Vec::new(),
        cached_update_services: Vec::new(),
        interface_events: vec![ServiceInterfaceEventSubscription {
            service: "audio".to_string(),
            event: "volume_changed".to_string(),
        }],
    })));
    let state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/interface-observer",
        summary,
        Arc::clone(&state),
    )));

    shell
        .deliver_service_event(&ServiceEvent::InterfaceEvent {
            service: "mesh.audio".to_string(),
            source_module: "@mesh/pipewire-audio".to_string(),
            name: "device_changed".to_string(),
            payload: serde_json::json!({}),
        })
        .unwrap();
    shell
        .deliver_service_event(&ServiceEvent::InterfaceEvent {
            service: "mesh.audio".to_string(),
            source_module: "@mesh/pipewire-audio".to_string(),
            name: "volume_changed".to_string(),
            payload: serde_json::json!({ "percent": 70.0 }),
        })
        .unwrap();

    let state = state.lock().unwrap();
    assert_eq!(state.observed, 0);
    assert_eq!(state.handled.len(), 1);
    assert!(matches!(
        &state.handled[0],
        ServiceEvent::InterfaceEvent { name, .. } if name == "volume_changed"
    ));
}

#[test]
fn service_delivery_index_deduplicates_component_subscriptions_when_rebuilt() {
    let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: vec!["audio".to_string(), "audio".to_string()],
        cached_update_services: vec!["power".to_string(), "power".to_string()],
        interface_events: vec![
            ServiceInterfaceEventSubscription {
                service: "audio".to_string(),
                event: "volume_changed".to_string(),
            },
            ServiceInterfaceEventSubscription {
                service: "audio".to_string(),
                event: "volume_changed".to_string(),
            },
        ],
    })));
    let state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/duplicate-observer",
        summary,
        Arc::clone(&state),
    )));

    shell.rebuild_service_delivery_index_if_needed();

    assert_eq!(
        shell.service_delivery_index.update_services["audio"],
        vec![0]
    );
    assert_eq!(
        shell.service_delivery_index.cached_update_services["power"],
        vec![0]
    );
    assert_eq!(
        shell.service_delivery_index.interface_events["audio"]["volume_changed"],
        vec![0]
    );

    shell
        .deliver_service_event(&service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 70.0 }),
        ))
        .unwrap();
    shell
        .deliver_service_event(&ServiceEvent::InterfaceEvent {
            service: "mesh.audio".to_string(),
            source_module: "@mesh/pipewire-audio".to_string(),
            name: "volume_changed".to_string(),
            payload: serde_json::json!({ "percent": 70.0 }),
        })
        .unwrap();

    let state = state.lock().unwrap();
    assert_eq!(state.observed, 0);
    assert_eq!(
        state.handled.len(),
        2,
        "each indexed event kind should be delivered exactly once"
    );
}

#[test]
fn service_delivery_index_rebuilds_when_marked_dirty() {
    let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: vec!["audio".to_string()],
        cached_update_services: Vec::new(),
        interface_events: Vec::new(),
    })));
    let state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/dynamic-observer",
        Arc::clone(&summary),
        Arc::clone(&state),
    )));

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();
    *summary.lock().unwrap() = Some(ServiceObservationSummary {
        update_services: vec!["power".to_string()],
        cached_update_services: Vec::new(),
        interface_events: Vec::new(),
    });
    shell.service_delivery_index.mark_dirty();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 60.0 }),
        ))
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.power",
            "@mesh/upower-power",
            serde_json::json!({ "available": true, "percentage": 88.0 }),
        ))
        .unwrap();

    let state = state.lock().unwrap();
    assert_eq!(state.observed, 0);
    assert_eq!(state.handled.len(), 2);
    assert!(matches!(
        &state.handled[1],
        ServiceEvent::Updated { service, .. } if service == "mesh.power"
    ));
}

#[test]
#[ignore = "release-only service delivery index microbenchmark"]
fn service_delivery_index_beats_full_component_scan_benchmark() {
    const COMPONENTS: usize = 256;
    const ITERATIONS: usize = 20_000;

    let mut shell = Shell::new();
    let mut states = Vec::new();
    for index in 0..COMPONENTS {
        let service = if index == 17 { "audio" } else { "power" };
        let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
            update_services: vec![service.to_string()],
            cached_update_services: Vec::new(),
            interface_events: Vec::new(),
        })));
        let state = Arc::new(Mutex::new(IndexedRecordingState::default()));
        states.push(Arc::clone(&state));
        shell.register_component(Box::new(IndexedRecordingComponent::new(
            &format!("@test/indexed-observer-{index:03}"),
            summary,
            state,
        )));
    }
    let event = service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 55.0 }),
    );

    let old_started = Instant::now();
    let mut old_hits = 0usize;
    for _ in 0..ITERATIONS {
        for runtime in &mut shell.components {
            if runtime
                .component
                .observes_service_event(std::hint::black_box(&event))
            {
                let _ = runtime.component.handle_service_event(&event).unwrap();
                old_hits += 1;
            }
        }
    }
    let old_elapsed = old_started.elapsed();

    shell.service_delivery_index.mark_dirty();
    shell.rebuild_service_delivery_index_if_needed();
    let new_started = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = shell
            .deliver_service_event(std::hint::black_box(&event))
            .unwrap();
    }
    let new_elapsed = new_started.elapsed();
    let speedup = old_elapsed.as_secs_f64() / new_elapsed.as_secs_f64();
    let delivered_total: usize = states
        .iter()
        .map(|state| state.lock().unwrap().handled.len())
        .sum();

    eprintln!(
        "service delivery scan: old={old_elapsed:?} indexed={new_elapsed:?} speedup={speedup:.3}x old_hits={old_hits} delivered_total={delivered_total}"
    );
    assert_eq!(old_hits, ITERATIONS);
    assert_eq!(delivered_total, ITERATIONS * 2);
    assert!(
        speedup >= 5.0,
        "indexed delivery should be at least 5x faster than a full component scan, measured {speedup:.3}x"
    );
}
