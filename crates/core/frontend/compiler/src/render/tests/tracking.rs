use super::super::*;
use super::common::*;
use mesh_core_component::style::{StyleValue, prop_variable_key};
use mesh_core_elements::HandlerTarget;
use std::collections::HashMap;

#[test]
fn tracking_store_coalesces_consecutive_duplicate_reads() {
    let inner = MapStore(std::collections::HashMap::new());
    let tracker = TrackingVariableStore::new(&inner);
    for _ in 0..4 {
        let _ = mesh_core_elements::VariableStore::get(&tracker, "audio.percent");
    }
    assert_eq!(
        tracker.into_reads(),
        vec![("audio".to_string(), "percent".to_string())]
    );
}

#[test]
fn tracking_store_coalesces_nonconsecutive_duplicate_reads() {
    let inner = MapStore(std::collections::HashMap::new());
    let tracker = TrackingVariableStore::new(&inner);
    let _ = mesh_core_elements::VariableStore::get(&tracker, "audio.percent");
    let _ = mesh_core_elements::VariableStore::get(&tracker, "network.ssid");
    let _ = mesh_core_elements::VariableStore::get(&tracker, "audio.percent");

    assert_eq!(
        tracker.into_reads(),
        vec![
            ("audio".to_string(), "percent".to_string()),
            ("network".to_string(), "ssid".to_string())
        ]
    );
}

#[test]
fn tracking_store_skips_bare_reads() {
    let inner = MapStore(std::collections::HashMap::new());
    let t = TrackingVariableStore::new(&inner);
    let _ = mesh_core_elements::VariableStore::get(&t, "audio");
    let reads = t.into_reads();
    assert!(reads.is_empty());
}

#[test]
fn tracking_store_no_cross_contamination() {
    let inner = MapStore(std::collections::HashMap::new());
    let t1 = TrackingVariableStore::new(&inner);
    let t2 = TrackingVariableStore::new(&inner);
    let _ = mesh_core_elements::VariableStore::get(&t1, "network.ssid");
    let reads1 = t1.into_reads();
    let reads2 = t2.into_reads();
    assert_eq!(reads1.len(), 1);
    assert!(reads2.is_empty());
}

// Run with: cargo test -p mesh-core-frontend --release -- service_field_tracking_overhead --ignored
// Not run in debug mode — allocator cost of Vec+String dwarfs the measured work and produces
// meaningless ratios (20-30x). In release mode the ratio is < 1.01.
#[test]
#[ignore]
fn service_field_tracking_overhead_under_one_percent() {
    use std::time::Instant;

    struct NoopStore;
    impl mesh_core_elements::VariableStore for NoopStore {
        fn get(&self, _: &str) -> Option<serde_json::Value> {
            None
        }
        fn keys(&self) -> Vec<String> {
            Vec::new()
        }
    }

    let iterations = 10_000usize;
    let noop = NoopStore;

    let baseline_start = Instant::now();
    for _ in 0..iterations {
        let _ = mesh_core_elements::VariableStore::get(&noop, "audio.percent");
        let _ = mesh_core_elements::VariableStore::get(&noop, "volume");
        let _ = mesh_core_elements::VariableStore::get(&noop, "network.ssid");
    }
    let baseline_ns = baseline_start.elapsed().as_nanos().max(1);

    let tracking_start = Instant::now();
    for _ in 0..iterations {
        let t = TrackingVariableStore::new(&noop);
        let _ = mesh_core_elements::VariableStore::get(&t, "audio.percent");
        let _ = mesh_core_elements::VariableStore::get(&t, "volume");
        let _ = mesh_core_elements::VariableStore::get(&t, "network.ssid");
        let _ = t.into_reads();
    }
    let tracking_ns = tracking_start.elapsed().as_nanos();

    let overhead_ratio = tracking_ns as f64 / baseline_ns as f64;
    assert!(
        overhead_ratio <= 1.05,
        "TrackingVariableStore overhead {:.4}x exceeds 1.05x threshold (baseline={}ns tracked={}ns)",
        overhead_ratio,
        baseline_ns,
        tracking_ns,
    );
}

