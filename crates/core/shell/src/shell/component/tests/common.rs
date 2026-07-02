use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::FrontendSurfaceComponent;
use crate::shell::component::catalog::FrontendCatalogEntry;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_component::parse_component;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::style::Display;
use mesh_core_frontend::CompiledFrontendModule;
use mesh_core_module::manifest::{
    CapabilitiesSection, CompatibilitySection, DependenciesSection, EntrypointsSection,
};
use mesh_core_module::{ExportsSection, Manifest, ModuleSection, ModuleType};
use mesh_core_scripting::ScriptContext;
use mesh_core_service::contract::ContractStateField;
use mesh_core_service::{
    ContractCapabilities, InterfaceArgument, InterfaceCatalog, InterfaceContract, InterfaceEvent,
    InterfaceMethod, InterfaceProvider, parse_contract_version,
};
use mesh_core_theme::{Theme, default_theme};
use std::collections::HashMap;
use std::path::PathBuf;

pub(super) fn audio_network_catalog() -> InterfaceCatalog {
    let mut catalog = InterfaceCatalog::default();
    catalog.register_contract(InterfaceContract {
        interface: "mesh.audio".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        state_fields: Vec::new(),
        methods: vec![
            InterfaceMethod {
                name: "set_volume".into(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "volume".into(),
                        arg_type: "float".into(),
                    },
                ],
                returns: None,
                coalesce: false,
            },
            InterfaceMethod {
                name: "set_muted".into(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "muted".into(),
                        arg_type: "boolean".into(),
                    },
                ],
                returns: None,
                coalesce: false,
            },
            InterfaceMethod {
                name: "volume_up".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
            },
            InterfaceMethod {
                name: "volume_down".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
            },
            InterfaceMethod {
                name: "toggle_mute".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
            },
        ],
        events: vec![InterfaceEvent {
            name: "VolumeChanged".into(),
            payload: Some("{ device_id: string, level: float }".into()),
        }],
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.audio".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/audio-interface".into()),
        provider_module: "@mesh/pipewire-audio".into(),
        backend_name: "PipeWire".into(),
        priority: 100,
    });
    catalog.register_contract(InterfaceContract {
        interface: "mesh.network".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        state_fields: Vec::new(),
        methods: vec![
            InterfaceMethod {
                name: "set_wifi_enabled".into(),
                args: vec![InterfaceArgument {
                    name: "enabled".into(),
                    arg_type: "bool".into(),
                }],
                returns: None,
                coalesce: false,
            },
            InterfaceMethod {
                name: "connect".into(),
                args: vec![InterfaceArgument {
                    name: "connection_id".into(),
                    arg_type: "string".into(),
                }],
                returns: None,
                coalesce: false,
            },
        ],
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.network".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/network-interface".into()),
        provider_module: "@mesh/networkmanager-network".into(),
        backend_name: "NetworkManager".into(),
        priority: 100,
    });
    catalog.register_contract(InterfaceContract {
        interface: "mesh.theme".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        state_fields: Vec::new(),
        methods: Vec::new(),
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.theme".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/theme-interface".into()),
        provider_module: "@mesh/shell-theme".into(),
        backend_name: "Shell Theme".into(),
        priority: 100,
    });
    catalog
}

pub(super) fn audio_network_power_catalog() -> InterfaceCatalog {
    let mut catalog = audio_network_catalog();
    catalog.register_contract(InterfaceContract {
        interface: "mesh.power".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        state_fields: Vec::new(),
        methods: Vec::new(),
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.power".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/power-interface".into()),
        provider_module: "@mesh/upower-power".into(),
        backend_name: "UPower".into(),
        priority: 100,
    });
    catalog
}

