#![allow(dead_code)] // Test-facing candidate wrappers share the production validator.

use super::super::*;
use super::{BackendLaunchCandidate, BackendLifecycleStatusRecord};
use crate::shell::core_provider::CoreServiceRegistry;
#[cfg(test)]
use mesh_core_capability::CapabilityPolicy;
use mesh_core_capability::EffectiveCapabilities;
use mesh_core_module::package::{BackendProviderNode, binary_available};

pub(in crate::shell) trait ServiceCatalogView {
    fn resolve(
        &self,
        requested: &str,
        requested_version: Option<&str>,
    ) -> mesh_core_service::InterfaceResolution;

    fn providers_for(&self, requested: &str) -> Vec<mesh_core_service::InterfaceProvider>;
}

impl ServiceCatalogView for mesh_core_service::ResolvedServiceCatalog {
    fn resolve(
        &self,
        requested: &str,
        requested_version: Option<&str>,
    ) -> mesh_core_service::InterfaceResolution {
        mesh_core_service::ResolvedServiceCatalog::resolve(self, requested, requested_version)
    }

    fn providers_for(&self, requested: &str) -> Vec<mesh_core_service::InterfaceProvider> {
        mesh_core_service::ResolvedServiceCatalog::providers_for(self, requested)
    }
}

impl ServiceCatalogView for mesh_core_service::ResolvedServiceCatalogHandle {
    fn resolve(
        &self,
        requested: &str,
        requested_version: Option<&str>,
    ) -> mesh_core_service::InterfaceResolution {
        mesh_core_service::ResolvedServiceCatalogHandle::resolve(self, requested, requested_version)
    }

    fn providers_for(&self, requested: &str) -> Vec<mesh_core_service::InterfaceProvider> {
        mesh_core_service::ResolvedServiceCatalogHandle::providers_for(self, requested)
    }
}

#[cfg(test)]
pub(in crate::shell) fn backend_launch_candidates_from_graph<T: ServiceCatalogView + ?Sized>(
    graph: &InstalledModuleGraph,
    modules: &HashMap<String, ModuleInstance>,
    settings: &SettingsStore,
    interfaces: &T,
) -> (
    Vec<BackendLaunchCandidate>,
    Vec<BackendLifecycleStatusRecord>,
) {
    let effective_capabilities = test_effective_capabilities(modules);
    backend_launch_candidates_from_graph_with_capabilities(
        graph,
        modules,
        settings,
        interfaces,
        &effective_capabilities,
    )
}

pub(in crate::shell) fn backend_launch_candidates_from_graph_with_capabilities<
    T: ServiceCatalogView + ?Sized,
>(
    graph: &InstalledModuleGraph,
    modules: &HashMap<String, ModuleInstance>,
    settings: &SettingsStore,
    interfaces: &T,
    effective_capabilities: &HashMap<String, EffectiveCapabilities>,
) -> (
    Vec<BackendLaunchCandidate>,
    Vec<BackendLifecycleStatusRecord>,
) {
    let mut statuses = backend_requirement_statuses(graph, interfaces);
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

        match launch_candidate_for_provider_with_capabilities(
            graph,
            modules,
            settings,
            interfaces,
            active_provider,
            effective_capabilities,
        ) {
            Ok(candidate) => candidates.push(candidate),
            Err(status) => statuses.push(status),
        }
    }

    (candidates, statuses)
}

pub(in crate::shell) fn launch_candidate_for_provider_with_capabilities<
    T: ServiceCatalogView + ?Sized,
>(
    graph: &InstalledModuleGraph,
    modules: &HashMap<String, ModuleInstance>,
    settings: &SettingsStore,
    interfaces: &T,
    provider: &BackendProviderNode,
    effective_capabilities: &HashMap<String, EffectiveCapabilities>,
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

    let entrypoint_path = match mesh_core_module::package::resolve_contained_module_file(
        &module.path,
        entrypoint,
        "backend entrypoint",
    ) {
        Ok(path) => path,
        Err(error) => {
            return Err(BackendLifecycleStatusRecord {
                interface,
                provider_id: Some(provider.module_id.clone()),
                status: "missing_entrypoint",
                message: format!(
                    "backend provider {} entrypoint is invalid or unreadable: {error}",
                    provider.module_id
                ),
            });
        }
    };
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

    let Some(effective) = effective_capabilities.get(&provider.module_id) else {
        return Err(BackendLifecycleStatusRecord {
            interface,
            provider_id: Some(provider.module_id.clone()),
            status: "missing_capability",
            message: format!(
                "backend provider {} has no activation-resolved capability grant",
                provider.module_id
            ),
        });
    };
    let capabilities = effective.granted_ids().map(str::to_string).collect();
    let settings = settings.namespace(&provider.module_id);
    let command_registry = interfaces
        .resolve(&interface, None)
        .contract
        .as_deref()
        .map(mesh_core_scripting::BackendCommandRegistry::from_contract);
    let event_registry = interfaces
        .resolve(&interface, None)
        .contract
        .as_deref()
        .map(mesh_core_scripting::BackendEventRegistry::from_contract);
    Ok(BackendLaunchCandidate {
        module_id: provider.module_id.clone(),
        service_name: service_name_from_interface(&interface),
        interface,
        entrypoint_path,
        script_source,
        capabilities,
        settings,
        command_registry,
        event_registry,
    })
}

