use crate::CompiledFrontendPlugin;

use mesh_component::parse_component;
use mesh_plugin::{Manifest, PluginType};

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum CompileFrontendError {
    #[error("plugin '{plugin_id}' is not a frontend plugin")]
    NotFrontendPlugin { plugin_id: String },

    #[error("plugin '{plugin_id}' is missing a .mesh frontend entrypoint")]
    MissingMeshEntrypoint { plugin_id: String },

    #[error("failed to read component source {path}: {source}")]
    ReadSource {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse component source {path}: {source}")]
    ParseSource {
        path: PathBuf,
        #[source]
        source: mesh_component::ParseError,
    },
}

pub fn is_frontend_plugin(manifest: &Manifest) -> bool {
    matches!(
        manifest.package.plugin_type,
        PluginType::Surface | PluginType::Widget
    )
}

pub fn compile_frontend_plugin(
    manifest: &Manifest,
    plugin_dir: &Path,
) -> Result<CompiledFrontendPlugin, CompileFrontendError> {
    if !is_frontend_plugin(manifest) {
        return Err(CompileFrontendError::NotFrontendPlugin {
            plugin_id: manifest.package.id.clone(),
        });
    }

    let entrypoint = manifest
        .entrypoints
        .main
        .as_deref()
        .filter(|path| path.ends_with(".mesh"))
        .ok_or_else(|| CompileFrontendError::MissingMeshEntrypoint {
            plugin_id: manifest.package.id.clone(),
        })?;

    let source_path = plugin_dir.join(entrypoint);
    let source = std::fs::read_to_string(&source_path).map_err(|source| {
        CompileFrontendError::ReadSource {
            path: source_path.clone(),
            source,
        }
    })?;
    let mut component =
        parse_component(&source).map_err(|source| CompileFrontendError::ParseSource {
            path: source_path.clone(),
            source,
        })?;

    let mut local_components: std::collections::HashMap<String, mesh_component::ComponentFile> =
        std::collections::HashMap::new();
    let components_dir = plugin_dir.join("src").join("components");
    if components_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&components_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("mesh") {
                    if let Ok(src) = std::fs::read_to_string(&path) {
                        match parse_component(&src) {
                            Ok(comp) => {
                                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                    local_components.insert(stem.to_string(), comp.clone());
                                    let pascal = stem
                                        .split('-')
                                        .filter(|part| !part.is_empty())
                                        .map(|part| {
                                            let mut chars = part.chars();
                                            match chars.next() {
                                                Some(first) => {
                                                    first.to_ascii_uppercase().to_string()
                                                        + chars.as_str()
                                                }
                                                None => String::new(),
                                            }
                                        })
                                        .collect::<String>();
                                    if !pascal.is_empty() {
                                        local_components.insert(pascal.clone(), comp.clone());
                                    }

                                    component
                                        .imports
                                        .entry(stem.to_string())
                                        .or_insert_with(|| manifest.package.id.clone());
                                    if !pascal.is_empty() {
                                        component
                                            .imports
                                            .entry(pascal)
                                            .or_insert_with(|| manifest.package.id.clone());
                                    }
                                }
                            }
                            Err(err) => {
                                tracing::warn!(
                                    "plugin '{}': failed to parse local component {}: {}",
                                    manifest.package.id,
                                    path.display(),
                                    err
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    let mut imports_to_fix = Vec::new();
    for (alias, target) in component.imports.iter() {
        if target.starts_with('@') {
            imports_to_fix.push(alias.clone());
        }
    }

    for alias in imports_to_fix {
        if let Some(target) = component.imports.get(&alias) {
            let rel = target.trim_start_matches('@');
            let mut candidate = plugin_dir.join(rel);
            if candidate.extension().is_none() {
                candidate.set_extension("mesh");
            }
            if let Ok(src) = std::fs::read_to_string(&candidate) {
                match parse_component(&src) {
                    Ok(comp) => {
                        local_components.insert(alias.clone(), comp);
                    }
                    Err(err) => tracing::warn!(
                        "plugin '{}': failed to parse imported local component {}: {}",
                        manifest.package.id,
                        candidate.display(),
                        err
                    ),
                }
            }
        }
    }

    for value in component.imports.values_mut() {
        if value.starts_with('@') {
            *value = manifest.package.id.clone();
        }
    }

    tracing::info!(
        "compiled frontend plugin '{}' from {}",
        manifest.package.id,
        source_path.display()
    );

    Ok(CompiledFrontendPlugin {
        manifest: manifest.clone(),
        source_path,
        component,
        local_components,
    })
}
