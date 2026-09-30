//! `props.source(name)` and `props.at(name, scope)`: read user intent beneath
//! the effective value.
//!
//! The host resolves every declared prop through the precedence ladder
//! (default → global → instance → per-instance) and publishes the winner as
//! `props.<name>`. Scripts may then assign over it. These helpers let a script
//! see which layer won and what each layer holds, so it can honor, clamp, or
//! temporarily override a saved preference without losing it.
//!
//! The layer snapshot is Rust-owned. The helpers are served from an `__index`
//! metatable on the `props` table, which the host re-attaches every time it
//! republishes that table; declared props cannot be named `source` or `at`.

use super::super::ScriptError;
use super::super::lookup::{lua_err, map_lua_error};
use super::*;
use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// Scopes `props.at` accepts, lowest precedence first.
const PROP_SCOPES: [&str; 5] = ["default", "global", "instance", "per_instance", "script"];

impl ScriptContext {
    /// Publish the host-resolved layers behind `props.source`/`props.at`.
    ///
    /// `layers` maps each declared prop name to
    /// `{ "layers": { <scope>: raw value, .. }, "winner": scope | null,
    /// "effective": value | null }`, where `effective` is the value the host
    /// published before any script assignment.
    pub fn set_prop_layers(&mut self, layers: Value) -> Result<(), ScriptError> {
        *self.prop_layers.lock().unwrap() = layers;
        self.attach_prop_introspection()
    }

    /// Re-attach the helpers to the current `props` table. Host publication
    /// replaces the table, so every publish path calls this.
    pub(super) fn attach_prop_introspection(&mut self) -> Result<(), ScriptError> {
        self.ensure_initialized()?;
        let LuaValue::Table(props) = self.env().get::<LuaValue>("props").map_err(map_lua_error)?
        else {
            return Ok(());
        };
        install(self.lua(), &props, Arc::clone(&self.prop_layers)).map_err(lua_err)
    }
}

fn install(lua: &Lua, props: &Table, layers: Arc<Mutex<Value>>) -> mlua::Result<()> {
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(move |lua, (table, key): (Table, String)| {
            let layers = Arc::clone(&layers);
            let helper = match key.as_str() {
                "source" => lua.create_function(move |lua, name: String| {
                    source(lua, &table, &layers.lock().unwrap(), &name)
                })?,
                "at" => lua.create_function(move |lua, (name, scope): (String, String)| {
                    at(lua, &table, &layers.lock().unwrap(), &name, &scope)
                })?,
                _ => return Ok(LuaValue::Nil),
            };
            Ok(LuaValue::Function(helper))
        })?,
    )?;
    props.set_metatable(Some(meta))?;
    Ok(())
}

/// The winning layer for `name`: `"script"` when a script assignment differs
/// from the host-resolved value, otherwise the host layer that won. `nil`
/// for an undeclared prop or one no layer sets.
fn source(lua: &Lua, props: &Table, layers: &Value, name: &str) -> mlua::Result<LuaValue> {
    let Some(entry) = layers.get(name) else {
        return Ok(LuaValue::Nil);
    };
    if script_override(lua, props, entry, name)?.is_some() {
        return Ok(LuaValue::String(lua.create_string("script")?));
    }
    match entry.get("winner").and_then(Value::as_str) {
        Some(winner) => Ok(LuaValue::String(lua.create_string(winner)?)),
        None => Ok(LuaValue::Nil),
    }
}

/// The raw value `name` holds at `scope`, or `nil` when that layer is unset.
fn at(lua: &Lua, props: &Table, layers: &Value, name: &str, scope: &str) -> mlua::Result<LuaValue> {
    if !PROP_SCOPES.contains(&scope) {
        return Err(mlua::Error::runtime(format!(
            "props.at: unknown scope '{scope}'; expected one of {}",
            PROP_SCOPES.join(", ")
        )));
    }
    let Some(entry) = layers.get(name) else {
        return Ok(LuaValue::Nil);
    };
    let value = if scope == "script" {
        script_override(lua, props, entry, name)?
    } else {
        entry
            .get("layers")
            .and_then(|layers| layers.get(scope))
            .cloned()
    };
    match value {
        Some(value) => lua.to_value(&value),
        None => Ok(LuaValue::Nil),
    }
}

/// The script-assigned value of `name`, if it differs from the host's.
fn script_override(
    lua: &Lua,
    props: &Table,
    entry: &Value,
    name: &str,
) -> mlua::Result<Option<Value>> {
    let current = props.raw_get::<LuaValue>(name)?;
    let current = match current {
        LuaValue::Nil => Value::Null,
        other => lua.from_value::<Value>(other)?,
    };
    let host = entry.get("effective").unwrap_or(&Value::Null);
    Ok((!same_value(&current, host)).then_some(current))
}

/// JSON equality that treats `12` and `12.0` alike: Luau numbers round-trip
/// as floats, while host values may be integers.
fn same_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        _ => left == right,
    }
}
