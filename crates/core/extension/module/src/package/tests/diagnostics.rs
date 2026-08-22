use super::super::*;
use super::common::*;
use std::collections::HashMap;
use std::fs;

#[test]
fn graph_diagnostics_report_required_icon_without_enabled_icon_pack() {
    let root = root_with_modules(&[("@mesh/example-widget", ModuleKind::Frontend)], &[], None);
    let mut frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.icon_requirements.required = vec!["audio-volume-high".into()];

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend]).unwrap();

    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.status == "missing_required_icon")
        .unwrap();
    assert_eq!(diagnostic.module_id, "@mesh/example-widget");
    assert!(diagnostic.message.contains("required semantic icon"));
    assert!(diagnostic.message.contains("audio-volume-high"));
}

#[test]
fn graph_diagnostics_report_optional_icon_missing_mapping() {
    let root = root_with_modules(
        &[
            ("@mesh/example-widget", ModuleKind::Frontend),
            ("@mesh/icons-material", ModuleKind::IconPack),
        ],
        &[],
        None,
    );
    let mut frontend = loaded_module(
        "@mesh/example-widget",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    frontend.manifest.mesh.icon_requirements.optional = vec!["weather-clear".into()];
    let mut icon_pack = loaded_module(
        "@mesh/icons-material",
        ModuleKind::IconPack,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    icon_pack.manifest.mesh.icon_pack = Some(crate::manifest::IconPackSection {
        id: "material".into(),
        mappings: HashMap::from([(
            "audio-volume-high".into(),
            "material-symbols/volume_up".into(),
        )]),
        ..crate::manifest::IconPackSection::default()
    });

    let graph = InstalledModuleGraph::from_parts(root, vec![frontend, icon_pack]).unwrap();

    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.status == "missing_optional_icon")
        .unwrap();
    assert_eq!(diagnostic.module_id, "@mesh/example-widget");
    assert!(
        diagnostic
            .contribution_id
            .as_deref()
            .is_some_and(|id| id.contains("optional:weather-clear"))
    );
    assert!(diagnostic.message.contains("optional semantic icon"));
}

#[test]
fn graph_diagnostics_report_missing_required_binary() {
    let dep = MeshDependencies {
        binaries: vec![crate::manifest::BinaryDependency {
            name: "this-binary-definitely-does-not-exist-on-any-system-12345".into(),
            version: None,
            reason: Some("test binary".into()),
            optional: false,
            packages: HashMap::from([
                ("arch".into(), "test-bin-arch".into()),
                ("debian".into(), "test-bin-deb".into()),
            ]),
        }],
        ..MeshDependencies::default()
    };
    let root = root_with_modules(&[("@mesh/backend", ModuleKind::Backend)], &[], None);
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![loaded_module(
            "@mesh/backend",
            ModuleKind::Backend,
            dep,
            vec![],
            MeshContributes::default(),
        )],
    )
    .unwrap();
    let diagnostic = graph
        .diagnostics()
        .iter()
        .find(|d| d.status == "missing_required_binary")
        .expect("missing_required_binary diagnostic");
    assert!(diagnostic.message.contains("arch:test-bin-arch"));
    assert!(diagnostic.message.contains("debian:test-bin-deb"));
}

#[test]
fn graph_diagnostics_skip_optional_missing_binary() {
    let dep = MeshDependencies {
        binaries: vec![crate::manifest::BinaryDependency {
            name: "this-binary-definitely-does-not-exist-on-any-system-12345".into(),
            version: None,
            reason: None,
            optional: true,
            packages: Default::default(),
        }],
        ..MeshDependencies::default()
    };
    let root = root_with_modules(&[("@mesh/backend", ModuleKind::Backend)], &[], None);
    let graph = InstalledModuleGraph::from_parts(
        root,
        vec![loaded_module(
            "@mesh/backend",
            ModuleKind::Backend,
            dep,
            vec![],
            MeshContributes::default(),
        )],
    )
    .unwrap();
    assert!(
        !graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "missing_required_binary")
    );
}

