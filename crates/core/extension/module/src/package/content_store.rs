//! Immutable module objects and activation snapshots.
//!
//! Installed module directories are still kept as the editable authoring
//! surface.  Package mutations also publish each validated tree into this
//! store, however, so a generation can refer to an exact immutable object
//! instead of relying on a mutable checkout remaining unchanged.

use super::{
    MeshLock, ModuleId, ModuleManifest, ModuleManifestError, module_install_path,
    module_tree_digest,
};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const OBJECTS_DIR: &str = "objects";
const SHA256_DIR: &str = "sha256";
const ACTIVATIONS_DIR: &str = "activations";
const ACTIVE_GENERATION: &str = "active-generation";
const STAGING_DIR: &str = "staging";
const SNAPSHOT_FILE: &str = "snapshot.json";
const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// A SHA-256-addressed, immutable collection of validated module trees.
#[derive(Debug, Clone)]
pub struct ModuleStore {
    root: PathBuf,
}

impl ModuleStore {
    /// Open (and create) a module store below `root`.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, ModuleManifestError> {
        let root = root.into();
        ensure_real_directory(&root, "module store")?;
        let store = Self { root };
        store.ensure_layout()?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Publish one source tree as an immutable content-addressed object.
    ///
    /// Existing objects are verified and reused.  A source tree is copied to
    /// a private staging directory, checked again, atomically renamed into its
    /// digest path, and only then made read-only.
    pub fn publish(&self, source: &Path) -> Result<StoredModule, ModuleManifestError> {
        super::validate_module_tree(source)?;
        let manifest = ModuleManifest::from_path(&source.join("module.json"))?;
        let digest = ContentDigest::parse(&module_tree_digest(source)?)?;
        let object = self.object_path(&digest)?;

        if object.exists() {
            self.verify_object(&object, &digest)?;
            return Ok(StoredModule {
                module_id: ModuleId::parse(&manifest.name)?,
                digest,
                path: object,
            });
        }

        let staging = self.staging_path("object")?;
        copy_tree(source, &staging)?;
        let staged_digest = ContentDigest::parse(&module_tree_digest(&staging)?)?;
        if staged_digest != digest {
            let _ = fs::remove_dir_all(&staging);
            return Err(ModuleManifestError::Validation(format!(
                "module object changed while staging {}",
                source.display()
            )));
        }

        let rename_result = fs::rename(&staging, &object);
        if let Err(source_error) = rename_result {
            let _ = fs::remove_dir_all(&staging);
            if !object.exists() {
                return Err(ModuleManifestError::Io {
                    path: object,
                    source: source_error,
                });
            }
        }
        make_immutable(&object)?;
        self.verify_object(&object, &digest)?;

        Ok(StoredModule {
            module_id: ModuleId::parse(&manifest.name)?,
            digest,
            path: object,
        })
    }

    /// Return the immutable object path for a digest after verifying it.
    pub fn object(&self, digest: &ContentDigest) -> Result<PathBuf, ModuleManifestError> {
        let path = self.object_path(digest)?;
        self.verify_object(&path, digest)?;
        Ok(path)
    }

    /// Build a snapshot from the lock's installed trees.  Every lock digest
    /// must already match the source tree; edited source is never silently
    /// promoted into an activation generation.
    pub fn snapshot_from_lock(
        &self,
        generation: u64,
        lock: &MeshLock,
        modules_dir: &Path,
    ) -> Result<ActivationSnapshot, ModuleManifestError> {
        if generation == 0 {
            return Err(ModuleManifestError::Validation(
                "activation snapshot generation must be greater than zero".into(),
            ));
        }
        let mut modules = std::collections::BTreeMap::new();
        for (module_id, locked) in &lock.modules {
            let id = ModuleId::parse(module_id)?;
            let expected = ContentDigest::parse(&locked.digest)?;
            let stored = {
                let object = self.object_path(&expected)?;
                if object.exists() {
                    self.verify_object(&object, &expected)?;
                    StoredModule {
                        module_id: id.clone(),
                        digest: expected.clone(),
                        path: object,
                    }
                } else {
                    let installed = module_install_path(modules_dir, module_id)?;
                    if !installed.is_dir() {
                        return Err(ModuleManifestError::Validation(format!(
                            "locked module {module_id} is missing at {}",
                            installed.display()
                        )));
                    }
                    self.publish(&installed)?
                }
            };
            if stored.digest != expected {
                return Err(ModuleManifestError::Validation(format!(
                    "locked module {module_id} digest {} does not match installed content {}",
                    locked.digest, stored.digest
                )));
            }
            modules.insert(
                id.as_str().to_string(),
                ActivationModule {
                    digest: stored.digest,
                    version: locked.version.clone(),
                },
            );
        }
        ActivationSnapshot::new(generation, modules, lock.composition.clone())
    }

