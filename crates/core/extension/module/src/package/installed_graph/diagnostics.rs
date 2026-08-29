use super::super::ModuleKind;
use super::*;
use crate::manifest::{
    KEYBOARD_MODE_VALUES, SURFACE_EDGE_VALUES, SURFACE_LAYER_VALUES, SURFACE_ROLE_VALUES,
    SurfaceLayoutSection, WINDOW_DECORATIONS_VALUES, canonical_keyboard_mode,
    canonical_surface_edge, canonical_surface_layer, canonical_surface_role,
    canonical_window_decorations,
};
use mesh_core_locale::normalize_locale_tag;
use mesh_core_service::{ContractCapabilities, InterfaceContract};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;

pub(super) fn build_graph_diagnostics(
    modules: &HashMap<String, InstalledModuleNode>,
    frontend_requirements: &HashMap<String, FrontendRequirementSet>,
    backend_providers: &HashMap<String, Vec<BackendProviderNode>>,
    contributions: &ModuleContributionIndex,
    interface_contracts: &HashMap<String, InterfaceContract>,
    manual_diagnostics: Vec<ModuleGraphDiagnostic>,
) -> Vec<ModuleGraphDiagnostic> {
    let mut diagnostics = manual_diagnostics;
    let contract_capabilities: HashMap<String, ContractCapabilities> = interface_contracts
        .iter()
        .map(|(name, contract)| (name.clone(), contract.capabilities.clone()))
        .collect();
    let contract_events: HashMap<String, std::collections::HashSet<String>> = interface_contracts
        .iter()
        .map(|(name, contract)| {
            (
                name.clone(),
                contract
                    .events
                    .iter()
                    .map(|event| event.name.clone())
                    .collect(),
            )
        })
        .collect();

    diagnose_frontend_requirements(
        modules,
        frontend_requirements,
        contributions,
        &contract_capabilities,
        &contract_events,
        &mut diagnostics,
    );
    diagnose_backend_providers(
        modules,
        backend_providers,
        &contract_capabilities,
        &contract_events,
        &mut diagnostics,
    );
    diagnose_icon_requirements(contributions, &mut diagnostics);
    diagnose_settings_namespaces(contributions, &mut diagnostics);
    diagnose_frontend_surfaces(contributions, &mut diagnostics);
    diagnose_required_binaries(modules, &mut diagnostics);
    diagnose_missing_interface_contracts(modules, &mut diagnostics);
    diagnose_duplicate_keybind_triggers(contributions, &mut diagnostics);

    sort_diagnostics(&mut diagnostics);
    diagnostics
}

pub(super) fn authoring_diagnostics_enabled() -> bool {
    std::env::var_os("MESH_AUTHORING_DIAGNOSTICS").is_some_and(|value| value != "0")
}

pub(super) fn build_authoring_diagnostics(
    modules: &HashMap<String, InstalledModuleNode>,
) -> Vec<ModuleGraphDiagnostic> {
    let mut diagnostics = Vec::new();
    diagnose_frontend_source_contracts(modules, &mut diagnostics);
    sort_diagnostics(&mut diagnostics);
    diagnostics
}

pub(super) fn sort_diagnostics(diagnostics: &mut [ModuleGraphDiagnostic]) {
    diagnostics.sort_by(|a, b| {
        a.status
            .cmp(&b.status)
            .then_with(|| a.module_id.cmp(&b.module_id))
            .then_with(|| a.contribution_id.cmp(&b.contribution_id))
    });
}

