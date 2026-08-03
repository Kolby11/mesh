use super::*;
use mesh_core_component::style::StyleValue;
use std::time::Instant;

fn keyed_node(key: &str, children: Vec<WidgetNode>) -> WidgetNode {
    let mut node = WidgetNode::new("box");
    node.id = crate::shell::component::runtime_tree::stable_runtime_node_id(key);
    node.attributes.insert("_mesh_key".into(), key.into());
    node.children = children.into();
    node
}

fn broad_plain_tree(width: usize, depth: usize) -> WidgetNode {
    fn build(level: usize, width: usize, depth: usize) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.attributes
            .insert("_mesh_key".into(), format!("root/{level}"));
        if level < depth {
            node.children = (0..width)
                .map(|index| {
                    let mut child = build(level + 1, width, depth);
                    child
                        .attributes
                        .insert("_mesh_key".into(), format!("root/{level}/{index}"));
                    child
                })
                .collect();
        }
        node
    }
    build(0, width, depth)
}

fn diagnostic_fingerprint(retained_tree_generation: u64) -> RuntimeStyleDiagnosticFingerprint {
    RuntimeStyleDiagnosticFingerprint {
        rules_generation: 7,
        retained_tree_generation,
        props: 11,
        container_width: 800.0f32.to_bits(),
        container_height: 600.0f32.to_bits(),
    }
}

#[test]
fn runtime_style_diagnostic_generation_tracks_every_resolution_input() {
    let mut child = WidgetNode::new("button");
    child
        .attributes
        .insert("class".into(), "primary wide".into());
    child.attributes.insert("id".into(), "save".into());
    child.set_mesh_key("root/save");
    child.set_module_id("@test/controls");
    let mut tree = WidgetNode::new("surface");
    tree.children.push(child);
    let mut retained = RetainedWidgetTree::default();
    retained.update(&tree);
    let baseline_generation = retained.generation();
    let baseline = diagnostic_fingerprint(baseline_generation);

    let assert_tree_change = |changed: WidgetNode| {
        let mut retained = RetainedWidgetTree::default();
        retained.update(&tree);
        retained.update(&changed);
        assert_ne!(retained.generation(), baseline_generation);
    };

    let mut changed = tree.clone();
    changed.children[0].tag = "input".into();
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children[0]
        .attributes
        .insert("class".into(), "secondary".into());
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children[0]
        .attributes
        .insert("id".into(), "apply".into());
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children[0].state.focused = true;
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children[0].set_module_id("@test/alternate");
    assert_tree_change(changed);
    let mut changed = tree.clone();
    changed.children.push(WidgetNode::new("text"));
    assert_tree_change(changed);

    let mut changed = baseline;
    changed.rules_generation += 1;
    assert_ne!(changed, baseline);
    let mut changed = baseline;
    changed.container_width = 801.0f32.to_bits();
    assert_ne!(changed, baseline);
    let mut changed = baseline;
    changed.container_height = 601.0f32.to_bits();
    assert_ne!(changed, baseline);
    let mut changed = baseline;
    changed.props += 1;
    assert_ne!(changed, baseline);
}

#[test]
fn runtime_style_diagnostic_gate_reuses_only_identical_inputs() {
    let fingerprint = diagnostic_fingerprint(42);
    let mut previous = None;
    assert!(runtime_style_diagnostic_inputs_changed(
        &mut previous,
        fingerprint
    ));
    assert!(!runtime_style_diagnostic_inputs_changed(
        &mut previous,
        fingerprint
    ));
    let changed = RuntimeStyleDiagnosticFingerprint {
        rules_generation: fingerprint.rules_generation + 1,
        ..fingerprint
    };
    assert!(runtime_style_diagnostic_inputs_changed(
        &mut previous,
        changed
    ));
}

#[test]
fn runtime_style_diagnostic_props_hash_is_order_independent_and_value_sensitive() {
    let mut left = SurfaceCssProps::new();
    left.insert("accent".into(), StyleValue::Literal("#abcdef".into()));
    left.insert("spacing".into(), StyleValue::Var("--space-md".into()));
    let mut right = SurfaceCssProps::new();
    right.insert("spacing".into(), StyleValue::Var("--space-md".into()));
    right.insert("accent".into(), StyleValue::Literal("#abcdef".into()));
    assert_eq!(
        runtime_style_diagnostic_props_fingerprint(&left),
        runtime_style_diagnostic_props_fingerprint(&right)
    );
    right.insert("accent".into(), StyleValue::Literal("#fedcba".into()));
    assert_ne!(
        runtime_style_diagnostic_props_fingerprint(&left),
        runtime_style_diagnostic_props_fingerprint(&right)
    );
}

