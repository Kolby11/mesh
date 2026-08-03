use super::common::*;
use super::*;

#[test]
fn shell_module_manifest_discovery_is_deterministic_and_stops_at_module_roots() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    write_shell_discovery_manifest(&first_root.path().join("zeta"), "@test/zeta", 0);
    write_shell_discovery_manifest(&first_root.path().join("alpha"), "@test/alpha", 0);
    write_shell_discovery_manifest(
        &first_root.path().join("alpha/nested/ignored"),
        "@test/ignored",
        0,
    );
    write_shell_discovery_manifest(&second_root.path().join("beta"), "@test/beta", 0);

    let roots = vec![
        second_root.path().to_path_buf(),
        first_root.path().to_path_buf(),
    ];
    let discovered = discover_shell_module_manifest_dirs(&roots);
    let relative = discovered
        .iter()
        .map(|path| {
            if let Ok(path) = path.strip_prefix(second_root.path()) {
                format!("second/{}", path.display())
            } else {
                format!(
                    "first/{}",
                    path.strip_prefix(first_root.path()).unwrap().display()
                )
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(relative, vec!["second/beta", "first/alpha", "first/zeta"]);
}

#[test]
#[ignore = "release-only shell discovery manifest loading microbenchmark"]
fn shell_module_manifest_parallel_loading_beats_serial_benchmark() {
    if rayon::current_num_threads() <= 1 {
        eprintln!("skipping benchmark: rayon has one worker thread");
        return;
    }

    let root = tempfile::tempdir().unwrap();
    let module_count = 192;
    for index in 0..module_count {
        write_shell_discovery_manifest(
            &root.path().join(format!("frontend/module-{index:03}")),
            &format!("@bench/shell-module-{index:03}"),
            24,
        );
    }

    let module_dirs = discover_shell_module_manifest_dirs(&[root.path().to_path_buf()]);
    assert_eq!(module_dirs.len(), module_count);
    let warmup = load_shell_module_manifests(&module_dirs);
    assert_eq!(warmup.len(), module_count);
    assert!(warmup.iter().all(|result| result.loaded.is_ok()));

    let iterations = 12;
    let serial_start = Instant::now();
    for _ in 0..iterations {
        let loaded = load_shell_module_manifests_serial(&module_dirs);
        assert_eq!(loaded.len(), module_count);
        assert!(loaded.iter().all(|result| result.loaded.is_ok()));
    }
    let serial_elapsed = serial_start.elapsed();

    let parallel_start = Instant::now();
    for _ in 0..iterations {
        let loaded = load_shell_module_manifests(&module_dirs);
        assert_eq!(loaded.len(), module_count);
        assert!(loaded.iter().all(|result| result.loaded.is_ok()));
    }
    let parallel_elapsed = parallel_start.elapsed();

    eprintln!(
        "shell manifest load over {iterations} iterations and {module_count} modules: serial {serial_elapsed:?}; parallel {parallel_elapsed:?}; ratio {:.1}x",
        serial_elapsed.as_secs_f64() / parallel_elapsed.as_secs_f64()
    );
    assert!(
        parallel_elapsed < serial_elapsed,
        "parallel shell manifest loading should beat serial loading"
    );
}