fn diagnose_frontend_requirements(
    modules: &HashMap<String, InstalledModuleNode>,
    frontend_requirements: &HashMap<String, FrontendRequirementSet>,
    contributions: &ModuleContributionIndex,
    contract_capabilities: &HashMap<String, ContractCapabilities>,
    contract_events: &HashMap<String, std::collections::HashSet<String>>,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for requirements in frontend_requirements.values() {
        for interface in requirements.backend.keys() {
            if let Some(capabilities) = contract_capabilities.get(interface) {
                let mut required_capabilities = capabilities.required.clone();
                if let Some(read_policy) = capabilities.read_policy() {
                    required_capabilities.extend(read_policy);
                }
                required_capabilities.sort();
                required_capabilities.dedup();
                for required in required_capabilities {
                    if !requirements.capabilities.iter().any(|cap| cap == &required) {
                        diagnostics.push(ModuleGraphDiagnostic {
                            module_id: requirements.module_id.clone(),
                            contribution_id: Some(format!(
                                "{}:interface:{}",
                                requirements.module_id, interface
                            )),
                            status: "missing_interface_required_capability".into(),
                            message: format!(
                                "frontend module {} requires interface {interface} but does not declare required capability {required}",
                                requirements.module_id
                            ),
                        });
                    }
                }
            }
        }
        for icon_pack in requirements.icons.keys() {
            if !resource_module_or_contribution_exists(
                modules,
                ModuleKind::IconPack,
                &contributions.icon_packs,
                icon_pack,
            ) {
                diagnostics.push(ModuleGraphDiagnostic {
                    module_id: requirements.module_id.clone(),
                    contribution_id: None,
                    status: "missing_icon_pack_requirement".into(),
                    message: format!(
                        "frontend module {} requires icon pack {icon_pack}, but no enabled icon-pack contribution is installed",
                        requirements.module_id
                    ),
                });
            }
        }
        for font_pack in requirements.fonts.keys() {
            if !resource_module_or_path_contribution_exists(
                modules,
                ModuleKind::FontPack,
                &contributions.fonts,
                font_pack,
            ) {
                diagnostics.push(ModuleGraphDiagnostic {
                    module_id: requirements.module_id.clone(),
                    contribution_id: None,
                    status: "missing_font_pack_requirement".into(),
                    message: format!(
                        "frontend module {} requires font pack {font_pack}, but no enabled font contribution is installed",
                        requirements.module_id
                    ),
                });
            }
        }
        for language_pack in requirements.i18n.keys() {
            if !resource_module_or_i18n_contribution_exists(
                modules,
                ModuleKind::LanguagePack,
                &contributions.i18n,
                language_pack,
            ) {
                diagnostics.push(ModuleGraphDiagnostic {
                    module_id: requirements.module_id.clone(),
                    contribution_id: None,
                    status: "missing_i18n_pack_requirement".into(),
                    message: format!(
                        "frontend module {} requires language pack {language_pack}, but no enabled i18n contribution is installed",
                        requirements.module_id
                    ),
                });
            }
        }
        for theme in requirements.themes.keys() {
            if !resource_module_or_theme_contribution_exists(
                modules,
                ModuleKind::Theme,
                &contributions.themes,
                theme,
            ) {
                diagnostics.push(ModuleGraphDiagnostic {
                    module_id: requirements.module_id.clone(),
                    contribution_id: None,
                    status: "missing_theme_requirement".into(),
                    message: format!(
                        "frontend module {} requires theme {theme}, but no enabled theme contribution is installed",
                        requirements.module_id
                    ),
                });
            }
        }
        let Some(module) = modules.get(&requirements.module_id) else {
            continue;
        };
        let module_dir = module.manifest_path.parent().unwrap_or(Path::new("."));
        let scan_root = module_dir.join("src");
        let scan_root = if scan_root.is_dir() {
            scan_root.as_path()
        } else {
            module_dir
        };
        for (path, content) in scan_mesh_files_recursive(scan_root) {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?");
            for (interface, event) in extract_frontend_interface_event_subscriptions(&content) {
                let requires_interface = requirements.backend.contains_key(&interface)
                    || requirements.optional_backend.contains_key(&interface);
                if !requires_interface {
                    continue;
                }
                if contract_events
                    .get(&interface)
                    .is_some_and(|events| !events.contains(&event))
                {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: requirements.module_id.clone(),
                        contribution_id: Some(format!(
                            "{}:event:{}",
                            requirements.module_id, file_name
                        )),
                        status: "undeclared_interface_event_subscription".into(),
                        message: format!(
                            "frontend module {} subscribes to event '{}' for interface {} in {}, but the interface contract does not declare it",
                            requirements.module_id, event, interface, file_name
                        ),
                    });
                }
            }
        }
    }
}

