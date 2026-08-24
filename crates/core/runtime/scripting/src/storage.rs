use mesh_core_runtime::SandboxConfig;
use serde_json::{Map, Value};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue, Variadic};
use sha2::{Digest, Sha256};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const STORAGE_SCHEMA_VERSION: u64 = 1;
const STORAGE_SCHEMA_KEY: &str = "schemaVersion";
const STORAGE_REVISION_KEY: &str = "revision";
const STORAGE_DATA_KEY: &str = "data";
const MAX_STORAGE_KEY_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageKind {
    Frontend,
    Backend,
}

impl StorageKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Frontend => "frontend",
            Self::Backend => "backend",
        }
    }
}

impl fmt::Display for StorageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageScope {
    module_id: String,
    owner_id: String,
    kind: StorageKind,
    instance_id: String,
}

impl StorageScope {
    pub fn frontend(
        module_id: impl Into<String>,
        component_id: impl Into<String>,
        instance_id: impl Into<String>,
    ) -> Self {
        Self {
            module_id: module_id.into(),
            owner_id: component_id.into(),
            kind: StorageKind::Frontend,
            instance_id: instance_id.into(),
        }
    }

    pub fn backend(
        module_id: impl Into<String>,
        provider_id: impl Into<String>,
        instance_id: impl Into<String>,
    ) -> Self {
        Self {
            module_id: module_id.into(),
            owner_id: provider_id.into(),
            kind: StorageKind::Backend,
            instance_id: instance_id.into(),
        }
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    pub fn kind(&self) -> &StorageKind {
        &self.kind
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageDiagnostic {
    pub scope: StorageScope,
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct StorageManager {
    root: PathBuf,
    max_document_bytes: u64,
}

impl StorageManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::new_with_limit(root, SandboxConfig::default().storage_budget)
    }

    pub fn new_with_limit(root: impl Into<PathBuf>, max_document_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_document_bytes,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path_for_scope(&self, scope: &StorageScope) -> PathBuf {
        self.root
            .join("storage")
            .join("v1")
            .join(scope.kind.as_str())
            .join(scope_segment(&scope.module_id))
            .join(scope_segment(&scope.owner_id))
            .join(format!("{}.json", scope_segment(&scope.instance_id)))
    }

    pub fn open(&self, scope: StorageScope) -> ScopedStorage {
        let path = self.path_for_scope(&scope);
        let mut diagnostics = Vec::new();
        let (mut document, revision) = match ensure_storage_parent(&self.root, path.parent())
            .and_then(|_| StorageLock::acquire(&lock_path(&path)))
        {
            Ok(_lock) => match load_or_recover(&path, self.max_document_bytes) {
                Ok(loaded) => {
                    diagnostics.extend(loaded.diagnostics.into_iter().map(|reason| {
                        StorageDiagnostic {
                            scope: scope.clone(),
                            path: path.clone(),
                            reason,
                        }
                    }));
                    (loaded.document, loaded.revision)
                }
                Err(error) => {
                    diagnostics.push(StorageDiagnostic {
                        scope: scope.clone(),
                        path: path.clone(),
                        reason: format!("storage document could not be read: {error}"),
                    });
                    (Map::new(), 0)
                }
            },
            Err(error) => {
                diagnostics.push(StorageDiagnostic {
                    scope: scope.clone(),
                    path: path.clone(),
                    reason: format!("storage directory or lock is not secure: {error}"),
                });
                (Map::new(), 0)
            }
        };

        let oversized_keys = document
            .keys()
            .filter(|key| key.len() > MAX_STORAGE_KEY_BYTES)
            .count();
        if oversized_keys > 0 {
            document.retain(|key, _| key.len() <= MAX_STORAGE_KEY_BYTES);
            diagnostics.push(StorageDiagnostic {
                scope: scope.clone(),
                path: path.clone(),
                reason: format!(
                    "storage document discarded {oversized_keys} key(s) exceeding {MAX_STORAGE_KEY_BYTES} bytes"
                ),
            });
        }

        if serialized_document(&document, revision)
            .map(|bytes| bytes.len() as u64 > self.max_document_bytes)
            .unwrap_or(true)
        {
            diagnostics.push(StorageDiagnostic {
                scope: scope.clone(),
                path: path.clone(),
                reason: format!(
                    "storage document exceeds {} byte budget",
                    self.max_document_bytes
                ),
            });
            document = Map::new();
        }

        ScopedStorage {
            root: self.root.clone(),
            scope,
            path,
            revision,
            document,
            diagnostics,
            dirty: false,
            max_document_bytes: self.max_document_bytes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScopedStorage {
    root: PathBuf,
    scope: StorageScope,
    path: PathBuf,
    revision: u64,
    document: Map<String, Value>,
    diagnostics: Vec<StorageDiagnostic>,
    dirty: bool,
    max_document_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    #[error("storage document exceeds {limit} byte budget (attempted {attempted} bytes)")]
    QuotaExceeded { limit: u64, attempted: u64 },
    #[error("storage key exceeds {limit} byte budget (attempted {attempted} bytes)")]
    KeyTooLarge { limit: usize, attempted: usize },
    #[error("storage revision conflict (expected {expected}, found {actual})")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("storage I/O error: {0}")]
    Io(String),
    #[error("storage value could not be serialized: {0}")]
    Serialization(String),
}

impl ScopedStorage {
    pub fn scope(&self) -> &StorageScope {
        &self.scope
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn diagnostics(&self) -> &[StorageDiagnostic] {
        &self.diagnostics
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.document.get(key)
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.try_set(key, value).ok().flatten()
    }

    pub fn try_set(
        &mut self,
        key: impl Into<String>,
        value: Value,
    ) -> Result<Option<Value>, StorageError> {
        let key = key.into();
        if key.len() > MAX_STORAGE_KEY_BYTES {
            return Err(StorageError::KeyTooLarge {
                limit: MAX_STORAGE_KEY_BYTES,
                attempted: key.len(),
            });
        }
        let mut candidate = self.document.clone();
        let previous = candidate.insert(key.clone(), value);
        let attempted = serialized_document(&candidate, self.revision.saturating_add(1))
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        if attempted.len() as u64 > self.max_document_bytes {
            return Err(StorageError::QuotaExceeded {
                limit: self.max_document_bytes,
                attempted: attempted.len() as u64,
            });
        }
        self.document = candidate;
        self.dirty = true;
        Ok(previous)
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let previous = self.document.remove(key);
        if previous.is_some() {
            self.dirty = true;
        }
        previous
    }

    pub fn clear(&mut self) {
        if !self.document.is_empty() {
            self.dirty = true;
        }
        self.document.clear();
    }

    pub fn snapshot(&self) -> Value {
        Value::Object(self.document.clone())
    }

    pub fn persist(&mut self) -> Result<(), StorageError> {
        ensure_storage_parent(&self.root, self.path.parent()).map_err(storage_io)?;
        let _lock = StorageLock::acquire(&lock_path(&self.path)).map_err(storage_io)?;
        let loaded = load_or_recover(&self.path, self.max_document_bytes).map_err(storage_io)?;
        for reason in loaded.diagnostics {
            self.diagnostics.push(StorageDiagnostic {
                scope: self.scope.clone(),
                path: self.path.clone(),
                reason,
            });
        }
        if loaded.revision != self.revision {
            return Err(StorageError::RevisionConflict {
                expected: self.revision,
                actual: loaded.revision,
            });
        }

        let next_revision = self.revision.saturating_add(1);
        let bytes = serialized_document(&self.document, next_revision)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        if bytes.len() as u64 > self.max_document_bytes {
            return Err(StorageError::QuotaExceeded {
                limit: self.max_document_bytes,
                attempted: bytes.len() as u64,
            });
        }
        let (temp_path, mut file) = create_storage_temporary(&self.path).map_err(storage_io)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);

            if fs::symlink_metadata(&self.path).is_ok() {
                ensure_secure_file(&self.path)?;
                fs::rename(&self.path, backup_path(&self.path))?;
            }
            fs::rename(&temp_path, &self.path)?;
            sync_storage_directory(self.path.parent().unwrap_or_else(|| Path::new(".")))
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temp_path);
            return Err(storage_io(error));
        }
        self.revision = next_revision;
        self.dirty = false;
        Ok(())
    }

    pub fn flush_if_dirty(&mut self) -> Result<bool, StorageError> {
        if !self.dirty {
            return Ok(false);
        }
        self.persist()?;
        Ok(true)
    }
}

#[derive(Debug)]
struct LoadedStorage {
    document: Map<String, Value>,
    revision: u64,
    diagnostics: Vec<String>,
}

#[derive(Debug)]
struct PersistedStorage {
    document: Map<String, Value>,
    revision: u64,
}

#[derive(Debug)]
struct StorageLock {
    file: File,
}

impl StorageLock {
    fn acquire(path: &Path) -> io::Result<Self> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(io::Error::other(format!(
                        "storage lock is not a regular file: {}",
                        path.display()
                    )));
                }
                ensure_owned(&metadata, path)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.mode(0o600);
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        ensure_secure_file(path)?;
        #[cfg(unix)]
        {
            let result =
                unsafe { libc::flock(std::os::fd::AsRawFd::as_raw_fd(&file), libc::LOCK_EX) };
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self { file })
    }
}

impl Drop for StorageLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            let _ = libc::flock(std::os::fd::AsRawFd::as_raw_fd(&self.file), libc::LOCK_UN);
        }
    }
}

