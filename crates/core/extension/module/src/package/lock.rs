//! `mesh.lock` — what is installed, where it came from, and whether the user
//! has edited it since.
//!
//! The lock is graph state, not a CLI concern: update and rollback are
//! transactions over it, so it lives beside the manifest and profile types.
//!
//! Three fields carry the weight the previous git-provenance-only file could
//! not:
//!
//! - `digest` answers *"has the user edited this module?"*. Installed source is
//!   directly editable and updates promise to preserve edits
//!   ([`02-installation`](../../../../../docs/spec/02-installation.md) §1);
//!   without a content hash that promise is unenforceable.
//! - `version` gives update semantics. A git revision is reproducible but you
//!   cannot ask "is there a compatible newer release" from a commit.
//! - `requested_by` makes uninstall safe and explains version conflicts.

use super::{
    MODULE_SIGNATURE_FILE, ModuleId, ModuleKind, ModuleManifest, ModuleManifestError,
    SignedProvenance, TrustTier, atomic_write, dependency_spec_to_string, resolve_closure,
    validate_module_tree, validate_regular_file,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Current on-disk lock format. Version 3 adds direct dependency requirements
/// to each module entry and is the format described by the installation spec.
pub const LOCK_SCHEMA_VERSION: u32 = 3;
const LEGACY_LOCK_SCHEMA_VERSION: u32 = 2;

/// Directories never part of a module's content identity.
const DIGEST_EXCLUDED_DIRS: [&str; 3] = [".git", "node_modules", "target"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeshLock {
    #[serde(default = "default_lock_schema_version")]
    pub schema_version: u32,
    /// Monotonic counter. Rollback restores generation `n - 1`.
    #[serde(default)]
    pub generation: u64,
    /// The composition this lock was resolved for, when one is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<LockedComposition>,
    #[serde(default)]
    pub modules: BTreeMap<String, LockedModule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedComposition {
    pub module: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockedModule {
    pub version: String,
    pub source: ModuleSource,
    /// Resolved git revision, when the source is a repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// `sha256:<hex>` over the installed tree at install time.
    pub digest: String,
    /// Provenance classification selected by the package source.
    #[serde(default)]
    pub trust: TrustTier,
    /// Optional detached signature metadata for a verified provenance record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<SignedProvenance>,
    /// Direct module dependency requirements from the normalized manifest.
    /// BTreeMap keeps the serialized lock deterministic across discovery order.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    /// Module ids that required this one; empty means the user asked directly.
    #[serde(default)]
    pub requested_by: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ModuleSource {
    Path {
        path: String,
    },
    Git {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
    },
}

impl ModuleSource {
    pub fn describe(&self) -> String {
        match self {
            Self::Path { path } => path.clone(),
            Self::Git {
                url,
                reference: Some(reference),
            } => format!("{url}#{reference}"),
            Self::Git { url, .. } => url.clone(),
        }
    }
}

fn default_lock_schema_version() -> u32 {
    LOCK_SCHEMA_VERSION
}

impl Default for MeshLock {
    fn default() -> Self {
        Self {
            schema_version: LOCK_SCHEMA_VERSION,
            generation: 0,
            composition: None,
            modules: BTreeMap::new(),
        }
    }
}

impl MeshLock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace one installed module using the same provenance and
    /// dependency bookkeeping for every package client.
    pub fn upsert_module(
        &mut self,
        manifest: &ModuleManifest,
        installed_at: &Path,
        source: &ModuleSource,
        revision: Option<&str>,
        installed_manifests: &[ModuleManifest],
        activate_composition: bool,
    ) -> Result<(), ModuleManifestError> {
        let digest = module_tree_digest(installed_at)?;
        let signature = super::load_module_signature(installed_at)?;
        let trust = if signature.is_some() {
            TrustTier::Verified
        } else {
            TrustTier::for_source(&manifest.name, matches!(source, ModuleSource::Git { .. }))
        };
        self.modules.insert(
            manifest.name.clone(),
            LockedModule {
                version: manifest.version.clone(),
                source: source.clone(),
                revision: revision.map(str::to_owned),
                digest,
                trust,
                signature,
                dependencies: Default::default(),
                requested_by: Default::default(),
            },
        );
        if activate_composition && manifest.mesh.kind == ModuleKind::Composition {
            self.composition = Some(LockedComposition {
                module: manifest.name.clone(),
                version: manifest.version.clone(),
            });
        }
        self.refresh_metadata(installed_manifests.iter());
        Ok(())
    }

    /// Recompute lock provenance from the installed module set.
    ///
    /// Installers historically inserted every entry with an empty
    /// `requestedBy`, which made a dependency look directly removable. The
    /// installed manifests are the authoritative dependency closure, so keep
    /// the lock's uninstall guard synchronized with that graph after each
    /// package transaction.
    pub fn refresh_requested_by<'a>(
        &mut self,
        manifests: impl IntoIterator<Item = &'a ModuleManifest>,
    ) {
        let manifests = manifests.into_iter().collect::<Vec<_>>();
        let roots = manifests
            .iter()
            .map(|manifest| manifest.name.as_str())
            .collect::<Vec<_>>();
        let resolution = resolve_closure(roots, manifests.iter().copied());
        for (module_id, entry) in &mut self.modules {
            entry.requested_by = resolution
                .modules
                .get(module_id)
                .map(|module| module.requirements.keys().cloned().collect())
                .unwrap_or_default();
        }
    }

    pub fn from_path(path: &Path) -> Result<Self, ModuleManifestError> {
        validate_regular_file(path, "mesh.lock")?;
        let content = fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut document: serde_json::Value =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        migrate_lock_document(&mut document, path)?;
        let lock: Self =
            serde_json::from_value(document).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        lock.validate()?;
        Ok(lock)
    }

    pub fn load_or_default(path: &Path) -> Result<Self, ModuleManifestError> {
        if fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(ModuleManifestError::Validation(format!(
                "mesh.lock {} must not be a symlink",
                path.display()
            )));
        }
        if path.exists() {
            Self::from_path(path)
        } else {
            Ok(Self::new())
        }
    }

    pub fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.schema_version != LOCK_SCHEMA_VERSION {
            return Err(ModuleManifestError::Validation(format!(
                "unsupported mesh.lock schemaVersion {}; supported version is {LOCK_SCHEMA_VERSION}",
                self.schema_version
            )));
        }
        if let Some(composition) = &self.composition {
            ModuleId::parse(&composition.module)?;
        }
        for (module_id, entry) in &self.modules {
            ModuleId::parse(module_id).map_err(|_| {
                ModuleManifestError::Validation(format!(
                    "mesh.lock entry '{module_id}' must be a module id such as @scope/name"
                ))
            })?;
            for requester in &entry.requested_by {
                ModuleId::parse(requester)?;
            }
            for dependency_id in entry.dependencies.keys() {
                ModuleId::parse(dependency_id)?;
            }
            if !entry.digest.starts_with("sha256:") {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.lock entry '{module_id}' has a digest without its algorithm prefix"
                )));
            }
            if let Some(signature) = &entry.signature {
                signature.validate().map_err(|message| {
                    ModuleManifestError::Validation(format!(
                        "mesh.lock entry '{module_id}' has invalid signed provenance: {message}"
                    ))
                })?;
            }
            if entry.trust.requires_signature() && entry.signature.is_none() {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.lock entry '{module_id}' claims verified provenance without a signature"
                )));
            }
        }
        Ok(())
    }

    /// Rebuild both direct dependency requirements and reverse requester
    /// metadata from the normalized installed manifests.
    pub fn refresh_metadata<'a>(
        &mut self,
        manifests: impl IntoIterator<Item = &'a ModuleManifest>,
    ) {
        let manifests = manifests.into_iter().collect::<Vec<_>>();
        for manifest in &manifests {
            if let Some(entry) = self.modules.get_mut(&manifest.name) {
                entry.dependencies = manifest
                    .mesh
                    .dependencies
                    .modules
                    .iter()
                    .map(|(module_id, spec)| (module_id.clone(), dependency_spec_to_string(spec)))
                    .collect();
                if manifest.mesh.kind == ModuleKind::Composition
                    && let Some(extends) = &manifest.mesh.extends
                {
                    entry.dependencies.insert(extends.clone(), "*".into());
                }
            }
        }
        self.refresh_requested_by(manifests.iter().copied());
        for manifest in &manifests {
            if manifest.mesh.kind == ModuleKind::Composition
                && let Some(extends) = &manifest.mesh.extends
                && let Some(entry) = self.modules.get_mut(extends)
            {
                entry.requested_by.insert(manifest.name.clone());
            }
        }
    }

    /// Write the next generation.
    ///
    /// A lock write failure fails the caller's transaction rather than being
    /// downgraded to a warning: the lock *is* the rollback record, so a
    /// composition whose lock did not land cannot be rolled back.
    pub fn save(&mut self, path: &Path) -> Result<(), ModuleManifestError> {
        self.validate()?;
        if path.exists() {
            validate_regular_file(path, "mesh.lock")?;
        }
        self.generation += 1;
        let mut content =
            serde_json::to_string_pretty(self).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        content.push('\n');
        atomic_write(path, content.as_bytes())
    }

    /// Persist a lock generation together with the immutable module objects
    /// and activation snapshot it names.
    ///
    /// The editable module tree remains the authoring source, but a package
    /// commit is not considered publishable until every locked module has a
    /// matching content-addressed object. The snapshot is written before the
    /// lock and activated only after the lock bytes land, so a failed lock
    /// write cannot advance the active generation.
    pub fn save_with_store(
        &mut self,
        path: &Path,
        modules_dir: &Path,
        store_root: &Path,
    ) -> Result<(), ModuleManifestError> {
        self.validate()?;
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            ModuleManifestError::Validation("mesh.lock generation overflow".into())
        })?;
        let store = super::ModuleStore::new(store_root.to_path_buf())?;
        let snapshot = store.snapshot_from_lock(generation, self, modules_dir)?;
        store.publish_snapshot(&snapshot)?;
        self.generation = generation;
        self.save_exact(path)?;
        store.activate_generation(generation)
    }

    /// Persist an already-resolved generation without advancing it.
    ///
    /// Rollback restores the selected lock generation exactly.  Calling
    /// [`Self::save`] here would turn the restored snapshot into a new, subtly
    /// different generation and would make a second rollback ambiguous.
    pub fn save_exact(&self, path: &Path) -> Result<(), ModuleManifestError> {
        self.validate()?;
        if path.exists() {
            validate_regular_file(path, "mesh.lock")?;
        }
        let mut content =
            serde_json::to_string_pretty(self).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        content.push('\n');
        atomic_write(path, content.as_bytes())
    }

    /// Persist an already-selected generation and publish its matching
    /// activation snapshot. This is used by rollback, where advancing the
    /// generation would change the meaning of the archived lock.
    pub fn save_exact_with_store(
        &self,
        path: &Path,
        modules_dir: &Path,
        store_root: &Path,
    ) -> Result<(), ModuleManifestError> {
        self.validate()?;
        let store = super::ModuleStore::new(store_root.to_path_buf())?;
        let snapshot = store.snapshot_from_lock(self.generation, self, modules_dir)?;
        store.publish_snapshot(&snapshot)?;
        self.save_exact(path)?;
        store.activate_generation(self.generation)
    }

    /// Archive the current on-disk lock before overwriting it, so
    /// `mesh rollback` has a previous generation to restore.
    pub fn archive(path: &Path, history_dir: &Path) -> Result<(), ModuleManifestError> {
        if fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(ModuleManifestError::Validation(format!(
                "mesh.lock {} must not be a symlink",
                path.display()
            )));
        }
        if !path.exists() {
            return Ok(());
        }
        validate_regular_file(path, "mesh.lock")?;
        let existing = Self::from_path(path)?;
        let content = fs::read(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let archived = history_dir.join(format!("mesh-{}.lock", existing.generation));
        atomic_write(&archived, &content)?;
        prune_history(history_dir, 10);
        Ok(())
    }

    /// Lock generations available for rollback, newest first.
    pub fn history(history_dir: &Path) -> Vec<(u64, PathBuf)> {
        let Ok(metadata) = fs::symlink_metadata(history_dir) else {
            return Vec::new();
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(history_dir) else {
            return Vec::new();
        };
        let mut generations: Vec<(u64, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if entry.file_type().ok()?.is_symlink() || !entry.file_type().ok()?.is_file() {
                    return None;
                }
                let stem = path.file_stem()?.to_str()?.strip_prefix("mesh-")?;
                Some((stem.parse::<u64>().ok()?, path))
            })
            .collect();
        generations.sort_by(|left, right| right.0.cmp(&left.0));
        generations
    }
}

