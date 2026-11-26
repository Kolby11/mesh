use super::super::lookup::lua_value_to_string;
use super::super::proxy::create_event_channel;
use super::LocaleCell;
use mesh_core_locale::LocalizedTextResolution;
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

/// Wraps a value read through a live `bind:this` proxy. Functions become
/// forwarding closures and tables become read-through proxies, recursively,
/// so the parent never holds the child's own function or table values. Every
/// call or write through a wrapper runs `mark`, which flags the child for a
/// state resync. Runs in an empty environment; its globals arrive as arguments.
const LIVE_BINDING_WRAPPER: &str = r#"
local setmetatable, next, type, mark = ...
local wrap
wrap = function(value)
  local kind = type(value)
  if kind == "function" then
    return function(...)
      mark()
      return value(...)
    end
  elseif kind ~= "table" then
    return value
  end
  return setmetatable({}, {
    __index = function(_, key) return wrap(value[key]) end,
    __newindex = function(_, key, next_value)
      mark()
      value[key] = next_value
    end,
    __len = function() return #value end,
    __iter = function()
      local key
      return function()
        local item
        key, item = next(value, key)
        if key == nil then return nil end
        return key, wrap(item)
      end
    end,
    __metatable = false,
  })
end
return wrap
"#;

/// Build the deep wrapper for one live binding; see [`LIVE_BINDING_WRAPPER`].
pub(super) fn live_binding_wrapper(lua: &Lua, accessed: Arc<AtomicBool>) -> mlua::Result<Function> {
    let globals = lua.globals();
    let mark = lua.create_function(move |_, ()| {
        accessed.store(true, Ordering::Release);
        Ok(())
    })?;
    lua.load(LIVE_BINDING_WRAPPER)
        .set_name("=mesh:live-binding")
        .set_environment(lua.create_table()?)
        .call((
            globals.get::<Function>("setmetatable")?,
            globals.get::<Function>("next")?,
            globals.get::<Function>("type")?,
            mark,
        ))
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
    locale_cell: Arc<Mutex<LocaleCell>>,
    localized_misses: Arc<Mutex<Vec<LocalizedTextResolution>>>,
) -> mlua::Result<Table> {
    let exports = lua.create_table()?;
    let empty_args = HashMap::new();
    exports.set(
        "t",
        lua.create_function(move |lua, (key, values): (LuaValue, Option<Table>)| {
            let string_key;
            let converted_key;
            let key = match key {
                LuaValue::String(ref value) => {
                    string_key = value.to_str()?;
                    string_key.as_ref()
                }
                other => {
                    converted_key = lua_value_to_string(other);
                    converted_key.as_str()
                }
            };
            let mut call_args;
            let args = if let Some(values) = values {
                call_args = HashMap::new();
                for pair in values.pairs::<String, LuaValue>() {
                    let (name, value) = pair?;
                    call_args.insert(name, lua_value_to_string(value));
                }
                &call_args
            } else {
                &empty_args
            };
            let resolver = Arc::clone(&locale_cell.lock().unwrap().resolver);
            if let Some(text) = resolver.translate_with(key, args) {
                return lua.create_string(text.as_bytes());
            }
            let miss = LocalizedTextResolution::missing(
                resolver.module_id(),
                key,
                None,
                resolver.snapshot_revision(),
            );
            let text = lua.create_string(&miss.text);
            localized_misses.lock().unwrap().push(miss);
            text
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocaleBoundState, ScriptState};
    use mesh_core_elements::VariableStore;
    use mesh_core_locale::{LocaleEngine, compile_catalog};
    use std::hint::black_box;
    use std::time::Instant;

    #[test]
    #[ignore = "release-only Luau translation workload"]
    fn settings_translation_hit_workload() {
        const MODULE: &str = "@mesh/settings";
        const LABELS: usize = 160;
        const BATCHES: usize = 1_500;
        let catalog: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../modules/frontend/settings/config/i18n/sk.json"
        )))
        .unwrap();
        let mut locale = LocaleEngine::new("sk");
        locale.load_module_catalog(MODULE, compile_catalog("sk", &catalog));
        let translator = locale.module_translator(MODULE);
        let cell = Arc::new(Mutex::new(LocaleCell {
            resolver: Arc::new(translator.owned()),
        }));
        let lua = Lua::new();
        let keys: Vec<_> = catalog
            .as_object()
            .unwrap()
            .keys()
            .take(LABELS)
            .map(String::as_str)
            .collect();
        let labels = lua.create_sequence_from(keys.iter().copied()).unwrap();
        assert_eq!(labels.raw_len(), LABELS);
        let misses = Arc::new(Mutex::new(Vec::new()));
        let current: Function = create_i18n_library(&lua, cell.clone(), misses.clone())
            .unwrap()
            .get("t")
            .unwrap();
        let previous = lua
            .create_function(move |_, (key, _): (LuaValue, Option<Table>)| {
                let key = match key {
                    LuaValue::String(value) => value.to_str()?.to_string(),
                    other => lua_value_to_string(other),
                };
                let resolution =
                    cell.lock()
                        .unwrap()
                        .resolver
                        .resolve_with(&key, &HashMap::new(), None);
                assert!(!resolution.missing);
                Ok(resolution.text)
            })
            .unwrap();
        let run = lua
            .load(
                "local t, labels, batches = ...\n\
                 local checksum = 0\n\
                 for _ = 1, batches do\n\
                     for _, key in ipairs(labels) do checksum += #t(key) end\n\
                 end\n\
                 return checksum",
            )
            .into_function()
            .unwrap();
        let luau = |translate: &Function| {
            run.call::<usize>((translate.clone(), labels.clone(), BATCHES))
                .unwrap()
        };
        eprintln!("{BATCHES} batches of {LABELS} Settings labels");
        compare_translation_paths(
            "luau_translation_hit_speedup",
            [&|| luau(&previous), &|| luau(&current)],
        );
        assert!(misses.lock().unwrap().is_empty());

        let state = ScriptState::new();
        let bound = LocaleBoundState::new(&state, translator.clone());
        let template = |translate: &dyn Fn(&str) -> String| {
            let mut checksum = 0;
            for _ in 0..BATCHES {
                for key in &keys {
                    checksum += translate(black_box(*key)).len();
                }
            }
            checksum
        };
        compare_translation_paths(
            "template_translation_hit_speedup",
            [
                &|| template(&|key| translator.resolve(key, None).text),
                &|| template(&|key| bound.translate(key).unwrap()),
            ],
        );
    }

    fn compare_translation_paths(metric: &str, lanes: [&dyn Fn() -> usize; 2]) {
        let mut samples = [Vec::new(), Vec::new()];
        let mut expected = None;
        for pair in 0..6 {
            for lane in [pair % 2, 1 - pair % 2] {
                let started = Instant::now();
                let checksum = lanes[lane]();
                let elapsed = started.elapsed();
                assert_eq!(checksum, *expected.get_or_insert(checksum));
                if pair > 0 {
                    samples[lane].push(elapsed);
                }
            }
        }
        for sample in &mut samples {
            sample.sort_unstable();
        }
        let [previous, current] = samples;
        let speedup = previous[2].as_secs_f64() / current[2].as_secs_f64();
        eprintln!("{metric}: prior {previous:?}; current {current:?}");
        println!("MESH_PERF metric={metric} value={speedup:.6}");
        assert!(speedup >= 1.25, "{metric} was {speedup:.2}x");
    }
}
