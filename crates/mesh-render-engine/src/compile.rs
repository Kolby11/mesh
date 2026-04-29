use crate::CompiledFrontendPlugin;

use mesh_component::{ComponentFile, ComponentImportTarget, parse_component};
use mesh_plugin::{Manifest, PluginType};

use std::collections::{HashMap, HashSet};
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

    #[error("component import alias '{alias}' is declared with multiple targets")]
    ConflictingImportAlias { alias: String },
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
    let component = parse_component_file(&source_path)?;
    let mut local_components: HashMap<String, ComponentFile> = HashMap::new();
    let mut plugin_component_imports = HashMap::new();
    let mut seen_local_paths = HashSet::new();
    collect_imports(
        &component,
        &source_path,
        plugin_dir,
        &mut local_components,
        &mut plugin_component_imports,
        &mut seen_local_paths,
    )?;

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
        plugin_component_imports,
    })
}

fn parse_component_file(path: &Path) -> Result<ComponentFile, CompileFrontendError> {
    let source =
        std::fs::read_to_string(path).map_err(|source| CompileFrontendError::ReadSource {
            path: path.to_path_buf(),
            source,
        })?;
    parse_component(&source).map_err(|source| CompileFrontendError::ParseSource {
        path: path.to_path_buf(),
        source,
    })
}

fn collect_imports(
    component: &ComponentFile,
    component_path: &Path,
    plugin_dir: &Path,
    local_components: &mut HashMap<String, ComponentFile>,
    plugin_component_imports: &mut HashMap<String, String>,
    seen_local_paths: &mut HashSet<PathBuf>,
) -> Result<(), CompileFrontendError> {
    for import in &component.imports {
        match &import.target {
            ComponentImportTarget::ComponentLocal(source) => {
                let target_path = resolve_local_component_path(source, component_path, plugin_dir);
                let parsed = parse_component_file(&target_path)?;
                insert_local_component(
                    &import.alias,
                    target_path.clone(),
                    parsed.clone(),
                    local_components,
                )?;
                let canonical = target_path.canonicalize().unwrap_or(target_path.clone());
                if seen_local_paths.insert(canonical) {
                    collect_imports(
                        &parsed,
                        &target_path,
                        plugin_dir,
                        local_components,
                        plugin_component_imports,
                        seen_local_paths,
                    )?;
                }
            }
            ComponentImportTarget::ComponentPlugin(plugin_id) => {
                insert_plugin_component_import(&import.alias, plugin_id, plugin_component_imports)?;
            }
            ComponentImportTarget::InterfaceApi { .. } => {}
        }
    }
    Ok(())
}

fn insert_local_component(
    alias: &str,
    path: PathBuf,
    component: ComponentFile,
    local_components: &mut HashMap<String, ComponentFile>,
) -> Result<(), CompileFrontendError> {
    local_components.insert(alias.to_string(), component);
    tracing::debug!(
        "registered local component import {alias} from {}",
        path.display()
    );
    Ok(())
}

fn insert_plugin_component_import(
    alias: &str,
    plugin_id: &str,
    plugin_component_imports: &mut HashMap<String, String>,
) -> Result<(), CompileFrontendError> {
    if let Some(existing) = plugin_component_imports.get(alias) {
        if existing != plugin_id {
            return Err(CompileFrontendError::ConflictingImportAlias {
                alias: alias.to_string(),
            });
        }
    }
    plugin_component_imports.insert(alias.to_string(), plugin_id.to_string());
    Ok(())
}

fn resolve_local_component_path(source: &str, component_path: &Path, plugin_dir: &Path) -> PathBuf {
    let mut path = if let Some(rest) = source.strip_prefix("@src/") {
        plugin_dir.join("src").join(rest)
    } else if source.starts_with('/') {
        PathBuf::from(source)
    } else {
        component_path.parent().unwrap_or(plugin_dir).join(source)
    };
    if path.extension().is_none() {
        path.set_extension("mesh");
    }
    path
}
