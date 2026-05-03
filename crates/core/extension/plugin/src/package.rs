use crate::manifest::{self, DependencySpec, Manifest, ManifestSource, PluginType};
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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
        if self.schema_version != 1 {
            return Err(PackageManifestError::Validation(format!(
                "unsupported schemaVersion {}; supported version is 1",
                self.schema_version
            )));
        }
        validate_relative_path("modulesDir", &self.modules_dir)?;
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
                entrypoints: MeshEntrypoints {
                    main: manifest.entrypoints.main,
                    settings_ui: manifest.entrypoints.settings_ui,
                },
                dependencies,
                provides,
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
    pub entrypoints: MeshEntrypoints,
    #[serde(default)]
    pub dependencies: MeshDependencies,
    #[serde(default)]
    pub provides: Vec<MeshProvidesDeclaration>,
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
        self.contributes.validate()
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
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MeshProvidesDeclaration {
    pub interface: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub priority: u32,
}

impl From<crate::manifest::ProvidedInterface> for MeshProvidesDeclaration {
    fn from(provided: crate::manifest::ProvidedInterface) -> Self {
        Self {
            interface: provided.interface,
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

pub fn load_module_manifest(
    module_dir: &Path,
) -> Result<LoadedModuleManifest, PackageManifestError> {
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

fn default_enabled() -> bool {
    true
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
  "schemaVersion": 1,
  "modulesDir": "modules",
  "modules": {},
  "providers": {},
  "layout": { "entrypoint": "@mesh/panel:main" },
  "theme": { "active": "@mesh/default-theme", "mode": "dark" }
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
    "entrypoints": { "main": "src/main.luau" },
    "provides": [
      { "interface": "mesh.audio", "provider": "pipewire", "label": "PipeWire", "priority": 100 }
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
    fn module_manifest_loader_preserves_legacy_panel_entrypoint() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../packages/plugins/frontend/core/panel");
        let loaded = load_module_manifest(&dir).unwrap();
        assert_eq!(loaded.source, ModuleManifestSource::LegacyPluginJson);
        assert_eq!(loaded.manifest.name, "@mesh/panel");
        assert_eq!(
            loaded.manifest.mesh.entrypoints.main.as_deref(),
            Some("src/main.mesh")
        );
    }
}
