use super::super::*;
use super::{BackendLaunchCandidate, BackendLifecycleStatusRecord};
use mesh_core_module::package::{BackendProviderNode, binary_available};

pub(in crate::shell) fn backend_launch_candidates_from_graph(
    graph: &InstalledModuleGraph,
    modules: &HashMap<String, ModuleInstance>,
    config: &ShellConfig,
    interfaces: &InterfaceRegistry,
) -> (
    Vec<BackendLaunchCandidate>,
    Vec<BackendLifecycleStatusRecord>,
) {
    let mut statuses = backend_requirement_statuses(graph);
    let mut interface_names: Vec<String> = graph
        .backend_provider_contributions()
        .into_iter()
        .map(|provider| provider.interface.clone())
        .collect();
    interface_names.sort();
    interface_names.dedup();

    let mut candidates = Vec::new();
    for interface in interface_names {
        let Some(active_provider) = graph.active_provider(&interface) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface,
                provider_id: None,
                status: "no_active_provider",
                message: "no active provider selected".to_string(),
            });
            continue;
        };

        match launch_candidate_for_provider(graph, modules, config, interfaces, active_provider) {
            Ok(candidate) => candidates.push(candidate),
            Err(status) => statuses.push(status),
        }
    }

    (candidates, statuses)
}

/// Build the launch candidate for one concrete provider of an interface,
/// validating manifest state, contract registration, required binaries, and
/// the entrypoint script. Shared by startup launch and supervised restarts.
pub(in crate::shell) fn launch_candidate_for_provider(
    graph: &InstalledModuleGraph,
    modules: &HashMap<String, ModuleInstance>,
    config: &ShellConfig,
    interfaces: &InterfaceRegistry,
    provider: &BackendProviderNode,
) -> Result<BackendLaunchCandidate, BackendLifecycleStatusRecord> {
    let interface = provider.interface.clone();
    let Some(module) = graph.module(&provider.module_id) else {
        return Err(BackendLifecycleStatusRecord {
            interface,
            provider_id: Some(provider.module_id.clone()),
            status: "invalid_manifest",
            message: format!("active provider {} is not installed", provider.module_id),
        });
    };

    if !module.enabled || module.kind != ModuleKind::Backend {
        return Err(BackendLifecycleStatusRecord {
            interface,
            provider_id: Some(provider.module_id.clone()),
            status: "invalid_manifest",
            message: format!(
                "active provider {} is not an enabled backend module",
                provider.module_id
            ),
        });
    }

    if let Some(status) = validate_backend_provider_contract(&interface, provider, interfaces) {
        return Err(status);
    }

    let Some(module) = modules.get(&provider.module_id) else {
        return Err(BackendLifecycleStatusRecord {
            interface,
            provider_id: Some(provider.module_id.clone()),
            status: "invalid_manifest",
            message: format!(
                "active provider {} has no discovered runtime manifest",
                provider.module_id
            ),
        });
    };

    if let Some(binary) = module
        .manifest
        .dependencies
        .binaries
        .iter()
        .find(|binary| !binary.optional && !binary_available(&binary.name))
    {
        return Err(BackendLifecycleStatusRecord {
            interface,
            provider_id: Some(provider.module_id.clone()),
            status: "missing_binary",
            message: format!(
                "backend provider {} requires unavailable binary {}{}",
                provider.module_id,
                binary.name,
                binary_package_hint(binary)
            ),
        });
    }

    let entrypoint = module.manifest.entrypoints.main.as_deref();
    let Some(entrypoint) = entrypoint else {
        return Err(BackendLifecycleStatusRecord {
            interface,
            provider_id: Some(provider.module_id.clone()),
            status: "missing_entrypoint",
            message: format!(
                "backend provider {} has no service entrypoint",
                provider.module_id
            ),
        });
    };

    let entrypoint_path = module.path.join(entrypoint);
    let Ok(script_source) = std::fs::read_to_string(&entrypoint_path) else {
        return Err(BackendLifecycleStatusRecord {
            interface,
            provider_id: Some(provider.module_id.clone()),
            status: "missing_entrypoint",
            message: format!(
                "backend provider {} entrypoint is unreadable: {}",
                provider.module_id,
                entrypoint_path.display()
            ),
        });
    };

    let capabilities = module
        .manifest
        .capabilities
        .required
        .iter()
        .chain(module.manifest.capabilities.optional.iter())
        .cloned()
        .collect::<Vec<_>>();
    let settings = backend_module_settings_json(config, &provider.module_id);
    Ok(BackendLaunchCandidate {
        module_id: provider.module_id.clone(),
        service_name: service_name_from_interface(&interface),
        interface,
        entrypoint_path,
        script_source,
        capabilities,
        settings,
    })
}

