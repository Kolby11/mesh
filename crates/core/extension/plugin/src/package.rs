use crate::manifest::{
    self, CapabilitiesSection, DependencySpec, Manifest, ManifestSource, PluginType,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PackageManifestError {
    #[error("failed to read package manifest {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse package manifest {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("invalid MESH_HOME: {0}")]
    InvalidMeshHome(String),

    #[error("invalid package manifest: {0}")]
    Validation(String),

    #[error("legacy manifest error for {path}: {message}")]
    LegacyManifest { path: PathBuf, message: String },
}

pub fn mesh_home() -> Result<PathBuf, PackageManifestError> {
    if let Ok(path) = std::env::var("MESH_HOME") {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(PackageManifestError::InvalidMeshHome(
                "MESH_HOME cannot be empty".into(),
            ));
        }
        let path = PathBuf::from(trimmed);
        if !path.is_absolute() {
            return Err(PackageManifestError::InvalidMeshHome(format!(
                "MESH_HOME must be absolute: {}",
                path.display()
            )));
        }
        return Ok(path);
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| PackageManifestError::InvalidMeshHome("HOME is not set".into()))?;
    Ok(home.join(".mesh"))
}

pub fn root_package_manifest_path() -> Result<PathBuf, PackageManifestError> {
    Ok(mesh_home()?.join("package.json"))
}

pub fn settings_path() -> Result<PathBuf, PackageManifestError> {
    Ok(mesh_home()?.join("settings.json"))
}

pub fn modules_dir() -> Result<PathBuf, PackageManifestError> {
    Ok(mesh_home()?.join("modules"))
}

