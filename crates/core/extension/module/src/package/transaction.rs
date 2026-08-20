//! Durable package transactions shared by the CLI and the running shell.
//!
//! Package operations touch several independently persisted objects: installed
//! module trees, the root graph, profiles, and the lock/history.  This module is
//! deliberately below both consumers so they cannot accidentally grow
//! different locking or recovery rules.  A transaction takes an advisory OS
//! lock, records byte-for-byte filesystem snapshots in a durable journal before
//! mutation, and restores those snapshots if the operation fails or the next
//! package operation finds an unfinished journal.

use super::{ModuleManifestError, contained_path};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const JOURNAL_SCHEMA_VERSION: u32 = 1;
const JOURNAL_FILE: &str = ".mesh-package-transaction.json";
const LOCK_FILE: &str = ".mesh-package.lock";
const WORKSPACE_PREFIX: &str = ".mesh-package-transaction-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum JournalPhase {
    Prepared,
    Applying,
    Committed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JournalTarget {
    /// Path relative to the transaction's configuration directory.
    target: String,
    /// Path relative to the transaction workspace, or absent when the target
    /// did not exist at protection time.
    backup: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Journal {
    schema_version: u32,
    operation: String,
    workspace: String,
    phase: JournalPhase,
    targets: Vec<JournalTarget>,
}

/// One serialized package mutation.
pub struct PackageTransaction {
    config_dir: PathBuf,
    journal_path: PathBuf,
    workspace: PathBuf,
    journal: Journal,
    lock_file: File,
    finished: bool,
}

impl std::fmt::Debug for PackageTransaction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PackageTransaction")
            .field("config_dir", &self.config_dir)
            .field("journal_path", &self.journal_path)
            .field("workspace", &self.workspace)
            .field("journal", &self.journal)
            .finish_non_exhaustive()
    }
}

impl PackageTransaction {
    /// Acquire the package lock and recover a prior interrupted operation.
    ///
    /// The lock is held by the returned value for its entire lifetime.  On
    /// Unix this is a blocking `flock`, so a second CLI invocation waits rather
    /// than racing a shell IPC request.  The journal is intentionally a fixed
    /// path: there can be at most one package transaction per configuration
    /// directory.
    pub fn begin(
        config_dir: &Path,
        operation: impl Into<String>,
    ) -> Result<Self, ModuleManifestError> {
        let config_dir = canonical_config_dir(config_dir)?;
        fs::create_dir_all(&config_dir).map_err(|source| ModuleManifestError::Io {
            path: config_dir.clone(),
            source,
        })?;

        let lock_path = config_dir.join(LOCK_FILE);
        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|source| ModuleManifestError::Io {
                path: lock_path.clone(),
                source,
            })?;
        lock_exclusive(&lock_file).map_err(|source| ModuleManifestError::Io {
            path: lock_path,
            source,
        })?;

        let journal_path = config_dir.join(JOURNAL_FILE);
        recover_journal(&config_dir, &journal_path)?;

        let workspace_name = format!(
            "{WORKSPACE_PREFIX}{}-{}",
            std::process::id(),
            monotonic_nonce()
        );
        let workspace = contained_path(&config_dir, &workspace_name, "transaction workspace")
            .map_err(|error| error)?;
        let staging = workspace.join("staging");
        fs::create_dir_all(&staging).map_err(|source| ModuleManifestError::Io {
            path: staging,
            source,
        })?;

