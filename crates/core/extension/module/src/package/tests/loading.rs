use super::super::*;
use super::common::*;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn load_installed_module_graph_auto_discovers_modules() {
    let root = temp_dir("auto-discovery-test");
    let config_dir = root.join("config");
    let modules_dir = root.join("modules");
    fs::create_dir_all(&config_dir).unwrap();
    fs::create_dir_all(modules_dir.join("frontend/panel")).unwrap();
    fs::create_dir_all(modules_dir.join("backend/audio")).unwrap();

    fs::write(
        modules_dir.join("frontend/panel/module.json"),
        r#"{ "name": "@me/panel", "version": "0.1.0", "mesh": { "apiVersion": "0.1", "kind": "frontend", "entry": "src/main.mesh", "surface": { "anchor": "top" }, "accessibility": { "role": "toolbar" } } }"#,
    )
    .unwrap();
    fs::write(
        modules_dir.join("backend/audio/module.json"),
        r#"{ "name": "@me/audio-backend", "version": "0.1.0", "mesh": { "apiVersion": "0.1", "kind": "backend", "entry": "src/main.luau", "implements": [{ "interface": "me.audio", "version": "1.0", "provider": "demo" }] } }"#,
    )
    .unwrap();

    // Decisions-only root graph: no `modules` inventory, one disabled module.
    fs::write(
        config_dir.join("module.json"),
        r#"{ "name": "@me/config", "version": "0.1.0", "mesh": { "schemaVersion": 1, "modulesDir": "../modules", "disabled": ["@me/audio-backend"] } }"#,
    )
    .unwrap();

    let graph = load_installed_module_graph(&config_dir.join("module.json")).unwrap();

    // Both modules are discovered from disk without a `modules` map...
    assert!(graph.module("@me/panel").unwrap().enabled);
    // ...and the `disabled` decision is honored.
    assert!(!graph.module("@me/audio-backend").unwrap().enabled);
    let frontends = graph.frontend_modules();
    assert_eq!(frontends.len(), 1);
    assert_eq!(frontends[0].id, "@me/panel");
}

#[test]
fn module_loader_resolves_keyed_external_interface_contract() {
    let root = temp_dir("external-interface-contract");
    fs::write(
        root.join("module.json"),
        r#"{
  "name": "@me/audio-interface",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "me.audio",
      "version": "1.0",
      "contract": "contract.json"
    }
  }
}"#,
    )
    .unwrap();
    fs::write(
        root.join("contract.json"),
        r#"{
  "state": { "percent": { "type": "float" } },
  "methods": { "set_percent": {
    "args": [{ "name": "value", "type": "float" }],
    "stateBinding": { "field": "percent", "fromArg": "value" }
  } },
  "events": { "Changed": { "payload": [] } }
}"#,
    )
    .unwrap();

    let loaded = load_module_manifest(&root).unwrap();
    let declaration = loaded.manifest.mesh.interface.as_ref().unwrap();
    let contract = declaration.contract.as_ref().unwrap();
    let parsed = mesh_core_service::parse_interface_contract(
        &declaration.name,
        declaration.version.as_deref().unwrap(),
        contract,
    )
    .unwrap();
    assert_eq!(parsed.methods[0].name, "set_percent");
    assert_eq!(parsed.events[0].name, "Changed");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_installed_module_graph_loads_explicit_inventory() {
    let root = temp_dir("explicit-installed-graph");
    let config_dir = root.join("config");
    let modules_dir = root.join("modules");
    fs::create_dir_all(&config_dir).unwrap();

    for (path, id) in [("panel", "@me/panel"), ("settings", "@me/settings")] {
        let module_dir = modules_dir.join(path);
        fs::create_dir_all(&module_dir).unwrap();
        fs::write(
            module_dir.join("module.json"),
            format!(
                r#"{{ "name": "{id}", "version": "0.1.0", "mesh": {{ "apiVersion": "0.1", "kind": "frontend", "entry": "src/main.mesh" }} }}"#
            ),
        )
        .unwrap();
    }

    fs::write(
        config_dir.join("module.json"),
        r#"{
  "name": "@me/config",
  "version": "0.1.0",
  "mesh": {
    "schemaVersion": 1,
    "modulesDir": "../modules",
    "modules": {
      "@me/panel": { "kind": "frontend", "path": "panel", "enabled": true },
      "@me/settings": { "kind": "frontend", "path": "settings", "enabled": false }
    }
  }
}"#,
    )
    .unwrap();

    let graph = load_installed_module_graph(&config_dir.join("module.json")).unwrap();
    assert!(graph.module("@me/panel").unwrap().enabled);
    assert!(!graph.module("@me/settings").unwrap().enabled);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn parallel_discovered_manifest_loading_preserves_directory_order() {
    let root = temp_dir("parallel-manifest-order");
    let modules_dir = root.join("modules");
    let ids = ["gamma", "alpha", "beta"];
    for id in ids {
        let module_dir = modules_dir.join(id);
        fs::create_dir_all(&module_dir).unwrap();
        fs::write(
            module_dir.join("module.json"),
            format!(
                r#"{{
  "name": "@me/{id}",
  "version": "0.1.0",
  "mesh": {{
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh"
  }}
}}"#
            ),
        )
        .unwrap();
    }

    let module_dirs = discover_module_dirs(&modules_dir);
    let serial = load_discovered_module_manifests_serial(&module_dirs).unwrap();
    let parallel = load_discovered_module_manifests(&module_dirs).unwrap();
    let serial_ids = serial
        .iter()
        .map(|(_, loaded)| loaded.manifest.name.as_str())
        .collect::<Vec<_>>();
    let parallel_ids = parallel
        .iter()
        .map(|(_, loaded)| loaded.manifest.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(serial_ids, parallel_ids);
    assert_eq!(parallel_ids, vec!["@me/alpha", "@me/beta", "@me/gamma"]);

    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "release-only startup manifest loading microbenchmark"]
