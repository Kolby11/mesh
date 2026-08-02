use super::super::cache::*;
use super::super::index::*;
use super::super::*;
use crate::lru::LruCache;
use mesh_core_component::style::{Declaration, StyleValue};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

#[test]
fn indexed_theme_defaults_reuse_lowered_declarations_per_revision() {
    let mut defaults = mesh_core_theme::ComponentDefaults::new();
    defaults.insert("font-size".into(), "var(--spacing-md)".into());
    defaults.insert("--local-accent".into(), "#445566".into());

    let first = indexed_theme_defaults(u64::MAX - 1, &defaults);
    let second = indexed_theme_defaults(u64::MAX - 1, &defaults);
    assert!(Arc::ptr_eq(&first, &second));
    assert!(matches!(&first[0].property, IndexedProperty::Custom(_)));
    assert!(matches!(&first[1].value, StyleValue::Var(_)));

    let changed_revision = indexed_theme_defaults(u64::MAX, &defaults);
    assert!(!Arc::ptr_eq(&first, &changed_revision));
}

#[test]
fn bounded_style_caches_evict_one_cold_entry_without_flushing_hot_entries() {
    INLINE_STYLE_CACHE.with(|cache| cache.borrow_mut().clear());
    let hot_inline = cached_inline_style("left: 7px;");
    for index in 0..MAX_INLINE_STYLE_CACHE_ENTRIES - 1 {
        cached_inline_style(&format!("left: {}px;", index + 100));
    }
    let refreshed_inline = cached_inline_style("left: 7px;");
    cached_inline_style("left: 99999px;");
    let retained_inline = cached_inline_style("left: 7px;");
    match (hot_inline, refreshed_inline, retained_inline) {
        (
            CachedInlineStyle::Declarations(first),
            CachedInlineStyle::Declarations(refreshed),
            CachedInlineStyle::Declarations(retained),
        ) => {
            assert!(Arc::ptr_eq(&first, &refreshed));
            assert!(Arc::ptr_eq(&first, &retained));
        }
        _ => panic!("valid inline declarations must remain cached"),
    }
    INLINE_STYLE_CACHE.with(|cache| {
        let cache = cache.borrow();
        assert_eq!(cache.len(), MAX_INLINE_STYLE_CACHE_ENTRIES);
        assert!(!cache.contains_key("left: 100px;"));
    });

    SHARED_THEME_DEFAULT_CACHE.with(|cache| cache.borrow_mut().clear());
    let props = HashMap::new();
    let hot_defaults = Rc::new(ThemeComponentDefaults::default());
    remember_shared_theme_defaults(1, 0, &props, "hot", None, &hot_defaults);
    for index in 0..MAX_SHARED_THEME_DEFAULTS_PER_REVISION - 1 {
        remember_shared_theme_defaults(
            1,
            0,
            &props,
            &format!("cold-{index}"),
            None,
            &Rc::new(ThemeComponentDefaults::default()),
        );
    }
    assert!(Rc::ptr_eq(
        &shared_theme_defaults(1, 0, &props, "hot", None).unwrap(),
        &hot_defaults
    ));
    remember_shared_theme_defaults(
        1,
        0,
        &props,
        "new",
        None,
        &Rc::new(ThemeComponentDefaults::default()),
    );
    assert!(Rc::ptr_eq(
        &shared_theme_defaults(1, 0, &props, "hot", None).unwrap(),
        &hot_defaults
    ));
    assert!(shared_theme_defaults(1, 0, &props, "cold-0", None).is_none());

    for revision in 2..=MAX_SHARED_THEME_REVISIONS as u64 {
        remember_shared_theme_defaults(
            revision,
            0,
            &props,
            "revision",
            None,
            &Rc::new(ThemeComponentDefaults::default()),
        );
    }
    let _ = shared_theme_defaults(1, 0, &props, "hot", None);
    remember_shared_theme_defaults(
        MAX_SHARED_THEME_REVISIONS as u64 + 1,
        0,
        &props,
        "revision",
        None,
        &Rc::new(ThemeComponentDefaults::default()),
    );
    SHARED_THEME_DEFAULT_CACHE.with(|cache| {
        let cache = cache.borrow();
        assert_eq!(cache.len(), MAX_SHARED_THEME_REVISIONS);
        assert!(cache.contains_key(&1));
        assert!(!cache.contains_key(&2));
    });

    THEME_DEFAULT_DECLARATION_CACHE.with(|cache| cache.borrow_mut().clear());
    let hot_declarations = Box::new(mesh_core_theme::ComponentDefaults::new());
    let hot_lowered = indexed_theme_defaults(10_001, &hot_declarations);
    let cold_declarations = (0..MAX_THEME_DEFAULT_DECLARATION_CACHE_ENTRIES - 1)
        .map(|_| Box::new(mesh_core_theme::ComponentDefaults::new()))
        .collect::<Vec<_>>();
    for declarations in &cold_declarations {
        indexed_theme_defaults(10_001, declarations);
    }
    let refreshed_lowered = indexed_theme_defaults(10_001, &hot_declarations);
    assert!(Arc::ptr_eq(&hot_lowered, &refreshed_lowered));
    let newcomer = Box::new(mesh_core_theme::ComponentDefaults::new());
    indexed_theme_defaults(10_001, &newcomer);
    let retained_lowered = indexed_theme_defaults(10_001, &hot_declarations);
    assert!(Arc::ptr_eq(&hot_lowered, &retained_lowered));
    THEME_DEFAULT_DECLARATION_CACHE.with(|cache| {
        assert_eq!(
            cache.borrow().len(),
            MAX_THEME_DEFAULT_DECLARATION_CACHE_ENTRIES
        );
    });
}

