use super::super::ScriptError;
use super::super::element_ref::install_bound_element_proxies;
use super::super::lookup::{lua_err, map_lua_error};
use super::super::proxy::interface_event_channel;
use super::*;
use mesh_core_elements::VariableStore;
use mlua::{Function, LuaSerdeExt, Value as LuaValue};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

impl ScriptContext {
    /// Copy a capability-authorized service payload into this context's
    /// Rust-owned snapshot store. Interface proxies read that store lazily.
    pub fn apply_service_payload(&mut self, service: &str, payload: &Value) {
        let fingerprint = Self::service_payload_fingerprint(payload);
        self.apply_service_payload_with_fingerprint(service, payload, fingerprint);
    }

    /// Hash a service payload once for reuse across a multi-context fan-out.
    pub fn service_payload_fingerprint(payload: &Value) -> u64 {
        json_value_fingerprint(payload)
    }

    /// Apply a payload with the dispatcher's fingerprint, so the same JSON is
    /// not re-hashed in every component context on a surface.
    pub fn apply_service_payload_with_fingerprint(
        &mut self,
        service: &str,
        payload: &Value,
        payload_fingerprint: u64,
    ) {
        if !self.can_read_service_interface(service) {
            return;
        }
        self.service_context_state
            .lock()
            .unwrap()
            .update(service, payload, payload_fingerprint);
    }

    /// Generation of the last capability-authorized service snapshot in this
    /// context. Equal payloads do not advance it.
    pub fn service_context_generation(&self) -> u64 {
        self.service_context_state.lock().unwrap().generation()
    }

    pub fn service_payload_generation(&self, service: &str) -> Option<u64> {
        self.service_context_state
            .lock()
            .unwrap()
            .payload_generation(service)
    }

    /// Publish per-paint element metrics so live `refs.<name>` reads reflect the
    /// current frame. The `{ name -> fields }` object stays in a shared
    /// surface-owned Rust store; each proxy lowers only its own entry into Lua.
    pub fn apply_element_metrics(&mut self, metrics: &Value) {
        self.apply_element_metrics_inner(Arc::new(metrics.clone()));
    }

    /// Publish element metrics only when the producer's full-snapshot
    /// fingerprint differs from the last snapshot installed in this context.
    pub fn apply_element_metrics_with_fingerprint(&mut self, metrics: &Value, fingerprint: u64) {
        if self.last_element_metrics_fingerprint == Some(fingerprint) {
            return;
        }
        self.apply_element_metrics_inner(Arc::new(metrics.clone()));
        self.last_element_metrics_fingerprint = Some(fingerprint);
    }

    /// Publish an already shared snapshot without cloning its JSON tree.
    pub fn apply_shared_element_metrics_with_fingerprint(
        &mut self,
        metrics: Arc<Value>,
        fingerprint: u64,
    ) {
        if self.last_element_metrics_fingerprint == Some(fingerprint) {
            return;
        }
        self.apply_element_metrics_inner(metrics);
        self.last_element_metrics_fingerprint = Some(fingerprint);
    }

    fn apply_element_metrics_inner(&mut self, metrics: Arc<Value>) {
        let _ = self.ensure_initialized();
        self.shared_element_metrics
            .lock()
            .unwrap()
            .replace(Arc::clone(&metrics));
        let _ = install_bound_element_proxies(
            self.lua(),
            self.env(),
            metrics.as_ref(),
            Arc::clone(&self.shared_element_metrics),
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
        let channel = interface_event_channel(self.lua(), &scope, service, event_name, None, true)
            .map_err(lua_err)?;
        let emit = channel.get::<Function>("emit").map_err(lua_err)?;
        let lua_payload = self.lua().to_value(payload).map_err(lua_err)?;
        emit.call::<()>((channel, lua_payload))
            .map_err(map_lua_error)?;
        self.sync_state_from_lua();
        self.sync_side_channels();
        Ok(())
    }

    /// Seed a host-owned global in this component's `_ENV`.
    ///
    /// Frontend contexts share one thread-local realm, so module- or
    /// instance-specific values must never reach `lua.globals()`. Registering
    /// the key as a builtin also keeps it out of reactive-global discovery: the
    /// host owns it, scripts read it through their normal environment.
    pub fn seed_context_global(&mut self, name: &str, value: Value) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let lua_value = self.lua().to_value(&value).map_err(lua_err)?;
        self.env().raw_set(name, lua_value).map_err(map_lua_error)?;
        self.builtin_globals.insert(name.to_string());
        self.state.set(name.to_string(), value);
        Ok(())
    }

    /// Set a public member on *this component's* `_ENV`, alongside the script's
    /// own bare assignments. Unlike [`Self::seed_context_global`], it
    /// participates in reactive synchronization — for pushing a value into a
    /// member the component declared, such as syncing a portal `hidden={...}`
    /// binding after the shell showed the surface itself.
    pub fn set_member_state(&mut self, name: &str, value: Value) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let lua_value = self.lua().to_value(&value).map_err(lua_err)?;
        self.env().set(name, lua_value).map_err(map_lua_error)?;
        self.state.set(name.to_string(), value);
        Ok(())
    }

    /// Set a public member only when its Rust-side value changed. Host code
    /// re-publishes props on every tree rebuild; skipping unchanged writes
    /// avoids JSON-to-Lua conversion, an `_ENV` mutation, and a public-member
    /// object rebuild.
    pub fn set_member_state_if_changed(
        &mut self,
        name: &str,
        value: Value,
    ) -> Result<bool, ScriptError> {
        if self
            .state
            .get_ref(name)
            .is_some_and(|current| current == &value)
        {
            return Ok(false);
        }
        self.set_member_state(name, value)?;
        Ok(true)
    }

    /// Borrow a candidate value and clone only when the member changed. The
    /// preferred form for host-owned snapshots such as props: an unchanged
    /// rebuild compares without cloning a nested JSON value just to discard it.
    pub fn set_member_state_if_changed_ref(
        &mut self,
        name: &str,
        value: &Value,
    ) -> Result<bool, ScriptError> {
        if self
            .state
            .get_ref(name)
            .is_some_and(|current| current == value)
        {
            return Ok(false);
        }
        self.set_member_state(name, value.clone())?;
        Ok(true)
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
        let mut names = self.state.keys();
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
}