fn diagnose_backend_providers(
    modules: &HashMap<String, InstalledModuleNode>,
    backend_providers: &HashMap<String, Vec<BackendProviderNode>>,
    contract_capabilities: &HashMap<String, ContractCapabilities>,
    contract_events: &HashMap<String, std::collections::HashSet<String>>,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for provider in backend_providers
        .values()
        .flat_map(|providers| providers.iter())
    {
        if let Some(base_module) = &provider.base_module {
            let declares_base_module = modules.get(&provider.module_id).is_some_and(|module| {
                module
                    .manifest
                    .mesh
                    .dependencies
                    .modules
                    .contains_key(base_module)
            });
            if !declares_base_module {
                diagnostics.push(ModuleGraphDiagnostic {
                    module_id: provider.module_id.clone(),
                    contribution_id: Some(provider.source.scoped_id.clone()),
                    status: "missing_provider_interface_module_dependency".into(),
                    message: format!(
                        "backend provider {} implements {} with base module {base_module} but does not declare it in mesh.uses.modules",
                        provider.module_id, provider.interface
                    ),
                });
            }
        }
        // A backend provider implements an interface; it must not restate the
        // interface's consumer capabilities (`service.<domain>.read/control`).
        // Those are powers for frontends/automation that *consume* the contract.
        // Providers request only generic host powers (exec.*, dbus.*, net.*).
        // Restating them is the drift that made these capabilities meaningless,
        // so flag each one with a concrete "remove it" action.
        if let Some(capabilities) = contract_capabilities.get(&provider.interface) {
            let consumer_capabilities: std::collections::HashSet<&str> = capabilities
                .required
                .iter()
                .chain(capabilities.optional.iter())
                .map(String::as_str)
                .collect();
            for capability in provider
                .required_capabilities
                .iter()
                .chain(provider.optional_capabilities.iter())
            {
                if consumer_capabilities.contains(capability.as_str()) {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: provider.module_id.clone(),
                        contribution_id: Some(provider.source.scoped_id.clone()),
                        status: "provider_declares_consumer_capability".into(),
                        message: format!(
                            "backend provider {} implements {} and should not declare consumer capability {capability}; remove it — providers request only host powers (exec.*, dbus.*, net.*), while {capability} is for modules that consume {}",
                            provider.module_id, provider.interface, provider.interface
                        ),
                    });
                }
            }
        }
        if let Some(events) = contract_events.get(&provider.interface)
            && let Some(module) = modules.get(&provider.module_id)
        {
            let module_dir = module.manifest_path.parent().unwrap_or(Path::new("."));
            let scan_root = module_dir.join("src");
            let scan_root = if scan_root.is_dir() {
                scan_root.as_path()
            } else {
                module_dir
            };
            for (path, content) in scan_files_recursive(scan_root, "luau") {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                for event in extract_backend_event_names(&content) {
                    if !events.contains(&event) {
                        diagnostics.push(ModuleGraphDiagnostic {
                            module_id: provider.module_id.clone(),
                            contribution_id: Some(format!(
                                "{}:event:{}",
                                provider.module_id, file_name
                            )),
                            status: "undeclared_interface_event_emit".into(),
                            message: format!(
                                "backend provider {} emits event '{}' for interface {} in {}, but the interface contract does not declare it",
                                provider.module_id, event, provider.interface, file_name
                            ),
                        });
                    }
                }
            }
        }
    }
}

fn diagnose_icon_requirements(
    contributions: &ModuleContributionIndex,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for requirement in &contributions.icon_requirements {
        if contributions
            .icon_packs
            .iter()
            .any(|pack| pack.mappings.contains_key(&requirement.name))
        {
            continue;
        }

        diagnostics.push(ModuleGraphDiagnostic {
            module_id: requirement.module_id.clone(),
            contribution_id: Some(requirement.source.scoped_id.clone()),
            status: if requirement.required {
                "missing_required_icon".into()
            } else {
                "missing_optional_icon".into()
            },
            message: format!(
                "module {} declares {} semantic icon {}, but no enabled icon pack maps it",
                requirement.module_id,
                if requirement.required {
                    "required"
                } else {
                    "optional"
                },
                requirement.name
            ),
        });
    }
}

