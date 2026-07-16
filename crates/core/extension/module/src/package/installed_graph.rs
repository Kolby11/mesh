use super::{
    InstalledModuleEntry, InterfaceRelationship, ModuleKind, ModuleManifest,
    ModuleManifestDiagnostic, ModuleManifestError, PathContribution, RootModuleGraphManifest,
    dependency_spec_to_string, parse_module_entrypoint, validate_relative_path,
};
use crate::manifest;
use mesh_core_component::{AttributeValue, SourceTag, TemplateNode, parse_component};
use mesh_core_service::{ContractCapabilities, InterfaceContract, parse_interface_contract};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LoadedModuleManifest {
    pub manifest: ModuleManifest,
    pub path: PathBuf,
    pub source: ModuleManifestSource,
    pub diagnostics: Vec<ModuleManifestDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleManifestSource {
    CanonicalModuleJson,
}

#[derive(Debug, Clone)]
pub struct InstalledModuleGraph {
    modules: HashMap<String, InstalledModuleNode>,
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
    layout_entrypoint: Option<ResolvedLayoutEntrypoint>,
}

impl InstalledModuleGraph {
    pub fn from_parts(
        mut root: RootModuleGraphManifest,
        modules: Vec<LoadedModuleManifest>,
    ) -> Result<Self, ModuleManifestError> {
        root.validate()?;
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

            let node = InstalledModuleNode {
                id: module_id.clone(),
                kind: entry.kind,
                path: entry.path.clone(),
                enabled: entry.enabled,
                manifest_path: loaded.path.clone(),
                manifest_source: loaded.source,
                manifest: loaded.manifest.clone(),
            };

            if entry.enabled {
                if entry.kind == ModuleKind::Frontend {
                    frontend_requirements.insert(
                        module_id.clone(),
                        FrontendRequirementSet::from_manifest(module_id, &node.manifest),
                    );
                }

                if entry.kind == ModuleKind::Interface
                    && let Some(interface) = &node.manifest.mesh.interface
                {
                    let declaration = InterfaceDeclarationNode {
                        source: ContributionSource::new(&node, &interface.name),
                        module_id: module_id.clone(),
                        name: interface.name.clone(),
                        version: interface.version.clone(),
                        contract: interface.contract.clone(),
                        domain: interface.domain.clone(),
                        extends: interface.extends.clone(),
                        relationship: interface.effective_relationship(),
                        reason: interface.reason.clone(),
                    };
                    interface_declarations.insert(declaration.name.clone(), declaration);
                }

                if entry.kind == ModuleKind::Backend {
                    for provided in node.manifest.mesh.implementations() {
                        let provider = BackendProviderNode {
                            source: ContributionSource::new(
                                &node,
                                provided.provider.as_deref().unwrap_or(&provided.interface),
                            ),
                            module_id: module_id.clone(),
                            interface: provided.interface.clone(),
                            version: provided.version.clone(),
                            base_module: provided.base_module.clone(),
                            provider: provided.provider.clone(),
                            label: provided.label.clone(),
                            priority: provided.priority,
                            required_capabilities: node.manifest.mesh.capabilities.required.clone(),
                            optional_capabilities: node.manifest.mesh.capabilities.optional.clone(),
                        };
                        backend_providers
                            .entry(provided.interface.clone())
                            .or_default()
                            .push(provider);
                    }
                }

                contributions.index_module(&node)?;
            }

            graph_modules.insert(module_id.clone(), node);
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
        let mut manual_diagnostics: Vec<ModuleGraphDiagnostic> = Vec::new();
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
                let candidate = InterfaceDeclarationNode {
                    source: ContributionSource::new(node, &interface.name),
                    module_id: module_id.clone(),
                    name: interface.name.clone(),
                    version: interface.version.clone(),
                    contract: interface.contract.clone(),
                    domain: interface.domain.clone(),
                    extends: interface.extends.clone(),
                    relationship: interface.effective_relationship(),
                    reason: interface.reason.clone(),
                };
                match interface_declarations.get(&candidate.name) {
                    None => {
                        interface_declarations.insert(candidate.name.clone(), candidate);
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

        // Auto-select a provider when exactly one enabled backend implements an
        // interface and the root graph names none. This removes the need to
        // hand-write a `providers` entry for the common single-provider case.
        // `backend_providers` only holds enabled providers, so a length of one
        // means a sole implementer. Explicit root selections always win, and
        // interfaces with multiple providers still require an explicit choice.
        for (interface, providers) in &backend_providers {
            if root.providers.contains_key(interface) {
                continue;
            }
            if let [sole] = providers.as_slice() {
                root.providers
                    .insert(interface.clone(), sole.module_id.clone());
            }
        }

        for (interface, module_id) in &root.providers {
            let Some(node) = graph_modules.get(module_id) else {
                return Err(ModuleManifestError::Validation(format!(
                    "active provider {module_id} for {interface} is not installed"
                )));
            };
            if !node.enabled {
                return Err(ModuleManifestError::Validation(format!(
                    "active provider {module_id} for {interface} is disabled"
                )));
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
                if !node.enabled || node.kind != ModuleKind::Frontend {
                    return Err(ModuleManifestError::Validation(format!(
                        "layout entrypoint module {module_id} must be an enabled frontend module"
                    )));
                }
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
            None => None,
        };
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
                    interface_contracts.insert(declaration.name.clone(), contract);
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

        let interface_guidance = build_interface_guidance(&interface_declarations);
        let diagnostics = build_graph_diagnostics(
            &graph_modules,
            &frontend_requirements,
            &backend_providers,
            &contributions,
            &interface_contracts,
            manual_diagnostics,
        );
        let health = build_graph_health(
            &backend_providers,
            &root.providers,
            &frontend_requirements,
            &diagnostics,
        );

        Ok(Self {
            modules: graph_modules,
            backend_providers,
            active_providers: root.providers,
            frontend_requirements,
            interface_declarations,
            interface_contracts,
            interface_guidance,
            diagnostics,
            health,
            contributions,
            layout_entrypoint,
        })
    }

    pub fn module(&self, id: &str) -> Option<&InstalledModuleNode> {
        self.modules.get(id)
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
        self.interface_declarations.get(interface)
    }

    pub fn interface_guidance(&self) -> &[InterfaceGuidanceRecord] {
        &self.interface_guidance
    }

    pub fn diagnostics(&self) -> &[ModuleGraphDiagnostic] {
        &self.diagnostics
    }

    pub fn health(&self) -> &[ModuleGraphHealthRecord] {
        &self.health
    }

    pub fn backend_providers_for_interface(&self, interface: &str) -> &[BackendProviderNode] {
        self.backend_providers
            .get(interface)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn active_provider(&self, interface: &str) -> Option<&BackendProviderNode> {
        let module_id = self.active_providers.get(interface)?;
        self.backend_providers_for_interface(interface)
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

    /// Typed contracts parsed from declared interface contract JSON, keyed by
    /// interface name.
    pub fn interface_contracts(&self) -> &HashMap<String, InterfaceContract> {
        &self.interface_contracts
    }

    pub fn interface_contract(&self, interface: &str) -> Option<&InterfaceContract> {
        self.interface_contracts.get(interface)
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
    fn new(module: &InstalledModuleNode, local_id: &str) -> Self {
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

fn build_graph_health(
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

fn binary_package_hint(binary: &manifest::BinaryDependency) -> String {
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

pub(crate) fn extract_icon_names_from_mesh_source(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(component) = parse_component(content) else {
        return names;
    };
    let Some(template) = component.template else {
        return names;
    };

    for node in &template.root {
        collect_icon_names_from_template_node(node, &mut names);
    }
    names.sort();
    names.dedup();
    names
}

fn collect_icon_names_from_template_node(node: &TemplateNode, names: &mut Vec<String>) {
    match node {
        TemplateNode::Element(element) => {
            if element.tag_kind == SourceTag::Icon {
                for attribute in &element.attributes {
                    if attribute.name == "name"
                        && let AttributeValue::Static(name) = &attribute.value
                        && !name.is_empty()
                    {
                        names.push(name.clone());
                    }
                }
            }
            for child in &element.children {
                collect_icon_names_from_template_node(child, names);
            }
        }
        TemplateNode::If(node) => {
            for child in &node.then_children {
                collect_icon_names_from_template_node(child, names);
            }
            for child in &node.else_children {
                collect_icon_names_from_template_node(child, names);
            }
        }
        TemplateNode::For(node) => {
            for child in &node.children {
                collect_icon_names_from_template_node(child, names);
            }
        }
        TemplateNode::Component(component) => {
            for child in &component.children {
                collect_icon_names_from_template_node(child, names);
            }
        }
        TemplateNode::Text(_) | TemplateNode::Expr(_) | TemplateNode::Slot(_) => {}
    }
}

pub(crate) fn extract_t_keys_from_mesh_source(content: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut remaining = content;
    while let Some(start) = remaining.find("t(") {
        remaining = &remaining[start + 2..];
        let quote = if remaining.starts_with('\'') {
            '\''
        } else if remaining.starts_with('"') {
            '"'
        } else {
            // dynamic expression — skip
            continue;
        };
        let inner = &remaining[1..];
        if let Some(end) = inner.find(quote) {
            let key = &inner[..end];
            if !key.is_empty() && !key.contains('{') && !key.contains(' ') {
                keys.push(key.to_string());
            }
            remaining = &inner[end + 1..];
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

pub(crate) fn extract_mesh_event_publish_channels(content: &str) -> Vec<String> {
    let mut channels = Vec::new();
    let mut remaining = content;
    while let Some(start) = remaining.find("mesh.events.publish(") {
        remaining = &remaining[start + "mesh.events.publish(".len()..];
        let quote = if remaining.starts_with('\'') {
            '\''
        } else if remaining.starts_with('"') {
            '"'
        } else {
            // Dynamic channel expression — skip.
            continue;
        };
        let inner = &remaining[1..];
        if let Some(end) = inner.find(quote) {
            let channel = &inner[..end];
            if !channel.is_empty() && !channel.contains('{') && !channel.contains(' ') {
                channels.push(channel.to_string());
            }
            remaining = &inner[end + 1..];
        }
    }
    channels.sort();
    channels.dedup();
    channels
}

pub(crate) fn extract_backend_emit_event_names(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut remaining = content;
    while let Some(start) = remaining.find("mesh.service.emit_event(") {
        remaining = &remaining[start + "mesh.service.emit_event(".len()..];
        let quote = if remaining.starts_with('\'') {
            '\''
        } else if remaining.starts_with('"') {
            '"'
        } else {
            // Dynamic event name — skip.
            continue;
        };
        let inner = &remaining[1..];
        if let Some(end) = inner.find(quote) {
            let name = &inner[..end];
            if !name.is_empty() && !name.contains('{') && !name.contains(' ') {
                names.push(name.to_string());
            }
            remaining = &inner[end + 1..];
        }
    }
    names.sort();
    names.dedup();
    names
}

pub(crate) fn extract_frontend_interface_event_subscriptions(
    content: &str,
) -> Vec<(String, String)> {
    let mut aliases = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(binding) = trimmed.strip_prefix("local ") else {
            continue;
        };
        let Some((alias, expression)) = binding.split_once('=') else {
            continue;
        };
        let alias = alias.trim();
        if alias.is_empty()
            || !alias
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            continue;
        }
        let expression = expression.trim();
        let Some(arguments) = expression.strip_prefix("require(") else {
            continue;
        };
        let quote = if arguments.starts_with('"') {
            '"'
        } else if arguments.starts_with('\'') {
            '\''
        } else {
            continue;
        };
        let quoted = &arguments[1..];
        let Some(end) = quoted.find(quote) else {
            continue;
        };
        let interface = &quoted[..end];
        if interface.starts_with("mesh.") {
            aliases.insert(alias.to_string(), interface.to_string());
        }
    }

    let mut subscriptions = Vec::new();
    for (alias, interface) in aliases {
        for prefix in [format!("{alias}."), format!("{alias}.events.")] {
            let mut remaining = content;
            while let Some(start) = remaining.find(&prefix) {
                remaining = &remaining[start + prefix.len()..];
                let event_len = remaining
                    .find(|character: char| {
                        !(character.is_ascii_alphanumeric() || character == '_')
                    })
                    .unwrap_or(remaining.len());
                let event = &remaining[..event_len];
                if event.is_empty()
                    || !event
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_uppercase())
                {
                    continue;
                }
                let suffix = &remaining[event_len..];
                let subscribes = if prefix.ends_with(".events.") {
                    suffix.starts_with(":subscribe(")
                } else {
                    suffix.starts_with(":on(")
                };
                if subscribes {
                    subscriptions.push((interface.clone(), event.to_string()));
                }
            }
        }
    }
    subscriptions.sort();
    subscriptions.dedup();
    subscriptions
}

pub(crate) fn extract_keybind_subscriptions_from_mesh_source(content: &str) -> Vec<(String, bool)> {
    let mut subscriptions = Vec::new();
    let mut remaining = content;
    while let Some(start) = remaining.find("keybind=") {
        if start > 0 {
            let previous = remaining[..start].chars().next_back().unwrap_or(' ');
            if previous.is_ascii_alphanumeric() || previous == '_' || previous == '-' {
                remaining = &remaining[start + "keybind=".len()..];
                continue;
            }
        }
        let tag_start = find_tag_start_before_attr(remaining, start).unwrap_or(0);
        let tag_end = find_tag_end_after_attr(remaining, start).unwrap_or(remaining.len());
        let tag = &remaining[tag_start..tag_end];
        let after = &remaining[start + "keybind=".len()..];
        let (value, advance) = if after.starts_with('"') || after.starts_with('\'') {
            let quote = after.chars().next().unwrap();
            let inner = &after[1..];
            match inner.find(quote) {
                Some(end) => (&inner[..end], end + 2),
                None => {
                    remaining = &after[1..];
                    continue;
                }
            }
        } else if let Some(inner) = after.strip_prefix('{') {
            match inner.find('}') {
                Some(end) => (&inner[..end], end + 2),
                None => {
                    remaining = inner;
                    continue;
                }
            }
        } else {
            remaining = after;
            continue;
        };

        let expression = value
            .strip_prefix('{')
            .and_then(|inner| inner.strip_suffix('}'));
        let is_expression = expression.is_some();
        let normalized = expression.unwrap_or(value).trim();
        let action_id = if let Some(start) = normalized.find("this.keybinds.") {
            let rest = &normalized[start + "this.keybinds.".len()..];
            rest.strip_suffix(".id").unwrap_or(rest)
        } else if !is_expression
            && !normalized.contains('{')
            && !normalized.contains('}')
            && !normalized.contains('.')
        {
            normalized
        } else {
            remaining = &after[advance.min(after.len())..];
            continue;
        };
        if !action_id.trim().is_empty() {
            subscriptions.push((action_id.trim().to_string(), tag.contains("onkeybind=")));
        }
        remaining = &after[advance.min(after.len())..];
    }
    subscriptions.sort();
    subscriptions.dedup();
    subscriptions
}

fn find_tag_start_before_attr(source: &str, attr_start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut in_string = false;
    let mut quote = b'"';
    let mut last_tag_start = None;
    let mut i = 0usize;

    while i < attr_start.min(bytes.len()) {
        let b = bytes[i];
        if in_string {
            if b == quote && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
        } else if b == b'<' {
            last_tag_start = Some(i);
        } else if b == b'>' {
            last_tag_start = None;
        }
        i += 1;
    }

    last_tag_start
}

fn find_tag_end_after_attr(source: &str, attr_start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut in_string = false;
    let mut quote = b'"';
    let mut i = attr_start.min(bytes.len());

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == quote && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
        } else if b == b'>' {
            return Some(i);
        }
        i += 1;
    }

    None
}

fn is_declared_shell_event_channel(channel: &str) -> bool {
    matches!(
        channel,
        "shell.show-surface"
            | "shell.hide-surface"
            | "shell.hide-popover"
            | "shell.toggle-surface"
            | "shell.position-surface"
            | "shell.activate-popover"
            | "shell.set-theme"
            | "shell.set-locale"
            | "shell.toggle-debug-overlay"
            | "shell.toggle-debug-layout-bounds"
            | "shell.toggle-debug-profiling"
            | "shell.run-debug-benchmark"
            | "shell.brightness-down"
            | "shell.brightness-up"
            | "shell.set-brightness"
            | "shell.toggle-calendar"
    )
}

fn scan_mesh_files_recursive(dir: &Path) -> Vec<(std::path::PathBuf, String)> {
    scan_files_recursive(dir, "mesh")
}

fn scan_files_recursive(dir: &Path, extension: &str) -> Vec<(std::path::PathBuf, String)> {
    let mut results = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return results;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            results.extend(scan_files_recursive(&path, extension));
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                results.push((path, content));
            }
        }
    }
    results
}

fn build_graph_diagnostics(
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
    diagnose_frontend_source_contracts(modules, &mut diagnostics);
    diagnose_missing_interface_contracts(modules, &mut diagnostics);
    diagnose_duplicate_keybind_triggers(contributions, &mut diagnostics);

    diagnostics.sort_by(|a, b| {
        a.status
            .cmp(&b.status)
            .then_with(|| a.module_id.cmp(&b.module_id))
            .then_with(|| a.contribution_id.cmp(&b.contribution_id))
    });
    diagnostics
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
                for required in &capabilities.required {
                    if !requirements.capabilities.iter().any(|cap| cap == required) {
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
                for event in extract_backend_emit_event_names(&content) {
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
                    "frontend module {} has a main entrypoint but does not declare mesh.surfaceLayout",
                    surface.module_id
                ),
            });
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
        let all_i18n: Vec<_> = module
            .manifest
            .mesh
            .contributes
            .i18n
            .iter()
            .chain(module.manifest.mesh.provides.i18n.iter())
            .collect();
        if !all_i18n.is_empty() {
            let contributed_locales = all_i18n
                .iter()
                .map(|catalog| catalog.locale.as_str())
                .collect::<std::collections::HashSet<_>>();
            for locale in &module.manifest.mesh.i18n.supported_locales {
                if !contributed_locales.contains(locale.as_str()) {
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
            // Warn when mesh.i18n.supportedLocales is redundant with provides.i18n.
            // Authors should declare catalogs once in provides.i18n and omit supportedLocales.
            if !module.manifest.mesh.i18n.supported_locales.is_empty() {
                let declared: std::collections::HashSet<&str> = module
                    .manifest
                    .mesh
                    .i18n
                    .supported_locales
                    .iter()
                    .map(String::as_str)
                    .collect();
                if declared == contributed_locales {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:i18n:supported_locales", module.id)),
                        status: "redundant_supported_locales".into(),
                        message: format!(
                            "module {} mesh.i18n.supportedLocales lists the same locales as provides.i18n; remove supportedLocales and declare catalogs once in provides.i18n",
                            module.id
                        ),
                    });
                }
            }
            // Warn when defaultLocale is declared but has no contributed catalog.
            if let Some(default) = module.manifest.mesh.i18n.default_locale.as_deref() {
                if !contributed_locales.contains(default) {
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
            .find(|c| c.locale == default_locale)
            .and_then(|c| {
                let catalog_path = module_dir.join(&c.path);
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

        for (path, content) in scan_mesh_files_recursive(scan_root) {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            for icon_name in extract_icon_names_from_mesh_source(&content) {
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
                for key in extract_t_keys_from_mesh_source(&content) {
                    if !catalog.contains(&key) {
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
            for channel in extract_mesh_event_publish_channels(&content) {
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
            for (action_id, has_handler) in extract_keybind_subscriptions_from_mesh_source(&content)
            {
                if !declared_keybinds.contains(action_id.as_str()) {
                    diagnostics.push(ModuleGraphDiagnostic {
                        module_id: module.id.clone(),
                        contribution_id: Some(format!("{}:keybind:{}", module.id, file_name)),
                        status: "undeclared_keybind_subscription".into(),
                        message: format!(
                            "module {} subscribes to keybind action '{}' in {}, but mesh.keybinds does not declare it",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendRequirementSet {
    pub module_id: String,
    pub modules: HashMap<String, String>,
    pub backend: HashMap<String, String>,
    pub optional_backend: HashMap<String, String>,
    pub icons: HashMap<String, String>,
    pub fonts: HashMap<String, String>,
    pub i18n: HashMap<String, String>,
    pub themes: HashMap<String, String>,
    pub capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
}

impl FrontendRequirementSet {
    fn from_manifest(module_id: &str, manifest: &ModuleManifest) -> Self {
        let dependencies = &manifest.mesh.dependencies;
        let modules = dependencies
            .modules
            .iter()
            .map(|(id, spec)| (id.clone(), dependency_spec_to_string(spec)))
            .collect();
        Self {
            module_id: module_id.into(),
            modules,
            backend: dependencies.backend.clone(),
            optional_backend: dependencies.optional_backend.clone(),
            icons: dependencies.icons.clone(),
            fonts: dependencies.fonts.clone(),
            i18n: dependencies.i18n.clone(),
            themes: dependencies.themes.clone(),
            capabilities: manifest.mesh.capabilities.required.clone(),
            optional_capabilities: manifest.mesh.capabilities.optional.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModuleContributionIndex {
    frontend_entrypoints: Vec<ContributedFrontendEntrypoint>,
    frontend_surfaces: Vec<ContributedFrontendSurface>,
    layout: Vec<ContributedLayout>,
    themes: Vec<ContributedTheme>,
    icons: Vec<ContributedPathResource>,
    fonts: Vec<ContributedPathResource>,
    i18n: Vec<ContributedI18n>,
    libraries: Vec<ContributedLibrary>,
    settings: Vec<ContributedSettingsSchema>,
    keybinds: Vec<ContributedKeybindAction>,
    icon_requirements: Vec<ContributedIconRequirement>,
    icon_packs: Vec<ContributedIconPack>,
}

impl ModuleContributionIndex {
    fn index_module(&mut self, module: &InstalledModuleNode) -> Result<(), ModuleManifestError> {
        let module_id = module.id.as_str();
        let manifest = &module.manifest;
        if module.kind == ModuleKind::Frontend {
            if let Some(path) = &manifest.mesh.entrypoints.main {
                validate_relative_path("frontend main entrypoint", path)?;
                let settings_namespace = manifest
                    .mesh
                    .contributes
                    .settings
                    .as_ref()
                    .map(|settings| settings.namespace.clone());
                self.frontend_entrypoints
                    .push(ContributedFrontendEntrypoint {
                        source: ContributionSource::new(module, "main"),
                        module_id: module_id.into(),
                        kind: FrontendEntrypointKind::Main,
                        path: path.clone(),
                    });
                self.frontend_surfaces.push(ContributedFrontendSurface {
                    source: ContributionSource::new(module, "surface"),
                    module_id: module_id.into(),
                    path: path.clone(),
                    settings_namespace,
                    accessibility: manifest.mesh.accessibility.clone(),
                    surface_layout: manifest.mesh.surface_layout.clone(),
                });
            }
            if let Some(path) = &manifest.mesh.entrypoints.settings_ui {
                validate_relative_path("frontend settings entrypoint", path)?;
                self.frontend_entrypoints
                    .push(ContributedFrontendEntrypoint {
                        source: ContributionSource::new(module, "settings-ui"),
                        module_id: module_id.into(),
                        kind: FrontendEntrypointKind::SettingsUi,
                        path: path.clone(),
                    });
            }
        }
        for contribution in &manifest.mesh.contributes.layout {
            validate_relative_path("layout entrypoint", &contribution.entrypoint)?;
            self.layout.push(ContributedLayout {
                source: ContributionSource::new(module, &contribution.id),
                module_id: module_id.into(),
                id: contribution.id.clone(),
                path: contribution.entrypoint.clone(),
                label: contribution.label.clone(),
            });
        }
        for contribution in &manifest.mesh.contributes.themes {
            for path in contribution.modes.values() {
                validate_relative_path("theme mode", path)?;
            }
            self.themes.push(ContributedTheme {
                source: ContributionSource::new(module, &contribution.id),
                module_id: module_id.into(),
                id: contribution.id.clone(),
                label: contribution.label.clone(),
                modes: contribution.modes.clone(),
                default_mode: contribution.default_mode.clone(),
            });
        }
        for contribution in &manifest.mesh.contributes.icons {
            self.icons.push(ContributedPathResource::from_contribution(
                module,
                contribution,
            )?);
        }
        for contribution in &manifest.mesh.contributes.fonts {
            self.fonts.push(ContributedPathResource::from_contribution(
                module,
                contribution,
            )?);
        }
        for contribution in &manifest.mesh.contributes.i18n {
            validate_relative_path("i18n contribution", &contribution.path)?;
            self.i18n.push(ContributedI18n {
                source: ContributionSource::new(module, &contribution.id),
                module_id: module_id.into(),
                id: contribution.id.clone(),
                locale: contribution.locale.clone(),
                path: contribution.path.clone(),
            });
        }
        for contribution in &manifest.mesh.contributes.libraries {
            contribution.validate()?;
            self.libraries.push(ContributedLibrary {
                source: ContributionSource::new(module, &contribution.namespace),
                module_id: module_id.into(),
                namespace: contribution.namespace.clone(),
                path: contribution.path.clone(),
            });
        }
        if let Some(settings) = &manifest.mesh.contributes.settings {
            self.settings.push(ContributedSettingsSchema {
                source: ContributionSource::new(module, &settings.namespace),
                module_id: module_id.into(),
                namespace: settings.namespace.clone(),
                schema: settings.schema.clone(),
            });
        }
        for (action_id, action) in &manifest.mesh.keybinds.actions {
            self.keybinds.push(ContributedKeybindAction {
                source: ContributionSource::new(module, action_id),
                module_id: module_id.into(),
                action_id: action_id.clone(),
                scope: action.scope,
                label: action.label.clone(),
                description: action.description.clone(),
                category: action.category.clone(),
                trigger: action.trigger.clone(),
                localized_triggers: action.localized_triggers.clone(),
            });
        }
        for icon in &manifest.mesh.icon_requirements.required {
            self.icon_requirements.push(ContributedIconRequirement {
                source: ContributionSource::new(module, &format!("required:{icon}")),
                module_id: module_id.into(),
                name: icon.clone(),
                required: true,
            });
        }
        for icon in &manifest.mesh.icon_requirements.optional {
            self.icon_requirements.push(ContributedIconRequirement {
                source: ContributionSource::new(module, &format!("optional:{icon}")),
                module_id: module_id.into(),
                name: icon.clone(),
                required: false,
            });
        }
        if let Some(icon_pack) = &manifest.mesh.icon_pack {
            self.icon_packs.push(ContributedIconPack {
                source: ContributionSource::new(module, &icon_pack.id),
                module_id: module_id.into(),
                id: icon_pack.id.clone(),
                mappings: icon_pack.mappings.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendEntrypointKind {
    Main,
    SettingsUi,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedFrontendEntrypoint {
    pub source: ContributionSource,
    pub module_id: String,
    pub kind: FrontendEntrypointKind,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ContributedFrontendSurface {
    pub source: ContributionSource,
    pub module_id: String,
    pub path: String,
    pub settings_namespace: Option<String>,
    pub accessibility: Option<manifest::AccessibilitySection>,
    pub surface_layout: Option<manifest::SurfaceLayoutSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedModuleRequirement {
    pub module_id: String,
    pub requirement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLayoutEntrypoint {
    pub module_id: String,
    pub entrypoint_id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedLayout {
    pub source: ContributionSource,
    pub module_id: String,
    pub id: String,
    pub path: String,
    pub label: Option<manifest::LocalizedText>,
}

impl ContributedLayout {
    pub fn label_text(&self) -> Option<&str> {
        self.label
            .as_ref()
            .map(manifest::LocalizedText::fallback_text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedTheme {
    pub source: ContributionSource,
    pub module_id: String,
    pub id: String,
    pub label: Option<manifest::LocalizedText>,
    pub modes: HashMap<String, String>,
    pub default_mode: Option<String>,
}

impl ContributedTheme {
    pub fn label_text(&self) -> Option<&str> {
        self.label
            .as_ref()
            .map(manifest::LocalizedText::fallback_text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedPathResource {
    pub source: ContributionSource,
    pub module_id: String,
    pub id: String,
    pub path: String,
    pub label: Option<manifest::LocalizedText>,
}

impl ContributedPathResource {
    fn from_contribution(
        module: &InstalledModuleNode,
        contribution: &PathContribution,
    ) -> Result<Self, ModuleManifestError> {
        validate_relative_path("path contribution", &contribution.path)?;
        Ok(Self {
            source: ContributionSource::new(module, &contribution.id),
            module_id: module.id.clone(),
            id: contribution.id.clone(),
            path: contribution.path.clone(),
            label: contribution.label.clone(),
        })
    }

    pub fn label_text(&self) -> Option<&str> {
        self.label
            .as_ref()
            .map(manifest::LocalizedText::fallback_text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedI18n {
    pub source: ContributionSource,
    pub module_id: String,
    pub id: String,
    pub locale: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedLibrary {
    pub source: ContributionSource,
    pub module_id: String,
    pub namespace: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContributedSettingsSchema {
    pub source: ContributionSource,
    pub module_id: String,
    pub namespace: String,
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedKeybindAction {
    pub source: ContributionSource,
    pub module_id: String,
    pub action_id: String,
    pub scope: manifest::KeybindScope,
    pub label: Option<manifest::LocalizedText>,
    pub description: Option<manifest::LocalizedText>,
    pub category: Option<manifest::LocalizedText>,
    pub trigger: manifest::KeybindTrigger,
    pub localized_triggers: HashMap<String, manifest::KeybindTrigger>,
}

impl ContributedKeybindAction {
    pub fn label_text(&self) -> Option<&str> {
        self.label
            .as_ref()
            .map(manifest::LocalizedText::fallback_text)
    }

    pub fn description_text(&self) -> Option<&str> {
        self.description
            .as_ref()
            .map(manifest::LocalizedText::fallback_text)
    }

    pub fn category_text(&self) -> Option<&str> {
        self.category
            .as_ref()
            .map(manifest::LocalizedText::fallback_text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedIconRequirement {
    pub source: ContributionSource,
    pub module_id: String,
    pub name: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedIconPack {
    pub source: ContributionSource,
    pub module_id: String,
    pub id: String,
    pub mappings: HashMap<String, String>,
}

pub fn load_installed_module_graph(
    root_module_graph_path: &Path,
) -> Result<InstalledModuleGraph, ModuleManifestError> {
    load_installed_module_graph_with(root_module_graph_path, load_module_manifests)
}

fn load_installed_module_graph_with(
    root_module_graph_path: &Path,
    load_manifests: impl Fn(&[PathBuf]) -> Result<Vec<LoadedModuleManifest>, ModuleManifestError>,
) -> Result<InstalledModuleGraph, ModuleManifestError> {
    let mut root = RootModuleGraphManifest::from_path(root_module_graph_path)?;
    let root_dir = root_module_graph_path.parent().ok_or_else(|| {
        ModuleManifestError::Validation(format!(
            "root module graph path must have a parent directory: {}",
            root_module_graph_path.display()
        ))
    })?;
    let modules_dir = root_dir.join(&root.modules_dir);
    let mut modules = Vec::new();

    if root.modules.is_empty() {
        // Auto-discovery: the root graph lists no modules, so scan `modulesDir`
        // for `module.json` files and build the installed set from the modules'
        // own manifests (each declares its `name` and `kind`). The root file
        // then only holds decisions — `disabled`, `providers`, `layout`,
        // `theme`. A discovered module is enabled unless named in `disabled`.
        let module_dirs = discover_module_dirs(&modules_dir);
        let loaded_manifests = load_manifests(&module_dirs)?;
        for (module_dir, loaded) in module_dirs.iter().cloned().zip(loaded_manifests) {
            let name = loaded.manifest.name.clone();
            let kind = loaded.manifest.mesh.kind;
            let relative = module_dir
                .strip_prefix(&modules_dir)
                .unwrap_or(&module_dir)
                .to_string_lossy()
                .replace('\\', "/");
            let enabled = !root.disabled.iter().any(|disabled| disabled == &name);
            root.modules.insert(
                name,
                InstalledModuleEntry {
                    kind,
                    path: relative,
                    enabled,
                },
            );
            modules.push(loaded);
        }
    } else {
        let module_dirs = root
            .modules
            .values()
            .map(|entry| modules_dir.join(&entry.path))
            .collect::<Vec<_>>();
        modules = load_manifests(&module_dirs)?;
    }

    InstalledModuleGraph::from_parts(root, modules)
}

#[cfg(test)]
pub(super) fn load_installed_module_graph_serial(
    root_module_graph_path: &Path,
) -> Result<InstalledModuleGraph, ModuleManifestError> {
    load_installed_module_graph_with(root_module_graph_path, load_module_manifests_serial)
}

#[cfg(test)]
pub(super) fn load_discovered_module_manifests(
    module_dirs: &[PathBuf],
) -> Result<Vec<(PathBuf, LoadedModuleManifest)>, ModuleManifestError> {
    let manifests = load_module_manifests(module_dirs)?;
    Ok(module_dirs.iter().cloned().zip(manifests).collect())
}

/// Load an already ordered set of module directories without serializing file
/// IO and JSON parsing on the caller. Indexed parallel iteration preserves the
/// input order, so callers retain their existing deterministic assembly order.
pub(super) fn load_module_manifests(
    module_dirs: &[PathBuf],
) -> Result<Vec<LoadedModuleManifest>, ModuleManifestError> {
    let loaded = module_dirs
        .par_iter()
        .map(|module_dir| load_module_manifest(module_dir))
        .collect::<Vec<_>>();
    loaded.into_iter().collect()
}

#[cfg(test)]
pub(super) fn load_discovered_module_manifests_serial(
    module_dirs: &[PathBuf],
) -> Result<Vec<(PathBuf, LoadedModuleManifest)>, ModuleManifestError> {
    let manifests = load_module_manifests_serial(module_dirs)?;
    Ok(module_dirs.iter().cloned().zip(manifests).collect())
}

#[cfg(test)]
pub(super) fn load_module_manifests_serial(
    module_dirs: &[PathBuf],
) -> Result<Vec<LoadedModuleManifest>, ModuleManifestError> {
    module_dirs
        .iter()
        .map(|module_dir| load_module_manifest(module_dir))
        .collect()
}

/// Recursively find directories under `modules_dir` that contain a
/// `module.json`. Descent stops once a `module.json` is found, so nested
/// resources inside a module are never treated as separate modules. Results are
/// sorted for deterministic ordering.
pub(super) fn discover_module_dirs(modules_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    discover_module_dirs_into(modules_dir, &mut found);
    found.sort();
    found
}

fn discover_module_dirs_into(dir: &Path, found: &mut Vec<PathBuf>) {
    if dir.join("module.json").is_file() {
        found.push(dir.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            discover_module_dirs_into(&path, found);
        }
    }
}

pub fn load_module_manifest(
    module_dir: &Path,
) -> Result<LoadedModuleManifest, ModuleManifestError> {
    let plugin_json = module_dir.join("plugin.json");
    if plugin_json.exists() {
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                plugin_json,
                None,
                None,
                "plugin.json is not a supported MESH module manifest",
                "remove plugin.json or replace it with module.json",
            ),
        });
    }

    let module_json = module_dir.join("module.json");
    let package_json = module_dir.join("package.json");
    let mesh_toml = module_dir.join("mesh.toml");
    let existing = [&module_json, &package_json, &mesh_toml]
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();

    if existing.len() > 1 {
        let manifest_names = existing
            .iter()
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                module_dir,
                None,
                None,
                format!("ambiguous module manifest files found: {manifest_names}"),
                "keep canonical module.json and remove the old manifest file",
            ),
        });
    }

    if module_json.exists() {
        let content =
            std::fs::read_to_string(&module_json).map_err(|source| ModuleManifestError::Io {
                path: module_json.clone(),
                source,
            })?;
        if crate::manifest::is_canonical_module_json(&content).map_err(|source| {
            ModuleManifestError::Json {
                path: module_json.clone(),
                source,
            }
        })? {
            let manifest = ModuleManifest::from_path(&module_json)?;
            let diagnostics = manifest.localized_text_diagnostics(&module_json);
            return Ok(LoadedModuleManifest {
                manifest,
                path: module_json,
                source: ModuleManifestSource::CanonicalModuleJson,
                diagnostics,
            });
        }

        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                &module_json,
                None,
                Some("$".into()),
                "legacy module.json shape uses id/type/api_version fields",
                "replace legacy module.json fields with canonical name/version/mesh",
            ),
        });
    }

    if package_json.exists() {
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                package_json,
                None,
                None,
                "package.json is a legacy MESH module manifest path",
                "rename package.json to module.json",
            ),
        });
    }

    if mesh_toml.exists() {
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                mesh_toml,
                None,
                None,
                "mesh.toml is a legacy MESH module manifest path",
                "replace mesh.toml with canonical module.json",
            ),
        });
    }

    Err(ModuleManifestError::Validation(format!(
        "no module.json found in {}",
        module_dir.display()
    )))
}
