use super::super::*;
use super::common::*;
use mesh_core_elements::accessibility::AccessibilityInfo;
use mesh_core_elements::{AttrKey, AttributeMap};

fn accessibility_info_eq(a: &AccessibilityInfo, b: &AccessibilityInfo) -> bool {
    a.role == b.role
        && a.label == b.label
        && a.description == b.description
        && a.focusable == b.focusable
        && a.focused == b.focused
        && a.keyboard_shortcut == b.keyboard_shortcut
        && a.state.disabled == b.state.disabled
        && a.state.checked == b.state.checked
        && a.state.expanded == b.state.expanded
        && a.state.selected == b.state.selected
        && a.state.pressed == b.state.pressed
        && a.state.busy == b.state.busy
        && a.state.invalid == b.state.invalid
        && a.state.required == b.state.required
        && a.state.value == b.state.value
        && a.state.value_min == b.state.value_min
        && a.state.value_max == b.state.value_max
}

#[test]
fn accessibility_for_element_empty_attributes_matches_unguarded_chain() {
    let empty = AttributeMap::new();
    for tag in ["box", "row", "column", "button", "text", "custom-widget"] {
        assert!(accessibility_info_eq(
            &accessibility_for_element(tag, tag, &empty),
            &accessibility_for_element_unguarded(tag, tag, &empty)
        ));
    }
}

#[test]
fn accessibility_for_element_non_empty_attributes_matches_unguarded_chain() {
    let mut attributes = AttributeMap::new();
    attributes.insert("class".into(), "active".to_string());
    attributes.insert("disabled".into(), "true".to_string());
    attributes.insert("min".into(), "1".to_string());
    assert!(accessibility_info_eq(
        &accessibility_for_element("input", "input", &attributes),
        &accessibility_for_element_unguarded("input", "input", &attributes)
    ));
}

#[test]
fn accessibility_single_pass_matches_lookup_chain_including_precedence() {
    // Every attribute the chain reads, in every alternative spelling, plus
    // unrelated attributes that must be ignored.
    let all: Vec<(&str, &str)> = vec![
        ("aria-label", "aria"),
        ("label", "label"),
        ("alt", "alt"),
        ("title", "title"),
        ("tooltip", "tooltip"),
        ("key", "ctrl+k"),
        ("keybind", "ctrl+b"),
        ("shortcut", "ctrl+s"),
        ("expanded", "true"),
        ("open", "false"),
        ("disabled", "1"),
        ("checked", "false"),
        ("selected", ""),
        ("pressed", "true"),
        ("busy", "nope"),
        ("invalid", "true"),
        ("required", "1"),
        ("value", " 42 "),
        ("min", " 1.5 "),
        ("max", "not-a-number"),
        ("class", "primary"),
        ("data-mesh-element", "button"),
    ];

    // Full set, then every single-attribute map, then each attribute with
    // its higher-precedence sibling removed.
    let mut cases: Vec<AttributeMap> = vec![
        all.iter()
            .map(|(k, v)| (AttrKey::new(k), v.to_string()))
            .collect(),
    ];
    for (name, value) in &all {
        cases.push(AttributeMap::from([(
            AttrKey::new(name),
            value.to_string(),
        )]));
    }
    for skip in ["aria-label", "label", "title", "key", "keybind", "expanded"] {
        cases.push(
            all.iter()
                .filter(|(name, _)| *name != skip)
                .map(|(k, v)| (AttrKey::new(k), v.to_string()))
                .collect(),
        );
    }

    for attributes in cases {
        for tag in ["input", "button", "box", "custom-widget"] {
            let single_pass = accessibility_for_element(tag, tag, &attributes);
            let chain = accessibility_for_element_unguarded(tag, tag, &attributes);
            assert!(
                accessibility_info_eq(&single_pass, &chain),
                "mismatch for <{tag}> with {attributes:?}"
            );
            assert_eq!(single_pass.label, chain.label);
            assert_eq!(single_pass.description, chain.description);
            assert_eq!(single_pass.keyboard_shortcut, chain.keyboard_shortcut);
        }
    }
}

