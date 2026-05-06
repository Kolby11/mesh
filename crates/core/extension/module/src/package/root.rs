use super::{
    PackageManifestError, default_enabled, default_modules_dir, default_schema_version,
    parse_module_entrypoint, validate_modules_dir, validate_relative_path,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    pub kind: super::ModuleKind,
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