/// Catalog covering every interface the shipped navigation-bar children
/// `require(...)`: audio, network, theme, power, plus brightness, hyprland, and
/// media. Without these the brightness/now-playing/etc. child components fail to
/// resolve their interface and render wide error text, overflowing the bar so
/// the right-cluster buttons land off-surface and become un-hit-testable.
pub(super) fn navigation_bar_catalog() -> InterfaceCatalog {
    let mut catalog = audio_network_power_catalog();
    for (interface, base_module, provider_module, backend_name) in [
        (
            "mesh.brightness",
            "@mesh/brightness-interface",
            "@mesh/backlight-brightness",
            "Backlight",
        ),
        (
            "mesh.hyprland",
            "@mesh/hyprland-interface",
            "@mesh/hyprland-wm",
            "Hyprland",
        ),
        (
            "mesh.media",
            "@mesh/media-interface",
            "@mesh/mpris-media",
            "MPRIS",
        ),
    ] {
        catalog.register_contract(InterfaceContract {
            interface: interface.into(),
            version: parse_contract_version("1.0").unwrap(),
            file_path: PathBuf::from("<test>"),
            state_fields: Vec::new(),
            methods: Vec::new(),
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        catalog.register_provider(InterfaceProvider {
            interface: interface.into(),
            version: Some("1.0".into()),
            base_module: Some(base_module.into()),
            provider_module: provider_module.into(),
            backend_name: backend_name.into(),
            priority: 100,
        });
    }
    catalog
}

pub(super) fn debug_catalog() -> InterfaceCatalog {
    let mut catalog = InterfaceCatalog::default();
    catalog.register_contract(InterfaceContract {
        interface: "mesh.debug".into(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("<test>"),
        state_fields: vec![
            ContractStateField {
                name: "overlay_enabled".into(),
                field_type: "boolean".into(),
                description: None,
            },
            ContractStateField {
                name: "layout_bounds_enabled".into(),
                field_type: "boolean".into(),
                description: None,
            },
            ContractStateField {
                name: "profiling_enabled".into(),
                field_type: "boolean".into(),
                description: None,
            },
            ContractStateField {
                name: "profiling_session_id".into(),
                field_type: "integer".into(),
                description: None,
            },
            ContractStateField {
                name: "active_view".into(),
                field_type: "string".into(),
                description: None,
            },
            ContractStateField {
                name: "modules".into(),
                field_type: "array".into(),
                description: None,
            },
            ContractStateField {
                name: "module_graph".into(),
                field_type: "array".into(),
                description: None,
            },
            ContractStateField {
                name: "interfaces".into(),
                field_type: "array".into(),
                description: None,
            },
            ContractStateField {
                name: "backend_runtimes".into(),
                field_type: "array".into(),
                description: None,
            },
            ContractStateField {
                name: "active_surfaces".into(),
                field_type: "array".into(),
                description: None,
            },
            ContractStateField {
                name: "benchmarks".into(),
                field_type: "object".into(),
                description: None,
            },
            ContractStateField {
                name: "profiling".into(),
                field_type: "object".into(),
                description: None,
            },
        ],
        methods: Vec::new(),
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    });
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.debug".into(),
        version: Some("1.0".into()),
        base_module: Some("@mesh/debug".into()),
        provider_module: "@mesh/core-debug".into(),
        backend_name: "Shell".into(),
        priority: 100,
    });
    catalog
}

pub(super) fn make_audio_ctx() -> ScriptContext {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.audio.control"));
    let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
    ctx.set_interface_catalog(audio_network_catalog());
    ctx
}

pub(super) fn make_network_ctx() -> ScriptContext {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.network.read"));
    caps.grant(Capability::new("service.network.control"));
    let mut ctx = ScriptContext::new("@mesh/quick-settings", caps).unwrap();
    ctx.set_interface_catalog(audio_network_catalog());
    ctx
}

pub(super) fn make_panel_ctx() -> ScriptContext {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.network.read"));
    caps.grant(Capability::new("service.power.read"));
    let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
    ctx.set_interface_catalog(audio_network_power_catalog());
    ctx
}

pub(super) fn shipped_component_script(source: &str) -> String {
    parse_component(source)
        .unwrap()
        .script
        .expect("shipped component should contain a script block")
        .source
}

pub(super) fn assert_no_legacy_service_callbacks(source_name: &str, source: &str) {
    for forbidden in ["mesh.service.bind", "mesh.service.on", ".on_change("] {
        assert!(
            !source.contains(forbidden),
            "{source_name} must not teach or use legacy service callback API {forbidden}"
        );
    }
}

pub(super) fn minimal_test_manifest(id: &str) -> Manifest {
    Manifest {
        package: ModuleSection {
            id: id.to_string(),
            name: None,
            version: "0.1.0".into(),
            module_type: ModuleType::Surface,
            api_version: "0.1".into(),
            license: None,
            description: None,
            authors: Vec::new(),
            repository: None,
        },
        compatibility: CompatibilitySection::default(),
        dependencies: DependenciesSection::default(),
        capabilities: CapabilitiesSection::default(),
        entrypoints: EntrypointsSection {
            main: Some("src/main.mesh".into()),
            settings_ui: None,
        },
        accessibility: None,
        keybinds: mesh_core_module::KeybindsSection::default(),
        i18n: None,
        theme: None,
        service: None,
        provides: Vec::new(),
        interface: None,
        extensions: Vec::new(),
        exports: ExportsSection::default(),
        provides_slots: HashMap::new(),
        slot_contributions: HashMap::new(),
        assets: None,
        icons: None,
        icon_pack: None,
        icon_requirements: mesh_core_module::IconRequirementsSection::default(),
        translations: HashMap::new(),
        surface_layout: None,
    }
}

pub(super) fn test_frontend_component(source: &str) -> FrontendSurfaceComponent {
    test_frontend_component_with_catalog(source, InterfaceCatalog::default(), &[])
}

pub(super) fn test_frontend_component_with_manifest(
    source: &str,
    manifest: Manifest,
) -> FrontendSurfaceComponent {
    let component_id = manifest.package.id.clone();
    let compiled = CompiledFrontendModule {
        manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(source).unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::new(),
        slot_contributions: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
    );
    component
        .mount(ComponentContext {
            component_id: component_id.clone(),
            surface_id: component_id.clone(),
            diagnostics: Diagnostics::new(component_id),
        })
        .unwrap();
    component.visible = true;
    component
}

pub(super) fn buffer_pixel(buffer: &PixelBuffer, x: u32, y: u32) -> [u8; 4] {
    let offset = (y * buffer.stride + x * 4) as usize;
    [
        buffer.data[offset],
        buffer.data[offset + 1],
        buffer.data[offset + 2],
        buffer.data[offset + 3],
    ]
}

pub(super) fn themed_primary(id: &str, primary_hex: &str) -> Theme {
    let mut theme = default_theme();
    theme.id = id.to_string();
    theme.name = id.to_string();
    theme.tokens.insert(
        "color.primary".into(),
        mesh_core_theme::TokenValue::String(primary_hex.to_string()),
    );
    theme
}

pub(super) fn test_theme(id: &str) -> Theme {
    mesh_core_theme::load_theme_from_path(&mesh_core_theme::theme_path_for_id(id))
        .unwrap_or_else(|err| panic!("failed to load test theme {id}: {err}"))
}

pub(super) fn test_frontend_component_with_required_icons(
    source: &str,
    required_icons: &[&str],
) -> FrontendSurfaceComponent {
    let mut manifest = minimal_test_manifest("@test/reactive-surface");
    manifest.icon_requirements.required = required_icons
        .iter()
        .map(|semantic_name| (*semantic_name).to_string())
        .collect();
    let compiled = CompiledFrontendModule {
        manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(source).unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::new(),
        slot_contributions: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
    );
    component
        .mount(ComponentContext {
            component_id: "@test/reactive-surface".into(),
            surface_id: "@test/reactive-surface".into(),
            diagnostics: Diagnostics::new("@test/reactive-surface"),
        })
        .unwrap();
    component.visible = true;
    component
}

pub(super) fn test_frontend_component_with_catalog(
    source: &str,
    interface_catalog: InterfaceCatalog,
    required_capabilities: &[&str],
) -> FrontendSurfaceComponent {
    let manifest = minimal_test_manifest("@test/reactive-surface");
    let mut manifest = manifest;
    manifest.capabilities.required = required_capabilities
        .iter()
        .map(|capability| (*capability).to_string())
        .collect();
    let compiled = CompiledFrontendModule {
        manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(source).unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::new(),
        slot_contributions: HashMap::new(),
    };
    let mut component =
        FrontendSurfaceComponent::new(compiled, PathBuf::from("."), catalog, interface_catalog);
    component
        .mount(ComponentContext {
            component_id: "@test/reactive-surface".into(),
            surface_id: "@test/reactive-surface".into(),
            diagnostics: Diagnostics::new("@test/reactive-surface"),
        })
        .unwrap();
    component.visible = true;
    component
}

pub(super) fn real_frontend_module_component(
    module_id: &str,
    interface_catalog: InterfaceCatalog,
) -> FrontendSurfaceComponent {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let navigation_dir = root.join("modules/frontend/navigation-bar");
    let audio_popover_dir = root.join("modules/frontend/audio-popover");
    let language_popover_dir = root.join("modules/frontend/language-popover");
    let theme_selector_dir = root.join("modules/frontend/theme-selector");
    let debug_inspector_dir = root.join("modules/frontend/debug-inspector");
    let settings_dir = root.join("modules/frontend/settings");

    let navigation_manifest = mesh_core_module::manifest::load_manifest(&navigation_dir)
        .expect("navigation manifest")
        .manifest;
    let audio_popover_manifest = mesh_core_module::manifest::load_manifest(&audio_popover_dir)
        .expect("audio manifest")
        .manifest;
    let language_popover_manifest =
        mesh_core_module::manifest::load_manifest(&language_popover_dir)
            .expect("language popover manifest")
            .manifest;
    let theme_selector_manifest = mesh_core_module::manifest::load_manifest(&theme_selector_dir)
        .expect("theme selector manifest")
        .manifest;
    let debug_inspector_manifest = mesh_core_module::manifest::load_manifest(&debug_inspector_dir)
        .expect("debug inspector manifest")
        .manifest;
    let settings_manifest = mesh_core_module::manifest::load_manifest(&settings_dir)
        .expect("settings manifest")
        .manifest;

    let navigation_compiled = CompiledFrontendModule {
        manifest: navigation_manifest,
        source_path: navigation_dir.join("src/main.mesh"),
        component: parse_component(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/navigation-bar/src/main.mesh"
        )))
        .unwrap(),
        local_components: HashMap::from([
            (
                "BatteryButton".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/battery-button.mesh"
                )))
                .unwrap(),
            ),
            (
                "MetaLabel".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/meta-label.mesh"
                )))
                .unwrap(),
            ),
            (
                "MetaPill".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/meta-pill.mesh"
                )))
                .unwrap(),
            ),
            (
                "SettingsButton".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/settings-button.mesh"
                )))
                .unwrap(),
            ),
            (
                "VolumeButton".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/volume-button.mesh"
                )))
                .unwrap(),
            ),
            (
                "ThemeButton".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/theme-button.mesh"
                )))
                .unwrap(),
            ),
            (
                "LanguageButton".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/language-button.mesh"
                )))
                .unwrap(),
            ),
            (
                "BrightnessButton".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/brightness-button.mesh"
                )))
                .unwrap(),
            ),
            (
                "ClockButton".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/clock-button.mesh"
                )))
                .unwrap(),
            ),
            (
                "NowPlaying".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/now-playing.mesh"
                )))
                .unwrap(),
            ),
            (
                "WindowTitle".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/window-title.mesh"
                )))
                .unwrap(),
            ),
            (
                "WorkspaceList".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/navigation-bar/src/components/workspace-list.mesh"
                )))
                .unwrap(),
            ),
        ]),
        module_component_imports: HashMap::from([
            ("AudioPopover".into(), "@mesh/audio-popover".into()),
            ("LanguagePopover".into(), "@mesh/language-popover".into()),
            ("ThemeSelector".into(), "@mesh/theme-selector".into()),
            ("QuickSettings".into(), "@mesh/quick-settings".into()),
            ("Settings".into(), "@mesh/settings".into()),
        ]),
        watched_paths: Vec::new(),
    };
    let audio_popover_compiled = CompiledFrontendModule {
        manifest: audio_popover_manifest,
        source_path: audio_popover_dir.join("src/main.mesh"),
        component: parse_component(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/audio-popover/src/main.mesh"
        )))
        .unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let language_popover_compiled = CompiledFrontendModule {
        manifest: language_popover_manifest,
        source_path: language_popover_dir.join("src/main.mesh"),
        component: parse_component(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/language-popover/src/main.mesh"
        )))
        .unwrap(),
        local_components: HashMap::from([(
            "BubbleOptions".into(),
            parse_component(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../modules/frontend/shared/components/bubble-options.mesh"
            )))
            .unwrap(),
        )]),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let theme_selector_compiled = CompiledFrontendModule {
        manifest: theme_selector_manifest,
        source_path: theme_selector_dir.join("src/main.mesh"),
        component: parse_component(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/theme-selector/src/main.mesh"
        )))
        .unwrap(),
        local_components: HashMap::from([(
            "BubbleOptions".into(),
            parse_component(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../modules/frontend/shared/components/bubble-options.mesh"
            )))
            .unwrap(),
        )]),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let debug_inspector_compiled = CompiledFrontendModule {
        manifest: debug_inspector_manifest,
        source_path: debug_inspector_dir.join("src/main.mesh"),
        component: parse_component(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/debug-inspector/src/main.mesh"
        )))
        .unwrap(),
        local_components: HashMap::from([
            (
                "ViewTabs".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/debug-inspector/src/components/view-tabs.mesh"
                )))
                .unwrap(),
            ),
            (
                "OverviewView".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/debug-inspector/src/components/overview-view.mesh"
                )))
                .unwrap(),
            ),
            (
                "ModulesView".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/debug-inspector/src/components/modules-view.mesh"
                )))
                .unwrap(),
            ),
            (
                "SurfacesView".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/debug-inspector/src/components/surfaces-view.mesh"
                )))
                .unwrap(),
            ),
            (
                "BackendServicesView".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/debug-inspector/src/components/backend-services-view.mesh"
                )))
                .unwrap(),
            ),
            (
                "BenchmarkView".into(),
                parse_component(include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../modules/frontend/debug-inspector/src/components/benchmark-view.mesh"
                )))
                .unwrap(),
            ),
        ]),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let settings_compiled = CompiledFrontendModule {
        manifest: settings_manifest,
        source_path: settings_dir.join("src/main.mesh"),
        component: parse_component(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../modules/frontend/settings/src/main.mesh"
        )))
        .unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };

    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                "@mesh/navigation-bar".into(),
                FrontendCatalogEntry {
                    module_dir: navigation_dir.clone(),
                    compiled: navigation_compiled.clone(),
                },
            ),
            (
                "@mesh/audio-popover".into(),
                FrontendCatalogEntry {
                    module_dir: audio_popover_dir.clone(),
                    compiled: audio_popover_compiled.clone(),
                },
            ),
            (
                "@mesh/language-popover".into(),
                FrontendCatalogEntry {
                    module_dir: language_popover_dir.clone(),
                    compiled: language_popover_compiled.clone(),
                },
            ),
            (
                "@mesh/theme-selector".into(),
                FrontendCatalogEntry {
                    module_dir: theme_selector_dir.clone(),
                    compiled: theme_selector_compiled.clone(),
                },
            ),
            (
                "@mesh/debug-inspector".into(),
                FrontendCatalogEntry {
                    module_dir: debug_inspector_dir.clone(),
                    compiled: debug_inspector_compiled.clone(),
                },
            ),
            (
                "@mesh/settings".into(),
                FrontendCatalogEntry {
                    module_dir: settings_dir.clone(),
                    compiled: settings_compiled.clone(),
                },
            ),
        ]),
        slot_contributions: HashMap::new(),
    };

    // Mirror the shell's graph i18n wiring so component `t(...)` calls resolve
    // against each module's `config/i18n/<locale>.json`, including child
    // components. Without this, `t("nav.volume_level")` would surface the raw
    // key instead of the translated string. Built before the dirs are moved
    // into the (compiled, module_dir) selection below.
    let mut graph_i18n_catalogs: Vec<(String, String, PathBuf)> = Vec::new();
    for (id, dir) in [
        ("@mesh/navigation-bar", &navigation_dir),
        ("@mesh/audio-popover", &audio_popover_dir),
        ("@mesh/language-popover", &language_popover_dir),
        ("@mesh/theme-selector", &theme_selector_dir),
        ("@mesh/debug-inspector", &debug_inspector_dir),
        ("@mesh/settings", &settings_dir),
    ] {
        let i18n_dir = dir.join("config/i18n");
        let Ok(entries) = std::fs::read_dir(&i18n_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Some(locale) = path.file_stem().and_then(|stem| stem.to_str()) {
                graph_i18n_catalogs.push((id.to_string(), locale.to_string(), path.clone()));
            }
        }
    }

    let (compiled, module_dir) = if module_id == "@mesh/audio-popover" {
        (audio_popover_compiled, audio_popover_dir)
    } else if module_id == "@mesh/language-popover" {
        (language_popover_compiled, language_popover_dir)
    } else if module_id == "@mesh/theme-selector" {
        (theme_selector_compiled, theme_selector_dir)
    } else if module_id == "@mesh/debug-inspector" {
        (debug_inspector_compiled, debug_inspector_dir)
    } else if module_id == "@mesh/settings" {
        (settings_compiled, settings_dir)
    } else {
        (navigation_compiled, navigation_dir)
    };

    let mut component =
        FrontendSurfaceComponent::new(compiled, module_dir, catalog, interface_catalog)
            .with_graph_i18n_catalogs(graph_i18n_catalogs);
    component
        .mount(ComponentContext {
            component_id: module_id.into(),
            surface_id: module_id.into(),
            diagnostics: Diagnostics::new(module_id),
        })
        .unwrap();
    component.visible = true;
    component
}