// cargo test -p mesh-core-elements --release -- bounded_style_cache_p95_beats_flush_all --ignored --nocapture
#[test]
#[ignore = "release-only cache-churn p95 benchmark"]
fn bounded_style_cache_p95_beats_flush_all() {
    fn percentile_95(mut samples: Vec<u128>) -> u128 {
        samples.sort_unstable();
        samples[samples.len() * 95 / 100]
    }

    fn measure(flush_all: bool, sources: &[String]) -> u128 {
        const CAPACITY: usize = 128;
        const HOT: usize = 96;
        const ROTATING_PER_FRAME: usize = 12;
        const FRAMES: usize = 300;
        let mut flush_cache = HashMap::<String, usize>::new();
        let mut lru_cache = LruCache::<String, usize>::new(CAPACITY);
        let mut frame_times = Vec::with_capacity(FRAMES);
        let mut checksum = 0usize;

        for frame in 0..FRAMES {
            let started = std::time::Instant::now();
            let rotating_start = HOT + (frame * ROTATING_PER_FRAME) % (sources.len() - HOT);
            let indices = (0..HOT).chain(
                (0..ROTATING_PER_FRAME)
                    .map(|offset| HOT + (rotating_start - HOT + offset) % (sources.len() - HOT)),
            );
            for index in indices {
                let source = &sources[index];
                if flush_all {
                    let value = if let Some(value) = flush_cache.get(source) {
                        *value
                    } else {
                        let parsed = mesh_core_component::parse_inline_style(source).unwrap();
                        if flush_cache.len() >= CAPACITY {
                            flush_cache.clear();
                        }
                        let value = parsed.len();
                        flush_cache.insert(source.clone(), value);
                        value
                    };
                    checksum = checksum.wrapping_add(value);
                } else {
                    let value = if let Some(value) = lru_cache.get(source) {
                        *value
                    } else {
                        let parsed = mesh_core_component::parse_inline_style(source).unwrap();
                        let value = parsed.len();
                        lru_cache.insert(source.clone(), value);
                        value
                    };
                    checksum = checksum.wrapping_add(value);
                }
            }
            frame_times.push(started.elapsed().as_nanos());
        }
        std::hint::black_box(checksum);
        percentile_95(frame_times)
    }

    let sources = (0..608)
        .map(|index| {
            format!(
                "left: {index}px; top: {}px; width: {}px; opacity: 0.8;",
                index + 1,
                index + 2
            )
        })
        .collect::<Vec<_>>();
    let mut flush_samples = Vec::new();
    let mut lru_samples = Vec::new();
    for _ in 0..5 {
        flush_samples.push(measure(true, &sources));
        lru_samples.push(measure(false, &sources));
    }
    let flush_min = *flush_samples.iter().min().unwrap();
    let flush_max = *flush_samples.iter().max().unwrap();
    let lru_min = *lru_samples.iter().min().unwrap();
    let lru_max = *lru_samples.iter().max().unwrap();
    let conservative_ratio = flush_min as f64 / lru_max as f64;
    eprintln!(
        "MESH_PERF metric=bounded_style_cache_p95_speedup value={conservative_ratio:.6} flush_all_p95_ns={flush_min}-{flush_max} lru_p95_ns={lru_min}-{lru_max} workload=300_frames_96_hot_12_rotating capacity=128"
    );
    assert!(
        conservative_ratio >= 1.25,
        "bounded eviction p95 must be at least 1.25x faster; conservative ratio {conservative_ratio:.3}x"
    );
}