pub fn themes_dir() -> Result<PathBuf, PackageManifestError> {
    Ok(mesh_home()?.join("themes"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootPackageManifest {
    pub schema_version: u32,
    #[serde(default = "default_modules_dir")]
    pub modules_dir: String,
    #[serde(default)]
    pub modules: HashMap<String, InstalledModuleEntry>,
    #[serde(default)]
    pub providers: HashMap<String, String>,
    #[serde(default)]
    pub layout: Option<RootLayoutSelection>,
    #[serde(default)]
    pub theme: Option<RootThemeSelection>,
}

impl RootPackageManifest {
    pub fn from_json_str(input: &str) -> Result<Self, PackageManifestError> {
        let parsed: RootPackageJson =
            serde_json::from_str(input).map_err(|source| PackageManifestError::Json {
                path: PathBuf::from("<inline>"),
                source,
            })?;
        let manifest = parsed.into_manifest();
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn from_path(path: &Path) -> Result<Self, PackageManifestError> {
        let content = std::fs::read_to_string(path).map_err(|source| PackageManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let parsed: RootPackageJson =
            serde_json::from_str(&content).map_err(|source| PackageManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        let manifest = parsed.into_manifest();
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), PackageManifestError> {
        if self.schema_version != 1 {
            return Err(PackageManifestError::Validation(format!(
                "unsupported schemaVersion {}; supported version is 1",
                self.schema_version
            )));
        }
        validate_modules_dir(&self.modules_dir)?;
        for (module_id, entry) in &self.modules {
            if module_id.trim().is_empty() {
                return Err(PackageManifestError::Validation(
                    "module id cannot be empty".into(),
                ));
            }
            entry.validate(module_id)?;
        }
        if let Some(layout) = &self.layout {
            layout.validate()?;
        }
        if let Some(theme) = &self.theme {
            theme.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootPackageJson {
    #[serde(default)]
    schema_version: Option<u32>,
    #[serde(default)]
    modules_dir: Option<String>,
    #[serde(default)]
    modules: HashMap<String, InstalledModuleEntry>,
    #[serde(default)]
    providers: HashMap<String, String>,
    #[serde(default)]
    layout: Option<RootLayoutSelection>,
    #[serde(default)]
    theme: Option<RootThemeSelection>,
    #[serde(default)]
    mesh: Option<RootMeshSection>,
}

impl RootPackageJson {
    fn into_manifest(self) -> RootPackageManifest {
        if let Some(mesh) = self.mesh {
            return RootPackageManifest {
                schema_version: mesh.schema_version,
                modules_dir: mesh.modules_dir,
                modules: mesh.modules,
                providers: mesh.providers,
                layout: mesh.layout,
                theme: mesh.theme,
            };
        }

        RootPackageManifest {
            schema_version: self.schema_version.unwrap_or(1),
            modules_dir: self.modules_dir.unwrap_or_else(default_modules_dir),
            modules: self.modules,
            providers: self.providers,
            layout: self.layout,
            theme: self.theme,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootMeshSection {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default = "default_modules_dir")]
    modules_dir: String,
    #[serde(default)]
    modules: HashMap<String, InstalledModuleEntry>,
    #[serde(default)]
    providers: HashMap<String, String>,
    #[serde(default)]
    layout: Option<RootLayoutSelection>,
    #[serde(default)]
    theme: Option<RootThemeSelection>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstalledModuleEntry {
    pub kind: ModuleKind,
    pub path: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl InstalledModuleEntry {
    fn validate(&self, module_id: &str) -> Result<(), PackageManifestError> {
        if self.path.trim().is_empty() {
            return Err(PackageManifestError::Validation(format!(
                "module {module_id} path cannot be empty"
            )));
        }
        validate_relative_path("module path", &self.path)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RootLayoutSelection {
    pub entrypoint: String,
}

impl RootLayoutSelection {
    fn validate(&self) -> Result<(), PackageManifestError> {
        if parse_module_entrypoint(&self.entrypoint).is_none() {
            return Err(PackageManifestError::Validation(format!(
                "layout entrypoint must use <module-id>:<entrypoint-id>: {}",
                self.entrypoint
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RootThemeSelection {
    pub active: String,
    #[serde(default)]
    pub mode: Option<String>,
}

impl RootThemeSelection {
    fn validate(&self) -> Result<(), PackageManifestError> {
        if self.active.trim().is_empty() {
            return Err(PackageManifestError::Validation(
                "theme.active cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModulePackageManifest {
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

impl ModulePackageManifest {
    pub fn from_json_str(input: &str) -> Result<Self, PackageManifestError> {
        let parsed: Self =
            serde_json::from_str(input).map_err(|source| PackageManifestError::Json {
                path: PathBuf::from("<inline>"),
                source,
            })?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn from_path(path: &Path) -> Result<Self, PackageManifestError> {
        let content = std::fs::read_to_string(path).map_err(|source| PackageManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let parsed: Self =
            serde_json::from_str(&content).map_err(|source| PackageManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn validate(&self) -> Result<(), PackageManifestError> {
        if self.name.trim().is_empty() {
            return Err(PackageManifestError::Validation(
                "module name cannot be empty".into(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(PackageManifestError::Validation(format!(
                "module {} version cannot be empty",
                self.name
            )));
        }
        if let Some(repository) = &self.repository {
            repository.validate()?;
        }
        self.mesh.validate()
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
        let theme = mesh
            .contributes
            .themes
            .first()
            .map(|theme| manifest::ThemeSection {
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
                icons: Some(icons.path.clone()),
            });
        let provides = mesh
            .implementations()
            .cloned()
            .into_iter()
            .map(|provided| manifest::ProvidedInterface {
                interface: provided.interface,
                version: provided.version,
                base_plugin: provided.base_plugin,
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

        Manifest {
            package: manifest::PackageSection {
                id: self.name,
                name: None,
                version: self.version,
                plugin_type: PluginType::from(mesh.kind),
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
            accessibility: None,
            settings,
            i18n,
            theme,
            service: None,
            provides,
            interface,
            extensions: Vec::new(),
            exports: manifest::ExportsSection::default(),
            provides_slots: HashMap::new(),
            slot_contributions: HashMap::new(),
            assets,
            icon_requirements: manifest::IconRequirementsSection::default(),
            translations: HashMap::new(),
            surface_layout: None,
        }
    }

    fn from_legacy_manifest(manifest: Manifest) -> Self {
        let package = manifest.package.clone();
        let mut contributes = MeshContributes::default();

        if package.plugin_type == PluginType::Surface || package.plugin_type == PluginType::Widget {
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
                    path: icons.clone(),
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
                kind: ModuleKind::from(package.plugin_type),
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
                dependencies,
                provides,
                implements: Vec::new(),
                interface,
                contributes,
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
    pub dependencies: MeshDependencies,
    #[serde(default)]
    pub provides: Vec<MeshProvidesDeclaration>,
    #[serde(default)]
    pub implements: Vec<MeshProvidesDeclaration>,
    #[serde(default)]
    pub interface: Option<MeshInterfaceDeclaration>,
    #[serde(default)]
    pub contributes: MeshContributes,
    #[serde(default)]
    pub experimental: serde_json::Value,
}

impl MeshModuleSection {
    fn validate(&self) -> Result<(), PackageManifestError> {
        if self.api_version.trim().is_empty() {
            return Err(PackageManifestError::Validation(
                "mesh.apiVersion cannot be empty".into(),
            ));
        }
        self.i18n.validate()?;
        if self.kind == ModuleKind::Interface && self.interface.is_none() {
            return Err(PackageManifestError::Validation(
                "interface modules must declare mesh.interface".into(),
            ));
        }
        if let Some(interface) = &self.interface {
            interface.validate()?;
            if self.kind == ModuleKind::Interface {
                if interface.version.is_none() {
                    return Err(PackageManifestError::Validation(
                        "interface modules must declare mesh.interface.version".into(),
                    ));
                }
                if interface.file.is_none() {
                    return Err(PackageManifestError::Validation(
                        "interface modules must declare mesh.interface.file".into(),
                    ));
                }
            }
        }
        for provided in self.implementations() {
            provided.validate()?;
        }
        self.contributes.validate()
    }

    fn implementations(&self) -> impl Iterator<Item = &MeshProvidesDeclaration> {
        self.provides.iter().chain(self.implements.iter())
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
    fn validate(&self) -> Result<(), PackageManifestError> {
        if let Some(default_locale) = &self.default_locale {
            if default_locale.trim().is_empty() {
                return Err(PackageManifestError::Validation(
                    "mesh.i18n.defaultLocale cannot be empty".into(),
                ));
            }
            if !self.supported_locales.is_empty()
                && !self
                    .supported_locales
                    .iter()
                    .any(|locale| locale == default_locale)
            {
                return Err(PackageManifestError::Validation(format!(
                    "mesh.i18n.defaultLocale {default_locale} must be listed in supportedLocales"
                )));
            }
        }

        for locale in &self.supported_locales {
            if locale.trim().is_empty() {
                return Err(PackageManifestError::Validation(
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

impl From<PluginType> for ModuleKind {
    fn from(plugin_type: PluginType) -> Self {
        match plugin_type {
            PluginType::Surface | PluginType::Widget => Self::Frontend,
            PluginType::Backend => Self::Backend,
            PluginType::Theme => Self::Theme,
            PluginType::IconPack => Self::IconPack,
            PluginType::LanguagePack => Self::LanguagePack,
            PluginType::Interface => Self::Interface,
        }
    }
}

impl From<ModuleKind> for PluginType {
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
    fn validate(&self) -> Result<(), PackageManifestError> {
        if self.repository_type == "git" && self.url.trim().is_empty() {
            return Err(PackageManifestError::Validation(
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
            modules: dependencies.plugins,
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
            plugins: self.modules,
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
    #[serde(default, rename = "basePlugin", alias = "base_plugin")]
    pub base_plugin: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub priority: u32,
}

impl MeshProvidesDeclaration {
    fn validate(&self) -> Result<(), PackageManifestError> {
        if self.interface.trim().is_empty() {
            return Err(PackageManifestError::Validation(
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
    fn validate(&self) -> Result<(), PackageManifestError> {
        if self.name.trim().is_empty() {
            return Err(PackageManifestError::Validation(
                "mesh.interface.name cannot be empty".into(),
            ));
        }
        if let Some(version) = &self.version
            && version.trim().is_empty()
        {
            return Err(PackageManifestError::Validation(
                "mesh.interface.version cannot be empty".into(),
            ));
        }
        if let Some(file) = &self.file
            && file.trim().is_empty()
        {
            return Err(PackageManifestError::Validation(
                "mesh.interface.file cannot be empty".into(),
            ));
        }
        if let Some(domain) = &self.domain
            && domain.trim().is_empty()
        {
            return Err(PackageManifestError::Validation(
                "mesh.interface.domain cannot be empty".into(),
            ));
        }
        if let Some(extends) = &self.extends
            && extends.trim().is_empty()
        {
            return Err(PackageManifestError::Validation(
                "mesh.interface.extends cannot be empty".into(),
            ));
        }
        Ok(())
    }

    fn effective_relationship(&self) -> InterfaceRelationship {
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
            base_plugin: provided.base_plugin,
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
    fn validate(&self) -> Result<(), PackageManifestError> {
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
    fn validate(&self) -> Result<(), PackageManifestError> {
        if self.namespace.trim().is_empty() {
            return Err(PackageManifestError::Validation(
                "library namespace cannot be empty".into(),
            ));
        }
        validate_relative_path("library contribution", &self.path)
    }
}

#[derive(Debug, Clone)]
pub struct LoadedModuleManifest {
    pub manifest: ModulePackageManifest,
    pub path: PathBuf,
    pub source: ModuleManifestSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleManifestSource {
    PackageJson,
    LegacyPluginJson,
}

#[derive(Debug, Clone)]
pub struct InstalledModuleGraph {
    modules: HashMap<String, InstalledModuleNode>,
    backend_providers: HashMap<String, Vec<BackendProviderNode>>,
    active_providers: HashMap<String, String>,
    frontend_requirements: HashMap<String, FrontendRequirementSet>,
    interface_declarations: HashMap<String, InterfaceDeclarationNode>,
    interface_guidance: Vec<InterfaceGuidanceRecord>,
    contributions: ModuleContributionIndex,
    layout_entrypoint: Option<ResolvedLayoutEntrypoint>,
}

impl InstalledModuleGraph {
    pub fn from_parts(
        root: RootPackageManifest,
        modules: Vec<LoadedModuleManifest>,
    ) -> Result<Self, PackageManifestError> {
        root.validate()?;
        let mut loaded_by_id = HashMap::new();
        for loaded in modules {
            loaded.manifest.validate()?;
            if loaded_by_id
                .insert(loaded.manifest.name.clone(), loaded)
                .is_some()
            {
                return Err(PackageManifestError::Validation(
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
                PackageManifestError::Validation(format!(
                    "root package references module {module_id} but no module package was loaded"
                ))
            })?;
            if loaded.manifest.mesh.kind != entry.kind {
                return Err(PackageManifestError::Validation(format!(
                    "module {module_id} kind mismatch: root has {:?}, package has {:?}",
                    entry.kind, loaded.manifest.mesh.kind
                )));
            }

            let node = InstalledModuleNode {
                id: module_id.clone(),
                kind: entry.kind,
                path: entry.path.clone(),
                enabled: entry.enabled,
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
                        module_id: module_id.clone(),
                        name: interface.name.clone(),
                        version: interface.version.clone(),
                        domain: interface.domain.clone(),
                        extends: interface.extends.clone(),
                        relationship: interface.effective_relationship(),
                        reason: interface.reason.clone(),
                    };
                    interface_declarations.insert(declaration.name.clone(), declaration);
                }

                for provided in node.manifest.mesh.implementations() {
                    let provider = BackendProviderNode {
                        module_id: module_id.clone(),
                        interface: provided.interface.clone(),
                        provider: provided.provider.clone(),
                        label: provided.label.clone(),
                        priority: provided.priority,
                    };
                    backend_providers
                        .entry(provided.interface.clone())
                        .or_default()
                        .push(provider);
                }

                contributions.index_module(module_id, &node.manifest)?;
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
                return Err(PackageManifestError::Validation(format!(
                    "active provider {module_id} for {interface} is not installed"
                )));
            };
            if !node.enabled {
                return Err(PackageManifestError::Validation(format!(
                    "active provider {module_id} for {interface} is disabled"
                )));
            }
            if node.kind != ModuleKind::Backend {
                return Err(PackageManifestError::Validation(format!(
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
                return Err(PackageManifestError::Validation(format!(
                    "active provider {module_id} does not provide {interface}"
                )));
            }
        }

        let layout_entrypoint = match root.layout {
            Some(layout) => {
                let (module_id, entrypoint_id) = parse_module_entrypoint(&layout.entrypoint)
                    .ok_or_else(|| {
                        PackageManifestError::Validation(format!(
                            "invalid layout entrypoint {}",
                            layout.entrypoint
                        ))
                    })?;
                let node = graph_modules.get(module_id).ok_or_else(|| {
                    PackageManifestError::Validation(format!(
                        "layout entrypoint module {module_id} is not installed"
                    ))
                })?;
                if !node.enabled || node.kind != ModuleKind::Frontend {
                    return Err(PackageManifestError::Validation(format!(
                        "layout entrypoint module {module_id} must be an enabled frontend module"
                    )));
                }
                let contribution = contributions
                    .layout
                    .iter()
                    .find(|item| item.module_id == module_id && item.id == entrypoint_id)
                    .ok_or_else(|| {
                        PackageManifestError::Validation(format!(
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

        Ok(Self {
            modules: graph_modules,
            backend_providers,
            active_providers: root.providers,
            frontend_requirements,
            interface_declarations,
            interface_guidance,
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
}

#[derive(Debug, Clone)]
pub struct InstalledModuleNode {
    pub id: String,
    pub kind: ModuleKind,
    pub path: String,
    pub enabled: bool,
    pub manifest: ModulePackageManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendProviderNode {
    pub module_id: String,
    pub interface: String,
    pub provider: Option<String>,
    pub label: Option<String>,
    pub priority: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDeclarationNode {
    pub module_id: String,
    pub name: String,
    pub version: Option<String>,
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
    layout: Vec<ContributedLayout>,
    themes: Vec<ContributedTheme>,
    icons: Vec<ContributedPathResource>,
    fonts: Vec<ContributedPathResource>,
    i18n: Vec<ContributedI18n>,
    libraries: Vec<ContributedLibrary>,
    settings: Vec<ContributedSettingsSchema>,
}

impl ModuleContributionIndex {
    fn index_module(
        &mut self,
        module_id: &str,
        manifest: &ModulePackageManifest,
    ) -> Result<(), PackageManifestError> {
        for contribution in &manifest.mesh.contributes.layout {
            validate_relative_path("layout entrypoint", &contribution.entrypoint)?;
            self.layout.push(ContributedLayout {
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
                module_id: module_id.into(),
                id: contribution.id.clone(),
                label: contribution.label.clone(),
                modes: contribution.modes.clone(),
                default_mode: contribution.default_mode.clone(),
            });
        }
        for contribution in &manifest.mesh.contributes.icons {
            self.icons.push(ContributedPathResource::from_contribution(
                module_id,
                contribution,
            )?);
        }
        for contribution in &manifest.mesh.contributes.fonts {
            self.fonts.push(ContributedPathResource::from_contribution(
                module_id,
                contribution,
            )?);
        }
        for contribution in &manifest.mesh.contributes.i18n {
            validate_relative_path("i18n contribution", &contribution.path)?;
            self.i18n.push(ContributedI18n {
                module_id: module_id.into(),
                id: contribution.id.clone(),
                locale: contribution.locale.clone(),
                path: contribution.path.clone(),
            });
        }
        for contribution in &manifest.mesh.contributes.libraries {
            contribution.validate()?;
            self.libraries.push(ContributedLibrary {
                module_id: module_id.into(),
                namespace: contribution.namespace.clone(),
                path: contribution.path.clone(),
            });
        }
        if let Some(settings) = &manifest.mesh.contributes.settings {
            self.settings.push(ContributedSettingsSchema {
                module_id: module_id.into(),
                namespace: settings.namespace.clone(),
                schema: settings.schema.clone(),
            });
        }
        Ok(())
    }
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
    pub module_id: String,
    pub id: String,
    pub path: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedTheme {
    pub module_id: String,
    pub id: String,
    pub label: String,
    pub modes: HashMap<String, String>,
    pub default_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedPathResource {
    pub module_id: String,
    pub id: String,
    pub path: String,
    pub label: Option<String>,
}

impl ContributedPathResource {
    fn from_contribution(
        module_id: &str,
        contribution: &PathContribution,
    ) -> Result<Self, PackageManifestError> {
        validate_relative_path("path contribution", &contribution.path)?;
        Ok(Self {
            module_id: module_id.into(),
            id: contribution.id.clone(),
            path: contribution.path.clone(),
            label: contribution.label.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedI18n {
    pub module_id: String,
    pub id: String,
    pub locale: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedLibrary {
    pub module_id: String,
    pub namespace: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContributedSettingsSchema {
    pub module_id: String,
    pub namespace: String,
    pub schema: serde_json::Value,
}

pub fn load_installed_module_graph(
    root_package_path: &Path,
) -> Result<InstalledModuleGraph, PackageManifestError> {
    let root = RootPackageManifest::from_path(root_package_path)?;
    let root_dir = root_package_path.parent().ok_or_else(|| {
        PackageManifestError::Validation(format!(
            "root package path must have a parent directory: {}",
            root_package_path.display()
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
) -> Result<LoadedModuleManifest, PackageManifestError> {
    let module_json = module_dir.join("module.json");
    if module_json.exists() {
        let loaded = manifest::load_manifest(module_dir).map_err(|err| {
            PackageManifestError::LegacyManifest {
                path: module_dir.to_path_buf(),
                message: err.to_string(),
            }
        })?;
        let path = loaded.path.clone();
        let manifest = ModulePackageManifest::from_legacy_manifest(loaded.manifest);
        return Ok(LoadedModuleManifest {
            manifest,
            path,
            source: match loaded.source {
                ManifestSource::PluginJson | ManifestSource::MeshToml => {
                    ModuleManifestSource::LegacyPluginJson
                }
            },
        });
    }

    let package_json = module_dir.join("package.json");
    if package_json.exists() {
        let manifest = ModulePackageManifest::from_path(&package_json)?;
        return Ok(LoadedModuleManifest {
            manifest,
            path: package_json,
            source: ModuleManifestSource::PackageJson,
        });
    }

    let plugin_json = module_dir.join("plugin.json");
    if plugin_json.exists() {
        let loaded = manifest::load_manifest(module_dir).map_err(|err| {
            PackageManifestError::LegacyManifest {
                path: module_dir.to_path_buf(),
                message: err.to_string(),
            }
        })?;
        let path = loaded.path.clone();
        let manifest = ModulePackageManifest::from_legacy_manifest(loaded.manifest);
        return Ok(LoadedModuleManifest {
            manifest,
            path,
            source: match loaded.source {
                ManifestSource::PluginJson | ManifestSource::MeshToml => {
                    ModuleManifestSource::LegacyPluginJson
                }
            },
        });
    }

    Err(PackageManifestError::Validation(format!(
        "no package.json or plugin.json found in {}",
        module_dir.display()
    )))
}

fn default_modules_dir() -> String {
    "modules".into()
}

fn default_schema_version() -> u32 {
    1
}

fn default_enabled() -> bool {
    true
}

fn validate_modules_dir(value: &str) -> Result<(), PackageManifestError> {
    let path = Path::new(value);
    if value.trim().is_empty() {
        return Err(PackageManifestError::Validation(
            "modulesDir cannot be empty".into(),
        ));
    }
    if path.is_absolute() {
        return Err(PackageManifestError::Validation(format!(
            "modulesDir must be a relative path: {value}"
        )));
    }
    Ok(())
}

fn validate_relative_path(label: &str, value: &str) -> Result<(), PackageManifestError> {
    let path = Path::new(value);
    if value.trim().is_empty() {
        return Err(PackageManifestError::Validation(format!(
            "{label} cannot be empty"
        )));
    }
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(PackageManifestError::Validation(format!(
            "{label} must be a relative path without '..': {value}"
        )));
    }
    Ok(())
}

fn parse_module_entrypoint(value: &str) -> Option<(&str, &str)> {
    let (module_id, entrypoint_id) = value.rsplit_once(':')?;
    if module_id.trim().is_empty() || entrypoint_id.trim().is_empty() {
        return None;
    }
    Some((module_id, entrypoint_id))
}

fn dependency_spec_to_string(spec: &DependencySpec) -> String {
    match spec {
        DependencySpec::Simple(value) => value.clone(),
        DependencySpec::Detailed { version, .. } => version.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EnvGuard {
        key: &'static str,
        old: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let old = std::env::var(key).ok();
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
            Self { key, old }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.old {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("mesh-{name}-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn module_package_paths_default_to_dot_mesh() {
        let _guard = EnvGuard::set("MESH_HOME", None);
        let path = root_package_manifest_path().unwrap();
        assert!(path.ends_with(".mesh/package.json"));
    }

    #[test]
    fn module_package_paths_reject_relative_mesh_home() {
        let _guard = EnvGuard::set("MESH_HOME", Some("relative/path"));
        assert!(matches!(
            mesh_home(),
            Err(PackageManifestError::InvalidMeshHome(_))
        ));
    }

    #[test]
    fn module_root_manifest_parses_minimal_package_json() {
        let content = r#"
{
  "name": "@mesh/local-config",
  "version": "0.1.0",
  "private": true,
  "mesh": {
  "schemaVersion": 1,
  "modulesDir": "modules",
  "modules": {},
  "providers": {},
  "layout": { "entrypoint": "@mesh/panel:main" },
  "theme": { "active": "@mesh/default-theme", "mode": "dark" }
  }
}
"#;
        let manifest = RootPackageManifest::from_json_str(content).unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.modules_dir, "modules");
        assert_eq!(
            manifest.layout.unwrap().entrypoint.as_str(),
            "@mesh/panel:main"
        );
    }

    #[test]
    fn module_root_manifest_accepts_legacy_top_level_shape() {
        let content = r#"
{
  "schemaVersion": 1,
  "modulesDir": "modules",
  "modules": {},
  "providers": {},
  "layout": { "entrypoint": "@mesh/panel:main" }
}
"#;
        let manifest = RootPackageManifest::from_json_str(content).unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.modules_dir, "modules");
        assert_eq!(
            manifest.layout.unwrap().entrypoint.as_str(),
            "@mesh/panel:main"
        );
    }

    #[test]
    fn module_package_manifest_parses_backend_package_json() {
        let content = r#"
{
  "name": "@mesh/pipewire-audio",
  "version": "0.1.0",
  "repository": {
    "type": "git",
    "url": "git+https://example.invalid/pipewire-audio.git"
  },
  "mesh": {
    "apiVersion": "0.1",
    "kind": "backend",
    "capabilities": { "required": ["exec.wpctl"] },
    "i18n": { "defaultLocale": "en", "supportedLocales": ["en", "sk"] },
    "dependencies": {
      "binaries": [{ "name": "wpctl", "reason": "PipeWire control" }]
    },
    "entrypoints": { "main": "src/main.luau" },
    "implements": [
      { "interface": "mesh.audio", "version": "1.0", "basePlugin": "@mesh/audio-interface", "provider": "pipewire", "label": "PipeWire", "priority": 100 }
    ]
  }
}
"#;
        let manifest = ModulePackageManifest::from_json_str(content).unwrap();
        assert_eq!(manifest.name, "@mesh/pipewire-audio");
        assert_eq!(manifest.mesh.kind, ModuleKind::Backend);
        assert_eq!(
            manifest.mesh.entrypoints.main.as_deref(),
            Some("src/main.luau")
        );
        assert_eq!(
            manifest.repository.unwrap().url,
            "git+https://example.invalid/pipewire-audio.git"
        );
        assert_eq!(manifest.mesh.capabilities.required, vec!["exec.wpctl"]);
        assert_eq!(manifest.mesh.dependencies.binaries[0].name, "wpctl");
        assert_eq!(manifest.mesh.i18n.default_locale.as_deref(), Some("en"));
        assert_eq!(manifest.mesh.i18n.supported_locales, vec!["en", "sk"]);
        assert_eq!(
            manifest.mesh.implements[0].base_plugin.as_deref(),
            Some("@mesh/audio-interface")
        );
    }

    #[test]
    fn module_package_manifest_parses_interface_relationship_metadata() {
        let content = r#"
{
  "name": "@alice/audio-streams-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "interface": {
      "name": "alice.audio-streams",
      "version": "1.0",
      "file": "interface.toml",
      "domain": "audio",
      "extends": "mesh.audio",
      "relationship": "extension"
    }
  }
}
"#;
        let manifest = ModulePackageManifest::from_json_str(content).unwrap();
        let interface = manifest.mesh.interface.unwrap();
        assert_eq!(interface.name, "alice.audio-streams");
        assert_eq!(interface.domain.as_deref(), Some("audio"));
        assert_eq!(interface.extends.as_deref(), Some("mesh.audio"));
        assert_eq!(
            interface.relationship,
            Some(InterfaceRelationship::Extension)
        );
    }

    #[test]
    fn module_package_manifest_rejects_empty_git_origin_url() {
        let content = r#"
{
  "name": "@mesh/bad",
  "version": "0.1.0",
  "repository": { "type": "git", "url": "" },
  "mesh": { "apiVersion": "0.1", "kind": "backend" }
}
"#;
        assert!(ModulePackageManifest::from_json_str(content).is_err());
    }

    #[test]
    fn module_manifest_loader_prefers_package_json_over_plugin_json() {
        let dir = temp_dir("module-precedence");
        fs::write(
            dir.join("package.json"),
            r#"{"name":"@mesh/package","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend"}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("plugin.json"),
            r#"{"id":"@mesh/plugin","version":"0.1.0","type":"surface","api_version":"0.1"}"#,
        )
        .unwrap();
        let loaded = load_module_manifest(&dir).unwrap();
        assert_eq!(loaded.source, ModuleManifestSource::PackageJson);
        assert_eq!(loaded.manifest.name, "@mesh/package");
    }

    #[test]
    fn module_manifest_loader_accepts_legacy_plugin_json() {
        let dir = temp_dir("legacy-plugin");
        fs::write(
            dir.join("plugin.json"),
            r#"{"id":"@mesh/plugin","version":"0.1.0","type":"surface","api_version":"0.1","entrypoints":{"main":"src/main.mesh"}}"#,
        )
        .unwrap();
        let loaded = load_module_manifest(&dir).unwrap();
        assert_eq!(loaded.source, ModuleManifestSource::LegacyPluginJson);
        assert_eq!(loaded.manifest.name, "@mesh/plugin");
    }

    #[test]
    fn module_manifest_loader_preserves_legacy_navigation_bar_entrypoint() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/frontend/navigation-bar");
        let loaded = load_module_manifest(&dir).unwrap();
        assert_eq!(loaded.source, ModuleManifestSource::LegacyPluginJson);
        assert_eq!(loaded.manifest.name, "@mesh/navigation-bar");
        assert_eq!(
            loaded.manifest.mesh.entrypoints.main.as_deref(),
            Some("src/main.mesh")
        );
    }

    #[test]
    fn installed_module_graph_loads_repo_package_fixture() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let graph =
            load_installed_module_graph(&workspace_root.join("config/package.json")).unwrap();

        assert_eq!(graph.frontend_modules().len(), 1);
        assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
        assert_eq!(
            graph.active_provider("mesh.audio").unwrap().module_id,
            "@mesh/pipewire-audio"
        );
        let layout = graph.layout_entrypoint().unwrap();
        assert_eq!(layout.module_id, "@mesh/navigation-bar");
        assert_eq!(layout.entrypoint_id, "main");
    }

    fn loaded_module(
        name: &str,
        kind: ModuleKind,
        dependencies: MeshDependencies,
        provides: Vec<MeshProvidesDeclaration>,
        contributes: MeshContributes,
    ) -> LoadedModuleManifest {
        LoadedModuleManifest {
            manifest: ModulePackageManifest {
                name: name.into(),
                version: "0.1.0".into(),
                description: None,
                license: None,
                repository: None,
                mesh: MeshModuleSection {
                    api_version: "0.1".into(),
                    kind,
                    capabilities: CapabilitiesSection::default(),
                    i18n: MeshI18nSupport::default(),
                    entrypoints: MeshEntrypoints::default(),
                    dependencies,
                    provides,
                    implements: Vec::new(),
                    interface: None,
                    contributes,
                    experimental: serde_json::Value::Null,
                },
            },
            path: PathBuf::from(format!("{name}/package.json")),
            source: ModuleManifestSource::PackageJson,
        }
    }

    fn root_with_modules(
        modules: &[(&str, ModuleKind)],
        providers: &[(&str, &str)],
        layout: Option<&str>,
    ) -> RootPackageManifest {
        RootPackageManifest {
            schema_version: 1,
            modules_dir: "modules".into(),
            modules: modules
                .iter()
                .map(|(id, kind)| {
                    (
                        (*id).into(),
                        InstalledModuleEntry {
                            kind: *kind,
                            path: format!("modules/{id}"),
                            enabled: true,
                        },
                    )
                })
                .collect(),
            providers: providers
                .iter()
                .map(|(interface, module_id)| ((*interface).into(), (*module_id).into()))
                .collect(),
            layout: layout.map(|entrypoint| RootLayoutSelection {
                entrypoint: entrypoint.into(),
            }),
            theme: None,
        }
    }

    #[test]
    fn installed_module_graph_exposes_kind_views_from_single_modules_map() {
        let root = root_with_modules(
            &[
                ("@mesh/front", ModuleKind::Frontend),
                ("@mesh/back", ModuleKind::Backend),
                ("@mesh/theme", ModuleKind::Theme),
                ("@mesh/icons", ModuleKind::IconPack),
                ("@mesh/fonts", ModuleKind::FontPack),
                ("@mesh/lang-en", ModuleKind::LanguagePack),
                ("@mesh/backend-kit", ModuleKind::Library),
            ],
            &[],
            None,
        );
        let modules = vec![
            loaded_module(
                "@mesh/front",
                ModuleKind::Frontend,
                MeshDependencies::default(),
                vec![],
                MeshContributes::default(),
            ),
            loaded_module(
                "@mesh/back",
                ModuleKind::Backend,
                MeshDependencies::default(),
                vec![],
                MeshContributes::default(),
            ),
            loaded_module(
                "@mesh/theme",
                ModuleKind::Theme,
                MeshDependencies::default(),
                vec![],
                MeshContributes::default(),
            ),
            loaded_module(
                "@mesh/icons",
                ModuleKind::IconPack,
                MeshDependencies::default(),
                vec![],
                MeshContributes::default(),
            ),
            loaded_module(
                "@mesh/fonts",
                ModuleKind::FontPack,
                MeshDependencies::default(),
                vec![],
                MeshContributes::default(),
            ),
            loaded_module(
                "@mesh/lang-en",
                ModuleKind::LanguagePack,
                MeshDependencies::default(),
                vec![],
                MeshContributes::default(),
            ),
            loaded_module(
                "@mesh/backend-kit",
                ModuleKind::Library,
                MeshDependencies::default(),
                vec![],
                MeshContributes::default(),
            ),
        ];

        let graph = InstalledModuleGraph::from_parts(root, modules).unwrap();
        assert_eq!(graph.frontend_modules().len(), 1);
        assert_eq!(graph.backend_modules().len(), 1);
        assert_eq!(graph.theme_modules().len(), 1);
        assert_eq!(graph.icon_modules().len(), 1);
        assert_eq!(graph.font_modules().len(), 1);
        assert_eq!(graph.language_modules().len(), 1);
        assert_eq!(graph.library_modules().len(), 1);
    }

    #[test]
    fn installed_module_graph_rejects_root_module_without_loaded_package() {
        let root = root_with_modules(&[("@mesh/missing", ModuleKind::Frontend)], &[], None);
        assert!(InstalledModuleGraph::from_parts(root, vec![]).is_err());
    }

    fn audio_modules() -> Vec<LoadedModuleManifest> {
        vec![
            loaded_module(
                "@mesh/pipewire-audio",
                ModuleKind::Backend,
                MeshDependencies::default(),
                vec![MeshProvidesDeclaration {
                    interface: "mesh.audio".into(),
                    version: None,
                    base_plugin: None,
                    provider: Some("pipewire".into()),
                    label: Some("PipeWire".into()),
                    priority: 100,
                }],
                MeshContributes::default(),
            ),
            loaded_module(
                "@mesh/pulseaudio-audio",
                ModuleKind::Backend,
                MeshDependencies::default(),
                vec![MeshProvidesDeclaration {
                    interface: "mesh.audio".into(),
                    version: None,
                    base_plugin: None,
                    provider: Some("pulseaudio".into()),
                    label: Some("PulseAudio".into()),
                    priority: 50,
                }],
                MeshContributes::default(),
            ),
        ]
    }

    fn interface_module(
        module_id: &str,
        name: &str,
        domain: &str,
        relationship: InterfaceRelationship,
        extends: Option<&str>,
    ) -> LoadedModuleManifest {
        let mut module = loaded_module(
            module_id,
            ModuleKind::Interface,
            MeshDependencies::default(),
            Vec::new(),
            MeshContributes::default(),
        );
        module.manifest.mesh.interface = Some(MeshInterfaceDeclaration {
            name: name.into(),
            version: Some("1.0".into()),
            file: Some("interface.toml".into()),
            domain: Some(domain.into()),
            extends: extends.map(str::to_string),
            relationship: Some(relationship),
            reason: None,
        });
        module
    }

    #[test]
    fn installed_module_graph_exposes_frontend_backend_requirements() {
        let mut deps = MeshDependencies::default();
        deps.backend.insert("mesh.audio".into(), ">=1.0.0".into());
        deps.backend.insert("mesh.network".into(), ">=1.0.0".into());
        deps.backend.insert("mesh.power".into(), ">=1.0.0".into());
        let mut modules = audio_modules();
        modules.push(loaded_module(
            "@mesh/quick-settings",
            ModuleKind::Frontend,
            deps,
            vec![],
            MeshContributes::default(),
        ));
        let root = root_with_modules(
            &[
                ("@mesh/quick-settings", ModuleKind::Frontend),
                ("@mesh/pipewire-audio", ModuleKind::Backend),
                ("@mesh/pulseaudio-audio", ModuleKind::Backend),
            ],
            &[("mesh.audio", "@mesh/pipewire-audio")],
            None,
        );

        let graph = InstalledModuleGraph::from_parts(root, modules).unwrap();
        let requirements = graph
            .requirements_for_frontend("@mesh/quick-settings")
            .unwrap();
        assert!(requirements.backend.contains_key("mesh.audio"));
        assert!(requirements.backend.contains_key("mesh.network"));
        assert!(requirements.backend.contains_key("mesh.power"));
    }

    #[test]
    fn installed_module_graph_keeps_multiple_audio_providers() {
        let root = root_with_modules(
            &[
                ("@mesh/pipewire-audio", ModuleKind::Backend),
                ("@mesh/pulseaudio-audio", ModuleKind::Backend),
            ],
            &[],
            None,
        );
        let graph = InstalledModuleGraph::from_parts(root, audio_modules()).unwrap();
        assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
    }

    #[test]
    fn installed_module_graph_records_interface_extension_guidance() {
        let root = root_with_modules(
            &[
                ("@mesh/audio-interface", ModuleKind::Interface),
                ("@alice/audio-mixer-interface", ModuleKind::Interface),
            ],
            &[],
            None,
        );
        let graph = InstalledModuleGraph::from_parts(
            root,
            vec![
                interface_module(
                    "@mesh/audio-interface",
                    "mesh.audio",
                    "audio",
                    InterfaceRelationship::Base,
                    None,
                ),
                interface_module(
                    "@alice/audio-mixer-interface",
                    "alice.audio-mixer",
                    "audio",
                    InterfaceRelationship::Independent,
                    None,
                ),
            ],
        )
        .unwrap();

        let guidance = graph.interface_guidance();
        assert_eq!(guidance.len(), 1);
        assert_eq!(guidance[0].interface, "alice.audio-mixer");
        assert_eq!(guidance[0].recommended_base, "mesh.audio");
    }

    #[test]
    fn installed_module_graph_does_not_warn_for_declared_interface_extension() {
        let root = root_with_modules(
            &[
                ("@mesh/audio-interface", ModuleKind::Interface),
                ("@alice/audio-streams-interface", ModuleKind::Interface),
            ],
            &[],
            None,
        );
        let graph = InstalledModuleGraph::from_parts(
            root,
            vec![
                interface_module(
                    "@mesh/audio-interface",
                    "mesh.audio",
                    "audio",
                    InterfaceRelationship::Base,
                    None,
                ),
                interface_module(
                    "@alice/audio-streams-interface",
                    "alice.audio-streams",
                    "audio",
                    InterfaceRelationship::Extension,
                    Some("mesh.audio"),
                ),
            ],
        )
        .unwrap();

        assert!(graph.interface_guidance().is_empty());
        assert_eq!(
            graph
                .declared_interface("alice.audio-streams")
                .unwrap()
                .extends
                .as_deref(),
            Some("mesh.audio")
        );
    }

    #[test]
    fn installed_module_graph_returns_explicit_active_provider() {
        let root = root_with_modules(
            &[
                ("@mesh/pipewire-audio", ModuleKind::Backend),
                ("@mesh/pulseaudio-audio", ModuleKind::Backend),
            ],
            &[("mesh.audio", "@mesh/pipewire-audio")],
            None,
        );
        let graph = InstalledModuleGraph::from_parts(root, audio_modules()).unwrap();
        assert_eq!(
            graph.active_provider("mesh.audio").unwrap().module_id,
            "@mesh/pipewire-audio"
        );
    }

    #[test]
    fn installed_module_graph_rejects_unknown_active_provider() {
        let root = root_with_modules(
            &[("@mesh/pipewire-audio", ModuleKind::Backend)],
            &[("mesh.audio", "@mesh/missing")],
            None,
        );
        let modules = vec![audio_modules().remove(0)];
        assert!(InstalledModuleGraph::from_parts(root, modules).is_err());
    }

    #[test]
    fn installed_module_graph_rejects_active_provider_interface_mismatch() {
        let root = root_with_modules(
            &[("@mesh/network", ModuleKind::Backend)],
            &[("mesh.audio", "@mesh/network")],
            None,
        );
        let network = loaded_module(
            "@mesh/network",
            ModuleKind::Backend,
            MeshDependencies::default(),
            vec![MeshProvidesDeclaration {
                interface: "mesh.network".into(),
                version: None,
                base_plugin: None,
                provider: Some("networkmanager".into()),
                label: Some("NetworkManager".into()),
                priority: 100,
            }],
            MeshContributes::default(),
        );
        assert!(InstalledModuleGraph::from_parts(root, vec![network]).is_err());
    }

    #[test]
    fn installed_module_graph_resolves_layout_entrypoint() {
        let contributes = MeshContributes {
            layout: vec![LayoutContribution {
                id: "main".into(),
                entrypoint: "src/main.mesh".into(),
                label: None,
            }],
            ..MeshContributes::default()
        };
        let root = root_with_modules(
            &[("@mesh/panel", ModuleKind::Frontend)],
            &[],
            Some("@mesh/panel:main"),
        );
        let graph = InstalledModuleGraph::from_parts(
            root,
            vec![loaded_module(
                "@mesh/panel",
                ModuleKind::Frontend,
                MeshDependencies::default(),
                vec![],
                contributes,
            )],
        )
        .unwrap();
        let entrypoint = graph.layout_entrypoint().unwrap();
        assert_eq!(entrypoint.module_id, "@mesh/panel");
        assert_eq!(entrypoint.entrypoint_id, "main");
        assert_eq!(entrypoint.path, "src/main.mesh");
    }

    #[test]
    fn installed_module_graph_indexes_theme_icon_font_i18n_contributions() {
        let mut modes = HashMap::new();
        modes.insert("dark".into(), "themes/dark.json".into());
        let contributes = MeshContributes {
            themes: vec![ThemeContribution {
                id: "mesh-default".into(),
                label: "MESH Default".into(),
                modes,
                default_mode: Some("dark".into()),
            }],
            icons: vec![PathContribution {
                id: "material".into(),
                path: "icons".into(),
                label: None,
            }],
            fonts: vec![PathContribution {
                id: "inter".into(),
                path: "fonts".into(),
                label: None,
            }],
            i18n: vec![I18nContribution {
                id: "en".into(),
                locale: "en".into(),
                path: "i18n/en.json".into(),
            }],
            ..MeshContributes::default()
        };
        let root = root_with_modules(&[("@mesh/resources", ModuleKind::Theme)], &[], None);
        let graph = InstalledModuleGraph::from_parts(
            root,
            vec![loaded_module(
                "@mesh/resources",
                ModuleKind::Theme,
                MeshDependencies::default(),
                vec![],
                contributes,
            )],
        )
        .unwrap();
        assert_eq!(graph.contributed_themes().len(), 1);
        assert_eq!(graph.contributed_icons().len(), 1);
        assert_eq!(graph.contributed_fonts().len(), 1);
        assert_eq!(graph.contributed_i18n().len(), 1);
    }

    #[test]
    fn installed_module_graph_indexes_library_contributions() {
        let contributes = MeshContributes {
            libraries: vec![LibraryContribution {
                namespace: "@mesh/backend-kit".into(),
                path: "lib".into(),
            }],
            ..MeshContributes::default()
        };
        let root = root_with_modules(&[("@mesh/backend-kit", ModuleKind::Library)], &[], None);
        let graph = InstalledModuleGraph::from_parts(
            root,
            vec![loaded_module(
                "@mesh/backend-kit",
                ModuleKind::Library,
                MeshDependencies::default(),
                vec![],
                contributes,
            )],
        )
        .unwrap();

        assert_eq!(graph.library_modules().len(), 1);
        assert_eq!(graph.contributed_libraries().len(), 1);
        assert_eq!(
            graph.contributed_libraries()[0],
            ContributedLibrary {
                module_id: "@mesh/backend-kit".into(),
                namespace: "@mesh/backend-kit".into(),
                path: "lib".into(),
            }
        );
    }

    #[test]
    fn installed_module_graph_rejects_library_path_escape() {
        let contributes = MeshContributes {
            libraries: vec![LibraryContribution {
                namespace: "@mesh/backend-kit".into(),
                path: "../lib".into(),
            }],
            ..MeshContributes::default()
        };
        let root = root_with_modules(&[("@mesh/backend-kit", ModuleKind::Library)], &[], None);
        let result = InstalledModuleGraph::from_parts(
            root,
            vec![loaded_module(
                "@mesh/backend-kit",
                ModuleKind::Library,
                MeshDependencies::default(),
                vec![],
                contributes,
            )],
        );

        assert!(result.is_err());
    }

    #[test]
    fn installed_module_graph_rejects_contribution_path_escape() {
        let contributes = MeshContributes {
            icons: vec![PathContribution {
                id: "bad".into(),
                path: "../outside.json".into(),
                label: None,
            }],
            ..MeshContributes::default()
        };
        let root = root_with_modules(&[("@mesh/icons", ModuleKind::IconPack)], &[], None);
        assert!(
            InstalledModuleGraph::from_parts(
                root,
                vec![loaded_module(
                    "@mesh/icons",
                    ModuleKind::IconPack,
                    MeshDependencies::default(),
                    vec![],
                    contributes,
                )]
            )
            .is_err()
        );
    }
}
