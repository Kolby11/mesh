//! Durable package transactions shared by the CLI and the running shell.
//!
//! Package operations touch several independently persisted objects: installed
//! module trees, the root graph, profiles, and the lock/history.  This module is
//! deliberately below both consumers so they cannot accidentally grow
//! different locking or recovery rules.  A transaction takes an advisory OS
//! lock, records byte-for-byte filesystem snapshots in a durable journal before
//! mutation, and restores those snapshots if the operation fails or the next
//! package operation finds an unfinished journal.

use super::{ModuleManifestError, contained_path, validate_module_tree};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const JOURNAL_SCHEMA_VERSION: u32 = 1;
const JOURNAL_FILE: &str = ".mesh-package-transaction.json";
const LOCK_FILE: &str = ".mesh-package.lock";
const WORKSPACE_PREFIX: &str = ".mesh-package-transaction-";

#[cfg(test)]
thread_local! {
    static TEST_FAILURE_POINT: RefCell<Option<&'static str>> = const { RefCell::new(None) };
}

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

    /// Recover an interrupted package transaction without starting a new one.
    ///
    /// Shell startup calls this before loading the installed graph.  A missing
    /// journal is a no-op and does not create a lock file, while an existing
    /// journal is recovered under the same lock used by mutating commands.
    pub fn recover(config_dir: &Path) -> Result<(), ModuleManifestError> {
        let config_dir = canonical_config_dir(config_dir)?;
        let journal_path = config_dir.join(JOURNAL_FILE);
        match fs::symlink_metadata(&journal_path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(source) => {
                return Err(ModuleManifestError::Io {
                    path: journal_path,
                    source,
                });
            }
        }

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
        recover_journal(&config_dir, &journal_path)
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
            config_dir.join(".mesh-store/active-generation"),
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
        maybe_inject_failure("replace.before")?;
        remove_path(target)?;
        maybe_inject_failure("replace.after_remove")?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| ModuleManifestError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::rename(staged, target).map_err(|source| ModuleManifestError::Io {
            path: target.to_path_buf(),
            source,
        })?;
        sync_directory(target.parent()).map_err(|source| ModuleManifestError::Io {
            path: target
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| target.to_path_buf()),
            source,
        })?;
        maybe_inject_failure("replace.after_rename")
    }

    /// Remove a live path after recording it in the journal.
    pub fn remove(&mut self, target: &Path) -> Result<(), ModuleManifestError> {
        self.protect(target)?;
        maybe_inject_failure("remove.before")?;
        remove_path(target)?;
        maybe_inject_failure("remove.after")
    }

    /// Mark the durable state committed and remove the journal/workspace.
    pub fn commit(mut self) -> Result<(), ModuleManifestError> {
        maybe_inject_failure("commit.before")?;
        self.sync_targets()?;
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
        maybe_inject_failure("journal.write.before")?;
        durable_replace(&self.journal_path, &content)?;
        sync_directory(Some(&self.workspace)).map_err(|source| ModuleManifestError::Io {
            path: self.workspace.clone(),
            source,
        })?;
        maybe_inject_failure("journal.write.after")
    }

    fn sync_targets(&self) -> Result<(), ModuleManifestError> {
        for target in &self.journal.targets {
            let path = contained_path(&self.config_dir, &target.target, "transaction target")
                .map_err(|error| error)?;
            sync_path(&path)?;
        }
        Ok(())
    }
}

impl Drop for PackageTransaction {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if matches!(
            self.journal.phase,
            JournalPhase::Committed | JournalPhase::Aborted
        ) {
            let _ = cleanup_workspace(&self.journal_path, &self.workspace);
        } else if restore_journal(&self.config_dir, &self.journal, &self.workspace).is_ok() {
            // Persist the recovery marker before deleting the journal.  If a
            // process dies during cleanup, the next process can safely retry
            // cleanup instead of interpreting a half-restored Applying
            // journal as an operation that still needs live mutations.
            self.journal.phase = JournalPhase::Aborted;
            if self.persist_journal().is_ok() {
                let _ = cleanup_workspace(&self.journal_path, &self.workspace);
            }
        }
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
    let metadata = match fs::symlink_metadata(journal_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ModuleManifestError::Io {
                path: journal_path.to_path_buf(),
                source,
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ModuleManifestError::Validation(format!(
            "package transaction journal {} must be a regular file",
            journal_path.display()
        )));
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
            let mut recovered = journal;
            recovered.phase = JournalPhase::Aborted;
            let content = serde_json::to_vec_pretty(&recovered).map_err(|source| {
                ModuleManifestError::Json {
                    path: journal_path.to_path_buf(),
                    source,
                }
            })?;
            durable_replace(journal_path, &content)?;
            sync_directory(Some(&workspace)).map_err(|source| ModuleManifestError::Io {
                path: workspace.clone(),
                source,
            })?;
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
    if fs::symlink_metadata(workspace).is_ok() {
        remove_workspace(workspace)?;
        sync_directory(workspace.parent()).map_err(|source| ModuleManifestError::Io {
            path: workspace
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| workspace.to_path_buf()),
            source,
        })?;
    }
    match fs::remove_file(journal_path) {
        Ok(()) => {
            sync_directory(journal_path.parent()).map_err(|source| ModuleManifestError::Io {
                path: journal_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| journal_path.to_path_buf()),
                source,
            })?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ModuleManifestError::Io {
            path: journal_path.to_path_buf(),
            source,
        }),
    }
}

