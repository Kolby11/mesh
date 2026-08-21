use super::super::trust::TrustAssessment;
use super::super::{
    InterfaceRelationship, ModuleKind, ModuleManifest, ModuleManifestDiagnostic,
    ModuleManifestError, NodeSlotOverride, ResolutionOutcome, RootModuleGraphManifest,
    SlotOverride, TrustPolicy, TrustTier, apply_slot_override, parse_module_entrypoint,
    resolve_closure,
};
use super::*;
use crate::manifest;
use mesh_core_service::{
    InterfaceContract, parse_contract_version, parse_interface_contract, parse_version_req,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

fn validate_unique_graph_identities(
    modules: &HashMap<String, InstalledModuleNode>,
) -> Result<(), ModuleManifestError> {
    let mut interfaces = modules
        .values()
        .filter(|module| module.enabled && module.kind == ModuleKind::Interface)
        .filter_map(|module| {
            module.manifest.mesh.interface.as_ref().map(|interface| {
                (
                    mesh_core_service::canonical_interface_name(&interface.name),
                    module.id.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    interfaces.sort();
    for pair in interfaces.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(ModuleManifestError::Validation(format!(
                "duplicate active interface declaration '{}' in modules {} and {}",
                pair[0].0, pair[0].1, pair[1].1
            )));
        }
    }

    let mut extension_points = modules
        .values()
        .filter(|module| module.enabled && module.kind == ModuleKind::Interface)
        .flat_map(|module| {
            module
                .manifest
                .mesh
                .extension_points
                .keys()
                .map(|name| (name.clone(), module.id.clone()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    extension_points.sort();
    for pair in extension_points.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(ModuleManifestError::Validation(format!(
                "duplicate active extension point declaration '{}' in modules {} and {}",
                pair[0].0, pair[0].1, pair[1].1
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct LoadedModuleManifest {
    pub manifest: ModuleManifest,
    pub path: PathBuf,
    pub source: ModuleManifestSource,
    pub diagnostics: Vec<ModuleManifestDiagnostic>,
}

/// Select providers only after the graph has parsed interface contracts. A
/// provider is eligible for an interface when its offered version and the
/// interface contract version both satisfy the consumer's requested range.
/// Required consumers participate in one shared selection decision; optional
/// consumers receive a degradation diagnostic when the selected provider does
/// not match their range.
fn resolve_active_providers(
    requested: &HashMap<String, String>,
    providers: &HashMap<String, Vec<BackendProviderNode>>,
    disabled_provider_interfaces: &BTreeSet<String>,
    requirements: &HashMap<String, FrontendRequirementSet>,
    declarations: &HashMap<String, InterfaceDeclarationNode>,
    contracts: &HashMap<String, InterfaceContract>,
) -> (
    HashMap<String, String>,
    Vec<ModuleGraphDiagnostic>,
    BTreeSet<String>,
) {
    let mut interfaces = BTreeSet::new();
    interfaces.extend(providers.keys().cloned());
    interfaces.extend(declarations.keys().cloned());
    interfaces.extend(requirements.values().flat_map(|requirements| {
        requirements
            .backend
            .keys()
            .chain(requirements.optional_backend.keys())
            .cloned()
    }));
    interfaces.extend(requested.keys().cloned());

    let mut active = HashMap::new();
    let mut diagnostics = Vec::new();
    let mut blocked_frontends = BTreeSet::new();

    for interface in interfaces {
        // Core-provided interfaces are registered by the shell after graph
        // construction. Without a graph declaration or graph provider there is
        // no module-owned version to validate here, so leave that decision to
        // the core registry rather than disabling every shipped frontend.
        if !providers.contains_key(&interface) && !declarations.contains_key(&interface) {
            continue;
        }

        let required_consumers = requirements
            .values()
            .filter(|requirements| requirements.backend.contains_key(&interface))
            .collect::<Vec<_>>();
        let optional_consumers = requirements
            .values()
            .filter(|requirements| requirements.optional_backend.contains_key(&interface))
            .collect::<Vec<_>>();
        let provider_candidates = providers
            .get(&interface)
            .map(|providers| {
                providers
                    .iter()
                    .filter(|provider| {
                        required_consumers.iter().all(|consumer| {
                            consumer.backend.get(&interface).is_some_and(|requirement| {
                                provider_satisfies_requirement(
                                    provider,
                                    requirement,
                                    declarations.get(&interface),
                                    contracts.get(&interface),
                                )
                            })
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let has_graph_provider = providers
            .get(&interface)
            .is_some_and(|list| !list.is_empty())
            || disabled_provider_interfaces.contains(&interface);

        let selected = if let Some(requested_module) = requested.get(&interface) {
            provider_candidates
                .iter()
                .copied()
                .find(|provider| provider.module_id == *requested_module)
        } else if provider_candidates.len() == 1 {
            provider_candidates.first().copied()
        } else {
            None
        };

        if let Some(provider) = selected {
            active.insert(interface.clone(), provider.module_id.clone());
        }

        for consumer in required_consumers {
            let Some(requirement) = consumer.backend.get(&interface) else {
                continue;
            };
            match selected {
                Some(provider)
                    if provider_satisfies_requirement(
                        provider,
                        requirement,
                        declarations.get(&interface),
                        contracts.get(&interface),
                    ) => {}
                _ => {
                    let status = if provider_candidates.is_empty() {
                        "required_interface_version_mismatch"
                    } else {
                        "required_interface_unavailable"
                    };
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: consumer.module_id.clone(),
                        contribution_id: Some(format!(
                            "{}:interface:{}",
                            consumer.module_id, interface
                        )),
                        status: status.into(),
                        message: format!(
                            "module {} requires {interface} {requirement}, but no selected provider satisfies the contract and provider versions",
                            consumer.module_id
                        ),
                    });
                    if has_graph_provider {
                        blocked_frontends.insert(consumer.module_id.clone());
                    }
                }
            }
        }

        for consumer in optional_consumers {
            let Some(requirement) = consumer.optional_backend.get(&interface) else {
                continue;
            };
            let (status, message) = match selected {
                None => (
                    "optional_interface_unavailable",
                    format!(
                        "module {} optionally uses {interface} {requirement}, but no compatible provider is active",
                        consumer.module_id
                    ),
                ),
                Some(provider)
                    if provider_satisfies_requirement(
                        provider,
                        requirement,
                        declarations.get(&interface),
                        contracts.get(&interface),
                    ) =>
                {
                    continue;
                }
                Some(provider) => (
                    "optional_interface_version_mismatch",
                    format!(
                        "module {} optionally uses {interface} {requirement}, but provider {} is incompatible",
                        consumer.module_id, provider.module_id
                    ),
                ),
            };
            diagnostics.push(ModuleGraphDiagnostic {
                module_id: consumer.module_id.clone(),
                contribution_id: Some(format!(
                    "{}:optional-interface:{}",
                    consumer.module_id, interface
                )),
                status: status.into(),
                message,
            });
        }
    }

    (active, diagnostics, blocked_frontends)
}

fn provider_satisfies_requirement(
    provider: &BackendProviderNode,
    requirement: &str,
    declaration: Option<&InterfaceDeclarationNode>,
    contract: Option<&InterfaceContract>,
) -> bool {
    let Some(request) = parse_version_req(requirement) else {
        return false;
    };
    if let Some(contract) = contract {
        if !request.matches(&contract.version) {
            return false;
        }
    } else if let Some(version) = declaration
        .and_then(|declaration| declaration.version.as_deref())
        .and_then(parse_contract_version)
        && !request.matches(&version)
    {
        return false;
    }

    let Some(provider_version) = provider.version.as_deref() else {
        // Older providers without a version inherit the declared contract
        // version, which was checked above when one exists.
        return true;
    };
    parse_contract_version(provider_version).is_some_and(|version| request.matches(&version))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleManifestSource {
    CanonicalModuleJson,
}

#[derive(Debug, Clone)]
pub struct InstalledModuleGraph {
    modules: HashMap<String, InstalledModuleNode>,
    trust_policy: TrustPolicy,
    backend_providers: HashMap<String, Vec<BackendProviderNode>>,
    active_providers: HashMap<String, String>,
    frontend_requirements: HashMap<String, FrontendRequirementSet>,
    interface_declarations: HashMap<String, InterfaceDeclarationNode>,
    /// Typed contracts parsed once from the declarations' contract JSON.
    interface_contracts: HashMap<String, InterfaceContract>,
    interface_guidance: Vec<InterfaceGuidanceRecord>,
    diagnostics: Vec<ModuleGraphDiagnostic>,
    health: Vec<ModuleGraphHealthRecord>,
    contributions: ModuleContributionIndex,
    resolution: ResolutionOutcome,
    /// Host↔contribution matching for every declared extension point, keyed by
    /// `(host module id, point contract name)`.
    extension_points: HashMap<(String, String), Vec<ResolvedExtensionPointContribution>>,
    /// Effective explicit placements for named customizable slots.
    node_slots:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, NodeSlotOverride>>,
    layout_entrypoint: Option<ResolvedLayoutEntrypoint>,
}

/// The normalized, user-facing change set between two resolved graphs.
/// Package planning and dry-run output use this rather than comparing raw
/// manifests, so disabled modules and provider selections are included in the
/// same decision surface as module additions and removals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleGraphDiff {
    pub added_modules: Vec<String>,
    pub removed_modules: Vec<String>,
    pub updated_modules: Vec<String>,
    pub enabled_modules: Vec<String>,
    pub disabled_modules: Vec<String>,
    pub provider_changes: Vec<ProviderChange>,
    pub profile_effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderChange {
    pub interface: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

impl ModuleGraphDiff {
    pub fn is_empty(&self) -> bool {
        self.added_modules.is_empty()
            && self.removed_modules.is_empty()
            && self.updated_modules.is_empty()
            && self.enabled_modules.is_empty()
            && self.disabled_modules.is_empty()
            && self.provider_changes.is_empty()
            && self.profile_effects.is_empty()
    }
}

/// What the active composition contributes to graph resolution beyond the
/// module set: how it arranges extension points, and which user overrides it no
/// longer has a home for.
#[derive(Debug, Clone, Default)]
pub struct CompositionContext {
    pub slots: std::collections::BTreeMap<String, SlotOverride>,
    pub node_slots:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, NodeSlotOverride>>,
    pub orphaned_overrides: Vec<String>,
}

impl InstalledModuleGraph {
    pub fn from_parts(
        root: RootModuleGraphManifest,
        modules: Vec<LoadedModuleManifest>,
    ) -> Result<Self, ModuleManifestError> {
        Self::from_parts_with_composition(root, modules, CompositionContext::default())
    }

    pub fn from_parts_with_composition(
        root: RootModuleGraphManifest,
        modules: Vec<LoadedModuleManifest>,
        composition: CompositionContext,
    ) -> Result<Self, ModuleManifestError> {
        Self::from_parts_with_trust(root, modules, composition, BTreeMap::new())
    }

    pub(in crate::package) fn from_parts_with_trust(
        root: RootModuleGraphManifest,
        modules: Vec<LoadedModuleManifest>,
        composition: CompositionContext,
        trust_by_module: BTreeMap<String, TrustTier>,
    ) -> Result<Self, ModuleManifestError> {
        let provenance_by_module = trust_by_module
            .into_iter()
            .map(|(module_id, trust)| (module_id, TrustAssessment::accepted(trust)))
            .collect();
        Self::from_parts_with_provenance(root, modules, composition, provenance_by_module)
    }

    pub(in crate::package) fn from_parts_with_provenance(
        root: RootModuleGraphManifest,
        modules: Vec<LoadedModuleManifest>,
        composition: CompositionContext,
        provenance_by_module: BTreeMap<String, TrustAssessment>,
    ) -> Result<Self, ModuleManifestError> {
        root.validate()?;
        let trust_policy = root.trust_policy.clone();
        let mut loaded_by_id = HashMap::new();
        for loaded in modules {
            loaded.manifest.validate()?;
            if loaded_by_id
                .insert(loaded.manifest.name.clone(), loaded)
                .is_some()
            {
                return Err(ModuleManifestError::Validation(
                    "duplicate loaded module package".into(),
                ));
            }
        }

        let mut graph_modules = HashMap::new();
        let mut backend_providers: HashMap<String, Vec<BackendProviderNode>> = HashMap::new();
        let mut frontend_requirements = HashMap::new();
        let mut interface_declarations = HashMap::new();
        let mut contributions = ModuleContributionIndex::default();
        let mut manual_diagnostics: Vec<ModuleGraphDiagnostic> = Vec::new();

        for (module_id, entry) in &root.modules {
            let loaded = loaded_by_id.get(module_id).ok_or_else(|| {
                ModuleManifestError::Validation(format!(
                    "root package references module {module_id} but no module package was loaded"
                ))
            })?;
            if loaded.manifest.mesh.kind != entry.kind {
                return Err(ModuleManifestError::Validation(format!(
                    "module {module_id} kind mismatch: root has {:?}, package has {:?}",
                    entry.kind, loaded.manifest.mesh.kind
                )));
            }

            let assessment = provenance_by_module
                .get(module_id)
                .cloned()
                .unwrap_or_else(|| {
                    TrustAssessment::accepted(TrustTier::default_for_module(module_id))
                });
            let trust_allowed = assessment.signature_valid && trust_policy.allows(assessment.tier);
            if entry.enabled && !assessment.signature_valid {
                manual_diagnostics.push(ModuleGraphDiagnostic {
                    module_id: module_id.clone(),
                    contribution_id: Some(format!("{module_id}:signature")),
                    status: "signature_invalid".into(),
                    message: format!(
                        "module {module_id} provenance signature is invalid: {}",
                        assessment.error.as_deref().unwrap_or("verification failed")
                    ),
                });
            } else if entry.enabled && !trust_allowed {
                manual_diagnostics.push(ModuleGraphDiagnostic {
                    module_id: module_id.clone(),
                    contribution_id: Some(format!("{module_id}:trust")),
                    status: "trust_policy_blocked".into(),
                    message: format!(
                        "module {module_id} has {:?} provenance, below the configured {:?} trust minimum",
                        assessment.tier,
                        trust_policy.minimum
                    ),
                });
            }

            let node = InstalledModuleNode {
                id: module_id.clone(),
                kind: entry.kind,
                path: entry.path.clone(),
                enabled: entry.enabled && trust_allowed,
                trust: assessment.tier,
                manifest_path: loaded.path.clone(),
                manifest_source: loaded.source,
                manifest: loaded.manifest.clone(),
            };

            graph_modules.insert(module_id.clone(), node);
        }

        // Resolve module edges before indexing any contributions. A required
        // dependency failure disables only the affected module closure; an
        // optional edge is retained as a degraded record and never blocks its
        // requester.
        let enabled_ids = graph_modules
            .values()
            .filter(|module| module.enabled)
            .map(|module| module.id.as_str())
            .collect::<Vec<_>>();
        let enabled_manifests = graph_modules
            .values()
            .map(|module| &module.manifest)
            .collect::<Vec<_>>();
        let resolution = resolve_closure(enabled_ids.iter().copied(), enabled_manifests);
        let active_module_ids = resolution.active_modules(enabled_ids.iter().copied());
        for node in graph_modules.values_mut() {
            node.enabled = active_module_ids.contains(&node.id);
        }
        validate_unique_graph_identities(&graph_modules)?;
        for (module_id, reasons) in &resolution.blocked {
            manual_diagnostics.push(ModuleGraphDiagnostic {
                module_id: module_id.clone(),
                contribution_id: Some(format!("{module_id}:dependencies")),
                status: "module_dependency_blocked".into(),
                message: format!(
                    "module {module_id} is not activated because {}",
                    reasons.iter().cloned().collect::<Vec<_>>().join("; ")
                ),
            });
        }
        for conflict in &resolution.conflicts {
            manual_diagnostics.push(ModuleGraphDiagnostic {
                module_id: conflict.module_id.clone(),
                contribution_id: Some(format!("{}:version", conflict.module_id)),
                status: "module_version_conflict".into(),
                message: conflict.message(),
            });
        }
        for (dependency_id, requirers) in &resolution.missing {
            for requester in requirers {
                manual_diagnostics.push(ModuleGraphDiagnostic {
                    module_id: requester.clone(),
                    contribution_id: Some(format!("{requester}:dependency:{dependency_id}")),
                    status: "missing_required_module_dependency".into(),
                    message: format!(
                        "module {requester} requires module {dependency_id}, but it is not installed"
                    ),
                });
            }
        }
        for optional in &resolution.optional {
            manual_diagnostics.push(ModuleGraphDiagnostic {
                module_id: optional.module_id.clone(),
                contribution_id: Some(format!(
                    "{}:dependency:{}",
                    optional.module_id, optional.dependency_id
                )),
                status: optional.status.clone(),
                message: format!("{} (requested {})", optional.message, optional.requirement),
            });
        }
        for node in graph_modules.values().filter(|node| node.enabled) {
            for (dependency_id, spec) in &node.manifest.mesh.dependencies.modules {
                if !spec.is_optional() {
                    continue;
                }
                if let Some(dependency) = graph_modules.get(dependency_id)
                    && !dependency.enabled
                {
                    manual_diagnostics.push(ModuleGraphDiagnostic {
                        module_id: node.id.clone(),
                        contribution_id: Some(format!(
                            "{}:dependency:{}",
                            node.id, dependency_id
                        )),
                        status: "optional_module_dependency_disabled".into(),
                        message: format!(
                            "optional module {dependency_id} is installed but disabled; module {} continues without it",
                            node.id
                        ),
                    });
                }
            }
        }

        for node in graph_modules.values() {
            if !node.enabled {
                continue;
            }
            if node.kind == ModuleKind::Frontend {
                frontend_requirements.insert(
                    node.id.clone(),
                    FrontendRequirementSet::from_manifest(&node.id, &node.manifest),
                );
            }
            if node.kind == ModuleKind::Interface
                && let Some(interface) = &node.manifest.mesh.interface
            {
                let name = mesh_core_service::canonical_interface_name(&interface.name);
                let declaration = InterfaceDeclarationNode {
                    source: ContributionSource::new(node, &name),
                    module_id: node.id.clone(),
                    name: name.clone(),
                    version: interface.version.clone(),
                    contract: interface.contract.clone(),
                    domain: interface.domain.clone(),
                    extends: interface
                        .extends
                        .as_deref()
                        .map(mesh_core_service::canonical_interface_name),
                    relationship: interface.effective_relationship(),
                    reason: interface.reason.clone(),
                };
                interface_declarations.insert(name, declaration);
            }
            if node.kind == ModuleKind::Backend {
                for provided in node.manifest.mesh.implementations() {
                    let interface =
                        mesh_core_service::canonical_interface_name(&provided.interface);
                    let provider = BackendProviderNode {
                        source: ContributionSource::new(
                            node,
                            provided.provider.as_deref().unwrap_or(&interface),
                        ),
                        module_id: node.id.clone(),
                        interface: interface.clone(),
                        version: provided.version.clone(),
                        base_module: provided.base_module.clone(),
                        provider: provided.provider.clone(),
                        label: provided.label.clone(),
                        priority: provided.priority,
                        required_capabilities: node.manifest.mesh.capabilities.required.clone(),
                        optional_capabilities: node.manifest.mesh.capabilities.optional.clone(),
                    };
                    backend_providers
                        .entry(interface)
                        .or_default()
                        .push(provider);
                }
            }
        }

        for providers in backend_providers.values_mut() {
            providers.sort_by(|a, b| {
                b.priority
                    .cmp(&a.priority)
                    .then_with(|| a.module_id.cmp(&b.module_id))
            });
        }

        // Collect inline interface declarations from backend modules
        // (`mesh.interfaces`). A standalone interface module always wins for
        // the same interface name; among duplicate inline declarations the
        // highest-priority provider's copy wins. Losers become diagnostics,
        // not errors — the graph stays loadable.
        let mut backend_ids: Vec<String> = graph_modules
            .values()
            .filter(|node| node.enabled && node.kind == ModuleKind::Backend)
            .map(|node| node.id.clone())
            .collect();
        backend_ids.sort();
        let provider_priority = |interface: &str, module_id: &str| -> u32 {
            backend_providers
                .get(interface)
                .and_then(|providers| {
                    providers
                        .iter()
                        .find(|provider| provider.module_id == module_id)
                })
                .map(|provider| provider.priority)
                .unwrap_or(0)
        };
        for module_id in backend_ids {
            let Some(node) = graph_modules.get(&module_id) else {
                continue;
            };
            for interface in &node.manifest.mesh.interfaces {
                let name = mesh_core_service::canonical_interface_name(&interface.name);
                let candidate = InterfaceDeclarationNode {
                    source: ContributionSource::new(node, &name),
                    module_id: module_id.clone(),
                    name: name.clone(),
                    version: interface.version.clone(),
                    contract: interface.contract.clone(),
                    domain: interface.domain.clone(),
                    extends: interface
                        .extends
                        .as_deref()
                        .map(mesh_core_service::canonical_interface_name),
                    relationship: interface.effective_relationship(),
                    reason: interface.reason.clone(),
                };
                match interface_declarations.get(&name) {
                    None => {
                        interface_declarations.insert(name.clone(), candidate);
                    }
                    Some(existing) => {
                        let existing_is_interface_module = graph_modules
                            .get(&existing.module_id)
                            .is_some_and(|node| node.kind == ModuleKind::Interface);
                        let replace = !existing_is_interface_module
                            && provider_priority(&candidate.name, &candidate.module_id)
                                > provider_priority(&existing.name, &existing.module_id);
                        let (winner_id, loser_id) = if replace {
                            (candidate.module_id.clone(), existing.module_id.clone())
                        } else {
                            (existing.module_id.clone(), candidate.module_id.clone())
                        };
                        manual_diagnostics.push(ModuleGraphDiagnostic {
                            module_id: loser_id.clone(),
                            contribution_id: Some(format!(
                                "{loser_id}:interface:{}",
                                candidate.name
                            )),
                            status: "duplicate_interface_declaration".into(),
                            message: format!(
                                "interface {} is declared by both {winner_id} and {loser_id}; the declaration from {winner_id} wins",
                                candidate.name
                            ),
                        });
                        if replace {
                            interface_declarations.insert(candidate.name.clone(), candidate);
                        }
                    }
                }
            }
        }

        for (interface, module_id) in &root.providers {
            let Some(node) = graph_modules.get(module_id) else {
                return Err(ModuleManifestError::Validation(format!(
                    "active provider {module_id} for {interface} is not installed"
                )));
            };
            if !node.enabled {
                manual_diagnostics.push(ModuleGraphDiagnostic {
                    module_id: module_id.clone(),
                    contribution_id: Some(format!("{module_id}:provider:{interface}")),
                    status: "active_provider_disabled".into(),
                    message: format!(
                        "active provider {module_id} for {interface} is disabled and cannot be activated"
                    ),
                });
                continue;
            }
            if node.kind != ModuleKind::Backend {
                return Err(ModuleManifestError::Validation(format!(
                    "active provider {module_id} for {interface} is not a backend module"
                )));
            }
            let provides_interface = backend_providers
                .get(interface)
                .map(|providers| {
                    providers
                        .iter()
                        .any(|provider| provider.module_id == *module_id)
                })
                .unwrap_or(false);
            if !provides_interface {
                return Err(ModuleManifestError::Validation(format!(
                    "active provider {module_id} does not provide {interface}"
                )));
            }
        }

        // Parse every declared contract once. Invalid contracts become
        // diagnostics and the interface simply has no typed contract.
        let mut interface_contracts: HashMap<String, InterfaceContract> = HashMap::new();
        for declaration in interface_declarations.values() {
            let Some(value) = &declaration.contract else {
                continue;
            };
            let version = declaration.version.as_deref().unwrap_or("1.0");
            match parse_interface_contract(&declaration.name, version, value) {
                Ok(contract) => {
                    interface_contracts.insert(contract.interface.clone(), contract);
                }
                Err(err) => manual_diagnostics.push(ModuleGraphDiagnostic {
                    module_id: declaration.module_id.clone(),
                    contribution_id: Some(format!(
                        "{}:interface:{}",
                        declaration.module_id, declaration.name
                    )),
                    status: "invalid_interface_contract".into(),
                    message: format!(
                        "interface {} declares an invalid contract: {err}",
                        declaration.name
                    ),
                }),
            }
        }

        let disabled_provider_interfaces = graph_modules
            .values()
            .filter(|node| !node.enabled && node.kind == ModuleKind::Backend)
            .flat_map(|node| {
                node.manifest.mesh.implementations().map(|implementation| {
                    mesh_core_service::canonical_interface_name(&implementation.interface)
                })
            })
            .collect::<BTreeSet<_>>();
        let requested_providers = root
            .providers
            .iter()
            .map(|(interface, module_id)| {
                (
                    mesh_core_service::canonical_interface_name(interface),
                    module_id.clone(),
                )
            })
            .collect::<HashMap<_, _>>();
        let (active_providers, compatibility_diagnostics, blocked_frontends) =
            resolve_active_providers(
                &requested_providers,
                &backend_providers,
                &disabled_provider_interfaces,
                &frontend_requirements,
                &interface_declarations,
                &interface_contracts,
            );
        manual_diagnostics.extend(compatibility_diagnostics);
        for module_id in blocked_frontends {
            if let Some(module) = graph_modules.get_mut(&module_id) {
                module.enabled = false;
            }
            frontend_requirements.remove(&module_id);
            manual_diagnostics.push(ModuleGraphDiagnostic {
                module_id: module_id.clone(),
                contribution_id: Some(format!("{module_id}:interfaces")),
                status: "interface_dependency_blocked".into(),
                message: format!(
                    "module {module_id} is not activated because a required interface has no compatible provider"
                ),
            });
        }

        // Contributions are indexed only after module and interface/provider
        // compatibility has made its activation decisions.
        for node in graph_modules.values() {
            if node.enabled {
                contributions.index_module(node)?;
            }
        }

        let layout_entrypoint = match root.layout {
            Some(layout) => {
                let (module_id, entrypoint_id) = parse_module_entrypoint(&layout.entrypoint)
                    .ok_or_else(|| {
                        ModuleManifestError::Validation(format!(
                            "invalid layout entrypoint {}",
                            layout.entrypoint
                        ))
                    })?;
                let node = graph_modules.get(module_id).ok_or_else(|| {
                    ModuleManifestError::Validation(format!(
                        "layout entrypoint module {module_id} is not installed"
                    ))
                })?;
                if node.kind != ModuleKind::Frontend {
                    return Err(ModuleManifestError::Validation(format!(
                        "layout entrypoint module {module_id} must be a frontend module"
                    )));
                }
                if !node.enabled {
                    manual_diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module_id.into(),
                        contribution_id: Some(layout.entrypoint.clone()),
                        status: "layout_entrypoint_blocked".into(),
                        message: format!(
                            "layout entrypoint {} is unavailable because its module was not activated",
                            layout.entrypoint
                        ),
                    });
                    None
                } else {
                    let contribution = contributions
                        .layout
                        .iter()
                        .find(|item| item.module_id == module_id && item.id == entrypoint_id)
                        .ok_or_else(|| {
                            ModuleManifestError::Validation(format!(
                                "layout contribution {} not found",
                                layout.entrypoint
                            ))
                        })?;
                    Some(ResolvedLayoutEntrypoint {
                        module_id: module_id.into(),
                        entrypoint_id: entrypoint_id.into(),
                        path: contribution.path.clone(),
                    })
                }
            }
            None => None,
        };

        let (mut extension_points, extension_point_diagnostics) =
            resolve_extension_points(&contributions);
        manual_diagnostics.extend(extension_point_diagnostics);

        // The composition has the last word on the UI its members contribute:
        // it may replace a page, hide one, or fix the order without editing any
        // member module.
        for ((_, point), entries) in extension_points.iter_mut() {
            let Some(over) = composition.slots.get(point.as_str()) else {
                continue;
            };
            apply_slot_override(
                entries,
                over,
                |entry| entry.source_module_id.clone(),
                |entry, module_id| entry.source_module_id = module_id,
            );
        }
        // A user override with no matching root is retained, never dropped: an
        // upstream rename must not silently discard the user's work.
        for instance_id in &composition.orphaned_overrides {
            manual_diagnostics.push(ModuleGraphDiagnostic {
                module_id: instance_id
                    .split('#')
                    .next()
                    .unwrap_or(instance_id)
                    .to_string(),
                contribution_id: Some(instance_id.clone()),
                status: "orphaned_profile_override".into(),
                message: format!(
                    "profile override for {instance_id} has no matching root in the active composition; it is retained. Clear it with `mesh profile prune`"
                ),
            });
        }

        let interface_guidance = build_interface_guidance(&interface_declarations);
        let diagnostics = build_graph_diagnostics(
            &graph_modules,
            &frontend_requirements,
            &backend_providers,
            &contributions,
            &interface_contracts,
            manual_diagnostics,
        );
        let mut diagnostics = diagnostics;
        if authoring_diagnostics_enabled() {
            diagnostics.extend(build_authoring_diagnostics(&graph_modules));
            sort_diagnostics(&mut diagnostics);
        }
        let health = build_graph_health(
            &backend_providers,
            &active_providers,
            &frontend_requirements,
            &diagnostics,
        );

        Ok(Self {
            modules: graph_modules,
            trust_policy,
            backend_providers,
            active_providers,
            frontend_requirements,
            interface_declarations,
            interface_contracts,
            interface_guidance,
            diagnostics,
            health,
            contributions,
            resolution,
            extension_points,
            node_slots: composition.node_slots,
            layout_entrypoint,
        })
    }

    pub fn module(&self, id: &str) -> Option<&InstalledModuleNode> {
        self.modules.get(id)
    }

    pub fn trust_policy(&self) -> &TrustPolicy {
        &self.trust_policy
    }

    /// Compare two already-validated graphs after dependency, provider, and
    /// profile resolution. This keeps dry-run output tied to the same
    /// normalized inventory the runtime will activate.
    pub fn diff(&self, next: &Self) -> ModuleGraphDiff {
        let before_ids = self.modules.keys().cloned().collect::<BTreeSet<_>>();
        let after_ids = next.modules.keys().cloned().collect::<BTreeSet<_>>();
        let mut diff = ModuleGraphDiff {
            added_modules: after_ids.difference(&before_ids).cloned().collect(),
            removed_modules: before_ids.difference(&after_ids).cloned().collect(),
            ..ModuleGraphDiff::default()
        };

        for module_id in before_ids.intersection(&after_ids) {
            let before = &self.modules[module_id];
            let after = &next.modules[module_id];
            if before.kind != after.kind
                || before.path != after.path
                || before.manifest.version != after.manifest.version
                || before.trust != after.trust
            {
                diff.updated_modules.push(module_id.clone());
            }
            match (before.enabled, after.enabled) {
                (false, true) => diff.enabled_modules.push(module_id.clone()),
                (true, false) => diff.disabled_modules.push(module_id.clone()),
                _ => {}
            }
        }

        let mut interfaces = self
            .active_providers
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        interfaces.extend(next.active_providers.keys().cloned());
        for interface in interfaces {
            let before = self
                .active_provider(&interface)
                .map(|provider| provider.module_id.clone());
            let after = next
                .active_provider(&interface)
                .map(|provider| provider.module_id.clone());
            if before != after {
                diff.provider_changes.push(ProviderChange {
                    interface,
                    before,
                    after,
                });
            }
        }

        let before_layout = self
            .layout_entrypoint
            .as_ref()
            .map(|layout| format!("{}:{}", layout.module_id, layout.entrypoint_id));
        let after_layout = next
            .layout_entrypoint
            .as_ref()
            .map(|layout| format!("{}:{}", layout.module_id, layout.entrypoint_id));
        if before_layout != after_layout {
            diff.profile_effects.push(format!(
                "layout: {} -> {}",
                before_layout.as_deref().unwrap_or("none"),
                after_layout.as_deref().unwrap_or("none")
            ));
        }

        let mut slots = BTreeSet::new();
        slots.extend(self.node_slots.iter().flat_map(|(root, values)| {
            values.keys().map(move |slot| (root.clone(), slot.clone()))
        }));
        slots.extend(next.node_slots.iter().flat_map(|(root, values)| {
            values.keys().map(move |slot| (root.clone(), slot.clone()))
        }));
        for (root, slot) in slots {
            if self
                .node_slots
                .get(&root)
                .and_then(|values| values.get(&slot))
                != next
                    .node_slots
                    .get(&root)
                    .and_then(|values| values.get(&slot))
            {
                diff.profile_effects
                    .push(format!("node slot {root}:{slot} changed"));
            }
        }

        diff.updated_modules.sort();
        diff.provider_changes
            .sort_by(|left, right| left.interface.cmp(&right.interface));
        diff.profile_effects.sort();
        diff
    }

    pub fn enabled_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules
            .values()
            .filter(|module| module.enabled)
            .collect()
    }

    pub fn modules(&self) -> Vec<&InstalledModuleNode> {
        let mut modules = self.modules.values().collect::<Vec<_>>();
        modules.sort_by(|left, right| left.id.cmp(&right.id));
        modules
    }

    pub fn modules_by_kind(&self, kind: ModuleKind) -> Vec<&InstalledModuleNode> {
        self.modules
            .values()
            .filter(|module| module.enabled && module.kind == kind)
            .collect()
    }

    pub fn frontend_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules_by_kind(ModuleKind::Frontend)
    }

    pub fn backend_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules_by_kind(ModuleKind::Backend)
    }

    pub fn interface_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules_by_kind(ModuleKind::Interface)
    }

    pub fn theme_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules_by_kind(ModuleKind::Theme)
    }

    pub fn icon_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules_by_kind(ModuleKind::IconPack)
    }

    pub fn font_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules_by_kind(ModuleKind::FontPack)
    }

    pub fn language_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules_by_kind(ModuleKind::LanguagePack)
    }

    pub fn library_modules(&self) -> Vec<&InstalledModuleNode> {
        self.modules_by_kind(ModuleKind::Library)
    }

    pub fn requirements_for_frontend(&self, module_id: &str) -> Option<&FrontendRequirementSet> {
        self.frontend_requirements.get(module_id)
    }

    pub fn declared_interface(&self, interface: &str) -> Option<&InterfaceDeclarationNode> {
        self.interface_declarations
            .get(&mesh_core_service::canonical_interface_name(interface))
    }

    pub fn interface_guidance(&self) -> &[InterfaceGuidanceRecord] {
        &self.interface_guidance
    }

    pub fn diagnostics(&self) -> &[ModuleGraphDiagnostic] {
        &self.diagnostics
    }

    /// Static authoring feedback that is intentionally excluded from normal
    /// graph construction. It parses module source, so callers should invoke
    /// it from explicit tooling such as `mesh-shell config doctor`.
    pub fn authoring_diagnostics(&self) -> Vec<ModuleGraphDiagnostic> {
        build_authoring_diagnostics(&self.modules)
    }

    pub fn health(&self) -> &[ModuleGraphHealthRecord] {
        &self.health
    }

    pub fn backend_providers_for_interface(&self, interface: &str) -> &[BackendProviderNode] {
        let interface = mesh_core_service::canonical_interface_name(interface);
        self.backend_providers
            .get(&interface)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn active_provider(&self, interface: &str) -> Option<&BackendProviderNode> {
        let interface = mesh_core_service::canonical_interface_name(interface);
        let module_id = self.active_providers.get(&interface)?;
        self.backend_providers_for_interface(&interface)
            .iter()
            .find(|provider| &provider.module_id == module_id)
    }

    pub fn fallback_provider(&self, interface: &str) -> Option<&BackendProviderNode> {
        self.backend_providers_for_interface(interface).first()
    }

    pub fn unresolved_backend_requirements(&self) -> Vec<UnresolvedModuleRequirement> {
        let mut unresolved = Vec::new();
        for requirements in self.frontend_requirements.values() {
            for interface in requirements.backend.keys() {
                if self.backend_providers_for_interface(interface).is_empty() {
                    unresolved.push(UnresolvedModuleRequirement {
                        module_id: requirements.module_id.clone(),
                        requirement: interface.clone(),
                    });
                }
            }
        }
        unresolved.sort_by(|a, b| {
            a.module_id
                .cmp(&b.module_id)
                .then_with(|| a.requirement.cmp(&b.requirement))
        });
        unresolved
    }

    pub fn layout_entrypoint(&self) -> Option<&ResolvedLayoutEntrypoint> {
        self.layout_entrypoint.as_ref()
    }

    pub fn contributed_themes(&self) -> &[ContributedTheme] {
        &self.contributions.themes
    }

    pub fn contributed_icons(&self) -> &[ContributedPathResource] {
        &self.contributions.icons
    }

    pub fn contributed_fonts(&self) -> &[ContributedPathResource] {
        &self.contributions.fonts
    }

    pub fn contributed_i18n(&self) -> &[ContributedI18n] {
        &self.contributions.i18n
    }

    pub fn contributed_libraries(&self) -> &[ContributedLibrary] {
        &self.contributions.libraries
    }

    pub fn settings_schemas(&self) -> &[ContributedSettingsSchema] {
        &self.contributions.settings
    }

    pub fn frontend_entrypoints(&self) -> &[ContributedFrontendEntrypoint] {
        &self.contributions.frontend_entrypoints
    }

    pub fn frontend_surfaces(&self) -> &[ContributedFrontendSurface] {
        &self.contributions.frontend_surfaces
    }

    pub fn contributed_layouts(&self) -> &[ContributedLayout] {
        &self.contributions.layout
    }

    pub fn keybind_actions(&self) -> &[ContributedKeybindAction] {
        &self.contributions.keybinds
    }

    pub fn icon_requirements(&self) -> &[ContributedIconRequirement] {
        &self.contributions.icon_requirements
    }

    pub fn icon_pack_contributions(&self) -> &[ContributedIconPack] {
        &self.contributions.icon_packs
    }

    /// Version resolution over the enabled closure: shared versions, conflicts,
    /// and required modules that are not installed.
    pub fn resolution(&self) -> &ResolutionOutcome {
        &self.resolution
    }

    pub fn declared_extension_points(&self) -> &[DeclaredExtensionPoint] {
        &self.contributions.extension_points
    }

    pub fn extension_point_hosts(&self) -> &[ExtensionPointHost] {
        &self.contributions.extension_point_hosts
    }

    /// Contributions this host should render at `point`, in render order.
    pub fn extension_point_contributions(
        &self,
        host_module_id: &str,
        point: &str,
    ) -> &[ResolvedExtensionPointContribution] {
        self.extension_points
            .get(&(host_module_id.to_string(), point.to_string()))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The entry `module_id` actually renders at `point`, after composition
    /// overrides. `None` means it contributes nothing there — either it never
    /// did, or a composition suppressed it.
    pub fn resolved_contribution_entry(&self, module_id: &str, point: &str) -> Option<&str> {
        self.extension_points
            .iter()
            .filter(|((_, resolved_point), _)| resolved_point == point)
            .flat_map(|(_, contributions)| contributions.iter())
            .find(|contribution| contribution.source_module_id == module_id)
            .map(|contribution| contribution.entry.as_str())
    }

    /// Every resolved contribution, for catalog compilation and change diffing.
    pub fn all_extension_point_contributions(
        &self,
    ) -> &HashMap<(String, String), Vec<ResolvedExtensionPointContribution>> {
        &self.extension_points
    }

    pub fn node_slot_overrides(
        &self,
    ) -> &std::collections::BTreeMap<String, std::collections::BTreeMap<String, NodeSlotOverride>>
    {
        &self.node_slots
    }

    /// Typed contracts parsed from declared interface contract JSON, keyed by
    /// interface name.
    pub fn interface_contracts(&self) -> &HashMap<String, InterfaceContract> {
        &self.interface_contracts
    }

    pub fn interface_contract(&self, interface: &str) -> Option<&InterfaceContract> {
        self.interface_contracts
            .get(&mesh_core_service::canonical_interface_name(interface))
    }

    pub fn declared_interfaces(&self) -> Vec<&InterfaceDeclarationNode> {
        let mut interfaces = self.interface_declarations.values().collect::<Vec<_>>();
        interfaces.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.module_id.cmp(&b.module_id))
        });
        interfaces
    }

    pub fn backend_provider_contributions(&self) -> Vec<&BackendProviderNode> {
        let mut providers = self
            .backend_providers
            .values()
            .flat_map(|providers| providers.iter())
            .collect::<Vec<_>>();
        providers.sort_by(|a, b| {
            a.interface
                .cmp(&b.interface)
                .then_with(|| a.module_id.cmp(&b.module_id))
        });
        providers
    }
}

#[derive(Debug, Clone)]
pub struct InstalledModuleNode {
    pub id: String,
    pub kind: ModuleKind,
    pub path: String,
    pub enabled: bool,
    pub trust: TrustTier,
    pub manifest_path: PathBuf,
    pub manifest_source: ModuleManifestSource,
    pub manifest: ModuleManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributionSource {
    pub module_id: String,
    pub module_kind: ModuleKind,
    pub module_path: String,
    pub manifest_path: PathBuf,
    pub manifest_source: ModuleManifestSource,
    pub local_id: String,
    pub scoped_id: String,
}

impl ContributionSource {
    pub(in crate::package) fn new(module: &InstalledModuleNode, local_id: &str) -> Self {
        Self {
            module_id: module.id.clone(),
            module_kind: module.kind,
            module_path: module.path.clone(),
            manifest_path: module.manifest_path.clone(),
            manifest_source: module.manifest_source,
            local_id: local_id.into(),
            scoped_id: format!("{}:{local_id}", module.id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendProviderNode {
    pub source: ContributionSource,
    pub module_id: String,
    pub interface: String,
    pub version: Option<String>,
    pub base_module: Option<String>,
    pub provider: Option<String>,
    pub label: Option<manifest::LocalizedText>,
    pub priority: u32,
    pub required_capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
}

impl BackendProviderNode {
    pub fn label_text(&self) -> Option<&str> {
        self.label
            .as_ref()
            .map(manifest::LocalizedText::fallback_text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDeclarationNode {
    pub source: ContributionSource,
    pub module_id: String,
    pub name: String,
    pub version: Option<String>,
    /// Contract JSON declared in the module manifest. Parsed into a typed
    /// [`InterfaceContract`] during graph construction.
    pub contract: Option<serde_json::Value>,
    pub domain: Option<String>,
    pub extends: Option<String>,
    pub relationship: InterfaceRelationship,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceGuidanceRecord {
    pub module_id: String,
    pub interface: String,
    pub domain: String,
    pub recommended_base: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGraphDiagnostic {
    pub module_id: String,
    pub contribution_id: Option<String>,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGraphHealthRecord {
    pub module_id: String,
    pub interface: Option<String>,
    pub provider_id: Option<String>,
    pub status: String,
    pub message: String,
}

fn build_interface_guidance(
    declarations: &HashMap<String, InterfaceDeclarationNode>,
) -> Vec<InterfaceGuidanceRecord> {
    let mut base_by_domain: HashMap<String, String> = HashMap::new();
    for declaration in declarations.values() {
        if declaration.relationship != InterfaceRelationship::Base {
            continue;
        }
        let Some(domain) = &declaration.domain else {
            continue;
        };
        let replace = base_by_domain.get(domain).map_or(true, |current| {
            !current.starts_with("mesh.") && declaration.name.starts_with("mesh.")
        });
        if replace {
            base_by_domain.insert(domain.clone(), declaration.name.clone());
        }
    }

    let mut guidance = Vec::new();
    for declaration in declarations.values() {
        if declaration.relationship != InterfaceRelationship::Independent
            || declaration.extends.is_some()
        {
            continue;
        }
        let Some(domain) = &declaration.domain else {
            continue;
        };
        let Some(base) = base_by_domain.get(domain) else {
            continue;
        };
        if base == &declaration.name {
            continue;
        }
        guidance.push(InterfaceGuidanceRecord {
            module_id: declaration.module_id.clone(),
            interface: declaration.name.clone(),
            domain: domain.clone(),
            recommended_base: base.clone(),
            status: "consider_extending_base_interface".into(),
            message: format!(
                "interface {} is an independent {domain} interface; prefer extending {base} when it can share normal {domain} state or commands",
                declaration.name
            ),
        });
    }
    guidance.sort_by(|a, b| {
        a.domain
            .cmp(&b.domain)
            .then_with(|| a.interface.cmp(&b.interface))
            .then_with(|| a.module_id.cmp(&b.module_id))
    });
    guidance
}
