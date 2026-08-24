use super::*;
use mesh_core_elements::VariableStore;
use mlua::{LuaSerdeExt, Value as LuaValue};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::Ordering;

/// Record a changed public member for the template-dependency gate.
///
/// The list is only cleared by a complete template evaluation. A frame whose
/// writes touch no template input never rebuilds, so pushing blindly would let
/// a member that changes every frame grow the list without bound and make the
/// gate's scan progressively slower.
fn record_changed_public_member(changed: &mut Vec<String>, name: &str) {
    if changed.iter().any(|recorded| recorded == name) {
        return;
    }
    changed.push(name.to_owned());
}

impl ScriptContext {
    /// Sync Lua globals back into ScriptState.
    ///
    /// Any global assigned by the script (i.e. not in the builtin snapshot,
    /// not prefixed with `__`, and not a function) is reactive state and gets
    /// synced to the template. Local variables are never synced.
    pub(in crate::context) fn sync_state_from_lua(&mut self) {
        let _span = tracing::debug_span!("sync_state_from_lua", module = %self.module_id).entered();
        if !self.user_globals_discovered {
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
                let proxy_value = is_proxyable_lua_scalar(&lua_value).then(|| lua_value.clone());
                if let Ok(value) = self.lua().from_value::<Value>(lua_value) {
                    self.state.set(name.clone(), value);
                    self.user_global_key_set.insert(name.clone());
                    self.user_global_keys.push(name.clone());
                    if let Some(proxy_value) = proxy_value {
                        let _ = self.proxy_scalar_global(&name, proxy_value);
                    }
                }
            }
            self.assigned_global_keys.lock().unwrap().clear();
            self.pending_assigned_global_keys
                .store(false, Ordering::Release);
            self.user_globals_discovered = true;
        } else {
            // Compound globals remain raw in `_ENV` because their tables can be
            // mutated in place. Scalars are absent from the raw table and only
            // need a read when `__newindex` records an assignment.
            let mut newly_proxyable = Vec::new();
            for key in &self.user_global_keys {
                if self.proxied_scalar_global_keys.contains(key) {
                    continue;
                }
                if let Ok(lua_value) = self.env().get::<LuaValue>(key.as_str()) {
                    if matches!(lua_value, LuaValue::Nil) {
                        if self.state.remove(key) {
                            record_changed_public_member(&mut self.changed_public_members, key);
                        }
                        continue;
                    }
                    if !matches!(lua_value, LuaValue::Nil | LuaValue::Function(_)) {
                        if is_proxyable_lua_scalar(&lua_value) {
                            newly_proxyable.push((key.clone(), lua_value.clone()));
                        }
                        if self
                            .state
                            .get_ref(key)
                            .is_some_and(|current| lua_scalar_matches_json(&lua_value, current))
                        {
                            continue;
                        }
                        if let Ok(value) = self.lua().from_value::<Value>(lua_value) {
                            self.state.set(key.clone(), value);
                            record_changed_public_member(&mut self.changed_public_members, key);
                        }
                    }
                }
            }
            for (name, value) in newly_proxyable {
                let _ = self.proxy_scalar_global(&name, value);
            }
            if self
                .pending_assigned_global_keys
                .swap(false, Ordering::AcqRel)
            {
                let assigned_keys = {
                    let mut assigned = self.assigned_global_keys.lock().unwrap();
                    assigned.drain().collect::<Vec<_>>()
                };
                for name in assigned_keys {
                    if name.starts_with("__") || self.builtin_globals.contains(&name) {
                        continue;
                    }
                    if self.user_global_key_set.contains(&name) {
                        if !self.proxied_scalar_global_keys.contains(&name) {
                            // Raw compound globals were already checked above.
                            continue;
                        }
                        if let Ok(lua_value) = self.env().get::<LuaValue>(name.as_str()) {
                            if matches!(lua_value, LuaValue::Nil) {
                                if self.proxied_scalar_global_keys.contains(&name) {
                                    let _ = self.unproxy_scalar_global(&name, lua_value.clone());
                                }
                                if self.state.remove(&name) {
                                    record_changed_public_member(
                                        &mut self.changed_public_members,
                                        &name,
                                    );
                                }
                            } else if is_proxyable_lua_scalar(&lua_value) {
                                if !self.state.get_ref(&name).is_some_and(|current| {
                                    lua_scalar_matches_json(&lua_value, current)
                                }) && let Ok(value) = self.lua().from_value::<Value>(lua_value)
                                {
                                    self.state.set(name.clone(), value);
                                    record_changed_public_member(
                                        &mut self.changed_public_members,
                                        &name,
                                    );
                                }
                            } else {
                                let value_for_env = lua_value.clone();
                                let _ = self.unproxy_scalar_global(&name, value_for_env);
                                if !matches!(lua_value, LuaValue::Nil | LuaValue::Function(_))
                                    && let Ok(value) = self.lua().from_value::<Value>(lua_value)
                                {
                                    self.state.set(name.clone(), value);
                                    record_changed_public_member(
                                        &mut self.changed_public_members,
                                        &name,
                                    );
                                }
                            }
                        }
                        continue;
                    }
                    self.user_global_key_set.insert(name.clone());
                    self.user_global_keys.push(name.clone());
                    if let Ok(lua_value) = self.env().get::<LuaValue>(name.as_str())
                        && !matches!(lua_value, LuaValue::Nil | LuaValue::Function(_))
                        && let Ok(value) = self.lua().from_value::<Value>(lua_value.clone())
                    {
                        self.state.set(name.clone(), value);
                        record_changed_public_member(&mut self.changed_public_members, &name);
                        if is_proxyable_lua_scalar(&lua_value) {
                            let _ = self.proxy_scalar_global(&name, lua_value);
                        }
                    }
                }
            }
        }

        if self.pending_redraw.swap(false, Ordering::AcqRel) {
            self.state.dirty = true;
        }
    }

    pub(super) fn sync_side_channels(&mut self) {
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
}
