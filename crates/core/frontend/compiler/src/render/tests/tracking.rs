use super::super::*;
use super::common::*;
use mesh_core_component::style::{StyleValue, prop_variable_key};
use mesh_core_elements::HandlerTarget;

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
{
    width = { type = "size", default = "10px" },
}
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