// cargo test -p mesh-core-frontend --release -- accessibility_attribute_pass_beats_lookup_chain --ignored --nocapture
#[test]
#[ignore = "release-only accessibility attribute-pass microbenchmark"]
fn accessibility_attribute_pass_beats_lookup_chain() {
    use std::time::Instant;

    // A representative populated element: a few real attributes, none of
    // which most of the accessibility chain is looking for.
    let attributes: AttributeMap = [
        ("class", "entry-action primary"),
        ("data-mesh-element", "button"),
        ("style", "padding: 4px"),
        ("aria-label", "Open entry"),
    ]
    .into_iter()
    .map(|(name, value)| (AttrKey::new(name), value.to_string()))
    .collect();
    let iterations = 2_000_000usize;

    let chain_started = Instant::now();
    let mut chain_checksum = 0usize;
    for _ in 0..iterations {
        let info = accessibility_for_element_unguarded(
            std::hint::black_box("button"),
            std::hint::black_box("button"),
            std::hint::black_box(&attributes),
        );
        chain_checksum ^= info.label.map(|label| label.len()).unwrap_or(0);
    }
    let chain_time = chain_started.elapsed();

    let pass_started = Instant::now();
    let mut pass_checksum = 0usize;
    for _ in 0..iterations {
        let info = accessibility_for_element(
            std::hint::black_box("button"),
            std::hint::black_box("button"),
            std::hint::black_box(&attributes),
        );
        pass_checksum ^= info.label.map(|label| label.len()).unwrap_or(0);
    }
    let pass_time = pass_started.elapsed();

    eprintln!(
        "accessibility ({} attributes) over {iterations} elements: lookup chain {chain_time:?}, single pass {pass_time:?}, ratio {:.2}x",
        attributes.len(),
        chain_time.as_secs_f64() / pass_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=accessibility_attribute_pass_speedup value={:.6}",
        chain_time.as_secs_f64() / pass_time.as_secs_f64()
    );
    assert_eq!(chain_checksum, pass_checksum);
    assert!(pass_time < chain_time);
}

// cargo test -p mesh-core-frontend --release -- typed_attribute_storage_beats_stringify_then_parse --ignored --nocapture
#[test]
#[ignore = "release-only typed attribute storage microbenchmark"]
fn typed_attribute_storage_beats_stringify_then_parse() {
    use std::time::Instant;

    const ITERATIONS: usize = 500_000;
    let values = [
        ("checked", serde_json::json!(false)),
        ("disabled", serde_json::json!(true)),
        ("expanded", serde_json::json!(true)),
        ("max", serde_json::json!(100.5)),
        ("min", serde_json::json!(1.5)),
        ("selected", serde_json::json!(false)),
    ];
    let legacy_map = || {
        let mut attributes = AttributeMap::with_capacity(values.len());
        for (name, value) in &values {
            attributes.insert(AttrKey::new(name), template_value_to_string(value.clone()));
        }
        attributes
    };
    let typed_map = || {
        let mut attributes = AttributeMap::with_capacity(values.len());
        for (name, value) in &values {
            attributes.insert_value(AttrKey::new(name), value.clone());
        }
        attributes
    };

    let legacy = legacy_map();
    let typed = typed_map();
    assert!(accessibility_info_eq(
        &accessibility_for_element("input", "input", &legacy),
        &accessibility_for_element("input", "input", &typed)
    ));

    let legacy_started = Instant::now();
    let mut legacy_checksum = 0f32;
    for _ in 0..ITERATIONS {
        let attributes = legacy_map();
        let info = accessibility_for_element(
            std::hint::black_box("input"),
            std::hint::black_box("input"),
            std::hint::black_box(&attributes),
        );
        legacy_checksum += std::hint::black_box(
            info.state.disabled as u8 as f32
                + info.state.expanded.unwrap_or(false) as u8 as f32
                + info.state.value_min.unwrap_or_default()
                + info.state.value_max.unwrap_or_default(),
        );
    }
    let legacy_time = legacy_started.elapsed();

    let typed_started = Instant::now();
    let mut typed_checksum = 0f32;
    for _ in 0..ITERATIONS {
        let attributes = typed_map();
        let info = accessibility_for_element(
            std::hint::black_box("input"),
            std::hint::black_box("input"),
            std::hint::black_box(&attributes),
        );
        typed_checksum += std::hint::black_box(
            info.state.disabled as u8 as f32
                + info.state.expanded.unwrap_or(false) as u8 as f32
                + info.state.value_min.unwrap_or_default()
                + info.state.value_max.unwrap_or_default(),
        );
    }
    let typed_time = typed_started.elapsed();

    assert_eq!(legacy_checksum, typed_checksum);
    eprintln!(
        "six bound attributes across {ITERATIONS} nodes: stringify + parse {legacy_time:?}, typed storage {typed_time:?}, ratio {:.2}x",
        legacy_time.as_secs_f64() / typed_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=typed_attribute_storage_speedup value={:.6}",
        legacy_time.as_secs_f64() / typed_time.as_secs_f64()
    );
    assert!(typed_time < legacy_time);
}