        let journal = Journal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            operation: operation.into(),
            workspace: workspace_name,
            phase: JournalPhase::Prepared,
            targets: Vec::new(),
        };
        let transaction = Self {
            config_dir,
            journal_path,
            workspace,
            journal,
            lock_file,
            finished: false,
        };
        transaction.persist_journal()?;
        Ok(transaction)
    }

    /// The directory for Git checkouts and other candidate materialization.
    /// It is never part of the live module tree.
    pub fn staging_dir(&self) -> PathBuf {
        self.workspace.join("staging")
    }

    /// Protect a file or directory before the caller mutates it.
    ///
    /// Protection is idempotent so callers can conservatively protect a whole
    /// package state set and then protect a more specific destination.
    pub fn protect(&mut self, path: &Path) -> Result<(), ModuleManifestError> {
        let relative = self.relative_target(path)?;
        if self
            .journal
            .targets
            .iter()
            .any(|target| target.target == relative)
        {
            return Ok(());
        }

        let target = self.config_dir.join(&relative);
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(ModuleManifestError::Io {
                    path: target,
                    source,
                });
            }
        };
        let backup = if let Some(metadata) = metadata {
            if metadata.file_type().is_symlink() {
                return Err(ModuleManifestError::Validation(format!(
                    "transaction target {} must not be a symlink",
                    target.display()
                )));
            }
            let backup_relative = format!("backup/{}", self.journal.targets.len());
            let backup_path = self.workspace.join(&backup_relative);
            copy_path(&target, &backup_path, &metadata)?;
            Some(backup_relative)
        } else {
            None
        };

        self.journal.targets.push(JournalTarget {
            target: relative,
            backup,
        });
        self.journal.phase = JournalPhase::Applying;
        self.persist_journal()
    }

    /// Protect the complete durable package state shared by install, update,
    /// uninstall, and rollback.
    pub fn protect_package_state(
        &mut self,
        root_graph: &Path,
        modules_dir: &Path,
    ) -> Result<(), ModuleManifestError> {
        let config_dir = self.config_dir.clone();
        for path in [
            root_graph.to_path_buf(),
            modules_dir.to_path_buf(),
            config_dir.join("mesh.lock"),
            config_dir.join("lock-history"),
            config_dir.join("profiles"),
            config_dir.join("active-profile"),
        ] {
            self.protect(&path)?;
        }
        Ok(())
    }

    /// Replace a live path with a candidate staged under [`Self::staging_dir`].
    pub fn replace_with(
        &mut self,
        target: &Path,
        staged: &Path,
    ) -> Result<(), ModuleManifestError> {
        self.protect(target)?;
        validate_staged_path(&self.workspace, staged)?;
        remove_path(target)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| ModuleManifestError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::rename(staged, target).map_err(|source| ModuleManifestError::Io {
            path: target.to_path_buf(),
            source,
        })
    }

    /// Remove a live path after recording it in the journal.
    pub fn remove(&mut self, target: &Path) -> Result<(), ModuleManifestError> {
        self.protect(target)?;
        remove_path(target)
    }

    /// Mark the durable state committed and remove the journal/workspace.
    pub fn commit(mut self) -> Result<(), ModuleManifestError> {
        self.journal.phase = JournalPhase::Committed;
        self.persist_journal()?;
        self.finished = true;
        cleanup_workspace(&self.journal_path, &self.workspace)
    }

    /// Restore every protected target in reverse order.
    pub fn abort(mut self) -> Result<(), ModuleManifestError> {
        restore_journal(&self.config_dir, &self.journal, &self.workspace)?;
        self.journal.phase = JournalPhase::Aborted;
        self.persist_journal()?;
        self.finished = true;
        cleanup_workspace(&self.journal_path, &self.workspace)
    }

    fn relative_target(&self, path: &Path) -> Result<String, ModuleManifestError> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|source| ModuleManifestError::Io {
                    path: PathBuf::from("."),
                    source,
                })?
                .join(path)
        };
        let relative = absolute.strip_prefix(&self.config_dir).map_err(|_| {
            ModuleManifestError::Validation(format!(
                "transaction target {} escapes {}",
                absolute.display(),
                self.config_dir.display()
            ))
        })?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
        {
            return Err(ModuleManifestError::Validation(format!(
                "transaction target {} is not a contained path",
                path.display()
            )));
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        contained_path(&self.config_dir, &relative, "transaction target").map_err(|error| error)?;
        Ok(relative)
    }

    fn persist_journal(&self) -> Result<(), ModuleManifestError> {
        let content = serde_json::to_vec_pretty(&self.journal).map_err(|source| {
            ModuleManifestError::Json {
                path: self.journal_path.clone(),
                source,
            }
        })?;
        durable_replace(&self.journal_path, &content)
    }
}

impl Drop for PackageTransaction {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _ = restore_journal(&self.config_dir, &self.journal, &self.workspace);
        let _ = cleanup_workspace(&self.journal_path, &self.workspace);
        // Keeping the advisory lock alive until this destructor returns makes
        // the restore indivisible from the next package operation.
        let _ = self.lock_file.sync_all();
    }
}

