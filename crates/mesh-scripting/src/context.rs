use crate::host_api::{HostApiManifest, InterfaceProxy};
/// Script execution context — one per plugin/component instance.
use mesh_capability::CapabilitySet;
use mesh_locale::LocaleEngine;
use mesh_service::contract::InterfaceTypeDef;
use mesh_service::{InterfaceCatalog, InterfaceResolution};
use mesh_ui::VariableStore;
use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::fmt;

#[derive(Debug, Clone)]
pub struct PublishedEvent {
    pub channel: String,
    pub payload: Value,
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

        if self.variables.get(&name) == Some(&value) {
            return;
        }
        self.variables.insert(name, value);
        self.dirty = true;
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

/// A reactive binding from a service field to a Lua global variable.
/// Registered at runtime when scripts call
/// `mesh.service.bind("service.field", "local_name")`.
#[derive(Debug, Clone)]
struct ServiceBinding {
    var_name: String,
    service: String,
    field: String,
}

/// An explicit service subscription registered by `mesh.service.on`.
#[derive(Debug, Clone)]
struct ServiceSubscription {
    service: String,
    handler_name: String,
}

/// A script execution context for one component instance.
///
/// Owns frontend script state, capability metadata, and the mlua runtime.
/// Scripts run as-written with no source preprocessing. Reactive state is
/// tracked via `mesh.state.set(key, value)` calls at runtime, and service
/// bindings/subscriptions are registered via `mesh.service.bind(...)` and
/// `mesh.service.on(...)`.
#[derive(Debug)]
pub struct ScriptContext {
    pub plugin_id: String,
    pub capabilities: CapabilitySet,
    pub state: ScriptState,
    lua: Lua,
    interface_catalog: InterfaceCatalog,
    interface_bindings: HashMap<String, InterfaceResolution>,
    shared_interface_bindings: Arc<Mutex<HashMap<String, InterfaceResolution>>>,
    /// Keys registered by `mesh.state.set` — synced back from Lua globals after each call.
    tracked_state_keys: Arc<Mutex<HashSet<String>>>,
    /// Service bindings registered by `mesh.service.bind` at script load time.
    runtime_service_bindings: Arc<Mutex<Vec<ServiceBinding>>>,
    /// Service update handlers registered explicitly via `mesh.service.on`.
    runtime_service_subscriptions: Arc<Mutex<Vec<ServiceSubscription>>>,
    published_events: Vec<PublishedEvent>,
    shared_published_events: Arc<Mutex<Vec<PublishedEvent>>>,
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
            tracked_state_keys: Arc::new(Mutex::new(HashSet::new())),
            runtime_service_bindings: Arc::new(Mutex::new(Vec::new())),
            runtime_service_subscriptions: Arc::new(Mutex::new(Vec::new())),
            published_events: Vec::new(),
            shared_published_events: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn set_interface_catalog(&mut self, catalog: InterfaceCatalog) {
        self.interface_catalog = catalog;
    }

    /// Load a script source. Executes the script, registering functions and
    /// seeding reactive state from `mesh.state.set` calls at the top level.
    pub fn load_script(&mut self, source: &str) -> Result<(), ScriptError> {
        self.interface_bindings.clear();
        self.shared_interface_bindings.lock().unwrap().clear();
        self.shared_published_events.lock().unwrap().clear();
        self.tracked_state_keys.lock().unwrap().clear();
        self.runtime_service_bindings.lock().unwrap().clear();
        self.runtime_service_subscriptions.lock().unwrap().clear();
        self.install_host_api()?;
        self.lua
            .load(source)
            .set_name(&self.plugin_id)
            .exec()
            .map_err(|err| map_lua_error(&self.interface_catalog, err))?;
        self.sync_state_from_lua();
        tracing::info!("loaded script for plugin {}", self.plugin_id);
        Ok(())
    }

    /// Copy service payload fields into bound Lua globals and script state.
    ///
    /// Called by core after each service update, before any subscribed handlers.
    /// Bindings are registered at runtime via `mesh.service.bind(...)`.
    pub fn apply_service_bindings(&mut self, service: &str, payload: &Value) {
        let bindings = self.runtime_service_bindings.lock().unwrap().clone();
        let updates: Vec<(String, Value)> = bindings
            .iter()
            .filter(|b| b.service == service)
            .filter_map(|b| payload.get(&b.field).map(|v| (b.var_name.clone(), v.clone())))
            .collect();
        for (var, val) in updates {
            self.state.set(var.clone(), val.clone());
            self.set_lua_global(&var, &val);
        }
    }