pub(super) fn runtime_value(
    component: &FrontendSurfaceComponent,
    name: &str,
) -> Option<serde_json::Value> {
    component
        .runtimes
        .lock()
        .unwrap()
        .get(component.id())
        .and_then(|runtime| runtime.script_ctx.state().get(name))
}

pub(super) fn runtime_number(component: &FrontendSurfaceComponent, name: &str) -> f64 {
    runtime_value(component, name)
        .and_then(|value| value.as_f64())
        .unwrap_or_else(|| panic!("expected numeric runtime value for {name}"))
}

pub(super) fn runtime_bool(component: &FrontendSurfaceComponent, name: &str) -> bool {
    runtime_value(component, name)
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| panic!("expected boolean runtime value for {name}"))
}

pub(super) fn rendered_text(component: &FrontendSurfaceComponent) -> Vec<String> {
    let tree = component.last_tree.as_ref().expect("rendered widget tree");
    let mut output = Vec::new();
    collect_text_content(tree, &mut output);
    output
}

pub(super) fn event_node(
    tag: &str,
    key: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    handlers: &[(&str, &str)],
) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.attributes.insert("_mesh_key".into(), key.into());
    node.layout.x = x;
    node.layout.y = y;
    node.layout.width = width;
    node.layout.height = height;
    node.event_handlers = handlers
        .iter()
        .map(|(event, handler)| ((*event).into(), (*handler).into()))
        .collect();
    node
}

