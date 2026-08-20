use super::super::*;
use super::common::*;
use std::fs;
use std::path::PathBuf;

#[test]
fn shipped_navigation_manifest_uses_explicit_localized_keybind_text() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/frontend/navigation-bar");
    let loaded = load_module_manifest(&dir).unwrap();

    assert!(
        loaded.diagnostics.iter().all(|diagnostic| {
            !diagnostic
                .message
                .contains("looks like an i18n key but is a raw literal string")
        }),
        "shipped navigation manifest should not use ambiguous raw i18n keys: {:?}",
        loaded.diagnostics
    );
    assert_eq!(
        loaded.manifest.mesh.i18n.default_locale.as_deref(),
        Some("en")
    );
    // supportedLocales removed from navigation-bar; locales declared once via provides.i18n
    assert!(loaded.manifest.mesh.i18n.supported_locales.is_empty());
    assert!(
        loaded
            .manifest
            .mesh
            .contributes
            .i18n
            .iter()
            .any(|entry| entry.locale == "en" && entry.path == "config/i18n/en.json")
    );
    assert!(
        loaded
            .manifest
            .mesh
            .contributes
            .i18n
            .iter()
            .any(|entry| entry.locale == "sk" && entry.path == "config/i18n/sk.json")
    );

    let action = loaded
        .manifest
        .mesh
        .keybinds
        .actions
        .get("mute")
        .expect("navigation mute keybind");
    assert_eq!(
        action.label,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.label".into(),
            fallback: "Mute audio".into(),
        })
    );
    assert_eq!(
        action.description,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.description".into(),
            fallback: "Toggle audio mute".into(),
        })
    );
    assert_eq!(
        action.category,
        Some(crate::manifest::LocalizedText::Translation {
            key: "keybind.category.audio".into(),
            fallback: "Audio".into(),
        })
    );
}

#[test]
fn shipped_module_graph_loads_repo_module_fixture() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let graph = load_installed_module_graph(&workspace_root.join("config/module.json")).unwrap();

    assert_eq!(graph.frontend_modules().len(), 5);
    assert!(
        graph
            .frontend_modules()
            .into_iter()
            .any(|module| module.id == "@mesh/settings")
    );
    assert!(
        graph
            .frontend_modules()
            .into_iter()
            .any(|module| module.id == "@mesh/composition-editor")
    );
    let component_ids: std::collections::HashSet<_> = graph
        .modules_by_kind(ModuleKind::Component)
        .into_iter()
        .map(|module| module.id.as_str())
        .collect();
    assert_eq!(component_ids.len(), 4);
    assert!(component_ids.contains("@mesh/audio-popover"));
    assert!(component_ids.contains("@mesh/quick-settings"));
    assert!(component_ids.contains("@mesh/theme-selector"));
    assert!(graph.module("@mesh/language-popover").unwrap().enabled);
    assert_eq!(
        graph
            .module("@mesh/navigation-bar")
            .unwrap()
            .manifest_source,
        ModuleManifestSource::CanonicalModuleJson
    );
    assert_eq!(
        graph
            .module("@mesh/audio-interface")
            .unwrap()
            .manifest_source,
        ModuleManifestSource::CanonicalModuleJson
    );
    assert_eq!(
        graph.module("@mesh/icons-default").unwrap().manifest_source,
        ModuleManifestSource::CanonicalModuleJson
    );
    assert_eq!(
        graph.declared_interface("mesh.audio").unwrap().module_id,
        "@mesh/audio-interface"
    );
    assert!(
        graph
            .interface_contract("mesh.audio")
            .unwrap()
            .methods
            .iter()
            .any(|method| method.name == "play_sound")
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
    assert!(
        graph
            .backend_providers_for_interface("mesh.audio")
            .iter()
            .any(|provider| provider.module_id == "@mesh/pulseaudio-audio")
    );
    let layout = graph.layout_entrypoint().unwrap();
    assert_eq!(layout.module_id, "@mesh/navigation-bar");
    assert_eq!(layout.entrypoint_id, "main");
    assert!(graph.frontend_entrypoints().iter().any(|entrypoint| {
        entrypoint.module_id == "@mesh/navigation-bar"
            && entrypoint.source.local_id == "main"
            && entrypoint.path == "src/main.mesh"
    }));
    assert!(
        graph
            .settings_schemas()
            .iter()
            .any(|settings| settings.namespace == "@mesh/navigation-bar")
    );
    let navigation_settings = graph
        .settings_schemas()
        .iter()
        .find(|settings| settings.namespace == "@mesh/navigation-bar")
        .expect("navigation props schema");
    assert_eq!(
        navigation_settings.schema["properties"]["blur_enabled"]["type"],
        serde_json::json!("bool")
    );
    assert_eq!(
        navigation_settings.schema["properties"]["blur_radius"]["type"],
        serde_json::json!("size")
    );
    assert!(
        graph
            .keybind_actions()
            .iter()
            .any(|keybind| keybind.module_id == "@mesh/navigation-bar"
                && keybind.action_id == "mute")
    );
    assert!(
        graph
            .icon_requirements()
            .iter()
            .any(|icon| icon.module_id == "@mesh/navigation-bar"
                && icon.name == "audio-volume-high"
                && icon.required)
    );
    assert!(
        graph
            .icon_pack_contributions()
            .iter()
            .any(|icon_pack| icon_pack.module_id == "@mesh/icons-default"
                && icon_pack.id == "default")
    );
}

