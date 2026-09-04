use super::child::{child_surface_id, child_surface_paint_cache_matches};
use mesh_core_elements::style::Color;
#[cfg(feature = "allocation-profiling")]
use mesh_core_elements::{NodeId, WidgetNode};
use mesh_core_render::PixelBuffer;
#[cfg(feature = "allocation-profiling")]
use mesh_core_render::{
    DisplayListRepaintPolicy, RenderObjectDirtySummary, RetainedDisplayList,
    paint_selected_display_list_for_module_with_profiling_metrics,
};
use smallvec::SmallVec;
use std::collections::HashSet;
use std::hint::black_box;
#[cfg(feature = "allocation-profiling")]
use std::time::Duration;
use std::time::Instant;

#[test]
fn child_paint_cache_requires_every_raster_input_to_match() {
    let matches = |generation, exiting, scale, offset| {
        child_surface_paint_cache_matches(
            generation,
            Some(7),
            exiting,
            Some(false),
            7,
            Some(7),
            scale,
            Some(1.0_f32.to_bits()),
            offset,
            Some((4, 4)),
        )
    };
    assert!(matches(Some(7), false, 1.0_f32.to_bits(), (4, 4)));
    assert!(!matches(None, false, 1.0_f32.to_bits(), (4, 4)));
    assert!(!matches(Some(8), false, 1.0_f32.to_bits(), (4, 4)));
    assert!(!matches(Some(7), true, 1.0_f32.to_bits(), (4, 4)));
    assert!(!matches(Some(7), false, 2.0_f32.to_bits(), (4, 4)));
    assert!(!matches(Some(7), false, 1.0_f32.to_bits(), (8, 4)));
    assert!(!child_surface_paint_cache_matches(
        Some(7),
        Some(7),
        false,
        Some(false),
        8,
        Some(7),
        1.0_f32.to_bits(),
        Some(1.0_f32.to_bits()),
        (4, 4),
        Some((4, 4)),
    ));
}

// cargo test -p mesh-core-shell --release -- cached_child_paint_generation_beats_eager_buffer_clear --ignored --nocapture
#[test]
#[ignore = "release-only stable child-surface paint benchmark"]
fn cached_child_paint_generation_beats_eager_buffer_clear() {
    let iterations = 10_000;
    let mut buffer = PixelBuffer::new(160, 90);

    let eager_started = Instant::now();
    for _ in 0..iterations {
        black_box(&mut buffer).clear(Color::TRANSPARENT);
    }
    let eager_time = eager_started.elapsed();

    let cached_started = Instant::now();
    let mut cache_hits = 0usize;
    for _ in 0..iterations {
        cache_hits += usize::from(child_surface_paint_cache_matches(
            black_box(Some(7)),
            black_box(Some(7)),
            black_box(false),
            black_box(Some(false)),
            black_box(7),
            black_box(Some(7)),
            black_box(1.0_f32.to_bits()),
            black_box(Some(1.0_f32.to_bits())),
            black_box((4, 4)),
            black_box(Some((4, 4))),
        ));
    }
    let cached_time = cached_started.elapsed();

    eprintln!(
        "stable child paint: eager clear {eager_time:?}; generation cache {cached_time:?}; ratio {:.1}x; hits={cache_hits}",
        eager_time.as_secs_f64() / cached_time.as_secs_f64()
    );
    assert_eq!(cache_hits, iterations);
    assert!(cached_time * 10 < eager_time);
}

// cargo test -p mesh-core-shell --release -- cached_child_surface_id_beats_reencoding --ignored --nocapture
#[test]
#[ignore = "release-only child-surface id microbenchmark"]
fn cached_child_surface_id_beats_reencoding() {
    let parent = "@mesh/navigation-bar";
    let key = "root/0/2/1/5/language-popover";
    let cached = child_surface_id(parent, key);
    let iterations = 100_000;

    let encode_started = Instant::now();
    for _ in 0..iterations {
        black_box(child_surface_id(black_box(parent), black_box(key)));
    }
    let encode = encode_started.elapsed();

    let clone_started = Instant::now();
    for _ in 0..iterations {
        black_box(black_box(&cached).clone());
    }
    let clone = clone_started.elapsed();

    eprintln!(
        "child id re-encode: {encode:?}; cached clone: {clone:?}; ratio: {:.1}x",
        encode.as_secs_f64() / clone.as_secs_f64()
    );
    assert!(clone < encode);
}

