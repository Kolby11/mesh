use super::lookup::{
    interface_error_message, lookup_failure_reason, lua_err, lua_value_to_string, map_lua_error,
    record_lookup_diagnostic, record_lookup_diagnostic_lua,
};
use super::proxy::create_interface_proxy;
use super::{PublishedEvent, ScriptDiagnostic, ScriptError, ScriptInterfaceImport, ScriptState};
use crate::host_api::{HostApiManifest, InterfaceProxy};
use mesh_core_capability::CapabilitySet;
use mesh_core_service::{InterfaceCatalog, InterfaceResolution};
use mlua::{Error as LuaError, Function, Lua, LuaSerdeExt, Table, Value as LuaValue, Variadic};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

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
    lua: Lua,
    interface_catalog: InterfaceCatalog,
    pub(super) interface_bindings: HashMap<String, InterfaceResolution>,
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
    /// Create a new script context for a module.
    pub fn new(
        module_id: impl Into<String>,
        capabilities: CapabilitySet,
    ) -> Result<Self, ScriptError> {
        Ok(Self {
            module_id: module_id.into(),
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
            .filter_map(|result| result.ok().map(|(key, _)| key))
            .collect();
        self.lua
            .load(source)
            .set_name(&self.module_id)
            .exec()
            .map_err(map_lua_error)?;
        self.sync_state_from_lua();
        tracing::info!("loaded script for module {}", self.module_id);
        Ok(())
    }

    /// Copy the latest service payload into the Lua runtime for proxy reads.
    ///
    /// Called by core after each service update so interface proxies can read
    /// state fields directly without explicit callback or binding APIs.
    pub fn apply_service_payload(&mut self, service: &str, payload: &Value) {
        let service_key = format!("__mesh_svc_{service}");
        if let Ok(lua_value) = self.lua.to_value(payload) {
            let _ = self.lua.globals().set(service_key, lua_value);
        }
    }

    pub fn set_global_state(&mut self, name: &str, value: Value) -> Result<(), ScriptError> {
        let lua_value = self.lua.to_value(&value).map_err(lua_err)?;
        self.lua
            .globals()
            .set(name, lua_value)
            .map_err(map_lua_error)?;
        self.state.set(name.to_string(), value);
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

    pub fn clear_tracked_service_fields(&self) {
        self.tracked_service_fields.lock().unwrap().clear();
    }

    /// Call the script's `init()` function if it exists.
    pub fn call_init(&mut self) -> Result<(), ScriptError> {
        if let Ok(init) = self.lua.globals().get::<Function>("init") {
            tracing::debug!("calling init() for {}", self.module_id);
            init.call::<()>(()).map_err(map_lua_error)?;
            self.sync_state_from_lua();
            self.sync_side_channels();
        }
        Ok(())
    }

    /// Call a named event handler.
    pub fn call_handler(&mut self, name: &str, args: &[Value]) -> Result<(), ScriptError> {
        let globals = self.lua.globals();
        let handler = globals
            .get::<Function>(name)
            .map_err(|_| ScriptError::HandlerNotFound(name.to_string()))?;
        tracing::debug!("calling handler {name}() for {}", self.module_id);
        match args.len() {
            0 => handler.call::<()>(()).map_err(map_lua_error)?,
            1 => {
                let arg = self.lua.to_value(&args[0]).map_err(lua_err)?;
                handler.call::<()>(arg).map_err(map_lua_error)?;
            }
            _ => {
                let mut multi_args = mlua::MultiValue::new();
                for arg in args {
                    multi_args.push_back(self.lua.to_value(arg).map_err(lua_err)?);
                }
                handler.call::<()>(multi_args).map_err(map_lua_error)?;
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
        let mesh_popover = self.lua.create_table().map_err(lua_err)?;
        let interface_catalog = self.interface_catalog.clone();
        let manifest = HostApiManifest::from_capabilities(&self.capabilities);
        let allowed_interfaces = manifest.interface_capabilities.clone();
        let has_theme_read = manifest.has_theme_read;
        let has_locale_read = manifest.has_locale_read;

        let published_events = Arc::clone(&self.shared_published_events);
        let module_id = self.module_id.clone();
        let capabilities = self.capabilities.clone();
        mesh_core_events
            .set(
                "publish",
                self.lua
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

        let module_id = self.module_id.clone();
        mesh_log
            .set(
                "info",
                self.lua
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
                self.lua
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
                self.lua
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
                self.lua
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
                self.lua
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
        globals.set("mesh", mesh).map_err(lua_err)?;
        globals
            .set("__mesh_request_redraw", false)
            .map_err(lua_err)?;

        let published_events = Arc::clone(&self.shared_published_events);
        let tracked_service_fields = Arc::clone(&self.tracked_service_fields);
        let module_id_for_require = self.module_id.clone();
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
                            LuaValue::String(value) => Ok(value.to_str()?.to_string()),
                            other => Ok(lua_value_to_string(other)),
                        })?,
                    )?;
                    return Ok(exports);
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

            self.shared_interface_bindings
                .lock()
                .unwrap()
                .insert(import.alias.clone(), resolution.clone());
            let proxy = create_interface_proxy(
                &self.lua,
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
        let user_globals: Vec<(String, LuaValue)> = self
            .lua
            .globals()
            .pairs::<String, LuaValue>()
            .filter_map(|result| result.ok())
            .filter(|(key, value)| {
                !key.starts_with("__")
                    && !self.builtin_globals.contains(key)
                    && !matches!(value, LuaValue::Function(_))
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
