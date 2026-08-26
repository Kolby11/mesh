use super::super::element_ref::{ElementAction, ElementMetricsStore};
use super::super::lookup::{lua_err, map_lua_error};
use super::super::state::ServiceContextState;
use super::super::{
    PublishedEvent, ScriptDiagnostic, ScriptDiagnosticCategory, ScriptError, ScriptInterfaceImport,
    ScriptState, ServiceCallCompletion,
};
use super::*;
use crate::policy::RuntimePolicy;
use crate::pool;
use crate::storage::{ScopedStorage, StorageManager, StorageScope};
use crate::util::default_runtime_storage_root;
use mesh_core_capability::CapabilitySet;
use mesh_core_locale::{CatalogEntry, LocalizedTextResolution, ModuleTranslator};
use mesh_core_service::{InterfaceCatalog, InterfaceResolution};
use mlua::{Lua, Table, Value as LuaValue};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

static NEXT_SCRIPT_CONTEXT_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

/// The immutable-at-read boundary shared by `mesh.locale.current()` and the
/// module-scoped `mesh.i18n` library. The shell replaces this cell with one
/// translation snapshot so a script cannot observe a new locale alongside an
/// older catalog (or the reverse) between host calls.
#[derive(Debug, Clone)]
pub(super) struct LocaleCell {
    pub(super) locale: String,
    pub(super) translations: HashMap<String, CatalogEntry>,
    pub(super) snapshot_revision: u64,
}

impl Default for LocaleCell {
    fn default() -> Self {
        Self {
            locale: "en".into(),
            translations: HashMap::new(),
            snapshot_revision: 0,
        }
    }
}

