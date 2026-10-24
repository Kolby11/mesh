use super::super::{
    ScriptError,
    lookup::{lua_err, map_lua_error},
};
use super::*;
use mesh_core_elements::{CallbackHandle, HandlerTarget};
use mesh_core_expression::CompiledExpression;
use mlua::{Function, LuaSerdeExt, MultiValue, Table, Value as LuaValue};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

const MAX_CALLBACK_BINDINGS: usize = 4096;
static NEXT_CALLBACK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(super) struct BoundCallback {
    id: u64,
    function: Function,
    scope: Weak<()>,
}

type BindingKey = (String, String);
#[derive(Debug, Default)]
pub(super) struct CallbackCache {
    functions: HashMap<(BindingKey, usize), Weak<BoundCallback>>,
    watches: HashMap<BindingKey, CallbackWatch>,
}
#[derive(Debug)]
struct CallbackWatch {
    expression: CompiledExpression,
    locals: Map<String, Value>,
    pointer: usize,
    lease: Option<Weak<BoundCallback>>,
}

impl ScriptContext {
    /// Bind a real function in the component's lexical scope and capture the
    /// rendered iteration's locals. No function value crosses JSON.
    pub fn bind_handler_expression(
        &self,
        expression: &CompiledExpression,
        locals: &Map<String, Value>,
        source: &str,
    ) -> Result<HandlerTarget, ScriptError> {
        self.bind_event_expression(expression, locals, source, false)?
            .ok_or_else(|| ScriptError::LuaError(format!("event binding `{source}` is disabled")))
    }

    pub fn bind_event_expression(
        &self,
        expression: &CompiledExpression,
        locals: &Map<String, Value>,
        source: &str,
        call_binding: bool,
    ) -> Result<Option<HandlerTarget>, ScriptError> {
        let _budget = self.realm_policy.begin_callback();
        let (value, local_values) = self.evaluate_handler_value(expression, locals)?;
        if matches!(value, LuaValue::Nil) {
            if expression.is_reference() {
                let key = (
                    expression.source().to_owned(),
                    serde_json::to_string(locals)
                        .map_err(|error| ScriptError::LuaError(error.to_string()))?,
                );
                let mut cache = self.callbacks.lock().unwrap();
                cache.watches.retain(|_, watch| {
                    watch
                        .lease
                        .as_ref()
                        .is_none_or(|lease| lease.strong_count() != 0)
                });
                if !cache.watches.contains_key(&key) && cache.watches.len() >= MAX_CALLBACK_BINDINGS
                {
                    return Err(ScriptError::LuaError(format!(
                        "component callback reference limit of {MAX_CALLBACK_BINDINGS} exceeded"
                    )));
                }
                cache.watches.insert(
                    key,
                    CallbackWatch {
                        expression: expression.clone(),
                        locals: locals.clone(),
                        pointer: 0,
                        lease: None,
                    },
                );
            }
            return Ok(None);
        }
        let LuaValue::Function(function) = value else {
            return Err(ScriptError::LuaError(format!(
                "event binding `{source}` must produce a Luau function"
            )));
        };
        let key = (
            (
                expression.source().to_owned(),
                serde_json::to_string(locals)
                    .map_err(|error| ScriptError::LuaError(error.to_string()))?,
            ),
            if call_binding {
                0
            } else {
                function.to_pointer() as usize
            },
        );
        let mut cache = self.callbacks.lock().unwrap();
        let watch_key = key.0.clone();
        let pointer = key.1;
        if let Some(callback) = cache.functions.get(&key).and_then(Weak::upgrade) {
            if expression.is_reference() {
                cache.watches.insert(
                    watch_key,
                    CallbackWatch {
                        expression: expression.clone(),
                        locals: locals.clone(),
                        pointer,
                        lease: Some(Arc::downgrade(&callback)),
                    },
                );
            }
            return Ok(Some(HandlerTarget::Function {
                instance_key: None,
                source: source.into(),
                callback: CallbackHandle::new(callback.id, callback),
            }));
        }
        if cache.functions.len() >= MAX_CALLBACK_BINDINGS {
            cache.functions.retain(|_, weak| weak.strong_count() != 0);
            cache.watches.retain(|_, watch| {
                watch
                    .lease
                    .as_ref()
                    .is_none_or(|lease| lease.strong_count() != 0)
            });
        }
        if cache.functions.len() >= MAX_CALLBACK_BINDINGS {
            return Err(ScriptError::LuaError(format!(
                "component callback binding limit of {MAX_CALLBACK_BINDINGS} exceeded"
            )));
        }
        let binders: Table = self
            .env()
            .get("__mesh_template_callback_binders")
            .map_err(map_lua_error)?;
        let binder: Function = binders.get(expression.source()).map_err(map_lua_error)?;
        let bound: Function = binder
            .call((function, local_values))
            .map_err(map_lua_error)?;
        let scope = Arc::downgrade(&self.callback_scope);
        let guard_scope = scope.clone();
        let accessed = self.live_binding_external_accessed.clone();
        let guard = self
            .lua()
            .create_function(move |_, ()| {
                if guard_scope.upgrade().is_none() {
                    return Err(mlua::Error::runtime(
                        "callback owner was reloaded or unmounted",
                    ));
                }
                accessed.store(true, Ordering::Release);
                Ok(())
            })
            .map_err(map_lua_error)?;
        let factory = match self
            .env()
            .raw_get::<Function>("__mesh_callback_guard_factory")
        {
            Ok(factory) => factory,
            Err(_) => {
                let factory: Function = self.lua().load("return function(guard, callback) return function(...) guard(); return callback(...) end end")
                    .set_environment(self.lua().create_table().map_err(map_lua_error)?)
                    .eval().map_err(map_lua_error)?;
                self.env()
                    .raw_set("__mesh_callback_guard_factory", factory.clone())
                    .map_err(map_lua_error)?;
                factory
            }
        };
        let function: Function = factory.call((guard, bound)).map_err(map_lua_error)?;
        let callback = Arc::new(BoundCallback {
            id: NEXT_CALLBACK_ID.fetch_add(1, Ordering::Relaxed),
            function,
            scope,
        });
        cache.functions.insert(key, Arc::downgrade(&callback));
        if expression.is_reference() {
            cache.watches.insert(
                watch_key,
                CallbackWatch {
                    expression: expression.clone(),
                    locals: locals.clone(),
                    pointer,
                    lease: Some(Arc::downgrade(&callback)),
                },
            );
        }
        Ok(Some(HandlerTarget::Function {
            instance_key: None,
            source: source.into(),
            callback: CallbackHandle::new(callback.id, callback),
        }))
    }