#[test]
fn graph_diagnostics_report_duplicate_keybind_trigger() {
    let root = root_with_modules(
        &[
            ("@mesh/mod-a", ModuleKind::Frontend),
            ("@mesh/mod-b", ModuleKind::Frontend),
        ],
        &[],
        None,
    );
    let mut mod_a = loaded_module(
        "@mesh/mod-a",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    mod_a.manifest.mesh.keybinds.actions.insert(
        "toggle".into(),
        crate::manifest::KeybindAction {
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("t".into()),
                modifiers: vec!["ctrl".into()],
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let mut mod_b = loaded_module(
        "@mesh/mod-b",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    mod_b.manifest.mesh.keybinds.actions.insert(
        "open".into(),
        crate::manifest::KeybindAction {
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("t".into()),
                modifiers: vec!["ctrl".into()],
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let graph = InstalledModuleGraph::from_parts(root, vec![mod_a, mod_b]).unwrap();
    let dupes: Vec<_> = graph
        .diagnostics()
        .iter()
        .filter(|d| d.status == "duplicate_keybind_trigger")
        .collect();
    assert_eq!(
        dupes.len(),
        2,
        "both conflicting actions should get a diagnostic"
    );
}

#[test]
fn graph_diagnostics_report_undeclared_i18n_key() {
    let dir = temp_dir("i18n-key-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let catalog_dir = dir.join("config").join("i18n");
    fs::create_dir_all(&catalog_dir).unwrap();

    // Write a .mesh file that uses a key not present in the catalog.
    fs::write(
        src_dir.join("main.mesh"),
        r#"<template><box><text>{t('nav.volume')}{t('nav.missing')}</text></box></template>"#,
    )
    .unwrap();
    // Write catalog with only one of those keys.
    fs::write(catalog_dir.join("en.json"), r#"{"nav.volume": "Volume"}"#).unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            i18n: vec![crate::package::module_manifest::I18nContribution {
                id: "en".into(),
                locale: "en".into(),
                path: "config/i18n/en.json".into(),
                module: None,
            }],
            ..MeshContributes::default()
        },
    );
    module.manifest.mesh.i18n.default_locale = Some("en".into());
    module.path = dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();
    assert!(
        !graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.status == "undeclared_i18n_key"),
        "source authoring checks must not run during normal graph construction"
    );
    let authoring = graph.authoring_diagnostics();
    let i18n_diags: Vec<_> = authoring
        .iter()
        .filter(|d| d.status == "undeclared_i18n_key")
        .collect();
    assert_eq!(
        i18n_diags.len(),
        1,
        "exactly one undeclared key; got: {:?}",
        i18n_diags
    );
    assert!(
        i18n_diags[0].message.contains("nav.missing"),
        "diagnostic should name the missing key"
    );
}

#[test]
fn graph_diagnostics_report_unknown_shell_event_publish() {
    let dir = temp_dir("unknown-shell-event-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("main.mesh"),
        r#"
<script>
mesh.events.publish("shell.set-theme", { theme_id = "dark" })
mesh.events.publish("shell.not-declared", {})
</script>
"#,
    )
    .unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    module.path = dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();

    let authoring = graph.authoring_diagnostics();
    assert!(authoring.iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/test-frontend"
            && diagnostic.status == "unknown_shell_event_publish"
            && diagnostic.message.contains("shell.not-declared")
    }));
    assert!(!authoring.iter().any(|diagnostic| {
        diagnostic.status == "unknown_shell_event_publish"
            && diagnostic.message.contains("shell.set-theme")
    }));
}

#[test]
fn graph_diagnostics_report_keybind_subscription_contract_gaps() {
    let dir = temp_dir("keybind-subscription-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(
        src_dir.join("main.mesh"),
        r#"
<template>
  <button keybind="{this.keybinds.mute.id}" onkeybind={onMute}></button>
  <button keybind="missing" onkeybind={onMissing}></button>
  <button keybind="mute"></button>
</template>
"#,
    )
    .unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    module.path = dir.join("module.json");
    module.manifest.mesh.keybinds.actions.insert(
        "mute".into(),
        crate::manifest::KeybindAction {
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("m".into()),
                modifiers: Vec::new(),
            },
            ..crate::manifest::KeybindAction::default()
        },
    );

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();

    let authoring = graph.authoring_diagnostics();
    assert!(authoring.iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/test-frontend"
            && diagnostic.status == "undeclared_keybind_subscription"
            && diagnostic.message.contains("missing")
    }));
    assert!(authoring.iter().any(|diagnostic| {
        diagnostic.module_id == "@mesh/test-frontend"
            && diagnostic.status == "keybind_subscription_missing_handler"
            && diagnostic.message.contains("mute")
    }));
}