// cargo test -p mesh-core-shell --release -- render_component_id_borrow_beats_extra_clone --ignored --nocapture
#[test]
#[ignore = "release-only render component id allocation microbenchmark"]
fn render_component_id_borrow_beats_extra_clone() {
    let surface_id = "@mesh/navigation-bar".to_string();
    let iterations = 2_000_000;

    let clone_started = Instant::now();
    let mut clone_len = 0usize;
    for _ in 0..iterations {
        let component_id = black_box(&surface_id).clone();
        clone_len += black_box(component_id.len());
    }
    let clone_time = clone_started.elapsed();

    let borrow_started = Instant::now();
    let mut borrow_len = 0usize;
    for _ in 0..iterations {
        let component_id = black_box(&surface_id).as_str();
        borrow_len += black_box(component_id.len());
    }
    let borrow_time = borrow_started.elapsed();

    eprintln!(
        "render component id: extra clone {clone_time:?}; borrow existing surface id {borrow_time:?}; ratio {:.1}x; lens={clone_len}/{borrow_len}",
        clone_time.as_secs_f64() / borrow_time.as_secs_f64()
    );
    assert_eq!(clone_len, borrow_len);
    assert!(borrow_time < clone_time);
}

// cargo test -p mesh-core-shell --release -- requested_child_keys_smallvec_beats_hashset_for_popovers --ignored --nocapture
#[test]
#[ignore = "release-only child requested-key membership microbenchmark"]
fn requested_child_keys_smallvec_beats_hashset_for_popovers() {
    let requested = [
        "root/0/language-popover",
        "root/1/theme-popover",
        "root/2/audio-popover",
    ];
    let retained = [
        "root/0/language-popover".to_string(),
        "root/1/theme-popover".to_string(),
        "root/4/stale-popover".to_string(),
    ];
    let iterations = 500_000;

    let hash_started = Instant::now();
    let mut hash_count = 0usize;
    for _ in 0..iterations {
        let requested_keys: HashSet<&str> = requested.iter().copied().collect();
        hash_count += retained
            .iter()
            .filter(|key| requested_keys.contains(key.as_str()))
            .count();
    }
    let hash_time = hash_started.elapsed();

    let small_started = Instant::now();
    let mut small_count = 0usize;
    for _ in 0..iterations {
        let requested_keys: SmallVec<[&str; 4]> = requested.iter().copied().collect();
        small_count += retained
            .iter()
            .filter(|key| requested_keys.contains(&key.as_str()))
            .count();
    }
    let small_time = small_started.elapsed();

    eprintln!(
        "requested child keys: HashSet {hash_time:?}; SmallVec {small_time:?}; ratio {:.1}x; counts={hash_count}/{small_count}",
        hash_time.as_secs_f64() / small_time.as_secs_f64()
    );
    assert_eq!(hash_count, small_count);
    assert!(small_time < hash_time);
}

