use crate::host_api::{HostApiManifest, InterfaceProxy};
/// Script execution context — one per plugin/component instance.
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_elements::VariableStore;
use mesh_core_locale::LocaleEngine;
use mesh_core_service::{InterfaceCatalog, InterfaceContract, InterfaceResolution};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct PublishedEvent {
    pub channel: String,
    pub payload: Value,
    pub source_plugin_id: String,
    pub source_capabilities: CapabilitySet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptDiagnostic {
    pub plugin_id: String,
    pub interface: String,
    pub requested_version: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInterfaceImport {
    pub alias: String,
    pub interface: String,
    pub version: Option<String>,
}

/// Errors from the scripting runtime.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ScriptError {
    #[error("Luau error: {0}")]
    LuaError(String),

    #[error("script init failed: {0}")]
    InitFailed(String),

    #[error("handler not found: {0}")]
    HandlerNotFound(String),

    #[error("capability denied: {0}")]
    CapabilityDenied(String),

    #[error("script execution timed out")]
    Timeout,

    #[error("interface unavailable: {0}")]
    InterfaceUnavailable(String),

    #[error("unsupported interface operation: {interface}.{method}")]
    UnsupportedOperation { interface: String, method: String },
}

/// Reactive state exposed to and mutated by Luau scripts.
///
/// When a script sets a variable, the state is marked dirty.
/// The UI layer checks this flag to know when to rebuild the widget tree.
pub struct ScriptState {
    variables: HashMap<String, Value>,
    dirty: bool,
    // Optional proxies that forward get/set to external sources (used by the
    // host to expose imported component variables as if they lived in the
    // same namespace). The getter is invoked on reads; the setter, if
    // provided, is invoked on writes from scripts.
    proxies: HashMap<String, Proxy>,
}

impl ScriptState {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            dirty: false,
            proxies: HashMap::new(),
        }
    }

    /// Set a variable and mark state as dirty.
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        let name = name.into();
        // If a proxy is registered for this name and exposes a setter,
        // forward the write to the proxy instead of storing locally. If the
        // proxy is read-only, fall back to storing locally.
        if let Some(proxy) = self.proxies.get(&name) {
            if let Some(setter) = &proxy.setter {
                (setter)(value);
                return;
            }
        }

        if self
            .variables
            .get(&name)
            .is_some_and(|previous| reactive_values_equal(previous, &value))
        {
            return;
        }
        self.variables.insert(name, value);
        self.dirty = true;
    }

    /// Set a host-maintained variable without requesting a component rebuild.
    ///
    /// Used for render-derived values, such as element layout metrics, that
    /// should be visible to scripts but should not themselves cause a repaint.
    pub fn set_host_value(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    /// Check if any variable changed since last tree build.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Reset the dirty flag after tree rebuild.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}

fn reactive_values_equal(previous: &Value, next: &Value) -> bool {
    previous == next
}

impl Default for ScriptState {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableStore for ScriptState {
    fn get(&self, name: &str) -> Option<Value> {
        // If a proxy exists, call its getter, otherwise read from local
        // variables.
        if let Some(proxy) = self.proxies.get(name) {
            return Some((proxy.getter)());
        }
        self.variables.get(name).cloned()
    }

    fn keys(&self) -> Vec<String> {
        // Merge local variable keys with proxy keys. Proxies may shadow
        // local variables.
        let mut keys: Vec<String> = self.variables.keys().cloned().collect();
        for k in self.proxies.keys() {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        keys
    }
}

// A lightweight proxy that forwards get/set operations to host-provided
// closures.
struct Proxy {
    getter: Box<dyn Fn() -> Value + Send + 'static>,
    setter: Option<Box<dyn Fn(Value) + Send + 'static>>,
}

impl Clone for ScriptState {
    fn clone(&self) -> Self {
        Self {
            variables: self.variables.clone(),
            dirty: self.dirty,
            proxies: HashMap::new(), // proxies are host-registered and not cloned
        }
    }
}

impl fmt::Debug for ScriptState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptState")
            .field("variables", &self.variables)
            .field("dirty", &self.dirty)
            .field("proxies_count", &self.proxies.len())
            .finish()
    }
}

impl ScriptState {
    /// Register or replace a proxy for a variable name.
    pub fn register_proxy(
        &mut self,
        name: impl Into<String>,
        getter: Box<dyn Fn() -> Value + Send + 'static>,
        setter: Option<Box<dyn Fn(Value) + Send + 'static>>,
    ) {
        let name = name.into();
        self.proxies.insert(name, Proxy { getter, setter });
    }

    /// Remove a previously-registered proxy.
    pub fn unregister_proxy(&mut self, name: &str) {
        self.proxies.remove(name);
    }

    /// Check if a proxy exists for the given name.
    pub fn has_proxy(&self, name: &str) -> bool {
        self.proxies.contains_key(name)
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
    pub plugin_id: String,
    pub capabilities: CapabilitySet,
    pub state: ScriptState,
    lua: Lua,
    interface_catalog: InterfaceCatalog,
    interface_bindings: HashMap<String, InterfaceResolution>,
    shared_interface_bindings: Arc<Mutex<HashMap<String, InterfaceResolution>>>,
    /// Global names present before user script execution (stdlib + host API).
    /// Sync skips these so only user-defined globals become reactive state.
    builtin_globals: HashSet<String>,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    published_events: Vec<PublishedEvent>,
    shared_published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    diagnostics: Vec<ScriptDiagnostic>,
    shared_diagnostics: Arc<Mutex<Vec<ScriptDiagnostic>>>,
}

impl ScriptContext {
    /// Create a new script context for a plugin.
    pub fn new(
        plugin_id: impl Into<String>,
        capabilities: CapabilitySet,
    ) -> Result<Self, ScriptError> {
        Ok(Self {
            plugin_id: plugin_id.into(),
            capabilities,
            state: ScriptState::new(),
            lua: Lua::new(),
            interface_catalog: InterfaceCatalog::default(),
            interface_bindings: HashMap::new(),
            shared_interface_bindings: Arc::new(Mutex::new(HashMap::new())),
            builtin_globals: HashSet::new(),
            tracked_service_fields: Arc::new(Mutex::new(HashMap::new())),
            published_events: Vec::new(),
            shared_published_events: Arc::new(Mutex::new(Vec::new())),
            diagnostics: Vec::new(),
            shared_diagnostics: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn set_interface_catalog(&mut self, catalog: InterfaceCatalog) {
        self.interface_catalog = catalog;
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
        self.interface_bindings.clear();
        self.shared_interface_bindings.lock().unwrap().clear();
        self.shared_published_events.lock().unwrap().clear();
        self.shared_diagnostics.lock().unwrap().clear();
        self.clear_tracked_service_fields();
        self.install_host_api()?;
        self.install_interface_imports(imports)?;
        // Snapshot all pre-script globals so auto-sync excludes them.
        self.builtin_globals = self
            .lua
            .globals()
            .pairs::<String, LuaValue>()
            .filter_map(|r| r.ok().map(|(k, _)| k))
            .collect();
        self.lua
            .load(source)
            .set_name(&self.plugin_id)
            .exec()
            .map_err(|err| map_lua_error(&self.interface_catalog, err))?;
        self.sync_state_from_lua();
        tracing::info!("loaded script for plugin {}", self.plugin_id);
        Ok(())
    }

    /// Copy the latest service payload into the Lua runtime for proxy reads.
    ///
    /// Called by core after each service update so interface proxies can read
    /// state fields directly without explicit callback or binding APIs.
    pub fn apply_service_payload(&mut self, service: &str, payload: &Value) {
        let svc_key = format!("__mesh_svc_{service}");
        if let Ok(lua_value) = self.lua.to_value(payload) {
            let _ = self.lua.globals().set(svc_key, lua_value);
        }
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

    pub fn clear_tracked_service_fields(&self) {
        self.tracked_service_fields.lock().unwrap().clear();
    }

    /// Call the script's `init()` function if it exists.
    pub fn call_init(&mut self) -> Result<(), ScriptError> {
        if let Ok(init) = self.lua.globals().get::<Function>("init") {
            tracing::debug!("calling init() for {}", self.plugin_id);
            init.call::<()>(())
                .map_err(|err| map_lua_error(&self.interface_catalog, err))?;
            self.sync_state_from_lua();
            self.sync_side_channels();
        }
        Ok(())
    }

    /// Call a named event handler.
    pub fn call_handler(&mut self, name: &str, _args: &[Value]) -> Result<(), ScriptError> {
        let globals = self.lua.globals();
        let handler = globals
            .get::<Function>(name)
            .map_err(|_| ScriptError::HandlerNotFound(name.to_string()))?;
        tracing::debug!("calling handler {name}() for {}", self.plugin_id);
        match _args.len() {
            0 => handler
                .call::<()>(())
                .map_err(|err| map_lua_error(&self.interface_catalog, err))?,
            1 => {
                let arg = self.lua.to_value(&_args[0]).map_err(lua_err)?;
                handler
                    .call::<()>(arg)
                    .map_err(|err| map_lua_error(&self.interface_catalog, err))?;
            }
            _ => {
                let mut args = mlua::MultiValue::new();
                for arg in _args {
                    args.push_back(self.lua.to_value(arg).map_err(lua_err)?);
                }
                handler
                    .call::<()>(args)
                    .map_err(|err| map_lua_error(&self.interface_catalog, err))?;
            }
        }
        self.sync_state_from_lua();
        self.sync_side_channels();
        Ok(())
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

    /// Check if a handler exists.
    pub fn has_handler(&self, name: &str) -> bool {
        self.lua.globals().get::<Function>(name).is_ok()
    }

    fn install_host_api(&mut self) -> Result<(), ScriptError> {
        let globals = self.lua.globals();
        let mesh = self.lua.create_table().map_err(lua_err)?;
        let mesh_core_service = self.lua.create_table().map_err(lua_err)?;
        let mesh_core_events = self.lua.create_table().map_err(lua_err)?;
        let mesh_ui_api = self.lua.create_table().map_err(lua_err)?;
        let mesh_log = self.lua.create_table().map_err(lua_err)?;
        let interface_catalog = self.interface_catalog.clone();
        let manifest = HostApiManifest::from_capabilities(&self.capabilities);
        let allowed_interfaces = manifest.interface_capabilities.clone();
        let has_theme_read = manifest.has_theme_read;
        let has_locale_read = manifest.has_locale_read;

        let published_events = Arc::clone(&self.shared_published_events);
        let tracked_service_fields = Arc::clone(&self.tracked_service_fields);
        let interface_catalog_for_service = interface_catalog.clone();
        let allowed_interfaces_for_service = allowed_interfaces.clone();
        let plugin_id_for_service = self.plugin_id.clone();
        let capabilities_for_service = self.capabilities.clone();
        let diagnostics_for_service = Arc::clone(&self.shared_diagnostics);
        mesh_core_service
            .set(
                "use",
                self.lua
                    .create_function(move |lua, service: String| {
                        let canonical = InterfaceProxy::canonical_name(&service);
                        let readable = canonical == "mesh.theme" && has_theme_read
                            || canonical == "mesh.locale" && has_locale_read
                            || allowed_interfaces_for_service.contains(&canonical)
                            || !canonical.starts_with("mesh.");
                        if canonical.starts_with("mesh.") && !readable {
                            return Err(record_lookup_diagnostic_lua(
                                &diagnostics_for_service,
                                &plugin_id_for_service,
                                &canonical,
                                None,
                                "capability denied",
                                ScriptError::CapabilityDenied(canonical.clone()),
                            ));
                        }

                        let resolution = interface_catalog_for_service.resolve(&canonical, None);
                        if resolution.provider.is_none() {
                            let reason =
                                lookup_failure_reason(&interface_catalog_for_service, &resolution);
                            return Err(record_lookup_diagnostic_lua(
                                &diagnostics_for_service,
                                &plugin_id_for_service,
                                &canonical,
                                None,
                                &reason,
                                ScriptError::InterfaceUnavailable(canonical.clone()),
                            ));
                        }
                        create_service_proxy(
                            lua,
                            service_name_from_interface(&canonical),
                            resolution.contract.clone(),
                            canonical,
                            plugin_id_for_service.clone(),
                            capabilities_for_service.clone(),
                            Arc::clone(&tracked_service_fields),
                            Arc::clone(&published_events),
                        )
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let published_events = Arc::clone(&self.shared_published_events);
        let plugin_id = self.plugin_id.clone();
        let capabilities = self.capabilities.clone();
        mesh_core_events
            .set(
                "publish",
                self.lua
                    .create_function(move |lua, (channel, payload): (String, Option<LuaValue>)| {
                        let payload = payload.unwrap_or(LuaValue::Nil);
                        let payload = lua.from_value::<Value>(payload)?;
                        tracing::info!("{} published event {}", plugin_id, channel);
                        published_events.lock().unwrap().push(PublishedEvent {
                            channel,
                            payload,
                            source_plugin_id: plugin_id.clone(),
                            source_capabilities: capabilities.clone(),
                        });
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        mesh_ui_api
            .set(
                "request_redraw",
                self.lua
                    .create_function(|lua, ()| {
                        lua.globals().set("__mesh_request_redraw", true)?;
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let plugin_id = self.plugin_id.clone();
        mesh_log
            .set(
                "info",
                self.lua
                    .create_function(move |_lua, message: String| {
                        tracing::info!("{}: {}", plugin_id, message);
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;
        let plugin_id = self.plugin_id.clone();
        mesh_log
            .set(
                "warn",
                self.lua
                    .create_function(move |_lua, message: String| {
                        tracing::warn!("{}: {}", plugin_id, message);
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;
        let plugin_id = self.plugin_id.clone();
        mesh_log
            .set(
                "error",
                self.lua
                    .create_function(move |_lua, message: String| {
                        tracing::error!("{}: {}", plugin_id, message);
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        mesh.set("service", mesh_core_service).map_err(lua_err)?;
        mesh.set("events", mesh_core_events).map_err(lua_err)?;
        mesh.set("ui", mesh_ui_api).map_err(lua_err)?;
        mesh.set("log", mesh_log).map_err(lua_err)?;
        globals.set("mesh", mesh).map_err(lua_err)?;
        globals
            .set("__mesh_request_redraw", false)
            .map_err(lua_err)?;

        let published_events = Arc::clone(&self.shared_published_events);
        let tracked_service_fields = Arc::clone(&self.tracked_service_fields);
        let plugin_id_for_require = self.plugin_id.clone();
        let capabilities_for_require = self.capabilities.clone();
        let diagnostics_for_require = Arc::clone(&self.shared_diagnostics);
        let require = self
            .lua
            .create_function(move |lua, module: String| {
                if module == "@mesh/i18n" {
                    let exports = lua.create_table()?;
                    exports.set(
                        "t",
                        lua.create_function(|_lua, key: LuaValue| match key {
                            LuaValue::String(s) => Ok(s.to_str()?.to_string()),
                            other => Ok(lua_value_to_string(other)),
                        })?,
                    )?;
                    return Ok(exports);
                }

                let mut module_name = module.as_str();
                let mut version = None;
                if let Some((left, right)) = module.rsplit_once('@') {
                    if left.starts_with("@mesh.")
                        || left.starts_with("@mesh/")
                        || left.starts_with("mesh.")
                    {
                        module_name = left;
                        version = Some(right.to_string());
                    }
                }

                let interface = if let Some(stripped) = module_name.strip_prefix("@mesh.") {
                    format!("mesh.{stripped}")
                } else if let Some(stripped) = module_name.strip_prefix("@mesh/") {
                    format!("mesh.{}", stripped.replace('/', "."))
                } else if module_name.starts_with("mesh.") {
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
                        &plugin_id_for_require,
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
                        &plugin_id_for_require,
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
                    plugin_id_for_require.clone(),
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
        let globals = self.lua.globals();
        for import in imports {
            let canonical = InterfaceProxy::canonical_name(&import.interface);
            let readable = canonical == "mesh.theme" && manifest.has_theme_read
                || canonical == "mesh.locale" && manifest.has_locale_read
                || manifest.interface_capabilities.contains(&canonical)
                || !canonical.starts_with("mesh.");
            if canonical.starts_with("mesh.") && !readable {
                record_lookup_diagnostic(
                    &self.shared_diagnostics,
                    &self.plugin_id,
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
                    &self.plugin_id,
                    &canonical,
                    import.version.as_deref(),
                    &reason,
                );
                return Err(ScriptError::InterfaceUnavailable(interface_error_message(
                    &canonical,
                    import.version.as_deref(),
                )));
            }

            self.shared_interface_bindings
                .lock()
                .unwrap()
                .insert(import.alias.clone(), resolution.clone());
            let proxy = create_interface_proxy(
                &self.lua,
                resolution,
                self.plugin_id.clone(),
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
    fn sync_state_from_lua(&mut self) {
        let user_globals: Vec<(String, LuaValue)> = self
            .lua
            .globals()
            .pairs::<String, LuaValue>()
            .filter_map(|r| r.ok())
            .filter(|(k, v)| {
                !k.starts_with("__")
                    && !self.builtin_globals.contains(k)
                    && !matches!(v, LuaValue::Function(_))
            })
            .collect();
        for (name, lua_value) in user_globals {
            if let Ok(value) = self.lua.from_value::<Value>(lua_value) {
                self.state.set(name, value);
            }
        }

        if self
            .lua
            .globals()
            .get::<bool>("__mesh_request_redraw")
            .unwrap_or(false)
        {
            self.state.dirty = true;
            let _ = self.lua.globals().set("__mesh_request_redraw", false);
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
        self.interface_bindings = self.shared_interface_bindings.lock().unwrap().clone();
    }
}

fn record_lookup_diagnostic_lua(
    diagnostics: &Arc<Mutex<Vec<ScriptDiagnostic>>>,
    plugin_id: &str,
    interface: &str,
    requested_version: Option<&str>,
    reason: &str,
    err: ScriptError,
) -> mlua::Error {
    record_lookup_diagnostic(diagnostics, plugin_id, interface, requested_version, reason);
    mlua::Error::external(err)
}

fn record_lookup_diagnostic(
    diagnostics: &Arc<Mutex<Vec<ScriptDiagnostic>>>,
    plugin_id: &str,
    interface: &str,
    requested_version: Option<&str>,
    reason: &str,
) {
    tracing::error!(
        plugin_id,
        interface,
        requested_version = requested_version.unwrap_or(""),
        reason,
        "service interface lookup failed"
    );
    diagnostics.lock().unwrap().push(ScriptDiagnostic {
        plugin_id: plugin_id.to_string(),
        interface: interface.to_string(),
        requested_version: requested_version.map(ToOwned::to_owned),
        reason: reason.to_string(),
    });
}

fn lookup_failure_reason(catalog: &InterfaceCatalog, resolution: &InterfaceResolution) -> String {
    let has_contracts = catalog
        .contracts
        .get(&resolution.requested)
        .is_some_and(|contracts| !contracts.is_empty());
    let has_providers = catalog
        .providers
        .get(&resolution.requested)
        .is_some_and(|providers| !providers.is_empty());

    match (
        resolution.contract.is_some(),
        resolution.provider.is_some(),
        resolution.requested_version.as_deref(),
        has_contracts,
        has_providers,
    ) {
        (false, false, Some(version), true, _) | (false, false, Some(version), _, true) => {
            format!(
                "requested version {version} did not match available interface contracts or providers"
            )
        }
        (false, true, _, _, _) => "missing contract".to_string(),
        (true, false, _, _, _) => "missing provider".to_string(),
        (false, false, _, false, false) => "missing contract and provider".to_string(),
        (false, false, _, false, true) => "missing contract".to_string(),
        (false, false, _, true, false) => "missing provider".to_string(),
        _ => "interface lookup failed".to_string(),
    }
}

fn interface_error_message(interface: &str, requested_version: Option<&str>) -> String {
    format!(
        "{}{}",
        interface,
        requested_version
            .map(|value| format!(" ({value})"))
            .unwrap_or_default()
    )
}

fn create_interface_proxy(
    lua: &Lua,
    resolution: InterfaceResolution,
    source_plugin_id: String,
    source_capabilities: CapabilitySet,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
) -> mlua::Result<Table> {
    create_service_proxy(
        lua,
        service_name_from_interface(&resolution.requested),
        resolution.contract,
        resolution.requested,
        source_plugin_id,
        source_capabilities,
        tracked_service_fields,
        published_events,
    )
}

fn create_service_proxy(
    lua: &Lua,
    service_name: String,
    contract: Option<InterfaceContract>,
    interface_name: String,
    source_plugin_id: String,
    source_capabilities: CapabilitySet,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    let meta = lua.create_table()?;

    let methods = contract
        .as_ref()
        .map(|c| c.methods.clone())
        .unwrap_or_default();
    let interface_name = contract
        .as_ref()
        .map(|c| c.interface.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or(interface_name);
    let state_proxy = create_service_state_proxy(
        lua,
        service_name.clone(),
        Arc::clone(&tracked_service_fields),
    )?;
    proxy.set("state", state_proxy)?;

    meta.set(
        "__index",
        lua.create_function(move |lua, (_table, key): (Table, String)| {
            if key == "state" {
                return _table.get::<LuaValue>("state");
            }
            // Case A: known contract method — dispatch as a service command.
            if let Some(method) = methods.iter().find(|m| m.name == key) {
                let required_capability = service_control_capability(&service_name);
                let method = method.clone();
                let iface = interface_name.clone();
                let events = Arc::clone(&published_events);
                let source_plugin_id = source_plugin_id.clone();
                let source_capabilities = source_capabilities.clone();
                return Ok(LuaValue::Function(lua.create_function(
                    move |lua, args: mlua::Variadic<LuaValue>| {
                        if !source_capabilities.is_granted(&required_capability) {
                            return command_result_table(
                                lua,
                                false,
                                false,
                                Some("capability denied"),
                            )
                            .map(LuaValue::Table);
                        }
                        let offset = consume_self_arg(&args);
                        let payload = method
                            .args
                            .iter()
                            .enumerate()
                            .map(|(index, arg)| {
                                let lua_value =
                                    args.get(index + offset).cloned().unwrap_or(LuaValue::Nil);
                                lua.from_value::<Value>(lua_value)
                                    .map(|value| (arg.name.clone(), value))
                            })
                            .collect::<mlua::Result<serde_json::Map<String, Value>>>()?;
                        events.lock().unwrap().push(PublishedEvent {
                            channel: format!("{}.{}", iface, method.name),
                            payload: Value::Object(payload),
                            source_plugin_id: source_plugin_id.clone(),
                            source_capabilities: source_capabilities.clone(),
                        });
                        command_result_table(lua, true, true, None).map(LuaValue::Table)
                    },
                )?));
            }

            // Case B: state field read from the live service payload table.
            tracked_service_fields
                .lock()
                .unwrap()
                .entry(service_name.clone())
                .or_default()
                .insert(key.clone());
            service_payload_field(lua, &service_name, &key)
        })?,
    )?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

fn create_service_state_proxy(
    lua: &Lua,
    service_name: String,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
) -> mlua::Result<Table> {
    let state = lua.create_table()?;
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(move |lua, (_table, key): (Table, String)| {
            tracked_service_fields
                .lock()
                .unwrap()
                .entry(service_name.clone())
                .or_default()
                .insert(key.clone());
            service_payload_field(lua, &service_name, &key)
        })?,
    )?;
    state.set_metatable(Some(meta))?;
    Ok(state)
}

fn service_payload_field(lua: &Lua, service_name: &str, key: &str) -> mlua::Result<LuaValue> {
    let svc_key = format!("__mesh_svc_{service_name}");
    let tbl = match lua.globals().get::<LuaValue>(svc_key.as_str()) {
        Ok(LuaValue::Table(t)) => Some(t),
        _ => None,
    };
    Ok(tbl
        .and_then(|t| t.get::<LuaValue>(key).ok())
        .unwrap_or(LuaValue::Nil))
}

fn command_result_table(
    lua: &Lua,
    ok: bool,
    queued: bool,
    error: Option<&str>,
) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    result.set("ok", ok)?;
    if ok {
        result.set("queued", queued)?;
    }
    if let Some(error) = error {
        result.set("error", error)?;
    }
    Ok(result)
}

fn consume_self_arg(args: &mlua::Variadic<LuaValue>) -> usize {
    match args.get(0) {
        Some(LuaValue::Table(_)) => 1,
        _ => 0,
    }
}

fn service_name_from_interface(interface: &str) -> String {
    interface
        .strip_prefix("mesh.")
        .unwrap_or(interface)
        .to_string()
}

fn service_control_capability(service_name: &str) -> Capability {
    Capability::new(format!("service.{service_name}.control"))
}

fn map_lua_error(_catalog: &InterfaceCatalog, err: mlua::Error) -> ScriptError {
    extract_script_error(&err).unwrap_or_else(|| ScriptError::LuaError(err.to_string()))
}

fn extract_script_error(err: &mlua::Error) -> Option<ScriptError> {
    match err {
        mlua::Error::CallbackError { cause, .. } => extract_script_error(cause),
        mlua::Error::ExternalError(err) => err.downcast_ref::<ScriptError>().cloned(),
        _ => None,
    }
}

fn lua_err(err: mlua::Error) -> ScriptError {
    ScriptError::LuaError(err.to_string())
}

fn lua_value_to_string(value: LuaValue) -> String {
    match value {
        LuaValue::Nil => "nil".to_string(),
        LuaValue::Boolean(v) => v.to_string(),
        LuaValue::Integer(v) => v.to_string(),
        LuaValue::Number(v) => v.to_string(),
        LuaValue::String(v) => v.to_string_lossy(),
        other => format!("{other:?}"),
    }
}

/// A `VariableStore` that combines script state with locale engine access.
///
/// Pass this to `build_preview_tree_with_state` so that template expressions
/// like `{t("greeting")}` resolve through the active locale engine.
pub struct LocaleBoundState<'a> {
    state: &'a ScriptState,
    locale: &'a LocaleEngine,
}

impl<'a> LocaleBoundState<'a> {
    pub fn new(state: &'a ScriptState, locale: &'a LocaleEngine) -> Self {
        Self { state, locale }
    }
}

impl<'a> VariableStore for LocaleBoundState<'a> {
    fn get(&self, name: &str) -> Option<Value> {
        self.state.get(name)
    }

    fn keys(&self) -> Vec<String> {
        self.state.keys()
    }

    fn translate(&self, key: &str) -> Option<String> {
        self.locale.translate(key).map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_capability::{Capability, CapabilitySet};
    use mesh_core_service::{
        ContractCapabilities, InterfaceArgument, InterfaceCatalog, InterfaceContract,
        InterfaceMethod, InterfaceProvider, parse_contract_version,
    };
    use std::path::PathBuf;

    fn audio_catalog() -> InterfaceCatalog {
        let mut catalog = InterfaceCatalog::default();
        catalog.register_contract(InterfaceContract {
            interface: "mesh.audio".into(),
            version: parse_contract_version("1.0").unwrap(),
            file_path: PathBuf::from("<test>"),
            // State fields are documented core reads — not callable methods.
            state_fields: Vec::new(),
            // Only mutating command methods belong here.
            methods: vec![
                InterfaceMethod {
                    name: "set_volume".into(),
                    args: vec![
                        InterfaceArgument {
                            name: "device_id".into(),
                            arg_type: "string".into(),
                        },
                        InterfaceArgument {
                            name: "volume".into(),
                            arg_type: "float".into(),
                        },
                    ],
                    returns: Some("Result".into()),
                },
                InterfaceMethod {
                    name: "volume_up".into(),
                    args: Vec::new(),
                    returns: None,
                },
                InterfaceMethod {
                    name: "volume_down".into(),
                    args: Vec::new(),
                    returns: None,
                },
                InterfaceMethod {
                    name: "toggle_mute".into(),
                    args: Vec::new(),
                    returns: None,
                },
                InterfaceMethod {
                    name: "set_muted".into(),
                    args: vec![
                        InterfaceArgument {
                            name: "device_id".into(),
                            arg_type: "string".into(),
                        },
                        InterfaceArgument {
                            name: "muted".into(),
                            arg_type: "boolean".into(),
                        },
                    ],
                    returns: Some("Result".into()),
                },
            ],
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        catalog.register_provider(InterfaceProvider {
            interface: "mesh.audio".into(),
            version: Some("1.0".into()),
            base_plugin: Some("@mesh/audio-interface".into()),
            provider_plugin: "@mesh/pipewire-audio".into(),
            backend_name: "PipeWire".into(),
            priority: 100,
        });
        catalog
    }

    fn theme_provider_only_catalog() -> InterfaceCatalog {
        let mut catalog = InterfaceCatalog::default();
        catalog.register_provider(InterfaceProvider {
            interface: "mesh.theme".into(),
            version: Some("1.0".into()),
            base_plugin: None,
            provider_plugin: "@mesh/shell-theme".into(),
            backend_name: "Shell Theme".into(),
            priority: 100,
        });
        catalog
    }

    #[test]
    fn require_import_installs_proxy() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = require("@mesh/audio@>=1.0")
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();
    }

    #[test]
    fn explicit_interface_import_installs_proxy_global() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script_with_interface_imports(
            r#"
audio_percent = 0

function init()
    audio_percent = audio.percent or 0
end
"#,
            &[ScriptInterfaceImport {
                alias: "audio".into(),
                interface: "mesh.audio".into(),
                version: Some(">=1.0".into()),
            }],
        )
        .unwrap();
        ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 72 }));
        ctx.call_init().unwrap();

        assert_eq!(
            ctx.interface_bindings
                .get("audio")
                .map(|resolution| resolution.requested.as_str()),
            Some("mesh.audio")
        );
        assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(72)));
        assert!(ctx.tracked_fields_for_service("audio").contains("percent"));
    }

    #[test]
    fn require_imports_interface_proxy() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));

        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = require("@mesh/audio")
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();
    }

    #[test]
    fn rejects_missing_interface_contract() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.load_script(
            r#"
function init()
    local audio = require("@mesh/audio@>=1.0")
end
"#,
        )
        .unwrap();

        let err = ctx.call_init().unwrap_err();
        assert!(matches!(err, ScriptError::InterfaceUnavailable(_)));
    }

    #[test]
    fn require_missing_interface_emits_visible_diagnostic() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/diagnostic-test", caps).unwrap();
        ctx.load_script(
            r#"
function init()
    require("@mesh/audio@>=1.0")
end
"#,
        )
        .unwrap();

        let err = ctx.call_init().unwrap_err();
        assert!(matches!(err, ScriptError::InterfaceUnavailable(_)));
        let diagnostics = ctx.drain_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].plugin_id, "@mesh/diagnostic-test");
        assert_eq!(diagnostics[0].interface, "mesh.audio");
        assert_eq!(diagnostics[0].requested_version.as_deref(), Some(">=1.0"));
        assert!(diagnostics[0].reason.contains("missing contract"));
    }

    #[test]
    fn pcall_require_still_emits_interface_diagnostic() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/pcall-test", caps).unwrap();
        ctx.load_script(
            r#"
ok = true

function init()
    ok = pcall(require, "@mesh/audio")
end
"#,
        )
        .unwrap();

        ctx.call_init().unwrap();
        assert_eq!(ctx.state.get("ok"), Some(Value::Bool(false)));
        let diagnostics = ctx.drain_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].plugin_id, "@mesh/pcall-test");
        assert_eq!(diagnostics[0].interface, "mesh.audio");
    }

    #[test]
    fn unknown_method_reads_state_field_as_nil() {
        // Unknown keys fall through to the live service state table (__mesh_svc_audio).
        // When no service has emitted yet the table doesn't exist, so the result is nil
        // and the call succeeds without error.
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = require("@mesh/audio@>=1.0")
    local val = audio.mute_all  -- unknown key: should return nil, not error
    assert(val == nil)
end
"#,
        )
        .unwrap();

        // Should succeed — no error for unknown keys.
        ctx.call_init().unwrap();
    }

    #[test]
    fn globals_are_reactive_state() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/local", caps).unwrap();
        ctx.load_script(
            r#"
volumeHidden = true
count = 0

function toggle()
    volumeHidden = not volumeHidden
end
"#,
        )
        .unwrap();

        assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(true)));
        assert_eq!(ctx.state.get("count"), Some(Value::Number(0.into())));

        ctx.call_handler("toggle", &[]).unwrap();
        assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(false)));

        ctx.call_handler("toggle", &[]).unwrap();
        assert_eq!(ctx.state.get("volumeHidden"), Some(Value::Bool(true)));
    }

    #[test]
    fn reactive_global_marks_dirty_only_when_value_changes() {
        let mut state = ScriptState::new();
        state.set("count", serde_json::json!(1));
        assert!(state.is_dirty());

        state.clear_dirty();
        state.set("count", serde_json::json!(1));
        assert!(!state.is_dirty());

        state.set("count", serde_json::json!(2));
        assert!(state.is_dirty());
    }

    #[test]
    fn reactive_table_compares_nested_values() {
        let mut state = ScriptState::new();
        state.set(
            "settings",
            serde_json::json!({
                "enabled": true,
                "label": "primary",
                "nested": { "value": 1 }
            }),
        );
        assert!(state.is_dirty());

        state.clear_dirty();
        state.set(
            "settings",
            serde_json::json!({
                "enabled": true,
                "label": "primary",
                "nested": { "value": 1 }
            }),
        );
        assert!(!state.is_dirty());

        state.set(
            "settings",
            serde_json::json!({
                "enabled": false,
                "label": "primary",
                "nested": { "value": 1 }
            }),
        );
        assert!(state.is_dirty());

        state.clear_dirty();
        state.set(
            "settings",
            serde_json::json!({
                "enabled": false,
                "label": "primary",
                "nested": { "value": 2 }
            }),
        );
        assert!(state.is_dirty());

        state.clear_dirty();
        state.set(
            "settings",
            serde_json::json!({
                "enabled": false,
                "label": "primary",
                "nested": { "value": 2 },
                "levels": [1, 2, 3]
            }),
        );
        assert!(state.is_dirty());

        state.clear_dirty();
        state.set(
            "settings",
            serde_json::json!({
                "enabled": false,
                "label": "primary",
                "nested": { "value": 2 },
                "levels": [1, 3, 3]
            }),
        );
        assert!(state.is_dirty());

        state.clear_dirty();
        state.set(
            "wifi_networks",
            serde_json::json!([
                { "connection_id": "home", "ssid": "Home", "strength": 70, "active": false },
                { "connection_id": "office", "ssid": "Office", "strength": 60, "active": true }
            ]),
        );
        assert!(state.is_dirty());

        state.clear_dirty();
        state.set(
            "wifi_networks",
            serde_json::json!([
                { "connection_id": "home", "ssid": "Home", "strength": 71, "active": true },
                { "connection_id": "office", "ssid": "Office", "strength": 60, "active": false }
            ]),
        );
        assert!(state.is_dirty());
    }

    #[test]
    fn host_value_update_does_not_mark_dirty() {
        let mut state = ScriptState::new();
        state.set_host_value("elements", serde_json::json!({ "root": true }));
        assert!(!state.is_dirty());
    }

    #[test]
    fn mesh_request_redraw_marks_dirty_without_global_change() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/redraw", caps).unwrap();
        ctx.load_script(
            r#"
function request()
    __mesh_request_redraw = true
end
"#,
        )
        .unwrap();

        ctx.state.clear_dirty();
        ctx.call_handler("request", &[]).unwrap();
        assert!(ctx.state.is_dirty());

        ctx.state.clear_dirty();
        ctx.sync_state_from_lua();
        assert!(!ctx.state.is_dirty());
    }

    #[test]
    fn if_then_end_executes_conditionally() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/if", caps).unwrap();
        ctx.load_script(
            r#"
a = true
b = false

function run()
    a = not a
    if not a then
        b = true
    end
end
"#,
        )
        .unwrap();

        ctx.call_handler("run", &[]).unwrap();
        assert_eq!(ctx.state.get("a"), Some(Value::Bool(false)));
        assert_eq!(ctx.state.get("b"), Some(Value::Bool(true)));

        ctx.call_handler("run", &[]).unwrap();
        assert_eq!(ctx.state.get("a"), Some(Value::Bool(true)));
        // b stays true — the if branch doesn't reset it
        assert_eq!(ctx.state.get("b"), Some(Value::Bool(true)));
    }

    #[test]
    fn interface_proxy_tracks_top_level_field_reads() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
icon_name = "audio-volume-muted"

function sync_audio_state()
    local audio = require("@mesh/audio@>=1.0")
    local percent = audio.percent or 0
    if audio.muted then
        icon_name = "audio-volume-muted"
    else
        if percent < 34 then
            icon_name = "audio-volume-low"
        else
            if percent < 67 then
                icon_name = "audio-volume-medium"
            else
                icon_name = "audio-volume-high"
            end
        end
    end
end
"#,
        )
        .unwrap();

        let payload = serde_json::json!({ "percent": 65, "muted": false });
        ctx.apply_service_payload("audio", &payload);
        ctx.call_handler("sync_audio_state", &[]).unwrap();
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-medium".into()))
        );

        let tracked = ctx.tracked_fields_for_service("audio");
        assert!(tracked.contains("percent"));
        assert!(tracked.contains("muted"));
    }

    #[test]
    fn interface_proxy_exposes_state_table() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
audio_state_type = ""

function init()
    local audio = require("@mesh/audio@>=1.0")
    audio_state_type = type(audio.state)
end
"#,
        )
        .unwrap();

        ctx.call_init().unwrap();

        assert_eq!(
            ctx.state.get("audio_state_type"),
            Some(serde_json::json!("table"))
        );
    }

    #[test]
    fn interface_proxy_state_reads_latest_payload() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
audio_percent = 0

function sync_audio_state()
    local audio = require("@mesh/audio@>=1.0")
    audio_percent = audio.state.percent or 0
end
"#,
        )
        .unwrap();

        ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 31 }));
        ctx.call_handler("sync_audio_state", &[]).unwrap();
        assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(31)));

        ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 88 }));
        ctx.call_handler("sync_audio_state", &[]).unwrap();
        assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(88)));
        assert!(ctx.tracked_fields_for_service("audio").contains("percent"));
    }

    #[test]
    fn interface_proxy_direct_field_read_remains_compatibility_alias() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