fn prune_history(history_dir: &Path, keep: usize) {
    let generations = MeshLock::history(history_dir);
    for (_, path) in generations.into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

/// Upgrade the previous lock format before strict deserialization.
///
/// Version 2 already carried source, revision, digest, requester, and
/// composition provenance. Version 3 makes direct dependency requirements
/// explicit, so old entries migrate with an empty map and are populated on the
/// next package operation when their manifests are available.
fn migrate_lock_document(
    document: &mut serde_json::Value,
    path: &Path,
) -> Result<(), ModuleManifestError> {
    let Some(object) = document.as_object_mut() else {
        // Let serde produce the normal shape error for malformed documents.
        return Ok(());
    };
    let schema_version = match object.get("schemaVersion") {
        None => LEGACY_LOCK_SCHEMA_VERSION,
        Some(value) => value.as_u64().ok_or_else(|| {
            ModuleManifestError::Validation(format!(
                "mesh.lock {} has a non-numeric schemaVersion",
                path.display()
            ))
        })? as u32,
    };
    if schema_version != LEGACY_LOCK_SCHEMA_VERSION {
        return Ok(());
    }

    object.insert(
        "schemaVersion".into(),
        serde_json::Value::from(LOCK_SCHEMA_VERSION),
    );
    if let Some(modules) = object
        .get_mut("modules")
        .and_then(serde_json::Value::as_object_mut)
    {
        for module in modules.values_mut() {
            if let Some(module) = module.as_object_mut() {
                module
                    .entry("dependencies")
                    .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            }
        }
    }
    Ok(())
}

/// Content digest of an installed module tree.
///
/// Hashes relative path, executable bit, and bytes of every file in sorted
/// order, so the value is stable across machines and checkout order. Only
/// source is hashed — compiled output lives in `~/.cache/mesh` and never inside
/// a module directory, so an ordinary shell run cannot make a module look
/// edited.
pub fn module_tree_digest(root: &Path) -> Result<String, ModuleManifestError> {
    validate_module_tree(root)?;
    let mut files = Vec::new();
    let mut visited = std::collections::HashSet::new();
    collect_files(root, root, &mut files, &mut visited)?;
    files.sort();

    let mut hasher = Sha256::new();
    for relative in &files {
        let path = root.join(relative);
        let metadata = fs::metadata(&path).map_err(|source| ModuleManifestError::Io {
            path: path.clone(),
            source,
        })?;
        let contents = fs::read(&path).map_err(|source| ModuleManifestError::Io {
            path: path.clone(),
            source,
        })?;
        // Path and length are framed so that concatenation cannot collide.
        hasher.update((relative.len() as u64).to_le_bytes());
        hasher.update(relative.as_bytes());
        hasher.update([u8::from(is_executable(&metadata))]);
        hasher.update((contents.len() as u64).to_le_bytes());
        hasher.update(&contents);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<String>,
    visited: &mut std::collections::HashSet<PathBuf>,
) -> Result<(), ModuleManifestError> {
    let canonical = fs::canonicalize(directory).map_err(|source| ModuleManifestError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    if !visited.insert(canonical) {
        return Ok(());
    }
    let entries = fs::read_dir(directory).map_err(|source| ModuleManifestError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ModuleManifestError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let metadata = fs::symlink_metadata(&path).map_err(|source| ModuleManifestError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ModuleManifestError::Validation(format!(
                "module tree contains unsupported symlink {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            if DIGEST_EXCLUDED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_files(root, &path, files, visited)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| {
                    ModuleManifestError::Validation(format!(
                        "module file {} escaped its module root",
                        path.display()
                    ))
                })?
                .to_string_lossy()
                .replace('\\', "/");
            if relative == MODULE_SIGNATURE_FILE {
                continue;
            }
            files.push(relative);
        }
    }
    Ok(())
}

/// Whether the installed tree still matches what the lock recorded.
pub fn has_local_edits(root: &Path, locked: &LockedModule) -> Result<bool, ModuleManifestError> {
    Ok(module_tree_digest(root)? != locked.digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("mesh-lock-{name}-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn locked(digest: &str) -> LockedModule {
        LockedModule {
            version: "1.0.0".into(),
            source: ModuleSource::Path {
                path: "../local".into(),
            },
            revision: None,
            digest: digest.into(),
            trust: Default::default(),
            signature: None,
            dependencies: BTreeMap::new(),
            requested_by: BTreeSet::new(),
        }
    }

    #[test]
    fn digest_is_stable_across_reinstall_and_changes_on_a_one_byte_edit() {
        let a = temp_dir("stable-a");
        let b = temp_dir("stable-b");
        for root in [&a, &b] {
            write(root, "module.json", r#"{"name":"@me/x"}"#);
            write(root, "src/main.mesh", "<template><box/></template>");
        }

        let first = module_tree_digest(&a).unwrap();
        assert_eq!(first, module_tree_digest(&b).unwrap());
        assert!(first.starts_with("sha256:"));

        write(&b, "src/main.mesh", "<template><box /></template>");
        assert_ne!(first, module_tree_digest(&b).unwrap());
    }

    #[test]
    fn digest_ignores_git_metadata_but_not_source() {
        let root = temp_dir("git");
        write(&root, "module.json", "{}");
        let before = module_tree_digest(&root).unwrap();

        write(&root, ".git/HEAD", "ref: refs/heads/main");
        assert_eq!(before, module_tree_digest(&root).unwrap());

        write(&root, "src/extra.mesh", "<template/>");
        assert_ne!(before, module_tree_digest(&root).unwrap());
    }

    #[test]
    fn digest_excludes_the_detached_signature_sidecar() {
        let root = temp_dir("signature");
        write(&root, "module.json", "{}");
        let before = module_tree_digest(&root).unwrap();
        write(
            &root,
            MODULE_SIGNATURE_FILE,
            r#"{"keyId":"release","algorithm":"ed25519","signature":"AAAA"}"#,
        );
        assert_eq!(before, module_tree_digest(&root).unwrap());
    }

    #[test]
    fn a_renamed_file_changes_the_digest() {
        let root = temp_dir("rename");
        write(&root, "a.txt", "same");
        let before = module_tree_digest(&root).unwrap();
        fs::remove_file(root.join("a.txt")).unwrap();
        write(&root, "b.txt", "same");
        assert_ne!(before, module_tree_digest(&root).unwrap());
    }

    #[test]
    fn local_edits_are_detected_against_the_recorded_digest() {
        let root = temp_dir("edits");
        write(&root, "module.json", "{}");
        let entry = locked(&module_tree_digest(&root).unwrap());
        assert!(!has_local_edits(&root, &entry).unwrap());

        write(&root, "module.json", "{ }");
        assert!(has_local_edits(&root, &entry).unwrap());
    }

    #[test]
    fn saving_advances_the_generation_and_round_trips() {
        let dir = temp_dir("save");
        let path = dir.join("mesh.lock");
        let mut lock = MeshLock::new();
        lock.modules.insert(
            "@me/x".into(),
            LockedModule {
                version: "1.2.3".into(),
                source: ModuleSource::Git {
                    url: "https://example.invalid/x".into(),
                    reference: Some("v1".into()),
                },
                revision: Some("abc123".into()),
                digest: "sha256:deadbeef".into(),
                trust: Default::default(),
                signature: None,
                dependencies: BTreeMap::new(),
                requested_by: BTreeSet::from(["@me/desk".to_string()]),
            },
        );
        lock.save(&path).unwrap();
        assert_eq!(lock.generation, 1);

        let reloaded = MeshLock::from_path(&path).unwrap();
        assert_eq!(reloaded.generation, 1);
        assert_eq!(reloaded.modules["@me/x"].version, "1.2.3");
        assert_eq!(
            reloaded.modules["@me/x"].revision.as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn archiving_keeps_generations_for_rollback() {
        let dir = temp_dir("archive");
        let path = dir.join("mesh.lock");
        let history = dir.join("lock-history");

        let mut lock = MeshLock::new();
        for _ in 0..3 {
            MeshLock::archive(&path, &history).unwrap();
            lock.save(&path).unwrap();
        }

        let generations: Vec<u64> = MeshLock::history(&history)
            .into_iter()
            .map(|(generation, _)| generation)
            .collect();
        assert_eq!(generations, vec![2, 1]);
    }

    #[test]
    fn requested_by_is_rebuilt_from_the_installed_dependency_closure() {
        let requester = ModuleManifest::from_json_str(
            r#"{"name":"@me/desk","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"frontend","entry":"main.mesh","dependencies":{"modules":{"@me/shared":"^1.0.0"}}}}"#,
        )
        .unwrap();
        let shared = ModuleManifest::from_json_str(
            r#"{"name":"@me/shared","version":"1.2.0","mesh":{"apiVersion":"0.1","kind":"library"}}"#,
        )
        .unwrap();
        let mut lock = MeshLock::new();
        lock.modules
            .insert("@me/desk".into(), locked("sha256:desk"));
        lock.modules
            .insert("@me/shared".into(), locked("sha256:shared"));

        lock.refresh_metadata([&requester, &shared]);

        assert_eq!(
            lock.modules["@me/shared"].requested_by,
            BTreeSet::from(["@me/desk".to_string()])
        );
        assert_eq!(
            lock.modules["@me/desk"].dependencies,
            BTreeMap::from([("@me/shared".to_string(), "^1.0.0".to_string())])
        );
        assert!(lock.modules["@me/desk"].requested_by.is_empty());
    }

    #[test]
    fn schema_v2_lock_is_migrated_to_the_authoritative_v3_format() {
        let dir = temp_dir("migrate-v2");
        let path = dir.join("mesh.lock");
        write(
            &dir,
            "mesh.lock",
            r#"{
              "schemaVersion": 2,
              "generation": 4,
              "modules": {
                "@me/x": {
                  "version": "1.0.0",
                  "source": {"kind":"path","path":"modules/x"},
                  "digest": "sha256:deadbeef",
                  "requestedBy": []
                }
              }
            }"#,
        );

        let lock = MeshLock::from_path(&path).unwrap();

        assert_eq!(lock.schema_version, LOCK_SCHEMA_VERSION);
        assert!(lock.modules["@me/x"].dependencies.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn composition_extends_edges_are_locked_as_dependency_provenance() {
        let derived = ModuleManifest::from_json_str(
            r#"{"name":"@me/derived","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"composition","extends":"@me/base","compose":{}}}"#,
        )
        .unwrap();
        let base = ModuleManifest::from_json_str(
            r#"{"name":"@me/base","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"composition","compose":{}}}"#,
        )
        .unwrap();
        let mut lock = MeshLock::new();
        lock.modules
            .insert("@me/derived".into(), locked("sha256:derived"));
        lock.modules
            .insert("@me/base".into(), locked("sha256:base"));

        lock.refresh_metadata([&derived, &base]);

        assert_eq!(
            lock.modules["@me/derived"].dependencies,
            BTreeMap::from([("@me/base".to_string(), "*".to_string())])
        );
        assert_eq!(
            lock.modules["@me/base"].requested_by,
            BTreeSet::from(["@me/derived".to_string()])
        );
    }

    #[test]
    fn a_digest_without_its_algorithm_prefix_is_rejected() {
        let mut lock = MeshLock::new();
        lock.modules.insert("@me/x".into(), locked("deadbeef"));
        assert!(lock.validate().is_err());
    }
}