// cargo test -p mesh-core-shell --release -- closing_child_keys_borrowed_compare_beats_owned_hashset --ignored --nocapture
#[test]
#[ignore = "release-only child closing-key allocation microbenchmark"]
fn closing_child_keys_borrowed_compare_beats_owned_hashset() {
    let closing = [
        "root/0/language-popover".to_string(),
        "root/1/theme-popover".to_string(),
        "root/2/audio-popover".to_string(),
    ];
    let existing: HashSet<String> = closing.iter().cloned().collect();
    let iterations = 500_000;

    let hash_started = Instant::now();
    let mut hash_count = 0usize;
    for _ in 0..iterations {
        let candidate: HashSet<String> = closing.iter().map(|key| black_box(key).clone()).collect();
        if candidate == existing {
            hash_count = hash_count.wrapping_add(candidate.len());
        }
    }
    let hash_time = hash_started.elapsed();

    let borrowed_started = Instant::now();
    let mut borrowed_count = 0usize;
    for _ in 0..iterations {
        let candidate: SmallVec<[&str; 4]> =
            closing.iter().map(|key| black_box(key.as_str())).collect();
        if existing.len() == candidate.len() && candidate.iter().all(|key| existing.contains(*key))
        {
            borrowed_count = borrowed_count.wrapping_add(candidate.len());
        }
    }
    let borrowed_time = borrowed_started.elapsed();

    eprintln!(
        "closing child keys: owned HashSet {hash_time:?}; borrowed SmallVec compare {borrowed_time:?}; ratio {:.1}x; counts={hash_count}/{borrowed_count}",
        hash_time.as_secs_f64() / borrowed_time.as_secs_f64()
    );
    assert_eq!(hash_count, borrowed_count);
    assert!(borrowed_time < hash_time);
}

// cargo test -p mesh-core-shell --release -- child_reconcile_borrowed_key_check_beats_clone_per_child --ignored --nocapture
#[test]
#[ignore = "release-only child reconcile key microbenchmark"]
fn child_reconcile_borrowed_key_check_beats_clone_per_child() {
    let children: Vec<String> = (0..16)
        .map(|index| format!("root/{index}/popover"))
        .collect();
    let requested: SmallVec<[&str; 4]> = [
        "root/0/popover",
        "root/4/popover",
        "root/8/popover",
        "root/12/popover",
    ]
    .into_iter()
    .collect();
    let iterations = 500_000usize;

    let clone_started = Instant::now();
    let mut clone_count = 0usize;
    for _ in 0..iterations {
        for child in &children {
            let node_key = black_box(child).clone();
            if requested.contains(&node_key.as_str()) {
                clone_count += 1;
            }
        }
    }
    let clone_time = clone_started.elapsed();

    let borrowed_started = Instant::now();
    let mut borrowed_count = 0usize;
    for _ in 0..iterations {
        for child in &children {
            if requested.contains(&black_box(child).as_str()) {
                borrowed_count += 1;
            }
        }
    }
    let borrowed_time = borrowed_started.elapsed();

    eprintln!(
        "child reconcile key check: clone {clone_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; counts={clone_count}/{borrowed_count}",
        clone_time.as_secs_f64() / borrowed_time.as_secs_f64()
    );
    assert_eq!(clone_count, borrowed_count);
    assert!(borrowed_time < clone_time);
}

