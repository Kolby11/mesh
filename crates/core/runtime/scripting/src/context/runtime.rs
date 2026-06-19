use super::lookup::{
    interface_error_message, lookup_failure_reason, lua_err, lua_value_to_string, map_lua_error,
    record_lookup_diagnostic, record_lookup_diagnostic_lua,
};
use super::proxy::{create_event_channel, create_interface_proxy, interface_event_channel};
use super::{PublishedEvent, ScriptDiagnostic, ScriptError, ScriptInterfaceImport, ScriptState};
use crate::chunk_cache::ChunkCache;
use crate::host_api::{HostApiManifest, InterfaceProxy};
use crate::pool;
use crate::storage::{ScopedStorage, StorageManager, StorageScope, create_lua_storage_table};
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use mesh_core_service::{InterfaceCatalog, InterfaceResolution};
use mlua::{Error as LuaError, Function, Lua, LuaSerdeExt, Table, Value as LuaValue, Variadic};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub struct BoundInstanceCall {
    pub parent_instance_key: String,
    pub binding: String,
    pub child_instance_key: String,
    pub function_name: String,
    pub args: Vec<Value>,
}

#[derive(Debug, Default)]
struct SharedInterfaceBindings {
    bindings: HashMap<String, InterfaceResolution>,
    generation: u64,
}

/// A script execution context for one component instance.
///
/// Owns frontend script state, capability metadata, and the mlua runtime.
/// Scripts run as-written with no source preprocessing. Reactive state follows
/// the standard Lua module pattern: bare global assignments are exported and
/// synced to the template; `local` variables are private to the script.
#[derive(Debug)]
pub struct ScriptContext {
    pub module_id: String,
    pub capabilities: CapabilitySet,
    pub state: ScriptState,
    vm: Option<pool::PooledVm>,
    env_table: Option<Table>,
    interface_catalog: InterfaceCatalog,
    pub(super) interface_bindings: HashMap<String, InterfaceResolution>,
    shared_interface_bindings: Arc<Mutex<SharedInterfaceBindings>>,
    interface_bindings_generation: u64,
    /// Global names present before user script execution (stdlib + host API).
    /// Sync skips these so only user-defined globals become reactive state.
    builtin_globals: HashSet<String>,
    /// Keys discovered in the first full globals walk after `load_script`.
    /// Subsequent `sync_state_from_lua` calls use targeted `get` lookups
    /// instead of iterating the entire globals table.
    user_global_keys: Vec<String>,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    published_events: Vec<PublishedEvent>,
    shared_published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    diagnostics: Vec<ScriptDiagnostic>,
    shared_diagnostics: Arc<Mutex<Vec<ScriptDiagnostic>>>,
    bound_instance_calls: Vec<BoundInstanceCall>,
    shared_bound_instance_calls: Arc<Mutex<Vec<BoundInstanceCall>>>,
    storage: Arc<Mutex<ScopedStorage>>,
    tracked_storage_keys: Arc<Mutex<HashSet<String>>>,
    changed_storage_keys: Arc<Mutex<HashSet<String>>>,
    tracking_storage_reads: Arc<Mutex<bool>>,
    /// `state.snapshot_generation()` at the time of the last `refresh_module_object` call.
    /// When this matches the current generation (and no proxies exist), the Lua
    /// `module.state` table is already up to date and the rebuild can be skipped.
    last_module_refresh_gen: u64,
}

impl Drop for ScriptContext {
    fn drop(&mut self) {
        self.flush_storage();
        self.uninit();
    }
}

impl ScriptContext {
    fn lua(&self) -> &Lua {
        self.vm
            .as_ref()
            .expect("ScriptContext not initialized — call ensure_initialized first")
            .lua()
    }

    fn env(&self) -> &Table {
        self.env_table
            .as_ref()
            .expect("ScriptContext not initialized — call ensure_initialized first")
    }

    /// Create a new script context for a module.
    pub fn new(
        module_id: impl Into<String>,
        capabilities: CapabilitySet,
    ) -> Result<Self, ScriptError> {
        Self::new_with_storage_root(module_id, capabilities, default_runtime_storage_root())
    }

    pub fn new_with_storage_root(
        module_id: impl Into<String>,
        capabilities: CapabilitySet,
        storage_root: impl Into<PathBuf>,
    ) -> Result<Self, ScriptError> {
        let module_id = module_id.into();
        let storage = StorageManager::new(storage_root.into()).open(StorageScope::frontend(
            module_id.clone(),
            module_id.clone(),
            module_id.clone(),
        ));
        let storage_diagnostics = storage
            .diagnostics()
            .iter()
            .map(|diagnostic| ScriptDiagnostic {
                module_id: module_id.clone(),
                interface: "self.storage".to_string(),
                requested_version: None,
                reason: diagnostic.reason.clone(),
            })
            .collect();
        Ok(Self {
            module_id,
            capabilities,
            state: ScriptState::new(),
            vm: None,
            env_table: None,
            interface_catalog: InterfaceCatalog::default(),
            interface_bindings: HashMap::new(),
            shared_interface_bindings: Arc::new(Mutex::new(SharedInterfaceBindings::default())),
            interface_bindings_generation: 0,
            builtin_globals: HashSet::new(),
            user_global_keys: Vec::new(),
            tracked_service_fields: Arc::new(Mutex::new(HashMap::new())),
            published_events: Vec::new(),
            shared_published_events: Arc::new(Mutex::new(Vec::new())),
            diagnostics: storage_diagnostics,
            shared_diagnostics: Arc::new(Mutex::new(Vec::new())),
            bound_instance_calls: Vec::new(),
            shared_bound_instance_calls: Arc::new(Mutex::new(Vec::new())),
            storage: Arc::new(Mutex::new(storage)),
            tracked_storage_keys: Arc::new(Mutex::new(HashSet::new())),
            changed_storage_keys: Arc::new(Mutex::new(HashSet::new())),
            tracking_storage_reads: Arc::new(Mutex::new(false)),
            last_module_refresh_gen: u64::MAX,
        })
    }