#[test]
fn typed_static_declarations_match_string_lowering() {
    let theme = mesh_core_theme::default_theme();
    let resolver = StyleResolver::new(&theme);
    let declarations = [
        ("background-color", "#112233"),
        ("color", "rgba(255, 255, 255, 0.75)"),
        ("font-size", "15px"),
        ("font-weight", "600"),
        ("line-height", "1.4"),
        ("padding", "4px 8px 12px 16px"),
        ("margin", "3px 6px"),
        ("border-width", "2px 1px"),
        ("border-radius", "2px 4px 6px 8px"),
        ("width", "75%"),
        ("height", "fit-content"),
        ("flex-basis", "24px"),
    ];
    let mut old_style = ComputedStyle::default();
    let mut typed_style = ComputedStyle::default();
    let mut old_variables = HashMap::new();
    let mut typed_variables = HashMap::new();

    for (property, value) in declarations {
        let declaration = Declaration {
            property: property.into(),
            value: StyleValue::Literal(value.into()),
        };
        resolver.apply_declaration_no_diagnostics(&mut old_style, &declaration, &mut old_variables);
        let indexed = IndexedDeclaration::from_declaration(&declaration);
        assert!(indexed.literal.is_some(), "{property} should lower once");
        resolver.apply_indexed_declaration(&mut typed_style, &indexed, None, &mut typed_variables);
    }

    assert_eq!(typed_style, old_style);
    assert_eq!(typed_variables, old_variables);
}

// cargo test -p mesh-core-elements --release -- style_function_scan_beats_contains_pair --ignored --nocapture
#[test]
#[ignore = "release-only literal style-value scan microbenchmark"]
fn style_function_scan_beats_contains_pair() {
    use std::time::Instant;

    // Representative literal declaration values: mostly plain, a few real
    // references, in the proportion a theme stylesheet produces.
    const VALUES: &[&str] = &[
        "8px",
        "1px solid rgba(0, 0, 0, 0.12)",
        "var(--color-surface)",
        "600",
        "center",
        "calc(var(--spacing-2) * 2)",
        "0 2px 8px rgba(0, 0, 0, 0.24)",
        "prop(size)",
        "flex-start",
        "12px 16px",
    ];
    const ITERATIONS: usize = 3_000_000;

    for value in VALUES {
        assert_eq!(
            references_style_function(value),
            value.contains("var(") || value.contains("prop(")
        );
    }

    let contains_started = Instant::now();
    let mut contains_total = 0usize;
    for index in 0..ITERATIONS {
        let value = std::hint::black_box(VALUES[index % VALUES.len()]);
        contains_total += (value.contains("var(") || value.contains("prop(")) as usize;
    }
    let contains = contains_started.elapsed();

    let scan_started = Instant::now();
    let mut scan_total = 0usize;
    for index in 0..ITERATIONS {
        let value = std::hint::black_box(VALUES[index % VALUES.len()]);
        scan_total += references_style_function(value) as usize;
    }
    let scan = scan_started.elapsed();

    assert_eq!(contains_total, scan_total);
    eprintln!(
        "literal style-value reference check over {ITERATIONS} values: contains pair {contains:?}, byte scan {scan:?}, ratio {:.2}x",
        contains.as_secs_f64() / scan.as_secs_f64()
    );
    println!(
        "MESH_PERF metric=style_function_scan_speedup value={:.6}",
        contains.as_secs_f64() / scan.as_secs_f64()
    );
}

#[test]
fn references_style_function_matches_the_contains_pair_it_replaced() {
    for value in [
        "",
        "(",
        "var",
        "prop",
        "var(",
        "prop(",
        "var(--x)",
        "prop(size)",
        "8px",
        "rgba(0, 0, 0, 0.5)",
        "calc(var(--gap) * 2)",
        "linear-gradient(prop(a), var(b))",
        "avar(",
        "aprop(",
        "novar",
        "wrap(",
        "p(",
        "ar(",
        "(var",
        "translate(4px) var(--y)",
        "☃var(--snow)",
    ] {
        assert_eq!(
            references_style_function(value),
            value.contains("var(") || value.contains("prop("),
            "mismatch for {value:?}"
        );
    }
}