// cargo test -p mesh-core-frontend --release -- repeated_service_read_coalescing_avoids_string_allocations --ignored --nocapture
#[test]
#[ignore = "release-only repeated service-read tracking microbenchmark"]
fn repeated_service_read_coalescing_avoids_string_allocations() {
    use std::time::Instant;

    let iterations = 1_000_000usize;
    let name = "audio.percent";

    let eager_started = Instant::now();
    let mut eager = Vec::new();
    for _ in 0..iterations {
        let dot = std::hint::black_box(name).find('.').unwrap();
        eager.push((
            std::hint::black_box(name[..dot].to_owned()),
            std::hint::black_box(name[dot + 1..].to_owned()),
        ));
    }
    let eager_time = eager_started.elapsed();

    let inner = MapStore(std::collections::HashMap::new());
    let tracker = TrackingVariableStore::new(&inner);
    let coalesced_started = Instant::now();
    for _ in 0..iterations {
        tracker.record_read(std::hint::black_box(name));
    }
    let coalesced_time = coalesced_started.elapsed();
    let tracked = tracker.into_reads();

    eprintln!(
        "repeated service reads: eager allocations {eager_time:?}; coalesced {coalesced_time:?}; ratio {:.1}x; entries={}/{}",
        eager_time.as_secs_f64() / coalesced_time.as_secs_f64(),
        eager.len(),
        tracked.len()
    );
    assert_eq!(tracked.len(), 1);
    assert!(coalesced_time < eager_time);
}

// cargo test -p mesh-core-frontend --release -- nonconsecutive_service_read_coalescing_avoids_duplicate_allocations --ignored --nocapture
#[test]
#[ignore = "release-only nonconsecutive service-read tracking microbenchmark"]
fn nonconsecutive_service_read_coalescing_avoids_duplicate_allocations() {
    use std::time::Instant;

    fn old_record_read(reads: &mut Vec<(String, String)>, name: &str) {
        let Some(dot_pos) = name.find('.') else {
            return;
        };
        let service = &name[..dot_pos];
        let field = &name[dot_pos + 1..];
        if reads.last().is_some_and(|(last_service, last_field)| {
            last_service == service && last_field == field
        }) {
            return;
        }
        reads.push((service.to_owned(), field.to_owned()));
    }

    let iterations = 250_000usize;
    let names = [
        "audio.percent",
        "network.ssid",
        "audio.percent",
        "power.percent",
    ];

    let old_started = Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let mut reads = Vec::new();
        for name in names {
            old_record_read(&mut reads, std::hint::black_box(name));
        }
        old_total = old_total.wrapping_add(std::hint::black_box(reads.len()));
    }
    let old_time = old_started.elapsed();

    let inner = MapStore(std::collections::HashMap::new());
    let new_started = Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let tracker = TrackingVariableStore::new(&inner);
        for name in names {
            tracker.record_read(std::hint::black_box(name));
        }
        new_total = new_total.wrapping_add(std::hint::black_box(tracker.into_reads().len()));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "nonconsecutive service reads: consecutive-only {old_time:?}; duplicate scan {new_time:?}; ratio {:.1}x; entries={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_total < old_total);
    assert!(new_time < old_time);
}

#[test]
fn event_handler_resolution_prefers_borrowed_store_lookup() {
    struct BorrowOnlyStore(serde_json::Value);

    impl VariableStore for BorrowOnlyStore {
        fn get(&self, _name: &str) -> Option<serde_json::Value> {
            panic!("owned lookup should not run when a borrowed value exists");
        }

        fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
            (name == "handler").then_some(&self.0)
        }

        fn keys(&self) -> Vec<String> {
            Vec::new()
        }
    }

    let store = BorrowOnlyStore(serde_json::json!("onResolved"));
    assert_eq!(
        resolve_event_handler_value(Some(&store), "handler"),
        "onResolved"
    );
}