#[test]
fn shipped_module_graph_preserves_navigation_localized_keybind_text() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let graph = load_installed_module_graph(&workspace_root.join("config/module.json")).unwrap();
    let keybind = graph
        .keybind_actions()
        .iter()
        .find(|keybind| keybind.module_id == "@mesh/navigation-bar" && keybind.action_id == "mute")
        .expect("navigation mute keybind contribution");

    assert_eq!(
        keybind.label.as_ref(),
        Some(&crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.label".into(),
            fallback: "Mute audio".into(),
        })
    );
    assert_eq!(
        keybind.description.as_ref(),
        Some(&crate::manifest::LocalizedText::Translation {
            key: "keybind.mute.description".into(),
            fallback: "Toggle audio mute".into(),
        })
    );
    assert_eq!(
        keybind.category.as_ref(),
        Some(&crate::manifest::LocalizedText::Translation {
            key: "keybind.category.audio".into(),
            fallback: "Audio".into(),
        })
    );
}

#[test]
fn shipped_module_diagnostics_report_missing_navigation_icon() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let mut navigation =
        load_module_manifest(&workspace_root.join("modules/frontend/navigation-bar")).unwrap();
    // This test isolates icon diagnostics from the graph's required-module
    // activation gate.
    navigation.manifest.mesh.uses.modules.clear();
    navigation.manifest.mesh.dependencies.modules.clear();
    navigation
        .manifest
        .mesh
        .icon_requirements
        .required
        .push("missing-shipped-proof-icon".into());
    let icons = load_module_manifest(&workspace_root.join("modules/icon-packs/default")).unwrap();
    let root = root_with_modules(
        &[
            ("@mesh/navigation-bar", ModuleKind::Frontend),
            ("@mesh/icons-default", ModuleKind::IconPack),
        ],
        &[],
        None,
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![navigation, icons]).unwrap();
    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.status == "missing_required_icon")
        .unwrap();

    assert_eq!(diagnostic.module_id, "@mesh/navigation-bar");
    assert!(
        diagnostic
            .contribution_id
            .as_deref()
            .is_some_and(|id| id.contains("required:missing-shipped-proof-icon"))
    );
}