fn diagnose_settings_namespaces(
    contributions: &ModuleContributionIndex,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    let mut settings_by_namespace: HashMap<&str, Vec<&ContributedSettingsSchema>> = HashMap::new();
    for settings in &contributions.settings {
        settings_by_namespace
            .entry(settings.namespace.as_str())
            .or_default()
            .push(settings);
    }
    for (namespace, schemas) in settings_by_namespace {
        if schemas.len() <= 1 {
            continue;
        }
        for schema in schemas {
            diagnostics.push(ModuleGraphDiagnostic {
                module_id: schema.module_id.clone(),
                contribution_id: Some(schema.source.scoped_id.clone()),
                status: "duplicate_settings_namespace".into(),
                message: format!(
                    "settings namespace {namespace} is contributed by multiple enabled modules"
                ),
            });
        }
    }
}

fn diagnose_frontend_surfaces(
    contributions: &ModuleContributionIndex,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for surface in &contributions.frontend_surfaces {
        if surface.surface_layout.is_none() {
            diagnostics.push(ModuleGraphDiagnostic {
                module_id: surface.module_id.clone(),
                contribution_id: Some(surface.source.scoped_id.clone()),
                status: "missing_frontend_surface_layout".into(),
                message: format!(
                    "frontend module {} has a main entrypoint but does not declare mesh.surface",
                    surface.module_id
                ),
            });
        }
        // Placement fields that belong to the other role are rejected, not
        // ignored: a `mesh.surface` block that names both an anchor and
        // `role: "window"` has two incompatible intents in it, and silently
        // dropping one leaves the author debugging a surface that ignores half
        // its own manifest.
        // A promotable surface is realized under both roles at different points
        // in its life, so both field sets apply to it and neither is a mismatch.
        if let Some(layout) = &surface.surface_layout {
            if !diagnose_surface_enum_values(surface, layout, diagnostics) {
                continue;
            }
            let role = layout
                .role
                .as_deref()
                .map(canonical_surface_role)
                .unwrap_or(Some("layer"));
            let is_window = role == Some("window");
            if layout.promotable == Some(true) {
                continue;
            }
            let (rejected, role_name) = if is_window {
                (layout.layer_only_fields(), "window")
            } else {
                (layout.window_only_fields(), "layer")
            };
            if !rejected.is_empty() {
                diagnostics.push(ModuleGraphDiagnostic {
                    module_id: surface.module_id.clone(),
                    contribution_id: Some(surface.source.scoped_id.clone()),
                    status: "surface_role_field_mismatch".into(),
                    message: format!(
                        "frontend module {} declares mesh.surface.role \"{role_name}\" but also sets {} — {}",
                        surface.module_id,
                        rejected.join(", "),
                        if is_window {
                            "a window is placed by the compositor, not by layer-shell anchoring"
                        } else {
                            "these fields only apply to a window surface"
                        }
                    ),
                });
            }
        }
        if surface.accessibility.is_none() {
            diagnostics.push(ModuleGraphDiagnostic {
                module_id: surface.module_id.clone(),
                contribution_id: Some(surface.source.scoped_id.clone()),
                status: "missing_frontend_accessibility".into(),
                message: format!(
                    "frontend module {} has a main entrypoint but does not declare mesh.accessibility",
                    surface.module_id
                ),
            });
        }
    }
}

fn diagnose_surface_enum_values(
    surface: &ContributedFrontendSurface,
    layout: &SurfaceLayoutSection,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) -> bool {
    let enum_fields = [
        (
            "role",
            layout.role.as_deref(),
            canonical_surface_role as fn(&str) -> Option<&'static str>,
            SURFACE_ROLE_VALUES,
        ),
        (
            "decorations",
            layout.decorations.as_deref(),
            canonical_window_decorations,
            WINDOW_DECORATIONS_VALUES,
        ),
        (
            "anchor",
            layout.anchor.as_deref(),
            canonical_surface_edge,
            SURFACE_EDGE_VALUES,
        ),
        (
            "layer",
            layout.layer.as_deref(),
            canonical_surface_layer,
            SURFACE_LAYER_VALUES,
        ),
        (
            "keyboard_mode",
            layout.keyboard_mode.as_deref(),
            canonical_keyboard_mode,
            KEYBOARD_MODE_VALUES,
        ),
    ];
    let mut valid = true;
    for (field, value, parser, allowed) in enum_fields {
        let Some(value) = value else {
            continue;
        };
        if parser(value).is_some() {
            continue;
        }
        valid = false;
        diagnostics.push(ModuleGraphDiagnostic {
            module_id: surface.module_id.clone(),
            contribution_id: Some(surface.source.scoped_id.clone()),
            status: "invalid_surface_enum".into(),
            message: format!(
                "frontend module {} has invalid mesh.surface.{field} value {value:?}; expected one of: {}",
                surface.module_id,
                allowed.join(", ")
            ),
        });
    }
    valid
}

