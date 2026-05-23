use super::PublishedEvent;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_service::{InterfaceContract, InterfaceResolution};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub(super) fn create_interface_proxy(
    lua: &Lua,
    resolution: InterfaceResolution,
    source_module_id: String,
    source_capabilities: CapabilitySet,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
) -> mlua::Result<Table> {
    create_service_proxy(
        lua,
        service_name_from_interface(&resolution.requested),
        resolution.contract,
        resolution.requested,
        source_module_id,
        source_capabilities,
        tracked_service_fields,
        published_events,
    )
}

pub(super) fn create_service_proxy(
    lua: &Lua,
    service_name: String,
    contract: Option<InterfaceContract>,
    interface_name: String,
    source_module_id: String,
    source_capabilities: CapabilitySet,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    let meta = lua.create_table()?;

    let methods = contract
        .as_ref()
        .map(|c| c.methods.clone())
        .unwrap_or_default();
    let interface_name = contract
        .as_ref()
        .map(|c| c.interface.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or(interface_name);
    let state_proxy = create_service_state_proxy(
        lua,
        service_name.clone(),
        Arc::clone(&tracked_service_fields),
    )?;
    proxy.set("state", state_proxy)?;
    let events_proxy = create_events_proxy(
        lua,
        contract
            .as_ref()
            .map(|contract| {
                contract
                    .events
                    .iter()
                    .map(|event| event.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    )?;
    proxy.set("events", events_proxy)?;

    meta.set(
        "__index",
        lua.create_function(move |lua, (table, key): (Table, String)| {
            if key == "state" || key == "events" {
                return table.get::<LuaValue>(key);
            }
            // Case A: known contract method — dispatch as a service command.
            if let Some(method) = methods.iter().find(|m| m.name == key) {
                let required_capability = service_control_capability(&service_name);
                let method = method.clone();
                let iface = interface_name.clone();
                let events = Arc::clone(&published_events);
                let source_module_id = source_module_id.clone();
                let source_capabilities = source_capabilities.clone();
                return Ok(LuaValue::Function(lua.create_function(
                    move |lua, args: mlua::Variadic<LuaValue>| {
                        if !source_capabilities.is_granted(&required_capability) {
                            return command_result_table(
                                lua,
                                false,
                                false,
                                Some("capability denied"),
                            )
                            .map(LuaValue::Table);
                        }
                        let offset = consume_self_arg(&args);
                        let payload = method
                            .args
                            .iter()
                            .enumerate()
                            .map(|(index, arg)| {
                                let lua_value =
                                    args.get(index + offset).cloned().unwrap_or(LuaValue::Nil);
                                lua.from_value::<Value>(lua_value)
                                    .map(|value| (arg.name.clone(), value))
                            })
                            .collect::<mlua::Result<serde_json::Map<String, Value>>>()?;
                        events.lock().unwrap().push(PublishedEvent {
                            channel: format!("{}.{}", iface, method.name),
                            payload: Value::Object(payload),
                            source_module_id: source_module_id.clone(),
                            source_capabilities: source_capabilities.clone(),
                        });
                        command_result_table(lua, true, true, None).map(LuaValue::Table)
                    },
                )?));
            }

            // Case B: state field read from the live service payload table.
            tracked_service_fields
                .lock()
                .unwrap()
                .entry(service_name.clone())
                .or_default()
                .insert(key.clone());
            service_payload_field(lua, &service_name, &key)
        })?,
    )?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

pub(super) fn create_events_proxy(lua: &Lua, event_names: Vec<String>) -> mlua::Result<Table> {
    let events = lua.create_table()?;
    for name in event_names {
        events.set(name, create_event_channel(lua)?)?;
    }
    Ok(events)
}

pub(super) fn create_event_channel(lua: &Lua) -> mlua::Result<Table> {
    let channel = lua.create_table()?;
    let subscribers = lua.create_table()?;
    channel.set("__subscribers", subscribers.clone())?;
    channel.set(
        "subscribe",
        lua.create_function(move |lua, (table, callback): (Table, Function)| {
            let subscribers: Table = table.get("__subscribers")?;
            let id = subscribers.raw_len() + 1;
            subscribers.raw_set(id, callback)?;
            Ok(lua.create_function(move |_lua, ()| subscribers.raw_set(id, LuaValue::Nil))?)
        })?,
    )?;
    channel.set(
        "emit",
        lua.create_function(move |_lua, (table, payload): (Table, LuaValue)| {
            let subscribers: Table = table.get("__subscribers")?;
            for callback in subscribers.sequence_values::<Function>().flatten() {
                callback.call::<()>(payload.clone())?;
            }
            Ok(())
        })?,
    )?;
    Ok(channel)
}

fn create_service_state_proxy(
    lua: &Lua,
    service_name: String,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
) -> mlua::Result<Table> {
    let state = lua.create_table()?;
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(move |lua, (_table, key): (Table, String)| {
            tracked_service_fields
                .lock()
                .unwrap()
                .entry(service_name.clone())
                .or_default()
                .insert(key.clone());
            service_payload_field(lua, &service_name, &key)
        })?,
    )?;
    state.set_metatable(Some(meta))?;
    Ok(state)
}

fn service_payload_field(lua: &Lua, service_name: &str, key: &str) -> mlua::Result<LuaValue> {
    let svc_key = format!("__mesh_svc_{service_name}");
    let table = match lua.globals().get::<LuaValue>(svc_key.as_str()) {
        Ok(LuaValue::Table(table)) => Some(table),
        _ => None,
    };
    Ok(table
        .and_then(|table| table.get::<LuaValue>(key).ok())
        .unwrap_or(LuaValue::Nil))
}

fn command_result_table(
    lua: &Lua,
    ok: bool,
    queued: bool,
    error: Option<&str>,
) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    result.set("ok", ok)?;
    if ok {
        result.set("queued", queued)?;
    }
    if let Some(error) = error {
        result.set("error", error)?;
    }
    Ok(result)
}

fn consume_self_arg(args: &mlua::Variadic<LuaValue>) -> usize {
    match args.get(0) {
        Some(LuaValue::Table(_)) => 1,
        _ => 0,
    }
}

pub(super) fn service_name_from_interface(interface: &str) -> String {
    interface
        .strip_prefix("mesh.")
        .unwrap_or(interface)
        .to_string()
}

fn service_control_capability(service_name: &str) -> Capability {
    Capability::new(format!("service.{service_name}.control"))
}
