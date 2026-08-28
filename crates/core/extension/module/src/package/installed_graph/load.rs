use super::super::trust::TrustAssessment;
use super::super::{
    InstalledModuleEntry, MeshLock, ModuleId, ModuleManifest, ModuleManifestDiagnostic,
    ModuleManifestError, ModuleStore, ProfilePaths, RootModuleGraphManifest, ShellProfile,
    TrustTier, contained_path, load_module_signature, module_store_dir, module_tree_digest,
    resolve_composition, validate_module_tree,
};
use super::graph::CompositionContext;
use super::*;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

pub fn load_installed_module_graph(
    root_module_graph_path: &Path,
) -> Result<InstalledModuleGraph, ModuleManifestError> {
    load_installed_module_graph_with(root_module_graph_path, None, load_module_manifests)
}

/// Resolve the installed graph against an explicit candidate profile without
/// changing `active-profile`. Live switching uses this to validate and prepare
/// a complete candidate before committing the pointer.
pub fn load_installed_module_graph_for_profile(
    root_module_graph_path: &Path,
    profile: &ShellProfile,
) -> Result<InstalledModuleGraph, ModuleManifestError> {
    load_installed_module_graph_with(root_module_graph_path, Some(profile), load_module_manifests)
}

fn load_installed_module_graph_with(
    root_module_graph_path: &Path,
    candidate_profile: Option<&ShellProfile>,
    load_manifests: impl Fn(&[PathBuf]) -> Result<Vec<LoadedModuleManifest>, ModuleManifestError>,
) -> Result<InstalledModuleGraph, ModuleManifestError> {
    let mut root = RootModuleGraphManifest::from_path(root_module_graph_path)?;
    let root_dir = root_module_graph_path.parent().ok_or_else(|| {
        ModuleManifestError::Validation(format!(
            "root module graph path must have a parent directory: {}",
            root_module_graph_path.display()
        ))
    })?;
    let lock = MeshLock::load_or_default(&root_dir.join("mesh.lock"))?;
    let modules_dir = root_dir.join(&root.modules_dir);
    let active_store = load_active_store(root_dir)?;
    let mut modules = Vec::new();

    if root.modules.is_empty() {
        // The root graph lists no modules: scan `modulesDir` for `module.json`
        // and build the installed set from each module's own manifest. The root
        // file then holds only decisions — `disabled`, `providers`, `layout`,
        // `theme` — and a discovered module is enabled unless disabled there.
        let module_dirs = if let Some((store, snapshot)) = &active_store {
            snapshot_module_dirs(store, snapshot)?
        } else {
            discover_module_dirs(&modules_dir)
        };
        let loaded_manifests = load_manifests(&module_dirs)?;
        for (module_dir, loaded) in module_dirs.iter().cloned().zip(loaded_manifests) {
            let name = loaded.manifest.name.clone();
            let kind = loaded.manifest.mesh.kind;
            let relative = if active_store.is_some() {
                ModuleId::parse(&name)?.relative_path().to_string()
            } else {
                module_dir
                    .strip_prefix(&modules_dir)
                    .unwrap_or(&module_dir)
                    .to_string_lossy()
                    .replace('\\', "/")
            };
            let enabled = !root.disabled.iter().any(|disabled| disabled == &name);
            root.modules.insert(
                name,
                InstalledModuleEntry {
                    kind,
                    path: relative,
                    enabled,
                },
            );
            modules.push(loaded);
        }
    } else {
        let use_snapshot = active_store.as_ref().is_some_and(|(_, snapshot)| {
            root.modules.keys().all(|module_id| {
                ModuleId::parse(module_id).is_ok() && snapshot.modules.contains_key(module_id)
            })
        });
        let module_dirs = if use_snapshot {
            let (store, snapshot) = active_store.as_ref().expect("snapshot was checked");
            root.modules
                .keys()
                .map(|module_id| {
                    ModuleId::parse(module_id)?;
                    let module = snapshot.modules.get(module_id).ok_or_else(|| {
                        ModuleManifestError::Validation(format!(
                            "active module snapshot is missing {module_id}"
                        ))
                    })?;
                    store.object(&module.digest)
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            root.modules
                .values()
                .map(|entry| contained_path(&modules_dir, &entry.path, "installed module path"))
                .collect::<Result<Vec<_>, _>>()?
        };
        modules = load_manifests(&module_dirs)?;
    }

    // Profiles are opt-in: without an `active-profile` file the root graph's
    // decisions stay authoritative. Once selected, the profile owns composition
    // and the installed directory becomes availability only. Dependencies and
    // sole providers are inferred before enabled contributions are indexed.
    let active_profile;
    let profile = if let Some(profile) = candidate_profile {
        Some(profile)
    } else {
        let profile_paths = ProfilePaths::from_root_graph(root_module_graph_path)?;
        active_profile = profile_paths.load_active()?.map(|(_, profile)| profile);
        active_profile.as_ref()
    };
    // A profile is a composition instance: resolve its `from` chain first, so
    // the composition's roots, bindings, and slot arrangement are in effect
    // before the activation closure runs. A profile with no `from` resolves to
    // itself.
    let mut composition = CompositionContext::default();
    if let Some(profile) = profile {
        let manifests = modules
            .iter()
            .map(|loaded| loaded.manifest.clone())
            .collect::<Vec<_>>();
        let resolved = resolve_composition(profile, manifests.iter())?;
        let effective_profile = resolved.to_profile();
        composition.slots = resolved.spec.slots.clone();
        composition.icon_pack_chain = Some(resolved.spec.resources.icons.clone());
        composition.font_pack_chain = Some(resolved.spec.resources.fonts.clone());
        composition.language_pack_chain = Some(resolved.spec.resources.languages.clone());
        // A node-slot placement is meaningful only while its host root is
        // active. Orphaned overrides remain in the profile for diagnostics,
        // but must not pull their contributed modules into the activation
        // closure or make catalog validation fail.
        composition.node_slots = effective_profile.node_slots.clone();
        composition.orphaned_overrides = resolved.orphaned_overrides.clone();
        effective_profile.apply_to_root(&mut root, &manifests)?;
    }

    let provenance_by_module = modules
        .iter()
        .map(|loaded| {
            let module_id = loaded.manifest.name.clone();
            let (trust, digest, signature) = if let Some(entry) = lock.modules.get(&module_id) {
                (entry.trust, entry.digest.clone(), entry.signature.clone())
            } else {
                let module_root = loaded.path.parent().ok_or_else(|| {
                    ModuleManifestError::Validation(format!(
                        "module {} manifest has no containing directory",
                        loaded.manifest.name
                    ))
                })?;
                let signature = load_module_signature(module_root)?;
                let digest = if signature.is_some() {
                    module_tree_digest(module_root)?
                } else {
                    String::new()
                };
                let trust = if signature.is_some() {
                    TrustTier::Verified
                } else {
                    TrustTier::default_for_module(&module_id)
                };
                (trust, digest, signature)
            };
            let assessment = root.trust_policy.assess(
                &module_id,
                &loaded.manifest.version,
                &digest,
                trust,
                signature.as_ref(),
            );
            Ok::<_, ModuleManifestError>((module_id, assessment))
        })
        .collect::<Result<std::collections::BTreeMap<String, TrustAssessment>, _>>()?;
    InstalledModuleGraph::from_parts_with_provenance(
        root,
        modules,
        composition,
        provenance_by_module,
    )
}

fn load_active_store(
    root_dir: &Path,
) -> Result<Option<(ModuleStore, super::super::ActivationSnapshot)>, ModuleManifestError> {
    let store_root = module_store_dir(root_dir);
    let active = store_root.join("active-generation");
    let metadata = match std::fs::symlink_metadata(&active) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ModuleManifestError::Io {
                path: active,
                source,
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ModuleManifestError::Validation(format!(
            "active module generation {} must be a regular file",
            active.display()
        )));
    }
    let store = ModuleStore::new(store_root)?;
    let snapshot = store.active_snapshot()?.ok_or_else(|| {
        ModuleManifestError::Validation(
            "active module generation exists without an activation snapshot".into(),
        )
    })?;
    Ok(Some((store, snapshot)))
}

fn snapshot_module_dirs(
    store: &ModuleStore,
    snapshot: &super::super::ActivationSnapshot,
) -> Result<Vec<PathBuf>, ModuleManifestError> {
    snapshot
        .modules
        .values()
        .map(|module| store.object(&module.digest))
        .collect()
}

#[cfg(test)]
pub(in crate::package) fn load_installed_module_graph_serial(
    root_module_graph_path: &Path,
) -> Result<InstalledModuleGraph, ModuleManifestError> {
    load_installed_module_graph_with(root_module_graph_path, None, load_module_manifests_serial)
}

#[cfg(test)]
pub(in crate::package) fn load_discovered_module_manifests(
    module_dirs: &[PathBuf],
) -> Result<Vec<(PathBuf, LoadedModuleManifest)>, ModuleManifestError> {
    let manifests = load_module_manifests(module_dirs)?;
    Ok(module_dirs.iter().cloned().zip(manifests).collect())
}

/// Load ordered module directories without serializing file IO and JSON parsing
/// on the caller. Indexed parallel iteration preserves the input order.
pub(in crate::package) fn load_module_manifests(
    module_dirs: &[PathBuf],
) -> Result<Vec<LoadedModuleManifest>, ModuleManifestError> {
    let loaded = module_dirs
        .par_iter()
        .map(|module_dir| load_module_manifest(module_dir))
        .collect::<Vec<_>>();
    loaded.into_iter().collect()
}

#[cfg(test)]
pub(in crate::package) fn load_discovered_module_manifests_serial(
    module_dirs: &[PathBuf],
) -> Result<Vec<(PathBuf, LoadedModuleManifest)>, ModuleManifestError> {
    let manifests = load_module_manifests_serial(module_dirs)?;
    Ok(module_dirs.iter().cloned().zip(manifests).collect())
}

#[cfg(test)]
pub(in crate::package) fn load_module_manifests_serial(
    module_dirs: &[PathBuf],
) -> Result<Vec<LoadedModuleManifest>, ModuleManifestError> {
    module_dirs
        .iter()
        .map(|module_dir| load_module_manifest(module_dir))
        .collect()
}

/// Recursively find directories under `modules_dir` that contain a
/// `module.json`. Descent stops once a `module.json` is found, so nested
/// resources inside a module are never treated as separate modules. Results are
/// sorted for deterministic ordering.
pub(in crate::package) fn discover_module_dirs(modules_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(metadata) = std::fs::symlink_metadata(modules_dir) else {
        return found;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return found;
    }
    let Ok(root) = std::fs::canonicalize(modules_dir) else {
        return found;
    };
    let mut visited = std::collections::HashSet::new();
    discover_module_dirs_into(modules_dir, &root, &mut visited, &mut found);
    found.sort();
    found
}

fn discover_module_dirs_into(
    dir: &Path,
    root: &Path,
    visited: &mut std::collections::HashSet<PathBuf>,
    found: &mut Vec<PathBuf>,
) {
    let Ok(metadata) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return;
    }
    let Ok(canonical) = std::fs::canonicalize(dir) else {
        return;
    };
    if !canonical.starts_with(root) || !visited.insert(canonical) {
        return;
    }
    let module_json = dir.join("module.json");
    if std::fs::symlink_metadata(&module_json)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
    {
        found.push(dir.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry
            .file_type()
            .map(|file_type| file_type.is_dir())
            .unwrap_or(false)
        {
            discover_module_dirs_into(&path, root, visited, found);
        }
    }
}

pub fn load_module_manifest(
    module_dir: &Path,
) -> Result<LoadedModuleManifest, ModuleManifestError> {
    validate_module_tree(module_dir)?;
    let plugin_json = module_dir.join("plugin.json");
    if plugin_json.exists() {
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                plugin_json,
                None,
                None,
                "plugin.json is not a supported MESH module manifest",
                "remove plugin.json or replace it with module.json",
            ),
        });
    }

    let module_json = module_dir.join("module.json");
    let package_json = module_dir.join("package.json");
    let mesh_toml = module_dir.join("mesh.toml");
    let existing = [&module_json, &package_json, &mesh_toml]
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();

    if existing.len() > 1 {
        let manifest_names = existing
            .iter()
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                module_dir,
                None,
                None,
                format!("ambiguous module manifest files found: {manifest_names}"),
                "keep canonical module.json and remove the old manifest file",
            ),
        });
    }

    if module_json.exists() {
        let content =
            std::fs::read_to_string(&module_json).map_err(|source| ModuleManifestError::Io {
                path: module_json.clone(),
                source,
            })?;
        if crate::manifest::is_canonical_module_json(&content).map_err(|source| {
            ModuleManifestError::Json {
                path: module_json.clone(),
                source,
            }
        })? {
            let mut manifest = ModuleManifest::from_path(&module_json)?;
            resolve_external_interface_contracts(&mut manifest, module_dir)?;
            let diagnostics = manifest.localized_text_diagnostics(&module_json);
            return Ok(LoadedModuleManifest {
                manifest,
                path: module_json,
                source: ModuleManifestSource::CanonicalModuleJson,
                diagnostics,
            });
        }

        let document: serde_json::Value =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: module_json.clone(),
                source,
            })?;
        let module_id = document
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let legacy_field = ["id", "type", "api_version"]
            .into_iter()
            .find(|field| document.get(field).is_some());
        let (field_path, message) = if let Some(field) = legacy_field {
            (
                Some(field.to_string()),
                format!("top-level {field} is a legacy module manifest field and is not supported"),
            )
        } else {
            (
                Some("$".into()),
                "legacy module.json shape uses id/type/api_version fields".into(),
            )
        };
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                &module_json,
                module_id,
                field_path,
                message,
                "replace legacy module.json fields with canonical name/version/mesh",
            ),
        });
    }

    if package_json.exists() {
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                package_json,
                None,
                None,
                "package.json is a legacy MESH module manifest path",
                "rename package.json to module.json",
            ),
        });
    }

    if mesh_toml.exists() {
        return Err(ModuleManifestError::Diagnostic {
            diagnostic: ModuleManifestDiagnostic::error(
                mesh_toml,
                None,
                None,
                "mesh.toml is a legacy MESH module manifest path",
                "replace mesh.toml with canonical module.json",
            ),
        });
    }

    Err(ModuleManifestError::Validation(format!(
        "no module.json found in {}",
        module_dir.display()
    )))
}

/// Replaces a module-relative external contract reference with its JSON object
/// before graph construction. Keeping the loaded manifest canonical means the
/// graph, runtime, and tooling share the existing contract path.
fn resolve_external_interface_contracts(
    manifest: &mut ModuleManifest,
    module_dir: &Path,
) -> Result<(), ModuleManifestError> {
    let module_id = manifest.name.clone();
    let mut declarations = manifest
        .mesh
        .interface
        .iter_mut()
        .chain(manifest.mesh.interfaces.iter_mut());

    for declaration in &mut declarations {
        let Some(serde_json::Value::String(relative_path)) = declaration.contract.as_ref() else {
            continue;
        };
        let path = contained_path(module_dir, relative_path, "external interface contract")?;
        let content = std::fs::read_to_string(&path).map_err(|source| ModuleManifestError::Io {
            path: path.clone(),
            source,
        })?;
        let contract: serde_json::Value =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.clone(),
                source,
            })?;
        if !contract.is_object() {
            return Err(ModuleManifestError::Validation(format!(
                "external contract '{}' for interface {} in module {} must be a JSON object",
                relative_path, declaration.name, module_id
            )));
        }
        declaration.contract = Some(contract);
    }
    Ok(())
}
