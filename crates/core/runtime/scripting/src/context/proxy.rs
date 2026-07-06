use super::PublishedEvent;
use mesh_core_capability::{Capability, CapabilitySet};
pub(super) use mesh_core_service::service_name_from_interface;
use mesh_core_service::{InterfaceContract, InterfaceResolution};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) fn create_interface_proxy(
    lua: &Lua,
    scope: &Table,
    resolution: InterfaceResolution,
    source_module_id: String,
    source_capabilities: CapabilitySet,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    subscribed_interface_events: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    pending_side_channels: Arc<AtomicBool>,
) -> mlua::Result<Table> {
    create_service_proxy(
        lua,
        scope,
        service_name_from_interface(&resolution.requested),
        resolution.contract,
        resolution.requested,
        source_module_id,
        source_capabilities,
        tracked_service_fields,
        subscribed_interface_events,
        published_events,
        pending_side_channels,
    )
}

pub(super) fn create_service_proxy(
    lua: &Lua,
    scope: &Table,
    service_name: String,
    contract: Option<Arc<InterfaceContract>>,
    interface_name: String,
    source_module_id: String,
    source_capabilities: CapabilitySet,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    subscribed_interface_events: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    pending_side_channels: Arc<AtomicBool>,
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
    let observed_state_fields = Arc::new(Mutex::new(HashSet::new()));
    let state_proxy = create_service_state_proxy(
        lua,
        service_name.clone(),
        Arc::clone(&tracked_service_fields),
        Arc::clone(&observed_state_fields),
    )?;
    proxy.set("state", state_proxy)?;
    let events_proxy = create_events_proxy(
        lua,
        scope,
        &service_name,
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
        Arc::clone(&subscribed_interface_events),
    )?;
    proxy.set("events", events_proxy)?;
    let event_names = contract
        .as_ref()
        .map(|contract| {
            contract
                .events
                .iter()
                .map(|event| event.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let method_names = methods
        .iter()
        .map(|method| method.name.clone())
        .collect::<HashSet<_>>();
    let state_field_names = contract
        .as_ref()
        .map(|contract| {
            contract
                .state_fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    let index_scope = scope.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (table, key): (Table, String)| {
            if key == "state" || key == "events" {
                return table.get::<LuaValue>(key);
            }
            if event_names.iter().any(|name| name == &key)
                && !method_names.contains(&key)
                && !state_field_names.contains(&key)
            {
                return interface_event_channel(
                    lua,
                    &index_scope,
                    &service_name,
                    &key,
                    Some(Arc::clone(&subscribed_interface_events)),
                )
                .map(LuaValue::Table);
            }
            // Case A: known contract method — dispatch as a service command.
            if let Some(method) = methods.iter().find(|m| m.name == key) {
                let required_capability = service_control_capability(&service_name);
                let method = method.clone();
                let iface = interface_name.clone();
                let events = Arc::clone(&published_events);
                let pending_side_channels = Arc::clone(&pending_side_channels);
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
                        pending_side_channels.store(true, Ordering::Release);
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
            record_tracked_field_once(
                &tracked_service_fields,
                &observed_state_fields,
                &service_name,
                &key,
            );
            service_payload_field(lua, &service_name, &key)
        })?,
    )?;
    proxy.set_metatable(Some(meta))?;
    Ok(proxy)
}

pub(super) fn create_events_proxy(
    lua: &Lua,
    scope: &Table,
    service_name: &str,
    event_names: Vec<String>,
    subscribed_interface_events: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
) -> mlua::Result<Table> {
    let events = lua.create_table()?;
    for name in event_names {
        events.set(
            name.as_str(),
            interface_event_channel(
                lua,
                scope,
                service_name,
                &name,
                Some(Arc::clone(&subscribed_interface_events)),
            )?,
        )?;
    }
    Ok(events)
}

/// Resolve (or lazily create) the interface-event channel for `(service, event)`.
///
/// The channel registry lives on the per-instance `_ENV` table (`scope`), not on
/// `lua.globals()`, so that components sharing a single surface VM keep
/// independent channels and per-context subscription tracking.
pub(super) fn interface_event_channel(
    lua: &Lua,
    scope: &Table,
    service_name: &str,
    event_name: &str,
    subscribed_interface_events: Option<Arc<Mutex<HashMap<String, HashMap<String, usize>>>>>,
) -> mlua::Result<Table> {
    let registry = match scope.raw_get::<LuaValue>("__mesh_interface_event_channels")? {
        LuaValue::Table(table) => table,
        _ => {
            let table = lua.create_table()?;
            scope.raw_set("__mesh_interface_event_channels", table.clone())?;
            table
        }
    };
    let service_table = match registry.raw_get::<LuaValue>(service_name)? {
        LuaValue::Table(table) => table,
        _ => {
            let table = lua.create_table()?;
            registry.raw_set(service_name, table.clone())?;
            table
        }
    };
    match service_table.raw_get::<LuaValue>(event_name)? {
        LuaValue::Table(channel) => Ok(channel),
        _ => {
            let channel = create_event_channel(
                lua,
                subscribed_interface_events,
                Some((service_name.to_string(), event_name.to_string())),
            )?;
            service_table.raw_set(event_name, channel.clone())?;
            Ok(channel)
        }
    }
}

pub(super) fn create_event_channel(
    lua: &Lua,
    subscribed_interface_events: Option<Arc<Mutex<HashMap<String, HashMap<String, usize>>>>>,
    subscription_key: Option<(String, String)>,
) -> mlua::Result<Table> {
    let channel = lua.create_table()?;
    let subscribers = lua.create_table()?;
    channel.set("__subscribers", subscribers.clone())?;
    channel.set(
        "subscribe",
        lua.create_function(move |lua, (table, callback): (Table, Function)| {
            let subscribers: Table = table.get("__subscribers")?;
            let id = subscribers.raw_len() + 1;
            subscribers.raw_set(id, callback)?;
            if let (Some(registry), Some((service_name, event_name))) =
                (&subscribed_interface_events, &subscription_key)
            {
                let mut registry = registry.lock().unwrap();
                *registry
                    .entry(service_name.clone())
                    .or_default()
                    .entry(event_name.clone())
                    .or_default() += 1;
            }
            let subscribed_interface_events = subscribed_interface_events.clone();
            let subscription_key = subscription_key.clone();
            Ok(lua.create_function(move |_lua, ()| {
                let existing = subscribers.raw_get::<LuaValue>(id)?;
                if !matches!(existing, LuaValue::Nil) {
                    subscribers.raw_set(id, LuaValue::Nil)?;
                    if let (Some(registry), Some((service_name, event_name))) =
                        (&subscribed_interface_events, &subscription_key)
                    {
                        let mut registry = registry.lock().unwrap();
                        if let Some(events) = registry.get_mut(service_name) {
                            if let Some(count) = events.get_mut(event_name) {
                                *count = count.saturating_sub(1);
                                if *count == 0 {
                                    events.remove(event_name);
                                }
                            }
                            if events.is_empty() {
                                registry.remove(service_name);
                            }
                        }
                    }
                }
                Ok(())
            })?)
        })?,
    )?;
    channel.set("on", channel.get::<Function>("subscribe")?)?;
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
    channel.set("fire", channel.get::<Function>("emit")?)?;
    Ok(channel)
}

fn create_service_state_proxy(
    lua: &Lua,
    service_name: String,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    observed_state_fields: Arc<Mutex<HashSet<String>>>,
) -> mlua::Result<Table> {
    let state = lua.create_table()?;
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(move |lua, (_table, key): (Table, String)| {
            record_tracked_field_once(
                &tracked_service_fields,
                &observed_state_fields,
                &service_name,
                &key,
            );
            service_payload_field(lua, &service_name, &key)
        })?,
    )?;
    state.set_metatable(Some(meta))?;
    Ok(state)
}

fn record_tracked_field_once(
    tracked_service_fields: &Arc<Mutex<HashMap<String, HashSet<String>>>>,
    observed_state_fields: &Arc<Mutex<HashSet<String>>>,
    service_name: &str,
    key: &str,
) {
    if !observed_state_fields
        .lock()
        .unwrap()
        .insert(key.to_string())
    {
        return;
    }
    tracked_service_fields
        .lock()
        .unwrap()
        .entry(service_name.to_string())
        .or_default()
        .insert(key.to_string());
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

fn service_control_capability(service_name: &str) -> Capability {
    Capability::new(format!("service.{service_name}.control"))
}