/// A script execution context for one component instance.
///
/// Scripts run as-written, with no source preprocessing. Reactive state follows
/// the standard Lua module pattern: bare global assignments are exported and
/// synced to the template; `local` variables stay private.
#[derive(Debug)]
pub struct ScriptContext {
    pub module_id: String,
    /// Component identity owning this context. It is distinct from the
    /// installed module so embedded components remain diagnosable.
    pub component_id: String,
    /// Stable frontend instance identity used for correlated service-result
    /// delivery when several components share one Lua VM.
    pub instance_id: String,
    /// Monotonic identity for this compiled environment generation.
    pub generation: u64,
    pub capabilities: CapabilitySet,
    pub state: ScriptState,
    pub(super) optional_interfaces: Arc<HashSet<String>>,
    pub(super) vm: Option<ScriptVm>,
    /// Optional surface-owned handle to the thread realm. Must be attached
    /// before the script is loaded so live bindings use the surface's handle.
    pub(super) shared_vm: Option<Lua>,
    /// Immutable policy and resource broker for the backing Luau realm.
    pub(super) realm_policy: RuntimePolicy,
    pub(super) env_table: Option<Table>,
    /// Scalar reactive globals, removed from the raw `_ENV` so every assignment
    /// passes through `__newindex`. Handler sync then consumes the write log
    /// instead of re-reading every unchanged scalar.
    pub(super) reactive_scalar_globals: Option<Table>,
    pub(super) interface_catalog: Arc<InterfaceCatalog>,
    pub(in crate::context) interface_bindings: HashMap<String, InterfaceResolution>,
    pub(super) shared_interface_bindings: Arc<Mutex<SharedInterfaceBindings>>,
    pub(super) interface_bindings_generation: u64,
    /// Global names present before user script execution (stdlib + host API).
    /// Sync skips these so only user-defined globals become reactive state.
    pub(super) builtin_globals: HashSet<String>,
    /// Keys from the first full globals walk after `load_script`. Later syncs
    /// use targeted `get` lookups instead of iterating the globals table.
    pub(super) user_global_keys: Vec<String>,
    pub(super) user_global_key_set: HashSet<String>,
    pub(super) proxied_scalar_global_keys: HashSet<String>,
    /// Whether the one-time discovery walk ran. Separate from
    /// `user_global_keys.is_empty()`: handler-only scripts have no globals.
    pub(super) user_globals_discovered: bool,
    pub(super) assigned_global_keys: Arc<Mutex<HashSet<String>>>,
    pub(super) pending_assigned_global_keys: Arc<AtomicBool>,
    /// Public members whose values changed during the most recent Lua sync.
    /// Reuses its allocation across handler calls.
    pub(super) changed_public_members: Vec<String>,
    /// Set after the first template evaluation. Before it, an empty dependency
    /// table means "unknown", not "expression-free", so writes stay conservative.
    pub(super) template_dependencies_ready: bool,
    pub(super) template_expression_cache: Mutex<TemplateExpressionCache>,
    pub(super) tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    pub(super) subscribed_interface_events: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
    pub(super) published_events: Vec<PublishedEvent>,
    pub(super) shared_published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    pub(super) diagnostics: Vec<ScriptDiagnostic>,
    pub(super) shared_diagnostics: Arc<Mutex<Vec<ScriptDiagnostic>>>,
    pub(super) element_actions: Vec<ElementAction>,
    pub(super) shared_element_actions: Arc<Mutex<Vec<ElementAction>>>,
    pub(super) storage: Arc<Mutex<ScopedStorage>>,
    pub(super) tracked_storage_keys: Arc<Mutex<HashSet<String>>>,
    pub(super) changed_storage_keys: Arc<Mutex<HashSet<String>>>,
    pub(super) tracking_storage_reads: Arc<AtomicBool>,
    pub(super) pending_side_channels: Arc<AtomicBool>,
    pub(super) pending_redraw: Arc<AtomicBool>,
    /// Set when another instance touches this context through a live
    /// `bind:this` proxy; gates the expensive cross-instance resync.
    pub(super) live_binding_external_accessed: Arc<AtomicBool>,
    /// Fingerprint of the last metrics snapshot published to the refs store;
    /// shell paints commonly repeat geometry across frames.
    pub(super) last_element_metrics_fingerprint: Option<u64>,
    /// Rust-owned live element snapshots. Individual `refs.<name>` proxies
    /// lower only their requested entry into Lua and cache it for this version.
    pub(super) shared_element_metrics: Arc<Mutex<ElementMetricsStore>>,
    pub(super) cached_self_table: Option<Table>,
    /// Capability-filtered service snapshots for this component instance.
    /// Never lower the backing map into the shared Lua globals.
    pub(super) service_context_state: Arc<Mutex<ServiceContextState>>,
    /// Terminal results for correlated service-call tickets created by this
    /// context. The shell writes completions through the owning component;
    /// Luau ticket handles read this store without crossing the VM boundary.
    pub(super) service_call_completions: Arc<Mutex<HashMap<u64, ServiceCallCompletion>>>,
    /// The module-scoped locale and catalog snapshot currently visible to
    /// `mesh.locale.current()` and `mesh.i18n.t()`.
    pub(super) locale_cell: Arc<Mutex<LocaleCell>>,
    /// Structured missing-key observations from Luau and template consumers.
    /// The shell drains these into stable per-key diagnostics.
    pub(super) localized_misses: Arc<Mutex<Vec<LocalizedTextResolution>>>,
}

impl Drop for ScriptContext {
    fn drop(&mut self) {
        self.flush_storage();
        self.uninit();
    }
}

impl ScriptContext {
    pub(super) fn lua(&self) -> &Lua {
        self.vm
            .as_ref()
            .expect("ScriptContext not initialized — call ensure_initialized first")
            .lua()
    }

    /// Attach the surface's handle so all of its component instances share one
    /// thread realm. No effect once the context is initialized.
    pub fn attach_shared_vm(&mut self, vm: &SurfaceVm) {
        if self.vm.is_some() {
            return;
        }
        self.shared_vm = Some(vm.handle());
        self.realm_policy = vm.policy();
        self.shared_element_metrics = Arc::clone(&vm.element_metrics);
    }

    pub(super) fn env(&self) -> &Table {
        self.env_table
            .as_ref()
            .expect("ScriptContext not initialized — call ensure_initialized first")
    }