// cargo test -p mesh-core-frontend --release -- accessibility_empty_attribute_guard_beats_full_lookup_chain --ignored --nocapture
#[test]
#[ignore = "release-only accessibility attribute-guard microbenchmark"]
fn accessibility_empty_attribute_guard_beats_full_lookup_chain() {
    use std::time::Instant;

    // Synthetic {#if}/{#for} column wrappers and many plain structural
    // elements carry no attributes; this is the realistic empty case.
    let empty = AttributeMap::new();
    let iterations = 2_000_000usize;

    let unguarded_started = Instant::now();
    let mut unguarded_checksum = 0usize;
    for _ in 0..iterations {
        let info = accessibility_for_element_unguarded(
            std::hint::black_box("column"),
            std::hint::black_box("column"),
            std::hint::black_box(&empty),
        );
        unguarded_checksum ^= info.focusable as usize;
    }
    let unguarded_time = unguarded_started.elapsed();

    let guarded_started = Instant::now();
    let mut guarded_checksum = 0usize;
    for _ in 0..iterations {
        let info = accessibility_for_element(
            std::hint::black_box("column"),
            std::hint::black_box("column"),
            std::hint::black_box(&empty),
        );
        guarded_checksum ^= info.focusable as usize;
    }
    let guarded_time = guarded_started.elapsed();

    eprintln!(
        "accessibility (empty attrs) over {iterations} nodes: unguarded {unguarded_time:?}, guarded {guarded_time:?}, ratio {:.2}x",
        unguarded_time.as_secs_f64() / guarded_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=accessibility_empty_attribute_guard_speedup value={:.6}",
        unguarded_time.as_secs_f64() / guarded_time.as_secs_f64()
    );
    assert_eq!(unguarded_checksum, guarded_checksum);
    assert!(guarded_time < unguarded_time);
}

