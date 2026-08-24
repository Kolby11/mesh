use super::super::element_ref::ElementAction;
use super::super::lookup::{lua_err, map_lua_error};
use super::super::{
    PublishedEvent, ScriptDiagnostic, ScriptDiagnosticCategory, ScriptError, ScriptState,
};
use super::*;
use crate::storage::create_lua_storage_table;
use crate::util::is_named_event_channel;
use mlua::{Function, LuaSerdeExt, MultiValue, Table, Value as LuaValue, Variadic};
use serde_json::Value;
use std::sync::{Arc, atomic::Ordering};

impl ScriptContext {
    /// Install a **live** `bind:this` reference to another component instance.
    ///
    /// Builds a proxy table whose metatable forwards `__index`/`__newindex`
    /// straight to the child's live `_ENV`. Parent and child share one thread VM
    /// (see [`SurfaceVm`]), so reads see the child's current value and calls run
    /// the child's real function synchronously, with no copy.
    ///
    /// A denylist from the child's `builtin_globals` plus the lifecycle hooks
    /// hides host internals, so only public values and functions pass through.
    /// Takes `&self`/`&child` so the caller can borrow both runtimes out of one
    /// map guard; both must already be initialized.
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
        let child_scalars = child
            .reactive_scalar_globals
            .as_ref()
            .expect("initialized child has reactive scalar table")
            .clone();
        let denylist = child.builtin_globals.clone();
        let child_external_accessed = Arc::clone(&child.live_binding_external_accessed);
        // The parent's own flag, for the reverse direction: the child firing a
        // `self.<Event>` channel the parent subscribed to.
        let parent_external_accessed = Arc::clone(&self.live_binding_external_accessed);
        let event_channel_wrappers = lua.create_table().map_err(lua_err)?;

        let proxy = lua.create_table().map_err(lua_err)?;
        let meta = lua.create_table().map_err(lua_err)?;

        let index_env = child_env.clone();
        let index_scalars = child_scalars;
        let index_deny = denylist.clone();
        let index_external_accessed = Arc::clone(&child_external_accessed);
        let index_parent_accessed = Arc::clone(&parent_external_accessed);
        let index_channel_wrappers = event_channel_wrappers;
        meta.set(
            "__index",
            lua.create_function(move |lua, (_proxy, key): (Table, String)| {
                if is_denied_binding_key(&key, &index_deny) {
                    return Ok(LuaValue::Nil);
                }
                // raw_get keeps the surface curated: only the child's own public
                // members are exposed, not globals inherited via `_ENV.__index`.
                let mut raw = index_env.raw_get::<LuaValue>(key.as_str())?;
                if matches!(raw, LuaValue::Nil) {
                    raw = index_scalars.raw_get::<LuaValue>(key.as_str())?;
                }
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
                // `self.Event:fire(...)` in the same tick.
                if is_named_event_channel(&key) {
                    let channel = self_event_channel(lua, &index_env, &key)?;
                    return parent_subscription_channel(
                        lua,
                        &channel,
                        &index_channel_wrappers,
                        &key,
                        &index_parent_accessed,
                    )
                    .map(LuaValue::Table);
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
                newindex_env.set(key, value)
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
    /// `child.set_volume(50)` through a live binding runs synchronously in the
    /// shared VM, so the child's Lua `_ENV` changes but its Rust-side
    /// `ScriptState` does not. The shell calls this on every bound child after a
    /// parent handler so `{bound vars}` re-render.
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

    /// Call the script's `init(self)` function if it exists. A no-argument
    /// `init()` still works — Luau ignores extra arguments.
    pub fn call_init(&mut self) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let _budget = self.realm_policy.begin_callback();
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
        let _budget = self.realm_policy.begin_callback();
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
            self.tracking_storage_reads.store(true, Ordering::Release);
            let result = self.call_handler("render", &[]);
            self.tracking_storage_reads.store(false, Ordering::Release);
            result?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(super) fn current_self_table(&mut self) -> Result<Table, ScriptError> {
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
                    category: ScriptDiagnosticCategory::Storage,
                    interface: "self.storage".to_string(),
                    requested_version: None,
                    reason,
                });
            }),
            Arc::new(move |key| {
                if tracking_storage_reads.load(Ordering::Acquire) {
                    tracked_storage_keys.lock().unwrap().insert(key.to_string());
                }
            }),
            Arc::new(move |key| {
                pending_storage_side_channels.store(true, Ordering::Release);
                changed_storage_keys.lock().unwrap().insert(key.to_string());
            }),
        )
        .map_err(lua_err)?;
        current_self.set("storage", storage).map_err(lua_err)?;
        // Self event channels (`self.Changed`) are registered on the per-instance
        // _ENV so two instances of the same component keep independent channels
        // when they share one thread VM.
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

    /// The current state, for tree building.
    pub fn state(&self) -> &ScriptState {
        &self.state
    }

    /// The current state, mutably.
    pub fn state_mut(&mut self) -> &mut ScriptState {
        &mut self.state
    }

    pub fn drain_published_events(&mut self) -> Vec<PublishedEvent> {
        self.sync_side_channels();
        let events = std::mem::take(&mut self.published_events);
        self.realm_policy.budget().release_queue(events.len());
        events
    }

    /// Deliver a terminal result to a ticket created by this context.
    ///
    /// The result is intentionally stored outside Lua. A backend completion
    /// can therefore arrive between handler calls without mutating the shared
    /// VM or executing arbitrary script code on the shell thread.
    pub fn complete_service_call(
        &mut self,
        call_id: u64,
        status: impl Into<String>,
        result: Value,
    ) -> bool {
        let mut completions = self.service_call_completions.lock().unwrap();
        if completions.contains_key(&call_id) {
            return false;
        }
        completions.insert(
            call_id,
            super::super::ServiceCallCompletion {
                status: status.into(),
                result,
            },
        );
        true
    }

    pub fn drain_diagnostics(&mut self) -> Vec<ScriptDiagnostic> {
        self.sync_side_channels();
        std::mem::take(&mut self.diagnostics)
    }

    /// Drain imperative element actions (`refs.<name>:focus()`, …) queued by the
    /// script so the shell can execute them against the real widget tree.
    pub fn drain_element_actions(&mut self) -> Vec<ElementAction> {
        self.sync_side_channels();
        let actions = std::mem::take(&mut self.element_actions);
        self.realm_policy.budget().release_queue(actions.len());
        actions
    }

    pub fn flush_storage(&mut self) {
        let result = self.storage.lock().unwrap().flush_if_dirty();
        if let Err(error) = result {
            self.diagnostics.push(ScriptDiagnostic {
                module_id: self.module_id.clone(),
                category: ScriptDiagnosticCategory::Storage,
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
}
