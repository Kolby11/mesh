use super::common::*;
use super::*;

#[test]
fn installed_module_graph_exposes_shell_package_choices() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph = mesh_core_module::package::load_installed_module_graph(
        &workspace_root.join("config/module.json"),
    )
    .unwrap();

    assert_eq!(
        graph.declared_interface("mesh.audio").unwrap().module_id,
        "@mesh/audio-interface"
    );
    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
    assert!(
        graph
            .backend_providers_for_interface("mesh.audio")
            .iter()
            .any(|provider| provider.module_id == "@mesh/pulseaudio-audio")
    );
    assert!(
        graph
            .icon_pack_contributions()
            .iter()
            .any(|icon_pack| icon_pack.module_id == "@mesh/icons-default")
    );
    assert!(graph.active_provider("mesh.network").is_none());
    assert_eq!(
        graph.active_provider("mesh.power").unwrap().module_id,
        "@mesh/upower-power"
    );
    assert_eq!(
        graph.active_provider("mesh.brightness").unwrap().module_id,
        "@mesh/backlight-brightness"
    );
    assert_eq!(
        graph.backend_providers_for_interface("mesh.network").len(),
        0
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.power").len(), 1);
    assert_eq!(
        graph
            .backend_providers_for_interface("mesh.brightness")
            .len(),
        1
    );
    assert_eq!(
        graph.declared_interface("mesh.device").unwrap().module_id,
        "@mesh/device-interface"
    );
    assert_eq!(
        graph.active_provider("mesh.device").unwrap().module_id,
        "@mesh/device-info"
    );
    assert_eq!(
        graph.backend_providers_for_interface("mesh.device").len(),
        1
    );
    assert!(
        graph
            .frontend_modules()
            .iter()
            .any(|module| module.id == "@mesh/navigation-bar")
    );
    assert!(
        graph
            .frontend_modules()
            .iter()
            .all(|module| module.id != "@mesh/text-selection-proof")
    );
    assert_eq!(
        graph.module("@mesh/text-selection-proof").unwrap().enabled,
        false
    );

    let layout = graph.layout_entrypoint().unwrap();
    assert_eq!(layout.module_id, "@mesh/navigation-bar");
    assert_eq!(layout.entrypoint_id, "main");

    let mut shell = Shell::new();
    shell.register_interfaces_from_graph(&graph);
    let contracts = shell.interfaces.contracts_for("mesh.audio");
    assert!(contracts.iter().any(|contract| {
        contract.interface == "mesh.audio"
            && contract
                .state_fields
                .iter()
                .any(|field| field.name == "available")
    }));
    let providers = shell.interfaces.providers_for("mesh.audio");
    assert_eq!(providers.len(), 2);
    assert!(providers.iter().any(|provider| {
        provider.provider_module == "@mesh/pipewire-audio"
            && provider.backend_name == "pipewire"
            && provider.base_module.as_deref() == Some("@mesh/audio-interface")
    }));
    assert!(providers.iter().any(|provider| {
        provider.provider_module == "@mesh/pulseaudio-audio"
            && provider.backend_name == "pulseaudio"
            && provider.base_module.as_deref() == Some("@mesh/audio-interface")
    }));
}

#[test]
fn load_frontend_components_keeps_shell_shipped_debug_inspector_even_when_not_in_package_graph() {
    let mut shell = Shell::new();
    shell.discover_modules();
    shell.resolve_modules().unwrap();
    shell.load_frontend_components().unwrap();

    assert!(
        shell
            .components
            .iter()
            .any(|runtime| runtime.surface_id == "@mesh/debug-inspector"),
        "built-in debug inspector should load as a shell surface even when absent from config/module.json"
    );
}

#[test]
fn core_crate_boundaries_do_not_regress() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");

    let frontend = manifest_dependencies(&root.join("crates/core/frontend/compiler/Cargo.toml"));
    assert!(!frontend.contains("mesh-core-shell"));
    assert!(!frontend.contains("mesh-core-render"));
    assert!(!frontend.contains("mesh-core-presentation"));

    let frontend_host = manifest_dependencies(&root.join("crates/core/frontend/host/Cargo.toml"));
    assert!(frontend_host.contains("mesh-core-render"));
    assert!(frontend_host.contains("mesh-core-wayland"));
    assert!(!frontend_host.contains("mesh-core-shell"));
    assert!(!frontend_host.contains("mesh-core-frontend"));
    assert!(!frontend_host.contains("mesh-core-presentation"));

    let animation = manifest_dependencies(&root.join("crates/core/ui/animation/Cargo.toml"));
    assert!(animation.contains("mesh-core-elements"));
    assert!(!animation.contains("mesh-core-shell"));
    assert!(!animation.contains("mesh-core-frontend"));
    assert!(!animation.contains("mesh-core-render"));

    let interaction = manifest_dependencies(&root.join("crates/core/ui/interaction/Cargo.toml"));
    assert!(interaction.contains("mesh-core-elements"));
    assert!(!interaction.contains("mesh-core-shell"));
    assert!(!interaction.contains("mesh-core-render"));
    assert!(!interaction.contains("mesh-core-presentation"));

    let render = manifest_dependencies(&root.join("crates/core/frontend/render/Cargo.toml"));
    assert!(render.contains("mesh-core-elements"));
    assert!(render.contains("mesh-core-icon"));
    assert!(!render.contains("mesh-core-shell"));
    assert!(!render.contains("mesh-core-frontend"));
    assert!(!render.contains("mesh-core-presentation"));

    let presentation = manifest_dependencies(&root.join("crates/core/presentation/Cargo.toml"));
    assert!(presentation.contains("mesh-core-render"));
    assert!(presentation.contains("mesh-core-wayland"));
    assert!(!presentation.contains("mesh-core-shell"));
    assert!(!presentation.contains("mesh-core-frontend"));

    let surface_config = manifest_dependencies(&root.join("crates/core/surface-config/Cargo.toml"));
    assert!(surface_config.contains("mesh-core-module"));
    assert!(surface_config.contains("mesh-core-wayland"));
    assert!(!surface_config.contains("mesh-core-shell"));
    assert!(!surface_config.contains("mesh-core-render"));
}