// cargo test -p mesh-core-shell --features allocation-profiling --release -- allocation_profiler_retained_render_overhead --ignored --nocapture
#[test]
#[cfg(feature = "allocation-profiling")]
#[ignore = "release-only retained-render allocation-profiler overhead benchmark"]
fn allocation_profiler_retained_render_overhead() {
    const NODE_COUNT: usize = 1_026;
    const GROUP_COUNT: usize = 5;
    const ITEMS_PER_GROUP: usize = 204;
    const MEASURED_FRAMES: usize = 120;
    const WARMUP_FRAMES: usize = 12;
    const DIRTY_FRAME_INTERVAL: usize = 6;
    const SAMPLES: usize = 5;
    const SURFACE_WIDTH: u32 = 1_200;
    const SURFACE_HEIGHT: u32 = 2_800;

    let mut ratios = Vec::with_capacity(SAMPLES);
    let mut tracked_elapsed_ns = Vec::with_capacity(SAMPLES);
    let mut suspended_elapsed_ns = Vec::with_capacity(SAMPLES);
    let mut tracked_allocations = mesh_core_debug::allocation::AllocationCounters::default();
    let mut tracked_workload = None;
    let mut untracked_workload = None;
    for sample in 0..SAMPLES {
        let (tracked, untracked) = if sample % 2 == 0 {
            let tracked = run_retained_render_mode::<NODE_COUNT, GROUP_COUNT, ITEMS_PER_GROUP>(
                true,
                WARMUP_FRAMES,
                MEASURED_FRAMES,
                DIRTY_FRAME_INTERVAL,
                SURFACE_WIDTH,
                SURFACE_HEIGHT,
            );
            let untracked = run_retained_render_mode::<NODE_COUNT, GROUP_COUNT, ITEMS_PER_GROUP>(
                false,
                WARMUP_FRAMES,
                MEASURED_FRAMES,
                DIRTY_FRAME_INTERVAL,
                SURFACE_WIDTH,
                SURFACE_HEIGHT,
            );
            (tracked, untracked)
        } else {
            let untracked = run_retained_render_mode::<NODE_COUNT, GROUP_COUNT, ITEMS_PER_GROUP>(
                false,
                WARMUP_FRAMES,
                MEASURED_FRAMES,
                DIRTY_FRAME_INTERVAL,
                SURFACE_WIDTH,
                SURFACE_HEIGHT,
            );
            let tracked = run_retained_render_mode::<NODE_COUNT, GROUP_COUNT, ITEMS_PER_GROUP>(
                true,
                WARMUP_FRAMES,
                MEASURED_FRAMES,
                DIRTY_FRAME_INTERVAL,
                SURFACE_WIDTH,
                SURFACE_HEIGHT,
            );
            (tracked, untracked)
        };
        let tracked_workload_sample = tracked.0;
        let untracked_workload_sample = untracked.0;
        tracked_workload = Some(tracked_workload_sample);
        untracked_workload = Some(untracked_workload_sample);
        tracked_allocations = tracked.1;
        tracked_elapsed_ns.push(tracked_workload_sample.elapsed.as_nanos());
        suspended_elapsed_ns.push(untracked_workload_sample.elapsed.as_nanos());
        assert_eq!(
            untracked.1,
            mesh_core_debug::allocation::AllocationCounters::default()
        );
        assert_eq!(tracked_workload_sample.frames, MEASURED_FRAMES);
        assert_eq!(untracked_workload_sample.frames, MEASURED_FRAMES);
        assert_eq!(
            tracked_workload_sample.dirty_frames,
            untracked_workload_sample.dirty_frames
        );
        assert_eq!(
            tracked_workload_sample.checksum,
            untracked_workload_sample.checksum
        );
        assert!(tracked_workload_sample.rebuilt_entries > 0);
        assert!(tracked_workload_sample.retained_frames > 0);
        assert!(tracked_workload_sample.selected_commands > 0);
        assert!(tracked_allocations.allocation_count > 0);
        ratios.push(
            tracked_workload_sample.elapsed.as_secs_f64()
                / untracked_workload_sample.elapsed.as_secs_f64(),
        );
    }
    let tracked_workload = tracked_workload.expect("benchmark has tracked workload samples");
    let untracked_workload = untracked_workload.expect("benchmark has suspended workload samples");
    assert_eq!(untracked_workload.frames, MEASURED_FRAMES);
    ratios.sort_unstable_by(f64::total_cmp);
    let minimum = *ratios.first().expect("benchmark has samples");
    let median = ratios[ratios.len() / 2];
    let maximum = *ratios.last().expect("benchmark has samples");
    tracked_elapsed_ns.sort_unstable();
    suspended_elapsed_ns.sort_unstable();
    let tracked_min_ns = *tracked_elapsed_ns
        .first()
        .expect("benchmark has tracked timings");
    let tracked_median_ns = tracked_elapsed_ns[tracked_elapsed_ns.len() / 2];
    let tracked_max_ns = *tracked_elapsed_ns
        .last()
        .expect("benchmark has tracked timings");
    let suspended_min_ns = *suspended_elapsed_ns
        .first()
        .expect("benchmark has suspended timings");
    let suspended_median_ns = suspended_elapsed_ns[suspended_elapsed_ns.len() / 2];
    let suspended_max_ns = *suspended_elapsed_ns
        .last()
        .expect("benchmark has suspended timings");

    eprintln!(
        "MESH_PERF metric=allocation_profiler_retained_render_overhead value={median:.3} workload_nodes={NODE_COUNT} groups={GROUP_COUNT} items_per_group={ITEMS_PER_GROUP} measured_frames={MEASURED_FRAMES} warmup_frames={WARMUP_FRAMES} dirty_interval={DIRTY_FRAME_INTERVAL} samples={SAMPLES} ratio={minimum:.3}..{maximum:.3} tracked_ns={tracked_min_ns}..{tracked_max_ns} tracked_median_ns={tracked_median_ns} tracking_suspended_ns={suspended_min_ns}..{suspended_max_ns} tracking_suspended_median_ns={suspended_median_ns} tracked_allocations={} tracked_allocated_bytes={} rebuilt_entries={} retained_frames={} selected_commands={}",
        tracked_allocations.allocation_count,
        tracked_allocations.allocated_bytes,
        tracked_workload.rebuilt_entries,
        tracked_workload.retained_frames,
        tracked_workload.selected_commands,
    );
    assert!(
        median < 4.0,
        "retained-render allocation profiler overhead unexpectedly high: {median:.2}x"
    );
}