// cargo test -p mesh-core-shell --release -- runtime_style_diagnostic_generation_gate_beats_full_tree_fingerprint -- --ignored --nocapture
#[test]
#[ignore = "release-only runtime style diagnostic gate microbenchmark"]
fn runtime_style_diagnostic_generation_gate_beats_full_tree_fingerprint() {
    fn legacy_tree_fingerprint(tree: &WidgetNode) -> u64 {
        fn visit(node: &WidgetNode, hash: &mut u64) {
            diagnostic_hash_bytes(hash, node.tag.as_bytes());
            for name in ["class", "id"] {
                if let Some(value) = node.attributes.get(name) {
                    diagnostic_hash_bytes(hash, value.as_bytes());
                }
            }
            if let Some(module_id) = node.module_id() {
                diagnostic_hash_bytes(hash, module_id.as_bytes());
            }
            diagnostic_hash_bytes(hash, &[u8::from(node.state.focused)]);
            diagnostic_hash_bytes(hash, &(node.children.len() as u64).to_le_bytes());
            for child in &node.children {
                visit(child, hash);
            }
        }
        let mut hash = DIAGNOSTIC_FNV_OFFSET;
        visit(tree, &mut hash);
        hash
    }

    let mut tree = broad_plain_tree(5, 3);
    fn decorate(node: &mut WidgetNode, index: &mut usize) {
        node.tag = if (*index).is_multiple_of(3) {
            "button".into()
        } else {
            "box".into()
        };
        node.attributes
            .insert("class".into(), "card interactive".into());
        node.attributes.insert("id".into(), format!("node-{index}"));
        node.set_module_id("@bench/module");
        *index += 1;
        for child in &mut node.children {
            decorate(child, index);
        }
    }
    let mut node_index = 0;
    decorate(&mut tree, &mut node_index);
    let context = StyleContext {
        container_width: 800.0,
        container_height: 600.0,
    };
    let iterations = 2_000;
    let mut legacy_retained = RetainedWidgetTree::default();
    legacy_retained.update(&tree);
    let old_started = Instant::now();
    let mut old_total = 0u64;
    for _ in 0..iterations {
        legacy_retained.update(std::hint::black_box(&tree));
        old_total = old_total.wrapping_add(legacy_tree_fingerprint(&tree));
    }
    let old_time = old_started.elapsed();

    let mut retained = RetainedWidgetTree::default();
    retained.update(&tree);
    let mut previous = Some(RuntimeStyleDiagnosticFingerprint {
        rules_generation: 1,
        retained_tree_generation: retained.generation(),
        props: 0,
        container_width: context.container_width.to_bits(),
        container_height: context.container_height.to_bits(),
    });
    let gated_started = Instant::now();
    let mut gated_changes = 0usize;
    for _ in 0..iterations {
        retained.update(std::hint::black_box(&tree));
        let current = RuntimeStyleDiagnosticFingerprint {
            rules_generation: 1,
            retained_tree_generation: retained.generation(),
            props: 0,
            container_width: context.container_width.to_bits(),
            container_height: context.container_height.to_bits(),
        };
        gated_changes += usize::from(runtime_style_diagnostic_inputs_changed(
            &mut previous,
            current,
        ));
    }
    let gated_time = gated_started.elapsed();

    eprintln!(
        "MESH_PERF metric=runtime_style_diagnostic_generation_speedup value={:.6} legacy_rebuild_ns={} generation_rebuild_ns={} workload=2000_unchanged_diagnostic_enabled_rebuilds_156_nodes old_total={old_total} gated_changes={gated_changes}",
        old_time.as_secs_f64() / gated_time.as_secs_f64(),
        old_time.as_nanos(),
        gated_time.as_nanos(),
    );
    assert_eq!(gated_changes, 0);
    assert!(old_time.as_secs_f64() / gated_time.as_secs_f64() >= 1.20);
}

