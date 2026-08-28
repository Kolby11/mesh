use super::{InstalledModuleGraph, ModuleManifestError, module_tree_digest};
use sha2::{Digest, Sha256};
use std::path::Path;

/// The canonical graph-authoring read model shared by tooling and the shell.
///
/// The resolved graph is already an immutable snapshot of canonical manifests,
/// activation decisions, provider contracts, diagnostics, and resource
/// selection. This alias gives consumers a name for that boundary without
/// creating a second inventory representation.
pub type AuthoringSnapshot = InstalledModuleGraph;

/// Load one canonical graph snapshot for CLI, doctor, LSP, and runtime use.
///
/// The graph's structural revision is mixed with the root graph, lock/profile,
/// and module-tree inputs so editing an implementation or authoring file also
/// produces a new snapshot identity.
pub fn load_authoring_snapshot(
    root_module_graph_path: &Path,
) -> Result<AuthoringSnapshot, ModuleManifestError> {
    let graph = super::load_installed_module_graph(root_module_graph_path)?;
    Ok(with_content_revision(root_module_graph_path, graph, None))
}

/// Load a canonical authoring snapshot against a candidate profile without
/// changing the active profile pointer.
pub fn load_authoring_snapshot_for_profile(
    root_module_graph_path: &Path,
    profile: &super::ShellProfile,
) -> Result<AuthoringSnapshot, ModuleManifestError> {
    let graph = super::load_installed_module_graph_for_profile(root_module_graph_path, profile)?;
    Ok(with_content_revision(
        root_module_graph_path,
        graph,
        Some(profile),
    ))
}

fn with_content_revision(
    root_module_graph_path: &Path,
    graph: InstalledModuleGraph,
    candidate_profile: Option<&super::ShellProfile>,
) -> AuthoringSnapshot {
    let mut hasher = Sha256::new();
    hasher.update(b"mesh-authoring-snapshot/v1\n");
    hasher.update(graph.revision().to_be_bytes());
    hash_file(&mut hasher, root_module_graph_path);
    if let Some(root_dir) = root_module_graph_path.parent() {
        hash_file(&mut hasher, &root_dir.join("mesh.lock"));
        let active_profile = root_dir.join("active-profile");
        hash_file(&mut hasher, &active_profile);
        if let Some(profile) = candidate_profile {
            if let Ok(bytes) = serde_json::to_vec(profile) {
                hasher.update(b"candidate-profile\n");
                hasher.update(bytes);
            }
        } else if let Ok(profile_id) = std::fs::read_to_string(&active_profile) {
            hash_file(
                &mut hasher,
                &root_dir
                    .join("profiles")
                    .join(format!("{}.json", profile_id.trim())),
            );
        }
    }
    for module in graph.modules() {
        hasher.update(module.id.as_bytes());
        hasher.update(module.manifest_path.to_string_lossy().as_bytes());
        let digest = module
            .manifest_path
            .parent()
            .and_then(|path| module_tree_digest(path).ok())
            .unwrap_or_else(|| "unavailable".into());
        hasher.update(digest.as_bytes());
    }
    let digest = hasher.finalize();
    let revision =
        u64::from_be_bytes(digest[..8].try_into().expect("sha256 has enough bytes")).max(1);
    graph.with_revision(revision)
}

fn hash_file(hasher: &mut Sha256, path: &Path) {
    hasher.update(path.to_string_lossy().as_bytes());
    match std::fs::read(path) {
        Ok(bytes) => {
            hasher.update([1]);
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        }
        Err(_) => hasher.update([0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{LoadedModuleManifest, RootModuleGraphManifest};

    #[test]
    fn graph_snapshots_have_a_stable_nonzero_revision() {
        let root =
            RootModuleGraphManifest::from_json_str(r#"{"mesh":{"schemaVersion":1}}"#).unwrap();
        let modules = Vec::<LoadedModuleManifest>::new();
        let first = InstalledModuleGraph::from_parts(root.clone(), modules.clone()).unwrap();
        let second = InstalledModuleGraph::from_parts(root, modules).unwrap();
        assert_ne!(first.revision(), 0);
        assert_eq!(first.revision(), second.revision());
    }

    #[test]
    fn loaded_snapshot_revision_tracks_root_graph_changes() {
        let root_dir =
            std::env::temp_dir().join(format!("mesh-authoring-snapshot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root_dir);
        std::fs::create_dir_all(root_dir.join("modules")).unwrap();
        let root_path = root_dir.join("module.json");
        std::fs::write(
            &root_path,
            r#"{"mesh":{"schemaVersion":1,"modulesDir":"modules"}}"#,
        )
        .unwrap();
        let first = load_authoring_snapshot(&root_path).unwrap();
        std::fs::write(
            &root_path,
            r#"{"mesh":{"schemaVersion":1,"modulesDir":"modules","disabled":["@example/disabled"]}}"#,
        )
        .unwrap();
        let second = load_authoring_snapshot(&root_path).unwrap();
        assert_ne!(first.revision(), second.revision());
        std::fs::remove_dir_all(root_dir).unwrap();
    }
}