fn backend_requirement_statuses(graph: &InstalledModuleGraph) -> Vec<BackendLifecycleStatusRecord> {
    let mut statuses = Vec::new();
    for frontend in graph.frontend_modules() {
        let Some(requirements) = graph.requirements_for_frontend(&frontend.id) else {
            continue;
        };
        for interface in requirements.backend.keys() {
            if graph.backend_providers_for_interface(interface).is_empty() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "unmet_backend_requirement",
                    message: format!(
                        "frontend module {} requires {interface}, but no enabled backend provider is installed",
                        frontend.id
                    ),
                });
            } else if graph.active_provider(interface).is_none() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "no_active_provider",
                    message: format!(
                        "frontend module {} requires {interface}, but no active provider is selected",
                        frontend.id
                    ),
                });
            } else if let Some(record) = graph.health().iter().find(|record| {
                record.module_id == frontend.id
                    && record.interface.as_deref() == Some(interface.as_str())
                    && record.status == "required_interface_unavailable"
            }) {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: record.provider_id.clone(),
                    status: "unmet_backend_requirement",
                    message: record.message.clone(),
                });
            }
        }
        for interface in requirements.optional_backend.keys() {
            if graph.backend_providers_for_interface(interface).is_empty() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "optional_backend_unavailable",
                    message: format!(
                        "frontend module {} can use optional {interface}, but no enabled backend provider is installed",
                        frontend.id
                    ),
                });
            } else if graph.active_provider(interface).is_none() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "optional_backend_inactive",
                    message: format!(
                        "frontend module {} can use optional {interface}, but no active provider is selected",
                        frontend.id
                    ),
                });
            } else if let Some(record) = graph.health().iter().find(|record| {
                record.module_id == frontend.id
                    && record.interface.as_deref() == Some(interface.as_str())
                    && record.status == "optional_interface_unavailable"
            }) {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: record.provider_id.clone(),
                    status: "optional_backend_unavailable",
                    message: record.message.clone(),
                });
            }
        }
    }
    statuses
}

fn binary_package_hint(binary: &mesh_core_module::manifest::BinaryDependency) -> String {
    if binary.packages.is_empty() {
        return String::new();
    }
    let mut packages = binary.packages.iter().collect::<Vec<_>>();
    packages.sort_by(|(left, _), (right, _)| left.cmp(right));
    format!(
        "; install package {}",
        packages
            .into_iter()
            .map(|(manager, package)| format!("{manager}:{package}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn validate_backend_provider_contract(
    interface: &str,
    provider: &BackendProviderNode,
    interfaces: &InterfaceRegistry,
) -> Option<BackendLifecycleStatusRecord> {
    let resolution = interfaces.resolve(interface, None);
    let provider_id = provider.module_id.as_str();
    if resolution.contract.is_none() && resolution.provider.is_none() {
        return None;
    }

    if resolution.contract.is_none() {
        return Some(BackendLifecycleStatusRecord {
            interface: canonical_interface_name(interface),
            provider_id: Some(provider_id.to_string()),
            status: "invalid_manifest",
            message: format!("active provider {provider_id} has no interface contract"),
        });
    }

    if !interfaces
        .providers_for(interface)
        .iter()
        .any(|provider| provider.provider_module == provider_id)
    {
        return Some(BackendLifecycleStatusRecord {
            interface: canonical_interface_name(interface),
            provider_id: Some(provider_id.to_string()),
            status: "invalid_manifest",
            message: format!(
                "active provider {provider_id} is not registered for interface {}",
                canonical_interface_name(interface)
            ),
        });
    }

    None
}

fn backend_module_settings_json(config: &ShellConfig, module_id: &str) -> serde_json::Value {
    config
        .modules
        .get(module_id)
        .map(|module| match serde_json::to_value(&module.values) {
            Ok(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
            Ok(_) => serde_json::json!({}),
            Err(err) => {
                tracing::warn!(
                    module_id = module_id,
                    "failed to serialize backend module settings: {err}"
                );
                serde_json::json!({})
            }
        })
        .unwrap_or_else(|| serde_json::json!({}))
}
