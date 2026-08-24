use super::PublishedEvent;
use super::state::ServiceContextState;
use crate::policy::ResourceBudget;
use mesh_core_capability::{Capability, CapabilitySet};
pub(super) use mesh_core_service::service_name_from_interface;
use mesh_core_service::{InterfaceContract, InterfaceResolution, TypeExpr};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

// Backend-owned CallId is intentionally kept in mesh-core-backend, which
// already depends on this crate. Reserve the high half of the u64 transport
// space for frontend-issued IDs so the shell can preserve the same identity
// without introducing a scripting/backend dependency cycle.
static NEXT_SERVICE_CALL_ID: AtomicU64 = AtomicU64::new(1 << 63);

pub(super) fn create_interface_proxy(
    lua: &Lua,
    scope: &Table,
    resolution: InterfaceResolution,
    source_module_id: String,
    source_instance_id: String,
    source_capabilities: CapabilitySet,
    service_context_state: Arc<Mutex<ServiceContextState>>,
    service_call_completions: Arc<Mutex<HashMap<u64, super::ServiceCallCompletion>>>,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    subscribed_interface_events: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    pending_side_channels: Arc<AtomicBool>,
    resources: ResourceBudget,
) -> mlua::Result<Table> {
    create_service_proxy(
        lua,
        scope,
        service_name_from_interface(&resolution.requested),
        resolution.contract,
        resolution.requested,
        source_module_id,
        source_instance_id,
        source_capabilities,
        service_context_state,
        service_call_completions,
        tracked_service_fields,
        subscribed_interface_events,
        published_events,
        pending_side_channels,
        resources,
    )
}

