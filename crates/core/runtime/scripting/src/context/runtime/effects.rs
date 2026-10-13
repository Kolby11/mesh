use super::super::lookup::{lua_err, map_lua_error};
use super::*;
use crate::ScriptError;
use mlua::{Function, RegistryKey, Table, Value as LuaValue};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

const MAX_EFFECTS: usize = 128;
const MAX_SETTLE_ROUNDS: usize = 8;

#[derive(Debug, Default)]
pub(super) struct EffectQueue {
    next_id: u64,
    active: usize,
    disposing: bool,
    entries: BTreeMap<u64, ReactiveEffect>,
}

#[derive(Debug)]
struct ReactiveEffect {
    callback: RegistryKey,
    cleanup: Arc<Mutex<Option<RegistryKey>>>,
    cancelled: Arc<AtomicBool>,
    inputs: Option<EffectInputs>,
}

#[derive(Debug)]
struct EffectInputs {
    generation: u64,
    revision: u64,
    public: HashMap<String, Option<Arc<serde_json::Value>>>,
    services: HashMap<String, (Option<Arc<serde_json::Value>>, HashSet<String>)>,
}

impl ScriptContext {
    pub(super) fn install_effect_api(&mut self, globals: &Table) -> Result<(), ScriptError> {
        let queue = Arc::clone(&self.effects);
        globals
            .set(
                "effect",
                self.lua()
                    .create_function(move |lua, callback: Function| {
                        let cleanup = Arc::new(Mutex::new(None::<RegistryKey>));
                        let cancelled = Arc::new(AtomicBool::new(false));
                        let mut queue = queue.lock().unwrap();
                        if queue.disposing {
                            return Err(mlua::Error::runtime(
                                "cannot register an effect during teardown",
                            ));
                        }
                        if queue.entries.len() + queue.active >= MAX_EFFECTS {
                            return Err(mlua::Error::runtime(
                                "too many reactive effects (limit 128)",
                            ));
                        }
                        queue.next_id += 1;
                        let id = queue.next_id;
                        queue.entries.insert(
                            id,
                            ReactiveEffect {
                                callback: lua.create_registry_value(callback)?,
                                cleanup: Arc::clone(&cleanup),
                                cancelled: Arc::clone(&cancelled),
                                inputs: None,
                            },
                        );
                        drop(queue);
                        let handle = lua.create_table()?;
                        handle.set(
                            "cancel",
                            lua.create_function(move |lua, _: mlua::MultiValue| {
                                if cancelled.swap(true, Ordering::AcqRel) {
                                    return Ok(false);
                                }
                                let previous = cleanup.lock().unwrap().take();
                                if let Some(key) = previous {
                                    lua.registry_value::<Function>(&key)?.call::<()>(())?;
                                }
                                Ok(true)
                            })?,
                        )?;
                        Ok(handle)
                    })
                    .map_err(lua_err)?,
            )
            .map_err(lua_err)
    }

    pub fn has_reactive_effects(&self) -> bool {
        !self.effects.lock().unwrap().entries.is_empty()
    }

    pub fn invalidate_reactive_effects(&self) {
        self.effect_revision.fetch_add(1, Ordering::Release);
    }

    pub fn reactive_effects_need_run(&self) -> bool {
        self.effects
            .lock()
            .unwrap()
            .entries
            .values()
            .any(|effect| self.effect_due(effect))
    }

    fn effect_inputs(
        &self,
        reads: HashMap<String, HashSet<String>>,
        mut members: HashSet<String>,
    ) -> EffectInputs {
        members.extend(
            self.user_global_keys
                .iter()
                .filter(|key| !self.proxied_scalar_global_keys.contains(*key))
                .cloned(),
        );
        members.extend(["props".into(), "settings".into()]);
        let services = self.service_context_state.lock().unwrap();
        EffectInputs {
            generation: self.state.mutation_generation(),
            revision: self.effect_revision.load(Ordering::Acquire),
            public: members
                .into_iter()
                .map(|key| {
                    let value = self.state.variables.get(&key).cloned();
                    (key, value)
                })
                .collect(),
            services: reads
                .into_iter()
                .map(|(name, fields)| {
                    let payload = services.payload(&name);
                    (name, (payload, fields))
                })
                .collect(),
        }
    }

    fn effect_due(&self, effect: &ReactiveEffect) -> bool {
        if effect.cancelled.load(Ordering::Acquire) {
            return false;
        }
        let Some(inputs) = &effect.inputs else {
            return true;
        };
        if inputs.revision != self.effect_revision.load(Ordering::Acquire) {
            return true;
        }
        if inputs.generation != self.state.mutation_generation()
            && inputs.public.iter().any(|(key, previous)| {
                match (previous, self.state.variables.get(key)) {
                    (Some(previous), Some(next)) => {
                        !Arc::ptr_eq(previous, next) && !same_input(previous, next)
                    }
                    (None, None) => false,
                    _ => true,
                }
            })
        {
            return true;
        }
        let services = self.service_context_state.lock().unwrap();
        inputs.services.iter().any(|(name, (previous, fields))| {
            let next = services.payload(name);
            if let (Some(previous), Some(next)) = (previous, &next)
                && Arc::ptr_eq(previous, next)
            {
                return false;
            }
            fields.iter().any(|field| {
                previous.as_ref().and_then(|value| value.get(field))
                    != next.as_ref().and_then(|value| value.get(field))
            })
        })
    }

