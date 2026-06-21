use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue, Variadic};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// A queued imperative action against a live element reference, e.g.
/// `refs.search_input:focus()`. The shell drains these after a script handler
/// runs and applies them to interaction state (focus, scroll, …) on the real
/// retained widget tree.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementAction {
    /// The `refs.<name>` target the script addressed (an element's `ref`/`id`).
    pub target: String,
    /// The method invoked, one of [`ELEMENT_METHODS`].
    pub action: String,
    /// Positional method arguments serialized from Lua, in call order, with the
    /// implicit `self` table (from `:` method calls) stripped. A JSON array;
    /// `focus`/`blur`/`scroll_into_view` ignore it, `scroll_to(top[, left])`
    /// reads `[0]`/`[1]`.
    pub args: Value,
    /// An optional trailing options table (e.g. `{ smooth = true, duration = 300 }`)
    /// serialized as a JSON object, or `Null` when none was passed. Lets methods
    /// take DOM-style behavior options without colliding with positional args.
    pub options: Value,
}

/// Imperative methods exposed on a live element-node proxy. Anything not in this
/// list is treated as a live geometry/state field read from the latest paint.
pub(super) const ELEMENT_METHODS: &[&str] =
    &["focus", "blur", "scroll_into_view", "scroll_to", "click"];

/// Build the `refs` proxy. `refs.<name>` returns a live element-node proxy whose
/// geometry/state fields read from the most recent paint (fed via
/// `__mesh_element_metrics` on the shared realm) and whose methods enqueue
/// [`ElementAction`]s for the shell to execute against the real widget tree.
///
/// `scope` is the per-component `_ENV`; its metatable falls through to globals,
/// where the shell publishes `__mesh_element_metrics` once per surface paint, so
/// every component in the surface sees the same live element table.
pub(super) fn create_refs_proxy(
    lua: &Lua,
    scope: &Table,
    actions: Arc<Mutex<Vec<ElementAction>>>,
) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    let meta = lua.create_table()?;
    let scope = scope.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (_proxy, name): (Table, String)| {
            create_element_node_proxy(lua, &scope, &name, Arc::clone(&actions)).map(LuaValue::Table)
        })?,
    )?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

fn create_element_node_proxy(
    lua: &Lua,
    scope: &Table,
    name: &str,
    actions: Arc<Mutex<Vec<ElementAction>>>,
) -> mlua::Result<Table> {
    let node = lua.create_table()?;
    // Tag the proxy so imperative methods can recognize the implicit `self` from
    // `obj:method(...)` calls and strip it, while keeping a real options table as
    // an argument. Set raw so it bypasses the `__index` geometry lookup below.
    node.raw_set("__mesh_is_element_ref", true)?;
    let meta = lua.create_table()?;
    let scope = scope.clone();
    let name_owned = name.to_string();
    meta.set(
        "__index",
        lua.create_function(move |lua, (_node, key): (Table, String)| {
            if key == "name" {
                return Ok(LuaValue::String(lua.create_string(&name_owned)?));
            }
            // `present`/`exists` let scripts guard against an element that is not
            // in the current tree (conditionally rendered or not yet painted).
            if key == "present" || key == "exists" {
                let present = element_metrics_entry(&scope, &name_owned)?.is_some();
                return Ok(LuaValue::Boolean(present));
            }
            if ELEMENT_METHODS.contains(&key.as_str()) {
                let actions = Arc::clone(&actions);
                let target = name_owned.clone();
                let action = key.clone();
                return Ok(LuaValue::Function(lua.create_function(
                    move |lua, args: Variadic<LuaValue>| {
                        // Separate args into positional values and an options table.
                        // A `:` method-call passes the node proxy as `self` (a table
                        // tagged with `__mesh_is_element_ref`), which we skip; any
                        // other table is a DOM-style options bag
                        // (e.g. `{ smooth = true }`); the rest (numbers) are
                        // positional, forwarded as a JSON array.
                        let mut positional = Vec::new();
                        let mut options = Value::Null;
                        for value in args.iter() {
                            match value {
                                LuaValue::Table(table) => {
                                    let is_self = table
                                        .raw_get::<Option<bool>>("__mesh_is_element_ref")?
                                        .unwrap_or(false);
                                    if !is_self {
                                        options = lua.from_value::<Value>(value.clone())?;
                                    }
                                }
                                other => {
                                    positional.push(lua.from_value::<Value>(other.clone())?);
                                }
                            }
                        }
                        actions.lock().unwrap().push(ElementAction {
                            target: target.clone(),
                            action: action.clone(),
                            args: Value::Array(positional),
                            options,
                        });
                        Ok(())
                    },
                )?));
            }
            // Live geometry/state field: read from the latest published metrics.
            match element_metrics_entry(&scope, &name_owned)? {
                Some(metrics) => metrics.get::<LuaValue>(key),
                None => Ok(LuaValue::Nil),
            }
        })?,
    )?;
    node.set_metatable(Some(meta))?;
    Ok(node)
}

/// Resolve the latest metrics table for `name` from the surface-wide
/// `__mesh_element_metrics` table (read through `_ENV.__index -> globals`).
fn element_metrics_entry(scope: &Table, name: &str) -> mlua::Result<Option<Table>> {
    let LuaValue::Table(all) = scope.get::<LuaValue>("__mesh_element_metrics")? else {
        return Ok(None);
    };
    match all.get::<LuaValue>(name)? {
        LuaValue::Table(entry) => Ok(Some(entry)),
        _ => Ok(None),
    }
}