    /// Create a new ScriptContext via the pool/cache integration point.
    ///
    /// Identical to `new()` — the constructor is already lazy (vm: None, env_table: None).
    /// Named `new_lazy` as the documented integration contract for Phase 95 INT-01.
    pub fn new_lazy(
        module_id: impl Into<String>,
        capabilities: CapabilitySet,
    ) -> Result<Self, ScriptError> {
        Self::new(module_id, capabilities)
    }

    pub fn set_interface_catalog(&mut self, catalog: InterfaceCatalog) {
        self.interface_catalog = catalog;
    }

    /// Check out a pooled VM, create a per-component _ENV table, install host API,
    /// sandbox the thread, and populate builtin_globals. Idempotent — does nothing
    /// if already initialized.
    fn ensure_initialized(&mut self) -> Result<(), ScriptError> {
        if self.vm.is_some() {
            return Ok(());
        }
        let vm = pool::checkout();
        let lua = vm.lua();

        // ISO-01: create per-component _ENV with __index = globals() fallthrough
        let env = lua.create_table().map_err(lua_err)?;
        let meta = lua.create_table().map_err(lua_err)?;
        meta.set("__index", lua.globals()).map_err(lua_err)?;
        env.set_metatable(Some(meta)).map_err(lua_err)?;

        self.vm = Some(vm);
        self.env_table = Some(env.clone());

        // Install host API into the per-component env table (ISO-02 completion)
        self.install_host_api(&env)?;

        // Snapshot all keys on env_table post-host-api-install (stdlib keys are
        // on globals, not env_table — only host API and later script globals show up here)
        self.builtin_globals = env
            .pairs::<String, LuaValue>()
            .filter_map(|result| result.ok().map(|(key, _)| key))
            .collect();

        Ok(())
    }

    /// Drop the env_table and return the pooled VM. ScriptContext methods
    /// must not be called after this without a subsequent ensure_initialized().
    pub fn uninit(&mut self) {
        self.env_table = None;
        self.builtin_globals.clear();
        self.user_global_keys.clear();
        self.vm = None;
    }

    /// Load a script source. Executes the script and seeds reactive state from
    /// any global variable assignments at the top level.
    pub fn load_script(&mut self, source: &str) -> Result<(), ScriptError> {
        self.load_script_with_interface_imports(source, &[])
    }

    /// Load a script source after installing explicit interface imports as Lua globals.
    pub fn load_script_with_interface_imports(
        &mut self,
        source: &str,
        imports: &[ScriptInterfaceImport],
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        self.interface_bindings.clear();
        self.user_global_keys.clear();
        {
            let mut shared_interface_bindings = self.shared_interface_bindings.lock().unwrap();
            shared_interface_bindings.bindings.clear();
            shared_interface_bindings.generation =
                shared_interface_bindings.generation.wrapping_add(1);
            self.interface_bindings_generation = shared_interface_bindings.generation;
        }
        self.shared_published_events.lock().unwrap().clear();
        self.shared_diagnostics.lock().unwrap().clear();
        self.shared_bound_instance_calls.lock().unwrap().clear();
        self.changed_storage_keys.lock().unwrap().clear();
        self.clear_tracked_service_fields();
        self.clear_tracked_storage_keys();
        // Host API already installed into env_table by ensure_initialized.
        // builtin_globals already populated.
        self.install_interface_imports(imports)?;
        self.refresh_module_object();
        self.lua()
            .load(source)
            .set_name(&self.module_id)
            .set_environment(self.env().clone())
            .exec()
            .map_err(map_lua_error)?;
        self.sync_state_from_lua();
        tracing::info!("loaded script for module {}", self.module_id);
        Ok(())
    }

    /// Compile and execute Luau source, caching the source string by its
    /// FNV64 content hash so hot-reload can evict on file change (Phase 95).
    /// Delegates to `load_script_with_interface_imports` for execution.
    pub fn compile_and_execute(
        &mut self,
        source: &str,
        imports: &[ScriptInterfaceImport],
    ) -> Result<(), ScriptError> {
        // Cache the source by content hash — Phase 95 mtime watcher calls
        // ChunkCache::remove(hash) to evict on .mesh file change.
        ChunkCache::get_or_insert(source);
        self.load_script_with_interface_imports(source, imports)
    }

    /// Compile and execute Luau source (no interface imports). See
    /// `compile_and_execute` for the full variant.
    pub fn compile_and_execute_simple(&mut self, source: &str) -> Result<(), ScriptError> {
        ChunkCache::get_or_insert(source);
        self.load_script(source)
    }

    /// Copy the latest service payload into the Lua runtime for proxy reads.
    ///
    /// Called by core after each service update so interface proxies can read
    /// state fields directly without explicit callback or binding APIs.
    pub fn apply_service_payload(&mut self, service: &str, payload: &Value) {
        let _ = self.ensure_initialized();
        let service_key = format!("__mesh_svc_{service}");
        if let Ok(lua_value) = self.lua().to_value(payload) {
            // Set on globals() so proxy __index (service_payload_field) can find it.
            // The env table __index falls through to globals, so scripts accessing
            // __mesh_svc_* directly from the env also work.
            let _ = self.lua().globals().set(service_key.as_str(), lua_value);
        }
        if service == "locale"
            && let Some(locale) = payload
                .get("locale")
                .or_else(|| payload.get("current"))
                .and_then(|value| value.as_str())
        {
            let _ = self.lua().globals().set("__mesh_locale_current", locale);
        }
        self.refresh_module_object();
    }