fn canonical_config_dir(path: &Path) -> Result<PathBuf, ModuleManifestError> {
    fs::create_dir_all(path).map_err(|source| ModuleManifestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|source| ModuleManifestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ModuleManifestError::Validation(format!(
            "transaction configuration directory {} must be a real directory",
            path.display()
        )));
    }
    fs::canonicalize(path).map_err(|source| ModuleManifestError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn recover_journal(config_dir: &Path, journal_path: &Path) -> Result<(), ModuleManifestError> {
    if !journal_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(journal_path).map_err(|source| ModuleManifestError::Io {
        path: journal_path.to_path_buf(),
        source,
    })?;
    let journal: Journal =
        serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
            path: journal_path.to_path_buf(),
            source,
        })?;
    if journal.schema_version != JOURNAL_SCHEMA_VERSION {
        return Err(ModuleManifestError::Validation(format!(
            "unsupported package transaction journal schemaVersion {}",
            journal.schema_version
        )));
    }
    let workspace = contained_path(config_dir, &journal.workspace, "transaction workspace")
        .map_err(|error| error)?;
    match journal.phase {
        JournalPhase::Committed | JournalPhase::Aborted => {
            cleanup_workspace(journal_path, &workspace)
        }
        JournalPhase::Prepared | JournalPhase::Applying => {
            restore_journal(config_dir, &journal, &workspace)?;
            cleanup_workspace(journal_path, &workspace)
        }
    }
}

fn restore_journal(
    config_dir: &Path,
    journal: &Journal,
    workspace: &Path,
) -> Result<(), ModuleManifestError> {
    for target in journal.targets.iter().rev() {
        let target_path = contained_path(config_dir, &target.target, "transaction target")
            .map_err(|error| error)?;
        remove_path(&target_path)?;
        let Some(backup) = &target.backup else {
            continue;
        };
        let backup_path =
            contained_path(workspace, backup, "transaction backup").map_err(|error| error)?;
        let metadata =
            fs::symlink_metadata(&backup_path).map_err(|source| ModuleManifestError::Io {
                path: backup_path.clone(),
                source,
            })?;
        copy_path(&backup_path, &target_path, &metadata)?;
    }
    Ok(())
}

fn cleanup_workspace(journal_path: &Path, workspace: &Path) -> Result<(), ModuleManifestError> {
    if workspace.exists() {
        remove_path(workspace)?;
    }
    match fs::remove_file(journal_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ModuleManifestError::Io {
            path: journal_path.to_path_buf(),
            source,
        }),
    }
}

fn validate_staged_path(workspace: &Path, staged: &Path) -> Result<(), ModuleManifestError> {
    let relative = staged.strip_prefix(workspace).map_err(|_| {
        ModuleManifestError::Validation(format!(
            "staged path {} escapes transaction workspace {}",
            staged.display(),
            workspace.display()
        ))
    })?;
    if relative.as_os_str().is_empty() {
        return Err(ModuleManifestError::Validation(
            "transaction cannot replace a workspace root".into(),
        ));
    }
    contained_path(workspace, &relative.to_string_lossy(), "staged path")
        .map(|_| ())
        .map_err(|error| error)
}

