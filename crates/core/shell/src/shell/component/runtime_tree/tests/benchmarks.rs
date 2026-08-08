use super::*;

// cargo test -p mesh-core-shell --release -- runtime_tree_primitive_hashing_beats_byte_fallback --ignored --nocapture
#[test]
#[ignore = "release-only retained-tree fingerprint microbenchmark"]
fn runtime_tree_primitive_hashing_beats_byte_fallback() {
    let style = benchmark_style();
    let iterations = 500_000;

    let old_started = Instant::now();
    let mut old_accumulator = 0u64;
    for _ in 0..iterations {
        let mut hasher = ByteOnlyRuntimeTreeHasher(FNV_OFFSET);
        hash_style_fields(std::hint::black_box(&style), &mut hasher);
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_accumulator = 0u64;
    for _ in 0..iterations {
        new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(style_fingerprint(
            std::hint::black_box(&style),
        )));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "runtime tree style fingerprint byte fallback: {old_time:?}; primitive-aware: {new_time:?}; ratio: {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_ne!(old_accumulator, 0);
    assert_ne!(new_accumulator, 0);
    assert!(new_time * 5 < old_time * 4);
}

// cargo test -p mesh-core-shell --release -- structural_key_id_beats_tree_rediscovery --ignored --nocapture
#[test]
#[ignore = "release-only structural interaction ID microbenchmark"]
fn structural_key_id_beats_tree_rediscovery() {
    fn build(key: String, node_id: NodeId, width: usize, depth: usize) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.id = node_id;
        node.set_mesh_key(key.clone());
        if depth > 0 {
            node.children = (0..width)
                .map(|index| {
                    build(
                        format!("{key}/{index}"),
                        child_runtime_node_id(node_id, index),
                        width,
                        depth - 1,
                    )
                })
                .collect();
        }
        node
    }

    fn find_id(node: &WidgetNode, key: &str) -> Option<NodeId> {
        if node.mesh_key() == Some(key) {
            return Some(node.id);
        }
        node.children.iter().find_map(|child| find_id(child, key))
    }

    let root_id = stable_runtime_node_id("root");
    let tree = build("root".into(), root_id, 4, 5);
    let key = "root/3/3/3/3/3";
    let iterations = 2_000usize;
    assert_eq!(find_id(&tree, key), Some(runtime_node_id_for_key(key)));

    let walk_started = std::time::Instant::now();
    let mut walk_total = 0u64;
    for _ in 0..iterations {
        walk_total ^= find_id(std::hint::black_box(&tree), std::hint::black_box(key)).unwrap();
    }
    let walk_time = walk_started.elapsed();

    let direct_started = std::time::Instant::now();
    let mut direct_total = 0u64;
    for _ in 0..iterations {
        direct_total ^= runtime_node_id_for_key(std::hint::black_box(key));
    }
    let direct_time = direct_started.elapsed();

    eprintln!(
        "interaction ID lookup over {iterations} passes of a 1,365-node tree: walk {walk_time:?}; structural key {direct_time:?}; ratio {:.1}x",
        walk_time.as_secs_f64() / direct_time.as_secs_f64()
    );
    assert_eq!(walk_total, direct_total);
    assert!(direct_time < walk_time);
}

