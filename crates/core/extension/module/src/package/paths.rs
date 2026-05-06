use super::PackageManifestError;
use std::path::PathBuf;

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
