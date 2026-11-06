use std::collections::HashMap;

fn builtin_state_contract(
    interface: &str,
    fields: &[(&str, &str)],
) -> mesh_core_service::InterfaceContract {
    builtin_contract(interface, fields, &[])
}

/// A core-provided contract: state fields the shell emits, plus the methods it
/// answers itself.
///
/// The shell is the provider for these interfaces, so the methods here are the
/// public, capability-gated way for any module to change composition and
/// configuration. Declaring them as a contract rather than a reserved channel
/// is what makes the settings frontend replaceable: the caller needs
/// `service.<name>.control`, not a particular module id.
/// Register a contract the shell itself owns.
///
/// `register_contract` discards compilation failures, which is survivable for a
/// module-supplied contract but not for one compiled into the shell: a single
/// bad field type silently removes the whole interface from the catalog, and
/// every method on it then rejects as an unknown channel. A malformed built-in
/// contract is a bug in this file, so say so loudly.
fn register_builtin_contract(
    catalog: &mut mesh_core_service::InterfaceCatalog,
    contract: mesh_core_service::InterfaceContract,
) {
    let interface = contract.interface.clone();
    if let Err(error) = catalog.try_register_contract(contract) {
        tracing::error!(
            "built-in interface '{interface}' was rejected and is unavailable: {error}"
        );
        debug_assert!(
            false,
            "built-in contract '{interface}' must compile: {error}"
        );
    }
}

fn builtin_contract(
    interface: &str,
    fields: &[(&str, &str)],
    methods: &[(&str, &[(&str, &str)])],
) -> mesh_core_service::InterfaceContract {
    mesh_core_service::InterfaceContract {
        interface: interface.to_string(),
        version: mesh_core_service::parse_contract_version("1.0")
            .expect("built-in interface version must be valid"),
        state_fields: fields
            .iter()
            .map(|(name, field_type)| mesh_core_service::ContractStateField {
                name: (*name).to_string(),
                field_type: (*field_type).to_string(),
                description: None,
                ..Default::default()
            })
            .collect(),
        methods: methods
            .iter()
            .map(|(name, args)| mesh_core_service::InterfaceMethod {
                name: (*name).to_string(),
                args: args
                    .iter()
                    .map(|(arg, arg_type)| mesh_core_service::InterfaceArgument {
                        name: (*arg).to_string(),
                        arg_type: (*arg_type).to_string(),
                    })
                    .collect(),
                returns: Some("Result".to_string()),
                // These land in the shell synchronously; there is no backend
                // queue to coalesce against and no optimistic state to bind.
                coalesce: false.into(),
                state_binding: None,
            })
            .collect(),
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: mesh_core_service::ContractCapabilities::default(),
    }
}

