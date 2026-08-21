use super::{Manifest, ManifestSource};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct LoadedManifest {
    pub manifest: Manifest,
    pub path: PathBuf,
    pub source: ManifestSource,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error(transparent)]
    Canonical(#[from] crate::package::ModuleManifestError),
}

/// Load the normalized runtime view from the canonical package manifest.
///
/// This adapter exists for older runtime consumers that still use `Manifest`.
/// It deliberately delegates to `package::load_module_manifest`, so legacy
/// `package.json`, `mesh.toml`, and old-shaped `module.json` inputs receive the
/// package loader's explicit migration diagnostic and can never be converted
/// into a runnable manifest.
pub fn load_canonical_manifest(module_dir: &Path) -> Result<LoadedManifest, ManifestError> {
    let loaded = crate::package::load_module_manifest(module_dir)?;
    Ok(LoadedManifest {
        manifest: loaded.manifest.into_runtime_manifest(),
        path: loaded.path,
        source: ManifestSource::CanonicalModuleJson,
    })
}

/// A JSON manifest is canonical when it carries both a `name` and a `mesh`
/// section. The package loader uses this structural check before invoking the
/// typed canonical parser so legacy-shaped `module.json` files can receive a
/// migration diagnostic instead of being interpreted as compatibility input.
pub(crate) fn is_canonical_module_json(content: &str) -> Result<bool, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(content)?;
    Ok(value.get("name").is_some() && value.get("mesh").is_some())
}