fn storage_io(error: io::Error) -> StorageError {
    StorageError::Io(error.to_string())
}

fn serialized_document(
    document: &Map<String, Value>,
    revision: u64,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(&serde_json::json!({
        STORAGE_SCHEMA_KEY: STORAGE_SCHEMA_VERSION,
        STORAGE_REVISION_KEY: revision,
        STORAGE_DATA_KEY: Value::Object(document.clone()),
    }))
}

fn decode_document(bytes: &[u8]) -> Result<PersistedStorage, String> {
    let value = serde_json::from_slice::<Value>(bytes).map_err(|error| error.to_string())?;
    let Value::Object(mut map) = value else {
        return Err("storage document root is not a JSON object".to_string());
    };

    let is_envelope = map
        .get(STORAGE_SCHEMA_KEY)
        .and_then(Value::as_u64)
        .is_some_and(|version| version == STORAGE_SCHEMA_VERSION)
        && map.contains_key(STORAGE_DATA_KEY);
    if !is_envelope {
        return Ok(PersistedStorage {
            document: map,
            revision: 0,
        });
    }

    let revision = map
        .remove(STORAGE_REVISION_KEY)
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "storage envelope has no valid revision".to_string())?;
    let data = map
        .remove(STORAGE_DATA_KEY)
        .ok_or_else(|| "storage envelope has no data object".to_string())?;
    let Value::Object(document) = data else {
        return Err("storage envelope data is not a JSON object".to_string());
    };
    Ok(PersistedStorage { document, revision })
}