state_percent = 0
direct_percent = 0

function sync_audio_state()
    local audio = require("@mesh/audio@>=1.0")
    state_percent = audio.state.percent or 0
    direct_percent = audio.percent or 0
end
"#,
        )
        .unwrap();

        ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 57 }));
        ctx.call_handler("sync_audio_state", &[]).unwrap();

        assert_eq!(ctx.state.get("state_percent"), Some(serde_json::json!(57)));
        assert_eq!(ctx.state.get("direct_percent"), Some(serde_json::json!(57)));
    }

    #[test]
    fn interface_proxy_reads_state_fields_without_callbacks() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
icon_name = "audio-volume-muted"

function init()
    local audio = require("@mesh/audio@>=1.0")
    if audio.muted then
        icon_name = "audio-volume-muted"
    elseif audio.percent < 50 then
        icon_name = "audio-volume-low"
    else
        icon_name = "audio-volume-high"
    end
end
"#,
        )
        .unwrap();
        let payload = serde_json::json!({ "percent": 80, "muted": false });
        ctx.apply_service_payload("audio", &payload);
        ctx.call_init().unwrap();
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-high".into()))
        );
    }

    #[test]
    fn interface_proxy_reads_latest_emitted_fields_after_repeated_updates() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
audio_percent = 0
audio_muted = false
audio_source = ""