fn diagnose_required_binaries(
    modules: &HashMap<String, InstalledModuleNode>,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for module in modules.values().filter(|m| m.enabled) {
        for binary in &module.manifest.mesh.dependencies.binaries {
            if !binary.optional && !binary_available(&binary.name) {
                diagnostics.push(ModuleGraphDiagnostic {
                    module_id: module.id.clone(),
                    contribution_id: None,
                    status: "missing_required_binary".into(),
                    message: format!(
                        "module {} requires binary '{}' but it was not found on PATH{}{}",
                        module.id,
                        binary.name,
                        binary
                            .reason
                            .as_deref()
                            .map(|r| format!("; needed for {r}"))
                            .unwrap_or_default(),
                        binary_package_hint(binary)
                    ),
                });
            }
        }
    }
}

fn diagnose_frontend_source_contracts(
    modules: &HashMap<String, InstalledModuleNode>,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for module in modules
        .values()
        .filter(|m| m.enabled && m.kind == ModuleKind::Frontend)
    {
        let module_dir = module.manifest_path.parent().unwrap_or(Path::new("."));
        let declared_keybinds = module
            .manifest
            .mesh
            .keybinds
            .actions
            .keys()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        let all_declared_icons: std::collections::HashSet<&str> = module
            .manifest
            .mesh
            .icon_requirements
            .required
            .iter()
            .chain(module.manifest.mesh.icon_requirements.optional.iter())
            .map(String::as_str)
            .collect();

        // Load the default-locale catalog keys once per module.
        let default_locale = module
            .manifest
            .mesh
            .i18n
            .default_locale
            .as_deref()
            .unwrap_or("en");
        let canonical_default_locale =
            normalize_locale_tag(default_locale).unwrap_or_else(|_| default_locale.to_string());
        let all_i18n: Vec<_> = module.manifest.mesh.contributes.i18n.iter().collect();
        if !all_i18n.is_empty() {
            let contributed_locales = all_i18n
                .iter()
                .filter_map(|catalog| normalize_locale_tag(&catalog.locale).ok())
                .collect::<std::collections::HashSet<_>>();
            for locale in &module.manifest.mesh.i18n.supported_locales {
                let canonical_locale =
                    normalize_locale_tag(locale).unwrap_or_else(|_| locale.clone());
                if !contributed_locales.contains(&canonical_locale) {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:i18n:{}", module.id, locale)),
                        status: "missing_supported_locale_catalog".into(),
                        message: format!(
                            "module {} declares supported locale '{}' but does not contribute an i18n catalog for it",
                            module.id, locale
                        ),
                    });
                }
            }
            // Warn when mesh.i18n.supportedLocales is redundant with
            // mesh.contributes.i18n. Authors should declare catalogs once in
            // contributes.i18n and omit supportedLocales.
            if !module.manifest.mesh.i18n.supported_locales.is_empty() {
                let declared: std::collections::HashSet<String> = module
                    .manifest
                    .mesh
                    .i18n
                    .supported_locales
                    .iter()
                    .filter_map(|locale| normalize_locale_tag(locale).ok())
                    .collect();
                if declared == contributed_locales {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:i18n:supported_locales", module.id)),
                        status: "redundant_supported_locales".into(),
                        message: format!(
                            "module {} mesh.i18n.supportedLocales lists the same locales as mesh.contributes.i18n; remove supportedLocales and declare catalogs once in mesh.contributes.i18n",
                            module.id
                        ),
                    });
                }
            }
            // Warn when defaultLocale is declared but has no contributed catalog.
            if let Some(default) = module.manifest.mesh.i18n.default_locale.as_deref() {
                let canonical_default =
                    normalize_locale_tag(default).unwrap_or_else(|_| default.to_string());
                if !contributed_locales.contains(&canonical_default) {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:i18n:default_locale", module.id)),
                        status: "missing_default_locale_catalog".into(),
                        message: format!(
                            "module {} declares defaultLocale '{}' but contributes no i18n catalog for it",
                            module.id, default
                        ),
                    });
                }
            }
        }
        let catalog_keys: Option<std::collections::HashSet<String>> = all_i18n
            .iter()
            .find(|c| {
                normalize_locale_tag(&c.locale)
                    .ok()
                    .is_some_and(|locale| locale == canonical_default_locale)
            })
            .and_then(|c| {
                let catalog_path =
                    super::super::contained_path(module_dir, &c.path, "i18n catalog").ok()?;
                let content = std::fs::read_to_string(&catalog_path).ok()?;
                let map: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(&content).ok()?;
                Some(map.keys().cloned().collect())
            });

        let mesh_src_dir = module_dir.join("src");
        let scan_root = if mesh_src_dir.is_dir() {
            mesh_src_dir.as_path()
        } else {
            module_dir
        };

        // Parsing every file's Luau is the expensive half of this scan, and
        // each file is independent — do it up front across the pool, then keep
        // the diagnostic loop itself serial so ordering stays deterministic.
        let mesh_files = scan_mesh_files_recursive(scan_root);
        let mesh_source_scans: Vec<MeshSourceScan> = mesh_files
            .par_iter()
            .map(|(_, content)| scan_mesh_source(content))
            .collect();

        for ((path, _content), scan) in mesh_files.iter().zip(&mesh_source_scans) {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            for icon_name in &scan.icon_names {
                if !all_declared_icons.contains(icon_name.as_str()) {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:icon:{}", module.id, file_name)),
                        status: "undeclared_icon_use".into(),
                        message: format!(
                            "module {} uses icon '{}' in {} but does not declare it in iconRequirements",
                            module.id, icon_name, file_name
                        ),
                    });
                }
            }
            if let Some(catalog) = &catalog_keys {
                for key in &scan.static_calls.t_keys {
                    if !catalog.contains(key.as_str()) {
                        diagnostics.push(ModuleGraphDiagnostic {
                            module_id: module.id.clone(),
                            contribution_id: Some(format!("{}:i18n:{}", module.id, file_name)),
                            status: "undeclared_i18n_key".into(),
                            message: format!(
                                "module {} uses translation key '{}' in {} but it is not in the '{}' catalog",
                                module.id, key, file_name, default_locale
                            ),
                        });
                    }
                }
            }
            for channel in &scan.static_calls.publish_channels {
                if channel.starts_with("mesh.") {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:event:{}", module.id, file_name)),
                        status: "raw_interface_domain_event_publish".into(),
                        message: format!(
                            "module {} publishes raw interface-domain event '{}' in {}; call the interface proxy method instead, or use a shell.* event for shell-owned commands",
                            module.id, channel, file_name
                        ),
                    });
                } else if channel.starts_with("shell.")
                    && !is_declared_shell_event_channel(&channel)
                {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:event:{}", module.id, file_name)),
                        status: "unknown_shell_event_publish".into(),
                        message: format!(
                            "module {} publishes shell event '{}' in {}, but the shell-owned event namespace does not declare it",
                            module.id, channel, file_name
                        ),
                    });
                }
            }
            for (action_id, has_handler) in &scan.keybind_subscriptions {
                if !declared_keybinds.contains(action_id.as_str()) {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:keybind:{}", module.id, file_name)),
                        status: "undeclared_keybind_subscription".into(),
                        message: format!(
                            "module {} subscribes to keybind action '{}' in {}, but mesh.contributes.keybinds does not declare it",
                            module.id, action_id, file_name
                        ),
                    });
                }
                if !has_handler {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:keybind:{}", module.id, file_name)),
                        status: "keybind_subscription_missing_handler".into(),
                        message: format!(
                            "module {} subscribes to keybind action '{}' in {} without an onkeybind handler",
                            module.id, action_id, file_name
                        ),
                    });
                }
            }
        }
    }
}