    /// Run effects whose observed service fields or component inputs changed.
    /// Each resumption has a script budget. Cleanup runs before replacement;
    /// independent effects continue after a callback failure.
    pub fn run_reactive_effects(&mut self) -> Vec<ScriptError> {
        if !self.has_reactive_effects() {
            return Vec::new();
        }
        let mut errors = Vec::new();
        if let Err(error) = self.ensure_initialized() {
            return vec![error];
        }
        for _ in 0..MAX_SETTLE_ROUNDS {
            let mut entries = {
                let mut queue = self.effects.lock().unwrap();
                let entries = std::mem::take(&mut queue.entries);
                queue.active = entries.len();
                entries
            };
            let mut ran = false;
            for effect in entries.values_mut() {
                if !self.effect_due(effect) {
                    if let Some(inputs) = &mut effect.inputs {
                        inputs.generation = self.state.mutation_generation();
                        let services = self.service_context_state.lock().unwrap();
                        for (service, (snapshot, _)) in &mut inputs.services {
                            *snapshot = services.payload(service);
                        }
                    }
                    continue;
                }
                ran = true;
                let previous = effect.cleanup.lock().unwrap().take();
                if let Some(key) = previous {
                    let _budget = self.realm_policy.begin_callback();
                    if let Err(error) = self
                        .lua()
                        .registry_value::<Function>(&key)
                        .and_then(|cleanup| cleanup.call::<()>(()))
                    {
                        errors.push(map_lua_error(error));
                    }
                }
                let previous_reads =
                    std::mem::take(&mut *self.tracked_service_fields.lock().unwrap());
                *self.effect_member_reads.lock().unwrap() = Some(HashSet::new());
                self.tracking_effect_members.store(true, Ordering::Release);
                self.tracking_storage_reads.store(true, Ordering::Release);
                let result = (|| {
                    let _budget = self.realm_policy.begin_callback();
                    let callback: Function = self
                        .lua()
                        .registry_value(&effect.callback)
                        .map_err(lua_err)?;
                    callback
                        .call::<LuaValue>(self.current_self_table()?)
                        .map_err(map_lua_error)
                })();
                self.tracking_storage_reads.store(false, Ordering::Release);
                let members = self
                    .effect_member_reads
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap_or_default();
                let reads = std::mem::take(&mut *self.tracked_service_fields.lock().unwrap());
                {
                    let mut tracked = self.tracked_service_fields.lock().unwrap();
                    for (service, fields) in previous_reads {
                        tracked.entry(service).or_default().extend(fields);
                    }
                    for (service, fields) in &reads {
                        tracked
                            .entry(service.clone())
                            .or_default()
                            .extend(fields.iter().cloned());
                    }
                }
                self.sync_state_from_lua();
                self.sync_side_channels();
                self.tracking_effect_members.store(false, Ordering::Release);
                effect.inputs = Some(self.effect_inputs(reads, members));
                match result {
                    Ok(LuaValue::Function(cleanup)) if effect.cancelled.load(Ordering::Acquire) => {
                        let _budget = self.realm_policy.begin_callback();
                        if let Err(error) = cleanup.call::<()>(()) {
                            errors.push(map_lua_error(error));
                        }
                        self.sync_state_from_lua();
                        self.sync_side_channels();
                    }
                    Ok(LuaValue::Function(cleanup)) => {
                        match self.lua().create_registry_value(cleanup) {
                            Ok(key) => *effect.cleanup.lock().unwrap() = Some(key),
                            Err(error) => errors.push(lua_err(error)),
                        }
                    }
                    Ok(LuaValue::Nil) => {}
                    Ok(_) => errors.push(ScriptError::LuaError(
                        "effect must return a cleanup function or nil".into(),
                    )),
                    Err(error) => errors.push(error),
                }
            }
            entries.retain(|_, effect| !effect.cancelled.load(Ordering::Acquire));
            let mut queue = self.effects.lock().unwrap();
            queue.active = 0;
            queue.entries.extend(entries);
            if !ran {
                return errors;
            }
        }
        if self
            .effects
            .lock()
            .unwrap()
            .entries
            .values()
            .any(|effect| self.effect_due(effect))
        {
            errors.push(ScriptError::LuaError(
                "reactive effects did not settle within eight rounds".into(),
            ));
        }
        errors
    }

    pub(super) fn clear_reactive_effects(&mut self) {
        let entries = std::mem::take(&mut self.effects.lock().unwrap().entries);
        if entries.is_empty() {
            return;
        }
        self.effects.lock().unwrap().disposing = true;
        for effect in entries.into_values() {
            effect.cancelled.store(true, Ordering::Release);
            let cleanup = effect.cleanup.lock().unwrap().take();
            if let Some(key) = cleanup
                && self.vm.is_some()
            {
                let _budget = self.realm_policy.begin_callback();
                if let Err(error) = self
                    .lua()
                    .registry_value::<Function>(&key)
                    .and_then(|cleanup| cleanup.call::<()>(()))
                {
                    tracing::warn!(module = %self.module_id, %error, "reactive effect cleanup failed");
                }
            }
        }
        {
            let mut queue = self.effects.lock().unwrap();
            queue.entries.clear();
            queue.active = 0;
            queue.disposing = false;
        }
        if self.vm.is_some() && self.env_table.is_some() {
            self.sync_state_from_lua();
            self.sync_side_channels();
        }
    }
}

// Host integers round-trip through Luau as floats, including inside props.
fn same_input(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a == b || a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_input(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| same_input(a, b)))
        }
        _ => left == right,
    }
}
