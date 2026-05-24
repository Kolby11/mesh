use super::{ModuleManifestDiagnostic, ModuleManifestError, validate_relative_path};
use crate::manifest::{self, CapabilitiesSection, DependencySpec, Manifest, ModuleType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModuleManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<ModuleRepository>,
    pub mesh: MeshModuleSection,
}

impl ModuleManifest {
    pub fn from_json_str(input: &str) -> Result<Self, ModuleManifestError> {
        let parsed: Self =
            serde_json::from_str(input).map_err(|source| ModuleManifestError::Json {
                path: PathBuf::from("<inline>"),
                source,
            })?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn from_path(path: &Path) -> Result<Self, ModuleManifestError> {
        let content = std::fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let parsed: Self =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.name.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "module name cannot be empty".into(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(ModuleManifestError::Validation(format!(
                "module {} version cannot be empty",
                self.name
            )));
        }
        if let Some(repository) = &self.repository {
            repository.validate()?;
        }
        self.mesh.validate()
    }

    pub(crate) fn localized_text_diagnostics(&self, path: &Path) -> Vec<ModuleManifestDiagnostic> {
        self.mesh
            .localized_text_diagnostics(path, self.name.as_str())
    }

    pub fn into_runtime_manifest(self) -> Manifest {
        let mesh = self.mesh;
        let settings =
            mesh.contributes
                .settings
                .clone()
                .map(|settings| manifest::SettingsSection {
                    namespace: Some(settings.namespace),
                    schema_path: None,
                    inline_schema: Some(settings.schema),
                });
        let i18n = mesh
            .contributes
            .i18n
            .first()
            .map(|i18n| manifest::I18nSection {
                default_locale: i18n.locale.clone(),
                bundled: i18n.path.clone(),
            });
        let contributed_theme =
            mesh.contributes
                .themes
                .first()
                .map(|theme| manifest::ThemeSection {
                    tokens: HashMap::new(),
                    defaults: manifest::ThemeDefaultsSection::default(),
                    tokens_used: Vec::new(),
                    base: None,
                    modes: theme.modes.clone(),
                    default_mode: theme.default_mode.clone(),
                    extends: None,
                });
        let assets = mesh
            .contributes
            .icons
            .first()
            .map(|icons| manifest::AssetsSection {
                icons: Some(manifest::IconAssets::Path(icons.path.clone())),
            });
        let provides = mesh
            .implementations()
            .cloned()
            .into_iter()
            .map(|provided| manifest::ProvidedInterface {
                interface: provided.interface,
                version: provided.version,
                base_module: provided.base_module,
                backend_name: provided.label.or(provided.provider),
                priority: provided.priority,
                optional_capabilities: Vec::new(),
            })
            .collect();
        let interface = mesh.interface.clone().and_then(|interface| {
            let version = interface.version?;
            let file = interface.file?;
            Some(manifest::InterfaceSection {
                name: interface.name,
                version,
                file,
                extends: interface.extends,
            })
        });

        let manifest_theme = mesh.theme.clone().or(contributed_theme);

        Manifest {
            package: manifest::ModuleSection {
                id: self.name,
                name: None,
                version: self.version,
                module_type: ModuleType::from(mesh.kind),
                api_version: mesh.api_version,
                license: self.license,
                description: self.description,
                authors: Vec::new(),
                repository: self.repository.map(|repository| repository.url),
            },
            compatibility: manifest::CompatibilitySection::default(),
            dependencies: mesh.dependencies.into_manifest_dependencies(),
            capabilities: mesh.capabilities,
            entrypoints: manifest::EntrypointsSection {
                main: mesh.entrypoints.main,
                settings_ui: mesh.entrypoints.settings_ui,
            },
            accessibility: mesh.accessibility,
            settings,
            keybinds: mesh.keybinds,
            i18n,
            theme: manifest_theme,
            service: None,
            provides,
            interface,
            extensions: Vec::new(),
            exports: manifest::ExportsSection::default(),
            provides_slots: HashMap::new(),
            slot_contributions: HashMap::new(),
            assets,
            icons: mesh.icons,
            icon_pack: mesh.icon_pack,
            icon_requirements: mesh.icon_requirements,
            translations: HashMap::new(),
            surface_layout: mesh.surface_layout,
        }
    }

    pub(crate) fn from_legacy_manifest(manifest: Manifest) -> Self {
        let package = manifest.package.clone();
        let mut contributes = MeshContributes::default();

        if package.module_type == ModuleType::Surface || package.module_type == ModuleType::Widget {
            if let Some(main) = manifest.entrypoints.main.clone() {
                contributes.layout.push(LayoutContribution {
                    id: "main".into(),
                    entrypoint: main,
                    label: package.name.clone(),
                });
            }
        }
        if let Some(settings) = &manifest.settings {
            contributes.settings = Some(SettingsContribution {
                namespace: settings
                    .namespace
                    .clone()
                    .unwrap_or_else(|| package.id.clone()),
                schema: settings.inline_schema.clone().unwrap_or_default(),
            });
        }
        if let Some(theme) = &manifest.theme {
            let mut modes = theme.modes.clone();
            if modes.is_empty() {
                if let Some(base) = &theme.base {
                    modes.insert("default".into(), base.clone());
                }
            }
            if !modes.is_empty() {
                contributes.themes.push(ThemeContribution {
                    id: package.id.clone(),
                    label: package.name.clone().unwrap_or_else(|| package.id.clone()),
                    modes,
                    default_mode: theme.default_mode.clone(),
                });
            }
        }
        if let Some(i18n) = &manifest.i18n {
            contributes.i18n.push(I18nContribution {
                id: package.id.clone(),
                locale: i18n.default_locale.clone(),
                path: i18n.bundled.clone(),
            });
        }
        if let Some(assets) = &manifest.assets {
            if let Some(icons) = &assets.icons {
                contributes.icons.push(PathContribution {
                    id: package.id.clone(),
                    path: icons.path().to_string(),
                    label: package.name.clone(),
                });
            }
        }
        for font in &manifest.dependencies.fonts {
            contributes.fonts.push(PathContribution {
                id: font.family.clone(),
                path: font.family.clone(),
                label: None,
            });
        }

        let provides = manifest
            .declared_provides()
            .into_iter()
            .map(MeshProvidesDeclaration::from)
            .collect();
        let interface = manifest
            .interface
            .as_ref()
            .map(|interface| MeshInterfaceDeclaration {
                name: interface.name.clone(),
                version: Some(interface.version.clone()),
                file: Some(interface.file.clone()),
                domain: interface.name.split('.').nth(1).map(str::to_string),
                extends: interface.extends.clone(),
                relationship: interface
                    .extends
                    .as_ref()
                    .map(|_| InterfaceRelationship::Extension),
                reason: None,
            });
        let dependencies = MeshDependencies::from_manifest_dependencies(manifest.dependencies);
        let icons = manifest.icons.clone();
        let icon_pack = manifest.icon_pack.clone();
        let icon_requirements = manifest.icon_requirements.clone();
        let keybinds = manifest.keybinds.clone();
        let accessibility = manifest.accessibility.clone();
        let surface_layout = manifest.surface_layout.clone();

        Self {
            name: package.id,
            version: package.version,
            description: package.description,
            license: package.license,
            repository: package.repository.map(|url| ModuleRepository {
                repository_type: "git".into(),
                url,
            }),
            mesh: MeshModuleSection {
                api_version: package.api_version,
                kind: ModuleKind::from(package.module_type),
                capabilities: manifest.capabilities,
                i18n: MeshI18nSupport {
                    default_locale: manifest
                        .i18n
                        .as_ref()
                        .map(|i18n| i18n.default_locale.clone()),
                    supported_locales: manifest
                        .i18n
                        .as_ref()
                        .map(|i18n| vec![i18n.default_locale.clone()])
                        .unwrap_or_default(),
                },
                entrypoints: MeshEntrypoints {
                    main: manifest.entrypoints.main,
                    settings_ui: manifest.entrypoints.settings_ui,
                },
                keybinds,
                dependencies,
                provides,
                implements: Vec::new(),
                interface,
                theme: None,
                contributes,
                icons,
                icon_pack,
                icon_requirements,
                accessibility,
                surface_layout,
                experimental: serde_json::Value::Null,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeshModuleSection {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: ModuleKind,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub i18n: MeshI18nSupport,
    #[serde(default)]
    pub entrypoints: MeshEntrypoints,
    #[serde(default)]
    pub keybinds: manifest::KeybindsSection,
    #[serde(default)]
    pub dependencies: MeshDependencies,
    #[serde(default)]
    pub provides: Vec<MeshProvidesDeclaration>,
    #[serde(default)]
    pub implements: Vec<MeshProvidesDeclaration>,
    #[serde(default)]
    pub interface: Option<MeshInterfaceDeclaration>,
    #[serde(default)]
    pub theme: Option<manifest::ThemeSection>,
    #[serde(default)]
    pub contributes: MeshContributes,
    #[serde(default)]
    pub icons: Option<manifest::IconsSection>,
    #[serde(default)]
    pub icon_pack: Option<manifest::IconPackSection>,
    #[serde(default, rename = "iconRequirements", alias = "icon_requirements")]
    pub icon_requirements: manifest::IconRequirementsSection,
    #[serde(default)]
    pub accessibility: Option<manifest::AccessibilitySection>,
    #[serde(default, rename = "surfaceLayout", alias = "surface_layout")]
    pub surface_layout: Option<manifest::SurfaceLayoutSection>,
    #[serde(default)]
    pub experimental: serde_json::Value,
}

impl MeshModuleSection {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.api_version.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "mesh.apiVersion cannot be empty".into(),
            ));
        }
        self.i18n.validate()?;
        if self.kind == ModuleKind::Interface && self.interface.is_none() {
            return Err(ModuleManifestError::Validation(
                "interface modules must declare mesh.interface".into(),
            ));
        }
        if let Some(interface) = &self.interface {
            interface.validate()?;
            if self.kind == ModuleKind::Interface {
                if interface.version.is_none() {
                    return Err(ModuleManifestError::Validation(
                        "interface modules must declare mesh.interface.version".into(),
                    ));
                }
                if interface.file.is_none() {
                    return Err(ModuleManifestError::Validation(
                        "interface modules must declare mesh.interface.file".into(),
                    ));
                }
            }
        }
        if let Some(theme) = &self.theme {
            if self.kind != ModuleKind::Frontend {
                return Err(ModuleManifestError::Validation(
                    "mesh.theme is only supported for frontend modules".into(),
                ));
            }
            theme.validate().map_err(ModuleManifestError::Validation)?;
        }
        self.keybinds
            .validate()
            .map_err(ModuleManifestError::Validation)?;
        for provided in self.implementations() {
            provided.validate()?;
        }
        self.contributes.validate()
    }

    pub fn implementations(&self) -> impl Iterator<Item = &MeshProvidesDeclaration> {
        self.provides.iter().chain(self.implements.iter())
    }

    fn localized_text_diagnostics(
        &self,
        path: &Path,
        module_id: &str,
    ) -> Vec<ModuleManifestDiagnostic> {
        let mut diagnostics = Vec::new();

        for (action_id, action) in &self.keybinds.actions {
            for (field, value) in [
                ("label", action.label.as_ref()),
                ("description", action.description.as_ref()),
                ("category", action.category.as_ref()),
            ] {
                let Some(value) = value else {
                    continue;
                };
                if !value.is_suspicious_raw_i18n_key() {
                    continue;
                }

                let field_path = format!("mesh.keybinds.{action_id}.{field}");
                let key = value.fallback_text();
                diagnostics.push(ModuleManifestDiagnostic::warning(
                    path,
                    Some(module_id.to_string()),
                    Some(field_path.clone()),
                    format!("{field_path} looks like an i18n key but is a raw literal string"),
                    format!(
                        "use {{ \"t\": \"{key}\", \"fallback\": \"...\" }} to localize this field"
                    ),
                ));
            }
        }

        diagnostics
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshI18nSupport {
    #[serde(default, rename = "defaultLocale", alias = "default_locale")]
    pub default_locale: Option<String>,
    #[serde(default, rename = "supportedLocales", alias = "supported_locales")]
    pub supported_locales: Vec<String>,
}

impl MeshI18nSupport {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if let Some(default_locale) = &self.default_locale {
            if default_locale.trim().is_empty() {
                return Err(ModuleManifestError::Validation(
                    "mesh.i18n.defaultLocale cannot be empty".into(),
                ));
            }
            if !self.supported_locales.is_empty()
                && !self
                    .supported_locales
                    .iter()
                    .any(|locale| locale == default_locale)
            {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.i18n.defaultLocale {default_locale} must be listed in supportedLocales"
                )));
            }
        }

