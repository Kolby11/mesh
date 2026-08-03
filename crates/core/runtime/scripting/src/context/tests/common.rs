use mesh_core_service::{
    ContractCapabilities, InterfaceArgument, InterfaceCatalog, InterfaceContract, InterfaceEvent,
    InterfaceMethod, InterfaceProvider, parse_contract_version,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn temp_storage_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "mesh-context-storage-{name}-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&root);
    root
}

pub(super) fn audio_catalog() -> InterfaceCatalog {
    let mut catalog = InterfaceCatalog::default();
    catalog.register_contract(InterfaceContract {
        interface: "mesh.audio".into(),
        version: parse_contract_version("1.0").unwrap(),
        // State fields are documented core reads — not callable methods.
        state_fields: Vec::new(),
        // Only mutating command methods belong here.
        methods: vec![
            InterfaceMethod {
                name: "set_volume".into(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "percent".into(),
                        arg_type: "float".into(),
                    },
                ],
                returns: Some("Result".into()),
                coalesce: false,
                state_binding: None,
            },
            InterfaceMethod {
                name: "volume_up".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
                state_binding: None,
            },
            InterfaceMethod {
                name: "volume_down".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
                state_binding: None,
            },
            InterfaceMethod {
                name: "toggle_mute".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
                state_binding: None,
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
                returns: Some("Result".into()),
                coalesce: false,
                state_binding: None,
            },
        ],
        events: vec![InterfaceEvent {
            name: "VolumeChanged".into(),
            payload: vec![
                InterfaceArgument {
                    name: "device_id".into(),
                    arg_type: "string".into(),
                },
                InterfaceArgument {
                    name: "level".into(),
                    arg_type: "float".into(),
                },
            ],
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
    catalog
}

pub(super) fn theme_provider_only_catalog() -> InterfaceCatalog {
    let mut catalog = InterfaceCatalog::default();
    catalog.register_provider(InterfaceProvider {
        interface: "mesh.theme".into(),
        version: Some("1.0".into()),
        base_module: None,
        provider_module: "@mesh/shell-theme".into(),
        backend_name: "Shell Theme".into(),
        priority: 100,
    });
    catalog
}