// cargo test -p mesh-core-frontend --release -- shared_module_id_beats_per_node_string --ignored --nocapture
#[test]
#[ignore = "release-only shared module-identity microbenchmark"]
fn shared_module_id_beats_per_node_string() {
    use std::time::Instant;

    // Every node in a built tree carries its module's identity.
    let module_id = "@mesh/navigation-bar";
    let nodes = 2_000usize;
    let passes = 500usize;

    // Only the identity assignment is timed: constructing the ~900-byte
    // node around it is unchanged by this work and would swamp the signal.
    let mut node = WidgetNode::new("row");

    let owned_started = Instant::now();
    let mut owned_total = 0usize;
    for _ in 0..passes {
        for _ in 0..nodes {
            node.set_module_id(std::hint::black_box(module_id).to_string());
            owned_total = owned_total.wrapping_add(node.module_id().unwrap().len());
        }
    }
    let owned_time = owned_started.elapsed();

    // Warm the shared cache so the benchmark measures steady-state lookups.
    let _ = shared_module_id(module_id);

    let shared_started = Instant::now();
    let mut shared_total = 0usize;
    for _ in 0..passes {
        for _ in 0..nodes {
            attach_module_id(&mut node, std::hint::black_box(module_id));
            shared_total = shared_total.wrapping_add(node.module_id().unwrap().len());
        }
    }
    let shared_time = shared_started.elapsed();

    eprintln!(
        "module identity over {} nodes: per-node string {owned_time:?}, shared Arc {shared_time:?}, ratio {:.2}x",
        nodes * passes,
        owned_time.as_secs_f64() / shared_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=shared_module_id_speedup value={:.6}",
        owned_time.as_secs_f64() / shared_time.as_secs_f64()
    );
    assert_eq!(owned_total, shared_total);
    assert!(shared_time < owned_time);
}

#[test]
fn shared_module_ids_reuse_one_allocation_per_module() {
    let mut first = WidgetNode::new("row");
    let mut second = WidgetNode::new("column");
    attach_module_id(&mut first, "@mesh/panel");
    attach_module_id(&mut second, "@mesh/panel");
    assert_eq!(first.module_id(), Some("@mesh/panel"));
    assert!(std::sync::Arc::ptr_eq(
        first.shared_module_id().unwrap(),
        second.shared_module_id().unwrap()
    ));

    let mut other = WidgetNode::new("box");
    attach_module_id(&mut other, "@mesh/launcher");
    assert_eq!(other.module_id(), Some("@mesh/launcher"));
    assert!(!std::sync::Arc::ptr_eq(
        first.shared_module_id().unwrap(),
        other.shared_module_id().unwrap()
    ));

    // The bounded cache must keep returning correct ids after eviction.
    for index in 0..(SHARED_MODULE_ID_CACHE_LIMIT * 2) {
        let id = format!("@mesh/module-{index}");
        let mut node = WidgetNode::new("row");
        attach_module_id(&mut node, &id);
        assert_eq!(node.module_id(), Some(id.as_str()));
    }
    let mut again = WidgetNode::new("row");
    attach_module_id(&mut again, "@mesh/panel");
    assert_eq!(again.module_id(), Some("@mesh/panel"));
}

// cargo test -p mesh-core-frontend --release -- for_children_capacity_beats_growing --ignored --nocapture
#[test]
#[ignore = "release-only {#for} child-vector capacity microbenchmark"]
fn for_children_capacity_beats_growing() {
    use std::time::Instant;

    // A `{#for}` over N items with M children per iteration pushes N*M
    // large `WidgetNode` values; growing re-copies everything pushed so far.
    let items = 200usize;
    let children_per_item = 3usize;
    let passes = 2_000usize;
    let total = items * children_per_item;

    let growing_started = Instant::now();
    let mut growing_total = 0usize;
    for _ in 0..passes {
        let mut children: Vec<WidgetNode> = Vec::new();
        for _ in 0..total {
            children.push(WidgetNode::new(std::hint::black_box("row")));
        }
        growing_total = growing_total.wrapping_add(children.len());
    }
    let growing_time = growing_started.elapsed();

    let reserved_started = Instant::now();
    let mut reserved_total = 0usize;
    for _ in 0..passes {
        let mut children: Vec<WidgetNode> = Vec::with_capacity(total);
        for _ in 0..total {
            children.push(WidgetNode::new(std::hint::black_box("row")));
        }
        reserved_total = reserved_total.wrapping_add(children.len());
    }
    let reserved_time = reserved_started.elapsed();

    eprintln!(
        "{{#for}} children ({total} nodes of {} bytes, {passes} passes): growing {growing_time:?}, reserved {reserved_time:?}, ratio {:.2}x",
        std::mem::size_of::<WidgetNode>(),
        growing_time.as_secs_f64() / reserved_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=for_children_capacity_speedup value={:.6}",
        growing_time.as_secs_f64() / reserved_time.as_secs_f64()
    );
    assert_eq!(growing_total, reserved_total);
    assert!(reserved_time < growing_time);
}

