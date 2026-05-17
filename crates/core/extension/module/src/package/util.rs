use super::ModuleManifestError;
use crate::manifest::DependencySpec;
use std::path::{Component, Path};

pub(crate) fn default_modules_dir() -> String {
    "modules".into()
}

pub(crate) fn default_schema_version() -> u32 {
    1
}

pub(crate) fn default_enabled() -> bool {
    true
}

pub(crate) fn validate_modules_dir(value: &str) -> Result<(), ModuleManifestError> {
    let path = Path::new(value);
    if value.trim().is_empty() {
        return Err(ModuleManifestError::Validation(
            "modulesDir cannot be empty".into(),
        ));
    }
    if path.is_absolute() {
        return Err(ModuleManifestError::Validation(format!(
            "modulesDir must be a relative path: {value}"
        )));
    }
    Ok(())
}

pub(crate) fn validate_relative_path(label: &str, value: &str) -> Result<(), ModuleManifestError> {
    let path = Path::new(value);
    if value.trim().is_empty() {
        return Err(ModuleManifestError::Validation(format!(
            "{label} cannot be empty"
        )));
    }
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ModuleManifestError::Validation(format!(
            "{label} must be a relative path without '..': {value}"
        )));
    }
    Ok(())
}

pub(crate) fn parse_module_entrypoint(value: &str) -> Option<(&str, &str)> {
    let (module_id, entrypoint_id) = value.rsplit_once(':')?;
    if module_id.trim().is_empty() || entrypoint_id.trim().is_empty() {
        return None;
    }
    Some((module_id, entrypoint_id))
}

pub(crate) fn dependency_spec_to_string(spec: &DependencySpec) -> String {
    match spec {
        DependencySpec::Simple(value) => value.clone(),
        DependencySpec::Detailed { version, .. } => version.clone(),
    }
}
