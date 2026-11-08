use super::common::*;
use super::*;

#[test]
fn material_profile_switch_retires_its_roots_and_restores_them() {
    let dir = tempfile::tempdir().unwrap();
    let (_settings, _graph, _lock) = isolated_settings_file(dir.path());
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let mut graph: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("config/module.json")).unwrap()).unwrap();
    graph["mesh"]["modulesDir"] =
        std::path::PathBuf::from("../".repeat(dir.path().ancestors().count() - 1))
            .join(root.strip_prefix("/").unwrap())
            .join("modules")
            .to_string_lossy()
            .into_owned()
            .into();
    std::fs::write(
        dir.path().join("module.json"),
        serde_json::to_vec(&graph).unwrap(),
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("profiles")).unwrap();
    for profile in ["material", "orbit"] {
        std::fs::copy(
            root.join(format!("config/profiles/{profile}.json")),
            dir.path().join(format!("profiles/{profile}.json")),
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("active-profile"), "material\n").unwrap();
    let mut shell = Shell::new();
    // The real audio, network, power and window-manager providers cannot start
    // in a unit test; this test is about which roots a switch retires.
    shell.replay = Some(super::super::replay_hooks::ReplayHarness {
        suppress_backends: true,
        driver: Box::new(|_| super::super::replay_hooks::ReplayStep { wake_at: None }),
        wake_at: None,
    });
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    shell.discover_modules();
    shell.resolve_modules().unwrap();
    shell.load_frontend_components().unwrap();
    let mut requests = shell.mount_components().unwrap();
    shell.drain_requests(&mut requests).unwrap();
    assert!(
        shell
            .components
            .iter()
            .any(|runtime| runtime.surface_id == "@mesh/material-workspace#default")
    );
    for (profile, retired) in [
        ("orbit", "@mesh/material-workspace#default"),
        ("material", "@mesh/orbit-bar#default"),
    ] {
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let mut requests = shell.apply_switch_profile_with_ack(profile, Some(sender));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            shell.drain_requests(&mut requests).unwrap();
            if shell.pending_resource_preparation.is_none()
                && shell.pending_profile_switch.is_none()
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{profile} switch did not settle"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
            requests.extend(shell.poll_pending_resource_preparation());
        }
        let result = receiver.try_recv().unwrap();
        assert!(
            matches!(
                result,
                super::super::types::IpcProfileSwitchResponse::Committed { .. }
            ),
            "{profile}: {result:?}"
        );
        assert!(
            !shell
                .components
                .iter()
                .any(|runtime| runtime.surface_id == retired)
        );
        assert!(!shell.core.surfaces.contains_key(retired));
        let active = shell.active_snapshot().unwrap();
        assert!(!active.roots().contains_key(retired));
    }
    assert!(
        shell
            .components
            .iter()
            .any(|runtime| runtime.surface_id == "@mesh/material-workspace#default")
    );
}
