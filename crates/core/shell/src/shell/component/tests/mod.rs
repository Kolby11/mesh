use super::*;

mod common;

mod integration;
mod interaction;
mod invalidation;
mod restyle;

#[test]
fn markup_expressions_run_as_full_luau_in_component_scope() {
    let mut component = common::test_frontend_component(
        r#"
<template>
  <column title={string.upper("scope")}>
    <text>{add(secret, 2)}</text>
    {#for item in values}
      <text>{add(item.value, secret)}</text>
    {/for}
  </column>
</template>
<script lang="luau">
local secret = 40
local values = {{ value = 1 }, { value = 2 }}
local function add(a, b)
  return a + b
end
</script>
"#,
    );
    let theme = mesh_core_theme::default_theme();
    let tree = component.build_tree(&theme, 200, 100);
    let column = &tree.children[0];
    assert_eq!(
        column.attributes.get("title").map(String::as_str),
        Some("SCOPE")
    );
    let text: Vec<_> = column
        .children
        .iter()
        .flat_map(|child| {
            if child.tag == "text" {
                vec![child]
            } else {
                child.children.iter().collect()
            }
        })
        .filter_map(|node| node.attributes.get("content").map(String::as_str))
        .collect();
    assert_eq!(text, vec!["42", "41", "42"]);
}

#[test]
fn generated_error_placeholder_is_bounded_after_restyle_constraints() {
    let message = "missing interface ".repeat(100);
    let mut node = runtime::bounded_error_widget(&message);

    // Simulate arbitrary host CSS winning during the normal restyle pass.
    node.computed_style.max_width = None;
    node.children[0].computed_style.max_width = None;
    rendering::constrain_error_placeholders(&mut node);

    for constrained in [&node, &node.children[0]] {
        assert_eq!(
            constrained.computed_style.max_width,
            Some(ERROR_PLACEHOLDER_MAX_WIDTH)
        );
        assert_eq!(constrained.computed_style.min_width, Some(0.0));
        assert_eq!(
            constrained.computed_style.overflow_x,
            mesh_core_elements::style::Overflow::Hidden
        );
        assert_eq!(
            constrained.computed_style.white_space,
            mesh_core_elements::style::WhiteSpace::Nowrap
        );
        assert_eq!(
            constrained.computed_style.text_overflow,
            mesh_core_elements::style::TextOverflow::Ellipsis
        );
    }
    assert_eq!(node.attributes.get("content"), Some(&message));
}

#[test]
fn element_metric_usage_splits_refs_from_elements() {
    let refs_only = common::test_frontend_component(
        r#"
<template>
  <button ref="action" />
</template>
<script lang="luau">
</script>
"#,
    );
    assert_eq!(
        refs_only.element_metric_usage,
        ElementMetricUsage {
            elements: false,
            refs: true
        }
    );

    let elements_binding = common::test_frontend_component(
        r#"
<template>
  <box />
</template>
<script lang="luau">
-- elements["root/0"].width
</script>
"#,
    );
    assert_eq!(
        elements_binding.element_metric_usage,
        ElementMetricUsage {
            elements: true,
            refs: false
        }
    );
}

#[test]
fn service_payload_cache_reuses_equal_payload_and_replaces_changed_payload() {
    let first = serde_json::json!({ "available": true, "percent": 42 });
    let changed = serde_json::json!({ "available": true, "percent": 73 });
    let first_fingerprint = ScriptContext::service_payload_fingerprint(&first);
    let changed_fingerprint = ScriptContext::service_payload_fingerprint(&changed);
    let mut cache = HashMap::new();

    assert!(
        update_cached_service_payload(&mut cache, "audio", &first, first_fingerprint).is_none()
    );
    let retained = Arc::clone(&cache["audio"].value);
    let previous =
        update_cached_service_payload(&mut cache, "audio", &first, first_fingerprint).unwrap();
    assert!(Arc::ptr_eq(&retained, &previous));
    assert!(Arc::ptr_eq(&retained, &cache["audio"].value));

    let previous =
        update_cached_service_payload(&mut cache, "audio", &changed, changed_fingerprint).unwrap();
    assert!(Arc::ptr_eq(&retained, &previous));
    assert!(!Arc::ptr_eq(&retained, &cache["audio"].value));
    assert_eq!(cache["audio"].value.as_ref(), &changed);
}

#[test]
fn service_capabilities_cache_reuses_the_global_entry() {
    let mut cache = HashMap::new();
    let first = cached_service_capabilities(&mut cache, "mesh.audio");
    let second = cached_service_capabilities(&mut cache, "mesh.audio");

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(cache.len(), 1);
    assert_eq!(second.service_name, "audio");
}

// cargo test -p mesh-core-shell --release -- local_service_capability_cache_beats_global_lookup --ignored --nocapture
#[test]
#[ignore = "release-only service capability lookup microbenchmark"]
fn local_service_capability_cache_beats_global_lookup() {
    let iterations = 1_000_000usize;
    let interface = "mesh.audio";
    let _ = service_capabilities(interface);

    let global_started = std::time::Instant::now();
    let mut global_total = 0usize;
    for _ in 0..iterations {
        let capabilities = service_capabilities(std::hint::black_box(interface));
        global_total += std::hint::black_box(capabilities.service_name.len());
    }
    let global_time = global_started.elapsed();

    let mut cache = HashMap::new();
    let _ = cached_service_capabilities(&mut cache, interface);
    let local_started = std::time::Instant::now();
    let mut local_total = 0usize;
    for _ in 0..iterations {
        let capabilities = cached_service_capabilities(&mut cache, std::hint::black_box(interface));
        local_total += std::hint::black_box(capabilities.service_name.len());
    }
    let local_time = local_started.elapsed();

    eprintln!(
        "service capability lookup over {iterations} events: global {global_time:?}; component-local {local_time:?}; ratio {:.2}x",
        global_time.as_secs_f64() / local_time.as_secs_f64()
    );
    assert_eq!(global_total, local_total);
    assert!(local_time < global_time);
}

#[test]
fn last_service_trace_is_retained_only_for_debug_logging() {
    let mut summary = None;
    update_last_service_trace(&mut summary, "mesh.audio", "@mesh/pipewire", true);
    assert_eq!(summary.as_deref(), Some("mesh.audio:@mesh/pipewire"));

    update_last_service_trace(&mut summary, "mesh.audio", "@mesh/pipewire", false);
    assert!(summary.is_none());
}

// cargo test -p mesh-core-shell --release -- disabled_service_trace_gate_beats_unconditional_formatting --ignored --nocapture
#[test]
#[ignore = "release-only service trace formatting microbenchmark"]
fn disabled_service_trace_gate_beats_unconditional_formatting() {
    assert!(
        !tracing::enabled!(tracing::Level::DEBUG),
        "benchmark requires debug tracing to be disabled"
    );
    let iterations = 1_000_000usize;
    let service = "mesh.audio";
    let source_module = "@mesh/pipewire-audio";

    let formatted_started = std::time::Instant::now();
    let mut formatted_total = 0usize;
    for _ in 0..iterations {
        let formatted = format!(
            "{}:{}",
            std::hint::black_box(service),
            std::hint::black_box(source_module)
        );
        formatted_total += std::hint::black_box(formatted.len());
    }
    let formatted_time = formatted_started.elapsed();

    let gated_started = std::time::Instant::now();
    let mut gated = None;
    let mut gated_total = 0usize;
    for _ in 0..iterations {
        update_last_service_trace(
            &mut gated,
            std::hint::black_box(service),
            std::hint::black_box(source_module),
            tracing::enabled!(tracing::Level::DEBUG),
        );
        gated_total += std::hint::black_box(gated.as_ref().map_or(0, String::len));
    }
    let gated_time = gated_started.elapsed();

    eprintln!(
        "disabled service trace over {iterations} updates: format {formatted_time:?}; gated {gated_time:?}; ratio {:.1}x",
        formatted_time.as_secs_f64() / gated_time.as_secs_f64()
    );
    assert!(formatted_total > gated_total);
    assert!(gated_time < formatted_time);
}

// cargo test -p mesh-core-shell --release -- unchanged_service_payload_cache_beats_replacement --ignored --nocapture
#[test]
#[ignore = "release-only service payload cache microbenchmark"]
fn unchanged_service_payload_cache_beats_replacement() {
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

    let mut replaced = HashMap::from([(
        "audio".to_owned(),
        CachedServicePayload {
            value: Arc::new(payload.clone()),
            fingerprint,
        },
    )]);
    let replaced_started = std::time::Instant::now();
    let mut replaced_total = 0usize;
    for _ in 0..iterations {
        let previous = replaced.get("audio").cloned();
        replaced.insert(
            "audio".to_owned(),
            CachedServicePayload {
                value: Arc::new(std::hint::black_box(&payload).clone()),
                fingerprint,
            },
        );
        replaced_total += std::hint::black_box(previous.is_some()) as usize;
    }
    let replaced_time = replaced_started.elapsed();

    let mut reused = HashMap::from([(
        "audio".to_owned(),
        CachedServicePayload {
            value: Arc::new(payload.clone()),
            fingerprint,
        },
    )]);
    let reused_started = std::time::Instant::now();
    let mut reused_total = 0usize;
    for _ in 0..iterations {
        let previous = update_cached_service_payload(
            &mut reused,
            "audio",
            std::hint::black_box(&payload),
            fingerprint,
        );
        reused_total += std::hint::black_box(previous.is_some()) as usize;
    }
    let reused_time = reused_started.elapsed();

    eprintln!(
        "unchanged service cache over {iterations} updates: replace {replaced_time:?}; reuse {reused_time:?}; ratio {:.2}x",
        replaced_time.as_secs_f64() / reused_time.as_secs_f64()
    );
    assert_eq!(replaced_total, reused_total);
    assert!(reused_time < replaced_time);
}

#[test]
fn ref_metrics_keep_scroll_offsets_from_unpublished_ancestors() {
    let mut root = WidgetNode::new("box");
    root.attributes.insert("_mesh_scroll_y".into(), "12".into());

    let mut child = WidgetNode::new("button");
    child.attributes.insert("_mesh_key".into(), "root/0".into());
    child.attributes.insert("ref".into(), "action".into());
    child.layout.y = 30.0;
    child.layout.height = 10.0;
    root.children.push(child);

    let mut elements = serde_json::Map::new();
    let mut refs = serde_json::Map::new();
    let mut ref_keys = HashMap::new();
    collect_element_metrics(
        &root,
        0.0,
        0.0,
        false,
        true,
        &mut elements,
        &mut refs,
        &mut ref_keys,
    );

    assert!(elements.is_empty());
    assert_eq!(ref_keys.get("action").map(String::as_str), Some("root/0"));
    let top = refs
        .get("action")
        .and_then(|metrics| metrics.get("top"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_default();
    assert_eq!(top, 18.0);
}

// cargo test -p mesh-core-shell --release -- ref_only_element_metrics_skip_elements_map --ignored --nocapture
#[test]
#[ignore = "release-only element metrics collection microbenchmark"]
fn ref_only_element_metrics_skip_elements_map() {
    fn make_tree(depth: usize, breadth: usize, index: &mut usize) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        let current = *index;
        *index += 1;
        node.attributes
            .insert("_mesh_key".into(), format!("root/{current}"));
        node.attributes
            .insert("id".into(), format!("node_{current}"));
        node.layout.width = 100.0 + current as f32;
        node.layout.height = 24.0;
        if depth > 0 {
            node.children = (0..breadth)
                .map(|_| make_tree(depth - 1, breadth, index))
                .collect();
        }
        node
    }

    let mut index = 0;
    let tree = make_tree(4, 4, &mut index);
    let iterations = 2_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let mut elements = serde_json::Map::new();
        let mut refs = serde_json::Map::new();
        let mut ref_keys = HashMap::new();
        collect_element_metrics(
            std::hint::black_box(&tree),
            0.0,
            0.0,
            true,
            true,
            &mut elements,
            &mut refs,
            &mut ref_keys,
        );
        old_total += std::hint::black_box(elements.len() + refs.len() + ref_keys.len());
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let mut elements = serde_json::Map::new();
        let mut refs = serde_json::Map::new();
        let mut ref_keys = HashMap::new();
        collect_element_metrics(
            std::hint::black_box(&tree),
            0.0,
            0.0,
            false,
            true,
            &mut elements,
            &mut refs,
            &mut ref_keys,
        );
        new_total += std::hint::black_box(elements.len() + refs.len() + ref_keys.len());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "ref-only element metrics: collect-both {old_time:?}; refs-only {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
    assert!(old_total > new_total);
}

// cargo test -p mesh-core-shell --release -- sparse_ref_metrics_skip_unpublished_snapshots --ignored --nocapture
#[test]
#[ignore = "release-only sparse element metrics collection microbenchmark"]
fn sparse_ref_metrics_skip_unpublished_snapshots() {
    fn make_tree(depth: usize, breadth: usize, index: &mut usize) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        let current = *index;
        *index += 1;
        node.attributes
            .insert("_mesh_key".into(), format!("root/{current}"));
        if current % 17 == 0 {
            node.attributes
                .insert("id".into(), format!("node_{current}"));
        }
        node.layout.width = 100.0 + current as f32;
        node.layout.height = 24.0;
        if current % 13 == 0 {
            node.attributes
                .insert("_mesh_scroll_y".into(), (current % 5).to_string());
        }
        if depth > 0 {
            node.children = (0..breadth)
                .map(|_| make_tree(depth - 1, breadth, index))
                .collect();
        }
        node
    }

    fn old_collect_refs(
        node: &WidgetNode,
        offset_x: f32,
        offset_y: f32,
        refs: &mut serde_json::Map<String, serde_json::Value>,
        ref_keys: &mut HashMap<String, String>,
    ) {
        let metrics = mesh_core_elements::element_snapshot_json(node, offset_x, offset_y);
        let scroll_x = metrics
            .get("scroll_x")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32;
        let scroll_y = metrics
            .get("scroll_y")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32;
        let node_key = node.mesh_key();
        if let Some(id) = node.attributes.get("id") {
            refs.insert(id.clone(), metrics.clone());
            if let Some(key) = node_key {
                ref_keys.insert(id.clone(), key.to_owned());
            }
        }
        if let Some(reference) = node.attributes.get("ref") {
            refs.insert(reference.clone(), metrics.clone());
            if let Some(key) = node_key {
                ref_keys.insert(reference.clone(), key.to_owned());
            }
        }
        if let Some(binding) = node.attributes.get("_mesh_bind_this") {
            refs.insert(binding.clone(), metrics);
            if let Some(key) = node_key {
                ref_keys.insert(binding.clone(), key.to_owned());
            }
        }

        let child_offset_x = offset_x - scroll_x;
        let child_offset_y = offset_y - scroll_y;
        for child in &node.children {
            old_collect_refs(child, child_offset_x, child_offset_y, refs, ref_keys);
        }
    }

    let mut index = 0;
    let tree = make_tree(4, 4, &mut index);
    let iterations = 2_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let mut refs = serde_json::Map::new();
        let mut ref_keys = HashMap::new();
        old_collect_refs(
            std::hint::black_box(&tree),
            0.0,
            0.0,
            &mut refs,
            &mut ref_keys,
        );
        old_total += std::hint::black_box(refs.len() + ref_keys.len());
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let mut elements = serde_json::Map::new();
        let mut refs = serde_json::Map::new();
        let mut ref_keys = HashMap::new();
        collect_element_metrics(
            std::hint::black_box(&tree),
            0.0,
            0.0,
            false,
            true,
            &mut elements,
            &mut refs,
            &mut ref_keys,
        );
        new_total += std::hint::black_box(refs.len() + ref_keys.len());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "sparse ref element metrics: eager {old_time:?}; lazy {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

#[test]
fn element_metrics_move_last_preserves_all_publications() {
    let mut node = WidgetNode::new("box");
    node.set_mesh_key("root/0");
    node.attributes.insert("id".into(), "by_id".into());
    node.attributes.insert("ref".into(), "by_ref".into());
    node.attributes
        .insert("_mesh_bind_this".into(), "by_binding".into());
    node.layout.width = 120.0;
    node.layout.height = 32.0;

    let mut elements = serde_json::Map::new();
    let mut refs = serde_json::Map::new();
    let mut ref_keys = HashMap::new();
    collect_element_metrics(
        &node,
        0.0,
        0.0,
        true,
        true,
        &mut elements,
        &mut refs,
        &mut ref_keys,
    );

    let expected = elements.get("root/0").expect("element publication");
    assert_eq!(refs.get("by_id"), Some(expected));
    assert_eq!(refs.get("by_ref"), Some(expected));
    assert_eq!(refs.get("by_binding"), Some(expected));
    assert_eq!(ref_keys.len(), 3);
}

// cargo test -p mesh-core-shell --release -- moving_final_element_metric_beats_clone_then_drop --ignored --nocapture
#[test]
#[ignore = "release-only element-metric ownership microbenchmark"]
fn moving_final_element_metric_beats_clone_then_drop() {
    fn old_collect(
        node: &WidgetNode,
        refs: &mut serde_json::Map<String, serde_json::Value>,
        ref_keys: &mut HashMap<String, String>,
    ) {
        if let Some(id) = node.attributes.get("id") {
            let metrics = mesh_core_elements::element_snapshot_json(node, 0.0, 0.0);
            refs.insert(id.clone(), metrics.clone());
            if let Some(key) = node.mesh_key() {
                ref_keys.insert(id.clone(), key.to_owned());
            }
        }
        for child in &node.children {
            old_collect(child, refs, ref_keys);
        }
    }

    let mut root = WidgetNode::new("row");
    root.children = (0..256)
        .map(|index| {
            let mut node = WidgetNode::new("box");
            node.set_mesh_key(format!("root/{index}"));
            node.attributes.insert("id".into(), format!("item_{index}"));
            node.layout.x = index as f32;
            node.layout.width = 24.0;
            node.layout.height = 24.0;
            node
        })
        .collect();
    let iterations = 2_000usize;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let mut refs = serde_json::Map::new();
        let mut ref_keys = HashMap::new();
        old_collect(std::hint::black_box(&root), &mut refs, &mut ref_keys);
        old_total += std::hint::black_box(refs.len() + ref_keys.len());
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let mut elements = serde_json::Map::new();
        let mut refs = serde_json::Map::new();
        let mut ref_keys = HashMap::new();
        collect_element_metrics(
            std::hint::black_box(&root),
            0.0,
            0.0,
            false,
            true,
            &mut elements,
            &mut refs,
            &mut ref_keys,
        );
        new_total += std::hint::black_box(refs.len() + ref_keys.len());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "single-name metrics over {iterations}x256 nodes: clone/drop {old_time:?}; move-final {new_time:?}; ratio {:.2}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- element_action_ref_keys_move_restore_beats_full_clone --ignored --nocapture
#[test]
#[ignore = "release-only element-action ref lookup microbenchmark"]
fn element_action_ref_keys_move_restore_beats_full_clone() {
    let entries = 512usize;
    let iterations = 100_000usize;
    let ref_keys: HashMap<String, String> = (0..entries)
        .map(|index| (format!("ref_{index}"), format!("root/{index}")))
        .collect();
    let lookup = "ref_511".to_string();

    let old_refs = std::cell::RefCell::new(ref_keys.clone());
    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let batch_ref_keys = old_refs.borrow().clone();
        old_total = old_total.wrapping_add(
            std::hint::black_box(batch_ref_keys.get(&lookup))
                .map(String::len)
                .unwrap_or_default(),
        );
    }
    let old_time = old_started.elapsed();

    let new_refs = std::cell::RefCell::new(ref_keys);
    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let batch_ref_keys = {
            let mut borrowed = new_refs.borrow_mut();
            std::mem::take(&mut *borrowed)
        };
        new_total = new_total.wrapping_add(
            std::hint::black_box(batch_ref_keys.get(&lookup))
                .map(String::len)
                .unwrap_or_default(),
        );
        *new_refs.borrow_mut() = batch_ref_keys;
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "element action ref keys: clone {old_time:?}; move/restore {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- refs_snapshot_move_beats_json_clone --ignored --nocapture
#[test]
#[ignore = "release-only refs snapshot ownership microbenchmark"]
fn refs_snapshot_move_beats_json_clone() {
    let entries = 256usize;
    let iterations = 20_000usize;
    let make_snapshot = || {
        let mut refs = serde_json::Map::with_capacity(entries);
        for index in 0..entries {
            refs.insert(
                format!("ref_{index}"),
                serde_json::json!({
                    "left": index,
                    "top": index + 1,
                    "width": 120,
                    "height": 24,
                }),
            );
        }
        serde_json::Value::Object(refs)
    };

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let value = make_snapshot();
        let state_value = value.clone();
        old_total += std::hint::black_box(state_value.as_object().unwrap().len());
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let value = make_snapshot();
        new_total += std::hint::black_box(value.as_object().unwrap().len());
        let _state_value = value;
    }
    let new_time = new_started.elapsed();

    assert_eq!(old_total, new_total);
    eprintln!(
        "refs snapshot ownership: clone {old_time:?}; move {new_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- element_ref_key_map_scratch_reuse_beats_fresh_allocations --ignored --nocapture
#[test]
#[ignore = "release-only element ref-key scratch microbenchmark"]
fn element_ref_key_map_scratch_reuse_beats_fresh_allocations() {
    let entries = 512usize;
    let iterations = 20_000usize;
    let keys: Vec<(String, String)> = (0..entries)
        .map(|index| (format!("ref_{index}"), format!("root/{index}")))
        .collect();

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let mut ref_keys = HashMap::new();
        for (name, node_key) in &keys {
            ref_keys.insert(name.clone(), node_key.clone());
        }
        old_total += std::hint::black_box(ref_keys.len());
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    let mut ref_keys = HashMap::new();
    for _ in 0..iterations {
        ref_keys.clear();
        for (name, node_key) in &keys {
            ref_keys.insert(name.clone(), node_key.clone());
        }
        new_total += std::hint::black_box(ref_keys.len());
    }
    let new_time = new_started.elapsed();

    assert_eq!(old_total, new_total);
    eprintln!(
        "element ref-key map: fresh {old_time:?}; scratch reuse {new_time:?}; ratio {:.2}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- runtime_prop_sync_single_lock_beats_contains_then_get_mut --ignored --nocapture
#[test]
#[ignore = "release-only runtime prop sync lock microbenchmark"]
fn runtime_prop_sync_single_lock_beats_contains_then_get_mut() {
    let iterations = 1_000_000usize;
    let key = "child".to_string();

    let old_runtimes = std::sync::Mutex::new(HashMap::from([(key.clone(), 0usize)]));
    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for value in 0..iterations {
        if old_runtimes.lock().unwrap().contains_key(&key)
            && let Some(slot) = old_runtimes.lock().unwrap().get_mut(&key)
        {
            *slot = value;
            old_total = old_total.wrapping_add(std::hint::black_box(*slot));
        }
    }
    let old_time = old_started.elapsed();

    let new_runtimes = std::sync::Mutex::new(HashMap::from([(key.clone(), 0usize)]));
    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for value in 0..iterations {
        if let Some(slot) = new_runtimes.lock().unwrap().get_mut(&key) {
            *slot = value;
            new_total = new_total.wrapping_add(std::hint::black_box(*slot));
        }
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "runtime prop sync lock pattern: contains+get_mut {old_time:?}; single get_mut {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}