fn parallel_discovered_manifest_loading_beats_serial_benchmark() {
    if rayon::current_num_threads() <= 1 {
        eprintln!("skipping benchmark: rayon has one worker thread");
        return;
    }

    let root = temp_dir("parallel-manifest-benchmark");
    let modules_dir = root.join("modules");
    let module_count = 192;
    for index in 0..module_count {
        let module_dir = modules_dir.join(format!("frontend/module-{index:03}"));
        fs::create_dir_all(&module_dir).unwrap();
        let mut slots = String::new();
        for slot in 0..16 {
            slots.push_str(&format!(
                r#""slot-{slot}": [{{ "widget": "@bench/module-{index:03}", "id": "item-{slot}", "order": {slot}, "props": {{ "label": "Item {slot}", "index": {index} }} }}]"#
            ));
            if slot + 1 < 16 {
                slots.push(',');
            }
        }
        fs::write(
            module_dir.join("module.json"),
            format!(
                r#"{{
  "name": "@bench/module-{index:03}",
  "version": "0.1.0",
  "mesh": {{
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "capabilities": {{
      "required": ["shell.surface"],
      "optional": ["service.demo.read", "service.demo.write"]
    }},
    "icons": {{
      "required": ["audio-volume-high", "settings"],
      "optional": ["weather-clear", "network-wireless"]
    }},
    "slots": {{
      {slots}
    }}
  }}
}}"#
            ),
        )
        .unwrap();
    }

    let module_dirs = discover_module_dirs(&modules_dir);
    assert_eq!(module_dirs.len(), module_count);

    let warmup = load_discovered_module_manifests(&module_dirs).unwrap();
    assert_eq!(warmup.len(), module_count);

    for (count, iterations) in [(8, 48), (24, 24), (module_count, 12)] {
        let dirs = &module_dirs[..count];
        let serial_start = Instant::now();
        for _ in 0..iterations {
            let loaded = load_discovered_module_manifests_serial(dirs).unwrap();
            assert_eq!(loaded.len(), count);
        }
        let serial_elapsed = serial_start.elapsed();

        let parallel_start = Instant::now();
        for _ in 0..iterations {
            let loaded = load_discovered_module_manifests(dirs).unwrap();
            assert_eq!(loaded.len(), count);
        }
        let parallel_elapsed = parallel_start.elapsed();

        eprintln!(
            "manifest load over {iterations} iterations and {count} modules: serial {serial_elapsed:?}; parallel {parallel_elapsed:?}; ratio {:.1}x",
            serial_elapsed.as_secs_f64() / parallel_elapsed.as_secs_f64()
        );
        assert!(
            parallel_elapsed < serial_elapsed,
            "parallel manifest loading should beat serial loading for {count} modules"
        );
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "release-only canonical startup manifest loading benchmark"]
fn canonical_module_manifest_loading_beats_serial_benchmark() {
    if rayon::current_num_threads() <= 1 {
        eprintln!("skipping benchmark: rayon has one worker thread");
        return;
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let module_dirs = discover_module_dirs(&workspace_root.join("modules"));
    assert!(
        module_dirs.len() >= 8,
        "canonical workload should contain enough modules to exercise parallel loading"
    );

    let serial = load_module_manifests_serial(&module_dirs).unwrap();
    let parallel = load_module_manifests(&module_dirs).unwrap();
    assert_eq!(serial.len(), parallel.len());
    for (serial, parallel) in serial.iter().zip(&parallel) {
        assert_eq!(serial.manifest.name, parallel.manifest.name);
        assert_eq!(serial.path, parallel.path);
        assert_eq!(serial.source, parallel.source);
        assert_eq!(serial.diagnostics, parallel.diagnostics);
    }

    // Re-read the exact shipped manifests enough times to make the sub-millisecond
    // startup stage measurable while retaining the per-startup parallelization cost.
    let iterations = 500;
    let mut serial_elapsed = Duration::ZERO;
    let mut parallel_elapsed = Duration::ZERO;
    for iteration in 0..iterations {
        let mut measure_serial = || {
            let start = Instant::now();
            let loaded = load_module_manifests_serial(std::hint::black_box(&module_dirs)).unwrap();
            std::hint::black_box(loaded);
            serial_elapsed += start.elapsed();
        };
        let mut measure_parallel = || {
            let start = Instant::now();
            let loaded = load_module_manifests(std::hint::black_box(&module_dirs)).unwrap();
            std::hint::black_box(loaded);
            parallel_elapsed += start.elapsed();
        };
        if iteration % 2 == 0 {
            measure_serial();
            measure_parallel();
        } else {
            measure_parallel();
            measure_serial();
        }
    }

    eprintln!(
        "canonical manifest load over {iterations} iterations and {} shipped modules: serial {serial_elapsed:?}; parallel {parallel_elapsed:?}; ratio {:.2}x",
        module_dirs.len(),
        serial_elapsed.as_secs_f64() / parallel_elapsed.as_secs_f64()
    );
    assert!(
        parallel_elapsed < serial_elapsed,
        "parallel manifest loading should improve the canonical startup workload"
    );

    let graph_path = workspace_root.join("config/module.json");
    let serial_graph = load_installed_module_graph_serial(&graph_path).unwrap();
    let parallel_graph = load_installed_module_graph(&graph_path).unwrap();
    let serial_modules = serial_graph
        .modules()
        .into_iter()
        .map(|module| (&module.id, module.kind, &module.path, module.enabled))
        .collect::<Vec<_>>();
    let parallel_modules = parallel_graph
        .modules()
        .into_iter()
        .map(|module| (&module.id, module.kind, &module.path, module.enabled))
        .collect::<Vec<_>>();
    assert_eq!(serial_modules, parallel_modules);
    assert_eq!(serial_graph.diagnostics(), parallel_graph.diagnostics());
    assert_eq!(serial_graph.health(), parallel_graph.health());

    let mut serial_graph_elapsed = Duration::ZERO;
    let mut parallel_graph_elapsed = Duration::ZERO;
    for iteration in 0..iterations {
        let mut measure_serial = || {
            let start = Instant::now();
            let graph =
                load_installed_module_graph_serial(std::hint::black_box(&graph_path)).unwrap();
            std::hint::black_box(graph);
            serial_graph_elapsed += start.elapsed();
        };
        let mut measure_parallel = || {
            let start = Instant::now();
            let graph = load_installed_module_graph(std::hint::black_box(&graph_path)).unwrap();
            std::hint::black_box(graph);
            parallel_graph_elapsed += start.elapsed();
        };
        if iteration % 2 == 0 {
            measure_serial();
            measure_parallel();
        } else {
            measure_parallel();
            measure_serial();
        }
    }

    eprintln!(
        "canonical installed-graph startup over {iterations} iterations: serial {serial_graph_elapsed:?}; parallel {parallel_graph_elapsed:?}; ratio {:.2}x",
        serial_graph_elapsed.as_secs_f64() / parallel_graph_elapsed.as_secs_f64()
    );
    assert!(
        parallel_graph_elapsed < serial_graph_elapsed,
        "parallel manifest loading should improve the complete canonical graph startup"
    );
}

fn write_frontend(modules_dir: &std::path::Path, dir: &str, id: &str) {
    let module_dir = modules_dir.join(dir);
    fs::create_dir_all(&module_dir).unwrap();
    fs::write(
        module_dir.join("module.json"),
        format!(
            r#"{{"name":"{id}","version":"0.1.0",
                "mesh":{{"apiVersion":"0.1","kind":"frontend","entry":"src/main.mesh"}}}}"#
        ),
    )
    .unwrap();
}