fn diagnose_missing_interface_contracts(
    modules: &HashMap<String, InstalledModuleNode>,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for module in modules
        .values()
        .filter(|m| m.enabled && m.kind == ModuleKind::Interface)
    {
        if let Some(interface) = &module.manifest.mesh.interface
            && interface.contract.is_none()
        {
            diagnostics.push(ModuleGraphDiagnostic {
                module_id: module.id.clone(),
                contribution_id: None,
                status: "missing_interface_contract".into(),
                message: format!(
                    "interface module {} declares {} without a contract; contract-based validation does not apply",
                    module.id, interface.name
                ),
            });
        }
    }
}

fn diagnose_duplicate_keybind_triggers(
    contributions: &ModuleContributionIndex,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    let mut trigger_owners: HashMap<(String, String, String, Vec<String>), Vec<(String, String)>> =
        HashMap::new();
    for action in &contributions.keybinds {
        if let Some(key) = &action.trigger.key {
            let mut mods: Vec<String> = action
                .trigger
                .modifiers
                .iter()
                .map(|m| m.to_ascii_lowercase())
                .collect();
            mods.sort();
            let effective = (
                format!("{:?}", action.scope),
                format!("{:?}", action.trigger.kind),
                key.to_ascii_lowercase(),
                mods,
            );
            trigger_owners
                .entry(effective)
                .or_default()
                .push((action.module_id.clone(), action.action_id.clone()));
        }
    }
    for ((_, _, key, mods), owners) in &trigger_owners {
        if owners.len() <= 1 {
            continue;
        }
        let trigger_str = if mods.is_empty() {
            key.clone()
        } else {
            format!("{}+{}", mods.join("+"), key)
        };
        for (module_id, action_id) in owners {
            diagnostics.push(ModuleGraphDiagnostic {
                module_id: module_id.clone(),
                contribution_id: Some(format!("{module_id}:{action_id}")),
                status: "duplicate_keybind_trigger".into(),
                message: format!(
                    "keybind action {module_id}:{action_id} has trigger '{trigger_str}' that conflicts with {} other action(s)",
                    owners.len() - 1
                ),
            });
        }
    }
}