    fn evaluate_handler_value(
        &self,
        expression: &CompiledExpression,
        locals: &Map<String, Value>,
    ) -> Result<(LuaValue, LuaValue), ScriptError> {
        let closures: Table = self
            .env()
            .get("__mesh_template_expressions")
            .map_err(map_lua_error)?;
        let closure: Function = closures.get(expression.source()).map_err(map_lua_error)?;
        let local_values = self.lua().to_value(locals).map_err(lua_err)?;
        let previous = std::mem::take(&mut *self.tracked_service_fields.lock().unwrap());
        let result = closure
            .call::<LuaValue>(local_values.clone())
            .map_err(map_lua_error);
        let reads = {
            let mut tracked = self.tracked_service_fields.lock().unwrap();
            let reads = tracked.clone();
            for (service, fields) in previous {
                tracked.entry(service).or_default().extend(fields);
            }
            reads
        };
        let member_reads = self
            .env()
            .get::<Table>("__mesh_template_expression_member_reads")
            .ok()
            .and_then(|all| all.get::<Table>(expression.source()).ok())
            .map(|reads| {
                reads
                    .pairs::<String, bool>()
                    .filter_map(|entry| entry.ok().and_then(|(name, read)| read.then_some(name)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut cache = self.template_expression_cache.lock().unwrap();
        cache
            .template_member_reads
            .extend(member_reads.iter().cloned());
        cache
            .member_reads
            .insert(expression.source().to_owned(), member_reads);
        cache
            .service_reads
            .entry(expression.source().to_owned())
            .or_default()
            .extend(reads.into_iter().flat_map(|(service, fields)| {
                fields
                    .into_iter()
                    .map(move |field| (service.clone(), field))
            }));
        result.map(|value| (value, local_values))
    }

    pub(super) fn synchronize_callback_references(&mut self) {
        let watches = self
            .callbacks
            .lock()
            .unwrap()
            .watches
            .iter()
            .filter(|(_, watch)| {
                watch
                    .lease
                    .as_ref()
                    .is_none_or(|lease| lease.strong_count() != 0)
            })
            .map(|(key, watch)| {
                (
                    key.clone(),
                    watch.expression.clone(),
                    watch.locals.clone(),
                    watch.pointer,
                )
            })
            .collect::<Vec<_>>();
        if watches.is_empty() {
            return;
        }
        let _budget = self.realm_policy.begin_callback();
        let mut changed = false;
        for (key, expression, locals, previous) in watches {
            let pointer = match self.evaluate_handler_value(&expression, &locals) {
                Ok((LuaValue::Function(function), _)) => function.to_pointer() as usize,
                _ => 0,
            };
            if pointer != previous {
                if let Some(watch) = self.callbacks.lock().unwrap().watches.get_mut(&key) {
                    watch.pointer = pointer;
                }
                changed = true;
            }
        }
        if changed {
            self.state.invalidate_function_binding();
            self.changed_public_members.clear();
            self.changed_list_items.clear();
        }
    }

    pub fn call_callback(
        &mut self,
        callback: &CallbackHandle,
        args: &[Value],
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let bound = callback
            .downcast_ref::<BoundCallback>()
            .ok_or_else(|| ScriptError::LuaError("invalid function callback handle".into()))?;
        if !bound.scope.ptr_eq(&Arc::downgrade(&self.callback_scope))
            || bound.scope.upgrade().is_none()
        {
            return Err(ScriptError::LuaError(
                "callback belongs to a retired or different component environment".into(),
            ));
        }
        let _budget = self.realm_policy.begin_callback();
        let mut values = MultiValue::new();
        for value in args {
            values.push_back(self.lua().to_value(value).map_err(lua_err)?);
        }
        bound.function.call::<()>(values).map_err(map_lua_error)?;
        self.sync_state_from_lua();
        self.sync_side_channels();
        self.live_binding_external_accessed
            .store(false, Ordering::Release);
        Ok(())
    }

    pub fn set_initial_callback_props(
        &mut self,
        callbacks: BTreeMap<mesh_core_elements::AttrKey, HandlerTarget>,
    ) {
        self.initial_callback_props = callbacks;
    }

    /// Install function-valued public inputs, restoring authored bindings when
    /// an input is removed. Guards and Lua upvalues retain the defining scope.
    pub fn set_callback_props(
        &mut self,
        callbacks: &BTreeMap<mesh_core_elements::AttrKey, HandlerTarget>,
    ) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let removed = self
            .callback_props
            .keys()
            .filter(|key| !callbacks.contains_key(key.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for name in removed {
            if let Some((_, original)) = self.callback_props.remove(&name) {
                self.env().set(name, original).map_err(map_lua_error)?;
            }
        }
        for (name, target) in callbacks {
            let HandlerTarget::Function { callback, .. } = target else {
                return Err(ScriptError::LuaError(
                    "component callback props require function values".into(),
                ));
            };
            let bound = callback
                .downcast_ref::<BoundCallback>()
                .ok_or_else(|| ScriptError::LuaError("invalid component callback handle".into()))?;
            if bound.scope.upgrade().is_none() {
                return Err(ScriptError::LuaError(
                    "component callback owner is no longer mounted".into(),
                ));
            }
            let current = self
                .env()
                .get::<LuaValue>(name.as_str())
                .map_err(map_lua_error)?;
            if self
                .callback_props
                .get(name.as_str())
                .is_some_and(|(id, _)| *id == bound.id)
                && matches!(&current, LuaValue::Function(function) if function.to_pointer() == bound.function.to_pointer())
            {
                continue;
            }
            let original = match self.callback_props.remove(name.as_str()) {
                Some((id, original)) if id != bound.id => original,
                _ => current,
            };
            self.env()
                .set(name.as_str(), bound.function.clone())
                .map_err(map_lua_error)?;
            self.callback_props
                .insert(name.as_str().into(), (bound.id, original));
        }
        Ok(())
    }
}
