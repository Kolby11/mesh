use super::element_ref::{ElementAction, create_refs_proxy, install_bound_element_proxies};
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
use crate::util::{default_runtime_storage_root, is_named_event_channel};
use mesh_core_capability::CapabilitySet;
use mesh_core_elements::VariableStore;
use mesh_core_service::{InterfaceCatalog, InterfaceResolution};
use mlua::{
    Error as LuaError, Function, Lua, LuaSerdeExt, MultiValue, Table, Value as LuaValue, Variadic,
};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Default)]
struct SharedInterfaceBindings {
    bindings: HashMap<String, InterfaceResolution>,
    generation: u64,
}

/// Backing VM for a [`ScriptContext`].
///
/// A standalone context (backend modules, tests) owns a private pool VM. A
/// frontend component that belongs to a surface instead borrows a clone of the
/// surface's shared `Lua` so that every component in the surface lives in one
/// realm — the prerequisite for live `bind:this` cross-component references.
/// `mlua`'s `Lua` is a cheap reference-counted handle; clones share one VM.
#[derive(Debug)]
enum ScriptVm {
    Pooled(pool::PooledVm),
    Shared(Lua),
}

impl ScriptVm {
    fn lua(&self) -> &Lua {
        match self {
            ScriptVm::Pooled(vm) => vm.lua(),
            ScriptVm::Shared(lua) => lua,
        }
    }
}

/// An opaque, shareable Lua realm owned by a single frontend surface.
///
/// Every component instance in the surface attaches a clone of the same
/// `SurfaceVm` (via [`ScriptContext::attach_shared_vm`]) so they all live in one
/// realm — the prerequisite for live `bind:this` cross-component references.
/// Clones are cheap handles to the same VM. Holding this type lets the shell own
/// the realm without depending on `mlua` directly.
#[derive(Clone, Debug)]
pub struct SurfaceVm(Lua);

impl SurfaceVm {
    /// Create a fresh sandboxed realm for one surface.
    pub fn new() -> Self {
        let lua = Lua::new();
        lua.sandbox(true).expect("surface vm sandbox init failed");
        Self(lua)
    }

    pub(crate) fn handle(&self) -> Lua {
        self.0.clone()
    }
}

impl Default for SurfaceVm {
    fn default() -> Self {
        Self::new()
    }
}

fn json_value_fingerprint(value: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_json_value(value, &mut hasher);
    hasher.finish()
}