fn load_or_recover(path: &Path, max_document_bytes: u64) -> io::Result<LoadedStorage> {
    match read_storage_file(path, max_document_bytes) {
        Ok(Some(document)) => Ok(LoadedStorage {
            document: document.document,
            revision: document.revision,
            diagnostics: Vec::new(),
        }),
        Ok(None) => recover_missing_storage(path, max_document_bytes),
        Err(primary_error) => {
            let mut recovered = recover_missing_storage(path, max_document_bytes)?;
            if recovered.revision == 0 && recovered.document.is_empty() {
                recovered.diagnostics.insert(
                    0,
                    format!("primary storage document was unusable: {primary_error}"),
                );
            } else {
                recovered.diagnostics.insert(
                    0,
                    format!("storage document recovered after primary failure: {primary_error}"),
                );
            }
            Ok(recovered)
        }
    }
}

fn recover_missing_storage(path: &Path, max_document_bytes: u64) -> io::Result<LoadedStorage> {
    let backup = backup_path(path);
    if let Ok(Some(document)) = read_storage_file(&backup, max_document_bytes) {
        let mut diagnostics = vec!["storage document recovered from its backup".to_string()];
        if fs::symlink_metadata(path).is_ok() {
            let corrupt = corrupt_path(path);
            if let Err(error) = fs::rename(path, &corrupt) {
                diagnostics.push(format!(
                    "could not preserve the damaged primary storage document: {error}"
                ));
            }
        }
        fs::rename(&backup, path)?;
        sync_storage_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
        return Ok(LoadedStorage {
            document: document.document,
            revision: document.revision,
            diagnostics,
        });
    }

    if let Some((temporary, document)) = recover_temporary(path, max_document_bytes)? {
        fs::rename(&temporary, path)?;
        sync_storage_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
        return Ok(LoadedStorage {
            document: document.document,
            revision: document.revision,
            diagnostics: vec!["storage document recovered from an interrupted write".to_string()],
        });
    }

    if fs::symlink_metadata(path).is_ok() {
        let corrupt = corrupt_path(path);
        fs::rename(path, &corrupt)?;
    }

    Ok(LoadedStorage {
        document: Map::new(),
        revision: 0,
        diagnostics: Vec::new(),
    })
}

fn recover_temporary(
    path: &Path,
    max_document_bytes: u64,
) -> io::Result<Option<(PathBuf, PersistedStorage)>> {
    let Some(parent) = path.parent() else {
        return Ok(None);
    };
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let prefix = format!(".{file_name}.tmp-");
    let mut candidates = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let candidate = entry.path();
        if !entry.file_name().to_string_lossy().starts_with(&prefix) {
            continue;
        }
        if let Ok(Some(document)) = read_storage_file(&candidate, max_document_bytes) {
            candidates.push((candidate, document));
        }
    }
    Ok(candidates
        .into_iter()
        .max_by_key(|(_, document)| document.revision))
}

