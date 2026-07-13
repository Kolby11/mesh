use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue, Variadic};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

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
pub(super) const ELEMENT_METHODS: &[&str] = &[
    "focus",
    "blur",
    "scroll_into_view",
    "scroll_to",
    "click",
    "set_value",
];

/// Tags whose `value` is editable text content (DOM `input.value`), so the proxy
/// reads/writes the live string rather than the snapshot's accessibility flag.
fn is_textual_value_tag(tag: &str) -> bool {
    matches!(tag, "input" | "textarea")
}

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
    pending_side_channels: Arc<AtomicBool>,
) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    let meta = lua.create_table()?;
    let scope = scope.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (proxy, name): (Table, String)| {
            let node = create_element_node_proxy(
                lua,
                &scope,
                &name,
                Arc::clone(&actions),
                Arc::clone(&pending_side_channels),
            )?;
            proxy.raw_set(name.as_str(), node.clone())?;
            Ok(LuaValue::Table(node))
        })?,
    )?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

pub(super) fn install_bound_element_proxies(
    lua: &Lua,
    scope: &Table,
    metrics: &Value,
    actions: Arc<Mutex<Vec<ElementAction>>>,
    pending_side_channels: Arc<AtomicBool>,
) -> mlua::Result<()> {
    let Some(entries) = metrics.as_object() else {
        return Ok(());
    };

    for (name, metrics) in entries {
        let binding = metrics
            .get("attributes")
            .and_then(Value::as_object)
            .and_then(|attributes| attributes.get("_mesh_bind_this"))
            .and_then(Value::as_str);
        if binding != Some(name.as_str()) {
            continue;
        }
        let proxy = create_element_node_proxy(
            lua,
            scope,
            name,
            Arc::clone(&actions),
            Arc::clone(&pending_side_channels),
        )?;
        scope.raw_set(name.as_str(), proxy)?;
    }

    Ok(())
}

fn create_element_node_proxy(
    lua: &Lua,
    scope: &Table,
    name: &str,
    actions: Arc<Mutex<Vec<ElementAction>>>,
    pending_side_channels: Arc<AtomicBool>,
) -> mlua::Result<Table> {
    let node = lua.create_table()?;
    // Tag the proxy so imperative methods can recognize the implicit `self` from
    // `obj:method(...)` calls and strip it, while keeping a real options table as
    // an argument. Set raw so it bypasses the `__index` geometry lookup below.
    node.raw_set("__mesh_is_element_ref", true)?;
    let meta = lua.create_table()?;
    let scope = scope.clone();
    let name_owned = name.to_string();
    let method_cache = lua.create_table()?;
    // Clones for the `__newindex` closure, since `__index` moves the originals.
    let write_actions = Arc::clone(&actions);
    let write_pending_side_channels = Arc::clone(&pending_side_channels);
    let write_name = name_owned.clone();
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
                let pending_side_channels = Arc::clone(&pending_side_channels);
                let target = name_owned.clone();
                let action = key.clone();
                if let LuaValue::Function(method) = method_cache.get::<LuaValue>(key.as_str())? {
                    return Ok(LuaValue::Function(method));
                }
                let method = lua.create_function(move |lua, args: Variadic<LuaValue>| {
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
                    pending_side_channels.store(true, Ordering::Release);
                    actions.lock().unwrap().push(ElementAction {
                        target: target.clone(),
                        action: action.clone(),
                        args: Value::Array(positional),
                        options,
                    });
                    Ok(())
                })?;
                method_cache.set(key.as_str(), method.clone())?;
                return Ok(LuaValue::Function(method));
            }
            let metrics = match element_metrics_entry(&scope, &name_owned)? {
                Some(metrics) => metrics,
                None => return Ok(LuaValue::Nil),
            };
            // `value` on an input-like element is the live text (DOM `input.value`),
            // read from the attributes map rather than the snapshot's a11y flag.
            if key == "value" && metrics_is_textual(&metrics)? {
                if let LuaValue::Table(attributes) = metrics.get::<LuaValue>("attributes")? {
                    return attributes.get::<LuaValue>("value");
                }
                return Ok(LuaValue::Nil);
            }
            if let LuaValue::Table(attributes) = metrics.get::<LuaValue>("attributes")? {
                if let Some(attribute_name) = element_member_attribute_name(&key) {
                    let value = attributes.get::<LuaValue>(attribute_name.as_str())?;
                    if !matches!(value, LuaValue::Nil) {
                        return Ok(value);
                    }
                }
            }
            // Live geometry/state field: read from the latest published metrics.
            metrics.get::<LuaValue>(key)
        })?,
    )?;
    // `refs.x.value = "..."` writes input text (DOM `input.value = ...`) by
    // queuing a `set_value` action; every other field is read-only.
    meta.set(
        "__newindex",
        lua.create_function(move |lua, (_node, key, value): (Table, String, LuaValue)| {
            if key != "value" {
                if let Some(attribute_name) = element_member_attribute_name(&key) {
                    let payload = lua.from_value::<Value>(value)?;
                    write_pending_side_channels.store(true, Ordering::Release);
                    write_actions.lock().unwrap().push(ElementAction {
                        target: write_name.clone(),
                        action: "set_attribute".to_string(),
                        args: Value::Array(vec![Value::String(attribute_name), payload]),
                        options: Value::Null,
                    });
                    return Ok(());
                }
                return Err(mlua::Error::RuntimeError(format!(
                    "element reference field '{key}' is read-only"
                )));
            }
            let payload = lua.from_value::<Value>(value)?;
            write_pending_side_channels.store(true, Ordering::Release);
            write_actions.lock().unwrap().push(ElementAction {
                target: write_name.clone(),
                action: "set_value".to_string(),
                args: Value::Array(vec![payload]),
                options: Value::Null,
            });
            Ok(())
        })?,
    )?;
    node.set_metatable(Some(meta))?;
    Ok(node)
}

fn element_member_attribute_name(key: &str) -> Option<String> {
    if key == "className" {
        return Some("class".to_string());
    }
    if matches!(
        key,
        "key"
            | "tag"
            | "element_type"
            | "x"
            | "y"
            | "left"
            | "top"
            | "right"
            | "bottom"
            | "width"
            | "height"
            | "client_width"
            | "client_height"
            | "bounding_client_rect"
            | "client_bound_rect"
            | "scroll_x"
            | "scroll_y"
            | "scroll_left"
            | "scroll_top"
            | "scroll_width"
            | "scroll_height"
            | "max_scroll_left"
            | "max_scroll_top"
            | "hovered"
            | "active"
            | "focused"
            | "disabled"
            | "checked"
            | "present"
            | "exists"
            | "attributes"
            | "name"
    ) {
        return None;
    }

    let mut result = String::new();
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            result.push('-');
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    Some(result)
}

/// Whether a metrics snapshot describes an input-like element (so `value` is
/// editable text). Reads the snapshot's `tag`.
fn metrics_is_textual(metrics: &Table) -> mlua::Result<bool> {
    match metrics.get::<LuaValue>("tag")? {
        LuaValue::String(tag) => Ok(is_textual_value_tag(&tag.to_str()?)),
        _ => Ok(false),
    }
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