/// Remove generated transaction material without following symlinks from a
/// staged checkout. Live package paths use `remove_path`, which validates the
/// entire module tree before recursive deletion; cleanup must also handle a
/// rejected candidate safely.
fn remove_workspace(path: &Path) -> Result<(), ModuleManifestError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })? {
            let entry = entry.map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            remove_workspace(&entry.path())?;
        }
        fs::remove_dir(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })
    } else {
        Err(ModuleManifestError::Validation(format!(
            "transaction workspace path {} is not removable",
            path.display()
        )))
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
        File::open(destination)
            .and_then(|file| file.sync_all())
            .map_err(|source| ModuleManifestError::Io {
                path: destination.to_path_buf(),
                source,
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
    })?;
    if metadata.is_dir() {
        sync_directory(Some(destination)).map_err(|source| ModuleManifestError::Io {
            path: destination.to_path_buf(),
            source,
        })?;
    }
    maybe_inject_failure("backup.copy.after")
}

fn remove_path(path: &Path) -> Result<(), ModuleManifestError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            fs::remove_file(path).map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })?
        }
        Ok(metadata) if metadata.is_dir() => {
            // A package removal must reject the complete tree before invoking
            // the recursive filesystem operation. This keeps symlinks and
            // unsupported special files from turning an uninstall into an
            // escape from the validated package path.
            validate_module_tree(path)?;
            fs::remove_dir_all(path).map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })?
        }
        Ok(_) => {
            return Err(ModuleManifestError::Validation(format!(
                "transaction path {} is not removable",
                path.display()
            )))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    sync_directory(path.parent()).map_err(|source| ModuleManifestError::Io {
        path: path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf()),
        source,
    })
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