#[cfg(feature = "allocation-profiling")]
struct RetainedRenderBenchmarkFixture {
    root: WidgetNode,
    list: RetainedDisplayList,
    buffer: PixelBuffer,
    dirty_groups: HashSet<NodeId>,
}

#[cfg(feature = "allocation-profiling")]
#[derive(Debug, Default, Clone, Copy)]
struct RetainedRenderWorkload {
    frames: usize,
    dirty_frames: usize,
    retained_frames: usize,
    rebuilt_entries: u64,
    selected_commands: u64,
    checksum: u64,
    elapsed: Duration,
}

#[cfg(feature = "allocation-profiling")]
fn run_retained_render_mode<
    const NODE_COUNT: usize,
    const GROUP_COUNT: usize,
    const ITEMS_PER_GROUP: usize,
>(
    tracked: bool,
    warmup_frames: usize,
    measured_frames: usize,
    dirty_frame_interval: usize,
    surface_width: u32,
    surface_height: u32,
) -> (
    RetainedRenderWorkload,
    mesh_core_debug::allocation::AllocationCounters,
) {
    let mut fixture =
        retained_render_benchmark_fixture::<NODE_COUNT, GROUP_COUNT, ITEMS_PER_GROUP>();
    run_retained_render_frames(
        &mut fixture,
        warmup_frames,
        dirty_frame_interval,
        surface_width,
        surface_height,
    );
    let before = mesh_core_debug::allocation::snapshot();
    let workload = if tracked {
        run_retained_render_frames(
            &mut fixture,
            measured_frames,
            dirty_frame_interval,
            surface_width,
            surface_height,
        )
    } else {
        mesh_core_debug::allocation::with_tracking_suspended(|| {
            run_retained_render_frames(
                &mut fixture,
                measured_frames,
                dirty_frame_interval,
                surface_width,
                surface_height,
            )
        })
    };
    let allocations = mesh_core_debug::allocation::snapshot().saturating_delta(before);
    (workload, allocations)
}

#[cfg(feature = "allocation-profiling")]
fn retained_render_benchmark_fixture<
    const NODE_COUNT: usize,
    const GROUP_COUNT: usize,
    const ITEMS_PER_GROUP: usize,
