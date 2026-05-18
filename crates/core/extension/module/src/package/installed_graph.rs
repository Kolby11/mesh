use super::{
    InterfaceRelationship, MeshDependencies, ModuleKind, ModuleManifest, ModuleManifestDiagnostic,
    ModuleManifestError, PathContribution, RootModuleGraphManifest, dependency_spec_to_string,
    parse_module_entrypoint, validate_relative_path,
};
use crate::manifest;
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
    LegacyPackageJson,
    LegacyModuleJson,
    LegacyMeshToml,
}

#[derive(Debug, Clone)]
pub struct InstalledModuleGraph {
    modules: HashMap<String, InstalledModuleNode>,
    backend_providers: HashMap<String, Vec<BackendProviderNode>>,
    active_providers: HashMap<String, String>,
    frontend_requirements: HashMap<String, FrontendRequirementSet>,
    interface_declarations: HashMap<String, InterfaceDeclarationNode>,
    interface_guidance: Vec<InterfaceGuidanceRecord>,
    diagnostics: Vec<ModuleGraphDiagnostic>,
    contributions: ModuleContributionIndex,
    layout_entrypoint: Option<ResolvedLayoutEntrypoint>,
}

impl InstalledModuleGraph {
    pub fn from_parts(
        root: RootModuleGraphManifest,
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
                        FrontendRequirementSet::from_dependencies(
                            module_id,
                            &node.manifest.mesh.dependencies,
                        ),
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
                        file: interface.file.clone(),
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
        let interface_guidance = build_interface_guidance(&interface_declarations);
        let diagnostics =
            build_graph_diagnostics(&graph_modules, &frontend_requirements, &contributions);

        Ok(Self {
            modules: graph_modules,
            backend_providers,
            active_providers: root.providers,
            frontend_requirements,
            interface_declarations,
            interface_guidance,
            diagnostics,
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

    pub fn keybind_actions(&self) -> &[ContributedKeybindAction] {
        &self.contributions.keybinds
    }

    pub fn icon_requirements(&self) -> &[ContributedIconRequirement] {
        &self.contributions.icon_requirements
    }

    pub fn icon_pack_contributions(&self) -> &[ContributedIconPack] {
        &self.contributions.icon_packs
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
    pub label: Option<String>,
    pub priority: u32,
    pub required_capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDeclarationNode {
    pub source: ContributionSource,
    pub module_id: String,
    pub name: String,
    pub version: Option<String>,
    pub file: Option<String>,
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

fn build_graph_diagnostics(
    modules: &HashMap<String, InstalledModuleNode>,
    frontend_requirements: &HashMap<String, FrontendRequirementSet>,
    contributions: &ModuleContributionIndex,
) -> Vec<ModuleGraphDiagnostic> {
    let mut diagnostics = Vec::new();

    for requirements in frontend_requirements.values() {
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
    }

    if !contributions.icon_packs.is_empty() {
        for requirement in contributions
            .icon_requirements
            .iter()
            .filter(|requirement| requirement.required)
        {
            if !contributions
                .icon_packs
                .iter()
                .any(|pack| pack.mappings.contains_key(&requirement.name))
            {
                diagnostics.push(ModuleGraphDiagnostic {
                    module_id: requirement.module_id.clone(),
                    contribution_id: Some(requirement.source.scoped_id.clone()),
                    status: "missing_required_icon".into(),
                    message: format!(
                        "module {} requires semantic icon {}, but no enabled icon pack maps it",
                        requirement.module_id, requirement.name
                    ),
                });
            }
        }
    }

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

    diagnostics.sort_by(|a, b| {
        a.status
            .cmp(&b.status)
            .then_with(|| a.module_id.cmp(&b.module_id))
            .then_with(|| a.contribution_id.cmp(&b.contribution_id))
    });
    diagnostics
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
    pub icons: HashMap<String, String>,
    pub fonts: HashMap<String, String>,
    pub i18n: HashMap<String, String>,
    pub themes: HashMap<String, String>,
}

impl FrontendRequirementSet {
    fn from_dependencies(module_id: &str, dependencies: &MeshDependencies) -> Self {
        let modules = dependencies
            .modules
            .iter()
            .map(|(id, spec)| (id.clone(), dependency_spec_to_string(spec)))
            .collect();
        Self {
            module_id: module_id.into(),
            modules,
            backend: dependencies.backend.clone(),
            icons: dependencies.icons.clone(),
            fonts: dependencies.fonts.clone(),
            i18n: dependencies.i18n.clone(),
            themes: dependencies.themes.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModuleContributionIndex {
    frontend_entrypoints: Vec<ContributedFrontendEntrypoint>,
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
                self.frontend_entrypoints
                    .push(ContributedFrontendEntrypoint {
                        source: ContributionSource::new(module, "main"),
                        module_id: module_id.into(),
                        kind: FrontendEntrypointKind::Main,
                        path: path.clone(),
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
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedTheme {
    pub source: ContributionSource,
    pub module_id: String,
    pub id: String,
    pub label: String,
    pub modes: HashMap<String, String>,
    pub default_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedPathResource {
    pub source: ContributionSource,
    pub module_id: String,
    pub id: String,
    pub path: String,
    pub label: Option<String>,
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
    pub label: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub trigger: manifest::KeybindTrigger,
    pub localized_triggers: HashMap<String, manifest::KeybindTrigger>,
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
    let root = RootModuleGraphManifest::from_path(root_module_graph_path)?;
    let root_dir = root_module_graph_path.parent().ok_or_else(|| {
        ModuleManifestError::Validation(format!(
            "root module graph path must have a parent directory: {}",
            root_module_graph_path.display()
        ))
    })?;
    let modules_dir = root_dir.join(&root.modules_dir);
    let mut modules = Vec::new();

    for entry in root.modules.values() {
        modules.push(load_module_manifest(&modules_dir.join(&entry.path))?);
    }

    InstalledModuleGraph::from_parts(root, modules)
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
        if is_canonical_module_json(&content).map_err(|source| ModuleManifestError::Json {
            path: module_json.clone(),
            source,
        })? {
            let manifest = ModuleManifest::from_path(&module_json)?;
            return Ok(LoadedModuleManifest {
                manifest,
                path: module_json,
                source: ModuleManifestSource::CanonicalModuleJson,
                diagnostics: Vec::new(),
            });
        }

        let loaded = manifest::load_manifest(module_dir).map_err(|err| {
            ModuleManifestError::LegacyManifest {
                path: module_dir.to_path_buf(),
                message: err.to_string(),
            }
        })?;
        let path = loaded.path.clone();
        let manifest = ModuleManifest::from_legacy_manifest(loaded.manifest);
        let module_id = Some(manifest.name.clone());
        return Ok(LoadedModuleManifest {
            manifest,
            path,
            source: ModuleManifestSource::LegacyModuleJson,
            diagnostics: vec![ModuleManifestDiagnostic::warning(
                &module_json,
                module_id,
                Some("$".into()),
                "legacy module.json shape uses id/type/api_version fields",
                "replace legacy module.json fields with name/version/mesh",
            )],
        });
    }

    if package_json.exists() {
        let manifest = ModuleManifest::from_path(&package_json)?;
        let module_id = Some(manifest.name.clone());
        return Ok(LoadedModuleManifest {
            manifest,
            path: package_json.clone(),
            source: ModuleManifestSource::LegacyPackageJson,
            diagnostics: vec![ModuleManifestDiagnostic::warning(
                package_json,
                module_id,
                None,
                "package.json is a legacy MESH module manifest path",
                "replace package.json with module.json",
            )],
        });
    }

    if mesh_toml.exists() {
        let loaded = manifest::load_manifest(module_dir).map_err(|err| {
            ModuleManifestError::LegacyManifest {
                path: module_dir.to_path_buf(),
                message: err.to_string(),
            }
        })?;
        let path = loaded.path.clone();
        let manifest = ModuleManifest::from_legacy_manifest(loaded.manifest);
        let module_id = Some(manifest.name.clone());
        return Ok(LoadedModuleManifest {
            manifest,
            path,
            source: ModuleManifestSource::LegacyMeshToml,
            diagnostics: vec![ModuleManifestDiagnostic::warning(
                mesh_toml,
                module_id,
                None,
                "mesh.toml is a legacy MESH module manifest path",
                "replace mesh.toml with module.json",
            )],
        });
    }

    Err(ModuleManifestError::Validation(format!(
        "no module.json, package.json, or mesh.toml found in {}",
        module_dir.display()
    )))
}

fn is_canonical_module_json(content: &str) -> Result<bool, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(content)?;
    Ok(value.get("name").is_some() && value.get("mesh").is_some())
}