/// A component shaped like a real shell surface: a styled root, nested
/// rows/columns with classes, text and expression nodes, a conditional,
/// and a loop over a list of items.
fn end_to_end_bench_component() -> mesh_core_component::ComponentFile {
    mesh_core_component::parse_component(
        r#"
<template>
  <column class="root">
<row class="header">
  <text class="title">Panel</text>
  <text class="subtitle">{subtitle}</text>
</row>
{#if expanded}
  <column class="body">
    {#for item in items}
      <row class="entry">
        <icon class="entry-icon" name="star" />
        <column class="entry-text">
          <text class="entry-title">{item.title}</text>
          <text class="entry-detail">{item.detail}</text>
        </column>
        <button class="entry-action" onclick={onActivate}>
          <text>Open</text>
        </button>
      </row>
    {/for}
  </column>
{/if}
  </column>
</template>
<style>
  .root { color: #ffffff; font-family: "Inter"; padding: 8px; }
  .header { font-size: 18px; padding-bottom: 4px; }
  .title { font-weight: 700; }
  .subtitle { color: #b0b0b0; font-size: 12px; }
  .body { padding: 4px; }
  .entry { padding: 6px; border-radius: 8px; }
  .entry:hover { color: #ffffff; }
  .entry-icon { width: 16px; height: 16px; }
  .entry-text { padding-left: 8px; }
  .entry-title { font-weight: 600; font-size: 14px; }
  .entry-detail { color: #9a9a9a; font-size: 11px; }
  .entry-action { padding: 4px; }
  button { font-weight: 600; }
  text { line-height: 1.4; }
  row { padding-left: 2px; }
  column { padding-top: 2px; }
</style>
"#,
    )
    .unwrap()
}

// cargo test -p mesh-core-frontend --release -- widget_tree_build_end_to_end_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only end-to-end widget-tree build benchmark"]
fn widget_tree_build_end_to_end_benchmark() {
    use std::time::Instant;

    let component = end_to_end_bench_component();
    let manifest = test_manifest();
    let theme = mesh_core_theme::default_theme();
    let items: Vec<serde_json::Value> = (0..64)
        .map(|index| {
            serde_json::json!({
                "title": format!("Entry {index}"),
                "detail": format!("detail line {index}"),
            })
        })
        .collect();
    let store = MapStore(
        [
            ("subtitle".to_string(), serde_json::json!("all systems")),
            ("expanded".to_string(), serde_json::json!(true)),
            ("items".to_string(), serde_json::Value::Array(items)),
            ("onActivate".to_string(), serde_json::json!("onActivate")),
        ]
        .into_iter()
        .collect(),
    );

    fn count(node: &WidgetNode) -> usize {
        1 + node.children.iter().map(count).sum::<usize>()
    }
    let node_count = count(&build_widget_tree_from_component(
        &component,
        &manifest,
        &theme,
        480.0,
        640.0,
        None,
        "root",
        Some(&store),
        &[],
    ));

    let builds = 2_000usize;
    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..builds {
        let tree = build_widget_tree_from_component(
            std::hint::black_box(&component),
            &manifest,
            &theme,
            480.0,
            640.0,
            None,
            "root",
            Some(&store),
            &[],
        );
        checksum = checksum.wrapping_add(tree.children.len());
    }
    let elapsed = started.elapsed();

    eprintln!(
        "widget tree build: {builds} builds of a {node_count}-node tree in {elapsed:?} ({:.3}ms per build, checksum {checksum})",
        elapsed.as_secs_f64() * 1000.0 / builds as f64
    );
}
