use super::*;
use std::hint::black_box;
use std::time::Instant;

#[cfg(feature = "allocation-benchmark")]
mod allocations {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub static COUNT: AtomicU64 = AtomicU64::new(0);
    pub struct CountingAllocator;
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            COUNT.fetch_add(1, Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            COUNT.fetch_add(1, Ordering::Relaxed);
            unsafe { System.realloc(ptr, layout, size) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;
}

fn allocation_count() -> u64 {
    #[cfg(feature = "allocation-benchmark")]
    {
        allocations::COUNT.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(feature = "allocation-benchmark"))]
    {
        0
    }
}

fn report(label: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    println!(
        "{label}: p50={:.3}ms p95={:.3}ms",
        samples[samples.len() / 2],
        samples[(samples.len() * 95 / 100).min(samples.len() - 1)]
    );
}

fn report_allocations(label: &str, mut samples: Vec<u64>) {
    if cfg!(feature = "allocation-benchmark") {
        samples.sort_unstable();
        println!(
            "{label}: allocations p50={} p95={}",
            samples[samples.len() / 2],
            samples[(samples.len() * 95 / 100).min(samples.len() - 1)]
        );
    }
}

#[test]
#[ignore = "release-only catalog parsing and projection workload"]
fn locale_catalog_workloads() {
    for keys in [100, 1000, 4000] {
        let object = (0..keys)
            .map(|index| {
                (
                    format!("setting.{index:05}"),
                    serde_json::json!(format!("Value {index}")),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let json = serde_json::to_string(&object).unwrap();
        let mut parse_samples = Vec::new();
        let mut parse_allocations = Vec::new();
        for _ in 0..30 {
            let start = Instant::now();
            let before = allocation_count();
            let (value, duplicates) = parse_catalog_document(black_box(&json)).unwrap();
            let catalog = compile_catalog("en", &value);
            assert!(duplicates.is_empty() && catalog.diagnostics.is_empty());
            assert_eq!(black_box(catalog.messages.len()), keys);
            parse_allocations.push(allocation_count() - before);
            parse_samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        report(
            &format!("parse+compile keys={keys} bytes={}", json.len()),
            parse_samples,
        );
        report_allocations(&format!("parse+compile keys={keys}"), parse_allocations);
        for layers in [1, 5] {
            let mut engine = LocaleEngine::new("en-US");
            let catalog = compile_catalog("en", &serde_json::Value::Object(object.clone()));
            for _ in 0..layers {
                engine.load_module_catalog("@test/catalog", catalog.clone());
            }
            let snapshot = engine.snapshot();
            let mut samples = Vec::new();
            let mut allocations = Vec::new();
            for _ in 0..30 {
                let start = Instant::now();
                let before = allocation_count();
                let projection = snapshot.effective_module_translations("@test/catalog");
                assert_eq!(black_box(projection.len()), keys);
                allocations.push(allocation_count() - before);
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            report(
                &format!(
                    "projection keys={keys} layers={layers} chain={}",
                    snapshot.fallback_chain().len()
                ),
                samples,
            );
            report_allocations(
                &format!("projection keys={keys} layers={layers}"),
                allocations,
            );
        }
    }
    for keys in [10, 100] {
        let mut engine = LocaleEngine::new("en");
        engine.load_module_translations(
            "@test/catalog",
            TranslationSet {
                locale: "en".into(),
                messages: (0..keys)
                    .map(|index| (format!("key{index}"), format!("value{index}")))
                    .collect(),
            },
        );
        let snapshot = engine.snapshot();
        for count in [1, 100, 1000] {
            let mut samples = Vec::new();
            let mut allocations = Vec::new();
            for _ in 0..30 {
                let start = Instant::now();
                let before = allocation_count();
                for _ in 0..count {
                    let translator = snapshot.module_translator(black_box("@test/catalog"));
                    assert!(black_box(translator.translate("key0")).is_some());
                }
                allocations.push(allocation_count() - before);
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            report(
                &format!("translator+lookup instances={count} keys={keys}"),
                samples,
            );
            report_allocations(
                &format!("translator+lookup instances={count} keys={keys}"),
                allocations,
            );
        }
    }
    let locales = ["en", "de", "fr", "it", "sk", "nl", "pl", "sv", "fi", "pt"];
    for fallback_count in [2, 5, 10] {
        for layers in [1, 5, 20] {
            let mut engine = LocaleEngine::new("en");
            for locale in &locales[..fallback_count] {
                let catalog = CompiledCatalog {
                    locale: (*locale).into(),
                    messages: (0..100)
                        .map(|index| {
                            (
                                format!("key{index}"),
                                CatalogEntry::Text(format!("{locale}-{index}")),
                            )
                        })
                        .collect(),
                    diagnostics: Vec::new(),
                };
                for _ in 0..layers {
                    engine.load_module_catalog("@test/catalog", catalog.clone());
                }
            }
            let mut snapshot = (*engine.snapshot()).clone();
            snapshot.selection.chain = locales[..fallback_count]
                .iter()
                .map(|locale| (*locale).to_string())
                .collect();
            let mut samples = Vec::new();
            let mut allocations = Vec::new();
            for _ in 0..30 {
                let start = Instant::now();
                let before = allocation_count();
                let projection = snapshot.effective_module_translations("@test/catalog");
                assert_eq!(black_box(projection.len()), 100);
                allocations.push(allocation_count() - before);
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            report(
                &format!(
                    "projection keys=100 fallback={fallback_count} layers_per_locale={layers}"
                ),
                samples,
            );
            report_allocations(
                &format!(
                    "projection keys=100 fallback={fallback_count} layers_per_locale={layers}"
                ),
                allocations,
            );
        }
    }
    let mut large_engine = LocaleEngine::new("en");
    for layer in 0..25 {
        let catalog = CompiledCatalog {
            locale: "en".into(),
            messages: (0..4000)
                .map(|index| {
                    (
                        format!("layer{layer}.key{index:04}"),
                        CatalogEntry::Text(format!("value {index}")),
                    )
                })
                .collect(),
            diagnostics: Vec::new(),
        };
        large_engine.load_module_catalog("@test/large", catalog);
    }
    let large = large_engine.snapshot();
    let mut large_samples = Vec::new();
    let mut large_allocations = Vec::new();
    for _ in 0..10 {
        let start = Instant::now();
        let before = allocation_count();
        let projection = large.effective_module_translations("@test/large");
        assert_eq!(black_box(projection.len()), 100_000);
        large_allocations.push(allocation_count() - before);
        large_samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    report(
        "projection unique_keys=100000 layers=25 keys_each=4000",
        large_samples,
    );
    report_allocations(
        "projection unique_keys=100000 layers=25 keys_each=4000",
        large_allocations,
    );
    drop(large);
    drop(large_engine);
    let path =
        std::env::temp_dir().join(format!("mesh-locale-benchmark-{}.json", std::process::id()));
    let object = (0..100)
        .map(|index| {
            (
                format!("setting.{index:05}"),
                serde_json::json!(format!("Value {index}")),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let json = serde_json::to_string(&object).unwrap();
    std::fs::write(&path, &json).unwrap();
    let engine = LocaleEngine::new("en");
    for count in [4, 40, 400] {
        let sources = (0..count)
            .map(|index| {
                CatalogSource::module(
                    format!("@test/module-{index}"),
                    "default",
                    "en",
                    path.clone(),
                )
            })
            .collect::<Vec<_>>();
        let mut samples = Vec::new();
        for _ in 0..10 {
            let start = Instant::now();
            let prepared = engine
                .prepare_catalog_snapshot(&sources, &HashMap::new())
                .unwrap();
            black_box(prepared);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        report(
            &format!(
                "prepare catalogs={count} keys_each=100 bytes_each={}",
                json.len()
            ),
            samples,
        );
    }
    std::fs::remove_file(path).unwrap();

    let directory = std::env::temp_dir().join(format!(
        "mesh-locale-benchmark-files-{}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap();
    let paths = (0..400)
        .map(|index| {
            let path = directory.join(format!("catalog-{index}.json"));
            std::fs::write(&path, &json).unwrap();
            path
        })
        .collect::<Vec<_>>();
    for count in [4, 40, 400] {
        let sources = paths[..count]
            .iter()
            .enumerate()
            .map(|(index, path)| {
                CatalogSource::module(
                    format!("@test/module-{index}"),
                    "default",
                    "en",
                    path.clone(),
                )
            })
            .collect::<Vec<_>>();
        for changed in [false, true] {
            let mut samples = Vec::new();
            let mut allocations = Vec::new();
            for iteration in 0..10 {
                if changed {
                    let mut changed_object = object.clone();
                    changed_object.insert(
                        "setting.00000".into(),
                        serde_json::json!(format!("changed-{iteration}")),
                    );
                    std::fs::write(&paths[0], serde_json::to_string(&changed_object).unwrap())
                        .unwrap();
                }
                let start = Instant::now();
                let before = allocation_count();
                black_box(
                    engine
                        .prepare_catalog_snapshot(&sources, &HashMap::new())
                        .unwrap(),
                );
                allocations.push(allocation_count() - before);
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            report(
                &format!("prepare separate_files={count} one_edited={changed} keys_each=100"),
                samples,
            );
            report_allocations(
                &format!("prepare separate_files={count} one_edited={changed}"),
                allocations,
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