    /// Publish a generation snapshot. Re-publishing a generation is allowed
    /// only when the bytes are identical, making snapshots immutable history.
    pub fn publish_snapshot(
        &self,
        snapshot: &ActivationSnapshot,
    ) -> Result<PathBuf, ModuleManifestError> {
        snapshot.validate()?;
        for module in snapshot.modules.values() {
            self.object(&module.digest)?;
        }

        let generation_dir = self.activation_generation_path(snapshot.generation)?;
        let snapshot_path = generation_dir.join(SNAPSHOT_FILE);
        if snapshot_path.exists() {
            super::validate_regular_file(&snapshot_path, "activation snapshot")?;
            let content =
                fs::read_to_string(&snapshot_path).map_err(|source| ModuleManifestError::Io {
                    path: snapshot_path.clone(),
                    source,
                })?;
            let existing: ActivationSnapshot =
                serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                    path: snapshot_path.clone(),
                    source,
                })?;
            existing.validate()?;
            if existing != *snapshot {
                return Err(ModuleManifestError::Validation(format!(
                    "activation generation {} is immutable and already contains a different snapshot",
                    snapshot.generation
                )));
            }
            return Ok(snapshot_path);
        }

        let staging = self.staging_path("activation")?;
        fs::create_dir_all(&staging).map_err(|source| ModuleManifestError::Io {
            path: staging.clone(),
            source,
        })?;
        let content =
            serde_json::to_vec_pretty(snapshot).map_err(|source| ModuleManifestError::Json {
                path: staging.join(SNAPSHOT_FILE),
                source,
            })?;
        write_new_file(&staging.join(SNAPSHOT_FILE), &content)?;
        make_immutable(&staging)?;
        if let Err(source) = fs::rename(&staging, &generation_dir) {
            let _ = fs::remove_dir_all(&staging);
            if !generation_dir.exists() {
                return Err(ModuleManifestError::Io {
                    path: generation_dir,
                    source,
                });
            }
        }
        super::validate_regular_file(&snapshot_path, "activation snapshot")?;
        Ok(snapshot_path)
    }

    /// Mark a published generation as the durable active generation.
    pub fn activate_generation(&self, generation: u64) -> Result<(), ModuleManifestError> {
        let snapshot = self.activation_snapshot(generation)?;
        let content = format!("{}\n", snapshot.generation);
        write_atomic(&self.active_generation_path(), content.as_bytes())
    }

    pub fn active_snapshot(&self) -> Result<Option<ActivationSnapshot>, ModuleManifestError> {
        let path = self.active_generation_path();
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(ModuleManifestError::Io { path, source });
            }
        };
        let generation = content.trim().parse::<u64>().map_err(|_| {
            ModuleManifestError::Validation(format!(
                "active module generation in {} is not numeric",
                self.active_generation_path().display()
            ))
        })?;
        self.activation_snapshot(generation).map(Some)
    }

    /// Resolve the immutable object directories named by the active
    /// generation. A missing pointer means the installation predates the
    /// store and should use its legacy editable discovery path.
    pub fn active_module_dirs(&self) -> Result<Option<Vec<PathBuf>>, ModuleManifestError> {
        let Some(snapshot) = self.active_snapshot()? else {
            return Ok(None);
        };
        snapshot
            .modules
            .values()
            .map(|module| self.object(&module.digest))
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    pub fn activation_snapshot(
        &self,
        generation: u64,
    ) -> Result<ActivationSnapshot, ModuleManifestError> {
        let path = self
            .activation_generation_path(generation)?
            .join(SNAPSHOT_FILE);
        super::validate_regular_file(&path, "activation snapshot")?;
        let content = fs::read_to_string(&path).map_err(|source| ModuleManifestError::Io {
            path: path.clone(),
            source,
        })?;
        let snapshot: ActivationSnapshot =
            serde_json::from_str(&content).map_err(|source| ModuleManifestError::Json {
                path: path.clone(),
                source,
            })?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Materialize a snapshot into a new read-only module directory. The
    /// caller owns the directory lifecycle; this method never removes an
    /// existing path.
    pub fn materialize_snapshot(
        &self,
        snapshot: &ActivationSnapshot,
        destination: &Path,
    ) -> Result<(), ModuleManifestError> {
        snapshot.validate()?;
        if destination.exists() {
            return Err(ModuleManifestError::Validation(format!(
                "activation materialization destination {} already exists",
                destination.display()
            )));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| ModuleManifestError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::create_dir_all(destination).map_err(|source| ModuleManifestError::Io {
            path: destination.to_path_buf(),
            source,
        })?;
        for (module_id, module) in &snapshot.modules {
            let source = self.object(&module.digest)?;
            let module_id = ModuleId::parse(module_id)?;
            let target = super::contained_path(
                destination,
                module_id.relative_path(),
                "activation module path",
            )?;
            copy_tree(&source, &target)?;
        }
        make_immutable(destination)?;
        Ok(())
    }

    fn ensure_layout(&self) -> Result<(), ModuleManifestError> {
        ensure_real_directory(
            &self.root.join(OBJECTS_DIR),
            "module store objects directory",
        )?;
        ensure_real_directory(
            &self.root.join(format!("{OBJECTS_DIR}/{SHA256_DIR}")),
            "module store object index",
        )?;
        ensure_real_directory(
            &self.root.join(ACTIVATIONS_DIR),
            "module store activations directory",
        )?;
        ensure_real_directory(
            &self.root.join(STAGING_DIR),
            "module store staging directory",
        )?;
        Ok(())
    }

    fn object_path(&self, digest: &ContentDigest) -> Result<PathBuf, ModuleManifestError> {
        super::contained_path(
            &self.root.join(format!("{OBJECTS_DIR}/{SHA256_DIR}")),
            digest.hex(),
            "module object path",
        )
    }

    fn activation_generation_path(&self, generation: u64) -> Result<PathBuf, ModuleManifestError> {
        if generation == 0 {
            return Err(ModuleManifestError::Validation(
                "activation snapshot generation must be greater than zero".into(),
            ));
        }
        super::contained_path(
            &self.root.join(ACTIVATIONS_DIR),
            &generation.to_string(),
            "activation generation path",
        )
    }

    fn active_generation_path(&self) -> PathBuf {
        self.root.join(ACTIVE_GENERATION)
    }

    fn staging_path(&self, label: &str) -> Result<PathBuf, ModuleManifestError> {
        let name = format!(
            "{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        super::contained_path(
            &self.root.join(STAGING_DIR),
            &name,
            "module store staging path",
        )
    }

    fn verify_object(
        &self,
        object: &Path,
        digest: &ContentDigest,
    ) -> Result<(), ModuleManifestError> {
        super::validate_module_tree(object)?;
        let actual = ContentDigest::parse(&module_tree_digest(object)?)?;
        if actual != *digest {
            return Err(ModuleManifestError::Validation(format!(
                "module object {} has digest {}, expected {}",
                object.display(),
                actual,
                digest
            )));
        }
        Ok(())
    }
}

/// The result of publishing a module tree into [`ModuleStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredModule {
    pub module_id: ModuleId,
    pub digest: ContentDigest,
    pub path: PathBuf,
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), ModuleManifestError> {
    let metadata =
        fs::symlink_metadata(source).map_err(|source_error| ModuleManifestError::Io {
            path: source.to_path_buf(),
            source: source_error,
        })?;
    if metadata.file_type().is_symlink() {
        return Err(ModuleManifestError::Validation(format!(
            "module store source {} must not contain symlinks",
            source.display()
        )));
    }
    if !metadata.is_dir() {
        return Err(ModuleManifestError::Validation(format!(
            "module store source {} must be a directory",
            source.display()
        )));
    }
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
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | "node_modules" | "target")) {
            continue;
        }
        let child_source = entry.path();
        let child_destination = destination.join(&name);
        let metadata = fs::symlink_metadata(&child_source).map_err(|source_error| {
            ModuleManifestError::Io {
                path: child_source.clone(),
                source: source_error,
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ModuleManifestError::Validation(format!(
                "module store source {} contains unsupported symlink",
                child_source.display()
            )));
        }
        if metadata.is_dir() {
            copy_tree(&child_source, &child_destination)?;
        } else if metadata.is_file() {
            fs::copy(&child_source, &child_destination).map_err(|source_error| {
                ModuleManifestError::Io {
                    path: child_destination.clone(),
                    source: source_error,
                }
            })?;
            fs::set_permissions(&child_destination, metadata.permissions()).map_err(
                |source_error| ModuleManifestError::Io {
                    path: child_destination,
                    source: source_error,
                },
            )?;
        } else {
            return Err(ModuleManifestError::Validation(format!(
                "module store source {} contains unsupported file type",
                child_source.display()
            )));
        }
    }
    fs::set_permissions(destination, metadata.permissions()).map_err(|source_error| {
        ModuleManifestError::Io {
            path: destination.to_path_buf(),
            source: source_error,
        }
    })?;
    Ok(())
}