fn read_storage_file(path: &Path, max_document_bytes: u64) -> io::Result<Option<PersistedStorage>> {
    let Some(metadata) = secure_existing_file(path)? else {
        return Ok(None);
    };
    if metadata.len() > max_document_bytes {
        return Err(io::Error::other(format!(
            "storage document exceeds {max_document_bytes} byte budget"
        )));
    }
    let bytes = fs::read(path)?;
    decode_document(&bytes).map(Some).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("storage document could not be decoded: {error}"),
        )
    })
}

fn ensure_storage_parent(root: &Path, parent: Option<&Path>) -> io::Result<()> {
    let parent = parent.ok_or_else(|| io::Error::other("storage path has no parent"))?;
    ensure_secure_directory(root)?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| io::Error::other("storage path escaped its root"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(io::Error::other(
                "storage path contains a non-normal component",
            ));
        };
        current.push(component);
        ensure_secure_directory(&current)?;
    }
    Ok(())
}

fn ensure_secure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::other(format!(
                    "storage directory is not a real directory: {}",
                    path.display()
                )));
            }
            ensure_owned(&metadata, path)?;
            set_directory_permissions(path)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => create_directory_tree(path),
        Err(error) => Err(error),
    }
}

fn create_directory_tree(path: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut current = path.to_path_buf();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(io::Error::other(format!(
                        "storage directory ancestor is not a real directory: {}",
                        current.display()
                    )));
                }
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(current.clone());
                current = current
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
            }
            Err(error) => return Err(error),
        }
    }

    for directory in missing.into_iter().rev() {
        match fs::create_dir(&directory) {
            Ok(()) => set_directory_permissions(&directory)?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                ensure_secure_directory(&directory)?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn ensure_secure_file(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::other(format!(
            "storage path is not a regular file: {}",
            path.display()
        )));
    }
    ensure_owned(&metadata, path)?;
    set_file_permissions(path)
}

fn secure_existing_file(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure_secure_file(path)?;
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn ensure_owned(metadata: &fs::Metadata, path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let owner = unsafe { libc::geteuid() } as u32;
        if metadata.uid() != owner {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "storage path is not owned by the current user: {}",
                    path.display()
                ),
            ));
        }
    }
    let _ = metadata;
    let _ = path;
    Ok(())
}

fn set_directory_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    let _ = path;
    Ok(())
}

fn set_file_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    let _ = path;
    Ok(())
}

fn create_storage_temporary(path: &Path) -> io::Result<(PathBuf, File)> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("storage path has no parent"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("storage.json");
    for _ in 0..128 {
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{sequence}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "could not allocate a unique storage temporary file in {}",
            parent.display()
        ),
    ))
}

fn sync_storage_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn lock_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("storage.json");
    path.with_file_name(format!(".{file_name}.lock"))
}

fn backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("storage.json");
    path.with_file_name(format!(".{file_name}.bak"))
}

fn corrupt_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("storage.json");
    let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(
        ".{file_name}.corrupt-{}-{sequence}",
        std::process::id()
    ))
}

pub type StorageDiagnosticSink = Arc<dyn Fn(String) + Send + Sync>;
pub type StorageKeySink = Arc<dyn Fn(&str) + Send + Sync>;