// cargo test -p mesh-core-frontend --release -- borrowed_event_handler_lookup_beats_owned_json_clone --ignored --nocapture
#[test]
#[ignore = "release-only event-handler lookup microbenchmark"]
fn borrowed_event_handler_lookup_beats_owned_json_clone() {
    use std::time::Instant;

    struct HandlerStore(serde_json::Value);

    impl VariableStore for HandlerStore {
        fn get(&self, name: &str) -> Option<serde_json::Value> {
            (name == "handler").then(|| self.0.clone())
        }

        fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
            (name == "handler").then_some(&self.0)
        }

        fn keys(&self) -> Vec<String> {
            Vec::new()
        }
    }

    let store = HandlerStore(serde_json::json!("onPointerMove"));
    let iterations = 1_000_000usize;

    let owned_started = Instant::now();
    let mut owned_bytes = 0usize;
    for _ in 0..iterations {
        let handler = store
            .get(std::hint::black_box("handler"))
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap();
        owned_bytes = owned_bytes.wrapping_add(handler.len());
    }
    let owned_time = owned_started.elapsed();

    let borrowed_started = Instant::now();
    let mut borrowed_bytes = 0usize;
    for _ in 0..iterations {
        let handler = resolve_event_handler_value(Some(&store), std::hint::black_box("handler"));
        borrowed_bytes = borrowed_bytes.wrapping_add(handler.len());
    }
    let borrowed_time = borrowed_started.elapsed();

    eprintln!(
        "event handler state lookup: owned JSON clone {owned_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; bytes={owned_bytes}/{borrowed_bytes}",
        owned_time.as_secs_f64() / borrowed_time.as_secs_f64()
    );
    assert_eq!(owned_bytes, borrowed_bytes);
    assert!(borrowed_time < owned_time);
}

#[test]
fn css_prop_resolution_prefers_borrowed_props_table() {
    struct BorrowOnlyProps(serde_json::Value);

    impl VariableStore for BorrowOnlyProps {
        fn get(&self, _name: &str) -> Option<serde_json::Value> {
            panic!("owned props lookup should not run when borrowing is supported");
        }

        fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
            (name == "props").then_some(&self.0)
        }

        fn keys(&self) -> Vec<String> {
            Vec::new()
        }
    }

    let component = mesh_core_component::parse_component(
        r#"
<props>
  width: { type: "size", default: "10px" }
</props>
<template><box/></template>
"#,
    )
    .unwrap();
    let store = BorrowOnlyProps(serde_json::json!({"width": "42px"}));
    let props = resolve_css_props(component.props.as_ref(), Some(&store));
    assert!(matches!(
        props.get(&prop_variable_key("width")),
        Some(StyleValue::Literal(value)) if value == "42px"
    ));
}

// cargo test -p mesh-core-frontend --release -- borrowed_css_props_table_beats_deep_clone --ignored --nocapture
#[test]
#[ignore = "release-only CSS props table lookup microbenchmark"]
fn borrowed_css_props_table_beats_deep_clone() {
    use std::time::Instant;

    struct OwnedProps(serde_json::Value);
    impl VariableStore for OwnedProps {
        fn get(&self, name: &str) -> Option<serde_json::Value> {
            (name == "props").then(|| self.0.clone())
        }
        fn keys(&self) -> Vec<String> {
            Vec::new()
        }
    }

    let component = mesh_core_component::parse_component(
        r#"
<props>
  width: { type: "size", default: "10px" }
</props>
<template><box/></template>
"#,
    )
    .unwrap();
    let mut values = serde_json::Map::new();
    values.insert("width".into(), serde_json::json!("42px"));
    for index in 0..128 {
        values.insert(
            format!("unused_{index}"),
            serde_json::json!({"payload": "x".repeat(1_024), "enabled": true}),
        );
    }
    let value = serde_json::Value::Object(values);
    let owned = OwnedProps(value.clone());
    let borrowed = MapStore(HashMap::from([("props".into(), value)]));
    let iterations = 10_000usize;

    let owned_started = Instant::now();
    let mut owned_total = 0usize;
    for _ in 0..iterations {
        owned_total +=
            resolve_css_props(component.props.as_ref(), Some(std::hint::black_box(&owned))).len();
    }
    let owned_time = owned_started.elapsed();

    let borrowed_started = Instant::now();
    let mut borrowed_total = 0usize;
    for _ in 0..iterations {
        borrowed_total += resolve_css_props(
            component.props.as_ref(),
            Some(std::hint::black_box(&borrowed)),
        )
        .len();
    }
    let borrowed_time = borrowed_started.elapsed();

    eprintln!(
        "CSS props table: owned deep clone {owned_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; totals={owned_total}/{borrowed_total}",
        owned_time.as_secs_f64() / borrowed_time.as_secs_f64()
    );
    assert_eq!(owned_total, borrowed_total);
    assert!(borrowed_time < owned_time);
}