        for locale in &self.supported_locales {
            if locale.trim().is_empty() {
                return Err(ModuleManifestError::Validation(
                    "mesh.i18n.supportedLocales cannot contain empty locales".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleKind {
    Frontend,
    Backend,
    Theme,
    IconPack,
    FontPack,
    LanguagePack,
    Interface,
    Library,
}

impl From<ModuleType> for ModuleKind {
    fn from(module_type: ModuleType) -> Self {
        match module_type {
            ModuleType::Surface | ModuleType::Widget => Self::Frontend,
            ModuleType::Backend => Self::Backend,
            ModuleType::Theme => Self::Theme,
            ModuleType::IconPack => Self::IconPack,
            ModuleType::LanguagePack => Self::LanguagePack,
            ModuleType::Interface => Self::Interface,
        }
    }
}

impl From<ModuleKind> for ModuleType {
    fn from(kind: ModuleKind) -> Self {
        match kind {
            ModuleKind::Frontend => Self::Surface,
            ModuleKind::Backend => Self::Backend,
            ModuleKind::Theme => Self::Theme,
            ModuleKind::IconPack => Self::IconPack,
            ModuleKind::LanguagePack => Self::LanguagePack,
            ModuleKind::Interface => Self::Interface,
            ModuleKind::FontPack | ModuleKind::Library => Self::Widget,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModuleRepository {
    #[serde(rename = "type")]
    pub repository_type: String,
    pub url: String,
}

impl ModuleRepository {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.repository_type == "git" && self.url.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "repository.url cannot be empty when repository.type is git".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshEntrypoints {
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub settings_ui: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshDependencies {
    #[serde(default)]
    pub modules: HashMap<String, DependencySpec>,
    #[serde(default)]
    pub backend: HashMap<String, String>,
    #[serde(default)]
    pub icons: HashMap<String, String>,
    #[serde(default)]
    pub fonts: HashMap<String, String>,
    #[serde(default)]
    pub i18n: HashMap<String, String>,
    #[serde(default)]
    pub themes: HashMap<String, String>,
    #[serde(default)]
    pub binaries: Vec<manifest::BinaryDependency>,
}

impl MeshDependencies {
    fn from_manifest_dependencies(dependencies: crate::manifest::DependenciesSection) -> Self {
        let icons = dependencies
            .icon_packs
            .required
            .into_iter()
            .map(|id| (id, "*".into()))
            .collect();
        let i18n = dependencies
            .language_packs
            .required
            .into_iter()
            .map(|id| (id, "*".into()))
            .collect();
        let themes = dependencies
            .themes
            .required
            .into_iter()
            .map(|id| (id, "*".into()))
            .collect();
        let fonts = dependencies
            .fonts
            .into_iter()
            .map(|font| (font.family, "*".into()))
            .collect();
        Self {
            modules: dependencies.modules,
            backend: HashMap::new(),
            icons,
            fonts,
            i18n,
            themes,
            binaries: dependencies.binaries,
        }
    }

    fn into_manifest_dependencies(self) -> manifest::DependenciesSection {
        manifest::DependenciesSection {
            modules: self.modules,
            interfaces: Vec::new(),
            icon_packs: manifest::OptionalDependencyGroup {
                required: self.icons.keys().cloned().collect(),
                optional: Vec::new(),
            },
            language_packs: manifest::OptionalDependencyGroup {
                required: self.i18n.keys().cloned().collect(),
                optional: Vec::new(),
            },
            themes: manifest::OptionalDependencyGroup {
                required: self.themes.keys().cloned().collect(),
                optional: Vec::new(),
            },
            native_libs: Vec::new(),
            binaries: self.binaries,
            fonts: self
                .fonts
                .keys()
                .cloned()
                .map(|family| manifest::FontDependency {
                    family,
                    reason: None,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeshProvidesDeclaration {
    pub interface: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, rename = "baseModule", alias = "base_module")]
    pub base_module: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub priority: u32,
}

impl MeshProvidesDeclaration {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.interface.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "mesh.provides interface cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshInterfaceDeclaration {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub relationship: Option<InterfaceRelationship>,
    #[serde(default)]
    pub reason: Option<String>,
}

impl MeshInterfaceDeclaration {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.name.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.name cannot be empty".into(),
            ));
        }
        if let Some(version) = &self.version
            && version.trim().is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.version cannot be empty".into(),
            ));
        }
        if let Some(file) = &self.file
            && file.trim().is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.file cannot be empty".into(),
            ));
        }
        if let Some(domain) = &self.domain
            && domain.trim().is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.domain cannot be empty".into(),
            ));
        }
        if let Some(extends) = &self.extends
            && extends.trim().is_empty()
        {
            return Err(ModuleManifestError::Validation(
                "mesh.interface.extends cannot be empty".into(),
            ));
        }
        match (self.relationship, self.extends.as_ref()) {
            (Some(InterfaceRelationship::Extension), None) => {
                return Err(ModuleManifestError::Validation(
                    "mesh.interface.relationship extension requires mesh.interface.extends".into(),
                ));
            }
            (Some(InterfaceRelationship::Base), Some(_)) => {
                return Err(ModuleManifestError::Validation(
                    "mesh.interface.relationship base cannot set mesh.interface.extends".into(),
                ));
            }
            (Some(InterfaceRelationship::Independent), Some(_)) => {
                return Err(ModuleManifestError::Validation(
                    "mesh.interface.relationship independent cannot set mesh.interface.extends"
                        .into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn effective_relationship(&self) -> InterfaceRelationship {
        self.relationship.unwrap_or_else(|| {
            if self.extends.is_some() {
                InterfaceRelationship::Extension
            } else if self.name.starts_with("mesh.") {
                InterfaceRelationship::Base
            } else {
                InterfaceRelationship::Independent
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterfaceRelationship {
    Base,
    Extension,
    Independent,
}

impl From<crate::manifest::ProvidedInterface> for MeshProvidesDeclaration {
    fn from(provided: crate::manifest::ProvidedInterface) -> Self {
        Self {
            interface: provided.interface,
            version: provided.version,
            base_module: provided.base_module,
            provider: provided.backend_name.clone(),
            label: provided.backend_name,
            priority: provided.priority,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MeshContributes {
    #[serde(default)]
    pub layout: Vec<LayoutContribution>,
    #[serde(default)]
    pub settings: Option<SettingsContribution>,
    #[serde(default)]
    pub themes: Vec<ThemeContribution>,
    #[serde(default)]
    pub icons: Vec<PathContribution>,
    #[serde(default)]
    pub fonts: Vec<PathContribution>,
    #[serde(default)]
    pub i18n: Vec<I18nContribution>,
    #[serde(default)]
    pub libraries: Vec<LibraryContribution>,
}

impl MeshContributes {
    fn validate(&self) -> Result<(), ModuleManifestError> {
        for contribution in &self.layout {
            validate_relative_path("layout entrypoint", &contribution.entrypoint)?;
        }
        for contribution in &self.themes {
            for path in contribution.modes.values() {
                validate_relative_path("theme mode", path)?;
            }
        }
        for contribution in &self.icons {
            validate_relative_path("icon contribution", &contribution.path)?;
        }
        for contribution in &self.fonts {
            validate_relative_path("font contribution", &contribution.path)?;
        }
        for contribution in &self.i18n {
            validate_relative_path("i18n contribution", &contribution.path)?;
        }
        for contribution in &self.libraries {
            contribution.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LayoutContribution {
    pub id: String,
    pub entrypoint: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SettingsContribution {
    pub namespace: String,
    #[serde(default)]
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThemeContribution {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub modes: HashMap<String, String>,
    #[serde(default)]
    pub default_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathContribution {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct I18nContribution {
    pub id: String,
    pub locale: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryContribution {
    pub namespace: String,
    pub path: String,
}

impl LibraryContribution {
    pub(crate) fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.namespace.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "library namespace cannot be empty".into(),
            ));
        }
        validate_relative_path("library contribution", &self.path)
    }
}
