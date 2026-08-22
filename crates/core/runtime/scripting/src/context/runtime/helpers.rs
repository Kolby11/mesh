use super::super::lookup::lua_value_to_string;
use super::super::proxy::create_event_channel;
use mesh_core_locale::{CatalogEntry, LocalizedTextResolution};
use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue, Variadic};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) fn is_lifecycle_handler(name: &str) -> bool {
    matches!(name, "init" | "render" | "mount" | "unmount" | "onRender")
}

pub(super) fn is_proxyable_lua_scalar(value: &LuaValue) -> bool {
    matches!(
        value,
        LuaValue::Boolean(_) | LuaValue::Integer(_) | LuaValue::Number(_) | LuaValue::String(_)
    )
}

pub(super) fn lua_scalar_matches_json(lua_value: &LuaValue, json_value: &Value) -> bool {
    match (lua_value, json_value) {
        (LuaValue::Boolean(left), Value::Bool(right)) => left == right,
        (LuaValue::Integer(left), Value::Number(right)) => right.as_i64() == Some(*left),
        (LuaValue::Number(left), Value::Number(right)) => {
            right.as_f64().is_some_and(|right| left == &right)
        }
        (LuaValue::String(left), Value::String(right)) => left
            .to_str()
            .ok()
            .is_some_and(|left| left.as_ref() == right),
        _ => false,
    }
}

pub(super) fn is_reserved_runtime_hook(name: &str) -> bool {
    is_lifecycle_handler(name)
}

/// Gate for the live `bind:this` proxy: hide host internals so only the child's
/// public values and functions cross the boundary. `denylist` is the child's
/// `builtin_globals` (`self`, `module`, `mesh`, `require`, the `__mesh_*`
/// sentinels installed before user script execution).
pub(super) fn is_denied_binding_key(key: &str, denylist: &HashSet<String>) -> bool {
    key.starts_with("__") || is_reserved_runtime_hook(key) || denylist.contains(key)
}

/// Resolve (or lazily create) a `self.<Event>` channel.
///
/// The registry lives on the per-instance `_ENV` table (`scope`) so two
/// instances of the same component keep independent `self` channels when they
/// share one thread VM.
pub(super) fn self_event_channel(
    lua: &Lua,
    scope: &Table,
    event_name: &str,
) -> mlua::Result<Table> {
    let registry = match scope.raw_get::<LuaValue>("__mesh_self_event_channels")? {
        LuaValue::Table(table) => table,
        _ => {
            let table = lua.create_table()?;
            scope.raw_set("__mesh_self_event_channels", table.clone())?;
            table
        }
    };
    match registry.raw_get::<LuaValue>(event_name)? {
        LuaValue::Table(channel) => Ok(channel),
        _ => {
            let channel = create_event_channel(lua, None, None)?;
            registry.raw_set(event_name, channel.clone())?;
            Ok(channel)
        }
    }
}

/// Wrap a child's `self.<Event>` channel for a parent holding a live
/// `bind:this` reference to that child.
///
/// The parent subscribes with a closure over its own `_ENV` and the child fires
/// it synchronously in the shared VM, so the parent's Lua state changes without
/// the shell dispatching a handler and its Rust-side state would stay stale.
/// Wrapping each registered callback flags the *parent* as externally accessed
/// when the callback runs, which is what the post-handler neighbour resync
/// keys on — the mirror of the wrapper in the proxy's `__index`, which flags
/// the *child*.
///
/// Everything but `on`/`subscribe` falls through to the real channel, so both
/// sides share one subscriber list. Wrappers are memoized per event name.
pub(super) fn parent_subscription_channel(
    lua: &Lua,
    channel: &Table,
    wrappers: &Table,
    event_name: &str,
    parent_accessed: &Arc<AtomicBool>,
) -> mlua::Result<Table> {
    if let LuaValue::Table(existing) = wrappers.raw_get::<LuaValue>(event_name)? {
        return Ok(existing);
    }

    let wrapper = lua.create_table()?;
    let meta = lua.create_table()?;
    meta.set("__index", channel.clone())?;
    wrapper.set_metatable(Some(meta))?;

    let target = channel.clone();
    let accessed = Arc::clone(parent_accessed);
    let subscribe = lua.create_function(move |lua, (_wrapper, callback): (Table, Function)| {
        let accessed = Arc::clone(&accessed);
        let tracked = lua.create_function(move |_lua, args: Variadic<LuaValue>| {
            accessed.store(true, Ordering::Release);
            callback.call::<MultiValue>(args)
        })?;
        target
            .get::<Function>("subscribe")?
            .call::<LuaValue>((target.clone(), tracked))
    })?;
    wrapper.set("subscribe", subscribe.clone())?;
    wrapper.set("on", subscribe)?;

    wrappers.raw_set(event_name, wrapper.clone())?;
    Ok(wrapper)
}

pub(super) fn create_i18n_library(
    lua: &Lua,
    translations: Arc<Mutex<HashMap<String, CatalogEntry>>>,
    locale: Arc<Mutex<String>>,
    snapshot_revision: Arc<Mutex<u64>>,
    owner_module_id: String,
    localized_misses: Arc<Mutex<Vec<LocalizedTextResolution>>>,
) -> mlua::Result<Table> {
    let exports = lua.create_table()?;
    exports.set(
        "t",
        lua.create_function(move |_lua, (key, values): (LuaValue, Option<Table>)| {
            let key = match key {
                LuaValue::String(value) => value.to_str()?.to_string(),
                other => lua_value_to_string(other),
            };
            let mut args = HashMap::new();
            if let Some(values) = values {
                for pair in values.pairs::<String, LuaValue>() {
                    let (name, value) = pair?;
                    args.insert(name, lua_value_to_string(value));
                }
            }
            let locale = locale.lock().unwrap().clone();
            let translated = translations
                .lock()
                .unwrap()
                .get(&key)
                .and_then(|entry| entry.render(&locale, &args))
                .unwrap_or_else(|| {
                    let resolution = LocalizedTextResolution::missing(
                        owner_module_id.clone(),
                        key.clone(),
                        None,
                        *snapshot_revision.lock().unwrap(),
                    );
                    localized_misses.lock().unwrap().push(resolution.clone());
                    resolution.text
                });
            Ok(translated)
        })?,
    )?;
    Ok(exports)
}

pub(super) fn resolve_host_api(mesh: &Table, module: &str) -> mlua::Result<Option<Table>> {
    if module.contains('@') {
        return Ok(None);
    }
    let Some(api_name) = module.strip_prefix("mesh.") else {
        return Ok(None);
    };
    match api_name {
        "events" | "ui" | "log" | "popover" | "locale" => mesh.get(api_name).map(Some),
        _ => Ok(None),
    }
}

pub(super) fn is_component_definition_specifier(module: &str) -> bool {
    module.ends_with(".mesh")
        || module.starts_with("./")
        || module.starts_with("../")
        || (module.starts_with("@") && !module[1..].contains('@'))
}
