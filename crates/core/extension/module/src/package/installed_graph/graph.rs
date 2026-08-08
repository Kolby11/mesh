use super::super::{
    InterfaceRelationship, ModuleKind, ModuleManifest, ModuleManifestDiagnostic,
    ModuleManifestError, NodeSlotOverride, ResolutionOutcome, RootModuleGraphManifest,
    SlotOverride, apply_slot_override, parse_module_entrypoint, resolve_closure,
};
use super::*;
use crate::manifest;
use mesh_core_service::{InterfaceContract, parse_interface_contract};
use std::collections::HashMap;
use std::path::PathBuf;

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
    resolution: ResolutionOutcome,
    /// Host↔contribution matching for every declared extension point, keyed by
    /// `(host module id, point contract name)`.
    extension_points: HashMap<(String, String), Vec<ResolvedExtensionPointContribution>>,
    /// Effective explicit placements for named customizable slots.
    node_slots:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, NodeSlotOverride>>,
    layout_entrypoint: Option<ResolvedLayoutEntrypoint>,
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
        mut root: RootModuleGraphManifest,
        modules: Vec<LoadedModuleManifest>,
        composition: CompositionContext,
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

        // Version resolution over the enabled closure. One version per module
        // id is not a limitation MESH could lift: the id is also the settings
        // namespace and the surface instance key.
        let enabled_manifests = graph_modules
            .values()
            .filter(|module| module.enabled)
            .map(|module| &module.manifest)
            .collect::<Vec<_>>();
        let enabled_ids = graph_modules
            .values()
            .filter(|module| module.enabled)
            .map(|module| module.id.as_str())
            .collect::<Vec<_>>();
        let resolution = resolve_closure(enabled_ids, enabled_manifests);
        for conflict in &resolution.conflicts {
            manual_diagnostics.push(ModuleGraphDiagnostic {
                module_id: conflict.module_id.clone(),
                contribution_id: Some(format!("{}:version", conflict.module_id)),
                status: "module_version_conflict".into(),
                message: conflict.message(),
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
            resolution,
            extension_points,
            node_slots: composition.node_slots,
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