fn hash_json_value(value: &Value, hasher: &mut DefaultHasher) {
    match value {
        Value::Null => 0u8.hash(hasher),
        Value::Bool(value) => {
            1u8.hash(hasher);
            value.hash(hasher);
        }
        Value::Number(value) => {
            2u8.hash(hasher);
            if let Some(value) = value.as_i64() {
                0u8.hash(hasher);
                value.hash(hasher);
            } else if let Some(value) = value.as_u64() {
                1u8.hash(hasher);
                value.hash(hasher);
            } else if let Some(value) = value.as_f64() {
                2u8.hash(hasher);
                value.to_bits().hash(hasher);
            } else {
                3u8.hash(hasher);
                value.to_string().hash(hasher);
            }
        }
        Value::String(value) => {
            3u8.hash(hasher);
            value.hash(hasher);
        }
        Value::Array(values) => {
            4u8.hash(hasher);
            values.len().hash(hasher);
            for value in values {
                hash_json_value(value, hasher);
            }
        }
        Value::Object(map) => {
            5u8.hash(hasher);
            map.len().hash(hasher);
            for (key, value) in map {
                key.hash(hasher);
                hash_json_value(value, hasher);
            }
        }
    }
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
    optional_interfaces: Arc<HashSet<String>>,
    vm: Option<ScriptVm>,
    /// When set, [`ensure_initialized`] runs on this shared VM (a clone of the
    /// owning surface's `Lua`) instead of checking out a private pool VM. Must
    /// be attached before the script is loaded.
    shared_vm: Option<Lua>,
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
    user_global_key_set: HashSet<String>,
    assigned_global_keys: Arc<Mutex<HashSet<String>>>,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    subscribed_interface_events: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
    published_events: Vec<PublishedEvent>,
    shared_published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    diagnostics: Vec<ScriptDiagnostic>,
    shared_diagnostics: Arc<Mutex<Vec<ScriptDiagnostic>>>,
    element_actions: Vec<ElementAction>,
    shared_element_actions: Arc<Mutex<Vec<ElementAction>>>,
    storage: Arc<Mutex<ScopedStorage>>,
    tracked_storage_keys: Arc<Mutex<HashSet<String>>>,
    changed_storage_keys: Arc<Mutex<HashSet<String>>>,
    tracking_storage_reads: Arc<Mutex<bool>>,
    pending_side_channels: Arc<AtomicBool>,
    /// Set when another component instance touches this context through a live
    /// `bind:this` proxy. The shell consumes this before doing the expensive
    /// cross-instance state resync.
    live_binding_external_accessed: Arc<AtomicBool>,
    /// `state.snapshot_generation()` at the time of the last `refresh_module_object` call.
    /// When this matches the current generation (and no proxies exist), the Lua
    /// `module.state` table is already up to date and the rebuild can be skipped.
    last_module_refresh_gen: u64,
    /// Fingerprint of the last element metrics snapshot converted into Lua.
    /// Shell paints commonly publish identical geometry across many frames.
    last_element_metrics_fingerprint: Option<u64>,
    cached_self_table: Option<Table>,
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

    /// Attach a shared surface VM to run on instead of a private pool checkout.
    ///
    /// Used by a frontend surface so all of its component instances share one
    /// Lua realm. Must be called before the script is loaded/initialized; it has
    /// no effect once the context is already initialized.
    pub fn attach_shared_vm(&mut self, vm: &SurfaceVm) {
        self.shared_vm = Some(vm.handle());
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
        let module_id = module_id.into();
        Self::new_with_storage_scope_inner(
            module_id.clone(),
            module_id.clone(),
            module_id,
            capabilities,
            default_runtime_storage_root(),
        )
    }

    /// Create a frontend context whose durable storage is isolated to one
    /// concrete component instance.
    pub fn new_for_instance(
        module_id: impl Into<String>,
        component_id: impl Into<String>,
        instance_id: impl Into<String>,
        capabilities: CapabilitySet,
    ) -> Result<Self, ScriptError> {
        Self::new_with_storage_scope_inner(
            module_id.into(),
            component_id.into(),
            instance_id.into(),
            capabilities,
            default_runtime_storage_root(),
        )
    }

    #[cfg(test)]
    pub fn new_with_storage_root(
        module_id: impl Into<String>,
        capabilities: CapabilitySet,
        storage_root: impl Into<PathBuf>,
    ) -> Result<Self, ScriptError> {
        let module_id = module_id.into();
        Self::new_with_storage_scope_inner(
            module_id.clone(),
            module_id.clone(),
            module_id,
            capabilities,
            storage_root,
        )
    }

    #[cfg(test)]
    pub fn new_with_storage_scope(
        module_id: impl Into<String>,
        component_id: impl Into<String>,
        instance_id: impl Into<String>,
        capabilities: CapabilitySet,
        storage_root: impl Into<PathBuf>,
    ) -> Result<Self, ScriptError> {
        Self::new_with_storage_scope_inner(
            module_id.into(),
            component_id.into(),
            instance_id.into(),
            capabilities,
            storage_root,
        )
    }

    fn new_with_storage_scope_inner(
        module_id: String,
        component_id: String,
        instance_id: String,
        capabilities: CapabilitySet,
        storage_root: impl Into<PathBuf>,
    ) -> Result<Self, ScriptError> {
        let storage = StorageManager::new(storage_root.into()).open(StorageScope::frontend(
            module_id.clone(),
            component_id,
            instance_id,
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
            optional_interfaces: Arc::new(HashSet::new()),
            vm: None,
            shared_vm: None,
            env_table: None,
            interface_catalog: InterfaceCatalog::default(),
            interface_bindings: HashMap::new(),
            shared_interface_bindings: Arc::new(Mutex::new(SharedInterfaceBindings::default())),
            interface_bindings_generation: 0,
            builtin_globals: HashSet::new(),
            user_global_keys: Vec::new(),
            user_global_key_set: HashSet::new(),
            assigned_global_keys: Arc::new(Mutex::new(HashSet::new())),
            tracked_service_fields: Arc::new(Mutex::new(HashMap::new())),
            subscribed_interface_events: Arc::new(Mutex::new(HashMap::new())),
            published_events: Vec::new(),
            shared_published_events: Arc::new(Mutex::new(Vec::new())),
            diagnostics: storage_diagnostics,
            shared_diagnostics: Arc::new(Mutex::new(Vec::new())),
            element_actions: Vec::new(),
            shared_element_actions: Arc::new(Mutex::new(Vec::new())),
            storage: Arc::new(Mutex::new(storage)),
            tracked_storage_keys: Arc::new(Mutex::new(HashSet::new())),
            changed_storage_keys: Arc::new(Mutex::new(HashSet::new())),
            tracking_storage_reads: Arc::new(Mutex::new(false)),
            pending_side_channels: Arc::new(AtomicBool::new(false)),
            live_binding_external_accessed: Arc::new(AtomicBool::new(false)),
            last_module_refresh_gen: u64::MAX,
            last_element_metrics_fingerprint: None,
            cached_self_table: None,
        })
    }

    pub fn set_interface_catalog(&mut self, catalog: InterfaceCatalog) {
        self.interface_catalog = catalog;
    }

    pub fn set_optional_interfaces(&mut self, interfaces: HashSet<String>) {
        self.optional_interfaces = Arc::new(interfaces);
    }

    /// Check out a pooled VM, create a per-component _ENV table, install host API,
    /// sandbox the thread, and populate builtin_globals. Idempotent — does nothing
    /// if already initialized.
    fn ensure_initialized(&mut self) -> Result<(), ScriptError> {
        if self.vm.is_some() {
            return Ok(());
        }
        let vm = match self.shared_vm.clone() {
            Some(lua) => ScriptVm::Shared(lua),
            None => ScriptVm::Pooled(pool::checkout()),
        };
        let lua = vm.lua();

        // ISO-01: create per-component _ENV with __index = globals() fallthrough
        let env = lua.create_table().map_err(lua_err)?;
        let meta = lua.create_table().map_err(lua_err)?;
        meta.set("__index", lua.globals()).map_err(lua_err)?;
        let assigned_global_keys = Arc::clone(&self.assigned_global_keys);
        meta.set(
            "__newindex",
            lua.create_function(move |_, (table, key, value): (Table, String, LuaValue)| {
                if !key.starts_with("__") {
                    assigned_global_keys.lock().unwrap().insert(key.clone());
                }
                table.raw_set(key, value)
            })
            .map_err(lua_err)?,
        )
        .map_err(lua_err)?;
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
        self.assigned_global_keys.lock().unwrap().clear();

        Ok(())
    }

    /// Drop the env_table and return the pooled VM. ScriptContext methods
    /// must not be called after this without a subsequent ensure_initialized().
    pub fn uninit(&mut self) {
        self.env_table = None;
        self.cached_self_table = None;
        self.builtin_globals.clear();
        self.user_global_keys.clear();
        self.user_global_key_set.clear();
        self.last_element_metrics_fingerprint = None;
        self.assigned_global_keys.lock().unwrap().clear();
        self.subscribed_interface_events.lock().unwrap().clear();
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
        self.user_global_key_set.clear();
        self.assigned_global_keys.lock().unwrap().clear();
        {
            let mut shared_interface_bindings = self.shared_interface_bindings.lock().unwrap();
            shared_interface_bindings.bindings.clear();
            shared_interface_bindings.generation =
                shared_interface_bindings.generation.wrapping_add(1);
            self.interface_bindings_generation = shared_interface_bindings.generation;
        }
        self.shared_published_events.lock().unwrap().clear();
        self.shared_diagnostics.lock().unwrap().clear();
        self.shared_element_actions.lock().unwrap().clear();
        self.changed_storage_keys.lock().unwrap().clear();
        self.pending_side_channels.store(false, Ordering::Release);
        self.clear_tracked_service_fields();
        self.clear_subscribed_interface_events();
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

    /// Copy the latest service payload into the Lua runtime for proxy reads.
    ///
    /// Called by core after each service update so interface proxies can read
    /// state fields directly without explicit callback or binding APIs.
    pub fn apply_service_payload(&mut self, service: &str, payload: &Value) {
        let _ = self.ensure_initialized();
        let payload_marker = payload as *const Value as usize;
        let payload_fingerprint = json_value_fingerprint(payload);
        let marker = format!("{payload_marker}:{payload_fingerprint}");
        let globals = self.lua().globals();
        let marker_table = match globals.get::<Table>("__mesh_service_payload_ptrs") {
            Ok(table) => table,
            Err(_) => match self.lua().create_table() {
                Ok(table) => {
                    let _ = globals.set("__mesh_service_payload_ptrs", table.clone());
                    table
                }
                Err(_) => return,
            },
        };
        if marker_table
            .get::<Option<String>>(service)
            .ok()
            .flatten()
            .is_some_and(|previous| previous == marker)
        {
            return;
        }
        let service_key = format!("__mesh_svc_{service}");
        if let Ok(lua_value) = self.lua().to_value(payload) {
            // Set on globals() so proxy __index (service_payload_field) can find it.
            // The env table __index falls through to globals, so scripts accessing
            // __mesh_svc_* directly from the env also work.
            let _ = self.lua().globals().set(service_key.as_str(), lua_value);
            let _ = marker_table.set(service, marker);
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

    #[cfg(test)]
    pub fn service_payload_marker_for_test(&mut self, service: &str) -> Option<String> {
        let _ = self.ensure_initialized();
        self.lua()
            .globals()
            .get::<Table>("__mesh_service_payload_ptrs")
            .ok()
            .and_then(|table| table.get::<Option<String>>(service).ok().flatten())
    }

    /// Publish the latest per-paint element metrics so live `refs.<name>` reads
    /// reflect the current frame's geometry/state.
    ///
    /// `metrics` is a `{ name -> fields }` object (the same shape the shell builds
    /// from the painted tree). Stored on the shared realm's globals so every
    /// component in the surface reads through `_ENV.__index -> globals`.
    pub fn apply_element_metrics(&mut self, metrics: &Value) {
        self.apply_element_metrics_inner(metrics);
    }

    /// Publish element metrics only when the producer's full-snapshot
    /// fingerprint differs from the last snapshot installed in this context.
    pub fn apply_element_metrics_with_fingerprint(&mut self, metrics: &Value, fingerprint: u64) {
        if self.last_element_metrics_fingerprint == Some(fingerprint) {
            return;
        }
        self.apply_element_metrics_inner(metrics);
        self.last_element_metrics_fingerprint = Some(fingerprint);
    }

    fn apply_element_metrics_inner(&mut self, metrics: &Value) {
        let _ = self.ensure_initialized();
        if let Ok(lua_value) = self.lua().to_value(metrics) {
            let _ = self
                .lua()
                .globals()
                .set("__mesh_element_metrics", lua_value);
        }
        let _ = install_bound_element_proxies(
            self.lua(),
            self.env(),
            metrics,
            Arc::clone(&self.shared_element_actions),
            Arc::clone(&self.pending_side_channels),
        );
    }

    pub fn emit_interface_event(
        &mut self,
        service: &str,
        event_name: &str,
        payload: &Value,
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let scope = self.env().clone();
        let channel = interface_event_channel(self.lua(), &scope, service, event_name, None)
            .map_err(lua_err)?;
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

    /// Set a public script member on *this component's* `_ENV`, where the
    /// script's own bare assignments live. A bare `foo = ...` in a component
    /// shadows any `globals()` fallthrough, so writing back through
    /// [`set_global_state`] would not be observed by the script's own handlers
    /// or `render`. Use this when the shell needs to push a value into a member
    /// the component declared itself — e.g. syncing a portal `hidden={...}`
    /// binding back to `false`/`true` after the shell shows/hides the bound
    /// surface through a path the trigger script never ran.
    pub fn set_member_state(&mut self, name: &str, value: Value) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let lua_value = self.lua().to_value(&value).map_err(lua_err)?;
        self.env().set(name, lua_value).map_err(map_lua_error)?;
        self.state.set(name.to_string(), value);
        self.refresh_module_object();
        Ok(())
    }

    pub fn tracked_service_fields(&self) -> HashMap<String, HashSet<String>> {
        self.tracked_service_fields.lock().unwrap().clone()
    }

    pub fn has_tracked_fields_for_service(&self, service: &str) -> bool {
        self.tracked_service_fields
            .lock()
            .unwrap()
            .get(service)
            .is_some_and(|fields| !fields.is_empty())
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

    pub fn subscribed_interface_events(&self) -> HashMap<String, HashSet<String>> {
        self.subscribed_interface_events
            .lock()
            .unwrap()
            .iter()
            .map(|(service, events)| {
                (
                    service.clone(),
                    events
                        .iter()
                        .filter(|(_, count)| **count > 0)
                        .map(|(event, _)| event.clone())
                        .collect(),
                )
            })
            .filter(|(_, events): &(String, HashSet<String>)| !events.is_empty())
            .collect()
    }

    pub fn has_interface_event_subscription_for_service(&self, service: &str) -> bool {
        self.subscribed_interface_events
            .lock()
            .unwrap()
            .get(service)
            .is_some_and(|events| events.values().any(|count| *count > 0))
    }

    pub fn is_subscribed_to_interface_event(&self, service: &str, event_name: &str) -> bool {
        self.subscribed_interface_events
            .lock()
            .unwrap()
            .get(service)
            .and_then(|events| events.get(event_name))
            .is_some_and(|count| *count > 0)
    }

    pub fn clear_subscribed_interface_events(&self) {
        self.subscribed_interface_events.lock().unwrap().clear();
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

    /// Install a **live** `bind:this` reference to another component instance.
    ///
    /// Builds a proxy table whose metatable forwards `__index`/`__newindex`
    /// straight to the child's live `_ENV`. Because parent and child share one
    /// surface VM (see [`SurfaceVm`]), the forwarded `Table` handle is valid in
    /// the parent — reads see the child's current value, calls run the child's
    /// real function synchronously and return its real value, all with no copy.
    ///
    /// Host internals are hidden by a denylist sourced from the child's
    /// `builtin_globals` (`self`, `module`, `mesh`, `require`, the `__mesh_*`
    /// sentinels) plus the lifecycle hooks, so only the child's public values and
    /// functions pass through. Takes `&self`/`&child` (no `&mut`) so the caller
    /// can borrow both runtimes out of one map guard at once; both must already
    /// be initialized (the binding is refreshed every render once they are).
    pub fn install_live_binding(
        &self,
        binding: &str,
        child: &ScriptContext,
    ) -> Result<(), ScriptError> {
        if self.vm.is_none() || child.vm.is_none() {
            return Ok(());
        }
        let lua = self.lua();
        let child_env = child.env().clone();
        let denylist = child.builtin_globals.clone();
        let child_external_accessed = Arc::clone(&child.live_binding_external_accessed);

        let proxy = lua.create_table().map_err(lua_err)?;
        let meta = lua.create_table().map_err(lua_err)?;

        let index_env = child_env.clone();
        let index_deny = denylist.clone();
        let index_external_accessed = Arc::clone(&child_external_accessed);
        meta.set(
            "__index",
            lua.create_function(move |lua, (_proxy, key): (Table, String)| {
                if is_denied_binding_key(&key, &index_deny) {
                    return Ok(LuaValue::Nil);
                }
                // raw_get keeps the surface curated: only the child's own public
                // members are exposed, not globals inherited via `_ENV.__index`.
                let raw = index_env.raw_get::<LuaValue>(key.as_str())?;
                if !matches!(raw, LuaValue::Nil) {
                    if let LuaValue::Function(function) = raw {
                        let accessed = Arc::clone(&index_external_accessed);
                        return lua
                            .create_function(move |_lua, args: Variadic<LuaValue>| {
                                accessed.store(true, Ordering::Release);
                                function.call::<MultiValue>(args)
                            })
                            .map(LuaValue::Function);
                    }
                    return Ok(raw);
                }
                // Child→parent events: a named-channel key with no public member
                // resolves the child's live `self.<Event>` channel, so the parent
                // can `child.Event:on(fn)` and receive the child's synchronous
                // `self.Event:fire(...)` in the same tick (same channel table,
                // shared VM, no marshalling).
                if is_named_event_channel(&key) {
                    return self_event_channel(lua, &index_env, &key).map(LuaValue::Table);
                }
                Ok(LuaValue::Nil)
            })
            .map_err(lua_err)?,
        )
        .map_err(lua_err)?;

        let newindex_env = child_env;
        let newindex_external_accessed = child_external_accessed;
        meta.set(
            "__newindex",
            lua.create_function(move |_, (_proxy, key, value): (Table, String, LuaValue)| {
                if is_denied_binding_key(&key, &denylist) {
                    return Ok(());
                }
                newindex_external_accessed.store(true, Ordering::Release);
                newindex_env.raw_set(key, value)
            })
            .map_err(lua_err)?,
        )
        .map_err(lua_err)?;

        proxy.set_metatable(Some(meta)).map_err(lua_err)?;
        self.env().set(binding, proxy).map_err(map_lua_error)?;
        Ok(())
    }

    /// Re-sync reactive state after a live `bind:this` cross-call mutated this
    /// child's `_ENV` directly (bypassing the shell's normal post-handler sync).
    ///
    /// A parent calling `child.set_volume(50)` through a live binding runs the
    /// child's function synchronously in the shared VM, so the child's Lua `_ENV`
    /// changes but its Rust-side `ScriptState` does not. The shell calls this on
    /// every bound child after a parent handler so `{bound vars}` re-render.
    pub fn resync_state(&mut self) {
        if self.vm.is_none() {
            return;
        }
        self.sync_state_from_lua();
        self.sync_side_channels();
    }

    /// Returns whether another component touched this context through a live
    /// `bind:this` proxy since the last call, clearing the flag.
    pub fn take_live_binding_external_accessed(&self) -> bool {
        self.live_binding_external_accessed
            .swap(false, Ordering::AcqRel)
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

    /// Call the canonical `render(self)` lifecycle handler if present.
    pub fn call_render_lifecycle(&mut self) -> Result<bool, ScriptError> {
        self.ensure_initialized()?;
        if self.has_handler("render") {
            self.clear_tracked_storage_keys();
            *self.tracking_storage_reads.lock().unwrap() = true;
            let result = self.call_handler("render", &[]);
            *self.tracking_storage_reads.lock().unwrap() = false;
            result?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn current_self_table(&mut self) -> Result<Table, ScriptError> {
        if let Some(table) = &self.cached_self_table {
            return Ok(table.clone());
        }
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
        let pending_storage_side_channels = Arc::clone(&self.pending_side_channels);
        let pending_storage_diagnostics = Arc::clone(&self.pending_side_channels);
        let storage = create_lua_storage_table(
            self.lua(),
            Arc::clone(&self.storage),
            Arc::new(move |reason| {
                pending_storage_diagnostics.store(true, Ordering::Release);
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
                pending_storage_side_channels.store(true, Ordering::Release);
                changed_storage_keys.lock().unwrap().insert(key);
            }),
        )
        .map_err(lua_err)?;
        current_self.set("storage", storage).map_err(lua_err)?;
        // Self event channels (`self.Changed`) are registered on the per-instance
        // _ENV so two instances of the same component keep independent channels
        // when they share one surface VM.
        let self_events_scope = self.env().clone();
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
                        let channel = self_event_channel(lua, &self_events_scope, &key)?;
                        table.set(key.as_str(), channel.clone())?;
                        Ok(LuaValue::Table(channel))
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;
        current_self
            .set_metatable(Some(self_events_meta))
            .map_err(lua_err)?;
        self.cached_self_table = Some(current_self.clone());
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

    pub fn drain_diagnostics(&mut self) -> Vec<ScriptDiagnostic> {
        self.sync_side_channels();
        std::mem::take(&mut self.diagnostics)
    }

    /// Drain imperative element actions (`refs.<name>:focus()`, …) queued by the
    /// script so the shell can execute them against the real widget tree.
    pub fn drain_element_actions(&mut self) -> Vec<ElementAction> {
        self.sync_side_channels();
        std::mem::take(&mut self.element_actions)
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
        self.install_module_api(globals)?;

        let mesh = self.lua().create_table().map_err(lua_err)?;
        let mesh_core_service = self.lua().create_table().map_err(lua_err)?;
        let mesh_core_events = self.lua().create_table().map_err(lua_err)?;
        let mesh_ui_api = self.lua().create_table().map_err(lua_err)?;
        let mesh_log = self.lua().create_table().map_err(lua_err)?;
        let mesh_popover = self.lua().create_table().map_err(lua_err)?;
        let mesh_locale = self.lua().create_table().map_err(lua_err)?;
        let manifest = HostApiManifest::from_capabilities(&self.capabilities);

        self.install_events_api(&mesh_core_events)?;
        self.install_ui_api(globals, &mesh_ui_api)?;
        self.install_locale_api(globals, &mesh_locale, &manifest)?;
        self.install_log_api(&mesh_log)?;
        self.install_popover_api(&mesh_popover)?;

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

        self.install_loader_api(globals, &mesh_for_require, &manifest)?;
        self.install_refs_api(globals)?;
        Ok(())
    }

    fn install_module_api(&mut self, globals: &mlua::Table) -> Result<(), ScriptError> {
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
                        let channel = create_event_channel(lua, None, None)?;
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
        globals.set("module", module_object).map_err(lua_err)
    }

    fn install_events_api(&mut self, mesh_core_events: &Table) -> Result<(), ScriptError> {
        let published_events = Arc::clone(&self.shared_published_events);
        let pending_side_channels = Arc::clone(&self.pending_side_channels);
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
                        pending_side_channels.store(true, Ordering::Release);
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
            .map_err(lua_err)
    }

    fn install_ui_api(
        &mut self,
        globals: &mlua::Table,
        mesh_ui_api: &Table,
    ) -> Result<(), ScriptError> {
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
            .map_err(lua_err)
    }

    fn install_locale_api(
        &mut self,
        globals: &mlua::Table,
        mesh_locale: &Table,
        manifest: &HostApiManifest,
    ) -> Result<(), ScriptError> {
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

        let has_locale_write = manifest.has_locale_write;
        let published_events_for_locale = Arc::clone(&self.shared_published_events);
        let pending_side_channels_for_locale = Arc::clone(&self.pending_side_channels);
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
                        pending_side_channels_for_locale.store(true, Ordering::Release);
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
            .map_err(lua_err)
    }

    fn install_log_api(&mut self, mesh_log: &Table) -> Result<(), ScriptError> {
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
            .map_err(lua_err)
    }

    fn install_popover_api(&mut self, mesh_popover: &Table) -> Result<(), ScriptError> {
        let published_events_for_popover = Arc::clone(&self.shared_published_events);
        let pending_side_channels_for_popover = Arc::clone(&self.pending_side_channels);
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
                        pending_side_channels_for_popover.store(true, Ordering::Release);
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
        let pending_side_channels_for_popover = Arc::clone(&self.pending_side_channels);
        let module_id_for_popover = self.module_id.clone();
        let capabilities_for_popover = self.capabilities.clone();
        mesh_popover
            .set(
                "hide",
                self.lua()
                    .create_function(move |_lua, args: Variadic<LuaValue>| {
                        let Some(LuaValue::String(surface_id)) = args.first() else {
                            return Err(LuaError::FromLuaConversionError {
                                from: "nil",
                                to: "String".to_string(),
                                message: Some("mesh.popover.hide expects a surface id".into()),
                            });
                        };
                        let surface_id = surface_id.to_str()?.to_string();
                        let defer_for_hover_bridge = match args.get(1) {
                            Some(LuaValue::Table(table)) => table
                                .get::<Option<bool>>("bridge")?
                                .or_else(|| {
                                    table
                                        .get::<Option<bool>>("defer_for_hover_bridge")
                                        .ok()
                                        .flatten()
                                })
                                .unwrap_or(false),
                            _ => false,
                        };
                        pending_side_channels_for_popover.store(true, Ordering::Release);
                        published_events_for_popover
                            .lock()
                            .unwrap()
                            .push(PublishedEvent {
                                channel: "shell.hide-popover".to_string(),
                                payload: serde_json::json!({
                                    "surface_id": surface_id,
                                    "defer_for_hover_bridge": defer_for_hover_bridge,
                                }),
                                source_module_id: module_id_for_popover.clone(),
                                source_capabilities: capabilities_for_popover.clone(),
                            });
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)
    }

    fn install_loader_api(
        &mut self,
        globals: &mlua::Table,
        mesh_for_require: &Table,
        manifest: &HostApiManifest,
    ) -> Result<(), ScriptError> {
        let interface_catalog = self.interface_catalog.clone();
        let allowed_interfaces = manifest.interface_capabilities.clone();
        let has_theme_read = manifest.has_theme_read;
        let has_locale_read = manifest.has_locale_read;
        let published_events = Arc::clone(&self.shared_published_events);
        let pending_side_channels = Arc::clone(&self.pending_side_channels);
        let tracked_service_fields = Arc::clone(&self.tracked_service_fields);
        let subscribed_interface_events = Arc::clone(&self.subscribed_interface_events);
        let module_id_for_require = self.module_id.clone();
        let capabilities_for_require = self.capabilities.clone();
        let diagnostics_for_require = Arc::clone(&self.shared_diagnostics);
        let pending_diagnostics_for_require = Arc::clone(&self.pending_side_channels);
        let optional_interfaces_for_require = Arc::clone(&self.optional_interfaces);
        // The per-instance _ENV is the channel-registry scope so interface event
        // channels stay private when components share one surface VM.
        let scope_for_require = globals.clone();
        let mesh_for_require = mesh_for_require.clone();
        let require = self
            .lua()
            .create_function(move |lua, module: String| {
                if module == "@mesh/i18n" || module == "mesh.i18n" {
                    return create_i18n_library(lua).map(LuaValue::Table);
                }

                if let Some(host_api) = resolve_host_api(&mesh_for_require, &module)? {
                    return Ok(LuaValue::Table(host_api));
                }

                if is_component_definition_specifier(&module) {
                    let definition = lua.create_table()?;
                    definition.set("__mesh_component_definition", true)?;
                    definition.set("source", module.as_str())?;
                    return Ok(LuaValue::Table(definition));
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
                        &pending_diagnostics_for_require,
                        &module_id_for_require,
                        &canonical,
                        version.as_deref(),
                        "capability denied",
                        ScriptError::CapabilityDenied(canonical.clone()),
                    ));
                }

                let resolution = interface_catalog.resolve(&canonical, version.as_deref());
                if resolution.provider.is_none() {
                    if optional_interfaces_for_require.contains(&canonical) {
                        return Ok(LuaValue::Nil);
                    }
                    let reason = lookup_failure_reason(&interface_catalog, &resolution);
                    return Err(record_lookup_diagnostic_lua(
                        &diagnostics_for_require,
                        &pending_diagnostics_for_require,
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
                    &scope_for_require,
                    resolution,
                    module_id_for_require.clone(),
                    capabilities_for_require.clone(),
                    Arc::clone(&tracked_service_fields),
                    Arc::clone(&subscribed_interface_events),
                    Arc::clone(&published_events),
                    Arc::clone(&pending_side_channels),
                )?;
                Ok(LuaValue::Table(proxy))
            })
            .map_err(lua_err)?;
        globals.set("require", require.clone()).map_err(lua_err)?;

        // `import(spec, ...names)` is the named-import companion to `require`:
        // it resolves the module through the very same `require` resolver (so
        // resolution and reactive field-tracking can never drift) and returns
        // the requested fields as multiple values, mirroring JS named imports.
        //
        //   local i18n = require("mesh.i18n")          -- default import
        //   local t, plural = import("mesh.i18n", "t", "plural")  -- named
        //   local translate = import("mesh.i18n", "t") -- rename freely
        //
        // With no names it is equivalent to `require` (returns the module).
        // Reading `module[name]` goes through the resolved table/proxy's
        // `__index`, so interface-proxy field reads stay tracked exactly as a
        // direct `audio.percent` access would be.
        let import = self
            .lua()
            .create_function(move |_lua, args: Variadic<LuaValue>| {
                let mut iter = args.into_iter();
                let spec = match iter.next() {
                    Some(LuaValue::String(spec)) => spec,
                    _ => {
                        return Err(LuaError::external(ScriptError::LuaError(
                            "import expects a module specifier string as its first argument"
                                .to_string(),
                        )));
                    }
                };

                let module: LuaValue = require.call(spec.clone())?;

                let names: Vec<LuaValue> = iter.collect();
                if names.is_empty() {
                    return Ok(Variadic::from_iter(std::iter::once(module)));
                }

                let LuaValue::Table(table) = &module else {
                    return Err(LuaError::external(ScriptError::LuaError(format!(
                        "import: module {:?} has no named members",
                        spec.to_string_lossy()
                    ))));
                };

                let mut results = Vec::with_capacity(names.len());
                for name in names {
                    let LuaValue::String(key) = name else {
                        return Err(LuaError::external(ScriptError::LuaError(
                            "import: member names must be strings".to_string(),
                        )));
                    };
                    results.push(table.get::<LuaValue>(key)?);
                }
                Ok(Variadic::from_iter(results))
            })
            .map_err(lua_err)?;
        globals.set("import", import).map_err(lua_err)?;
        Ok(())
    }

    fn install_refs_api(&mut self, globals: &mlua::Table) -> Result<(), ScriptError> {
        // `refs.<name>` is a live element-node reference: geometry/state fields
        // read from the latest paint (`__mesh_element_metrics`, published by the
        // shell each frame) and methods (`focus`, `blur`, …) enqueue element
        // actions the shell executes against the real widget tree.
        let refs_proxy = create_refs_proxy(
            self.lua(),
            globals,
            Arc::clone(&self.shared_element_actions),
            Arc::clone(&self.pending_side_channels),
        )
        .map_err(lua_err)?;
        globals.set("refs", refs_proxy).map_err(lua_err)?;
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
                    &self.pending_side_channels,
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
                // Optional interfaces resolve to `nil` rather than aborting: the
                // script's own `require("mesh.x")` then returns nil via the lazy
                // path. Leave the alias unbound so it falls through to nil.
                if self.optional_interfaces.contains(&canonical) {
                    globals
                        .set(import.alias.as_str(), LuaValue::Nil)
                        .map_err(lua_err)?;
                    continue;
                }
                let reason = lookup_failure_reason(&self.interface_catalog, &resolution);
                record_lookup_diagnostic(
                    &self.shared_diagnostics,
                    &self.pending_side_channels,
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
                self.pending_side_channels.store(true, Ordering::Release);
            }
            let proxy = create_interface_proxy(
                self.lua(),
                &globals,
                resolution,
                self.module_id.clone(),
                self.capabilities.clone(),
                Arc::clone(&self.tracked_service_fields),
                Arc::clone(&self.subscribed_interface_events),
                Arc::clone(&self.shared_published_events),
                Arc::clone(&self.pending_side_channels),
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
        let _span = tracing::debug_span!("sync_state_from_lua", module = %self.module_id).entered();
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
                    self.user_global_key_set.insert(name.clone());
                    self.user_global_keys.push(name);
                }
            }
            self.assigned_global_keys.lock().unwrap().clear();
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
            let assigned_keys = {
                let mut assigned = self.assigned_global_keys.lock().unwrap();
                assigned.drain().collect::<Vec<_>>()
            };
            for name in assigned_keys {
                if name.starts_with("__")
                    || self.builtin_globals.contains(&name)
                    || self.user_global_key_set.contains(&name)
                {
                    continue;
                }
                self.user_global_key_set.insert(name.clone());
                self.user_global_keys.push(name.clone());
                if let Ok(lua_value) = self.env().get::<LuaValue>(name.as_str())
                    && !matches!(lua_value, LuaValue::Nil | LuaValue::Function(_))
                    && let Ok(value) = self.lua().from_value::<Value>(lua_value)
                {
                    self.state.set(name, value);
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
        if !self.pending_side_channels.swap(false, Ordering::AcqRel) {
            return;
        }
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
            let mut element_actions = self.shared_element_actions.lock().unwrap();
            if !element_actions.is_empty() {
                self.element_actions.extend(element_actions.drain(..));
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

    #[cfg(test)]
    pub(crate) fn has_user_global_key_for_test(&self, key: &str) -> bool {
        self.user_global_key_set.contains(key)
    }

    #[cfg(test)]
    pub(crate) fn clear_cached_self_table_for_benchmark(&mut self) {
        self.cached_self_table = None;
    }

    #[cfg(test)]
    pub(crate) fn pending_side_channels_for_test(&self) -> bool {
        self.pending_side_channels.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn sync_side_channels_for_benchmark(&mut self) {
        self.sync_side_channels();
    }

    #[cfg(test)]
    pub(crate) fn old_sync_side_channels_for_benchmark(&mut self) {
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
            let mut element_actions = self.shared_element_actions.lock().unwrap();
            if !element_actions.is_empty() {
                self.element_actions.extend(element_actions.drain(..));
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

    #[cfg(test)]
    pub(crate) fn call_lua_function_without_sync_for_test(
        &mut self,
        name: &str,
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let handler = self
            .env()
            .get::<Function>(name)
            .map_err(|_| ScriptError::HandlerNotFound(name.to_string()))?;
        handler.call::<()>(()).map_err(map_lua_error)
    }

    #[cfg(test)]
    pub(crate) fn old_sync_state_from_lua_scan_for_benchmark(&mut self) {
        for key in &self.user_global_keys {
            if let Ok(lua_value) = self.env().get::<LuaValue>(key.as_str())
                && !matches!(lua_value, LuaValue::Nil | LuaValue::Function(_))
                && let Ok(value) = self.lua().from_value::<Value>(lua_value)
            {
                self.state.set(key.clone(), value);
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
                self.user_global_key_set.insert(name.clone());
                self.user_global_keys.push(name);
            }
        }
        self.sync_module_exports_from_lua();
        self.refresh_module_object();
    }
}

fn is_lifecycle_handler(name: &str) -> bool {
    matches!(name, "init" | "render" | "mount" | "unmount" | "onRender")
}

fn is_reserved_runtime_hook(name: &str) -> bool {
    is_lifecycle_handler(name)
}

/// Gate for the live `bind:this` proxy: hide host internals so only the child's
/// public values and functions cross the boundary. `denylist` is the child's
/// `builtin_globals` (`self`, `module`, `mesh`, `require`, the `__mesh_*`
/// sentinels installed before user script execution).
fn is_denied_binding_key(key: &str, denylist: &HashSet<String>) -> bool {
    key.starts_with("__") || is_reserved_runtime_hook(key) || denylist.contains(key)
}

/// Resolve (or lazily create) a `self.<Event>` channel.
///
/// The registry lives on the per-instance `_ENV` table (`scope`) so two
/// instances of the same component keep independent `self` channels when they
/// share one surface VM.
fn self_event_channel(lua: &Lua, scope: &Table, event_name: &str) -> mlua::Result<Table> {
    let registry = match scope.raw_get::<LuaValue>("__mesh_self_event_channels")? {
        LuaValue::Table(table) => table,
        _ => {
            let table = lua.create_table()?;
            scope.raw_set("__mesh_self_event_channels", table.clone())?;
            table
        }
    };
    match registry.raw_get::<LuaValue>(event_name)? {
        LuaValue::Table(channel) => Ok(channel),
        _ => {
            let channel = create_event_channel(lua, None, None)?;
            registry.raw_set(event_name, channel.clone())?;
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