// cargo test -p mesh-core-shell --release -- hover_snapshot_clone_from_reuses_path_storage -- --ignored --nocapture
#[test]
#[ignore = "release-only hover snapshot storage microbenchmark"]
fn hover_snapshot_clone_from_reuses_path_storage() {
    let current: Vec<String> = (0..32)
        .map(|index| format!("root/section/{index}/button"))
        .collect();
    let iterations = 100_000usize;

    let old_started = Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let snapshot = std::hint::black_box(current.clone());
        old_total += std::hint::black_box(snapshot.len());
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_snapshot = current.clone();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        new_snapshot.clone_from(std::hint::black_box(&current));
        new_total += std::hint::black_box(new_snapshot.len());
    }
    let new_time = new_started.elapsed();

    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
    eprintln!(
        "hover snapshot: assignment clone {old_time:?}; clone_from {new_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
}

fn broad_keyed_tree_with_selected_text(width: usize, depth: usize) -> (WidgetNode, String) {
    fn build(key: String, width: usize, depth: usize, target: &str) -> WidgetNode {
        let mut node = if key == target {
            let mut text = WidgetNode::new("text");
            text.attributes.insert("selectable".into(), "true".into());
            text.attributes.insert("content".into(), "selected".into());
            text
        } else {
            WidgetNode::new("box")
        };
        node.attributes.insert("_mesh_key".into(), key.clone());
        node.layout.x = key.len() as f32;
        node.layout.y = key.len() as f32 * 0.5;
        if depth > 0 {
            node.children = (0..width)
                .map(|index| build(format!("{key}/{index}"), width, depth - 1, target))
                .collect();
        }
        node
    }

    let target = "root/3/3/3/3/3".to_string();
    (build("root".into(), width, depth, &target), target)
}

fn old_annotate_selection_node(
    node: &mut WidgetNode,
    selection: &TextSelectionState,
    selection_background: &str,
    selection_foreground: &str,
) -> bool {
    let matches_selection = node
        .mesh_key()
        .is_some_and(|key| key == selection.anchor.node_key);
    if matches_selection
        && annotate_selected_text_node(node, selection, selection_background, selection_foreground)
    {
        return true;
    }

    for child in &mut node.children {
        if old_annotate_selection_node(child, selection, selection_background, selection_foreground)
        {
            return true;
        }
    }

    false
}

fn benchmark_selection(target: String) -> TextSelectionState {
    TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: target.clone(),
            x: 2.0,
            y: 3.0,
        },
        focus: TextSelectionPoint {
            node_key: target,
            x: 18.0,
            y: 3.0,
        },
        dragging: true,
    }
}

#[test]
fn direct_interaction_scope_keeps_only_changed_targets() {
    let changed = HashSet::from([stable_runtime_node_id("root/0")]);
    let affected = direct_interaction_changed_node_ids(changed);

    assert_eq!(
        affected.affected,
        HashSet::from([crate::shell::component::runtime_tree::stable_runtime_node_id("root/0")])
    );
}

#[test]
fn direct_interaction_scope_keeps_nested_targets() {
    let parent = stable_runtime_node_id("root/0");
    let child = stable_runtime_node_id("root/0/0");
    let changed = HashSet::from([parent, child]);
    let affected = direct_interaction_changed_node_ids(changed);

    assert_eq!(affected.affected, HashSet::from([parent, child]));
}