pub(super) fn event_node_with_attrs(
    tag: &str,
    key: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    attrs: &[(&str, &str)],
    handlers: &[(&str, &str)],
) -> WidgetNode {
    let mut node = event_node(tag, key, x, y, width, height, handlers);
    for (name, value) in attrs {
        node.attributes.insert((*name).into(), (*value).into());
    }
    node
}

pub(super) fn root_with(children: Vec<WidgetNode>) -> WidgetNode {
    let mut root = WidgetNode::new("box");
    root.attributes.insert("_mesh_key".into(), "root".into());
    root.layout.width = 240.0;
    root.layout.height = 160.0;
    root.children = children;
    root
}

pub(super) fn text_node(
    key: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    selectable: bool,
) -> WidgetNode {
    let mut node = WidgetNode::new("text");
    node.attributes.insert("_mesh_key".into(), key.into());
    node.attributes
        .insert("content".into(), "Selectable text".into());
    if selectable {
        node.attributes.insert("selectable".into(), "true".into());
    }
    node.layout.x = x;
    node.layout.y = y;
    node.layout.width = width;
    node.layout.height = height;
    node
}

pub(super) fn count_selectable_text_nodes(node: &WidgetNode) -> usize {
    let here = usize::from(
        node.tag == "text"
            && node
                .attributes
                .get("selectable")
                .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1")),
    );
    here + node
        .children
        .iter()
        .map(count_selectable_text_nodes)
        .sum::<usize>()
}