fn copy_path(
    source: &Path,
    destination: &Path,
    metadata: &fs::Metadata,
) -> Result<(), ModuleManifestError> {
    if metadata.file_type().is_symlink() {
        return Err(ModuleManifestError::Validation(format!(
            "transaction path {} must not be a symlink",
            source.display()
        )));
    }
    if metadata.is_dir() {
        fs::create_dir_all(destination).map_err(|source_error| ModuleManifestError::Io {
            path: destination.to_path_buf(),
            source: source_error,
        })?;
        for entry in fs::read_dir(source).map_err(|source_error| ModuleManifestError::Io {
            path: source.to_path_buf(),
            source: source_error,
        })? {
            let entry = entry.map_err(|source_error| ModuleManifestError::Io {
                path: source.to_path_buf(),
                source: source_error,
            })?;
            let child = entry.path();
            let child_metadata =
                fs::symlink_metadata(&child).map_err(|source_error| ModuleManifestError::Io {
                    path: child.clone(),
                    source: source_error,
                })?;
            copy_path(
                &child,
                &destination.join(entry.file_name()),
                &child_metadata,
            )?;
        }
    } else if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source_error| ModuleManifestError::Io {
                path: parent.to_path_buf(),
                source: source_error,
            })?;
        }
        fs::copy(source, destination).map_err(|source_error| ModuleManifestError::Io {
            path: destination.to_path_buf(),
            source: source_error,
        })?;
    } else {
        return Err(ModuleManifestError::Validation(format!(
            "transaction path {} must be a regular file or directory",
            source.display()
        )));
    }
    fs::set_permissions(destination, metadata.permissions()).map_err(|source| {
        ModuleManifestError::Io {
            path: destination.to_path_buf(),
            source,
        }
    })
}

fn remove_path(path: &Path) -> Result<(), ModuleManifestError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            fs::remove_file(path).map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
        Ok(metadata) if metadata.is_dir() => {
            fs::remove_dir_all(path).map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
        Ok(_) => Err(ModuleManifestError::Validation(format!(
            "transaction path {} is not removable",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn durable_replace(path: &Path, content: &[u8]) -> Result<(), ModuleManifestError> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ModuleManifestError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        sync_directory(path.parent())
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

fn sync_directory(path: Option<&Path>) -> std::io::Result<()> {
    let Some(path) = path else { return Ok(()) };
    File::open(path)?.sync_all()
}

fn monotonic_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(unix)]
fn lock_exclusive(file: &File) -> std::io::Result<()> {
    let result = unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(file), libc::LOCK_EX) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn lock_exclusive(_file: &File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mesh-package-transaction-{label}-{}",
            monotonic_nonce()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn failed_transaction_restores_files_and_directories() {
        let root = temp_dir("restore");
        let modules = root.join("modules");
        fs::create_dir_all(&modules).unwrap();
        fs::write(modules.join("old.txt"), "old").unwrap();
        let graph = root.join("module.json");
        fs::write(&graph, "before").unwrap();

        {
            let mut transaction = PackageTransaction::begin(&root, "install").unwrap();
            transaction.protect_package_state(&graph, &modules).unwrap();
            fs::write(&graph, "after").unwrap();
            fs::remove_dir_all(&modules).unwrap();
            fs::create_dir_all(&modules).unwrap();
            fs::write(modules.join("new.txt"), "new").unwrap();
        }

        assert_eq!(fs::read_to_string(&graph).unwrap(), "before");
        assert_eq!(fs::read_to_string(modules.join("old.txt")).unwrap(), "old");
        assert!(!modules.join("new.txt").exists());
        assert!(!root.join(JOURNAL_FILE).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unfinished_journal_is_recovered_by_the_next_transaction() {
        let root = temp_dir("crash");
        let graph = root.join("module.json");
        fs::write(&graph, "before").unwrap();
        let mut transaction = PackageTransaction::begin(&root, "update").unwrap();
        transaction.protect(&graph).unwrap();
        fs::write(&graph, "after").unwrap();
        // Simulate a process dying after the mutation but before cleanup.  The
        // finished flag prevents this test's Drop implementation from doing
        // the recovery that a real crash would leave to the next process.
        transaction.finished = true;
        drop(transaction);

        let transaction = PackageTransaction::begin(&root, "rollback").unwrap();
        assert_eq!(fs::read_to_string(&graph).unwrap(), "before");
        transaction.commit().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replacement_never_reads_a_candidate_from_the_live_tree() {
        let root = temp_dir("replace");
        let target = root.join("modules/widget");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("version"), "old").unwrap();

        let mut transaction = PackageTransaction::begin(&root, "update").unwrap();
        transaction.protect(&root.join("modules")).unwrap();
        let staged = transaction.staging_dir().join("widget");
        fs::create_dir_all(&staged).unwrap();
        fs::write(staged.join("version"), "new").unwrap();
        transaction.replace_with(&target, &staged).unwrap();
        assert_eq!(fs::read_to_string(target.join("version")).unwrap(), "new");
        transaction.commit().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