/// Core-owned service contracts shared by the shell and component hosts.
pub fn builtin_interface_catalog() -> mesh_core_service::InterfaceCatalog {
    let mut builtin_interface_catalog = mesh_core_service::InterfaceCatalog::default();
    let mut theme_contract = builtin_contract(
        "mesh.theme",
        &[
            ("current", "string?"),
            ("theme_id", "string?"),
            ("mode", "string?"),
            ("mode_policy", "object?"),
            ("modes", "object[]?"),
            ("color_scheme", "string?"),
            ("contrast", "string?"),
            ("details", "object"),
            ("tokens", "object?"),
            ("provenance", "object?"),
            ("revision", "string?"),
            ("durable_revision", "string?"),
            ("fingerprint", "int?"),
            ("is_dark", "boolean?"),
            ("themes", "object[]?"),
            ("available", "string[]?"),
            ("scope", "string?"),
            ("can_pin", "boolean?"),
            ("system_resources", "object?"),
            ("backdrop_image", "string?"),
            ("compositor_glass", "boolean?"),
        ],
        &[
            ("set_theme", &[("theme_id", "string"), ("scope", "string?")]),
            ("set_mode", &[("mode", "string")]),
            ("set_icon_theme", &[("theme_id", "string")]),
            ("set_font_family", &[("family", "string")]),
            ("set_font_size", &[("size", "int")]),
            ("set_shell_scale", &[("scale", "int")]),
            ("set_tokens", &[("tokens", "object")]),
            ("set_backdrop_image", &[("path", "string")]),
            (
                "list_font_families",
                &[("offset", "int?"), ("limit", "int?")],
            ),
            (
                "read_details",
                &[
                    ("revision", "string"),
                    ("offset", "int?"),
                    ("limit", "int?"),
                ],
            ),
        ],
    );
    theme_contract
        .capabilities
        .methods
        .insert("list_font_families".into(), vec!["theme.read".into()]);
    theme_contract
        .capabilities
        .methods
        .insert("read_details".into(), vec!["theme.read".into()]);
    theme_contract.events = vec![
        mesh_core_service::InterfaceEvent {
            name: "ThemeChanged".into(),
            payload: [
                ("theme_id", "string?"),
                ("mode", "string?"),
                ("mode_policy", "object?"),
                ("modes", "object[]?"),
                ("color_scheme", "string?"),
                ("contrast", "string?"),
                ("revision", "string?"),
                ("durable_revision", "string?"),
                ("tokens", "object?"),
                ("provenance", "object?"),
                ("changed_tokens", "object[]?"),
                ("changed_token_count", "int?"),
                ("details", "object?"),
                ("details_required", "boolean?"),
            ]
            .into_iter()
            .map(|(name, arg_type)| mesh_core_service::InterfaceArgument {
                name: name.into(),
                arg_type: arg_type.into(),
            })
            .collect(),
        },
        mesh_core_service::InterfaceEvent {
            name: "TokenChanged".into(),
            payload: [
                ("theme_id", "string?"),
                ("mode", "string?"),
                ("name", "string?"),
                ("details", "object?"),
                ("details_required", "boolean?"),
                ("value", "any?"),
                ("provenance", "any?"),
                ("revision", "string?"),
                ("durable_revision", "string?"),
            ]
            .into_iter()
            .map(|(name, arg_type)| mesh_core_service::InterfaceArgument {
                name: name.into(),
                arg_type: arg_type.into(),
            })
            .collect(),
        },
    ];
    register_builtin_contract(&mut builtin_interface_catalog, theme_contract);
    // Locale writes stay on the `mesh.locale.set` host API, which already
    // enforces `locale.write`. A second, service-shaped way in would mean
    // two capability names for one operation.
    register_builtin_contract(
        &mut builtin_interface_catalog,
        builtin_state_contract(
            "mesh.locale",
            &[
                ("current", "string"),
                ("locale", "string"),
                ("chain", "string[]"),
                ("direction", "string"),
                ("policy", "string"),
                ("revision", "string"),
                ("durable_revision", "string"),
            ],
        ),
    );
    register_builtin_contract(
        &mut builtin_interface_catalog,
        builtin_contract(
            mesh_core_debug::DEBUG_INTERFACE,
            &[],
            &[
                ("toggle_overlay", &[]),
                ("toggle_layout_bounds", &[]),
                ("toggle_element_picker", &[]),
                ("open_source", &[("path", "string"), ("line", "int?")]),
                ("toggle_profiling", &[]),
                ("run_benchmark", &[("scenario_id", "string")]),
            ],
        ),
    );
    register_builtin_contract(
        &mut builtin_interface_catalog,
        builtin_contract(
            "mesh.keybinds",
            &[("entries", "any"), ("revision", "string")],
            &[
                (
                    "remap",
                    &[
                        ("surface", "string"),
                        ("action", "string"),
                        ("key", "string"),
                    ],
                ),
                ("set_option", &[("name", "string"), ("value", "string")]),
                ("reassign", &[]),
            ],
        ),
    );
    register_builtin_contract(
        &mut builtin_interface_catalog,
        builtin_contract(
            "mesh.settings",
            &[
                ("revision", "string"),
                ("durable_revision", "string"),
                ("namespaces", "object"),
            ],
            &[
                (
                    "set_prop",
                    &[
                        ("module_id", "string"),
                        ("instance_id", "string?"),
                        ("prop", "string"),
                        ("value", "any"),
                    ],
                ),
                (
                    "unset_prop",
                    &[
                        ("module_id", "string"),
                        ("instance_id", "string?"),
                        ("prop", "string"),
                    ],
                ),
            ],
        ),
    );
    register_builtin_contract(
        &mut builtin_interface_catalog,
        builtin_contract(
            "mesh.packages",
            &[
                ("modules", "object[]"),
                ("providers", "object"),
                ("profiles", "string[]"),
                ("active_profile", "string"),
            ],
            &[
                (
                    "set_module_enabled",
                    &[("module_id", "string"), ("enabled", "boolean")],
                ),
                (
                    "set_provider",
                    &[("interface", "string"), ("provider_id", "string")],
                ),
                ("switch_profile", &[("profile_id", "string")]),
                (
                    "install",
                    &[
                        ("source", "string"),
                        ("profile_id", "string?"),
                        ("available_only", "boolean?"),
                        ("allow_elevated", "boolean?"),
                        ("allow_high", "boolean?"),
                    ],
                ),
                (
                    "uninstall",
                    &[("module_id", "string"), ("force", "boolean?")],
                ),
            ],
        ),
    );
    register_builtin_contract(
        &mut builtin_interface_catalog,
        builtin_contract(
            "mesh.composition",
            &[
                ("mode", "string"),
                ("recovery_reason", "string?"),
                ("profile_id", "string"),
                ("generation", "string"),
                ("roots", "object[]"),
                ("slots", "object[]"),
                ("palette", "object[]"),
            ],
            &[
                (
                    "apply_node_slot",
                    &[
                        ("profile_id", "string"),
                        ("root_instance", "string"),
                        ("slot", "string"),
                        ("nodes", "object[]"),
                        ("expected_generation", "string"),
                    ],
                ),
                (
                    "reset_node_slot",
                    &[
                        ("profile_id", "string"),
                        ("root_instance", "string"),
                        ("slot", "string"),
                        ("expected_generation", "string"),
                    ],
                ),
            ],
        ),
    );
    builtin_interface_catalog
}
