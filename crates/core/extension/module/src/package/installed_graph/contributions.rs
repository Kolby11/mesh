use super::super::{
    ModuleKind, ModuleManifest, ModuleManifestError, PathContribution, contained_path,
    dependency_spec_to_string, validate_relative_path,
};
use super::*;
use crate::manifest;
use mesh_core_component::parse_component;
use mesh_core_service::canonical_interface_name;
use std::collections::HashMap;

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
    pub(in crate::package) fn from_manifest(module_id: &str, manifest: &ModuleManifest) -> Self {
        let dependencies = &manifest.mesh.dependencies;
        let modules = dependencies
            .modules
            .iter()
            .map(|(id, spec)| (id.clone(), dependency_spec_to_string(spec)))
            .collect();
        Self {
            module_id: module_id.into(),
            modules,
            backend: dependencies
                .backend
                .iter()
                .map(|(interface, requirement)| {
                    (canonical_interface_name(interface), requirement.clone())
                })
                .collect(),
            optional_backend: dependencies
                .optional_backend
                .iter()
                .map(|(interface, requirement)| {
                    (canonical_interface_name(interface), requirement.clone())
                })
                .collect(),
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
    pub(in crate::package) frontend_entrypoints: Vec<ContributedFrontendEntrypoint>,
    pub(in crate::package) frontend_surfaces: Vec<ContributedFrontendSurface>,
    pub(in crate::package) layout: Vec<ContributedLayout>,
    pub(in crate::package) themes: Vec<ContributedTheme>,
    pub(in crate::package) icons: Vec<ContributedPathResource>,
    pub(in crate::package) fonts: Vec<ContributedPathResource>,
    pub(in crate::package) i18n: Vec<ContributedI18n>,
    pub(in crate::package) libraries: Vec<ContributedLibrary>,
    pub(in crate::package) settings: Vec<ContributedSettingsSchema>,
    pub(in crate::package) keybinds: Vec<ContributedKeybindAction>,
    pub(in crate::package) icon_requirements: Vec<ContributedIconRequirement>,
    pub(in crate::package) icon_packs: Vec<ContributedIconPack>,
    pub(in crate::package) extension_points: Vec<DeclaredExtensionPoint>,
    pub(in crate::package) extension_point_hosts: Vec<ExtensionPointHost>,
    pub(in crate::package) extension_point_contributions: Vec<ContributedExtensionPoint>,
}

impl ModuleContributionIndex {
    pub(in crate::package) fn index_module(
        &mut self,
        module: &InstalledModuleNode,
    ) -> Result<(), ModuleManifestError> {
        let module_id = module.id.as_str();
        let manifest = &module.manifest;
        let derived_props_schema = if module.kind == ModuleKind::Frontend {
            manifest
                .mesh
                .entrypoints
                .main
                .as_deref()
                .and_then(|entrypoint| {
                    module
                        .manifest_path
                        .parent()
                        .and_then(|dir| contained_path(dir, entrypoint, "frontend entrypoint").ok())
                })
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|source| parse_component(&source).ok())
                .and_then(|component| {
                    mesh_core_component::props_settings_schema(component.props.as_ref())
                })
        } else {
            None
        };
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
        }
        for (point_name, declaration) in &manifest.mesh.extension_points {
            self.extension_points.push(DeclaredExtensionPoint {
                source: ContributionSource::new(module, point_name),
                module_id: module_id.into(),
                name: point_name.clone(),
                version: declaration.version.clone(),
                multiple: declaration.multiple,
                props: declaration
                    .props
                    .iter()
                    .map(|prop| (prop.name.clone(), prop.prop_type.clone()))
                    .collect(),
            });
        }
        for (point_name, hosted) in &manifest.mesh.hosts {
            self.extension_point_hosts.push(ExtensionPointHost {
                source: ContributionSource::new(module, point_name),
                module_id: module_id.into(),
                name: point_name.clone(),
                version_req: hosted.version.clone(),
                layout: hosted.layout.clone(),
                max: hosted.max,
            });
        }
        for (point_name, contributions) in &manifest.mesh.contributes.extension_points {
            for contribution in contributions {
                validate_relative_path("extension point contribution entry", &contribution.entry)?;
                self.extension_point_contributions
                    .push(ContributedExtensionPoint {
                        source: ContributionSource::new(module, &contribution.id),
                        module_id: module_id.into(),
                        point: point_name.clone(),
                        id: contribution.id.clone(),
                        entry: contribution.entry.clone(),
                        order: contribution.order.unwrap_or(0),
                        props: contribution.props.clone(),
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
                mode_metadata: contribution.mode_metadata.clone(),
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
            let target_module_id = contribution
                .module
                .as_deref()
                .unwrap_or(module_id)
                .to_string();
            self.i18n.push(ContributedI18n {
                source: ContributionSource::new(module, &contribution.id),
                module_id: module_id.into(),
                target_module_id,
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
        if let Some(settings) = &manifest.mesh.contributes.settings
            && !(settings.namespace == module_id && derived_props_schema.is_some())
        {
            self.settings.push(ContributedSettingsSchema {
                source: ContributionSource::new(module, &settings.namespace),
                module_id: module_id.into(),
                namespace: settings.namespace.clone(),
                schema: settings.schema.clone(),
                settings_page: settings_page_entry(manifest),
            });
        }
        if let Some(schema) = derived_props_schema {
            self.settings.push(ContributedSettingsSchema {
                source: ContributionSource::new(module, "props"),
                module_id: module_id.into(),
                namespace: module_id.into(),
                schema,
                settings_page: settings_page_entry(manifest),
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
    pub mode_metadata: HashMap<String, manifest::ThemeModeMetadata>,
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
    /// Module that owns the source file (the language-pack module for a pack).
    pub module_id: String,
    /// Translation namespace receiving this catalog. Module-owned catalogs
    /// point at `module_id`; language packs explicitly target another module.
    pub target_module_id: String,
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
    /// Module-authored settings page, when this module contributes one to
    /// `mesh.settings.page`. It replaces the generated layout while this schema
    /// still governs validation and persistence.
    pub settings_page: Option<String>,
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

/// Host↔contribution matching for every declared extension point.
///
/// Runs on the graph's enabled set: a host that is installed but not composed
/// receives nothing, and its contributions report `unhosted_contribution`
/// rather than disappearing silently.
pub(in crate::package) fn resolve_extension_points(
    contributions: &ModuleContributionIndex,
) -> (
    HashMap<(String, String), Vec<ResolvedExtensionPointContribution>>,
    Vec<super::ModuleGraphDiagnostic>,
) {
    use mesh_core_service::{parse_contract_version, parse_version_req};

    let mut diagnostics = Vec::new();
    let declarations: HashMap<&str, &DeclaredExtensionPoint> = contributions
        .extension_points
        .iter()
        .map(|declaration| (declaration.name.as_str(), declaration))
        .collect();

    let mut hosts: Vec<&ExtensionPointHost> = Vec::new();
    for host in &contributions.extension_point_hosts {
        let Some(declaration) = declarations.get(host.name.as_str()) else {
            diagnostics.push(super::ModuleGraphDiagnostic {
                module_id: host.module_id.clone(),
                contribution_id: Some(format!("{}:hosts:{}", host.module_id, host.name)),
                status: "unknown_extension_point".into(),
                message: format!(
                    "{} hosts extension point {}, which no installed interface module declares",
                    host.module_id, host.name
                ),
            });
            continue;
        };
        if let Some(requirement) = &host.version_req
            && let (Some(request), Some(declared)) = (
                parse_version_req(requirement),
                parse_contract_version(&declaration.version),
            )
            && !request.matches(&declared)
        {
            diagnostics.push(super::ModuleGraphDiagnostic {
                module_id: host.module_id.clone(),
                contribution_id: Some(format!("{}:hosts:{}", host.module_id, host.name)),
                status: "extension_point_version_mismatch".into(),
                message: format!(
                    "{} hosts {} {requirement}, but the declared version is {}",
                    host.module_id, host.name, declaration.version
                ),
            });
            continue;
        }
        hosts.push(host);
    }

    let mut resolved: HashMap<(String, String), Vec<ResolvedExtensionPointContribution>> =
        HashMap::new();
    for contribution in &contributions.extension_point_contributions {
        let Some(declaration) = declarations.get(contribution.point.as_str()) else {
            diagnostics.push(super::ModuleGraphDiagnostic {
                module_id: contribution.module_id.clone(),
                contribution_id: Some(format!(
                    "{}:extension-point:{}",
                    contribution.module_id, contribution.id
                )),
                status: "unknown_extension_point".into(),
                message: format!(
                    "{} contributes to extension point {}, which no installed interface module declares",
                    contribution.module_id, contribution.point
                ),
            });
            continue;
        };
        if let Some(error) = extension_point_prop_error(declaration, contribution) {
            diagnostics.push(super::ModuleGraphDiagnostic {
                module_id: contribution.module_id.clone(),
                contribution_id: Some(format!(
                    "{}:extension-point:{}",
                    contribution.module_id, contribution.id
                )),
                status: "invalid_extension_point_props".into(),
                message: error,
            });
            continue;
        }

        let matching_hosts = hosts
            .iter()
            .filter(|host| host.name == contribution.point)
            .collect::<Vec<_>>();
        if matching_hosts.is_empty() {
            diagnostics.push(super::ModuleGraphDiagnostic {
                module_id: contribution.module_id.clone(),
                contribution_id: Some(format!(
                    "{}:extension-point:{}",
                    contribution.module_id, contribution.id
                )),
                status: "unhosted_contribution".into(),
                message: format!(
                    "{} contributes '{}' to {}, but no enabled module hosts that point",
                    contribution.module_id, contribution.id, contribution.point
                ),
            });
            continue;
        }
        for host in matching_hosts {
            resolved
                .entry((host.module_id.clone(), contribution.point.clone()))
                .or_default()
                .push(ResolvedExtensionPointContribution {
                    host_module_id: host.module_id.clone(),
                    point: contribution.point.clone(),
                    source_module_id: contribution.module_id.clone(),
                    contribution_id: contribution.id.clone(),
                    entry: contribution.entry.clone(),
                    order: contribution.order,
                    props: contribution.props.clone(),
                });
        }
    }

    // Deterministic render order, so a rebuild never reshuffles a settings page.
    for entries in resolved.values_mut() {
        entries.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.source_module_id.cmp(&right.source_module_id))
                .then_with(|| left.contribution_id.cmp(&right.contribution_id))
        });
    }
    for ((host_module_id, point), entries) in &resolved {
        let Some(declaration) = declarations.get(point.as_str()) else {
            continue;
        };
        if !declaration.multiple && entries.len() > 1 {
            diagnostics.push(super::ModuleGraphDiagnostic {
                module_id: host_module_id.clone(),
                contribution_id: Some(format!("{host_module_id}:hosts:{point}")),
                status: "extension_point_overfilled".into(),
                message: format!(
                    "{point} accepts one contribution, but {} modules contribute to {host_module_id}: {}",
                    entries.len(),
                    entries
                        .iter()
                        .map(|entry| entry.source_module_id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    }

    (resolved, diagnostics)
}

fn extension_point_prop_error(
    declaration: &DeclaredExtensionPoint,
    contribution: &ContributedExtensionPoint,
) -> Option<String> {
    use mesh_core_service::TypeExpr;

    for (name, value) in &contribution.props {
        let Some((_, type_expression)) = declaration
            .props
            .iter()
            .find(|(declared, _)| declared == name)
        else {
            return Some(format!(
                "contribution '{}' passes prop '{name}', which {} does not declare",
                contribution.id, declaration.name
            ));
        };
        match TypeExpr::parse(type_expression) {
            Ok(expression) if expression.matches(value) => {}
            Ok(_) => {
                return Some(format!(
                    "contribution '{}' prop '{name}' does not match declared type {type_expression}",
                    contribution.id
                ));
            }
            // An unparseable declared type is the interface module's bug and is
            // already reported there; do not blame the contributor for it.
            Err(_) => {}
        }
    }
    None
}

/// The entry of this module's own `mesh.settings.page` contribution, if any.
fn settings_page_entry(manifest: &ModuleManifest) -> Option<String> {
    manifest
        .mesh
        .contributes
        .extension_points
        .get(SETTINGS_PAGE_POINT)?
        .first()
        .map(|contribution| contribution.entry.clone())
}

/// The extension point a settings frontend hosts. Named here only to relate a
/// module's own page back to its schema for diagnostics — host matching itself
/// never names a module.
pub const SETTINGS_PAGE_POINT: &str = "mesh.settings.page";

/// An extension point contract declared by an interface module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredExtensionPoint {
    pub source: ContributionSource,
    pub module_id: String,
    pub name: String,
    pub version: String,
    pub multiple: bool,
    /// Declared prop name → type expression, in the interface type grammar.
    pub props: Vec<(String, String)>,
}

/// A module that renders contributions to an extension point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionPointHost {
    pub source: ContributionSource,
    pub module_id: String,
    pub name: String,
    pub version_req: Option<String>,
    pub layout: Option<String>,
    pub max: Option<u32>,
}

/// A module's contribution to an extension point, before host matching.
#[derive(Debug, Clone, PartialEq)]
pub struct ContributedExtensionPoint {
    pub source: ContributionSource,
    pub module_id: String,
    pub point: String,
    pub id: String,
    pub entry: String,
    pub order: i64,
    pub props: serde_json::Map<String, serde_json::Value>,
}

/// One contribution matched to one host, in render order.
///
/// A contribution resolves into *every* enabled host of its point: two settings
/// frontends both receive the pages, which is the correct behavior and needs no
/// special case.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedExtensionPointContribution {
    pub host_module_id: String,
    pub point: String,
    pub source_module_id: String,
    pub contribution_id: String,
    pub entry: String,
    pub order: i64,
    pub props: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedIconPack {
    pub source: ContributionSource,
    pub module_id: String,
    pub id: String,
    pub mappings: HashMap<String, String>,
}