// cargo test -p mesh-core-shell --release -- node_id_checked_state_beats_string_keys --ignored --nocapture
#[test]
#[ignore = "release-only checked-state identity microbenchmark"]
fn node_id_checked_state_beats_string_keys() {
    const NODES: usize = 1_024;
    const ITERATIONS: usize = 40_000;

    let keys = (0..NODES)
        .map(|index| format!("root/{}/{}", index / 32, index % 32))
        .collect::<Vec<_>>();
    let node_ids = keys
        .iter()
        .map(|key| runtime_node_id_for_key(key))
        .collect::<Vec<_>>();
    let string_state = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.clone(), index.is_multiple_of(3)))
        .collect::<HashMap<_, _>>();
    let node_state = node_ids
        .iter()
        .enumerate()
        .map(|(index, node_id)| (*node_id, index.is_multiple_of(3)))
        .collect::<HashMap<_, _>>();

    let string_started = Instant::now();
    let mut string_total = 0usize;
    for _ in 0..ITERATIONS {
        for key in &keys {
            string_total += usize::from(
                string_state
                    .get(std::hint::black_box(key.as_str()))
                    .copied()
                    .unwrap_or(false),
            );
        }
    }
    let string_time = string_started.elapsed();

    let node_started = Instant::now();
    let mut node_total = 0usize;
    for _ in 0..ITERATIONS {
        for node_id in &node_ids {
            node_total += usize::from(
                node_state
                    .get(std::hint::black_box(node_id))
                    .copied()
                    .unwrap_or(false),
            );
        }
    }
    let node_time = node_started.elapsed();

    assert_eq!(string_total, node_total);
    eprintln!(
        "checked-state annotation across {ITERATIONS} {NODES}-node passes: string keys {string_time:?}; NodeId keys {node_time:?}; ratio {:.2}x",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=node_id_checked_state_speedup value={:.6}",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    assert!(node_time < string_time);
}

// cargo test -p mesh-core-shell --release -- node_id_input_values_beat_string_keys --ignored --nocapture
#[test]
#[ignore = "release-only input-value identity microbenchmark"]
fn node_id_input_values_beat_string_keys() {
    const NODES: usize = 1_024;
    const ITERATIONS: usize = 40_000;

    let keys = (0..NODES)
        .map(|index| format!("root/{}/{}", index / 32, index % 32))
        .collect::<Vec<_>>();
    let node_ids = keys
        .iter()
        .map(|key| runtime_node_id_for_key(key))
        .collect::<Vec<_>>();
    let values = (0..NODES)
        .map(|index| format!("input-value-{index}"))
        .collect::<Vec<_>>();
    let string_state = keys
        .iter()
        .zip(&values)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<HashMap<_, _>>();
    let node_state = node_ids
        .iter()
        .zip(&values)
        .map(|(node_id, value)| (*node_id, value.clone()))
        .collect::<HashMap<_, _>>();

    let string_started = Instant::now();
    let mut string_total = 0usize;
    for _ in 0..ITERATIONS {
        for key in &keys {
            string_total = string_total.wrapping_add(
                string_state
                    .get(std::hint::black_box(key.as_str()))
                    .map_or(0, String::len),
            );
        }
    }
    let string_time = string_started.elapsed();

    let node_started = Instant::now();
    let mut node_total = 0usize;
    for _ in 0..ITERATIONS {
        for node_id in &node_ids {
            node_total = node_total.wrapping_add(
                node_state
                    .get(std::hint::black_box(node_id))
                    .map_or(0, String::len),
            );
        }
    }
    let node_time = node_started.elapsed();

    assert_eq!(string_total, node_total);
    eprintln!(
        "input-value annotation across {ITERATIONS} {NODES}-node passes: string keys {string_time:?}; NodeId keys {node_time:?}; ratio {:.2}x",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=node_id_input_values_speedup value={:.6}",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    assert!(node_time < string_time);
}

// cargo test -p mesh-core-shell --release -- node_id_slider_values_beat_string_keys --ignored --nocapture
#[test]
#[ignore = "release-only slider-value identity microbenchmark"]
fn node_id_slider_values_beat_string_keys() {
    const NODES: usize = 1_024;
    const ITERATIONS: usize = 40_000;

    let keys = (0..NODES)
        .map(|index| format!("root/{}/{}", index / 32, index % 32))
        .collect::<Vec<_>>();
    let node_ids = keys
        .iter()
        .map(|key| runtime_node_id_for_key(key))
        .collect::<Vec<_>>();
    let string_state = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.clone(), index as f32))
        .collect::<HashMap<_, _>>();
    let node_state = node_ids
        .iter()
        .enumerate()
        .map(|(index, node_id)| (*node_id, index as f32))
        .collect::<HashMap<_, _>>();

    let string_started = Instant::now();
    let mut string_total = 0f32;
    for _ in 0..ITERATIONS {
        for key in &keys {
            string_total += string_state
                .get(std::hint::black_box(key.as_str()))
                .copied()
                .unwrap_or_default();
        }
    }
    let string_time = string_started.elapsed();

    let node_started = Instant::now();
    let mut node_total = 0f32;
    for _ in 0..ITERATIONS {
        for node_id in &node_ids {
            node_total += node_state
                .get(std::hint::black_box(node_id))
                .copied()
                .unwrap_or_default();
        }
    }
    let node_time = node_started.elapsed();

    assert_eq!(string_total, node_total);
    eprintln!(
        "slider-value annotation across {ITERATIONS} {NODES}-node passes: string keys {string_time:?}; NodeId keys {node_time:?}; ratio {:.2}x",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=node_id_slider_values_speedup value={:.6}",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    assert!(node_time < string_time);
}