pub(super) fn contains_interactive_tags(node: &WidgetNode) -> bool {
    matches!(
        node.tag.as_str(),
        "button" | "slider" | "switch" | "checkbox" | "input"
    ) || node.children.iter().any(contains_interactive_tags)
}

pub(super) fn child_with_attrs(tag: &str, attrs: &[(&str, &str)]) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    for (name, value) in attrs {
        node.attributes.insert((*name).into(), (*value).into());
    }
    node
}

pub(super) fn first_node_by_tag<'a>(node: &'a WidgetNode, tag: &str) -> Option<&'a WidgetNode> {
    if node.tag == tag {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| first_node_by_tag(child, tag))
}

/// First node whose `class` attribute contains `class` as a whitespace-
/// separated token (matching CSS class semantics, not a substring match).
pub(super) fn first_node_by_class<'a>(node: &'a WidgetNode, class: &str) -> Option<&'a WidgetNode> {
    if node
        .attributes
        .get("class")
        .is_some_and(|value| value.split_whitespace().any(|token| token == class))
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| first_node_by_class(child, class))
}

pub(super) fn first_node_with_click_handler<'a>(
    node: &'a WidgetNode,
    handler: &str,
) -> Option<&'a WidgetNode> {
    if node
        .event_handlers
        .get("click")
        .is_some_and(|candidate| candidate == handler)
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| first_node_with_click_handler(child, handler))
}

