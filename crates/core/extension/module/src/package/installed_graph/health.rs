use super::*;
use crate::manifest;
use std::collections::HashMap;
use std::path::Path;

pub(super) fn build_graph_health(
    backend_providers: &HashMap<String, Vec<BackendProviderNode>>,
    active_providers: &HashMap<String, String>,
    frontend_requirements: &HashMap<String, FrontendRequirementSet>,
    diagnostics: &[ModuleGraphDiagnostic],
) -> Vec<ModuleGraphHealthRecord> {
    let mut health = Vec::new();
    let unavailable_backend_modules = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.status == "missing_required_binary")
        .map(|diagnostic| diagnostic.module_id.as_str())
        .collect::<std::collections::HashSet<_>>();

    let provider_available = |provider: &BackendProviderNode| {
        !unavailable_backend_modules.contains(provider.module_id.as_str())
    };

    for provider in backend_providers
        .values()
        .flat_map(|providers| providers.iter())
    {
        let available = provider_available(provider);
        health.push(ModuleGraphHealthRecord {
            module_id: provider.module_id.clone(),
            interface: Some(provider.interface.clone()),
            provider_id: Some(provider.module_id.clone()),
            status: if available {
                "provider_available".into()
            } else {
                "provider_unavailable".into()
            },
            message: if available {
                format!(
                    "provider {} is available for {}",
                    provider.module_id, provider.interface
                )
            } else {
                format!(
                    "provider {} is unavailable for {} because a required runtime dependency is missing",
                    provider.module_id, provider.interface
                )
            },
        });
    }

    let mut interfaces = std::collections::BTreeSet::new();
    interfaces.extend(backend_providers.keys().cloned());
    interfaces.extend(active_providers.keys().cloned());
    for requirements in frontend_requirements.values() {
        interfaces.extend(requirements.backend.keys().cloned());
        interfaces.extend(requirements.optional_backend.keys().cloned());
    }

    let mut interface_statuses: HashMap<String, (String, Option<String>, String)> = HashMap::new();
    for interface in interfaces {
        let providers = backend_providers
            .get(&interface)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let active_provider = active_providers.get(&interface);
        let (status, provider_id, message) = if providers.is_empty() {
            (
                "interface_unavailable".to_string(),
                None,
                format!("interface {interface} has no installed backend provider"),
            )
        } else if let Some(active_provider_id) = active_provider {
            let active = providers
                .iter()
                .find(|provider| provider.module_id == *active_provider_id);
            match active {
                Some(provider) if provider_available(provider) => (
                    "interface_available".to_string(),
                    Some(provider.module_id.clone()),
                    format!(
                        "interface {interface} is available through active provider {}",
                        provider.module_id
                    ),
                ),
                Some(provider) => (
                    "interface_unavailable".to_string(),
                    Some(provider.module_id.clone()),
                    format!(
                        "interface {interface} is unavailable because active provider {} is unavailable",
                        provider.module_id
                    ),
                ),
                None => (
                    "interface_unavailable".to_string(),
                    Some(active_provider_id.clone()),
                    format!(
                        "interface {interface} selects missing active provider {active_provider_id}"
                    ),
                ),
            }
        } else {
            (
                "interface_unconfigured".to_string(),
                None,
                format!("interface {interface} has installed providers but no active provider"),
            )
        };

        interface_statuses.insert(
            interface.clone(),
            (status.clone(), provider_id.clone(), message.clone()),
        );
        health.push(ModuleGraphHealthRecord {
            module_id: provider_id
                .clone()
                .unwrap_or_else(|| format!("interface:{interface}")),
            interface: Some(interface),
            provider_id,
            status,
            message,
        });
    }

    for requirements in frontend_requirements.values() {
        for interface in requirements.backend.keys() {
            if let Some((status, provider_id, message)) = interface_statuses.get(interface)
                && status != "interface_available"
            {
                health.push(ModuleGraphHealthRecord {
                    module_id: requirements.module_id.clone(),
                    interface: Some(interface.clone()),
                    provider_id: provider_id.clone(),
                    status: "required_interface_unavailable".into(),
                    message: format!(
                        "frontend module {} requires {interface}, but {message}",
                        requirements.module_id
                    ),
                });
            }
        }
        for interface in requirements.optional_backend.keys() {
            if let Some((status, provider_id, message)) = interface_statuses.get(interface)
                && status != "interface_available"
            {
                health.push(ModuleGraphHealthRecord {
                    module_id: requirements.module_id.clone(),
                    interface: Some(interface.clone()),
                    provider_id: provider_id.clone(),
                    status: "optional_interface_unavailable".into(),
                    message: format!(
                        "frontend module {} can use optional {interface}, but {message}",
                        requirements.module_id
                    ),
                });
            }
        }
    }

    health.sort_by(|a, b| {
        a.module_id
            .cmp(&b.module_id)
            .then_with(|| a.interface.cmp(&b.interface))
            .then_with(|| a.provider_id.cmp(&b.provider_id))
            .then_with(|| a.status.cmp(&b.status))
    });
    health
}

/// Return whether a declared executable can be resolved from an explicit path
/// or the current process PATH.
pub fn binary_available(name: &str) -> bool {
    if name.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(name).is_file();
    }
    std::env::var_os("PATH")
        .map(|path_var| std::env::split_paths(&path_var).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

pub(super) fn binary_package_hint(binary: &manifest::BinaryDependency) -> String {
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