fn ensure_real_directory(path: &Path, label: &str) -> Result<(), ModuleManifestError> {
    super::validate_no_symlink_path(path, label)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(ModuleManifestError::Validation(format!(
                "{label} {} must be a real directory",
                path.display()
            )))
        }
        Ok(_) => super::validate_no_symlink_path(path, label),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|source| ModuleManifestError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            super::validate_no_symlink_path(path, label)
        }
        Err(source) => Err(ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn make_immutable(path: &Path) -> Result<(), ModuleManifestError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| ModuleManifestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(ModuleManifestError::Validation(format!(
            "immutable module path {} must not be a symlink",
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
            make_immutable(&entry.path())?;
        }
    }
    let mut permissions = metadata.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|source| ModuleManifestError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_new_file(path: &Path, content: &[u8]) -> Result<(), ModuleManifestError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(content)
        .map_err(|source| ModuleManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| ModuleManifestError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<(), ModuleManifestError> {
    super::validate_no_symlink_path(path, "module store write target")?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
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

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ContentDigest(String);

impl std::fmt::Display for ContentDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl ContentDigest {
    pub fn parse(value: &str) -> Result<Self, ModuleManifestError> {
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(ModuleManifestError::Validation(format!(
                "content digest '{value}' must use the sha256:<hex> form"
            )));
        };
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ModuleManifestError::Validation(format!(
                "content digest '{value}' must contain exactly 64 hexadecimal characters"
            )));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn hex(&self) -> &str {
        &self.0[7..]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActivationModule {
    pub digest: ContentDigest,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActivationSnapshot {
    pub schema_version: u32,
    pub generation: u64,
    pub modules: std::collections::BTreeMap<String, ActivationModule>,
    pub composition: Option<super::LockedComposition>,
}

impl ActivationSnapshot {
    pub fn new(
        generation: u64,
        modules: std::collections::BTreeMap<String, ActivationModule>,
        composition: Option<super::LockedComposition>,
    ) -> Result<Self, ModuleManifestError> {
        let snapshot = Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            generation,
            modules,
            composition,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<(), ModuleManifestError> {
        if self.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(ModuleManifestError::Validation(format!(
                "unsupported activation snapshot schemaVersion {}; supported version is {SNAPSHOT_SCHEMA_VERSION}",
                self.schema_version
            )));
        }
        if self.generation == 0 {
            return Err(ModuleManifestError::Validation(
                "activation snapshot generation must be greater than zero".into(),
            ));
        }
        for (module_id, module) in &self.modules {
            ModuleId::parse(module_id)?;
            ContentDigest::parse(module.digest.as_str())?;
            if module.version.trim().is_empty() {
                return Err(ModuleManifestError::Validation(format!(
                    "activation snapshot module {} has an empty version",
                    module_id.as_str()
                )));
            }
        }
        if let Some(composition) = &self.composition {
            ModuleId::parse(&composition.module)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mesh-content-store-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn module(root: &Path, body: &str) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("module.json"),
            r#"{"name":"@me/example","version":"1.0.0","mesh":{"apiVersion":"0.1","kind":"component","entry":"src/main.mesh"}}"#,
        )
        .unwrap();
        fs::write(root.join("src/main.mesh"), body).unwrap();
    }

    #[test]
    fn content_addressed_objects_are_reused_and_read_only() {
        let root = temp_dir("objects");
        let source = root.join("source");
        module(&source, "<template><box/></template>");
        let store = ModuleStore::new(root.join("store")).unwrap();

        let first = store.publish(&source).unwrap();
        let second = store.publish(&source).unwrap();
        assert_eq!(first, second);
        assert!(first.path.join("module.json").is_file());
        assert!(fs::metadata(&first.path).unwrap().permissions().readonly());
        assert_eq!(store.object(&first.digest).unwrap(), first.path);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshots_are_generation_stamped_and_immutable() {
        let root = temp_dir("snapshots");
        let source = root.join("source");
        module(&source, "<template><box/></template>");
        let store = ModuleStore::new(root.join("store")).unwrap();
        let stored = store.publish(&source).unwrap();
        let snapshot = ActivationSnapshot::new(
            7,
            BTreeMap::from([(
                stored.module_id.as_str().to_string(),
                ActivationModule {
                    digest: stored.digest.clone(),
                    version: "1.0.0".into(),
                },
            )]),
            None,
        )
        .unwrap();

        let path = store.publish_snapshot(&snapshot).unwrap();
        assert_eq!(store.activation_snapshot(7).unwrap(), snapshot);
        store.activate_generation(7).unwrap();
        assert_eq!(store.active_snapshot().unwrap(), Some(snapshot.clone()));
        assert!(fs::metadata(path).unwrap().permissions().readonly());

        let changed = ActivationSnapshot::new(7, BTreeMap::new(), None).unwrap();
        assert!(store.publish_snapshot(&changed).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn materialized_snapshot_is_contained_and_read_only() {
        let root = temp_dir("materialize");
        let source = root.join("source");
        module(&source, "<template><box/></template>");
        let store = ModuleStore::new(root.join("store")).unwrap();
        let stored = store.publish(&source).unwrap();
        let snapshot = ActivationSnapshot::new(
            1,
            BTreeMap::from([(
                stored.module_id.as_str().to_string(),
                ActivationModule {
                    digest: stored.digest,
                    version: "1.0.0".into(),
                },
            )]),
            None,
        )
        .unwrap();
        let destination = root.join("activation");
        store.materialize_snapshot(&snapshot, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("me/example/module.json")).unwrap(),
            fs::read_to_string(source.join("module.json")).unwrap()
        );
        assert!(fs::metadata(destination).unwrap().permissions().readonly());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_digests_are_rejected_before_path_use() {
        for value in ["deadbeef", "sha256:deadbeef", "sha256:GG"] {
            assert!(ContentDigest::parse(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn lock_commit_publishes_and_activates_the_matching_generation() {
        let root = temp_dir("lock-generation");
        let modules_dir = root.join("modules").join("me/example");
        module(&modules_dir, "<template><box/></template>");
        let digest = module_tree_digest(&modules_dir).unwrap();
        let mut lock = MeshLock::new();
        lock.modules.insert(
            "@me/example".into(),
            super::super::LockedModule {
                version: "1.0.0".into(),
                source: super::super::ModuleSource::Path {
                    path: "modules/me/example".into(),
                },
                revision: None,
                digest,
                trust: Default::default(),
                signature: None,
                dependencies: Default::default(),
                requested_by: Default::default(),
            },
        );

        let lock_path = root.join("mesh.lock");
        let store_root = root.join(".mesh-store");
        lock.save_with_store(&lock_path, &root.join("modules"), &store_root)
            .unwrap();

        let store = ModuleStore::new(store_root).unwrap();
        let active = store.active_snapshot().unwrap().unwrap();
        assert_eq!(active.generation, 1);
        assert!(active.modules.contains_key("@me/example"));
        assert_eq!(MeshLock::from_path(&lock_path).unwrap().generation, 1);

        fs::remove_dir_all(root).unwrap();
    }
}