    pub(super) fn proxy_scalar_global(&mut self, name: &str, value: LuaValue) -> mlua::Result<()> {
        self.env().raw_set(name, LuaValue::Nil)?;
        self.reactive_scalar_globals
            .as_ref()
            .expect("reactive scalar table initialized with _ENV")
            .raw_set(name, value)?;
        self.proxied_scalar_global_keys.insert(name.to_string());
        Ok(())
    }

    fn restore_proxied_scalars_to_env(&mut self) -> mlua::Result<()> {
        if self.proxied_scalar_global_keys.is_empty() {
            return Ok(());
        }
        let backing = self
            .reactive_scalar_globals
            .as_ref()
            .expect("reactive scalar table initialized with _ENV")
            .clone();
        let env = self.env().clone();
        for name in self.proxied_scalar_global_keys.drain() {
            let value = backing.raw_get::<LuaValue>(name.as_str())?;
            backing.raw_set(name.as_str(), LuaValue::Nil)?;
            if !matches!(value, LuaValue::Nil) {
                env.raw_set(name, value)?;
            }
        }
        Ok(())
    }

    pub(super) fn unproxy_scalar_global(
        &mut self,
        name: &str,
        value: LuaValue,
    ) -> mlua::Result<()> {
        self.reactive_scalar_globals
            .as_ref()
            .expect("reactive scalar table initialized with _ENV")
            .raw_set(name, LuaValue::Nil)?;
        if !matches!(value, LuaValue::Nil) {
            self.env().raw_set(name, value)?;
        }
        self.proxied_scalar_global_keys.remove(name);
        Ok(())
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
        let generation = NEXT_SCRIPT_CONTEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        let realm_policy = pool::thread_policy();
        let storage =
            StorageManager::new_with_limit(storage_root.into(), realm_policy.storage_budget())
                .open(StorageScope::frontend(
                    module_id.clone(),
                    component_id.clone(),
                    instance_id.clone(),
                ));
        let storage_diagnostics = storage
            .diagnostics()
            .iter()
            .map(|diagnostic| ScriptDiagnostic {
                module_id: module_id.clone(),
                category: ScriptDiagnosticCategory::Storage,
                interface: "self.storage".to_string(),
                requested_version: None,
                reason: diagnostic.reason.clone(),
            })
            .collect();
        Ok(Self {
            module_id,
            component_id,
            instance_id,
            generation,
            capabilities,
            state: ScriptState::new(),
            optional_interfaces: Arc::new(HashSet::new()),
            vm: None,
            shared_vm: None,
            realm_policy,
            env_table: None,
            reactive_scalar_globals: None,
            interface_catalog: Arc::new(InterfaceCatalog::default()),
            interface_bindings: HashMap::new(),
            shared_interface_bindings: Arc::new(Mutex::new(SharedInterfaceBindings::default())),
            interface_bindings_generation: 0,
            builtin_globals: HashSet::new(),
            user_global_keys: Vec::new(),
            user_global_key_set: HashSet::new(),
            proxied_scalar_global_keys: HashSet::new(),
            user_globals_discovered: false,
            assigned_global_keys: Arc::new(Mutex::new(HashSet::new())),
            pending_assigned_global_keys: Arc::new(AtomicBool::new(false)),
            changed_public_members: Vec::new(),
            template_dependencies_ready: false,
            template_expression_cache: Mutex::new(TemplateExpressionCache::default()),
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
            tracking_storage_reads: Arc::new(AtomicBool::new(false)),
            pending_side_channels: Arc::new(AtomicBool::new(false)),
            pending_redraw: Arc::new(AtomicBool::new(false)),
            live_binding_external_accessed: Arc::new(AtomicBool::new(false)),
            last_element_metrics_fingerprint: None,
            shared_element_metrics: Arc::new(Mutex::new(ElementMetricsStore::default())),
            cached_self_table: None,
            service_context_state: Arc::new(Mutex::new(ServiceContextState::default())),
            service_call_completions: Arc::new(Mutex::new(HashMap::new())),
            locale_cell: Arc::new(Mutex::new(LocaleCell::default())),
            localized_misses: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn set_interface_catalog(&mut self, catalog: impl Into<Arc<InterfaceCatalog>>) {
        self.interface_catalog = catalog.into();
    }

    pub fn set_optional_interfaces(&mut self, interfaces: HashSet<String>) {
        self.optional_interfaces = Arc::new(interfaces);
    }

    /// Replace the compatibility catalog behind `mesh.i18n.t()`. Existing
    /// Luau handles share the cell, so a locale switch takes effect immediately.
    pub fn set_i18n_translations(&mut self, translations: HashMap<String, String>) {
        let mut cell = self.locale_cell.lock().unwrap();
        cell.translations = translations
            .into_iter()
            .map(|(key, value)| (key, CatalogEntry::Text(value)))
            .collect();
        cell.locale = "en".into();
        cell.snapshot_revision = 0;
    }

    /// Replace the catalog behind `mesh.i18n.t()` with the owning module's
    /// scoped translator snapshot.
    pub fn set_i18n_translator(&mut self, translator: &ModuleTranslator<'_>) {
        *self.locale_cell.lock().unwrap() = LocaleCell {
            locale: translator.locale().to_string(),
            translations: translator.entries(),
            snapshot_revision: translator.snapshot_revision(),
        };
        // Template expressions containing t(...) can otherwise remain in the
        // pure-member cache when the script members themselves did not change.
        // A locale/catalog replacement changes their result even when all
        // referenced public values are identical.
        self.clear_template_expression_cache();
    }

    /// Shared sink used by template evaluation while the Rust state snapshot
    /// is being rendered. It keeps template and Luau misses on one path.
    pub fn localized_misses_handle(&self) -> Arc<Mutex<Vec<LocalizedTextResolution>>> {
        Arc::clone(&self.localized_misses)
    }

    pub fn drain_localized_misses(&mut self) -> Vec<LocalizedTextResolution> {
        self.localized_misses.lock().unwrap().drain(..).collect()
    }

    /// Clone the thread VM, create a per-component `_ENV`, install host APIs,
    /// and populate `builtin_globals`. Idempotent.
    pub(super) fn ensure_initialized(&mut self) -> Result<(), ScriptError> {
        if self.vm.is_some() {
            return Ok(());
        }
        let vm = ScriptVm(self.shared_vm.clone().unwrap_or_else(pool::thread_vm));
        let lua = vm.lua();

        // Per-component _ENV with __index = globals() fallthrough.
        let env = lua.create_table().map_err(lua_err)?;
        let meta = lua.create_table().map_err(lua_err)?;
        let reactive_scalar_globals = lua.create_table().map_err(lua_err)?;
        let reactive_scalar_reads = reactive_scalar_globals.clone();
        let fallback_globals = lua.globals();
        meta.set(
            "__index",
            lua.create_function(move |_, (_table, key): (Table, String)| {
                let value = reactive_scalar_reads.raw_get::<LuaValue>(key.as_str())?;
                if matches!(value, LuaValue::Nil) {
                    fallback_globals.get::<LuaValue>(key.as_str())
                } else {
                    Ok(value)
                }
            })
            .map_err(lua_err)?,
        )
        .map_err(lua_err)?;
        let assigned_global_keys = Arc::clone(&self.assigned_global_keys);
        let pending_assigned_global_keys = Arc::clone(&self.pending_assigned_global_keys);
        let reactive_scalar_writes = reactive_scalar_globals.clone();
        meta.set(
            "__newindex",
            lua.create_function(move |_, (table, key, value): (Table, String, LuaValue)| {
                if !key.starts_with("__") {
                    assigned_global_keys.lock().unwrap().insert(key.clone());
                    pending_assigned_global_keys.store(true, Ordering::Release);
                }
                if !matches!(
                    reactive_scalar_writes.raw_get::<LuaValue>(key.as_str())?,
                    LuaValue::Nil
                ) {
                    reactive_scalar_writes.raw_set(key, value)
                } else {
                    table.raw_set(key, value)
                }
            })
            .map_err(lua_err)?,
        )
        .map_err(lua_err)?;
        env.set_metatable(Some(meta)).map_err(lua_err)?;

        self.vm = Some(vm);
        self.env_table = Some(env.clone());
        self.reactive_scalar_globals = Some(reactive_scalar_globals);

        // Install the host API into the per-component env table.
        self.install_host_api(&env)?;

        // Snapshot env_table after host-api install. Stdlib keys live on
        // globals, so only host API and later script globals appear here.
        self.builtin_globals = env
            .pairs::<String, LuaValue>()
            .filter_map(|result| result.ok().map(|(key, _)| key))
            .collect();
        self.assigned_global_keys.lock().unwrap().clear();
        self.pending_assigned_global_keys
            .store(false, Ordering::Release);
        self.template_dependencies_ready = false;
        *self.template_expression_cache.lock().unwrap() = TemplateExpressionCache::default();

        Ok(())
    }

    /// Tear down this context's `_ENV` graph. ScriptContext methods must not be
    /// called afterward without a subsequent `ensure_initialized()`.
    pub fn uninit(&mut self) {
        self.cached_self_table = None;
        self.service_context_state.lock().unwrap().clear();
        self.service_call_completions.lock().unwrap().clear();
        *self.locale_cell.lock().unwrap() = LocaleCell::default();
        self.localized_misses.lock().unwrap().clear();
        if let Some(env) = self.env_table.take() {
            // One realm per thread, so sever the _ENV graph explicitly to make
            // host callbacks and script closures collectible. Errors are
            // ignored: uninit also runs from Drop, which cannot report them.
            let _ = env.clear();
            let _ = env.set_metatable(None);
        }
        if let Some(globals) = self.reactive_scalar_globals.take() {
            let _ = globals.clear();
        }
        self.builtin_globals.clear();
        self.user_global_keys.clear();
        self.user_global_key_set.clear();
        self.proxied_scalar_global_keys.clear();
        self.user_globals_discovered = false;
        self.last_element_metrics_fingerprint = None;
        self.assigned_global_keys.lock().unwrap().clear();
        self.pending_assigned_global_keys
            .store(false, Ordering::Release);
        self.template_dependencies_ready = false;
        *self.template_expression_cache.lock().unwrap() = TemplateExpressionCache::default();
        self.subscribed_interface_events.lock().unwrap().clear();
        self.vm = None;
    }

    /// Execute a script and seed reactive state from its top-level globals.
    pub fn load_script(&mut self, source: &str) -> Result<(), ScriptError> {
        self.load_script_with_interface_imports(source, &[])
    }

    /// Check the active contract policy for a service-state read. This is the
    /// same resolved binding used by `require` and keeps shell-side cached
    /// payload delivery from falling back to interface-name conventions.
    pub fn can_read_service_interface(&self, interface: &str) -> bool {
        let canonical = crate::host_api::InterfaceProxy::canonical_name(interface);
        let resolution = self.interface_catalog.resolve(&canonical, None);
        resolution.contract.as_ref().map_or_else(
            || crate::host_api::InterfaceProxy::can_read(&self.capabilities, &canonical),
            |contract| {
                crate::host_api::InterfaceProxy::can_read_contract(&self.capabilities, contract)
            },
        )
    }

    /// Check the resolved contract policy for an interface-event subscription.
    /// Event delivery intentionally has its own capability decision: an
    /// event-only consumer may receive the event payload, but it must not make
    /// the service state snapshot available to that context.
    pub fn can_subscribe_service_event(&self, interface: &str, event: &str) -> bool {
        let canonical = crate::host_api::InterfaceProxy::canonical_name(interface);
        let resolution = self.interface_catalog.resolve(&canonical, None);
        resolution.contract.as_ref().map_or(true, |contract| {
            crate::host_api::InterfaceProxy::can_subscribe_contract_event(
                &self.capabilities,
                contract,
                event,
            )
        })
    }

    /// Load a script source after installing explicit interface imports as Lua globals.
    pub fn load_script_with_interface_imports(
        &mut self,
        source: &str,
        imports: &[ScriptInterfaceImport],
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        self.restore_proxied_scalars_to_env().map_err(lua_err)?;
        self.interface_bindings.clear();
        self.user_global_keys.clear();
        self.user_global_key_set.clear();
        self.user_globals_discovered = false;
        self.assigned_global_keys.lock().unwrap().clear();
        self.pending_assigned_global_keys
            .store(false, Ordering::Release);
        self.template_dependencies_ready = false;
        *self.template_expression_cache.lock().unwrap() = TemplateExpressionCache::default();
        {
            let mut shared_interface_bindings = self.shared_interface_bindings.lock().unwrap();
            shared_interface_bindings.bindings.clear();
            shared_interface_bindings.generation =
                shared_interface_bindings.generation.wrapping_add(1);
            self.interface_bindings_generation = shared_interface_bindings.generation;
        }
        let published_event_count = self.published_events.len();
        let published_event_bytes = self
            .published_events
            .iter()
            .map(PublishedEvent::queued_output_bytes)
            .sum();
        self.published_events.clear();
        crate::operation::release_side_effect(
            &self.realm_policy.budget(),
            published_event_count,
            published_event_bytes,
        );
        let element_action_count = self.element_actions.len();
        let element_action_bytes = self
            .element_actions
            .iter()
            .map(ElementAction::queued_output_bytes)
            .sum();
        self.element_actions.clear();
        crate::operation::release_side_effect(
            &self.realm_policy.budget(),
            element_action_count,
            element_action_bytes,
        );
        let (published_event_count, published_event_bytes) = {
            let mut published_events = self.shared_published_events.lock().unwrap();
            let count = published_events.len();
            let bytes = published_events
                .iter()
                .map(PublishedEvent::queued_output_bytes)
                .sum();
            published_events.clear();
            (count, bytes)
        };
        crate::operation::release_side_effect(
            &self.realm_policy.budget(),
            published_event_count,
            published_event_bytes,
        );
        self.service_call_completions.lock().unwrap().clear();
        self.shared_diagnostics.lock().unwrap().clear();
        self.localized_misses.lock().unwrap().clear();
        let (element_action_count, element_action_bytes) = {
            let mut element_actions = self.shared_element_actions.lock().unwrap();
            let count = element_actions.len();
            let bytes = element_actions
                .iter()
                .map(ElementAction::queued_output_bytes)
                .sum();
            element_actions.clear();
            (count, bytes)
        };
        crate::operation::release_side_effect(
            &self.realm_policy.budget(),
            element_action_count,
            element_action_bytes,
        );
        self.changed_storage_keys.lock().unwrap().clear();
        self.pending_side_channels.store(false, Ordering::Release);
        self.clear_tracked_service_fields();
        self.clear_subscribed_interface_events();
        self.clear_tracked_storage_keys();
        self.install_interface_imports(imports)?;
        let _budget = self.realm_policy.begin_callback();
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

    /// Compile and execute Luau source.
    pub fn compile_and_execute(
        &mut self,
        source: &str,
        imports: &[ScriptInterfaceImport],
    ) -> Result<(), ScriptError> {
        self.load_script_with_interface_imports(source, imports)
    }

    pub fn compile_and_execute_component(
        &mut self,
        source: &str,
        imports: &[ScriptInterfaceImport],
        template_expressions: &[String],
    ) -> Result<(), ScriptError> {
        let compiled = template_expressions
            .iter()
            .map(|expression| {
                mesh_core_expression::compile_expression(expression)
                    .map_err(|error| ScriptError::LuaError(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.compile_and_execute_component_with_compiled(source, imports, &compiled)
    }

    pub fn compile_and_execute_component_with_compiled(
        &mut self,
        source: &str,
        imports: &[ScriptInterfaceImport],
        template_expressions: &[mesh_core_expression::SharedCompiledExpression],
    ) -> Result<(), ScriptError> {
        let source =
            component_source_with_compiled_template_expressions(source, template_expressions);
        self.load_script_with_interface_imports(&source, imports)
    }
}