function sync_audio_state()
    local audio = require("@mesh/audio@>=1.0")
    audio_percent = audio.percent or 0
    audio_muted = audio.muted or false
    audio_source = audio.source_plugin or ""
end
"#,
        )
        .unwrap();

        ctx.apply_service_payload(
            "audio",
            &serde_json::json!({
                "percent": 25,
                "muted": false,
                "source_plugin": "@mesh/pulse"
            }),
        );
        ctx.call_handler("sync_audio_state", &[]).unwrap();
        assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(25)));
        assert_eq!(ctx.state.get("audio_muted"), Some(serde_json::json!(false)));
        assert_eq!(
            ctx.state.get("audio_source"),
            Some(serde_json::json!("@mesh/pulse"))
        );

        ctx.apply_service_payload(
            "audio",
            &serde_json::json!({
                "percent": 82,
                "muted": true,
                "source_plugin": "@mesh/pipewire"
            }),
        );
        ctx.call_handler("sync_audio_state", &[]).unwrap();
        assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(82)));
        assert_eq!(ctx.state.get("audio_muted"), Some(serde_json::json!(true)));
        assert_eq!(
            ctx.state.get("audio_source"),
            Some(serde_json::json!("@mesh/pipewire"))
        );
    }

    #[test]
    fn service_use_reads_state_fields_without_callbacks() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
