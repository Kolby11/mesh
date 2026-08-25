use super::child::{child_surface_id, child_surface_paint_cache_matches};
use mesh_core_elements::style::Color;
use mesh_core_render::PixelBuffer;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::hint::black_box;
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
