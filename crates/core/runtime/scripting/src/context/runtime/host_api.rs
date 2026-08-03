use super::super::element_ref::create_refs_proxy;
use super::super::lookup::{
    interface_error_message, lookup_failure_reason, lua_err, record_lookup_diagnostic,
    record_lookup_diagnostic_lua,
};
use super::super::proxy::{create_event_channel, create_interface_proxy};
use super::super::{PublishedEvent, ScriptError, ScriptInterfaceImport};
use super::*;
use crate::host_api::{HostApiManifest, InterfaceProxy};
use mlua::{Error as LuaError, LuaSerdeExt, Table, Value as LuaValue, Variadic};
use serde_json::Value;
use std::sync::{Arc, atomic::Ordering};

impl ScriptContext {
    pub(super) fn install_host_api(&mut self, target: &mlua::Table) -> Result<(), ScriptError> {
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
            .set("__mesh_locale_current", "en")
            .map_err(lua_err)?;

        self.install_loader_api(globals, &mesh_for_require, &manifest)?;
        self.install_refs_api(globals)?;
        Ok(())
    }

    fn install_module_api(&mut self, globals: &mlua::Table) -> Result<(), ScriptError> {
        let module_object = self.lua().create_table().map_err(lua_err)?;
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
        _globals: &mlua::Table,
        mesh_ui_api: &Table,
    ) -> Result<(), ScriptError> {
        let pending_redraw = Arc::clone(&self.pending_redraw);
        mesh_ui_api
            .set(
                "request_redraw",
                self.lua()
                    .create_function(move |_lua, ()| {
                        pending_redraw.store(true, Ordering::Release);
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
        let i18n_translations = Arc::clone(&self.i18n_translations);
        // The per-instance _ENV is the channel-registry scope so interface event
        // channels stay private when components share one thread VM.
        let scope_for_require = globals.clone();
        let mesh_for_require = mesh_for_require.clone();
        let require = self
            .lua()
            .create_function(move |lua, module: String| {
                if module == "@mesh/i18n" || module == "mesh.i18n" {
                    return create_i18n_library(lua, Arc::clone(&i18n_translations))
                        .map(LuaValue::Table);
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
        // read lazily from the latest surface snapshot and methods (`focus`,
        // `blur`, …) enqueue element actions against the real widget tree.
        let refs_proxy = create_refs_proxy(
            self.lua(),
            Arc::clone(&self.shared_element_metrics),
            Arc::clone(&self.shared_element_actions),
            Arc::clone(&self.pending_side_channels),
        )
        .map_err(lua_err)?;
        globals.set("refs", refs_proxy).map_err(lua_err)?;
        Ok(())
    }

    pub(super) fn install_interface_imports(
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
}