    /// Call all handlers registered for a specific service.
    pub fn call_service_handlers(&mut self, service: &str) -> Result<(), ScriptError> {
        let handlers: Vec<String> = self
            .runtime_service_subscriptions
            .lock()
            .unwrap()
            .iter()
            .filter(|subscription| subscription.service == service)
            .map(|subscription| subscription.handler_name.clone())
            .collect();

        for handler_name in handlers {
            self.call_handler(&handler_name, &[])?;
        }

        Ok(())
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

    /// Check if a handler exists.
    pub fn has_handler(&self, name: &str) -> bool {
        self.lua.globals().get::<Function>(name).is_ok()
    }

    fn install_host_api(&mut self) -> Result<(), ScriptError> {
        let globals = self.lua.globals();
        let mesh = self.lua.create_table().map_err(lua_err)?;
        let mesh_state = self.lua.create_table().map_err(lua_err)?;
        let mesh_service = self.lua.create_table().map_err(lua_err)?;
        let mesh_interfaces = self.lua.create_table().map_err(lua_err)?;
        let mesh_services = self.lua.create_table().map_err(lua_err)?;
        let mesh_events = self.lua.create_table().map_err(lua_err)?;
        let mesh_ui = self.lua.create_table().map_err(lua_err)?;
        let mesh_log = self.lua.create_table().map_err(lua_err)?;

        let tracked = Arc::clone(&self.tracked_state_keys);
        mesh_state.set(
            "set",
            self.lua
                .create_function(move |lua, (key, value): (String, LuaValue)| {
                    lua.globals().set(key.clone(), value)?;
                    tracked.lock().unwrap().insert(key);
                    Ok(())
                })
                .map_err(lua_err)?,
        )
        .map_err(lua_err)?;

        let service_bindings = Arc::clone(&self.runtime_service_bindings);
        mesh_service.set(
            "bind",
            self.lua
                .create_function(move |lua, (path, alias): (String, Option<String>)| {
                    let Some((service, field)) = path.split_once('.') else {
                        return Ok(LuaValue::Nil);
                    };
                    let var_name = alias.unwrap_or_else(|| field.to_string());
                    lua.globals().set(var_name.clone(), LuaValue::Nil)?;
                    service_bindings.lock().unwrap().push(ServiceBinding {
                        var_name,
                        service: service.to_string(),
                        field: field.to_string(),
                    });
                    Ok(LuaValue::Nil)
                })
                .map_err(lua_err)?,
        )
        .map_err(lua_err)?;

        let service_subscriptions = Arc::clone(&self.runtime_service_subscriptions);
        mesh_service.set(
            "on",
            self.lua
                .create_function(move |_lua, (service, handler_name): (String, String)| {
                    service_subscriptions
                        .lock()
                        .unwrap()
                        .push(ServiceSubscription {
                            service,
                            handler_name,
                        });
                    Ok(LuaValue::Nil)
                })
                .map_err(lua_err)?,
        )
        .map_err(lua_err)?;

        let interface_catalog = self.interface_catalog.clone();
        let manifest = HostApiManifest::from_capabilities(&self.capabilities);
        let allowed_interfaces = manifest.interface_capabilities.clone();
        let has_theme_read = manifest.has_theme_read;
        let has_locale_read = manifest.has_locale_read;
        let bindings = Arc::clone(&self.shared_interface_bindings);
        mesh_interfaces
            .set(
                "get",
                self.lua
                    .create_function(move |lua, (name, version): (String, Option<String>)| {
                        let canonical = InterfaceProxy::canonical_name(&name);
                        let readable = canonical == "mesh.theme" && has_theme_read
                            || canonical == "mesh.locale" && has_locale_read
                            || allowed_interfaces.contains(&canonical)
                            || !canonical.starts_with("mesh.");
                        if canonical.starts_with("mesh.") && !readable {
                            return Err(mlua::Error::external(ScriptError::CapabilityDenied(
                                canonical,
                            )));
                        }

                        let resolution = interface_catalog.resolve(&canonical, version.as_deref());
                        if resolution.contract.is_none() || resolution.provider.is_none() {
                            return Err(mlua::Error::external(ScriptError::InterfaceUnavailable(
                                format!(
                                    "{}{}",
                                    canonical,
                                    version
                                        .as_deref()
                                        .map(|value| format!(" ({value})"))
                                        .unwrap_or_default()
                                ),
                            )));
                        }

                        bindings.lock().unwrap().insert(name.clone(), resolution.clone());
                        create_interface_proxy(lua, resolution)
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let interfaces_get: Function = mesh_interfaces.get("get").map_err(lua_err)?;
        mesh_services
            .set(
                "get",
                self.lua
                    .create_function(move |_lua, name: String| {
                        interfaces_get.call::<LuaValue>((name, Option::<String>::None))
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        let published_events = Arc::clone(&self.shared_published_events);
        let plugin_id = self.plugin_id.clone();
        mesh_events
            .set(
                "publish",
                self.lua
                    .create_function(move |lua, (channel, payload): (String, Option<LuaValue>)| {
                        let payload = payload.unwrap_or(LuaValue::Nil);
                        let payload = lua.from_value::<Value>(payload)?;
                        tracing::info!("{} published event {}", plugin_id, channel);
                        published_events
                            .lock()
                            .unwrap()
                            .push(PublishedEvent { channel, payload });
                        Ok(())
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)?;

        mesh_ui
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

        mesh.set("state", mesh_state).map_err(lua_err)?;
        mesh.set("service", mesh_service).map_err(lua_err)?;
        mesh.set("interfaces", mesh_interfaces).map_err(lua_err)?;
        mesh.set("services", mesh_services).map_err(lua_err)?;
        mesh.set("events", mesh_events).map_err(lua_err)?;
        mesh.set("ui", mesh_ui).map_err(lua_err)?;
        mesh.set("log", mesh_log).map_err(lua_err)?;
        globals.set("mesh", mesh).map_err(lua_err)?;
        globals.set("__mesh_request_redraw", false).map_err(lua_err)?;

        let require = self
            .lua
            .create_function(|lua, module: String| {
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
                Err(mlua::Error::external(ScriptError::LuaError(format!(
                    "unsupported require: {module}"
                ))))
            })
            .map_err(lua_err)?;
        globals.set("require", require).map_err(lua_err)?;
        Ok(())
    }

    fn set_lua_global(&self, name: &str, value: &Value) {
        if let Ok(lua_value) = self.lua.to_value(value) {
            let _ = self.lua.globals().set(name, lua_value);
        }
    }

    /// Sync Lua global values back into ScriptState for all keys registered
    /// via `mesh.state.set`. Called after every script execution.
    fn sync_state_from_lua(&mut self) {
        let keys: Vec<String> = self.tracked_state_keys.lock().unwrap().iter().cloned().collect();
        let globals = self.lua.globals();
        for name in &keys {
            if let Ok(lua_value) = globals.get::<LuaValue>(name.as_str()) {
                if let Ok(value) = self.lua.from_value::<Value>(lua_value) {
                    self.state.set(name.clone(), value);
                }
            }
        }
        if globals
            .get::<bool>("__mesh_request_redraw")
            .unwrap_or(false)
        {
            self.state.dirty = true;
            let _ = globals.set("__mesh_request_redraw", false);
        }
    }

    fn sync_side_channels(&mut self) {
        {
            let mut published = self.shared_published_events.lock().unwrap();
            if !published.is_empty() {
                self.published_events.extend(published.drain(..));
            }
        }
        self.interface_bindings = self.shared_interface_bindings.lock().unwrap().clone();
    }
}

fn create_interface_proxy(lua: &Lua, resolution: InterfaceResolution) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    let meta = lua.create_table()?;
    let resolution_for_index = resolution.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (_table, key): (Table, String)| {
            let resolution = resolution_for_index.clone();
            let method_name = key.clone();
            lua.create_function(move |lua, _args: mlua::Variadic<LuaValue>| {
                let contract = resolution.contract.as_ref().ok_or_else(|| {
                    mlua::Error::external(ScriptError::InterfaceUnavailable(
                        resolution.requested.clone(),
                    ))
                })?;

                let method = contract.methods.iter().find(|candidate| candidate.name == method_name);
                let Some(method) = method else {
                    return Err(mlua::Error::external(ScriptError::UnsupportedOperation {
                        interface: contract.interface.clone(),
                        method: method_name.clone(),
                    }));
                };

                default_lua_value_for_type(lua, &contract.types, method.returns.as_deref())
            })
        })?,
    )?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

fn default_lua_value_for_type(
    lua: &Lua,
    types: &HashMap<String, InterfaceTypeDef>,
    ty: Option<&str>,
) -> mlua::Result<LuaValue> {
    let Some(ty) = ty.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(LuaValue::Nil);
    };

    if ty.ends_with('?') {
        return Ok(LuaValue::Nil);
    }

    if ty.starts_with('[') && ty.ends_with(']') {
        return Ok(LuaValue::Table(lua.create_table()?));
    }

    let value = match ty {
        "string" => LuaValue::String(lua.create_string("")?),
        "boolean" => LuaValue::Boolean(false),
        "float" | "number" | "integer" | "int" => LuaValue::Integer(0),
        "Result" => LuaValue::Nil,
        custom => {
            if let Some(def) = types.get(custom) {
                let table = lua.create_table()?;
                for field in &def.fields {
                    let field_value = default_lua_value_for_type(lua, types, Some(&field.arg_type))?;
                    table.set(field.name.as_str(), field_value)?;
                }
                LuaValue::Table(table)
            } else {
                LuaValue::Nil
            }
        }
    };

    Ok(value)
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
    use mesh_capability::{Capability, CapabilitySet};
    use mesh_service::{
        ContractCapabilities, InterfaceCatalog, InterfaceContract, InterfaceMethod,
        InterfaceProvider, parse_contract_version,
    };
    use std::path::PathBuf;

    fn audio_catalog() -> InterfaceCatalog {
        let mut catalog = InterfaceCatalog::default();
        catalog.register_contract(InterfaceContract {
            interface: "mesh.audio".into(),
            version: parse_contract_version("1.0").unwrap(),
            file_path: PathBuf::from("<test>"),
            methods: vec![
                InterfaceMethod {
                    name: "default_output".into(),
                    args: Vec::new(),
                    returns: Some("Device?".into()),
                },
                InterfaceMethod {
                    name: "set_volume".into(),
                    args: Vec::new(),
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
            provider_plugin: "@mesh/pipewire-audio".into(),
            backend_name: "PipeWire".into(),
            priority: 100,
        });
        catalog
    }

    #[test]
    fn binds_interface_from_new_api() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();

        assert_eq!(
            ctx.interface_bindings
                .get("mesh.audio")
                .map(|resolution| resolution.requested.as_str()),
            Some("mesh.audio")
        );
    }

    #[test]
    fn binds_interface_from_legacy_service_api() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));

        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = mesh.services.get("audio")
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();

        assert_eq!(
            ctx.interface_bindings
                .get("audio")
                .map(|resolution| resolution.requested.as_str()),
            Some("mesh.audio")
        );
    }

    #[test]
    fn rejects_missing_interface_contract() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.load_script(
            r#"
function init()
    local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
end
"#,
        )
        .unwrap();

        let err = ctx.call_init().unwrap_err();
        assert!(matches!(err, ScriptError::InterfaceUnavailable(_)));
    }

    #[test]
    fn rejects_unsupported_interface_method() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/test", caps).unwrap();
        ctx.set_interface_catalog(audio_catalog());
        ctx.load_script(
            r#"
function init()
    local audio = mesh.interfaces.get("mesh.audio", ">=1.0")
    audio:mute_all()
end
"#,
        )
        .unwrap();

        let err = ctx.call_init().unwrap_err();
        assert!(matches!(
            err,
            ScriptError::UnsupportedOperation { interface, method }
            if interface == "mesh.audio" && method == "mute_all"
        ));
    }

    #[test]
    fn explicit_state_set_seeds_and_tracks_state() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/local", caps).unwrap();
        ctx.load_script(
            r#"
mesh.state.set("volumeHidden", true)
mesh.state.set("count", 0)

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
    fn if_then_end_executes_conditionally() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/if", caps).unwrap();
        ctx.load_script(
            r#"
mesh.state.set("a", true)
mesh.state.set("b", false)

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
    fn service_bindings_update_globals_and_trigger_handler() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
        ctx.load_script(
            r#"
mesh.state.set("icon_name", "audio-volume-muted")
mesh.service.bind("audio.muted", "audio_muted")
mesh.service.bind("audio.percent", "audio_percent")
mesh.service.on("audio", "sync_audio_state")

function sync_audio_state()
    if audio_muted then
        icon_name = "audio-volume-muted"
    else
        if audio_percent < 34 then
            icon_name = "audio-volume-low"
        else
            if audio_percent < 67 then
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

        // Initial default
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-muted".into()))
        );

        // Simulate 65% volume, not muted
        let payload = serde_json::json!({ "percent": 65, "muted": false });
        ctx.apply_service_bindings("audio", &payload);
        ctx.call_service_handlers("audio").unwrap();
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-medium".into()))
        );

        // High volume
        let payload = serde_json::json!({ "percent": 90, "muted": false });
        ctx.apply_service_bindings("audio", &payload);
        ctx.call_service_handlers("audio").unwrap();
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-high".into()))
        );

        // Muted
        let payload = serde_json::json!({ "percent": 90, "muted": true });
        ctx.apply_service_bindings("audio", &payload);
        ctx.call_service_handlers("audio").unwrap();
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(Value::String("audio-volume-muted".into()))
        );
    }

    #[test]
    fn handler_receives_event_payload_argument() {
        let caps = CapabilitySet::new();
        let mut ctx = ScriptContext::new("@test/click", caps).unwrap();
        ctx.load_script(
            r#"
mesh.state.set("last_margin_left", 0)
mesh.state.set("last_pointer_x", 0)

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