// cargo test -p mesh-core-shell --release -- chained_runtime_ids_beat_rehashing_deep_paths --ignored --nocapture
#[test]
#[ignore = "release-only runtime node id microbenchmark"]
fn chained_runtime_ids_beat_rehashing_deep_paths() {
    let paths = (0..10)
        .scan("root".to_string(), |path, index| {
            path.push('/');
            path.push_str(&index.to_string());
            Some(path.clone())
        })
        .collect::<Vec<_>>();
    let iterations = 500_000;

    let old_started = Instant::now();
    let mut old_accumulator = 0u64;
    for _ in 0..iterations {
        for path in &paths {
            old_accumulator ^= stable_runtime_node_id(std::hint::black_box(path));
        }
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_accumulator = 0u64;
    for _ in 0..iterations {
        let mut parent = stable_runtime_node_id("root");
        for index in 0..paths.len() {
            parent = child_runtime_node_id(parent, index);
            new_accumulator ^= std::hint::black_box(parent);
        }
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "runtime node ids: full-path hash {old_time:?}; parent-chain {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- typed_json_arg_hashing_beats_to_string_fingerprint --ignored --nocapture
#[test]
#[ignore = "release-only JSON handler arg fingerprint microbenchmark"]
fn typed_json_arg_hashing_beats_to_string_fingerprint() {
    fn old_hash_json_value(value: &serde_json::Value, hasher: &mut impl Hasher) {
        value.to_string().hash(hasher);
    }

    let arg = serde_json::json!({
        "id": "alpha",
        "meta": {
            "index": 42,
            "enabled": true,
            "ratio": 0.875,
            "label": "A moderately long label used by a pre-bound handler"
        },
        "tags": ["audio", "primary", "interactive", "toolbar"],
        "bounds": { "x": 10, "y": 20, "width": 140, "height": 32 }
    });
    let iterations = 500_000;

    let old_started = Instant::now();
    let mut old_accumulator = 0u64;
    for _ in 0..iterations {
        let mut hasher = RuntimeTreeHasher::default();
        old_hash_json_value(std::hint::black_box(&arg), &mut hasher);
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_accumulator = 0u64;
    for _ in 0..iterations {
        let mut hasher = RuntimeTreeHasher::default();
        hash_json_value(std::hint::black_box(&arg), &mut hasher);
        new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "typed JSON arg fingerprint: to_string {old_time:?}; direct hash {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_ne!(old_accumulator, 0);
    assert_ne!(new_accumulator, 0);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- focused_annotation_skip_beats_redundant_attribute_hash --ignored --nocapture
#[test]
#[ignore = "release-only focused annotation fingerprint microbenchmark"]
fn focused_annotation_skip_beats_redundant_attribute_hash() {
    fn old_attributes_fingerprint(node: &WidgetNode) -> u64 {
        let mut hasher = RuntimeTreeHasher::default();
        node.tag.hash(&mut hasher);
        for (key, value) in &node.attributes {
            if key == "_mesh_key" {
                continue;
            }
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }
        hasher.finish()
    }

    let mut node = WidgetNode::new("input");
    node.attributes.insert("_mesh_key".into(), "root/0".into());
    node.attributes
        .insert("_mesh_focused".into(), "true".into());
    node.attributes
        .insert("value".into(), "active field".into());
    node.attributes.insert("placeholder".into(), "Name".into());
    let iterations = 2_000_000;

    let old_started = Instant::now();
    let mut old_accumulator = 0u64;
    for _ in 0..iterations {
        old_accumulator =
            old_accumulator.wrapping_add(old_attributes_fingerprint(std::hint::black_box(&node)));
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_accumulator = 0u64;
    for _ in 0..iterations {
        new_accumulator =
            new_accumulator.wrapping_add(attributes_fingerprint(std::hint::black_box(&node)));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "focused annotation fingerprint: redundant attribute hash {old_time:?}; skipped {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_ne!(old_accumulator, 0);
    assert_ne!(new_accumulator, 0);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- scroll_annotation_skip_beats_redundant_attribute_hash --ignored --nocapture
#[test]
#[ignore = "release-only scroll annotation fingerprint microbenchmark"]
fn scroll_annotation_skip_beats_redundant_attribute_hash() {
    fn old_attributes_fingerprint(node: &WidgetNode) -> u64 {
        let mut hasher = RuntimeTreeHasher::default();
        node.tag.hash(&mut hasher);
        for (key, value) in &node.attributes {
            if key == "_mesh_key" || key == "_mesh_focused" {
                continue;
            }
            if key == "content" && !node.children.is_empty() {
                continue;
            }
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }
        hasher.finish()
    }

    let mut node = WidgetNode::new("scroll-area");
    node.attributes.insert("_mesh_key".into(), "root/0".into());
    node.attributes.insert("class".into(), "scroller".into());
    node.attributes
        .insert("_mesh_scroll_x".into(), "12.5".into());
    node.attributes
        .insert("_mesh_scroll_y".into(), "24.75".into());
    node.attributes
        .insert("_mesh_scroll_max_x".into(), "360.125".into());
    node.attributes
        .insert("_mesh_scroll_max_y".into(), "480.875".into());
    node.attributes
        .insert("_mesh_content_width".into(), "720.25".into());
    node.attributes
        .insert("_mesh_content_height".into(), "960.5".into());
    let iterations = 2_000_000;

    let old_started = Instant::now();
    let mut old_accumulator = 0u64;
    for _ in 0..iterations {
        old_accumulator =
            old_accumulator.wrapping_add(old_attributes_fingerprint(std::hint::black_box(&node)));
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_accumulator = 0u64;
    for _ in 0..iterations {
        new_accumulator =
            new_accumulator.wrapping_add(attributes_fingerprint(std::hint::black_box(&node)));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "scroll annotation fingerprint: redundant attribute hash {old_time:?}; skipped {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_ne!(old_accumulator, 0);
    assert_ne!(new_accumulator, 0);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- lazy_source_tag_checks_beat_eager_annotation_allocation --ignored --nocapture
#[test]
#[ignore = "release-only runtime annotation source-tag microbenchmark"]
fn lazy_source_tag_checks_beat_eager_annotation_allocation() {
    fn old_eager_source_tag_walk(node: &WidgetNode) -> usize {
        let source_tag = source_element_tag(node).to_string();
        let mut total = source_tag.len();
        if node_is_source(node, &["switch", "checkbox", "radio", "option"])
            && matches!(source_tag.as_str(), "radio" | "option")
        {
            total += 1;
        }
        if node_is_source(node, &["select", "radio-group"]) {
            total += 1;
        }
        for child in &node.children {
            total += old_eager_source_tag_walk(child);
        }
        total
    }

    fn new_lazy_source_tag_walk(node: &WidgetNode) -> usize {
        let source_tag = source_element_tag(node);
        let checkable_choice = matches!(source_tag, "switch" | "checkbox" | "radio" | "option");
        let selects_choice = matches!(source_tag, "radio" | "option");
        let selectable_group = matches!(source_tag, "select" | "radio-group");
        let mut total = usize::from(checkable_choice) * source_tag.len();
        if checkable_choice && selects_choice {
            total += 1;
        }
        if selectable_group {
            total += 1;
        }
        for child in &node.children {
            total += new_lazy_source_tag_walk(child);
        }
        total
    }

    let tree = benchmark_plain_tree(4, 5);
    let iterations = 20_000;

    let old_started = Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total += old_eager_source_tag_walk(std::hint::black_box(&tree));
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        new_total += new_lazy_source_tag_walk(std::hint::black_box(&tree));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "runtime annotation source tags: eager allocation {old_time:?}; lazy borrowed checks {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_ne!(old_total, 0);
    assert_eq!(new_total, 0);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- attribute_fingerprint_skips_redundant_runtime_key_hash --ignored --nocapture
#[test]
#[ignore = "release-only attribute fingerprint microbenchmark"]
fn attribute_fingerprint_skips_redundant_runtime_key_hash() {
    fn old_attributes_fingerprint(node: &WidgetNode) -> u64 {
        let mut hasher = RuntimeTreeHasher::default();
        node.tag.hash(&mut hasher);
        for (key, value) in &node.attributes {
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }
        hasher.finish()
    }

    let mut node = WidgetNode::new("box");
    node.id = stable_runtime_node_id("root/0/1/2/3/4/5/6/7/8/9");
    node.attributes
        .insert("_mesh_key".into(), "root/0/1/2/3/4/5/6/7/8/9".into());
    node.attributes.insert("class".into(), "card active".into());
    node.attributes.insert("role".into(), "button".into());
    let iterations = 2_000_000;

    let old_started = Instant::now();
    let mut old_accumulator = 0u64;
    for _ in 0..iterations {
        old_accumulator =
            old_accumulator.wrapping_add(old_attributes_fingerprint(std::hint::black_box(&node)));
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_accumulator = 0u64;
    for _ in 0..iterations {
        new_accumulator =
            new_accumulator.wrapping_add(attributes_fingerprint(std::hint::black_box(&node)));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "attribute fingerprint: runtime-key hash {old_time:?}; node-id identity {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- inline_child_ids_beat_fresh_vec_allocations --ignored --nocapture
#[test]
#[ignore = "release-only retained child-id allocation microbenchmark"]
fn inline_child_ids_beat_fresh_vec_allocations() {
    let child_ids = [11_u64, 12, 13, 14];
    let iterations = 2_000_000;

    let old_started = Instant::now();
    let mut old_total = 0u64;
    for _ in 0..iterations {
        let ids = child_ids.iter().copied().collect::<Vec<NodeId>>();
        old_total = old_total.wrapping_add(std::hint::black_box(ids)[0]);
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0u64;
    for _ in 0..iterations {
        let ids = child_ids.iter().copied().collect::<SmallVec<[NodeId; 8]>>();
        new_total = new_total.wrapping_add(std::hint::black_box(ids)[0]);
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "retained child ids: Vec {old_time:?}; inline SmallVec {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- reused_dirty_secondary_map_beats_fresh_allocations --ignored --nocapture
#[test]
#[ignore = "release-only retained dirty-map allocation microbenchmark"]
fn reused_dirty_secondary_map_beats_fresh_allocations() {
    let mut nodes: SlotMap<RetainedNodeKey, RetainedNodeSnapshot> = SlotMap::with_key();
    let keys = (0..128)
        .map(|_| {
            nodes.insert(RetainedNodeSnapshot {
                layout: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                style_hash: 0,
                attributes_hash: 0,
                child_ids: SmallVec::new(),
                state: ElementState::default(),
                render: RenderObjectFingerprint::for_node(&WidgetNode::new("box"), None),
                last_seen_epoch: 0,
            })
        })
        .collect::<Vec<_>>();
    let iterations = 20_000;

    let old_started = Instant::now();
    let mut old_count = 0;
    for _ in 0..iterations {
        let mut dirty = SecondaryMap::new();
        for key in &keys {
            dirty.insert(*key, RetainedNodeDirtyFlags::STATE);
        }
        old_count += std::hint::black_box(dirty.len());
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_count = 0;
    let mut dirty = SecondaryMap::new();
    for _ in 0..iterations {
        dirty.clear();
        for key in &keys {
            dirty.insert(*key, RetainedNodeDirtyFlags::STATE);
        }
        new_count += std::hint::black_box(dirty.len());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "retained dirty map: fresh {old_time:?}; reused {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_count, new_count);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- drained_retained_snapshot_map_beats_clone_transfer --ignored --nocapture
#[test]
#[ignore = "release-only retained snapshot update microbenchmark"]
fn drained_retained_snapshot_map_beats_clone_transfer() {
    let snapshots = (0..256_u64)
        .map(|index| {
            let mut child_ids = SmallVec::<[NodeId; 8]>::new();
            child_ids.extend((0..6).map(|child| index * 16 + child));
            (
                index,
                RetainedNodeSnapshot {
                    layout: (index as u32, 1, 2, 3, 0, 0, 0, 0, 0, 0),
                    style_hash: index.wrapping_mul(31),
                    attributes_hash: index.wrapping_mul(131),
                    child_ids,
                    state: ElementState {
                        hovered: index % 2 == 0,
                        focused: index % 3 == 0,
                        ..ElementState::default()
                    },
                    render: RenderObjectFingerprint::for_node(&WidgetNode::new("box"), None),
                    last_seen_epoch: 0,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let iterations = 20_000;

    let clone_started = Instant::now();
    let mut clone_total = 0usize;
    for _ in 0..iterations {
        let source = snapshots.clone();
        let mut slots = HashMap::with_capacity(source.len());
        for (&id, snapshot) in &source {
            slots.insert(id, snapshot.clone());
        }
        clone_total += std::hint::black_box(slots.len());
    }
    let clone_time = clone_started.elapsed();

    let move_started = Instant::now();
    let mut move_total = 0usize;
    for _ in 0..iterations {
        let mut source = snapshots.clone();
        let mut slots = HashMap::with_capacity(source.len());
        for (id, snapshot) in source.drain() {
            slots.insert(id, snapshot);
        }
        move_total += std::hint::black_box(slots.len());
    }
    let move_time = move_started.elapsed();

    eprintln!(
        "retained snapshot map transfer: clone {clone_time:?}; drain-move {move_time:?}; ratio {:.1}x; counts={clone_total}/{move_total}",
        clone_time.as_secs_f64() / move_time.as_secs_f64()
    );
    assert_eq!(clone_total, move_total);
    assert!(move_time < clone_time);
}

// cargo test -p mesh-core-shell --release -- direct_narrow_diff_walk_beats_snapshot_map --ignored --nocapture
#[test]
#[ignore = "release-only direct narrow-diff walk microbenchmark"]
fn direct_narrow_diff_walk_beats_snapshot_map() {
    fn map_narrow_script_diff(
        retained: &RetainedWidgetTree,
        root: &WidgetNode,
    ) -> Option<(HashSet<NodeId>, usize)> {
        let mut fresh_snapshots = HashMap::with_capacity(retained.node_keys.len());
        collect_retained_snapshots(root, &mut fresh_snapshots);
        let total = fresh_snapshots.len();
        let mut affected = HashSet::new();
        for (&node_id, fresh) in &fresh_snapshots {
            let previous_key = retained.node_keys.get(&node_id).copied()?;
            let previous = retained.nodes.get(previous_key)?;
            let (flags, _) = previous.diff_flags(fresh);
            if flags.is_empty() {
                continue;
            }
            if flags.contains(RetainedNodeDirtyFlags::CHILDREN) {
                return None;
            }
            let ancestor_only_flags =
                RetainedNodeDirtyFlags::LAYOUT | RetainedNodeDirtyFlags::ATTRIBUTES;
            if !fresh.child_ids.is_empty() && flags.difference(ancestor_only_flags).is_empty() {
                continue;
            }
            affected.insert(node_id);
        }
        Some((affected, total))
    }

    let mut tree = benchmark_plain_tree(2, 9);
    annotate_with_empty_context(&mut tree);
    let mut retained = RetainedWidgetTree::default();
    retained.update(&tree);
    let iterations = 2_000;

    let old_started = Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total ^= std::hint::black_box(map_narrow_script_diff(
            std::hint::black_box(&retained),
            std::hint::black_box(&tree),
        ))
        .map(|(_, total)| total)
        .unwrap_or_default();
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        new_total ^= std::hint::black_box(retained.narrow_script_diff(std::hint::black_box(&tree)))
            .map(|(_, total)| total)
            .unwrap_or_default();
    }
    let new_time = new_started.elapsed();

    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
    eprintln!(
        "narrow diff: temporary snapshot map {old_time:?}; direct slotmap walk {new_time:?}; ratio {:.2}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
}

// cargo test -p mesh-core-shell --release -- hovered_key_set_beats_path_scan --ignored --nocapture
#[test]
#[ignore = "release-only hover membership microbenchmark"]
fn hovered_key_set_beats_path_scan() {
    let hovered_path: Vec<String> = (0..64).map(|index| format!("root/{index}")).collect();
    let keys: Vec<String> = (0..4_096).map(|index| format!("root/{index}")).collect();
    let hovered_keys: HashSet<&str> = hovered_path.iter().map(String::as_str).collect();
    let iterations = 2_000usize;

    let old_started = Instant::now();
    let mut old_matches = 0usize;
    for _ in 0..iterations {
        for key in &keys {
            old_matches +=
                std::hint::black_box(hovered_path.iter().any(|hovered_key| hovered_key == key))
                    as usize;
        }
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_matches = 0usize;
    for _ in 0..iterations {
        for key in &keys {
            new_matches += std::hint::black_box(hovered_keys.contains(key.as_str())) as usize;
        }
    }
    let new_time = new_started.elapsed();

    assert_eq!(old_matches, new_matches);
    assert!(new_time < old_time);
    eprintln!(
        "hovered key membership: path scan {old_time:?}; hash set {new_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
}

// cargo test -p mesh-core-shell --release -- mutable_runtime_key_paths_beat_format_per_child --ignored --nocapture
#[test]
#[ignore = "release-only runtime key path construction microbenchmark"]
fn mutable_runtime_key_paths_beat_format_per_child() {
    fn old_sum_paths(key: String, width: usize, depth: usize) -> usize {
        let mut total = key.len();
        if depth > 0 {
            for index in 0..width {
                total += old_sum_paths(format!("{key}/{index}"), width, depth - 1);
            }
        }
        total
    }

    fn new_sum_paths(key: &mut String, width: usize, depth: usize) -> usize {
        let mut total = key.len();
        if depth > 0 {
            for index in 0..width {
                let previous_len = key.len();
                {
                    use std::fmt::Write as _;
                    let _ = write!(key, "/{index}");
                }
                total += new_sum_paths(key, width, depth - 1);
                key.truncate(previous_len);
            }
        }
        total
    }

    let iterations = 20_000;
    let old_started = Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        old_total += old_sum_paths("root".to_string(), 4, 5);
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let mut key = "root".to_string();
        new_total += new_sum_paths(&mut key, 4, 5);
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "runtime key paths: format-per-child {old_time:?}; mutable path {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- retained_single_fingerprint_pass_beats_separate_render_diff --ignored --nocapture
#[test]
#[ignore = "release-only fused retained/render fingerprint benchmark"]
fn retained_single_fingerprint_pass_beats_separate_render_diff() {
    use mesh_core_render::RenderObjectTree;

    let mut legacy_root = benchmark_plain_tree(4, 5);
    annotate_with_empty_context(&mut legacy_root);
    let mut fused_root = legacy_root.clone();
    let mut legacy_retained = RetainedWidgetTree::default();
    let mut fused_retained = RetainedWidgetTree::default();
    let mut separate_render = RenderObjectTree::default();
    legacy_retained.update_without_render_fingerprints(&legacy_root);
    fused_retained.update(&fused_root);
    separate_render.update(&legacy_root);

    let iterations = 2_000_u64;
    let mut legacy_time = std::time::Duration::ZERO;
    let mut fused_time = std::time::Duration::ZERO;
    let mut legacy_changes = 0usize;
    let mut fused_changes = 0usize;
    for generation in 2..iterations + 2 {
        let content = if generation % 2 == 0 { "b" } else { "a" };
        first_deep_leaf_mut(&mut legacy_root)
            .attributes
            .insert("content".into(), content.into());
        first_deep_leaf_mut(&mut fused_root)
            .attributes
            .insert("content".into(), content.into());

        if generation % 2 == 0 {
            let started = Instant::now();
            legacy_retained.update_without_render_fingerprints(&legacy_root);
            legacy_changes += separate_render
                .update_for_retained_dirty_nodes(
                    &legacy_root,
                    generation,
                    legacy_retained.dirty_node_ids(),
                )
                .text;
            legacy_time += started.elapsed();

            let started = Instant::now();
            fused_retained.update(&fused_root);
            fused_changes += fused_retained.render_dirty().text;
            fused_time += started.elapsed();
        } else {
            let started = Instant::now();
            fused_retained.update(&fused_root);
            fused_changes += fused_retained.render_dirty().text;
            fused_time += started.elapsed();

            let started = Instant::now();
            legacy_retained.update_without_render_fingerprints(&legacy_root);
            legacy_changes += separate_render
                .update_for_retained_dirty_nodes(
                    &legacy_root,
                    generation,
                    legacy_retained.dirty_node_ids(),
                )
                .text;
            legacy_time += started.elapsed();
        }
    }

    assert_eq!(fused_changes, iterations as usize);
    assert_eq!(fused_changes, legacy_changes);
    let speedup = legacy_time.as_secs_f64() / fused_time.as_secs_f64();
    eprintln!(
        "retained/render fingerprints over {iterations} one-node-dirty 1,365-node frames: separate {legacy_time:?}; single pass {fused_time:?}; ratio {speedup:.2}x"
    );
    eprintln!("MESH_PERF metric=retained_single_fingerprint_speedup value={speedup:.6}");
    assert!(
        fused_time * 20 < legacy_time * 19,
        "single retained fingerprint pass should be at least 5% faster"
    );
}

// cargo test -p mesh-core-shell --release -- narrow_script_scope_beats_full_fingerprinting --ignored --nocapture
#[test]
#[ignore = "release-only narrow-script retained-scope benchmark"]
fn narrow_script_scope_beats_full_fingerprinting() {
    let mut tree = benchmark_plain_tree(4, 5);
    annotate_with_empty_context(&mut tree);
    let mut full_tree = tree.clone();
    let mut previous_scoped_tree = tree.clone();
    let mut scoped_tree = tree;

    let mut full = RetainedWidgetTree::default();
    let mut scoped = RetainedWidgetTree::default();
    full.update(&full_tree);
    scoped.update(&scoped_tree);

    let iterations = 2_000;
    let full_started = Instant::now();
    let mut full_total = 0usize;
    for iteration in 0..iterations {
        first_deep_leaf_mut(&mut full_tree)
            .attributes
            .insert("content".into(), format!("value-{}", iteration % 2));
        full_total +=
            std::hint::black_box(full.update(std::hint::black_box(&full_tree))).attributes;
    }
    let full_time = full_started.elapsed();

    let scoped_started = Instant::now();
    let mut scoped_total = 0usize;
    for iteration in 0..iterations {
        let content = format!("value-{}", iteration % 2);
        first_deep_leaf_mut(&mut scoped_tree)
            .attributes
            .insert("content".into(), content.clone());
        let dirty_roots = narrow_script_dirty_roots(
            std::hint::black_box(&previous_scoped_tree),
            std::hint::black_box(&scoped_tree),
        )
        .expect("benchmark structure remains stable");
        scoped_total += std::hint::black_box(
            scoped.update_for_dirty_roots(std::hint::black_box(&scoped_tree), &dirty_roots),
        )
        .attributes;
        first_deep_leaf_mut(&mut previous_scoped_tree)
            .attributes
            .insert("content".into(), content);
    }
    let scoped_time = scoped_started.elapsed();

    assert_eq!(scoped_total, full_total);
    let speedup = full_time.as_secs_f64() / scoped_time.as_secs_f64();
    eprintln!(
        "narrow-script retained fingerprinting over {iterations} one-leaf-dirty 1,365-node frames: full {full_time:?}; direct diff + scoped {scoped_time:?}; ratio {speedup:.2}x"
    );
    eprintln!("MESH_PERF metric=narrow_script_scope_speedup value={speedup:.3}");
    assert!(
        scoped_time * 3 < full_time * 2,
        "direct narrow diff plus scoped fingerprints should be at least 1.5x faster"
    );
}

// cargo test -p mesh-core-shell --release -- direct_dirty_refs_beat_rewalking_for_render_sync --ignored --nocapture
#[test]
#[ignore = "release-only retained-to-render direct-reference benchmark"]
fn direct_dirty_refs_beat_rewalking_for_render_sync() {
    use mesh_core_render::RenderObjectTree;

    let mut traversed_root = benchmark_plain_tree(4, 5);
    annotate_with_empty_context(&mut traversed_root);
    let mut direct_root = traversed_root.clone();
    let dirty_id = first_deep_leaf_mut(&mut traversed_root).id;
    debug_assert_eq!(first_deep_leaf_mut(&mut direct_root).id, dirty_id);
    let dirty_roots = HashSet::from([dirty_id]);

    let mut traversed_retained = RetainedWidgetTree::default();
    let mut direct_retained = RetainedWidgetTree::default();
    traversed_retained.update(&traversed_root);
    direct_retained.update(&direct_root);
    let mut traversed_render = RenderObjectTree::default();
    let mut direct_render = RenderObjectTree::default();
    traversed_render.update_for_retained_generation(&traversed_root, 1);
    direct_render.update_for_retained_generation(&direct_root, 1);

    let iterations = 2_000_u64;
    let mut traversed_time = std::time::Duration::ZERO;
    let mut direct_time = std::time::Duration::ZERO;
    let mut traversed_changes = 0usize;
    let mut direct_changes = 0usize;
    for generation in 2..iterations + 2 {
        let next = if generation % 2 == 0 { "b" } else { "a" };
        first_deep_leaf_mut(&mut traversed_root)
            .attributes
            .insert("content".into(), next.into());
        first_deep_leaf_mut(&mut direct_root)
            .attributes
            .insert("content".into(), next.into());

        if generation % 2 == 0 {
            let started = Instant::now();
            traversed_retained.update_for_dirty_roots(&traversed_root, &dirty_roots);
            traversed_changes += traversed_render
                .update_for_retained_dirty_nodes(
                    &traversed_root,
                    generation,
                    traversed_retained.dirty_node_ids(),
                )
                .text;
            traversed_time += started.elapsed();

            let started = Instant::now();
            let (_, dirty_refs) =
                direct_retained.update_for_dirty_roots_collect(&direct_root, &dirty_roots);
            direct_changes += direct_render
                .update_for_retained_dirty_node_refs(
                    &direct_root,
                    generation,
                    dirty_refs.as_deref().expect("scoped refs"),
                )
                .text;
            direct_time += started.elapsed();
        } else {
            let started = Instant::now();
            let (_, dirty_refs) =
                direct_retained.update_for_dirty_roots_collect(&direct_root, &dirty_roots);
            direct_changes += direct_render
                .update_for_retained_dirty_node_refs(
                    &direct_root,
                    generation,
                    dirty_refs.as_deref().expect("scoped refs"),
                )
                .text;
            direct_time += started.elapsed();

            let started = Instant::now();
            traversed_retained.update_for_dirty_roots(&traversed_root, &dirty_roots);
            traversed_changes += traversed_render
                .update_for_retained_dirty_nodes(
                    &traversed_root,
                    generation,
                    traversed_retained.dirty_node_ids(),
                )
                .text;
            traversed_time += started.elapsed();
        }
    }

    let speedup = traversed_time.as_secs_f64() / direct_time.as_secs_f64();
    assert_eq!(direct_changes, iterations as usize);
    assert_eq!(direct_changes, traversed_changes);
    eprintln!(
        "retained diff plus render sync over {iterations} one-node-dirty 1,365-node frames: tree walk {traversed_time:?}; direct refs {direct_time:?}; ratio {speedup:.2}x"
    );
    eprintln!("MESH_PERF metric=retained_render_direct_scope_speedup value={speedup:.3}");
    assert!(
        direct_time * 10 < traversed_time * 9,
        "direct dirty references should improve combined retained/render sync by at least 10%"
    );
}

// cargo test -p mesh-core-shell --release -- scoped_retained_update_beats_full_fingerprinting --ignored --nocapture
#[test]
#[ignore = "release-only scoped retained fingerprint benchmark"]
fn scoped_retained_update_beats_full_fingerprinting() {
    let mut tree = benchmark_plain_tree(4, 5);
    annotate_with_empty_context(&mut tree);
    let mut full_tree = tree.clone();
    let mut scoped_tree = tree;
    let dirty_id = first_deep_leaf_mut(&mut scoped_tree).id;
    assert_eq!(first_deep_leaf_mut(&mut full_tree).id, dirty_id);
    let dirty_roots = HashSet::from([dirty_id]);

    let mut full = RetainedWidgetTree::default();
    let mut scoped = RetainedWidgetTree::default();
    full.update(&full_tree);
    scoped.update(&scoped_tree);

    let iterations = 2_000;
    let full_started = Instant::now();
    let mut full_total = 0usize;
    for iteration in 0..iterations {
        first_deep_leaf_mut(&mut full_tree)
            .computed_style
            .background_color = if iteration % 2 == 0 {
            Color::BLACK
        } else {
            Color::WHITE
        };
        full_total += std::hint::black_box(full.update(std::hint::black_box(&full_tree))).style;
    }
    let full_time = full_started.elapsed();

    let scoped_started = Instant::now();
    let mut scoped_total = 0usize;
    for iteration in 0..iterations {
        first_deep_leaf_mut(&mut scoped_tree)
            .computed_style
            .background_color = if iteration % 2 == 0 {
            Color::BLACK
        } else {
            Color::WHITE
        };
        scoped_total += std::hint::black_box(
            scoped.update_for_dirty_roots(std::hint::black_box(&scoped_tree), &dirty_roots),
        )
        .style;
    }
    let scoped_time = scoped_started.elapsed();

    assert_eq!(scoped_total, full_total);
    assert_eq!(scoped.dirty_node_ids(), full.dirty_node_ids());
    let speedup = full_time.as_secs_f64() / scoped_time.as_secs_f64();
    eprintln!(
        "retained fingerprint scope over {iterations} one-leaf-dirty 1,365-node frames: full {full_time:?}; scoped {scoped_time:?}; ratio {speedup:.2}x"
    );
    eprintln!("MESH_PERF metric=retained_scope_speedup value={speedup:.3}");
    assert!(
        scoped_time * 2 < full_time,
        "scoped retained fingerprinting should be at least 2x faster"
    );
}

// cargo test -p mesh-core-shell --release -- scroll_layout_scope_beats_full_fingerprinting --ignored --nocapture
#[test]
#[ignore = "release-only scroll retained fingerprint benchmark"]
fn scroll_layout_scope_beats_full_fingerprinting() {
    let mut full_tree = benchmark_plain_tree(4, 5);
    annotate_with_empty_context(&mut full_tree);
    let mut scoped_tree = full_tree.clone();
    let mut full = RetainedWidgetTree::default();
    let mut scoped = RetainedWidgetTree::default();
    full.update(&full_tree);
    scoped.update(&scoped_tree);
    let iterations = 2_000_u64;
    let empty_roots = HashSet::new();

    let full_started = Instant::now();
    let mut full_layout_changes = 0usize;
    for generation in 0..iterations {
        first_deep_leaf_mut(&mut full_tree).scroll_metrics =
            Some(mesh_core_elements::WidgetScrollMetrics {
                y: (generation % 2) as f32,
                max_y: 128.0,
                content_height: 256.0,
                ..Default::default()
            });
        full_layout_changes += std::hint::black_box(full.update(&full_tree).layout);
    }
    let full_time = full_started.elapsed();

    let scoped_started = Instant::now();
    let mut scoped_layout_changes = 0usize;
    for generation in 0..iterations {
        first_deep_leaf_mut(&mut scoped_tree).scroll_metrics =
            Some(mesh_core_elements::WidgetScrollMetrics {
                y: (generation % 2) as f32,
                max_y: 128.0,
                content_height: 256.0,
                ..Default::default()
            });
        scoped_layout_changes += std::hint::black_box(
            scoped
                .update_for_dirty_roots(&scoped_tree, &empty_roots)
                .layout,
        );
    }
    let scoped_time = scoped_started.elapsed();
    let speedup = full_time.as_secs_f64() / scoped_time.as_secs_f64();

    assert_eq!(scoped_layout_changes, full_layout_changes);
    assert_eq!(scoped_layout_changes, iterations as usize);
    eprintln!(
        "scroll retained fingerprinting over {iterations} one-node-scrolled 1,365-node frames: full {full_time:?}; layout-scoped {scoped_time:?}; ratio {speedup:.2}x"
    );
    eprintln!("MESH_PERF metric=scroll_retained_scope_speedup value={speedup:.3}");
    assert!(
        scoped_time * 2 < full_time,
        "layout-scoped scroll fingerprinting should be at least 2x faster"
    );
}

// cargo test -p mesh-core-shell --release -- geometry_only_snapshot_beats_full_fingerprinting --ignored --nocapture
#[test]
#[ignore = "release-only geometry-only retained fingerprint benchmark"]
fn geometry_only_snapshot_beats_full_fingerprinting() {
    let mut nodes = (0..256_u64)
        .map(|index| {
            let mut node = WidgetNode::new("row");
            node.id = index + 1;
            node.attributes
                .insert("class".into(), "resource-row".into());
            node.attributes
                .insert("data-resource-id".into(), format!("resource-{index}"));
            node.attributes
                .insert("aria-label".into(), format!("Resource {index}"));
            node.children = (0..4)
                .map(|child| {
                    let mut child_node = WidgetNode::new("text");
                    child_node.id = 10_000 + index * 4 + child;
                    child_node
                })
                .collect();
            node
        })
        .collect::<Vec<_>>();
    let previous = nodes.iter().map(retained_snapshot).collect::<Vec<_>>();
    for (index, node) in nodes.iter_mut().enumerate() {
        node.layout.y = index as f32 + 1.0;
    }

    let iterations = 2_000;
    let full_started = Instant::now();
    let mut full_layout_changes = 0usize;
    for _ in 0..iterations {
        for (node, previous) in nodes.iter().zip(&previous) {
            let next = retained_snapshot_with_render(node, previous.render.clone(), None);
            full_layout_changes += usize::from(
                previous
                    .diff_flags(std::hint::black_box(&next))
                    .0
                    .contains(RetainedNodeDirtyFlags::LAYOUT),
            );
        }
    }
    let full_time = full_started.elapsed();

    let geometry_started = Instant::now();
    let mut geometry_layout_changes = 0usize;
    for _ in 0..iterations {
        for (node, previous) in nodes.iter().zip(&previous) {
            let next = retained_snapshot_with_render(
                node,
                previous.render.clone(),
                Some(std::hint::black_box(previous)),
            );
            geometry_layout_changes += usize::from(
                previous
                    .diff_flags(std::hint::black_box(&next))
                    .0
                    .contains(RetainedNodeDirtyFlags::LAYOUT),
            );
        }
    }
    let geometry_time = geometry_started.elapsed();
    let speedup = full_time.as_secs_f64() / geometry_time.as_secs_f64();

    assert_eq!(geometry_layout_changes, full_layout_changes);
    assert_eq!(geometry_layout_changes, iterations * nodes.len());
    eprintln!(
        "retained snapshots over {iterations} geometry-only frames with 256 moved rows: full {full_time:?}; split {geometry_time:?}; ratio {speedup:.2}x"
    );
    eprintln!("MESH_PERF metric=geometry_only_fingerprint_speedup value={speedup:.3}");
    assert!(
        geometry_time * 5 < full_time * 2,
        "split geometry fingerprints should be at least 2.5x faster"
    );
}

// cargo test -p mesh-core-shell --release -- animation_retained_scope_beats_full_fingerprinting --ignored --nocapture
#[test]
#[ignore = "release-only animation retained fingerprint benchmark"]
fn animation_retained_scope_beats_full_fingerprinting() {
    let mut tree = benchmark_plain_tree(4, 5);
    annotate_with_empty_context(&mut tree);
    let mut full_tree = tree.clone();
    let mut scoped_tree = tree;
    let dirty_roots: HashSet<_> = scoped_tree
        .children
        .iter_mut()
        .map(|child| first_deep_leaf_mut(child).id)
        .collect();
    let full_dirty_roots: HashSet<_> = full_tree
        .children
        .iter_mut()
        .map(|child| first_deep_leaf_mut(child).id)
        .collect();
    assert_eq!(dirty_roots, full_dirty_roots);

    let mut full = RetainedWidgetTree::default();
    let mut scoped = RetainedWidgetTree::default();
    full.update(&full_tree);
    scoped.update(&scoped_tree);

    let iterations = 2_000;
    let full_started = Instant::now();
    let mut full_total = 0usize;
    for iteration in 0..iterations {
        for child in &mut full_tree.children {
            first_deep_leaf_mut(child).computed_style.opacity =
                if iteration % 2 == 0 { 0.25 } else { 0.75 };
        }
        full_total += std::hint::black_box(full.update(std::hint::black_box(&full_tree))).style;
    }
    let full_time = full_started.elapsed();

    let scoped_started = Instant::now();
    let mut scoped_total = 0usize;
    for iteration in 0..iterations {
        for child in &mut scoped_tree.children {
            first_deep_leaf_mut(child).computed_style.opacity =
                if iteration % 2 == 0 { 0.25 } else { 0.75 };
        }
        scoped_total += std::hint::black_box(
            scoped.update_for_dirty_roots(std::hint::black_box(&scoped_tree), &dirty_roots),
        )
        .style;
    }
    let scoped_time = scoped_started.elapsed();

    assert_eq!(scoped_total, full_total);
    assert_eq!(scoped.dirty_node_ids(), full.dirty_node_ids());
    let speedup = full_time.as_secs_f64() / scoped_time.as_secs_f64();
    eprintln!(
        "retained animation fingerprinting over {iterations} four-node-dirty 1,365-node frames: full {full_time:?}; scoped {scoped_time:?}; ratio {speedup:.2}x"
    );
    eprintln!("MESH_PERF metric=animation_retained_scope_speedup value={speedup:.3}");
    assert!(
        scoped_time * 2 < full_time,
        "scoped animation fingerprinting should be at least 2x faster"
    );
}

// cargo test -p mesh-core-shell --release -- direct_retained_update_beats_snapshot_map --ignored --nocapture
#[test]
#[ignore = "release-only retained update traversal benchmark"]
fn direct_retained_update_beats_snapshot_map() {
    let mut tree = benchmark_plain_tree(4, 5);
    annotate_with_empty_context(&mut tree);
    let mut map_tree = tree.clone();
    let mut direct_tree = tree;

    let mut map_retained = RetainedWidgetTree::default();
    let mut direct_retained = RetainedWidgetTree::default();
    let mut snapshot_scratch = HashMap::new();
    update_via_snapshot_map(&mut map_retained, &map_tree, &mut snapshot_scratch);
    direct_retained.update(&direct_tree);

    let iterations = 2_000;
    let map_started = Instant::now();
    let mut map_total = 0usize;
    for iteration in 0..iterations {
        map_tree.children[0].computed_style.background_color = if iteration % 2 == 0 {
            Color::BLACK
        } else {
            Color::WHITE
        };
        map_total += std::hint::black_box(update_via_snapshot_map(
            &mut map_retained,
            std::hint::black_box(&map_tree),
            &mut snapshot_scratch,
        ))
        .style;
    }
    let map_time = map_started.elapsed();

    let direct_started = Instant::now();
    let mut direct_total = 0usize;
    for iteration in 0..iterations {
        direct_tree.children[0].computed_style.background_color = if iteration % 2 == 0 {
            Color::BLACK
        } else {
            Color::WHITE
        };
        direct_total +=
            std::hint::black_box(direct_retained.update(std::hint::black_box(&direct_tree))).style;
    }
    let direct_time = direct_started.elapsed();

    assert_eq!(map_total, direct_total);
    assert_eq!(
        map_retained.node_keys.len(),
        direct_retained.node_keys.len()
    );
    let speedup = map_time.as_secs_f64() / direct_time.as_secs_f64();
    eprintln!(
        "retained update over {iterations} one-node-dirty 1,365-node frames: snapshot map {map_time:?}; direct retained slots {direct_time:?}; ratio {:.2}x",
        speedup
    );
    eprintln!("MESH_PERF metric=retained_update_speedup value={speedup:.3}");
    assert!(
        direct_time * 10 < map_time * 9,
        "direct retained update should beat snapshot-map staging by at least 10%"
    );
}

// cargo test -p mesh-core-shell --release -- direct_dirty_node_id_membership_beats_slot_indirection --ignored --nocapture
#[test]
#[ignore = "release-only retained dirty-node membership microbenchmark"]
fn direct_dirty_node_id_membership_beats_slot_indirection() {
    fn collect_node_ids(node: &WidgetNode, ids: &mut Vec<NodeId>) {
        ids.push(node.id);
        for child in &node.children {
            collect_node_ids(child, ids);
        }
    }

    let mut tree = benchmark_plain_tree(4, 5);
    annotate_with_empty_context(&mut tree);
    let mut retained = RetainedWidgetTree::default();
    retained.update(&tree);
    tree.children[0].computed_style.background_color = Color::BLACK;
    retained.update(&tree);

    let mut node_ids = Vec::new();
    collect_node_ids(&tree, &mut node_ids);
    assert_eq!(retained.dirty_node_ids().len(), 1);

    let iterations = 10_000;
    let indirect_started = Instant::now();
    let mut indirect_total = 0usize;
    for _ in 0..iterations {
        for &node_id in &node_ids {
            indirect_total += usize::from(std::hint::black_box(retained.is_node_dirty(node_id)));
        }
    }
    let indirect_time = indirect_started.elapsed();

    let direct_started = Instant::now();
    let mut direct_total = 0usize;
    for _ in 0..iterations {
        for &node_id in &node_ids {
            direct_total += usize::from(std::hint::black_box(
                retained.dirty_node_ids().contains(&node_id),
            ));
        }
    }
    let direct_time = direct_started.elapsed();

    assert_eq!(direct_total, indirect_total);
    eprintln!(
        "retained dirty membership: slot-indirect {indirect_time:?}; direct NodeId set {direct_time:?}; ratio {:.1}x",
        indirect_time.as_secs_f64() / direct_time.as_secs_f64()
    );
    assert!(
        direct_time * 10 < indirect_time * 9,
        "direct dirty NodeId membership should beat slot-indirect lookups by at least 10%"
    );
}

// cargo test -p mesh-core-shell --release -- fused_runtime_overflow_annotation_beats_two_tree_walks --ignored --nocapture
#[test]
#[ignore = "release-only fused finalize annotation benchmark"]
fn fused_runtime_overflow_annotation_beats_two_tree_walks() {
    fn annotate(
        tree: &mut WidgetNode,
        scroll_offsets: &mut HashMap<NodeId, ScrollOffsetState>,
        fused: bool,
    ) {
        let input_values = HashMap::new();
        let mut slider_values = HashMap::new();
        let mut slider_script_values = HashMap::new();
        let checked_values = HashMap::new();
        let mut context = RuntimeAnnotationContext::new(
            None,
            None,
            &[],
            None,
            None,
            &input_values,
            &mut slider_values,
            &mut slider_script_values,
            &checked_values,
            scroll_offsets,
        );
        if fused {
            annotate_runtime_and_overflow_tree(tree, "root".to_string(), &mut context);
        } else {
            annotate_runtime_tree(tree, "root".to_string(), &mut context);
            drop(context);
            mesh_core_interaction::annotate_overflow_tree(tree, scroll_offsets);
        }
    }

    let tree = benchmark_plain_tree(4, 5);
    let mut separate_tree = tree.clone();
    let mut fused_tree = tree.clone();
    let mut separate_offsets = HashMap::new();
    let mut fused_offsets = HashMap::new();
    annotate(&mut separate_tree, &mut separate_offsets, false);
    annotate(&mut fused_tree, &mut fused_offsets, true);
    assert_eq!(format!("{fused_tree:?}"), format!("{separate_tree:?}"));
    assert_eq!(fused_offsets.len(), separate_offsets.len());
    for (key, fused_offset) in &fused_offsets {
        let separate_offset = separate_offsets.get(key).expect("matching scroll key");
        assert_eq!(fused_offset.x.to_bits(), separate_offset.x.to_bits());
        assert_eq!(fused_offset.y.to_bits(), separate_offset.y.to_bits());
    }

    let iterations = 2_000;
    let separate_started = Instant::now();
    for _ in 0..iterations {
        annotate(
            std::hint::black_box(&mut separate_tree),
            std::hint::black_box(&mut separate_offsets),
            false,
        );
    }
    let separate_time = separate_started.elapsed();

    let fused_started = Instant::now();
    for _ in 0..iterations {
        annotate(
            std::hint::black_box(&mut fused_tree),
            std::hint::black_box(&mut fused_offsets),
            true,
        );
    }
    let fused_time = fused_started.elapsed();

    assert_eq!(format!("{fused_tree:?}"), format!("{separate_tree:?}"));
    eprintln!(
        "runtime + overflow annotation: separate {separate_time:?}; fused {fused_time:?}; ratio {:.1}x",
        separate_time.as_secs_f64() / fused_time.as_secs_f64()
    );
    assert!(
        fused_time * 10 < separate_time * 9,
        "fused runtime/overflow annotation should beat separate walks by at least 10%"
    );
}

// cargo test -p mesh-core-shell --release -- retained_analysis_result_capacity_beats_growth --ignored --nocapture
#[test]
#[ignore = "release-only retained analysis result allocation microbenchmark"]
fn retained_analysis_result_capacity_beats_growth() {
    let ids: Vec<NodeId> = (0..4_096).collect();
    let iterations = 20_000usize;

    let old_started = Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let mut ids_set = HashSet::new();
        for &id in &ids {
            ids_set.insert(id);
        }
        old_total += std::hint::black_box(ids_set.len());
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let mut ids_set = HashSet::with_capacity(ids.len().min(256));
        for &id in &ids {
            ids_set.insert(id);
        }
        new_total += std::hint::black_box(ids_set.len());
    }
    let new_time = new_started.elapsed();

    assert_eq!(old_total, new_total);
    eprintln!(
        "retained analysis result set: growth {old_time:?}; reserved {new_time:?}; ratio {:.2}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-shell --release -- service_dependency_borrowed_lookup_beats_tuple_allocation --ignored --nocapture
#[test]
#[ignore = "release-only service dependency lookup microbenchmark"]
fn service_dependency_borrowed_lookup_beats_tuple_allocation() {
    let mut node = WidgetNode::new("text");
    for index in 0..64 {
        node.service_field_reads
            .push(("audio".into(), format!("field_{index}")));
    }
    let mut root = WidgetNode::new("column");
    root.children.push(node);
    let deps = NodeServiceFieldDependencies::build(&root);
    let fields: Vec<String> = (0..64).map(|index| format!("field_{index}")).collect();
    let old_reverse: HashMap<(String, String), HashSet<NodeId>> = fields
        .iter()
        .map(|field| {
            (
                ("audio".into(), field.clone()),
                HashSet::from([root.children[0].id]),
            )
        })
        .collect();
    let iterations = 1_000_000usize;

    let old_started = Instant::now();
    let mut old_total = 0usize;
    for index in 0..iterations {
        let key = ("audio".to_string(), fields[index % 64].clone());
        old_total += std::hint::black_box(old_reverse.get(&key).unwrap().len());
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    let mut new_total = 0usize;
    for index in 0..iterations {
        new_total +=
            std::hint::black_box(deps.nodes_reading_field("audio", &fields[index % 64]).len());
    }
    let new_time = new_started.elapsed();

    assert_eq!(old_total, new_total);
    eprintln!(
        "service dependency lookup: tuple allocation {old_time:?}; borrowed nested map {new_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}