pub fn create_lua_storage_table(
    lua: &Lua,
    storage: Arc<Mutex<ScopedStorage>>,
    diagnostic_sink: StorageDiagnosticSink,
    read_sink: StorageKeySink,
    write_sink: StorageKeySink,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let metatable = lua.create_table()?;

    let snapshot_storage = Arc::clone(&storage);
    table.set(
        "snapshot",
        lua.create_function(move |lua, _args: Variadic<LuaValue>| {
            let snapshot = snapshot_storage.lock().unwrap().snapshot();
            lua.to_value(&snapshot)
        })?,
    )?;

    let index_storage = Arc::clone(&storage);
    let index_diagnostics = Arc::clone(&diagnostic_sink);
    metatable.set(
        "__index",
        lua.create_function(move |lua, (_table, key): (Table, LuaValue)| {
            with_storage_key_from_lua(&key, &index_diagnostics, |key| {
                read_sink(key);
                let storage = index_storage.lock().unwrap();
                match storage.get(key) {
                    Some(value) => lua.to_value(value),
                    None => Ok(LuaValue::Nil),
                }
            })?
            .map_or(Ok(LuaValue::Nil), Ok)
        })?,
    )?;

    let newindex_storage = Arc::clone(&storage);
    metatable.set(
        "__newindex",
        lua.create_function(
            move |lua, (_table, key, value): (Table, LuaValue, LuaValue)| {
                with_storage_key_from_lua(&key, &diagnostic_sink, |key| {
                    if matches!(value, LuaValue::Nil) {
                        newindex_storage.lock().unwrap().remove(key);
                        write_sink(key);
                        return Ok(());
                    }

                    match lua.from_value::<Value>(value) {
                        Ok(value) => match newindex_storage.lock().unwrap().try_set(key, value) {
                            Ok(_) => write_sink(key),
                            Err(error) => {
                                diagnostic_sink(error.to_string());
                                return Err(mlua::Error::external(error));
                            }
                        },
                        Err(error) => {
                            diagnostic_sink(format!(
                                "unsupported storage value for key '{key}': {error}"
                            ));
                        }
                    }
                    Ok(())
                })?;
                Ok(())
            },
        )?,
    )?;

    table.set_metatable(Some(metatable))?;
    Ok(table)
}

fn with_storage_key_from_lua<R>(
    value: &LuaValue,
    diagnostic_sink: &StorageDiagnosticSink,
    f: impl FnOnce(&str) -> mlua::Result<R>,
) -> mlua::Result<Option<R>> {
    match value {
        LuaValue::String(value) => f(value.to_str()?.as_ref()).map(Some),
        LuaValue::Integer(value) => {
            let key = value.to_string();
            f(&key).map(Some)
        }
        LuaValue::Number(value) => {
            let key = value.to_string();
            f(&key).map(Some)
        }
        _ => {
            diagnostic_sink("storage keys must be strings or numbers".to_string());
            Ok(None)
        }
    }
}