fn enabled_module_exists(
    modules: &HashMap<String, InstalledModuleNode>,
    kind: ModuleKind,
    id: &str,
) -> bool {
    modules
        .get(id)
        .is_some_and(|module| module.enabled && module.kind == kind)
}

fn resource_module_or_contribution_exists(
    modules: &HashMap<String, InstalledModuleNode>,
    kind: ModuleKind,
    contributions: &[ContributedIconPack],
    id: &str,
) -> bool {
    enabled_module_exists(modules, kind, id)
        || contributions
            .iter()
            .any(|contribution| contribution.module_id == id || contribution.id == id)
}

fn resource_module_or_path_contribution_exists(
    modules: &HashMap<String, InstalledModuleNode>,
    kind: ModuleKind,
    contributions: &[ContributedPathResource],
    id: &str,
) -> bool {
    enabled_module_exists(modules, kind, id)
        || contributions
            .iter()
            .any(|contribution| contribution.module_id == id || contribution.id == id)
}

fn resource_module_or_i18n_contribution_exists(
    modules: &HashMap<String, InstalledModuleNode>,
    kind: ModuleKind,
    contributions: &[ContributedI18n],
    id: &str,
) -> bool {
    enabled_module_exists(modules, kind, id)
        || contributions.iter().any(|contribution| {
            contribution.module_id == id || contribution.id == id || contribution.locale == id
        })
}

fn resource_module_or_theme_contribution_exists(
    modules: &HashMap<String, InstalledModuleNode>,
    kind: ModuleKind,
    contributions: &[ContributedTheme],
    id: &str,
) -> bool {
    enabled_module_exists(modules, kind, id)
        || contributions
            .iter()
            .any(|contribution| contribution.module_id == id || contribution.id == id)
}