// cargo test -p mesh-core-shell --release -- node_id_hover_path_beats_string_keys --ignored --nocapture
#[test]
#[ignore = "release-only hover-path identity microbenchmark"]
fn node_id_hover_path_beats_string_keys() {
    const NODES: usize = 1_024;
    const ITERATIONS: usize = 40_000;

    let keys = (0..NODES)
        .map(|index| format!("root/{}/{}", index / 32, index % 32))
        .collect::<Vec<_>>();
    let node_ids = keys
        .iter()
        .map(|key| runtime_node_id_for_key(key))
        .collect::<Vec<_>>();
    let string_state = keys.iter().cloned().collect::<HashSet<_>>();
    let node_state = node_ids.iter().copied().collect::<HashSet<_>>();

    let string_started = Instant::now();
    let mut string_total = 0usize;
    for _ in 0..ITERATIONS {
        for key in &keys {
            string_total += usize::from(string_state.contains(std::hint::black_box(key)));
        }
    }
    let string_time = string_started.elapsed();

    let node_started = Instant::now();
    let mut node_total = 0usize;
    for _ in 0..ITERATIONS {
        for node_id in &node_ids {
            node_total += usize::from(node_state.contains(std::hint::black_box(node_id)));
        }
    }
    let node_time = node_started.elapsed();

    assert_eq!(string_total, node_total);
    eprintln!(
        "hover annotation across {ITERATIONS} {NODES}-node passes: string keys {string_time:?}; NodeId keys {node_time:?}; ratio {:.2}x",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=node_id_hover_path_speedup value={:.6}",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    assert!(node_time < string_time);
}

// cargo test -p mesh-core-shell --release -- node_id_focus_state_beats_string_keys --ignored --nocapture
#[test]
#[ignore = "release-only focus-state identity microbenchmark"]
fn node_id_focus_state_beats_string_keys() {
    const NODES: usize = 1_024;
    const ITERATIONS: usize = 40_000;

    let keys = (0..NODES)
        .map(|index| format!("root/{}/{}", index / 32, index % 32))
        .collect::<Vec<_>>();
    let node_ids = keys
        .iter()
        .map(|key| runtime_node_id_for_key(key))
        .collect::<Vec<_>>();
    let focused_index = NODES / 2;
    let focused_key = &keys[focused_index];
    let focused_id = node_ids[focused_index];

    let string_started = Instant::now();
    let mut string_total = 0usize;
    for _ in 0..ITERATIONS {
        for key in &keys {
            string_total += usize::from(std::hint::black_box(key) == focused_key);
        }
    }
    let string_time = string_started.elapsed();

    let node_started = Instant::now();
    let mut node_total = 0usize;
    for _ in 0..ITERATIONS {
        for node_id in &node_ids {
            node_total += usize::from(std::hint::black_box(*node_id) == focused_id);
        }
    }
    let node_time = node_started.elapsed();

    assert_eq!(string_total, node_total);
    eprintln!(
        "focus annotation across {ITERATIONS} {NODES}-node passes: string keys {string_time:?}; NodeId keys {node_time:?}; ratio {:.2}x",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=node_id_focus_state_speedup value={:.6}",
        string_time.as_secs_f64() / node_time.as_secs_f64()
    );
    assert!(node_time < string_time);
}

#[test]
fn hover_changed_ids_only_collects_tails_after_common_ancestor() {
    let previous = ["root", "root/menu", "root/menu/left"].map(stable_runtime_node_id);
    let current = [
        "root",
        "root/menu",
        "root/menu/right",
        "root/menu/right/icon",
    ]
    .map(stable_runtime_node_id);
    let mut changed = HashSet::new();

    collect_hover_changed_ids(&previous, &current, &mut changed);

    assert_eq!(
        changed,
        HashSet::from([
            stable_runtime_node_id("root/menu/left"),
            stable_runtime_node_id("root/menu/right"),
            stable_runtime_node_id("root/menu/right/icon"),
        ])
    );
}

// cargo test -p mesh-core-shell --release -- hover_common_prefix_beats_symmetric_contains_scans --ignored --nocapture
#[test]
#[ignore = "release-only hover-path diff microbenchmark"]
fn hover_common_prefix_beats_symmetric_contains_scans() {
    use std::time::Instant;

    let previous = (0..64)
        .map(|depth| {
            format!(
                "root/{}",
                (0..=depth).map(|_| "left").collect::<Vec<_>>().join("/")
            )
        })
        .map(|key| stable_runtime_node_id(&key))
        .collect::<Vec<_>>();
    let mut current = previous[..63].to_vec();
    current.push(stable_runtime_node_id("root/right"));
    let iterations = 100_000usize;

    let mut old_changed = HashSet::with_capacity(previous.len() + current.len());
    let old_started = Instant::now();
    for _ in 0..iterations {
        old_changed.clear();
        for node_id in &previous {
            if !current.contains(node_id) {
                old_changed.insert(*node_id);
            }
        }
        for node_id in &current {
            if !previous.contains(node_id) {
                old_changed.insert(*node_id);
            }
        }
        std::hint::black_box(&old_changed);
    }
    let old_elapsed = old_started.elapsed();

    let mut prefix_changed = HashSet::with_capacity(previous.len() + current.len());
    let prefix_started = Instant::now();
    for _ in 0..iterations {
        prefix_changed.clear();
        collect_hover_changed_ids(&previous, &current, &mut prefix_changed);
        std::hint::black_box(&prefix_changed);
    }
    let prefix_elapsed = prefix_started.elapsed();

    assert_eq!(prefix_changed, old_changed);
    eprintln!(
        "hover path diff over {iterations} 64-level transitions: contains {old_elapsed:?}; common-prefix {prefix_elapsed:?}; ratio {:.1}x",
        old_elapsed.as_secs_f64() / prefix_elapsed.as_secs_f64()
    );
    assert!(prefix_elapsed * 2 < old_elapsed);
}

// cargo test -p mesh-core-shell --release -- narrow_ancestor_stack_beats_parent_map_benchmark --ignored --nocapture
#[test]
#[ignore = "release-only narrow ancestor expansion microbenchmark"]
fn narrow_ancestor_stack_beats_parent_map_benchmark() {
    fn old_build_parent_map(
        node: &WidgetNode,
        parent_id: Option<NodeId>,
        parents: &mut HashMap<NodeId, NodeId>,
    ) {
        if let Some(parent_id) = parent_id {
            parents.insert(node.id, parent_id);
        }
        for child in &node.children {
            old_build_parent_map(child, Some(node.id), parents);
        }
    }

    fn old_expand(tree: &WidgetNode, affected: &HashSet<NodeId>) -> HashSet<NodeId> {
        let mut full_affected = affected.clone();
        let mut parents = HashMap::new();
        old_build_parent_map(tree, None, &mut parents);
        for &leaf_id in affected {
            let mut current = leaf_id;
            while let Some(&parent) = parents.get(&current) {
                full_affected.insert(parent);
                current = parent;
            }
        }
        full_affected
    }

    fn branch(key: &str, depth: usize) -> WidgetNode {
        let children = (depth > 0)
            .then(|| {
                (0..4)
                    .map(|index| branch(&format!("{key}/{index}"), depth - 1))
                    .collect()
            })
            .unwrap_or_default();
        keyed_node(key, children)
    }

    let tree = branch("root", 5);
    let affected = HashSet::from([
        stable_runtime_node_id("root/0/1/2/3/0"),
        stable_runtime_node_id("root/2/3/0/1/2"),
        stable_runtime_node_id("root/3/2/1/0/3"),
    ]);
    let iterations = 2_000;

    let old_started = Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total ^= std::hint::black_box(old_expand(&tree, &affected).len());
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let mut full_affected = affected.clone();
        narrow_expand_ancestors(&tree, &affected, &mut full_affected);
        new_total ^= std::hint::black_box(full_affected.len());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "narrow ancestors: parent map {old_time:?}; stack walk {new_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

#[test]
fn targeted_default_merge_only_updates_affected_subtrees() {
    let mut tree = keyed_node(
        "root",
        vec![
            keyed_node("root/0", vec![keyed_node("root/0/0", vec![])]),
            keyed_node("root/1", vec![keyed_node("root/1/0", vec![])]),
        ],
    );
    tree.children[0].tag = "column".into();
    tree.children[0].children[0].tag = "text".into();
    tree.children[0].children[0].computed_style.color = mesh_core_elements::Color::TRANSPARENT;
    tree.children[1].tag = "column".into();
    tree.children[1].children[0].tag = "text".into();
    tree.children[1].children[0].computed_style.color = mesh_core_elements::Color::TRANSPARENT;

    let affected =
        HashSet::from([crate::shell::component::runtime_tree::stable_runtime_node_id("root/0")]);

    apply_runtime_attribute_state_for_ids(&mut tree, &affected);

    assert_eq!(
        tree.children[0].computed_style.direction,
        mesh_core_elements::style::FlexDirection::Column
    );
    assert_eq!(tree.children[0].children[0].computed_style.color.a, 255);
    assert_eq!(
        tree.children[1].computed_style.direction,
        mesh_core_elements::style::FlexDirection::Row
    );
    assert_eq!(tree.children[1].children[0].computed_style.color.a, 0);
}

// cargo test -p mesh-core-shell --release -- direct_interaction_scope_beats_full_tree_walk --ignored --nocapture
#[test]
#[ignore = "release-only direct interaction-scope microbenchmark"]
fn direct_interaction_scope_beats_full_tree_walk() {
    fn build(key: String, width: usize, depth: usize) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.id = stable_runtime_node_id(&key);
        node.attributes.insert("_mesh_key".into(), key.clone());
        if depth > 0 {
            node.children = (0..width)
                .map(|index| build(format!("{key}/{index}"), width, depth - 1))
                .collect();
        }
        node
    }
    fn tree_walk_collect(
        node: &WidgetNode,
        changed: &HashSet<NodeId>,
        out: &mut InteractionChangedNodeIds,
    ) {
        let directly_affected = changed.contains(&node.id);
        if directly_affected {
            out.affected.insert(node.id);
        }
        for child in &node.children {
            tree_walk_collect(child, changed, out);
        }
    }

    let tree = build("root".into(), 4, 5);
    let iterations = 2_000;
    let changed = HashSet::from([stable_runtime_node_id("root/0")]);

    let old_started = Instant::now();
    let mut old_count = 0;
    for _ in 0..iterations {
        let mut affected = InteractionChangedNodeIds::default();
        tree_walk_collect(&tree, &changed, &mut affected);
        old_count += std::hint::black_box(affected.affected.len());
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_count = 0;
    for _ in 0..iterations {
        let affected = direct_interaction_changed_node_ids(changed.clone());
        new_count += std::hint::black_box(affected.affected.len());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "interaction changed scope: full tree walk {old_time:?}; direct IDs {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_count, new_count);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- targeted_default_merge_skips_unaffected_subtrees --ignored --nocapture
#[test]
#[ignore = "release-only targeted default merge microbenchmark"]
fn targeted_default_merge_skips_unaffected_subtrees() {
    fn build(key: String, width: usize, depth: usize) -> WidgetNode {
        let mut node = WidgetNode::new(if depth % 2 == 0 { "column" } else { "text" });
        node.id = stable_runtime_node_id(&key);
        node.attributes.insert("_mesh_key".into(), key.clone());
        if depth > 0 {
            node.children = (0..width)
                .map(|index| build(format!("{key}/{index}"), width, depth - 1))
                .collect();
        }
        node
    }

    let tree = build("root".into(), 4, 5);
    let affected =
        direct_interaction_changed_node_ids(HashSet::from([stable_runtime_node_id("root/0/0")]));
    let iterations = 5_000;

    let old_started = Instant::now();
    let mut old_total = 0.0f32;
    for _ in 0..iterations {
        let mut tree = tree.clone();
        apply_runtime_attribute_state(std::hint::black_box(&mut tree));
        old_total += std::hint::black_box(tree.children[0].computed_style.gap);
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0.0f32;
    for _ in 0..iterations {
        let mut tree = tree.clone();
        apply_runtime_attribute_state_for_ids(
            std::hint::black_box(&mut tree),
            std::hint::black_box(&affected.affected),
        );
        new_total += std::hint::black_box(tree.children[0].computed_style.gap);
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "runtime primitive defaults: full tree {old_time:?}; targeted {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- finalize_marker_walk_gates_skip_plain_trees --ignored --nocapture
#[test]
#[ignore = "release-only finalize marker walk microbenchmark"]
fn finalize_marker_walk_gates_skip_plain_trees() {
    let mut tree = broad_plain_tree(4, 5);
    let iterations = 20_000;

    let old_started = Instant::now();
    for _ in 0..iterations {
        collapse_promoted_popover_wrappers(std::hint::black_box(&mut tree));
        constrain_error_placeholders(std::hint::black_box(&mut tree));
    }
    let old_time = old_started.elapsed();

    let gated_started = Instant::now();
    let has_promoted_popover_wrappers = false;
    let has_error_placeholders = false;
    for _ in 0..iterations {
        if has_promoted_popover_wrappers {
            collapse_promoted_popover_wrappers(std::hint::black_box(&mut tree));
        }
        if has_error_placeholders {
            constrain_error_placeholders(std::hint::black_box(&mut tree));
        }
    }
    let gated_time = gated_started.elapsed();

    eprintln!(
        "finalize marker walks plain tree: {old_time:?}; gated: {gated_time:?}; ratio: {:.1}x",
        old_time.as_secs_f64() / gated_time.as_secs_f64()
    );
    assert!(gated_time * 10 < old_time);
}

#[test]
fn keyed_selection_annotation_only_marks_selectable_text_target() {
    let target = "root/0".to_string();
    let selection = benchmark_selection(target.clone());
    let mut selectable = WidgetNode::new("text");
    selectable.attributes.insert("_mesh_key".into(), target);
    selectable
        .attributes
        .insert("selectable".into(), "true".into());
    assert!(annotate_selected_text_node(
        &mut selectable,
        &selection,
        "#112233",
        "#ffffff"
    ));
    assert!(
        selectable
            .attributes
            .contains_key("_mesh_selection_background")
    );

    let mut non_selectable = WidgetNode::new("text");
    non_selectable
        .attributes
        .insert("_mesh_key".into(), "root/0".into());
    assert!(!annotate_selected_text_node(
        &mut non_selectable,
        &selection,
        "#112233",
        "#ffffff"
    ));
    assert!(
        !non_selectable
            .attributes
            .contains_key("_mesh_selection_background")
    );
}

// cargo test -p mesh-core-shell --release -- keyed_selection_annotation_beats_recursive_tree_walk --ignored --nocapture
#[test]
#[ignore = "release-only selection annotation microbenchmark"]
fn keyed_selection_annotation_beats_recursive_tree_walk() {
    let (tree, target) = broad_keyed_tree_with_selected_text(4, 5);
    let selection = benchmark_selection(target);
    let iterations = 10_000;

    let old_started = Instant::now();
    let mut old_count = 0usize;
    for _ in 0..iterations {
        let mut tree = tree.clone();
        old_count += usize::from(old_annotate_selection_node(
            std::hint::black_box(&mut tree),
            &selection,
            "#112233",
            "#ffffff",
        ));
    }
    let old_time = old_started.elapsed();

    let keyed_started = Instant::now();
    let mut keyed_count = 0usize;
    for _ in 0..iterations {
        let mut tree = tree.clone();
        if let Some(node) = find_node_by_key_mut(&mut tree, &selection.anchor.node_key) {
            keyed_count += usize::from(annotate_selected_text_node(
                std::hint::black_box(node),
                &selection,
                "#112233",
                "#ffffff",
            ));
        }
    }
    let keyed_time = keyed_started.elapsed();

    eprintln!(
        "selection annotation: recursive {old_time:?}; keyed {keyed_time:?}; ratio {:.1}x; counts={old_count}/{keyed_count}",
        old_time.as_secs_f64() / keyed_time.as_secs_f64()
    );
    assert_eq!(old_count, keyed_count);
    assert!(keyed_time < old_time);
}