fn scope_segment(raw: &str) -> String {
    let mut readable = String::new();
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            readable.push(character);
        } else {
            readable.push('_');
        }
        if readable.len() >= 48 {
            break;
        }
    }

    let readable = readable.trim_matches('.');
    let readable = if readable.is_empty() {
        "scope"
    } else {
        readable
    };
    let mut digest = Sha256::new();
    digest.update(raw.as_bytes());
    format!("{readable}--{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mesh-storage-test-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn paths_are_deterministic_sanitized_and_scope_isolated() {
        let root = temp_root("paths");
        let manager = StorageManager::new(&root);
        let first = StorageScope::frontend("module/one", "component:main", "instance 1");
        let second = StorageScope::frontend("module/one", "component:main", "instance 2");
        let first_path = manager.path_for_scope(&first);

        assert_eq!(first_path, manager.path_for_scope(&first));
        assert_ne!(first_path, manager.path_for_scope(&second));
        assert!(first_path.starts_with(&root));
        assert!(!first_path.to_string_lossy().contains("module/one"));
        assert!(first_path.to_string_lossy().ends_with(".json"));
    }

    #[test]
    fn document_operations_update_the_in_memory_snapshot() {
        let root = temp_root("ops");
        let manager = StorageManager::new(root);
        let mut storage = manager.open(StorageScope::backend("network", "wifi", "default"));

        assert!(storage.diagnostics().is_empty());
        assert_eq!(storage.get("enabled"), None);

        storage.set("enabled", json!(true));
        storage.set("name", json!("Home"));
        assert_eq!(storage.get("enabled"), Some(&json!(true)));
        assert_eq!(storage.remove("name"), Some(json!("Home")));
        assert_eq!(storage.snapshot(), json!({ "enabled": true }));

        storage.clear();
        assert_eq!(storage.snapshot(), json!({}));
    }

    #[test]
    fn persist_writes_document_that_can_be_reloaded() {
        let root = temp_root("persist");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::frontend("clock", "face", "panel-1");
        let mut storage = manager.open(scope.clone());

        storage.set("timezone", json!("Europe/Bratislava"));
        storage.set("show_seconds", json!(false));
        storage.persist().unwrap();

        let reloaded = manager.open(scope);
        assert!(reloaded.diagnostics().is_empty());
        assert_eq!(
            reloaded.snapshot(),
            json!({ "show_seconds": false, "timezone": "Europe/Bratislava" })
        );
    }

    #[test]
    fn corrupt_document_recovers_with_diagnostic_and_empty_state() {
        let root = temp_root("corrupt");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::backend("audio", "pipewire", "default");
        let path = manager.path_for_scope(&scope);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not-json").unwrap();

        let storage = manager.open(scope);
        assert_eq!(storage.snapshot(), json!({}));
        assert_eq!(storage.diagnostics().len(), 1);
        assert!(
            storage.diagnostics()[0]
                .reason
                .contains("could not be decoded")
        );
    }

    #[test]
    fn persist_replaces_previous_valid_document_atomically() {
        let root = temp_root("atomic");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::backend("theme", "palette", "default");
        let mut storage = manager.open(scope.clone());
        storage.set("version", json!(1));
        storage.persist().unwrap();

        let mut reloaded = manager.open(scope.clone());
        reloaded.set("version", json!(2));
        reloaded.persist().unwrap();

        let final_document = manager.open(scope);
        assert_eq!(final_document.get("version"), Some(&json!(2)));
        assert!(final_document.diagnostics().is_empty());
    }

    #[test]
    fn same_key_is_private_between_scopes() {
        let root = temp_root("private");
        let manager = StorageManager::new(&root);
        let first_scope = StorageScope::frontend("module", "component", "one");
        let second_scope = StorageScope::frontend("module", "component", "two");
        let mut first = manager.open(first_scope.clone());
        let mut second = manager.open(second_scope.clone());

        first.set("value", json!("first"));
        second.set("value", json!("second"));
        first.persist().unwrap();
        second.persist().unwrap();

        assert_eq!(
            manager.open(first_scope).get("value"),
            Some(&json!("first"))
        );
        assert_eq!(
            manager.open(second_scope).get("value"),
            Some(&json!("second"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn storage_paths_use_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("permissions");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::backend("audio", "pipewire", "default");
        let mut storage = manager.open(scope);
        storage.set("volume", json!(42));
        storage.persist().unwrap();

        let path = storage.path().to_path_buf();
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&lock_path(&path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&backup_path(&path)).is_err(),
            true,
            "the first committed document has no previous revision"
        );
    }

    #[test]
    fn storage_creates_missing_parent_directories_securely() {
        let root = temp_root("parent-chain").join("nested").join("runtime");
        let manager = StorageManager::new(&root);
        let mut storage = manager.open(StorageScope::backend("audio", "pipewire", "default"));
        storage.set("volume", json!(42));
        storage.persist().unwrap();

        assert!(root.is_dir());
        assert!(root.join("storage/v1/backend").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn storage_does_not_follow_a_symlinked_primary() {
        use std::os::unix::fs::symlink;

        let root = temp_root("symlink");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::backend("audio", "pipewire", "default");
        let path = manager.path_for_scope(&scope);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let target = root.join("outside.json");
        fs::write(&target, "{\"outside\":true}").unwrap();
        symlink(&target, &path).unwrap();

        let storage = manager.open(scope);
        assert_eq!(storage.snapshot(), json!({}));
        assert!(
            storage
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.reason.contains("not a regular file"))
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"outside\":true}");
        assert!(!path.exists());
    }

    #[test]
    fn stale_writers_are_rejected_by_revision() {
        let root = temp_root("revision");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::backend("network", "wifi", "default");
        let mut first = manager.open(scope.clone());
        let mut second = manager.open(scope.clone());

        first.set("owner", json!("first"));
        first.persist().unwrap();
        assert_eq!(first.revision(), 1);

        second.set("owner", json!("second"));
        let error = second.persist().unwrap_err();
        assert_eq!(
            error,
            StorageError::RevisionConflict {
                expected: 0,
                actual: 1
            }
        );
        assert_eq!(manager.open(scope).get("owner"), Some(&json!("first")));
    }

    #[test]
    fn interrupted_replace_recovers_the_last_committed_revision() {
        let root = temp_root("recovery");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::backend("theme", "palette", "default");
        let mut storage = manager.open(scope.clone());
        storage.set("version", json!(1));
        storage.persist().unwrap();
        storage.set("version", json!(2));
        storage.persist().unwrap();

        let path = manager.path_for_scope(&scope);
        let backup = backup_path(&path);
        fs::remove_file(&backup).unwrap();
        fs::rename(&path, &backup).unwrap();

        let recovered = manager.open(scope);
        assert_eq!(recovered.revision(), 2);
        assert_eq!(recovered.get("version"), Some(&json!(2)));
        assert!(
            recovered
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.reason.contains("recovered from its backup"))
        );
    }

    #[test]
    fn interrupted_write_recovers_a_fully_written_temporary_revision() {
        let root = temp_root("temporary-recovery");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::backend("theme", "palette", "default");
        let path = manager.path_for_scope(&scope);
        let _ = manager.open(scope.clone());
        let (temporary, mut file) = create_storage_temporary(&path).unwrap();
        file.write_all(
            &serialized_document(&Map::from_iter([(String::from("version"), json!(2))]), 2)
                .unwrap(),
        )
        .unwrap();
        file.sync_all().unwrap();
        drop(file);

        let recovered = manager.open(scope);
        assert_eq!(recovered.revision(), 2);
        assert_eq!(recovered.get("version"), Some(&json!(2)));
        assert!(
            recovered
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.reason.contains("interrupted write"))
        );
        assert!(!temporary.exists());
    }

    #[test]
    fn key_and_path_bounds_are_enforced() {
        let root = temp_root("bounds");
        let manager = StorageManager::new_with_limit(&root, 256);
        let long_scope = "x".repeat(4_096);
        let scope = StorageScope::frontend(&long_scope, &long_scope, &long_scope);
        let path = manager.path_for_scope(&scope);
        assert!(path.to_string_lossy().len() < 512);

        let mut storage = manager.open(StorageScope::backend("module", "owner", "default"));
        let error = storage
            .try_set("k".repeat(MAX_STORAGE_KEY_BYTES + 1), json!(true))
            .unwrap_err();
        assert_eq!(
            error,
            StorageError::KeyTooLarge {
                limit: MAX_STORAGE_KEY_BYTES,
                attempted: MAX_STORAGE_KEY_BYTES + 1
            }
        );
    }

    #[test]
    fn legacy_documents_migrate_to_revisioned_envelopes() {
        let root = temp_root("migration");
        let manager = StorageManager::new(&root);
        let scope = StorageScope::frontend("module", "component", "default");
        let path = manager.path_for_scope(&scope);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            serde_json::to_vec(&json!({ "legacy": true })).unwrap(),
        )
        .unwrap();

        let mut storage = manager.open(scope.clone());
        assert_eq!(storage.revision(), 0);
        storage.persist().unwrap();
        let raw: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(raw[STORAGE_SCHEMA_KEY], json!(STORAGE_SCHEMA_VERSION));
        assert_eq!(raw[STORAGE_REVISION_KEY], json!(1));
        assert_eq!(raw[STORAGE_DATA_KEY]["legacy"], json!(true));
    }

    #[test]
    fn lua_snapshot_method_does_not_track_storage_key_read() {
        let lua = Lua::new();
        let root = temp_root("lua-snapshot-method");
        let manager = StorageManager::new(root);
        let mut scoped = manager.open(StorageScope::frontend("module", "component", "one"));
        scoped.set("theme", json!("dark"));
        let reads = Arc::new(Mutex::new(Vec::new()));
        let read_sink = {
            let reads = Arc::clone(&reads);
            Arc::new(move |key: &str| reads.lock().unwrap().push(key.to_string()))
        };
        let storage = create_lua_storage_table(
            &lua,
            Arc::new(Mutex::new(scoped)),
            Arc::new(|_| {}),
            read_sink,
            Arc::new(|_| {}),
        )
        .unwrap();
        lua.globals().set("storage", storage).unwrap();

        let theme: String = lua.load("return storage:snapshot().theme").eval().unwrap();

        assert_eq!(theme, "dark");
        assert!(
            reads.lock().unwrap().is_empty(),
            "looking up the table-owned snapshot method should not be tracked as a storage key read"
        );
    }

    #[test]
    #[ignore = "release-only storage snapshot lookup microbenchmark"]
    fn table_owned_snapshot_method_beats_index_allocated_function() {
        use std::time::Instant;

        fn old_lua_storage_table(lua: &Lua, storage: Arc<Mutex<ScopedStorage>>) -> Table {
            let table = lua.create_table().unwrap();
            let metatable = lua.create_table().unwrap();
            let index_storage = Arc::clone(&storage);
            metatable
                .set(
                    "__index",
                    lua.create_function(move |lua, (_table, key): (Table, LuaValue)| {
                        let key = match key {
                            LuaValue::String(value) => value.to_str()?.to_string(),
                            _ => return Ok(LuaValue::Nil),
                        };
                        if key == "snapshot" {
                            let snapshot_storage = Arc::clone(&index_storage);
                            let snapshot =
                                lua.create_function(move |lua, _args: Variadic<LuaValue>| {
                                    let snapshot = snapshot_storage.lock().unwrap().snapshot();
                                    lua.to_value(&snapshot)
                                })?;
                            return Ok(LuaValue::Function(snapshot));
                        }
                        Ok(LuaValue::Nil)
                    })
                    .unwrap(),
                )
                .unwrap();
            table.set_metatable(Some(metatable)).unwrap();
            table
        }

        fn benchmark_storage(root_name: &str) -> Arc<Mutex<ScopedStorage>> {
            let root = temp_root(root_name);
            let manager = StorageManager::new(root);
            let mut scoped = manager.open(StorageScope::frontend("module", "component", "one"));
            scoped.set("theme", json!("dark"));
            scoped.set("locale", json!("sk"));
            Arc::new(Mutex::new(scoped))
        }

        let iterations = 100_000;
        let old_lua = Lua::new();
        old_lua
            .globals()
            .set(
                "storage",
                old_lua_storage_table(&old_lua, benchmark_storage("snapshot-old")),
            )
            .unwrap();
        let old_started = Instant::now();
        for _ in 0..iterations {
            let theme: String = old_lua
                .load("return storage:snapshot().theme")
                .eval()
                .unwrap();
            std::hint::black_box(theme);
        }
        let old = old_started.elapsed();

        let new_lua = Lua::new();
        let new_table = create_lua_storage_table(
            &new_lua,
            benchmark_storage("snapshot-new"),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
        )
        .unwrap();
        new_lua.globals().set("storage", new_table).unwrap();
        let new_started = Instant::now();
        for _ in 0..iterations {
            let theme: String = new_lua
                .load("return storage:snapshot().theme")
                .eval()
                .unwrap();
            std::hint::black_box(theme);
        }
        let new = new_started.elapsed();

        eprintln!(
            "storage snapshot lookup over {iterations} calls: index-created fn {old:?}, table-owned fn {new:?}, ratio {:.1}x",
            old.as_secs_f64() / new.as_secs_f64()
        );
        assert!(new < old);
    }

    #[test]
    #[ignore = "release-only storage read microbenchmark"]
    fn borrowed_storage_reads_avoid_key_and_value_clone() {
        use std::time::Instant;

        fn old_lua_storage_table(lua: &Lua, storage: Arc<Mutex<ScopedStorage>>) -> Table {
            let table = lua.create_table().unwrap();
            let metatable = lua.create_table().unwrap();
            metatable
                .set(
                    "__index",
                    lua.create_function(move |lua, (_table, key): (Table, LuaValue)| {
                        let key = match key {
                            LuaValue::String(value) => value.to_str()?.to_string(),
                            LuaValue::Integer(value) => value.to_string(),
                            LuaValue::Number(value) => value.to_string(),
                            _ => return Ok(LuaValue::Nil),
                        };
                        let value = storage.lock().unwrap().get(&key).cloned();
                        match value {
                            Some(value) => lua.to_value(&value),
                            None => Ok(LuaValue::Nil),
                        }
                    })
                    .unwrap(),
                )
                .unwrap();
            table.set_metatable(Some(metatable)).unwrap();
            table
        }

        fn benchmark_storage(root_name: &str) -> Arc<Mutex<ScopedStorage>> {
            let root = temp_root(root_name);
            let manager = StorageManager::new(root);
            let mut scoped = manager.open(StorageScope::frontend("module", "component", "one"));
            scoped.set(
                "theme",
                json!({
                    "name": "dark",
                    "palette": {
                        "accent": "#4f8cff",
                        "surface": "#101820",
                        "text": "#f8fafc"
                    },
                    "tokens": (0..24).map(|index| json!({ "name": format!("token-{index}"), "value": index })).collect::<Vec<_>>()
                }),
            );
            Arc::new(Mutex::new(scoped))
        }

        let iterations = 100_000usize;
        let old_lua = Lua::new();
        old_lua
            .globals()
            .set(
                "storage",
                old_lua_storage_table(&old_lua, benchmark_storage("read-old")),
            )
            .unwrap();
        let old_started = Instant::now();
        for _ in 0..iterations {
            let name: String = old_lua
                .load("return storage.theme.palette.accent")
                .eval()
                .unwrap();
            std::hint::black_box(name);
        }
        let old = old_started.elapsed();

        let new_lua = Lua::new();
        let new_table = create_lua_storage_table(
            &new_lua,
            benchmark_storage("read-new"),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
        )
        .unwrap();
        new_lua.globals().set("storage", new_table).unwrap();
        let new_started = Instant::now();
        for _ in 0..iterations {
            let name: String = new_lua
                .load("return storage.theme.palette.accent")
                .eval()
                .unwrap();
            std::hint::black_box(name);
        }
        let new = new_started.elapsed();

        eprintln!(
            "storage read over {iterations} table reads: cloned key/value {old:?}, borrowed key/value {new:?}, ratio {:.1}x",
            old.as_secs_f64() / new.as_secs_f64()
        );
        assert!(new < old);
    }
}