audio_icon = "audio-volume-muted"

function init()
    local audio = mesh.service.use("audio")
    if audio.muted then
        audio_icon = "audio-volume-muted"
    else
        audio_icon = "audio-volume-high"
    end
end
"#,
        )
        .unwrap();

        let payload = serde_json::json!({ "muted": false });
        ctx.apply_service_payload("audio", &payload);
        ctx.call_init().unwrap();
        assert_eq!(
            ctx.state.get("audio_icon"),
            Some(Value::String("audio-volume-high".into()))
        );
    }

    #[test]
    fn provider_only_service_use_creates_read_only_proxy() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.theme.read"));
        let mut ctx = ScriptContext::new("@test/theme-widget", caps).unwrap();
        ctx.set_interface_catalog(theme_provider_only_catalog());
        ctx.load_script(
            r#"
theme_icon = "weather-clear-night"

function sync_theme_state()
    local theme = mesh.service.use("theme")
    if theme.is_dark then
        theme_icon = "weather-clear-night"
    else
        theme_icon = "weather-clear"
    end
end
"#,
        )
        .unwrap();

        ctx.apply_service_payload("theme", &serde_json::json!({ "is_dark": false }));
        ctx.call_handler("sync_theme_state", &[]).unwrap();
        assert_eq!(
            ctx.state.get("theme_icon"),
            Some(Value::String("weather-clear".into()))
        );
        assert!(ctx.drain_diagnostics().is_empty());
    }

    #[test]
    fn interface_proxy_method_publishes_service_command() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        caps.grant(Capability::new("service.audio.control"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = require("@mesh/audio@>=1.0")
    audio:set_volume("default", 0.5)
    audio.set_volume("default", 0.5)
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();
        let published = ctx.drain_published_events();
        assert_eq!(published.len(), 2);
        for event in published {
            assert_eq!(event.channel, "mesh.audio.set_volume");
            assert_eq!(event.source_plugin_id, "@test/audio-widget");
            assert!(
                event
                    .source_capabilities
                    .is_granted(&Capability::new("service.audio.control"))
            );
            assert_eq!(
                event.payload,
                serde_json::json!({ "device_id": "default", "volume": 0.5 })
            );
        }
    }

    #[test]
    fn interface_proxy_method_returns_queued_result() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        caps.grant(Capability::new("service.audio.control"));
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
queued_ok = false
queued = false

function init()
    local audio = require("@mesh/audio@>=1.0")
    local result = audio.set_volume("default", 0.5)
    queued_ok = result.ok
    queued = result.queued
end
"#,
        )
        .unwrap();

        ctx.call_init().unwrap();

        assert_eq!(ctx.state.get("queued_ok"), Some(serde_json::json!(true)));
        assert_eq!(ctx.state.get("queued"), Some(serde_json::json!(true)));
        let published = ctx.drain_published_events();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].channel, "mesh.audio.set_volume");
    }

    #[test]
    fn read_only_interface_proxy_returns_capability_denied_result() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@test/read-only-audio", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