>() -> RetainedRenderBenchmarkFixture {
    assert_eq!(1 + GROUP_COUNT + GROUP_COUNT * ITEMS_PER_GROUP, NODE_COUNT);

    let mut root = benchmark_node(1, "column", 0.0, 0.0, 1_200.0, 2_800.0);
    root.computed_style.background_color = Color::from_hex("#101820").unwrap();
    let mut next_id = 2;
    let mut dirty_group = 0;

    for group_index in 0..GROUP_COUNT {
        let group_id = next_id;
        next_id += 1;
        if group_index == 0 {
            dirty_group = group_id;
        }
        let mut group = benchmark_node(
            group_id,
            "column",
            0.0,
            (group_index * 520) as f32,
            1_200.0,
            500.0,
        );
        group.computed_style.background_color = if group_index % 2 == 0 {
            Color::from_hex("#1d2a35").unwrap()
        } else {
            Color::from_hex("#243542").unwrap()
        };

        for item_index in 0..ITEMS_PER_GROUP {
            let column = item_index % 17;
            let row = item_index / 17;
            let item_id = next_id;
            next_id += 1;
            let mut item = benchmark_node(
                item_id,
                if item_index % 4 == 0 { "text" } else { "box" },
                (column * 68 + 8) as f32,
                (row * 40 + 8) as f32,
                56.0,
                28.0,
            );
            item.computed_style.background_color = if item_index % 3 == 0 {
                Color::from_hex("#4b6b7b").unwrap()
            } else {
                Color::from_hex("#38515e").unwrap()
            };
            if item_index % 4 == 0 {
                item.attributes.insert(
                    "content".into(),
                    format!("item-{group_index:02}-{item_index:03}"),
                );
            }
            group.children.push(item);
        }
        root.children.push(group);
    }

    let mut list = RetainedDisplayList::default();
    let initial = list.update(&root, 1_200, 2_800, true, true);
    assert!(initial.entries_total >= NODE_COUNT as u64);
    assert_eq!(next_id as usize, NODE_COUNT + 1);

    RetainedRenderBenchmarkFixture {
        root,
        list,
        buffer: PixelBuffer::new(1_200, 2_800),
        dirty_groups: HashSet::from([dirty_group]),
    }
}

#[cfg(feature = "allocation-profiling")]
fn benchmark_node(id: NodeId, tag: &str, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.id = id;
    node.layout.x = x;
    node.layout.y = y;
    node.layout.width = width;
    node.layout.height = height;
    node
}

#[cfg(feature = "allocation-profiling")]
fn run_retained_render_frames(
    fixture: &mut RetainedRenderBenchmarkFixture,
    frames: usize,
    dirty_frame_interval: usize,
    surface_width: u32,
    surface_height: u32,
) -> RetainedRenderWorkload {
    let started = Instant::now();
    let mut workload = RetainedRenderWorkload {
        frames,
        ..Default::default()
    };
    let mut tree_generation = 0;

    for frame in 0..frames {
        let metrics = if frame % dirty_frame_interval == 0 {
            tree_generation += 1;
            fixture.root.children[0]
                .computed_style
                .transform
                .translate_x = (frame % 20) as f32 * 0.25;
            workload.dirty_frames += 1;
            fixture.list.update_for_retained_generation(
                &fixture.root,
                tree_generation,
                RenderObjectDirtySummary {
                    transform: 1,
                    ..Default::default()
                },
                &fixture.dirty_groups,
                surface_width,
                surface_height,
                false,
                true,
            )
        } else {
            workload.retained_frames += 1;
            fixture.list.update_for_retained_generation(
                &fixture.root,
                tree_generation,
                RenderObjectDirtySummary::default(),
                &HashSet::new(),
                surface_width,
                surface_height,
                false,
                true,
            )
        };

        let selected = fixture.list.select_paint_commands(
            Some(metrics.damage_rect),
            DisplayListRepaintPolicy::MinimalDamage,
        );
        let selected_count = selected.len() as u64;
        if selected_count > 0 {
            fixture.buffer.clear_rect(
                metrics.damage_rect.x,
                metrics.damage_rect.y,
                metrics.damage_rect.width,
                metrics.damage_rect.height,
                Color::TRANSPARENT,
            );
            paint_selected_display_list_for_module_with_profiling_metrics(
                &selected,
                &mut fixture.buffer,
                1.0,
                None,
                None,
                None,
                Some("@mesh/benchmark-retained-render"),
            );
        }
        workload.rebuilt_entries = workload
            .rebuilt_entries
            .saturating_add(metrics.entries_rebuilt);
        workload.selected_commands = workload.selected_commands.saturating_add(selected_count);
        workload.checksum = workload
            .checksum
            .wrapping_add(metrics.entries_reused)
            .wrapping_add(metrics.entries_rebuilt)
            .wrapping_add(selected_count);
        black_box(workload.checksum);
    }

    workload.elapsed = started.elapsed();
    workload
}
