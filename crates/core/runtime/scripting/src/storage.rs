use serde_json::{Map, Value};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue, Variadic};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
}

impl StorageManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
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
        let document = match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<Value>(&contents) {
                Ok(Value::Object(map)) => map,
                Ok(_) => {
                    diagnostics.push(StorageDiagnostic {
                        scope: scope.clone(),
                        path: path.clone(),
                        reason: "storage document root is not a JSON object".to_string(),
                    });
                    Map::new()
                }
                Err(error) => {
                    diagnostics.push(StorageDiagnostic {
                        scope: scope.clone(),
                        path: path.clone(),
                        reason: format!("storage document could not be decoded: {error}"),
                    });
                    Map::new()
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Map::new(),
            Err(error) => {
                diagnostics.push(StorageDiagnostic {
                    scope: scope.clone(),
                    path: path.clone(),
                    reason: format!("storage document could not be read: {error}"),
                });
                Map::new()
            }
        };

        ScopedStorage {
            scope,
            path,
            document,
            diagnostics,
            dirty: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScopedStorage {
    scope: StorageScope,
    path: PathBuf,
    document: Map<String, Value>,
    diagnostics: Vec<StorageDiagnostic>,
    dirty: bool,
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

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.document.get(key)
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        let previous = self.document.insert(key.into(), value);
        self.dirty = true;
        previous
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

    pub fn persist(&mut self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let temp_path = self.temp_path();
        let bytes = serde_json::to_vec_pretty(&self.snapshot())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(&temp_path, bytes)?;
        fs::rename(temp_path, &self.path)?;
        self.dirty = false;
        Ok(())
    }

    pub fn flush_if_dirty(&mut self) -> io::Result<bool> {
        if !self.dirty {
            return Ok(false);
        }
        self.persist()?;
        Ok(true)
    }

    fn temp_path(&self) -> PathBuf {
        let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("storage.json");
        self.path
            .with_file_name(format!("{file_name}.{count}.{}.tmp", std::process::id()))
    }
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
                        Ok(value) => {
                            newindex_storage.lock().unwrap().set(key, value);
                            write_sink(key);
                        }
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
    format!("{readable}--{}", hex_bytes(raw.as_bytes()))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
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