pub(super) fn create_service_proxy(
    lua: &Lua,
    scope: &Table,
    service_name: String,
    contract: Option<Arc<InterfaceContract>>,
    interface_name: String,
    source_module_id: String,
    source_instance_id: String,
    source_capabilities: CapabilitySet,
    service_context_state: Arc<Mutex<ServiceContextState>>,
    service_call_completions: Arc<Mutex<HashMap<u64, super::ServiceCallCompletion>>>,
    tracked_service_fields: Arc<Mutex<HashMap<String, HashSet<String>>>>,
    subscribed_interface_events: Arc<Mutex<HashMap<String, HashMap<String, usize>>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    pending_side_channels: Arc<AtomicBool>,
    resources: ResourceBudget,
) -> mlua::Result<Table> {
    let proxy = lua.create_table()?;
    // Method closures use this private marker to distinguish a Lua colon-call
    // on the proxy from a legitimate table-valued first argument.
    proxy.set("__mesh_interface_proxy", true)?;
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
        Arc::clone(&service_context_state),
        Arc::clone(&tracked_service_fields),
        Arc::clone(&observed_state_fields),
    )?;
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
        contract.clone(),
        source_capabilities.clone(),
    )?;
    let state_for_index = state_proxy.clone();
    let events_for_index = events_proxy.clone();
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

    let contract_for_index = contract.clone();
    let index_scope = scope.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (_table, key): (Table, String)| {
            if key == "state" {
                return Ok(LuaValue::Table(state_for_index.clone()));
            }
            if key == "events" {
                return Ok(LuaValue::Table(events_for_index.clone()));
            }
            if event_names.iter().any(|name| name == &key)
                && !method_names.contains(&key)
                && !state_field_names.contains(&key)
            {
                let event_allowed = contract_for_index.as_ref().map_or(true, |contract| {
                    crate::host_api::InterfaceProxy::can_subscribe_contract_event(
                        &source_capabilities,
                        contract,
                        &key,
                    )
                });
                return interface_event_channel(
                    lua,
                    &index_scope,
                    &service_name,
                    &key,
                    Some(Arc::clone(&subscribed_interface_events)),
                    event_allowed,
                )
                .map(LuaValue::Table);
            }
            // Case A: known contract method — dispatch as a service command.
            if let Some(method) = methods.iter().find(|m| m.name == key) {
                let required_capability = service_control_capability(&service_name);
                let method_contract = contract_for_index.clone();
                let method = method.clone();
                let iface = interface_name.clone();
                let events = Arc::clone(&published_events);
                let pending_side_channels = Arc::clone(&pending_side_channels);
                let source_module_id = source_module_id.clone();
                let source_instance_id = source_instance_id.clone();
                let source_capabilities = source_capabilities.clone();
                let service_call_completions = Arc::clone(&service_call_completions);
                let method_resources = resources.clone();
                return Ok(LuaValue::Function(lua.create_function(
                    move |lua, args: mlua::Variadic<LuaValue>| {
                        let authorized = method_contract.as_ref().map_or_else(
                            || source_capabilities.is_granted(&required_capability),
                            |contract| {
                                crate::host_api::InterfaceProxy::can_call_contract_method(
                                    &source_capabilities,
                                    contract,
                                    &method.name,
                                )
                            },
                        );
                        if !authorized {
                            return command_result_table(
                                lua,
                                false,
                                false,
                                Some("capability denied"),
                            )
                            .map(LuaValue::Table);
                        }
                        let offset = consume_self_arg(&args)?;
                        let supplied = args.len().saturating_sub(offset);
                        let required = method
                            .args
                            .iter()
                            .filter(|argument| {
                                !TypeExpr::parse(&argument.arg_type)
                                    .map(|value_type| value_type.optional)
                                    .unwrap_or(false)
                            })
                            .count();
                        if supplied < required || supplied > method.args.len() {
                            return Err(mlua::Error::runtime(format!(
                                "service method '{}.{}' expects {}..{} arguments, got {}",
                                iface,
                                method.name,
                                required,
                                method.args.len(),
                                supplied
                            )));
                        }
                        let payload = method
                            .args
                            .iter()
                            .enumerate()
                            .take(supplied)
                            .map(|(index, arg)| {
                                let lua_value = args
                                    .get(index + offset)
                                    .cloned()
                                    .expect("validated service method argument count");
                                let value = lua.from_value::<Value>(lua_value)?;
                                let value_type =
                                    TypeExpr::parse(&arg.arg_type).map_err(mlua::Error::runtime)?;
                                if let Some(contract) = method_contract.as_ref()
                                    && !value_type.matches_with_types(&value, &contract.types)
                                {
                                    return Err(mlua::Error::runtime(format!(
                                        "service method '{}.{}' argument '{}' expected {}, got {}",
                                        iface,
                                        method.name,
                                        arg.name,
                                        arg.arg_type,
                                        json_type_name(&value)
                                    )));
                                }
                                Ok((arg.name.clone(), value))
                            })
                            .collect::<mlua::Result<serde_json::Map<String, Value>>>()?;
                        let call_id = NEXT_SERVICE_CALL_ID.fetch_add(1, Ordering::Relaxed);
                        let payload = Value::Object(payload);
                        let output_bytes = serde_json::to_vec(&payload)
                            .map_err(mlua::Error::external)?
                            .len();
                        method_resources
                            .reserve_output(output_bytes)
                            .map_err(|error| mlua::Error::external(error.to_string()))?;
                        method_resources
                            .reserve_queue()
                            .map_err(|error| mlua::Error::external(error.to_string()))?;
                        pending_side_channels.store(true, Ordering::Release);
                        events.lock().unwrap().push(PublishedEvent {
                            channel: format!("{}.{}", iface, method.name),
                            payload,
                            source_module_id: source_module_id.clone(),
                            source_capabilities: source_capabilities.clone(),
                            call_id: Some(call_id),
                            source_instance_id: Some(source_instance_id.clone()),
                        });
                        create_service_call_ticket(
                            lua,
                            call_id,
                            iface.clone(),
                            source_module_id.clone(),
                            source_instance_id.clone(),
                            source_capabilities.clone(),
                            Arc::clone(&service_call_completions),
                            Arc::clone(&events),
                            Arc::clone(&pending_side_channels),
                            method_resources.clone(),
                        )
                        .map(LuaValue::Table)
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
            service_payload_field(lua, &service_context_state, &service_name, &key)
        })?,
    )?;
    meta.set(
        "__newindex",
        lua.create_function(|_, (_table, key, _value): (Table, String, LuaValue)| {
            Err::<(), _>(mlua::Error::runtime(format!(
                "service state field '{key}' is read-only"
            )))
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
    contract: Option<Arc<InterfaceContract>>,
    source_capabilities: CapabilitySet,
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
                contract.as_ref().map_or(true, |contract| {
                    crate::host_api::InterfaceProxy::can_subscribe_contract_event(
                        &source_capabilities,
                        contract,
                        &name,
                    )
                }),
            )?,
        )?;
    }
    Ok(events)
}

/// Resolve (or lazily create) the interface-event channel for `(service, event)`.
///
/// The channel registry lives on the per-instance `_ENV` table (`scope`), not on
/// `lua.globals()`, so that components sharing a single thread VM keep
/// independent channels and per-context subscription tracking.
pub(super) fn interface_event_channel(
    lua: &Lua,
    scope: &Table,
    service_name: &str,
    event_name: &str,
    subscribed_interface_events: Option<Arc<Mutex<HashMap<String, HashMap<String, usize>>>>>,
    subscription_allowed: bool,
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
            let channel = create_event_channel_with_policy(
                lua,
                subscribed_interface_events,
                Some((service_name.to_string(), event_name.to_string())),
                subscription_allowed,
                Some(format!(
                    "capability denied for service event '{service_name}.{event_name}'"
                )),
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
    create_event_channel_with_policy(
        lua,
        subscribed_interface_events,
        subscription_key,
        true,
        None,
    )
}

fn create_event_channel_with_policy(
    lua: &Lua,
    subscribed_interface_events: Option<Arc<Mutex<HashMap<String, HashMap<String, usize>>>>>,
    subscription_key: Option<(String, String)>,
    subscription_allowed: bool,
    denial_message: Option<String>,
) -> mlua::Result<Table> {
    let channel = lua.create_table()?;
    let subscribers = lua.create_table()?;
    let next_subscription_id = Arc::new(AtomicU64::new(1));
    let event_name = subscription_key
        .as_ref()
        .map(|(service, event)| format!("{service}.{event}"))
        .unwrap_or_else(|| "unnamed".to_string());
    channel.set("__subscribers", subscribers.clone())?;
    channel.set(
        "subscribe",
        lua.create_function(move |lua, (table, callback): (Table, Function)| {
            if !subscription_allowed {
                return Err(mlua::Error::runtime(
                    denial_message
                        .clone()
                        .unwrap_or_else(|| "capability denied".to_string()),
                ));
            }
            let subscribers: Table = table.get("__subscribers")?;
            let id = next_subscription_id.fetch_add(1, Ordering::Relaxed);
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
    let event_name_for_emit = event_name.clone();
    channel.set(
        "emit",
        lua.create_function(move |_lua, (table, payload): (Table, LuaValue)| {
            let subscribers: Table = table.get("__subscribers")?;
            dispatch_event_subscribers(&subscribers, payload, &event_name_for_emit)
        })?,
    )?;
    channel.set("fire", channel.get::<Function>("emit")?)?;
    Ok(channel)
}

/// Dispatch a stable snapshot of subscription IDs. Looking up each callback
/// again before invocation makes an unsubscribe during an earlier callback
/// safe: removed subscribers are skipped, while newly added subscribers wait
/// for the next emission. Every callback is attempted and reported separately.
fn dispatch_event_subscribers(
    subscribers: &Table,
    payload: LuaValue,
    event_name: &str,
) -> mlua::Result<()> {
    let mut subscription_ids = subscribers
        .pairs::<u64, Function>()
        .map(|pair| pair.map(|(id, _)| id))
        .collect::<mlua::Result<Vec<_>>>()?;
    subscription_ids.sort_unstable();

    for subscription_id in subscription_ids {
        let callback = match subscribers.raw_get::<LuaValue>(subscription_id)? {
            LuaValue::Function(callback) => callback,
            _ => continue,
        };
        if let Err(error) = callback.call::<()>(payload.clone()) {
            tracing::warn!(
                event = event_name,
                subscription_id,
                error = %error,
                "event subscriber callback failed; continuing dispatch"
            );
        }
    }
    Ok(())
}

fn create_service_state_proxy(
    lua: &Lua,
    service_name: String,
    service_context_state: Arc<Mutex<ServiceContextState>>,
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
            service_payload_field(lua, &service_context_state, &service_name, &key)
        })?,
    )?;
    meta.set(
        "__newindex",
        lua.create_function(|_, (_table, key, _value): (Table, String, LuaValue)| {
            Err::<(), _>(mlua::Error::runtime(format!(
                "service state field '{key}' is read-only"
            )))
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
    let mut observed = observed_state_fields.lock().unwrap();
    if observed.contains(key) {
        return;
    }
    observed.insert(key.to_string());
    drop(observed);

    tracked_service_fields
        .lock()
        .unwrap()
        .entry(service_name.to_string())
        .or_default()
        .insert(key.to_string());
}

fn service_payload_field(
    lua: &Lua,
    service_context_state: &Arc<Mutex<ServiceContextState>>,
    service_name: &str,
    key: &str,
) -> mlua::Result<LuaValue> {
    let value = service_context_state
        .lock()
        .unwrap()
        .field(service_name, key);
    value
        .map(|value| lua.to_value(&value))
        .transpose()
        .map(|value| value.unwrap_or(LuaValue::Nil))
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

fn service_call_snapshot(
    lua: &Lua,
    call_id: u64,
    completions: &Arc<Mutex<HashMap<u64, super::ServiceCallCompletion>>>,
) -> mlua::Result<Table> {
    let snapshot = lua.create_table()?;
    snapshot.set("call_id", call_id)?;
    let completion = completions.lock().unwrap().get(&call_id).cloned();
    if let Some(completion) = completion {
        snapshot.set("done", true)?;
        snapshot.set("status", completion.status)?;
        snapshot.set("result", lua.to_value(&completion.result)?)?;
    } else {
        snapshot.set("done", false)?;
        snapshot.set("status", "pending")?;
    }
    Ok(snapshot)
}

fn create_service_call_ticket(
    lua: &Lua,
    call_id: u64,
    interface: String,
    source_module_id: String,
    source_instance_id: String,
    source_capabilities: CapabilitySet,
    completions: Arc<Mutex<HashMap<u64, super::ServiceCallCompletion>>>,
    published_events: Arc<Mutex<Vec<PublishedEvent>>>,
    pending_side_channels: Arc<AtomicBool>,
    resources: ResourceBudget,
) -> mlua::Result<Table> {
    let ticket = lua.create_table()?;
    ticket.set("call_id", call_id)?;
    ticket.set("status", "pending")?;
    ticket.set("ok", true)?;
    ticket.set("queued", true)?;

    let poll_completions = Arc::clone(&completions);
    let poll = lua.create_function(move |lua, _args: mlua::Variadic<LuaValue>| {
        service_call_snapshot(lua, call_id, &poll_completions)
    })?;
    ticket.set("poll", poll.clone())?;
    // Handler execution is synchronous, so `await` is a cooperative poll
    // point. It has the same stable shape as poll and can be called from a
    // later tick/handler without blocking the shell thread.
    ticket.set("await", poll)?;

    let cancel_completions = Arc::clone(&completions);
    let cancel_events = Arc::clone(&published_events);
    let cancel_pending = Arc::clone(&pending_side_channels);
    let cancel_module = source_module_id.clone();
    let cancel_instance = source_instance_id.clone();
    let cancel_capabilities = source_capabilities.clone();
    let cancel_resources = resources;
    let cancel_requested = Arc::new(AtomicBool::new(false));
    let cancel_requested_for_call = Arc::clone(&cancel_requested);
    ticket.set(
        "cancel",
        lua.create_function(move |_lua, _args: mlua::Variadic<LuaValue>| {
            if cancel_requested_for_call.swap(true, Ordering::AcqRel)
                || cancel_completions.lock().unwrap().contains_key(&call_id)
            {
                return Ok(false);
            }
            let payload = serde_json::json!({
                "interface": interface,
                "call_id": call_id,
            });
            let output_bytes = serde_json::to_vec(&payload)
                .map_err(mlua::Error::external)?
                .len();
            cancel_resources
                .reserve_output(output_bytes)
                .map_err(|error| mlua::Error::external(error.to_string()))?;
            cancel_resources
                .reserve_queue()
                .map_err(|error| mlua::Error::external(error.to_string()))?;
            cancel_pending.store(true, Ordering::Release);
            cancel_events.lock().unwrap().push(PublishedEvent {
                channel: "mesh.service.cancel".to_string(),
                payload,
                source_module_id: cancel_module.clone(),
                source_capabilities: cancel_capabilities.clone(),
                call_id: Some(call_id),
                source_instance_id: Some(cancel_instance.clone()),
            });
            Ok(true)
        })?,
    )?;

    Ok(ticket)
}

fn consume_self_arg(args: &mlua::Variadic<LuaValue>) -> mlua::Result<usize> {
    match args.get(0) {
        Some(LuaValue::Table(table)) => Ok(table
            .raw_get::<bool>("__mesh_interface_proxy")
            .map(|is_proxy| usize::from(is_proxy))?),
        _ => Ok(0),
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn service_control_capability(service_name: &str) -> Capability {
    Capability::new(format!("service.{service_name}.control"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sinks() -> (
        Arc<Mutex<HashMap<String, HashSet<String>>>>,
        Arc<Mutex<HashSet<String>>>,
    ) {
        (
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashSet::new())),
        )
    }

    #[test]
    fn repeated_tracked_field_reads_record_once() {
        let (tracked, observed) = sinks();
        record_tracked_field_once(&tracked, &observed, "audio", "percent");
        record_tracked_field_once(&tracked, &observed, "audio", "percent");
        record_tracked_field_once(&tracked, &observed, "audio", "muted");

        let tracked = tracked.lock().unwrap();
        let audio = tracked.get("audio").expect("audio reads");
        assert_eq!(audio.len(), 2);
        assert!(audio.contains("percent"));
        assert!(audio.contains("muted"));
    }

    // cargo test -p mesh-core-scripting --release -- tracked_field_duplicate_read_gate_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only tracked service-field read microbenchmark"]
    fn tracked_field_duplicate_read_gate_benchmark() {
        fn old_record(
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

        let iterations = 1_000_000usize;
        let (old_tracked, old_observed) = sinks();
        old_record(&old_tracked, &old_observed, "audio", "percent");
        let old_started = std::time::Instant::now();
        for _ in 0..iterations {
            old_record(
                std::hint::black_box(&old_tracked),
                std::hint::black_box(&old_observed),
                std::hint::black_box("audio"),
                std::hint::black_box("percent"),
            );
        }
        let old_time = old_started.elapsed();

        let (new_tracked, new_observed) = sinks();
        record_tracked_field_once(&new_tracked, &new_observed, "audio", "percent");
        let new_started = std::time::Instant::now();
        for _ in 0..iterations {
            record_tracked_field_once(
                std::hint::black_box(&new_tracked),
                std::hint::black_box(&new_observed),
                std::hint::black_box("audio"),
                std::hint::black_box("percent"),
            );
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "duplicate tracked service reads: allocate-insert {old_time:?}; borrowed-gate {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }
}