pub(super) fn maybe_inject_failure(point: &str) -> Result<(), ModuleManifestError> {
    #[cfg(not(test))]
    let _ = point;
    #[cfg(test)]
    {
        let injected = TEST_FAILURE_POINT.with(|failure| {
            let mut failure = failure.borrow_mut();
            if failure.as_deref() == Some(point) {
                failure.take()
            } else {
                None
            }
        });
        if injected.is_some() {
            return Err(ModuleManifestError::Validation(format!(
                "injected package transaction failure at {point}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
struct FailureInjectionGuard {
    previous: Option<&'static str>,
}

#[cfg(test)]
fn inject_failure_at(point: &'static str) -> FailureInjectionGuard {
    let previous = TEST_FAILURE_POINT.with(|failure| failure.replace(Some(point)));
    FailureInjectionGuard { previous }
}

#[cfg(test)]
impl Drop for FailureInjectionGuard {
    fn drop(&mut self) {
        TEST_FAILURE_POINT.with(|failure| {
            failure.replace(self.previous.take());
        });
    }
}

fn sync_directory(path: Option<&Path>) -> std::io::Result<()> {
    let Some(path) = path else { return Ok(()) };
    File::open(path)?.sync_all()
}

fn sync_path(path: &Path) -> Result<(), ModuleManifestError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(ModuleManifestError::Validation(format!(
            "transaction target {} must not be a symlink",
            path.display()
        )));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })? {
            let entry = entry.map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            sync_path(&entry.path())?;
        }
        sync_directory(Some(path)).map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    } else if metadata.is_file() {
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })?;
    } else {
        return Err(ModuleManifestError::Validation(format!(
            "transaction target {} is not syncable",
            path.display()
        )));
    }
    Ok(())
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

    fn make_test_tree_writable(path: &Path) {
        let metadata = fs::symlink_metadata(path).unwrap();
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                make_test_tree_writable(&entry.unwrap().path());
            }
        }
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
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
    fn public_recovery_restores_an_interrupted_transaction() {
        let root = temp_dir("public-recover");
        let graph = root.join("module.json");
        fs::write(&graph, "before").unwrap();
        let mut transaction = PackageTransaction::begin(&root, "update").unwrap();
        transaction.protect(&graph).unwrap();
        fs::write(&graph, "after").unwrap();
        transaction.finished = true;
        drop(transaction);

        PackageTransaction::recover(&root).unwrap();

        assert_eq!(fs::read_to_string(&graph).unwrap(), "before");
        assert!(!root.join(JOURNAL_FILE).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn injected_journal_write_failure_leaves_no_live_transaction() {
        let root = temp_dir("journal-failure");
        let failure = inject_failure_at("journal.write.after");
        assert!(PackageTransaction::begin(&root, "failure-test").is_err());
        drop(failure);

        assert!(!root.join(JOURNAL_FILE).exists());
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(WORKSPACE_PREFIX))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn injected_commit_failure_aborts_and_restores() {
        let root = temp_dir("commit-failure");
        let graph = root.join("module.json");
        fs::write(&graph, "before").unwrap();
        let mut transaction = PackageTransaction::begin(&root, "failure-test").unwrap();
        transaction.protect(&graph).unwrap();
        fs::write(&graph, "after").unwrap();

        let failure = inject_failure_at("commit.before");
        assert!(transaction.commit().is_err());
        drop(failure);

        assert_eq!(fs::read_to_string(&graph).unwrap(), "before");
        assert!(!root.join(JOURNAL_FILE).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn injected_failures_restore_each_live_mutation_boundary() {
        for (index, point) in [
            "replace.after_remove",
            "replace.after_rename",
            "remove.after",
            "package.write.after",
            "store.write.after",
            "store.activate.after",
        ]
        .into_iter()
        .enumerate()
        {
            let root = temp_dir(&format!("failure-{index}"));
            let graph = root.join("module.json");
            let modules = root.join("modules");
            let module = modules.join("widget");
            fs::create_dir_all(&module).unwrap();
            fs::write(&graph, "before").unwrap();
            fs::write(module.join("version"), "old").unwrap();

            let mut transaction = PackageTransaction::begin(&root, "failure-test").unwrap();
            transaction.protect_package_state(&graph, &modules).unwrap();
            let staged = transaction.staging_dir().join("widget");
            fs::create_dir_all(&staged).unwrap();
            fs::write(staged.join("version"), "new").unwrap();

            let failure = inject_failure_at(point);
            let result = match point {
                "replace.after_remove" | "replace.after_rename" => {
                    transaction.replace_with(&module, &staged)
                }
                "remove.after" => transaction.remove(&module),
                "package.write.after" => super::super::profile::atomic_write(&graph, b"after"),
                "store.write.after" | "store.activate.after" => {
                    let mut lock = super::super::MeshLock::new();
                    lock.save_with_store(
                        &root.join("mesh.lock"),
                        &modules,
                        &root.join(".mesh-store"),
                    )
                }
                _ => unreachable!(),
            };
            assert!(result.is_err(), "failure point {point} did not fail");
            drop(failure);
            drop(transaction);

            assert_eq!(fs::read_to_string(&graph).unwrap(), "before");
            assert_eq!(fs::read_to_string(module.join("version")).unwrap(), "old");
            if point == "store.activate.after" {
                assert!(!root.join(".mesh-store/active-generation").exists());
            }
            assert!(!root.join(JOURNAL_FILE).exists());
            assert!(
                fs::read_dir(&root)
                    .unwrap()
                    .filter_map(Result::ok)
                    .all(|entry| !entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(WORKSPACE_PREFIX))
            );
            make_test_tree_writable(&root);
            fs::remove_dir_all(root).unwrap();
        }
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

    #[test]
    fn removal_rejects_traversal_before_deleting_anything() {
        let root = temp_dir("remove-traversal");
        let modules = root.join("modules");
        let outside = root.join("outside");
        fs::create_dir_all(&modules).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let sentinel = outside.join("keep.txt");
        fs::write(&sentinel, "keep").unwrap();

        let traversal = modules.join("..").join("outside");
        let mut transaction = PackageTransaction::begin(&root, "uninstall").unwrap();
        let error = transaction.remove(&traversal).unwrap_err();

        assert!(error.to_string().contains("contained"));
        assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep");
        drop(transaction);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn removal_deletes_a_valid_contained_module_tree() {
        let root = temp_dir("remove-valid");
        let modules = root.join("modules");
        let module = modules.join("module");
        fs::create_dir_all(module.join("src")).unwrap();
        fs::write(module.join("module.json"), "{}").unwrap();
        fs::write(module.join("src/main.mesh"), "<text />").unwrap();

        let mut transaction = PackageTransaction::begin(&root, "uninstall").unwrap();
        transaction.remove(&module).unwrap();

        assert!(!module.exists());
        transaction.commit().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn removal_rejects_nested_symlink_before_recursive_deletion() {
        let root = temp_dir("remove-symlink");
        let modules = root.join("modules");
        let outside = root.join("outside");
        let module = modules.join("module");
        fs::create_dir_all(&module).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let sentinel = outside.join("keep.txt");
        fs::write(&sentinel, "keep").unwrap();
        std::os::unix::fs::symlink(&outside, module.join("escape")).unwrap();

        let mut transaction = PackageTransaction::begin(&root, "uninstall").unwrap();
        let error = transaction.remove(&module).unwrap_err();

        assert!(error.to_string().contains("symlink"));
        assert!(module.exists());
        assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep");
        drop(transaction);
        fs::remove_dir_all(root).unwrap();
    }
}
