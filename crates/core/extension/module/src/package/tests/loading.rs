use super::super::*;
use super::common::*;
use std::fs;

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
        r#"{ "name": "@me/panel", "version": "0.1.0", "mesh": { "apiVersion": "0.1", "kind": "ui", "entry": "src/main.mesh", "surface": { "anchor": "top" }, "accessibility": { "role": "toolbar" } } }"#,
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
                r#"{{ "name": "{id}", "version": "0.1.0", "mesh": {{ "apiVersion": "0.1", "kind": "ui", "entry": "src/main.mesh" }} }}"#
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
      "@me/panel": { "kind": "ui", "path": "panel", "enabled": true },
      "@me/settings": { "kind": "ui", "path": "settings", "enabled": false }
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
    "kind": "ui",
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

fn write_frontend(modules_dir: &std::path::Path, dir: &str, id: &str) {
    let module_dir = modules_dir.join(dir);
    fs::create_dir_all(&module_dir).unwrap();
    fs::write(
        module_dir.join("module.json"),
        format!(
            r#"{{"name":"{id}","version":"0.1.0",
                "mesh":{{"apiVersion":"0.1","kind":"ui","entry":"src/main.mesh"}}}}"#
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

    assert!(
        graph
            .module("@me/alpha")
            .is_some_and(|module| module.enabled)
    );
    assert!(
        graph
            .module("@me/beta")
            .is_some_and(|module| module.enabled)
    );
    let isolated = graph
        .diagnostics()
        .iter()
        .map(|diagnostic| (diagnostic.module_id.as_str(), diagnostic.status.as_str()))
        .filter(|(_, status)| status.starts_with("module_"))
        .collect::<Vec<_>>();
    assert!(
        isolated.contains(&("broken", "module_manifest_invalid")),
        "{isolated:?}"
    );
    // Running as root can read a 0o000 directory; only assert when it can't.
    if fs::read_dir(&locked).is_err() {
        assert!(
            isolated.contains(&("locked", "module_discovery_failed")),
            "{isolated:?}"
        );
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
    assert!(
        retained
            .module("@me/beta")
            .is_some_and(|module| module.enabled)
    );
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
            "modules":{"@me/alpha":{"kind":"ui","path":"alpha","enabled":true}}}}"#,
    )
    .unwrap();
    let graph_path = root.join("module.json");
    let previous = load_installed_module_graph(&graph_path).unwrap();

    fs::write(modules_dir.join("alpha/module.json"), "not json").unwrap();
    let retained = load_installed_module_graph_retaining(&graph_path, None, &previous)
        .expect("an invalid explicit module keeps its last manifest");
    assert!(
        retained
            .module("@me/alpha")
            .is_some_and(|module| module.enabled)
    );
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
              "@me/alpha":{"kind":"ui","path":"alpha","enabled":true},
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

#[test]
fn retained_manifests_require_unchanged_source_and_identity_in_both_discovery_modes() {
    for explicit in [false, true] {
        let root = temp_dir("retained-source-identity");
        let modules = root.join("modules");
        write_frontend(&modules, "alpha", "@me/alpha");
        let inventory = if explicit {
            serde_json::json!({"@me/alpha":{"kind":"ui","path":"alpha"}})
        } else {
            serde_json::json!({})
        };
        let graph_path = root.join("module.json");
        fs::write(&graph_path, serde_json::json!({"name":"@me/config","version":"1","mesh":{"modulesDir":"modules","modules":inventory}}).to_string()).unwrap();
        let original = load_installed_module_graph(&graph_path).unwrap();
        for filename in ["new.luau", "assets/new.json"] {
            let added = modules.join("alpha").join(filename);
            fs::create_dir_all(added.parent().unwrap()).unwrap();
            fs::write(&added, "new content").unwrap();
            fs::write(modules.join("alpha/module.json"), "{").unwrap();
            let changed =
                load_installed_module_graph_retaining(&graph_path, None, &original).unwrap();
            assert!(changed.module("@me/alpha").is_none());
            fs::remove_file(added).unwrap();
        }
        fs::write(modules.join("alpha/module.json"), "{").unwrap();
        fs::write(
            modules.join("alpha/main.mesh"),
            "<template><text>changed</text></template>",
        )
        .unwrap();
        let changed = load_installed_module_graph_retaining(&graph_path, None, &original).unwrap();
        assert!(changed.module("@me/alpha").is_none());
        write_frontend(&modules, "alpha", "@me/alpha");
        let original = load_installed_module_graph(&graph_path).unwrap();
        fs::write(
            modules.join("alpha/module.json"),
            r#"{"name":"@me/replacement","version":"1","mesh":{"kind":"invalid"}}"#,
        )
        .unwrap();
        let renamed = load_installed_module_graph_retaining(&graph_path, None, &original).unwrap();
        assert!(renamed.module("@me/alpha").is_none());
        assert!(renamed.module("@me/replacement").is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
