use super::ModuleManifestError;
use std::path::PathBuf;

pub fn mesh_home() -> Result<PathBuf, ModuleManifestError> {
    if let Ok(path) = std::env::var("MESH_HOME") {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(ModuleManifestError::InvalidMeshHome(
                "MESH_HOME cannot be empty".into(),
            ));
        }
        let path = PathBuf::from(trimmed);
        if !path.is_absolute() {
            return Err(ModuleManifestError::InvalidMeshHome(format!(
                "MESH_HOME must be absolute: {}",
                path.display()
            )));
        }
        return Ok(path);
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| ModuleManifestError::InvalidMeshHome("HOME is not set".into()))?;
    Ok(home.join(".mesh"))
}

pub fn root_module_graph_manifest_path() -> Result<PathBuf, ModuleManifestError> {
    Ok(mesh_home()?.join("module.json"))
}

pub fn settings_path() -> Result<PathBuf, ModuleManifestError> {
    Ok(mesh_home()?.join("settings.json"))
}

pub fn modules_dir() -> Result<PathBuf, ModuleManifestError> {
    Ok(mesh_home()?.join("modules"))
}

pub fn themes_dir() -> Result<PathBuf, ModuleManifestError> {
    Ok(mesh_home()?.join("themes"))
}