#[test]
fn an_invalid_or_unreadable_module_is_isolated_not_fatal() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_dir("isolated-modules");
    let modules_dir = root.join("modules");
    write_frontend(&modules_dir, "alpha", "@me/alpha");
    write_frontend(&modules_dir, "beta", "@me/beta");
    let broken = modules_dir.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("module.json"), "{ \"name\": ").unwrap();
    let locked = modules_dir.join("locked");
    write_frontend(&modules_dir, "locked/gamma", "@me/gamma");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    fs::write(
        root.join("module.json"),
        r#"{"name":"@me/config","version":"0.1.0",
            "mesh":{"schemaVersion":1,"modulesDir":"modules","modules":{}}}"#,
    )
    .unwrap();

    let graph = load_installed_module_graph(&root.join("module.json"));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    let graph = graph.expect("one bad module must not abort the graph");

    assert!(graph.module("@me/alpha").is_some_and(|module| module.enabled));
    assert!(graph.module("@me/beta").is_some_and(|module| module.enabled));
    let isolated = graph
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.module_id.as_str(), diagnostic.status.as_str()))
        .filter(|(_, status)| status.starts_with("module_"))
        .collect::<Vec<_>>();
    assert!(isolated.contains(&("broken", "module_manifest_invalid")), "{isolated:?}");
    // Running as root can read a 0o000 directory; only assert when it can't.
    if fs::read_dir(&locked).is_err() {
        assert!(isolated.contains(&("locked", "module_discovery_failed")), "{isolated:?}");
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_module_mid_edit_keeps_its_last_valid_manifest() {
    let root = temp_dir("retained-manifest");
    let modules_dir = root.join("modules");
    write_frontend(&modules_dir, "alpha", "@me/alpha");
    write_frontend(&modules_dir, "beta", "@me/beta");
    fs::write(
        root.join("module.json"),
        r#"{"name":"@me/config","version":"0.1.0",
            "mesh":{"schemaVersion":1,"modulesDir":"modules","modules":{}}}"#,
    )
    .unwrap();
    let graph_path = root.join("module.json");
    let previous = load_installed_module_graph(&graph_path).unwrap();

    // A half-typed edit: the file no longer parses.
    fs::write(modules_dir.join("beta/module.json"), "{ \"name\": \"@me/be").unwrap();
    let retained = load_installed_module_graph_retaining(&graph_path, None, &previous)
        .expect("an invalid manifest must not abort the reload");
    assert_eq!(
        retained
            .module("@me/beta")
            .map(|module| (module.manifest.name.clone(), module.manifest_path.clone())),
        previous
            .module("@me/beta")
            .map(|module| (module.manifest.name.clone(), module.manifest_path.clone())),
        "the module keeps the manifest it had before the edit"
    );
    assert!(retained.module("@me/beta").is_some_and(|module| module.enabled));
    assert!(retained.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@me/beta" && diagnostic.status == "module_manifest_retained"
    }));

    // Without a previous graph (a cold start) the module is isolated.
    let cold = load_installed_module_graph(&graph_path).unwrap();
    assert!(cold.module("@me/beta").is_none());

    // Fixing the file picks up the new manifest and clears the diagnostic.
    write_frontend(&modules_dir, "beta", "@me/beta");
    let fixed = load_installed_module_graph_retaining(&graph_path, None, &retained).unwrap();
    assert!(fixed.module("@me/beta").is_some());
    assert!(
        !fixed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.status == "module_manifest_retained")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_explicit_module_mid_edit_keeps_its_last_valid_manifest_and_decisions() {
    let root = temp_dir("retained-explicit");
    let modules_dir = root.join("modules");
    write_frontend(&modules_dir, "alpha", "@me/alpha");
    fs::write(
        root.join("module.json"),
        r#"{"name":"@me/config","version":"0.1.0","mesh":{"schemaVersion":1,
            "modulesDir":"modules",
            "modules":{"@me/alpha":{"kind":"frontend","path":"alpha","enabled":true}}}}"#,
    )
    .unwrap();
    let graph_path = root.join("module.json");
    let previous = load_installed_module_graph(&graph_path).unwrap();

    fs::write(modules_dir.join("alpha/module.json"), "not json").unwrap();
    let retained = load_installed_module_graph_retaining(&graph_path, None, &previous)
        .expect("an invalid explicit module keeps its last manifest");
    assert!(retained.module("@me/alpha").is_some_and(|module| module.enabled));
    assert!(retained.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@me/alpha" && diagnostic.status == "module_manifest_retained"
    }));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_invalid_explicit_module_drops_only_itself_and_decisions_naming_it() {
    let root = temp_dir("isolated-explicit");
    let modules_dir = root.join("modules");
    write_frontend(&modules_dir, "alpha", "@me/alpha");
    let broken = modules_dir.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("module.json"), "not json").unwrap();
    fs::write(
        root.join("module.json"),
        r#"{"name":"@me/config","version":"0.1.0","mesh":{"schemaVersion":1,
            "modulesDir":"modules",
            "modules":{
              "@me/alpha":{"kind":"frontend","path":"alpha","enabled":true},
              "@me/broken":{"kind":"backend","path":"broken","enabled":true}},
            "providers":{"mesh.audio":"@me/broken"}}}"#,
    )
    .unwrap();

    let graph = load_installed_module_graph(&root.join("module.json"))
        .expect("an invalid explicit module is isolated");
    assert!(graph.module("@me/alpha").is_some());
    assert!(graph.module("@me/broken").is_none());
    assert!(graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.module_id == "@me/broken" && diagnostic.status == "module_manifest_invalid"
    }));

    fs::remove_dir_all(root).unwrap();
}