#[test]
fn graph_diagnostics_no_undeclared_i18n_key_when_all_present() {
    let dir = temp_dir("i18n-key-ok-test");
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let catalog_dir = dir.join("config").join("i18n");
    fs::create_dir_all(&catalog_dir).unwrap();

    fs::write(
        src_dir.join("main.mesh"),
        r#"<text>{t('nav.volume')}</text>"#,
    )
    .unwrap();
    fs::write(catalog_dir.join("en.json"), r#"{"nav.volume": "Volume"}"#).unwrap();

    let root = root_with_modules(&[("@mesh/test-frontend", ModuleKind::Frontend)], &[], None);
    let mut module = loaded_module(
        "@mesh/test-frontend",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes {
            i18n: vec![crate::package::module_manifest::I18nContribution {
                id: "en".into(),
                locale: "en".into(),
                path: "config/i18n/en.json".into(),
                module: None,
            }],
            ..MeshContributes::default()
        },
    );
    module.manifest.mesh.i18n.default_locale = Some("en".into());
    module.path = dir.join("module.json");

    let graph = InstalledModuleGraph::from_parts(root, vec![module]).unwrap();
    assert!(
        !graph
            .authoring_diagnostics()
            .iter()
            .any(|d| d.status == "undeclared_i18n_key"),
        "no undeclared_i18n_key diagnostic when all keys are in catalog"
    );
}

#[test]
fn graph_diagnostics_no_duplicate_keybind_for_unique_triggers() {
    let root = root_with_modules(
        &[
            ("@mesh/mod-a", ModuleKind::Frontend),
            ("@mesh/mod-b", ModuleKind::Frontend),
        ],
        &[],
        None,
    );
    let mut mod_a = loaded_module(
        "@mesh/mod-a",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    mod_a.manifest.mesh.keybinds.actions.insert(
        "toggle".into(),
        crate::manifest::KeybindAction {
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("t".into()),
                modifiers: vec!["ctrl".into()],
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let mut mod_b = loaded_module(
        "@mesh/mod-b",
        ModuleKind::Frontend,
        MeshDependencies::default(),
        vec![],
        MeshContributes::default(),
    );
    mod_b.manifest.mesh.keybinds.actions.insert(
        "open".into(),
        crate::manifest::KeybindAction {
            scope: crate::manifest::KeybindScope::Surface,
            trigger: crate::manifest::KeybindTrigger {
                kind: crate::manifest::KeybindTriggerKind::Shortcut,
                key: Some("o".into()),
                modifiers: vec!["ctrl".into()],
            },
            ..crate::manifest::KeybindAction::default()
        },
    );
    let graph = InstalledModuleGraph::from_parts(root, vec![mod_a, mod_b]).unwrap();
    assert!(
        !graph
            .diagnostics()
            .iter()
            .any(|d| d.status == "duplicate_keybind_trigger"),
        "different trigger keys must not generate duplicate_keybind_trigger"
    );
}