#[cfg(test)]
fn test_effective_capabilities(
    modules: &HashMap<String, ModuleInstance>,
) -> HashMap<String, EffectiveCapabilities> {
    let approvals = modules
        .values()
        .map(|module| {
            let capabilities = module
                .manifest
                .capabilities
                .required
                .iter()
                .chain(module.manifest.capabilities.optional.iter())
                .cloned()
                .collect();
            (module.manifest.package.id.clone(), capabilities)
        })
        .collect::<Vec<_>>();
    let policy = CapabilityPolicy::from_approvals(approvals);
    modules
        .iter()
        .map(|(module_id, module)| {
            let effective = policy
                .resolve(
                    module_id,
                    &module.manifest.capabilities.required,
                    &module.manifest.capabilities.optional,
                )
                .expect("test backend manifests must use known capabilities");
            (module_id.clone(), effective)
        })
        .collect()
}

fn backend_requirement_statuses<T: ServiceCatalogView + ?Sized>(
    graph: &InstalledModuleGraph,
    interfaces: &T,
) -> Vec<BackendLifecycleStatusRecord> {
    let core_service_providers = CoreServiceRegistry::builtin();
    let is_core_provider = |provider: &mesh_core_service::InterfaceProvider| {
        core_service_providers.is_provider(&provider.interface, &provider.provider_module)
    };
    let mut statuses = Vec::new();
    for frontend in graph.frontend_modules() {
        let Some(requirements) = graph.requirements_for_frontend(&frontend.id) else {
            continue;
        };
        for interface in requirements.backend.keys() {
            let providers = interfaces.providers_for(interface);
            let shell_owned = providers.iter().any(is_core_provider);
            if graph.backend_providers_for_interface(interface).is_empty() && providers.is_empty() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "unmet_backend_requirement",
                    message: format!(
                        "frontend module {} requires {interface}, but no enabled backend provider is installed",
                        frontend.id
                    ),
                });
            } else if graph.active_provider(interface).is_none() && !shell_owned {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "no_active_provider",
                    message: format!(
                        "frontend module {} requires {interface}, but no active provider is selected",
                        frontend.id
                    ),
                });
            } else if !shell_owned
                && let Some(record) = graph.health().iter().find(|record| {
                    record.module_id == frontend.id
                        && record.interface.as_deref() == Some(interface.as_str())
                        && record.status == "required_interface_unavailable"
                })
            {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: record.provider_id.clone(),
                    status: "unmet_backend_requirement",
                    message: record.message.clone(),
                });
            }
        }
        for interface in requirements.optional_backend.keys() {
            let providers = interfaces.providers_for(interface);
            let shell_owned = providers.iter().any(is_core_provider);
            if graph.backend_providers_for_interface(interface).is_empty() && providers.is_empty() {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "optional_backend_unavailable",
                    message: format!(
                        "frontend module {} can use optional {interface}, but no enabled backend provider is installed",
                        frontend.id
                    ),
                });
            } else if graph.active_provider(interface).is_none() && !shell_owned {
                statuses.push(BackendLifecycleStatusRecord {
                    interface: interface.clone(),
                    provider_id: Some(frontend.id.clone()),
                    status: "optional_backend_inactive",
                    message: format!(
                        "frontend module {} can use optional {interface}, but no active provider is selected",
                        frontend.id
                    ),
                });
            } else if !shell_owned
                && let Some(record) = graph.health().iter().find(|record| {
                    record.module_id == frontend.id
                        && record.interface.as_deref() == Some(interface.as_str())
                        && record.status == "optional_interface_unavailable"
                })
            {
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

fn validate_backend_provider_contract<T: ServiceCatalogView + ?Sized>(
    interface: &str,
    provider: &BackendProviderNode,
    interfaces: &T,
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
