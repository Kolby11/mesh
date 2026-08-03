use super::super::{
    ModuleKind, ModuleManifest, ModuleManifestError, PathContribution, dependency_spec_to_string,
    validate_relative_path,
};
use super::*;
use crate::manifest;
use mesh_core_component::parse_component;
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
                        .map(|dir| dir.join(entrypoint))
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
        if let Some(settings) = &manifest.mesh.contributes.settings
            && !(settings.namespace == module_id && derived_props_schema.is_some())
        {
            self.settings.push(ContributedSettingsSchema {
                source: ContributionSource::new(module, &settings.namespace),
                module_id: module_id.into(),
                namespace: settings.namespace.clone(),
                schema: settings.schema.clone(),
                settings_ui: manifest.mesh.entrypoints.settings_ui.clone(),
            });
        }
        if let Some(schema) = derived_props_schema {
            self.settings.push(ContributedSettingsSchema {
                source: ContributionSource::new(module, "props"),
                module_id: module_id.into(),
                namespace: module_id.into(),
                schema,
                settings_ui: manifest.mesh.entrypoints.settings_ui.clone(),
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
    /// Optional module-authored settings component that replaces the generated
    /// layout while retaining this schema for validation and persistence.
    pub settings_ui: Option<String>,
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
