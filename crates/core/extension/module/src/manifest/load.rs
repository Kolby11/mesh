use super::{Manifest, ManifestSource, json::JsonManifest, toml::TomlManifest};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LoadedManifest {
    pub manifest: Manifest,
    pub path: PathBuf,
    pub source: ManifestSource,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("failed to read manifest: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse mesh.toml manifest: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("failed to parse package.json manifest: {0}")]
    Json(#[from] serde_json::Error),

    #[error("no manifest found in module directory {0}")]
    NotFound(PathBuf),
}

pub fn load_manifest(module_dir: &Path) -> Result<LoadedManifest, ManifestError> {
    let package_json_path = module_dir.join("package.json");
    if package_json_path.exists() {
        return load_package_json(&package_json_path);
    }

    let module_json_path = module_dir.join("module.json");
    if module_json_path.exists() {
        return load_module_json(&module_json_path);
    }

    let mesh_toml_path = module_dir.join("mesh.toml");
    if mesh_toml_path.exists() {
        return load_mesh_toml(&mesh_toml_path);
    }

    Err(ManifestError::NotFound(module_dir.to_path_buf()))
}

fn load_package_json(path: &Path) -> Result<LoadedManifest, ManifestError> {
    let content = std::fs::read_to_string(path)?;
    let parsed: crate::package::ModulePackageManifest = serde_json::from_str(&content)?;

    Ok(LoadedManifest {
        manifest: parsed.into_runtime_manifest(),
        path: path.to_path_buf(),
        source: ManifestSource::PackageJson,
    })
}

fn load_module_json(path: &Path) -> Result<LoadedManifest, ManifestError> {
    let content = std::fs::read_to_string(path)?;
    let parsed: JsonManifest = serde_json::from_str(&content)?;

    Ok(LoadedManifest {
        manifest: parsed.into_manifest(),
        path: path.to_path_buf(),
        source: ManifestSource::ModuleJson,
    })
}

fn load_mesh_toml(path: &Path) -> Result<LoadedManifest, ManifestError> {
    let content = std::fs::read_to_string(path)?;
    let parsed: TomlManifest = toml::from_str(&content)?;

    Ok(LoadedManifest {
        manifest: parsed.into_manifest(),
        path: path.to_path_buf(),
        source: ManifestSource::MeshToml,
    })
}