audio_percent = 0
denied_ok = true
denied_error = ""

function read_state()
    local audio = require("@mesh/audio@>=1.0")
    audio_percent = audio.percent or 0
end

function change_volume()
    local audio = require("@mesh/audio@>=1.0")
    local result = audio.set_volume("default", 0.5)
    denied_ok = result.ok
    denied_error = result.error or ""
end
"#,
        )
        .unwrap();

        ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 64 }));
        ctx.call_handler("read_state", &[]).unwrap();
        assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(64)));

        ctx.call_handler("change_volume", &[]).unwrap();
        assert_eq!(ctx.state.get("denied_ok"), Some(serde_json::json!(false)));
        assert_eq!(
            ctx.state.get("denied_error"),
            Some(serde_json::json!("capability denied"))
        );
        assert!(
            ctx.drain_published_events().is_empty(),
            "read-only audio proxy must not publish mesh.audio.set_volume"
        );
    }

    #[test]
    fn handler_receives_event_payload_argument() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/click", caps).unwrap();
        ctx.load_script(
            r#"
last_margin_left = 0
last_pointer_x = 0

function on_click(event)
    last_margin_left = event.current_target.position.margin_left
    last_pointer_x = event.pointer.x
end
"#,
        )
        .unwrap();

        ctx.call_handler(
            "on_click",
            &[serde_json::json!({
                "pointer": { "x": 42.0, "y": 10.0 },
                "current_target": {
                    "position": {
                        "margin_left": 128,
                        "margin_top": 8
                    }
                }
            })],
        )
        .unwrap();

        assert_eq!(
            ctx.state.get("last_margin_left"),
            Some(Value::Number(128.into()))
        );
        assert_eq!(ctx.state.get("last_pointer_x"), Some(serde_json::json!(42)));
    }
}