pub(super) fn node_by_mesh_key<'a>(node: &'a WidgetNode, key: &str) -> &'a WidgetNode {
    find_node_by_mesh_key(node, key).unwrap_or_else(|| panic!("expected node with _mesh_key {key}"))
}

pub(super) fn find_node_by_mesh_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node
        .attributes
        .get("_mesh_key")
        .is_some_and(|value| value == key)
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_node_by_mesh_key(child, key))
}

pub(super) fn first_node_with_attr<'a>(
    node: &'a WidgetNode,
    attr: &str,
    value: &str,
) -> Option<&'a WidgetNode> {
    if node
        .attributes
        .get(attr)
        .is_some_and(|candidate| candidate == value)
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| first_node_with_attr(child, attr, value))
}

pub(super) fn collect_text_content(node: &WidgetNode, output: &mut Vec<String>) {
    if node.computed_style.display == Display::None {
        return;
    }
    if node.tag == "text" {
        if let Some(content) = node.attributes.get("content") {
            output.push(content.clone());
        }
    }
    for child in &node.children {
        collect_text_content(child, output);
    }
}

pub(super) fn count_tag(node: &WidgetNode, tag: &str) -> usize {
    usize::from(node.tag == tag)
        + node
            .children
            .iter()
            .map(|child| count_tag(child, tag))
            .sum::<usize>()
}