#[test]
fn shipped_frontend_icon_literals_are_declared() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let module_dir = workspace_root.join("modules/frontend/navigation-bar");
    let loaded = load_module_manifest(&module_dir).unwrap();
    let declared = loaded
        .manifest
        .mesh
        .icon_requirements
        .required
        .iter()
        .chain(loaded.manifest.mesh.icon_requirements.optional.iter())
        .collect::<std::collections::HashSet<_>>();

    for source_path in [
        module_dir.join("src/main.mesh"),
        module_dir.join("src/components/battery-button.mesh"),
        module_dir.join("src/components/volume-button.mesh"),
        module_dir.join("src/components/now-playing.mesh"),
        module_dir.join("src/components/settings-button.mesh"),
        module_dir.join("src/components/theme-button.mesh"),
    ] {
        let source = fs::read_to_string(&source_path).unwrap();
        for icon in obvious_semantic_icon_literals(&source) {
            assert!(
                declared.contains(&icon),
                "{} uses semantic icon '{icon}' but @mesh/navigation-bar does not declare it in iconRequirements",
                source_path.display()
            );
        }
    }
}

#[test]
fn shipped_frontend_translation_keys_are_declared() {
    // End-to-end guard for the Luau scan. The substring scanner this replaced
    // matched the `t(` inside `string.format(`, so shipped modules reported
    // format strings such as `%d%%` as undeclared translation keys. Every key
    // the scanner now reports must be a real key in the module's own catalog.
    use crate::package::installed_graph::extract_t_keys_from_mesh_source;

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let frontend_root = workspace_root.join("modules/frontend");
    let mut checked_modules = 0;
    let mut checked_keys = 0;

    for entry in fs::read_dir(&frontend_root).unwrap().flatten() {
        let module_dir = entry.path();
        if !module_dir.is_dir() {
            continue;
        }
        let Ok(loaded) = load_module_manifest(&module_dir) else {
            continue;
        };
        let Some(default_locale) = loaded.manifest.mesh.i18n.default_locale.clone() else {
            continue;
        };
        let Some(contribution) = loaded
            .manifest
            .mesh
            .contributes
            .i18n
            .iter()
            .find(|contribution| contribution.locale == default_locale)
        else {
            continue;
        };
        let Ok(catalog) = fs::read_to_string(module_dir.join(&contribution.path)) else {
            continue;
        };
        let catalog: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&catalog).unwrap();
        checked_modules += 1;

        let mut sources = Vec::new();
        collect_mesh_files(&module_dir.join("src"), &mut sources);
        for source_path in sources {
            let content = fs::read_to_string(&source_path).unwrap();
            for key in extract_t_keys_from_mesh_source(&content) {
                checked_keys += 1;
                assert!(
                    catalog.contains_key(&key),
                    "{} calls t('{key}') but it is not in the '{default_locale}' catalog",
                    source_path.display()
                );
            }
        }
    }

    assert!(
        checked_modules > 0 && checked_keys > 0,
        "expected to check real shipped modules; got {checked_modules} modules, {checked_keys} keys"
    );
}

// cargo test -p mesh-core-module --release -- shipped_module_luau_scan_cost --ignored --nocapture
#[test]
#[ignore = "release-only graph-scan cost measurement"]
fn shipped_module_luau_scan_cost() {
    use crate::package::installed_graph::scan_mesh_source;
    use std::time::Instant;

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let mut sources = Vec::new();
    collect_mesh_files(&workspace_root.join("modules"), &mut sources);
    let contents: Vec<String> = sources
        .iter()
        .map(|path| fs::read_to_string(path).unwrap())
        .collect();
    let lines: usize = contents.iter().map(|c| c.lines().count()).sum();

    use rayon::prelude::*;
    let shared_started = Instant::now();
    let shared_found: usize = contents
        .par_iter()
        .map(|content| {
            let scan = scan_mesh_source(content);
            scan.icon_names.len()
                + scan.static_calls.t_keys.len()
                + scan.static_calls.publish_channels.len()
                + scan.keybind_subscriptions.len()
        })
        .sum();
    let shared = shared_started.elapsed();

    eprintln!(
        "scanned {} .mesh files ({lines} lines), {shared_found} static scan facts: shared parse {shared:?}",
        contents.len()
    );
}
