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

use super::{ModuleManifestError, atomic_write};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const LOCK_SCHEMA_VERSION: u32 = 2;

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

    pub fn from_path(path: &Path) -> Result<Self, ModuleManifestError> {
        let content = fs::read_to_string(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let lock: Self =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        lock.validate()?;
        Ok(lock)
    }

    pub fn load_or_default(path: &Path) -> Result<Self, ModuleManifestError> {
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
        for (module_id, entry) in &self.modules {
            if !module_id.starts_with('@') {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.lock entry '{module_id}' must be a module id such as @scope/name"
                )));
            }
            if !entry.digest.starts_with("sha256:") {
                return Err(ModuleManifestError::Validation(format!(
                    "mesh.lock entry '{module_id}' has a digest without its algorithm prefix"
                )));
            }
        }
        Ok(())
    }

    /// Write the next generation.
    ///
    /// A lock write failure fails the caller's transaction rather than being
    /// downgraded to a warning: the lock *is* the rollback record, so a
    /// composition whose lock did not land cannot be rolled back.
    pub fn save(&mut self, path: &Path) -> Result<(), ModuleManifestError> {
        self.validate()?;
        self.generation += 1;
        let mut content =
            serde_json::to_string_pretty(self).map_err(|source| ModuleManifestError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        content.push('\n');
        atomic_write(path, content.as_bytes())
    }

    /// Archive the current on-disk lock before overwriting it, so
    /// `mesh rollback` has a previous generation to restore.
    pub fn archive(path: &Path, history_dir: &Path) -> Result<(), ModuleManifestError> {
        if !path.exists() {
            return Ok(());
        }
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
        let Ok(entries) = fs::read_dir(history_dir) else {
            return Vec::new();
        };
        let mut generations: Vec<(u64, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
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

/// Content digest of an installed module tree.
///
/// Hashes relative path, executable bit, and bytes of every file in sorted
/// order, so the value is stable across machines and checkout order. Only
/// source is hashed — compiled output lives in `~/.cache/mesh` and never inside
/// a module directory, so an ordinary shell run cannot make a module look
/// edited.
pub fn module_tree_digest(root: &Path) -> Result<String, ModuleManifestError> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
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
) -> Result<(), ModuleManifestError> {
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
        let file_type = entry
            .file_type()
            .map_err(|source| ModuleManifestError::Io {
                path: path.clone(),
                source,
            })?;
        if file_type.is_dir() {
            if DIGEST_EXCLUDED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
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
    fn a_digest_without_its_algorithm_prefix_is_rejected() {
        let mut lock = MeshLock::new();
        lock.modules.insert("@me/x".into(), locked("deadbeef"));
        assert!(lock.validate().is_err());
    }
}
