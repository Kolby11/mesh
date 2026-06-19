use super::super::*;
use super::{BackendLaunchCandidate, BackendLifecycleStatusRecord};
use mesh_core_module::package::BackendProviderNode;

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

        let Some(module) = graph.module(&active_provider.module_id) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} is not installed",
                    active_provider.module_id
                ),
            });
            continue;
        };

        if !module.enabled || module.kind != ModuleKind::Backend {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} is not an enabled backend module",
                    active_provider.module_id
                ),
            });
            continue;
        }

        if active_provider.interface != interface {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} was indexed for {}, not {interface}",
                    active_provider.module_id, active_provider.interface
                ),
            });
            continue;
        }

        if let Some(status) =
            validate_backend_provider_contract(&interface, active_provider, interfaces)
        {
            statuses.push(status);
            continue;
        }

        let Some(module) = modules.get(&active_provider.module_id) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "invalid_manifest",
                message: format!(
                    "active provider {} has no discovered runtime manifest",
                    active_provider.module_id
                ),
            });
            continue;
        };

        if let Some(binary) = module
            .manifest
            .dependencies
            .binaries
            .iter()
            .find(|binary| !binary.optional && !binary_exists(&binary.name))
        {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_binary",
                message: format!(
                    "backend provider {} requires unavailable binary {}{}",
                    active_provider.module_id,
                    binary.name,
                    binary_package_hint(binary)
                ),
            });
            continue;
        }

        let entrypoint = module.manifest.entrypoints.main.as_deref();
        let Some(entrypoint) = entrypoint else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_entrypoint",
                message: format!(
                    "backend provider {} has no service entrypoint",
                    active_provider.module_id
                ),
            });
            continue;
        };

        let entrypoint_path = module.path.join(entrypoint);
        let Ok(script_source) = std::fs::read_to_string(&entrypoint_path) else {
            statuses.push(BackendLifecycleStatusRecord {
                interface: interface.clone(),
                provider_id: Some(active_provider.module_id.clone()),
                status: "missing_entrypoint",
                message: format!(
                    "backend provider {} entrypoint is unreadable: {}",
                    active_provider.module_id,
                    entrypoint_path.display()
                ),
            });
            continue;
        };

        let capabilities = module
            .manifest
            .capabilities
            .required
            .iter()
            .chain(module.manifest.capabilities.optional.iter())
            .cloned()
            .collect::<Vec<_>>();
        let settings = backend_module_settings_json(config, &active_provider.module_id);
        candidates.push(BackendLaunchCandidate {
            module_id: active_provider.module_id.clone(),
            interface: interface.clone(),
            service_name: service_name_from_interface(&interface),
            entrypoint_path,
            script_source,
            capabilities,
            settings,
        });
    }

    (candidates, statuses)
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

pub(super) fn legacy_backend_candidates_from_discovery(
    modules: &HashMap<String, ModuleInstance>,
    config: &ShellConfig,
) -> Vec<BackendLaunchCandidate> {
    let mut module_ids: Vec<String> = modules.keys().cloned().collect();
    module_ids.sort();
    let mut services: HashMap<String, Vec<(&ModuleInstance, u32)>> = HashMap::new();

    for module_id in module_ids {
        let Some(module) = modules.get(&module_id) else {
            continue;
        };
        if module.manifest.package.module_type != ModuleType::Backend {
            continue;
        }
        let Some(service) = module.manifest.primary_service() else {
            continue;
        };
        let service_name = service_name_from_interface(&service.provides);
        services
            .entry(service_name)
            .or_default()
            .push((module, service.priority));
    }

    let mut candidates = Vec::new();
    for (service_name, mut service_candidates) in services {
        service_candidates.sort_by(|(a, a_priority), (b, b_priority)| {
            b_priority
                .cmp(a_priority)
                .then_with(|| a.manifest.package.id.cmp(&b.manifest.package.id))
        });
        let Some((module, _)) = service_candidates.into_iter().next() else {
            continue;
        };
        let missing_binary = module
            .manifest
            .dependencies
            .binaries
            .iter()
            .find(|binary| !binary.optional && !binary_exists(&binary.name));
        if let Some(binary) = missing_binary {
            tracing::info!(
                "skipping legacy backend '{}' for service '{}' because binary '{}' is unavailable",
                module.manifest.package.id,
                service_name,
                binary.name
            );
            continue;
        }
        let Some(entrypoint) = module.manifest.entrypoints.main.as_deref() else {
            tracing::warn!(
                "legacy backend module {} has no service entrypoint",
                module.manifest.package.id
            );
            continue;
        };
        let entrypoint_path = module.path.join(entrypoint);
        let Ok(script_source) = std::fs::read_to_string(&entrypoint_path) else {
            tracing::warn!(
                "legacy backend module {} has no readable script at {}",
                module.manifest.package.id,
                entrypoint_path.display()
            );
            continue;
        };
        let capabilities = module
            .manifest
            .capabilities
            .required
            .iter()
            .chain(module.manifest.capabilities.optional.iter())
            .cloned()
            .collect::<Vec<_>>();
        candidates.push(BackendLaunchCandidate {
            module_id: module.manifest.package.id.clone(),
            interface: format!("mesh.{service_name}"),
            service_name,
            entrypoint_path,
            script_source,
            capabilities,
            settings: backend_module_settings_json(config, &module.manifest.package.id),
        });
    }

    candidates
}

fn binary_exists(name: &str) -> bool {
    if name.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(name).is_file();
    }

    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths).any(|dir| dir.join(name).is_file())
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
