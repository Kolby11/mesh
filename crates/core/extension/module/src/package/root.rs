use super::{
    ModuleManifestError, default_enabled, default_modules_dir, default_schema_version,
    parse_module_entrypoint, validate_modules_dir, validate_relative_path,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootModuleGraphManifest {
    pub schema_version: u32,
    #[serde(default = "default_modules_dir")]
    pub modules_dir: String,
    #[serde(default)]
    pub modules: HashMap<String, InstalledModuleEntry>,
    /// Modules to keep disabled when the installed set is auto-discovered from
    /// `modulesDir` (i.e. when `modules` is empty). Decisions, not inventory.
    #[serde(default)]
    pub disabled: Vec<String>,
    #[serde(default)]
    pub providers: HashMap<String, String>,
    /// Explicit capability approvals keyed by module id. Required grants must
    /// be present here before activation; optional grants are denied unless
    /// they are also listed.
    #[serde(default)]
    pub capability_approvals: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub layout: Option<RootLayoutSelection>,
    #[serde(default)]
    pub theme: Option<RootThemeSelection>,
}

impl RootModuleGraphManifest {
    pub fn from_json_str(input: &str) -> Result<Self, ModuleManifestError> {
        let parsed: RootModuleGraphJson =
            serde_json::from_str(input).map_err(|source| ModuleManifestError::Json {
                path: PathBuf::from("<inline>"),
                source,
            })?;
        let manifest = parsed.into_manifest()?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn from_path(path: &Path) -> Result<Self, ModuleManifestError> {
        let content = std::fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let parsed: RootModuleGraphJson =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        let manifest = parsed.into_manifest()?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.schema_version != 1 {
            return Err(ModuleManifestError::Validation(format!(
                "unsupported schemaVersion {}; supported version is 1",
                self.schema_version
            )));
        }
        validate_modules_dir(&self.modules_dir)?;
        for (module_id, entry) in &self.modules {
            if module_id.trim().is_empty() {
                return Err(ModuleManifestError::Validation(
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

    /// Persist the inventory portion of the root graph while preserving the
    /// document's top-level metadata. The loader accepts a canonical
    /// `module.json` envelope, so writing the derived struct directly would
    /// accidentally drop the `mesh` wrapper and any author-owned fields.
    pub fn save(&self, path: &Path) -> Result<(), ModuleManifestError> {
        self.validate()?;
        let mut document = if path.exists() {
            let content = fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            serde_json::from_str::<serde_json::Value>(&content).map_err(|source| {
                ModuleManifestError::Json {
                    path: path.to_path_buf(),
                    source,
                }
            })?
        } else {
            serde_json::json!({})
        };
        let mesh = serde_json::to_value(self).map_err(|source| ModuleManifestError::Json {
            path: path.to_path_buf(),
            source,
        })?;
        let Some(object) = document.as_object_mut() else {
            return Err(ModuleManifestError::Validation(format!(
                "root module graph {} must contain a JSON object",
                path.display()
            )));
        };
        object.insert("mesh".into(), mesh);
        let mut content = serde_json::to_string_pretty(&document).map_err(|source| {
            ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            }
        })?;
        content.push('\n');
        super::profile::atomic_write(path, content.as_bytes())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootModuleGraphJson {
    #[serde(default)]
    mesh: Option<RootMeshSection>,
}

impl RootModuleGraphJson {
    fn into_manifest(self) -> Result<RootModuleGraphManifest, ModuleManifestError> {
        let Some(mesh) = self.mesh else {
            return Err(ModuleManifestError::Validation(
                "root module graph must use canonical name/version/mesh shape".into(),
            ));
        };

        Ok(RootModuleGraphManifest {
            schema_version: mesh.schema_version,
            modules_dir: mesh.modules_dir,
            modules: mesh.modules,
            disabled: mesh.disabled,
            providers: mesh.providers,
            capability_approvals: mesh.capability_approvals,
            layout: mesh.layout,
            theme: mesh.theme,
        })
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
    disabled: Vec<String>,
    #[serde(default)]
    providers: HashMap<String, String>,
    #[serde(
        default,
        rename = "capabilityApprovals",
        alias = "capability_approvals"
    )]
    capability_approvals: BTreeMap<String, Vec<String>>,
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
    fn validate(&self, module_id: &str) -> Result<(), ModuleManifestError> {
        if self.path.trim().is_empty() {
            return Err(ModuleManifestError::Validation(format!(
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
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if parse_module_entrypoint(&self.entrypoint).is_none() {
            return Err(ModuleManifestError::Validation(format!(
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
    fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.active.trim().is_empty() {
            return Err(ModuleManifestError::Validation(
                "theme.active cannot be empty".into(),
            ));
        }
        Ok(())
    }
}