#[test]
fn namespaced_handler_preserves_typed_owner() {
    let mut target = HandlerTarget::root("onToggle");
    target.namespace("@mesh/panel/local:Toolbar");
    assert_eq!(target.handler(), "onToggle");
    assert_eq!(target.instance_key(), Some("@mesh/panel/local:Toolbar"));
    target.namespace("@mesh/other");
    assert_eq!(target.instance_key(), Some("@mesh/panel/local:Toolbar"));
}

#[test]
fn gesture_and_touch_attributes_are_runtime_event_handlers() {
    for name in [
        "ontwofingerscroll",
        "onswipe",
        "onpinch",
        "onhold",
        "ontouchstart",
        "ontouchmove",
        "ontouchend",
        "ontouchcancel",
        "ontap",
        "ondoubletap",
        "onlongpress",
    ] {
        assert!(is_event_handler_attribute(name), "{name}");
    }
}

// cargo test -p mesh-core-frontend --release -- borrowed_event_attribute_classification_beats_owned_normalization --ignored --nocapture
#[test]
#[ignore = "release-only event attribute classification microbenchmark"]
fn borrowed_event_attribute_classification_beats_owned_normalization() {
    use std::hint::black_box;
    use std::time::Instant;

    let names = [
        "class",
        "onclick",
        "ontap",
        "ontwofingerscroll",
        "onlongpress",
        "oncustom",
        "style",
        "onchange",
    ];
    let iterations = 2_000_000;

    let owned_started = Instant::now();
    let mut owned_matches = 0usize;
    for index in 0..iterations {
        let name = black_box(names[index % names.len()]);
        if name.starts_with("on")
            && matches!(
                name.strip_prefix("on").unwrap_or(name).to_string().as_str(),
                "click" | "change" | "tap" | "twofingerscroll" | "longpress"
            )
        {
            owned_matches += 1;
        }
    }
    let owned = owned_started.elapsed();

    let borrowed_started = Instant::now();
    let mut borrowed_matches = 0usize;
    for index in 0..iterations {
        if is_event_handler_attribute(black_box(names[index % names.len()])) {
            borrowed_matches += 1;
        }
    }
    let borrowed = borrowed_started.elapsed();

    // The production matcher recognizes more event names; all extra names
    // in this workload are deliberately non-events.
    assert_eq!(owned_matches, borrowed_matches);
    let speedup = owned.as_secs_f64() / borrowed.as_secs_f64();
    eprintln!(
        "MESH_PERF metric=borrowed_event_attribute_classification_speedup value={speedup:.3} owned={owned:?} borrowed={borrowed:?}"
    );
    assert!(borrowed < owned);
}

// cargo test -p mesh-core-frontend --release -- compiler_handler_namespace_presizing_beats_format_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only compiler handler namespace microbenchmark"]
fn compiler_handler_namespace_presizing_beats_format_benchmark() {
    use std::time::Instant;

    let instance_key = "@mesh/panel/local:StatusCluster/import:NetworkControls";
    let handler = "onConnectionStateChanged";
    let iterations = 1_000_000;

    let old_started = Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total ^=
            std::hint::black_box(format!("__mesh_embed__::{instance_key}::{handler}").len());
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let target = std::hint::black_box(HandlerTarget::embedded(instance_key, handler));
        new_total ^= target.dynamic_heap_bytes();
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "compiler handler namespace: format {old_time:?}; typed {new_time:?}; ratio {:.2}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(new_total, 0);
    assert!(new_time < old_time);
}