    pub fn emit_interface_event(
        &mut self,
        service: &str,
        event_name: &str,
        payload: &Value,
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let channel = interface_event_channel(self.lua(), service, event_name).map_err(lua_err)?;
        let emit = channel.get::<Function>("emit").map_err(lua_err)?;
        let lua_payload = self.lua().to_value(payload).map_err(lua_err)?;
        emit.call::<()>((channel, lua_payload))
            .map_err(map_lua_error)?;
        self.sync_state_from_lua();
        self.sync_side_channels();
        Ok(())
    }

    pub fn set_global_state(&mut self, name: &str, value: Value) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let lua_value = self.lua().to_value(&value).map_err(lua_err)?;
        self.lua()
            .globals()
            .set(name, lua_value)
            .map_err(map_lua_error)?;
        self.state.set(name.to_string(), value);
        self.refresh_module_object();
        Ok(())
    }

    pub fn tracked_service_fields(&self) -> HashMap<String, HashSet<String>> {
        self.tracked_service_fields.lock().unwrap().clone()
    }

    pub fn tracked_fields_for_service(&self, service: &str) -> HashSet<String> {
        self.tracked_service_fields
            .lock()
            .unwrap()
            .get(service)
            .cloned()
            .unwrap_or_default()
    }

    pub fn tracked_service_fields_changed(
        &self,
        service: &str,
        previous: Option<&Value>,
        next: &Value,
    ) -> bool {
        let tracked_service_fields = self.tracked_service_fields.lock().unwrap();
        let Some(tracked_fields) = tracked_service_fields.get(service) else {
            return false;
        };
        tracked_fields.iter().any(|field| {
            let previous_value = previous.and_then(|value| value.get(field));
            let next_value = next.get(field);
            previous_value != next_value
        })
    }

    pub fn clear_tracked_service_fields(&self) {
        self.tracked_service_fields.lock().unwrap().clear();
    }

    pub fn tracked_storage_keys(&self) -> HashSet<String> {
        self.tracked_storage_keys.lock().unwrap().clone()
    }

    pub fn clear_tracked_storage_keys(&self) {
        self.tracked_storage_keys.lock().unwrap().clear();
    }

    pub fn public_field_names(&self) -> Vec<String> {
        let mut names = self
            .state
            .keys()
            .into_iter()
            .filter(|name| name != "exports")
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    pub fn public_function_names(&mut self) -> Vec<String> {
        let _ = self.ensure_initialized();
        let mut names = self
            .env()
            .pairs::<String, LuaValue>()
            .filter_map(|pair| {
                let (name, value) = pair.ok()?;
                if self.builtin_globals.contains(&name)
                    || name.starts_with("__mesh_")
                    || is_reserved_runtime_hook(&name)
                    || !matches!(value, LuaValue::Function(_))
                {
                    return None;
                }
                Some(name)
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    pub fn public_member_snapshot(&mut self) -> Value {
        let mut object = serde_json::Map::new();
        for name in self.public_field_names() {
            if let Some(value) = self.state.get(&name) {
                object.insert(name, value);
            }
        }
        object.insert(
            "__functions".to_string(),
            Value::Array(
                self.public_function_names()
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
        Value::Object(object)
    }

    pub fn install_bound_instance_proxy(
        &mut self,
        parent_instance_key: &str,
        binding: &str,
        child_instance_key: &str,
        snapshot: &Value,
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        self.state.set(binding.to_string(), snapshot.clone());

        let table = self.lua().create_table().map_err(lua_err)?;
        if let Value::Object(object) = snapshot {
            for (key, value) in object {
                if key == "__functions" {
                    continue;
                }
                table
                    .set(key.as_str(), self.lua().to_value(value).map_err(lua_err)?)
                    .map_err(lua_err)?;
            }

            if let Some(functions) = object.get("__functions").and_then(Value::as_array) {
                for function_name in functions.iter().filter_map(Value::as_str) {
                    let call_queue = Arc::clone(&self.shared_bound_instance_calls);
                    let parent_instance_key = parent_instance_key.to_string();
                    let binding = binding.to_string();
                    let child_instance_key = child_instance_key.to_string();
                    let function_name_owned = function_name.to_string();
                    let function = self
                        .lua()
                        .create_function(move |lua, args: Variadic<LuaValue>| {
                            let mut serialized_args = Vec::new();
                            for (index, arg) in args.into_iter().enumerate() {
                                if index == 0
                                    && is_bound_instance_self_arg(&arg, child_instance_key.as_str())
                                {
                                    continue;
                                }
                                serialized_args.push(lua.from_value(arg)?);
                            }
                            call_queue.lock().unwrap().push(BoundInstanceCall {
                                parent_instance_key: parent_instance_key.clone(),
                                binding: binding.clone(),
                                child_instance_key: child_instance_key.clone(),
                                function_name: function_name_owned.clone(),
                                args: serialized_args,
                            });
                            Ok(())
                        })
                        .map_err(lua_err)?;
                    table.set(function_name, function).map_err(lua_err)?;
                }
            }
        }

        self.env().set(binding, table).map_err(map_lua_error)?;
        Ok(())
    }

    /// Call the script's `init(self)` function if it exists.
    ///
    /// Legacy no-argument `init()` handlers remain compatible because Luau
    /// ignores extra arguments.
    pub fn call_init(&mut self) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        if let Ok(init) = self.env().get::<Function>("init") {
            tracing::debug!("calling init() for {}", self.module_id);
            let current_self = self.current_self_table()?;
            init.call::<()>(current_self).map_err(map_lua_error)?;
            self.sync_state_from_lua();
            self.sync_side_channels();
        }
        Ok(())
    }

    /// Call a named event handler.
    pub fn call_handler(&mut self, name: &str, args: &[Value]) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let handler = self
            .env()
            .get::<Function>(name)
            .map_err(|_| ScriptError::HandlerNotFound(name.to_string()))?;
        tracing::debug!("calling handler {name}() for {}", self.module_id);
        if is_lifecycle_handler(name) {
            let mut lifecycle_args = mlua::MultiValue::new();
            lifecycle_args.push_back(LuaValue::Table(self.current_self_table()?));
            for arg in args {
                lifecycle_args.push_back(self.lua().to_value(arg).map_err(lua_err)?);
            }
            handler.call::<()>(lifecycle_args).map_err(map_lua_error)?;
        } else {
            match args.len() {
                0 => handler.call::<()>(()).map_err(map_lua_error)?,
                1 => {
                    let arg = self.lua().to_value(&args[0]).map_err(lua_err)?;
                    handler.call::<()>(arg).map_err(map_lua_error)?;
                }
                _ => {
                    let mut multi_args = mlua::MultiValue::new();
                    for arg in args {
                        multi_args.push_back(self.lua().to_value(arg).map_err(lua_err)?);
                    }
                    handler.call::<()>(multi_args).map_err(map_lua_error)?;
                }
            }
        }
        self.sync_state_from_lua();
        self.sync_side_channels();
        if name == "unmount" {
            self.flush_storage();
        }
        Ok(())
    }

    /// Call the canonical render lifecycle if present, otherwise fall back to
    /// the legacy handler name used by existing shipped surfaces.
    pub fn call_render_lifecycle(&mut self) -> Result<bool, ScriptError> {
        self.ensure_initialized()?;
        if self.has_handler("render") {
            self.clear_tracked_storage_keys();
            *self.tracking_storage_reads.lock().unwrap() = true;
            let result = self.call_handler("render", &[]);
            *self.tracking_storage_reads.lock().unwrap() = false;
            result?;
            Ok(true)
        } else if self.has_handler("onRender") {
            self.clear_tracked_storage_keys();
            *self.tracking_storage_reads.lock().unwrap() = true;
            let result = self.call_handler("onRender", &[]);
            *self.tracking_storage_reads.lock().unwrap() = false;
            result?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn current_self_table(&self) -> Result<Table, ScriptError> {
        let current_self = self.lua().create_table().map_err(lua_err)?;
        let meta = self.lua().create_table().map_err(lua_err)?;
        meta.set("module_id", self.module_id.as_str())
            .map_err(lua_err)?;
        meta.set("component_id", self.module_id.as_str())
            .map_err(lua_err)?;
        meta.set("kind", "frontend").map_err(lua_err)?;
        meta.set("instance_id", self.module_id.as_str())
            .map_err(lua_err)?;
        meta.set("diagnostics_id", self.module_id.as_str())
            .map_err(lua_err)?;
        current_self.set("meta", meta).map_err(lua_err)?;
        let storage_diagnostics = Arc::clone(&self.shared_diagnostics);
        let storage_module_id = self.module_id.clone();
        let tracked_storage_keys = Arc::clone(&self.tracked_storage_keys);
        let tracking_storage_reads = Arc::clone(&self.tracking_storage_reads);
        let changed_storage_keys = Arc::clone(&self.changed_storage_keys);
        let storage = create_lua_storage_table(
            self.lua(),
            Arc::clone(&self.storage),
            Arc::new(move |reason| {
                storage_diagnostics.lock().unwrap().push(ScriptDiagnostic {
                    module_id: storage_module_id.clone(),
                    interface: "self.storage".to_string(),
                    requested_version: None,
                    reason,
                });
            }),
            Arc::new(move |key| {
                if *tracking_storage_reads.lock().unwrap() {
                    tracked_storage_keys.lock().unwrap().insert(key);
                }
            }),
            Arc::new(move |key| {
                changed_storage_keys.lock().unwrap().insert(key);
            }),
        )
        .map_err(lua_err)?;
        current_self.set("storage", storage).map_err(lua_err)?;
        let module_id = self.module_id.clone();
        let self_events_meta = self.lua().create_table().map_err(lua_err)?;
        self_events_meta
            .set(
                "__index",
                self.lua()
                    .create_function(move |lua, (table, key): (Table, String)| {
                        if key == "meta" {
                            return table.get::<LuaValue>("meta");
                        }
                        if !is_named_event_channel(&key) {
                            return Ok(LuaValue::Nil);
                        }
                        let channel = self_event_channel(lua, &module_id, &key)?;
                        table.set(key.as_str(), channel.clone())?;
                        Ok(LuaValue::Table(channel))
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;
        current_self
            .set_metatable(Some(self_events_meta))
            .map_err(lua_err)?;
        Ok(current_self)
    }

    /// Get a reference to the current state for tree building.
    pub fn state(&self) -> &ScriptState {
        &self.state
    }

    /// Get a mutable reference to state.
    pub fn state_mut(&mut self) -> &mut ScriptState {
        &mut self.state
    }

    pub fn drain_published_events(&mut self) -> Vec<PublishedEvent> {
        self.sync_side_channels();
        std::mem::take(&mut self.published_events)
    }

    pub fn drain_bound_instance_calls(&mut self) -> Vec<BoundInstanceCall> {
        self.sync_side_channels();
        std::mem::take(&mut self.bound_instance_calls)
    }

    pub fn drain_diagnostics(&mut self) -> Vec<ScriptDiagnostic> {
        self.sync_side_channels();
        std::mem::take(&mut self.diagnostics)
    }

    pub fn flush_storage(&mut self) {
        let result = self.storage.lock().unwrap().flush_if_dirty();
        if let Err(error) = result {
            self.diagnostics.push(ScriptDiagnostic {
                module_id: self.module_id.clone(),
                interface: "self.storage".to_string(),
                requested_version: None,
                reason: format!("storage persistence failed: {error}"),
            });
        }
    }

    /// Check if a handler exists.
    pub fn has_handler(&mut self, name: &str) -> bool {
        let _ = self.ensure_initialized();
        self.env().get::<Function>(name).is_ok()
    }

    fn install_host_api(&mut self, target: &mlua::Table) -> Result<(), ScriptError> {
        let globals = target;
        globals
            .set("self", self.current_self_table()?)
            .map_err(lua_err)?;
        let module_object = self.lua().create_table().map_err(lua_err)?;
        let module_state = self.lua().create_table().map_err(lua_err)?;
        let module_exports = self.lua().create_table().map_err(lua_err)?;
        let module_events = self.lua().create_table().map_err(lua_err)?;
        let module_events_meta = self.lua().create_table().map_err(lua_err)?;
        module_events_meta
            .set(
                "__index",
                self.lua()
                    .create_function(|lua, (table, key): (Table, String)| {
                        let channel = create_event_channel(lua)?;
                        table.set(key.as_str(), channel.clone())?;
                        Ok(channel)
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;
        module_events
            .set_metatable(Some(module_events_meta))
            .map_err(lua_err)?;
        module_object.set("state", module_state).map_err(lua_err)?;
        module_object
            .set("exports", module_exports)
            .map_err(lua_err)?;
        module_object
            .set("events", module_events)
            .map_err(lua_err)?;
        globals.set("module", module_object).map_err(lua_err)?;

        let mesh = self.lua().create_table().map_err(lua_err)?;
        let mesh_core_service = self.lua().create_table().map_err(lua_err)?;
        let mesh_core_events = self.lua().create_table().map_err(lua_err)?;
        let mesh_ui_api = self.lua().create_table().map_err(lua_err)?;
        let mesh_log = self.lua().create_table().map_err(lua_err)?;
        let mesh_popover = self.lua().create_table().map_err(lua_err)?;
        let mesh_locale = self.lua().create_table().map_err(lua_err)?;
        let interface_catalog = self.interface_catalog.clone();
        let manifest = HostApiManifest::from_capabilities(&self.capabilities);
        let allowed_interfaces = manifest.interface_capabilities.clone();
        let has_theme_read = manifest.has_theme_read;
        let has_locale_read = manifest.has_locale_read;
        let has_locale_write = manifest.has_locale_write;

        let published_events = Arc::clone(&self.shared_published_events);
        let module_id = self.module_id.clone();
        let capabilities = self.capabilities.clone();
        mesh_core_events
            .set(
                "publish",
                self.lua()
                    .create_function(move |lua, (channel, payload): (String, Option<LuaValue>)| {
                        let payload = payload.unwrap_or(LuaValue::Nil);
                        let payload = lua.from_value::<Value>(payload)?;
                        tracing::info!("{} published event {}", module_id, channel);
                        published_events.lock().unwrap().push(PublishedEvent {
                            channel,
                            payload,
                            source_module_id: module_id.clone(),
                            source_capabilities: capabilities.clone(),
                        });
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let env_for_redraw = globals.clone();
        mesh_ui_api
            .set(
                "request_redraw",
                self.lua()
                    .create_function(move |_lua, ()| {
                        env_for_redraw.set("__mesh_request_redraw", true)?;
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let env_for_locale = globals.clone();
        mesh_locale
            .set(
                "current",
                self.lua()
                    .create_function(move |_lua, ()| {
                        env_for_locale
                            .get::<Option<String>>("__mesh_locale_current")
                            .map(|locale| locale.unwrap_or_else(|| "en".to_string()))
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let published_events_for_locale = Arc::clone(&self.shared_published_events);
        let module_id_for_locale = self.module_id.clone();
        let capabilities_for_locale = self.capabilities.clone();
        mesh_locale
            .set(
                "set",
                self.lua()
                    .create_function(move |_lua, locale: String| {
                        if !has_locale_write {
                            return Err(LuaError::external(ScriptError::CapabilityDenied(
                                "locale.write".to_string(),
                            )));
                        }
                        published_events_for_locale
                            .lock()
                            .unwrap()
                            .push(PublishedEvent {
                                channel: "shell.set-locale".to_string(),
                                payload: serde_json::json!({ "locale": locale }),
                                source_module_id: module_id_for_locale.clone(),
                                source_capabilities: capabilities_for_locale.clone(),
                            });
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let module_id = self.module_id.clone();
        mesh_log
            .set(
                "info",
                self.lua()
                    .create_function(move |_lua, message: String| {
                        tracing::info!("{}: {}", module_id, message);
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;
        let module_id = self.module_id.clone();
        mesh_log
            .set(
                "warn",
                self.lua()
                    .create_function(move |_lua, message: String| {
                        tracing::warn!("{}: {}", module_id, message);
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;
        let module_id = self.module_id.clone();
        mesh_log
            .set(
                "error",
                self.lua()
                    .create_function(move |_lua, message: String| {
                        tracing::error!("{}: {}", module_id, message);
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let published_events_for_popover = Arc::clone(&self.shared_published_events);
        let module_id_for_popover = self.module_id.clone();
        let capabilities_for_popover = self.capabilities.clone();
        mesh_popover
            .set(
                "activate",
                self.lua()
                    .create_function(move |_lua, args: Variadic<LuaValue>| {
                        let Some(LuaValue::String(surface_id)) = args.first() else {
                            return Err(LuaError::FromLuaConversionError {
                                from: "nil",
                                to: "String".to_string(),
                                message: Some("mesh.popover.activate expects a surface id".into()),
                            });
                        };
                        let surface_id = surface_id.to_str()?.to_string();
                        let event = match args.get(1) {
                            Some(LuaValue::Table(table)) => Some(table.clone()),
                            _ => None,
                        };
                        let focus = match args.get(2) {
                            Some(LuaValue::Boolean(value)) => *value,
                            Some(LuaValue::Table(table)) => table
                                .get::<Option<bool>>("focus")?
                                .or_else(|| {
                                    table.get::<Option<bool>>("focus_on_open").ok().flatten()
                                })
                                .unwrap_or(true),
                            _ => true,
                        };
                        // Extract trigger surface + key from a click
                        // event passed in by the script. Falls back
                        // to empty strings if the script invoked
                        // activate without an event (no Tab targeting
                        // possible in that case but the popover still
                        // shows).
                        let (trigger_surface, trigger_key) = if let Some(event_tbl) = event {
                            let surface = event_tbl
                                .get::<Table>("surface")
                                .ok()
                                .and_then(|s| s.get::<String>("id").ok())
                                .unwrap_or_default();
                            let key = event_tbl
                                .get::<Table>("current")
                                .ok()
                                .and_then(|c| c.get::<String>("key").ok())
                                .or_else(|| {
                                    event_tbl
                                        .get::<Table>("current_target")
                                        .ok()
                                        .and_then(|c| c.get::<String>("key").ok())
                                })
                                .unwrap_or_default();
                            (surface, key)
                        } else {
                            (String::new(), String::new())
                        };
                        let payload = serde_json::json!({
                            "surface_id": surface_id,
                            "trigger_surface": trigger_surface,
                            "trigger_key": trigger_key,
                            "focus": focus,
                        });
                        tracing::info!(
                            "{} called mesh.popover.activate target={} trigger_surface={} trigger_key={} focus={}",
                            module_id_for_popover, surface_id, trigger_surface, trigger_key, focus
                        );
                        published_events_for_popover
                            .lock()
                            .unwrap()
                            .push(PublishedEvent {
                                channel: "shell.activate-popover".to_string(),
                                payload,
                                source_module_id: module_id_for_popover.clone(),
                                source_capabilities: capabilities_for_popover.clone(),
                            });
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let published_events_for_popover = Arc::clone(&self.shared_published_events);
        let module_id_for_popover = self.module_id.clone();
        let capabilities_for_popover = self.capabilities.clone();
        mesh_popover
            .set(
                "hide",
                self.lua()
                    .create_function(move |_lua, surface_id: String| {
                        published_events_for_popover
                            .lock()
                            .unwrap()
                            .push(PublishedEvent {
                                channel: "shell.hide-surface".to_string(),
                                payload: serde_json::json!({ "surface_id": surface_id }),
                                source_module_id: module_id_for_popover.clone(),
                                source_capabilities: capabilities_for_popover.clone(),
                            });
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        mesh.set("service", mesh_core_service).map_err(lua_err)?;
        mesh.set("events", mesh_core_events).map_err(lua_err)?;
        mesh.set("ui", mesh_ui_api).map_err(lua_err)?;
        mesh.set("log", mesh_log).map_err(lua_err)?;
        mesh.set("popover", mesh_popover).map_err(lua_err)?;
        mesh.set("locale", mesh_locale).map_err(lua_err)?;
        let mesh_for_require = mesh.clone();
        globals.set("mesh", mesh).map_err(lua_err)?;
        globals
            .set("__mesh_request_redraw", false)
            .map_err(lua_err)?;
        globals
            .set("__mesh_locale_current", "en")
            .map_err(lua_err)?;

        let published_events = Arc::clone(&self.shared_published_events);
        let tracked_service_fields = Arc::clone(&self.tracked_service_fields);
        let module_id_for_require = self.module_id.clone();
        let capabilities_for_require = self.capabilities.clone();
        let diagnostics_for_require = Arc::clone(&self.shared_diagnostics);
        let require = self
            .lua()
            .create_function(move |lua, module: String| {
                if module == "@mesh/i18n" || module == "mesh.i18n" {
                    return create_i18n_library(lua);
                }

                if let Some(host_api) = resolve_host_api(&mesh_for_require, &module)? {
                    return Ok(host_api);
                }

                if is_component_definition_specifier(&module) {
                    let definition = lua.create_table()?;
                    definition.set("__mesh_component_definition", true)?;
                    definition.set("source", module.as_str())?;
                    return Ok(definition);
                }

                let mut module_name = module.as_str();
                let mut version = None;
                if let Some((left, right)) = module.rsplit_once('@') {
                    if left.starts_with("mesh.") {
                        module_name = left;
                        version = Some(right.to_string());
                    }
                }

                let interface = if module_name.starts_with("mesh.") {
                    module_name.to_string()
                } else {
                    return Err(mlua::Error::external(ScriptError::LuaError(format!(
                        "unsupported require: {module}"
                    ))));
                };

                let canonical = InterfaceProxy::canonical_name(&interface);
                let readable = canonical == "mesh.theme" && has_theme_read
                    || canonical == "mesh.locale" && has_locale_read
                    || allowed_interfaces.contains(&canonical)
                    || !canonical.starts_with("mesh.");
                if canonical.starts_with("mesh.") && !readable {
                    return Err(record_lookup_diagnostic_lua(
                        &diagnostics_for_require,
                        &module_id_for_require,
                        &canonical,
                        version.as_deref(),
                        "capability denied",
                        ScriptError::CapabilityDenied(canonical.clone()),
                    ));
                }

                let resolution = interface_catalog.resolve(&canonical, version.as_deref());
                if resolution.provider.is_none() {
                    let reason = lookup_failure_reason(&interface_catalog, &resolution);
                    return Err(record_lookup_diagnostic_lua(
                        &diagnostics_for_require,
                        &module_id_for_require,
                        &canonical,
                        version.as_deref(),
                        &reason,
                        ScriptError::InterfaceUnavailable(interface_error_message(
                            &canonical,
                            version.as_deref(),
                        )),
                    ));
                }

                let proxy = create_interface_proxy(
                    lua,
                    resolution,
                    module_id_for_require.clone(),
                    capabilities_for_require.clone(),
                    Arc::clone(&tracked_service_fields),
                    Arc::clone(&published_events),
                )?;
                Ok(proxy)
            })
            .map_err(lua_err)?;
        globals.set("require", require).map_err(lua_err)?;
        Ok(())
    }

    fn install_interface_imports(
        &mut self,
        imports: &[ScriptInterfaceImport],
    ) -> Result<(), ScriptError> {
        if imports.is_empty() {
            return Ok(());
        }

        let manifest = HostApiManifest::from_capabilities(&self.capabilities);
        let globals = self.env().clone();
        for import in imports {
            let canonical = InterfaceProxy::canonical_name(&import.interface);
            let readable = canonical == "mesh.theme" && manifest.has_theme_read
                || canonical == "mesh.locale" && manifest.has_locale_read
                || manifest.interface_capabilities.contains(&canonical)
                || !canonical.starts_with("mesh.");
            if canonical.starts_with("mesh.") && !readable {
                record_lookup_diagnostic(
                    &self.shared_diagnostics,
                    &self.module_id,
                    &canonical,
                    import.version.as_deref(),
                    "capability denied",
                );
                return Err(ScriptError::CapabilityDenied(canonical));
            }

            let resolution = self
                .interface_catalog
                .resolve(&canonical, import.version.as_deref());
            if resolution.provider.is_none() {
                let reason = lookup_failure_reason(&self.interface_catalog, &resolution);
                record_lookup_diagnostic(
                    &self.shared_diagnostics,
                    &self.module_id,
                    &canonical,
                    import.version.as_deref(),
                    &reason,
                );
                return Err(ScriptError::InterfaceUnavailable(interface_error_message(
                    &canonical,
                    import.version.as_deref(),
                )));
            }

            {
                let mut shared_interface_bindings = self.shared_interface_bindings.lock().unwrap();
                shared_interface_bindings
                    .bindings
                    .insert(import.alias.clone(), resolution.clone());
                shared_interface_bindings.generation =
                    shared_interface_bindings.generation.wrapping_add(1);
            }
            let proxy = create_interface_proxy(
                self.lua(),
                resolution,
                self.module_id.clone(),
                self.capabilities.clone(),
                Arc::clone(&self.tracked_service_fields),
                Arc::clone(&self.shared_published_events),
            )
            .map_err(lua_err)?;
            globals.set(import.alias.as_str(), proxy).map_err(lua_err)?;
        }

        Ok(())
    }

    /// Sync Lua globals back into ScriptState.
    ///
    /// Any global assigned by the script (i.e. not in the builtin snapshot,
    /// not prefixed with `__`, and not a function) is reactive state and gets
    /// synced to the template. Local variables are never synced.
    pub(super) fn sync_state_from_lua(&mut self) {
        if self.user_global_keys.is_empty() {
            // Full scan: discover all user globals (runs once per load_script).
            let user_globals: Vec<(String, LuaValue)> = self
                .env()
                .pairs::<String, LuaValue>()
                .filter_map(|result| result.ok())
                .filter(|(key, value)| {
                    !key.starts_with("__")
                        && !self.builtin_globals.contains(key)
                        && !matches!(value, LuaValue::Function(_))
                })
                .collect();
            for (name, lua_value) in user_globals {
                if let Ok(value) = self.lua().from_value::<Value>(lua_value) {
                    self.state.set(name.clone(), value);
                    self.user_global_keys.push(name);
                }
            }
        } else {
            // Fast path: only check known user global keys.
            for key in &self.user_global_keys {
                if let Ok(lua_value) = self.env().get::<LuaValue>(key.as_str()) {
                    if !matches!(lua_value, LuaValue::Nil | LuaValue::Function(_)) {
                        if let Ok(value) = self.lua().from_value::<Value>(lua_value) {
                            self.state.set(key.clone(), value);
                        }
                    }
                }
            }
            let known = self
                .user_global_keys
                .iter()
                .cloned()
                .collect::<HashSet<_>>();
            let new_user_globals: Vec<(String, LuaValue)> = self
                .env()
                .pairs::<String, LuaValue>()
                .filter_map(|result| result.ok())
                .filter(|(key, value)| {
                    !known.contains(key)
                        && !key.starts_with("__")
                        && !self.builtin_globals.contains(key)
                        && !matches!(value, LuaValue::Function(_))
                })
                .collect();
            for (name, lua_value) in new_user_globals {
                if let Ok(value) = self.lua().from_value::<Value>(lua_value) {
                    self.state.set(name.clone(), value);
                    self.user_global_keys.push(name);
                }
            }
        }

        self.sync_module_exports_from_lua();
        self.refresh_module_object();

        if self
            .env()
            .get::<bool>("__mesh_request_redraw")
            .unwrap_or(false)
        {
            self.state.dirty = true;
            let _ = self.env().set("__mesh_request_redraw", false);
        }
    }

    fn sync_side_channels(&mut self) {
        {
            let mut published = self.shared_published_events.lock().unwrap();
            if !published.is_empty() {
                self.published_events.extend(published.drain(..));
            }
        }
        {
            let mut diagnostics = self.shared_diagnostics.lock().unwrap();
            if !diagnostics.is_empty() {
                self.diagnostics.extend(diagnostics.drain(..));
            }
        }
        {
            let mut calls = self.shared_bound_instance_calls.lock().unwrap();
            if !calls.is_empty() {
                self.bound_instance_calls.extend(calls.drain(..));
            }
        }
        let changed_storage_keys = {
            let mut changed = self.changed_storage_keys.lock().unwrap();
            changed.drain().collect::<HashSet<_>>()
        };
        if !changed_storage_keys.is_empty() {
            let tracked_storage_keys = self.tracked_storage_keys.lock().unwrap();
            if changed_storage_keys
                .iter()
                .any(|key| tracked_storage_keys.contains(key))
            {
                self.state.dirty = true;
            }
        }
        let shared_interface_bindings = self.shared_interface_bindings.lock().unwrap();
        if self.interface_bindings_generation != shared_interface_bindings.generation {
            self.interface_bindings = shared_interface_bindings.bindings.clone();
            self.interface_bindings_generation = shared_interface_bindings.generation;
        }
    }

    fn sync_module_exports_from_lua(&mut self) {
        let Ok(module_table) = self.env().get::<Table>("module") else {
            return;
        };
        let Ok(exports) = module_table.get::<LuaValue>("exports") else {
            return;
        };
        if let Ok(value) = self.lua().from_value::<Value>(exports) {
            self.state.set("exports", value);
        }
    }

    fn refresh_module_object(&mut self) {
        // Skip the expensive full-state re-serialization when nothing has changed.
        // Proxy getters are external and can change without going through set(),
        // so we must always rebuild when proxies are present.
        let current_gen = self.state.snapshot_generation();
        if !self.state.has_proxies() && self.last_module_refresh_gen == current_gen {
            return;
        }

        let Ok(module_table) = self.env().get::<Table>("module") else {
            return;
        };
        if let Ok(state_value) = self.lua().to_value(&self.state.snapshot()) {
            let _ = module_table.set("state", state_value);
        }
        self.last_module_refresh_gen = current_gen;
    }
}

fn is_lifecycle_handler(name: &str) -> bool {
    matches!(name, "init" | "render" | "mount" | "unmount" | "onRender")
}

fn default_runtime_storage_root() -> PathBuf {
    std::env::temp_dir()
        .join("mesh")
        .join("runtime-storage")
        .join(std::process::id().to_string())
}

fn is_reserved_runtime_hook(name: &str) -> bool {
    is_lifecycle_handler(name)
}

fn is_bound_instance_self_arg(value: &LuaValue, child_instance_key: &str) -> bool {
    let LuaValue::Table(table) = value else {
        return false;
    };
    table
        .get::<String>("__instance_id")
        .is_ok_and(|instance_id| instance_id == child_instance_key)
}

fn is_named_event_channel(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn self_event_channel(lua: &Lua, module_id: &str, event_name: &str) -> mlua::Result<Table> {
    let globals = lua.globals();
    let registry = match globals.get::<LuaValue>("__mesh_self_event_channels") {
        Ok(LuaValue::Table(table)) => table,
        _ => {
            let table = lua.create_table()?;
            globals.set("__mesh_self_event_channels", table.clone())?;
            table
        }
    };
    let module_table = match registry.get::<LuaValue>(module_id)? {
        LuaValue::Table(table) => table,
        _ => {
            let table = lua.create_table()?;
            registry.set(module_id, table.clone())?;
            table
        }
    };
    match module_table.get::<LuaValue>(event_name)? {
        LuaValue::Table(channel) => Ok(channel),
        _ => {
            let channel = create_event_channel(lua)?;
            module_table.set(event_name, channel.clone())?;
            Ok(channel)
        }
    }
}

fn create_i18n_library(lua: &Lua) -> mlua::Result<Table> {
    let exports = lua.create_table()?;
    exports.set(
        "t",
        lua.create_function(|_lua, key: LuaValue| match key {
            LuaValue::String(value) => Ok(value.to_str()?.to_string()),
            other => Ok(lua_value_to_string(other)),
        })?,
    )?;
    Ok(exports)
}

fn resolve_host_api(mesh: &Table, module: &str) -> mlua::Result<Option<Table>> {
    if module.contains('@') {
        return Ok(None);
    }
    let Some(api_name) = module.strip_prefix("mesh.") else {
        return Ok(None);
    };
    match api_name {
        "events" | "ui" | "log" | "popover" | "locale" => mesh.get(api_name).map(Some),
        _ => Ok(None),
    }
}

fn is_component_definition_specifier(module: &str) -> bool {
    module.ends_with(".mesh")
        || module.starts_with("./")
        || module.starts_with("../")
        || (module.starts_with("@") && !module[1..].contains('@'))
}
