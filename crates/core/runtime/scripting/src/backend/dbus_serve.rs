//! D-Bus service export for backend providers.
//!
//! `mesh.dbus_serve` owns one well-known name and exports one interface at one
//! object path. Method calls arrive on the owning stream as `call` records;
//! the provider answers them with `mesh.dbus_reply` / `mesh.dbus_reply_error`
//! and emits declared signals with `mesh.dbus_emit`. Bodies use the same
//! `{signature, value}` JSON shape as signal records and `mesh.dbus_call`.
//!
//! The host validates call signatures against the declared methods, answers
//! `Introspect` and `Ping`, and replies `NoReply` to calls the provider leaves
//! unanswered, so a buggy provider cannot stall its clients past the D-Bus
//! default timeout. Arrays longer than [`MAX_ARRAY_ITEMS`] (raw pixel data
//! in notification hints, for example) are replaced by `{"elided": <len>}`
//! so one large argument cannot push a call past the record limit. Dropping
//! the stream drops the connection, which releases the name.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value as Json;
use tokio::sync::mpsc;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::zvariant::Structure;
use zbus::{Connection, Message, MessageStream};

use super::dbus_call::{BusTarget, body_types, encode_body};
use super::exec_stream::{
    MAX_STREAM_LINE_BYTES, StreamEvent, StreamEventKind, StreamHandle, StreamId, StreamState,
};
use super::host_sources::{finish, spawn_source};

/// Calls a provider may hold unanswered at once; further calls are refused.
const MAX_PENDING_CALLS: usize = 64;
/// The reference implementation's default method-call timeout.
const CALL_DEADLINE: Duration = Duration::from_secs(25);
const SWEEP_INTERVAL: Duration = Duration::from_secs(1);
const MAX_ARRAY_ITEMS: usize = 1024;

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MethodSpec {
    #[serde(default, rename = "in")]
    pub input: String,
    #[serde(default, rename = "out")]
    pub output: String,
}

#[derive(Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PropertySpec {
    pub signature: String,
    pub value: Json,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServiceSpec {
    pub name: String,
    pub path: String,
    pub interface: String,
    #[serde(default)]
    pub methods: BTreeMap<String, MethodSpec>,
    #[serde(default)]
    pub signals: BTreeMap<String, String>,
    #[serde(default)]
    pub properties: BTreeMap<String, PropertySpec>,
    /// Take the name from a current owner that allows replacement.
    #[serde(default)]
    pub replace: bool,
}

impl ServiceSpec {
    fn validate(&self) -> Result<(), String> {
        zbus::names::WellKnownName::try_from(self.name.as_str())
            .map_err(|error| format!("invalid bus name {:?}: {error}", self.name))?;
        zbus::zvariant::ObjectPath::try_from(self.path.as_str())
            .map_err(|error| format!("invalid object path {:?}: {error}", self.path))?;
        zbus::names::InterfaceName::try_from(self.interface.as_str())
            .map_err(|error| format!("invalid interface {:?}: {error}", self.interface))?;
        for (member, method) in &self.methods {
            zbus::names::MemberName::try_from(member.as_str())
                .map_err(|error| format!("invalid method name {member:?}: {error}"))?;
            body_types(&method.input)?;
            body_types(&method.output)?;
        }
        if self.properties.len() > 256 {
            return Err("too many D-Bus properties".into());
        }
        for (name, property) in &self.properties {
            zbus::names::MemberName::try_from(name.as_str()).map_err(|error| error.to_string())?;
            if body_types(&property.signature)?.len() != 1 {
                return Err("a property needs exactly one D-Bus type".into());
            }
            encode_body(&property.signature, &[property.value.clone()])?;
            if property.value.to_string().len() > MAX_STREAM_LINE_BYTES / 4 {
                return Err("D-Bus property exceeds size limit".into());
            }
        }
        for (member, signature) in &self.signals {
            zbus::names::MemberName::try_from(member.as_str())
                .map_err(|error| format!("invalid signal name {member:?}: {error}"))?;
            body_types(signature)?;
        }
        Ok(())
    }

    fn introspection(&self) -> String {
        let args = |signature: &str, direction: Option<&str>| {
            body_types(signature)
                .unwrap_or_default()
                .iter()
                .map(|field| match direction {
                    Some(direction) => {
                        format!("      <arg type=\"{field}\" direction=\"{direction}\"/>\n")
                    }
                    None => format!("      <arg type=\"{field}\"/>\n"),
                })
                .collect::<String>()
        };
        let mut xml = String::from(
            "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\" \
             \"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">\n<node>\n",
        );
        xml.push_str(&format!("  <interface name=\"{}\">\n", self.interface));
        for (member, method) in &self.methods {
            xml.push_str(&format!("    <method name=\"{member}\">\n"));
            xml.push_str(&args(&method.input, Some("in")));
            xml.push_str(&args(&method.output, Some("out")));
            xml.push_str("    </method>\n");
        }
        for (member, signature) in &self.signals {
            xml.push_str(&format!("    <signal name=\"{member}\">\n"));
            xml.push_str(&args(signature, None));
            xml.push_str("    </signal>\n");
        }
        for (name, property) in &self.properties {
            xml.push_str(&format!(
                "    <property name=\"{name}\" type=\"{}\" access=\"read\"/>\n",
                property.signature
            ));
        }
        xml.push_str("  </interface>\n  <interface name=\"org.freedesktop.DBus.Properties\">\n    <method name=\"Get\"><arg type=\"s\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/><arg type=\"v\" direction=\"out\"/></method>\n    <method name=\"GetAll\"><arg type=\"s\" direction=\"in\"/><arg type=\"a{sv}\" direction=\"out\"/></method>\n    <method name=\"Set\"><arg type=\"s\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/><arg type=\"v\" direction=\"in\"/></method>\n    <signal name=\"PropertiesChanged\"><arg type=\"s\"/><arg type=\"a{sv}\"/><arg type=\"as\"/></signal>\n");
        xml.push_str(
            "  </interface>\n  <interface name=\"org.freedesktop.DBus.Introspectable\">\n    \
             <method name=\"Introspect\">\n      <arg type=\"s\" direction=\"out\"/>\n    \
             </method>\n  </interface>\n  <interface name=\"org.freedesktop.DBus.Peer\">\n    \
             <method name=\"Ping\"/>\n  </interface>\n</node>\n",
        );
        xml
    }
}

enum Command {
    PropertyChanged {
        body: Structure<'static>,
    },
    Reply {
        id: u64,
        body: Option<Structure<'static>>,
    },
    Error {
        id: u64,
        name: String,
        message: String,
    },
    Emit {
        member: String,
        body: Option<Structure<'static>>,
    },
}

struct Service {
    spec: Arc<ServiceSpec>,
    properties: Arc<Mutex<BTreeMap<String, PropertySpec>>>,
    commands: mpsc::UnboundedSender<Command>,
    /// Pending call id -> method name, so replies encode against the
    /// declared output signature before they reach the connection task.
    pending: Arc<Mutex<HashMap<u64, String>>>,
}

/// The services one backend runtime exports, keyed by owning stream.
#[derive(Default)]
pub(super) struct DbusServices {
    services: Mutex<HashMap<StreamId, Service>>,
}

impl DbusServices {
    pub(super) fn reply(&self, stream: StreamId, id: u64, args: &[Json]) -> Result<(), String> {
        let services = self
            .services
            .lock()
            .expect("dbus service registry poisoned");
        let service = services.get(&stream).ok_or("not an active D-Bus service")?;
        let member = service
            .pending
            .lock()
            .expect("dbus pending registry poisoned")
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("no pending D-Bus call {id}"))?;
        let output = &service.spec.methods[&member].output;
        let body = encode_body(output, args)?;
        service
            .commands
            .send(Command::Reply { id, body })
            .map_err(|_| "D-Bus service has stopped".to_string())
    }

    pub(super) fn reply_error(
        &self,
        stream: StreamId,
        id: u64,
        name: String,
        message: String,
    ) -> Result<(), String> {
        zbus::names::ErrorName::try_from(name.as_str())
            .map_err(|error| format!("invalid D-Bus error name {name:?}: {error}"))?;
        let services = self
            .services
            .lock()
            .expect("dbus service registry poisoned");
        let service = services.get(&stream).ok_or("not an active D-Bus service")?;
        if !service
            .pending
            .lock()
            .expect("dbus pending registry poisoned")
            .contains_key(&id)
        {
            return Err(format!("no pending D-Bus call {id}"));
        }
        service
            .commands
            .send(Command::Error { id, name, message })
            .map_err(|_| "D-Bus service has stopped".to_string())
    }

    pub(super) fn set_property(
        &self,
        stream: StreamId,
        name: &str,
        value: Json,
    ) -> Result<(), String> {
        let services = self
            .services
            .lock()
            .expect("dbus service registry poisoned");
        let service = services.get(&stream).ok_or("not an active D-Bus service")?;
        let mut properties = service
            .properties
            .lock()
            .expect("dbus property registry poisoned");
        let property = properties.get_mut(name).ok_or("property is not declared")?;
        encode_body(&property.signature, &[value.clone()])?;
        if value.to_string().len() > MAX_STREAM_LINE_BYTES / 4 {
            return Err("D-Bus property exceeds size limit".into());
        }
        if property.value == value {
            return Ok(());
        }
        let variant = serde_json::json!({"signature": property.signature, "value": value});
        let body = encode_body(
            "sa{sv}as",
            &[
                Json::String(service.spec.interface.clone()),
                serde_json::json!({name: variant}),
                serde_json::json!([]),
            ],
        )?
        .unwrap();
        service
            .commands
            .send(Command::PropertyChanged { body })
            .map_err(|_| "D-Bus service has stopped".to_string())?;
        property.value = value;
        Ok(())
    }

    pub(super) fn emit(&self, stream: StreamId, member: &str, args: &[Json]) -> Result<(), String> {
        let services = self
            .services
            .lock()
            .expect("dbus service registry poisoned");
        let service = services.get(&stream).ok_or("not an active D-Bus service")?;
        let signature = service
            .spec
            .signals
            .get(member)
            .ok_or_else(|| format!("signal {member:?} is not declared"))?;
        let body = encode_body(signature, args)?;
        service
            .commands
            .send(Command::Emit {
                member: member.to_string(),
                body,
            })
            .map_err(|_| "D-Bus service has stopped".to_string())
    }
}

pub(super) fn spawn_service(
    state: &Arc<StreamState>,
    services: &Arc<DbusServices>,
    bus: BusTarget,
    spec: ServiceSpec,
) -> std::io::Result<StreamHandle> {
    spec.validate().map_err(std::io::Error::other)?;
    let spec = Arc::new(spec);
    let (commands, receiver) = mpsc::unbounded_channel();
    let pending = Arc::new(Mutex::new(HashMap::new()));
    let registry = Arc::clone(services);
    let task_spec = Arc::clone(&spec);
    let properties = Arc::new(Mutex::new(spec.properties.clone()));
    let task_properties = Arc::clone(&properties);
    let task_pending = Arc::clone(&pending);
    // Hold the registry across the spawn so a service that fails at once
    // removes its entry only after it was inserted.
    let mut active = services
        .services
        .lock()
        .expect("dbus service registry poisoned");
    let handle = spawn_source(
        state,
        format!("dbus-serve:{}", spec.name),
        &spec.path,
        move |state, stream, mut stop| async move {
            let result = tokio::select! {
                _ = &mut stop => Ok(()),
                result = serve(&state, &stream, bus, &task_spec, &task_properties, &task_pending, receiver) => result,
            };
            registry
                .services
                .lock()
                .expect("dbus service registry poisoned")
                .remove(&stream.id());
            finish(&state, stream, result.err());
        },
    )?;
    active.insert(
        handle.id(),
        Service {
            spec,
            properties,
            commands,
            pending,
        },
    );
    Ok(handle)
}

async fn serve(
    state: &Arc<StreamState>,
    stream: &StreamHandle,
    bus: BusTarget,
    spec: &ServiceSpec,
    properties: &Mutex<BTreeMap<String, PropertySpec>>,
    pending_members: &Mutex<HashMap<u64, String>>,
    mut commands: mpsc::UnboundedReceiver<Command>,
) -> Result<(), String> {
    let connection = bus
        .connect()
        .await
        .map_err(|error| format!("D-Bus connection failed: {error}"))?;
    let mut messages = MessageStream::from(&connection);
    let flags = if spec.replace {
        RequestNameFlags::DoNotQueue | RequestNameFlags::ReplaceExisting
    } else {
        RequestNameFlags::DoNotQueue.into()
    };
    match connection
        .request_name_with_flags(spec.name.as_str(), flags)
        .await
    {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {}
        Ok(_) | Err(zbus::Error::NameTaken) => {
            return Err(format!(
                "D-Bus name {} is owned by another client",
                spec.name
            ));
        }
        Err(error) => return Err(format!("D-Bus name request failed: {error}")),
    }
    state.push_event(StreamEvent {
        stream: stream.clone(),
        kind: StreamEventKind::Started,
    });

    let mut pending: HashMap<u64, (Message, Instant)> = HashMap::new();
    let mut next_id: u64 = 1;
    let mut sweep = tokio::time::interval(SWEEP_INTERVAL);
    let error = |error: zbus::Error| error.to_string();
    loop {
        tokio::select! {
            message = messages.next() => {
                let Some(message) = message else {
                    return Err("D-Bus connection closed".into());
                };
                let message = message.map_err(error)?;
                let Some(record) = route(&connection, spec, properties, &message).await.map_err(error)? else {
                    continue;
                };
                if pending.len() >= MAX_PENDING_CALLS {
                    connection
                        .reply_error(
                            &message.header(),
                            "org.freedesktop.DBus.Error.LimitsExceeded",
                            &"too many pending calls",
                        )
                        .await
                        .map_err(error)?;
                    continue;
                }
                let id = next_id;
                next_id += 1;
                let mut record = record;
                record["id"] = id.into();
                let line = record.to_string();
                if line.len() > MAX_STREAM_LINE_BYTES {
                    connection
                        .reply_error(
                            &message.header(),
                            "org.freedesktop.DBus.Error.LimitsExceeded",
                            &"call exceeds the provider record limit",
                        )
                        .await
                        .map_err(error)?;
                    continue;
                }
                pending_members
                    .lock()
                    .expect("dbus pending registry poisoned")
                    .insert(id, record["member"].as_str().unwrap_or_default().to_string());
                pending.insert(id, (message, Instant::now()));
                state.push_event(StreamEvent {
                    stream: stream.clone(),
                    kind: StreamEventKind::Line(line),
                });
            }
            command = commands.recv() => {
                let Some(command) = command else {
                    return Ok(());
                };
                match command {
                    Command::PropertyChanged { body } => {
                        connection.emit_signal(None::<&str>, spec.path.as_str(), "org.freedesktop.DBus.Properties", "PropertiesChanged", &body).await.map_err(error)?;
                    }
                    Command::Reply { id, body } => {
                        if let Some((call, _)) = take(&mut pending, pending_members, id) {
                            match body {
                                Some(body) => connection.reply(&call.header(), &body).await,
                                None => connection.reply(&call.header(), &()).await,
                            }
                            .map_err(error)?;
                        }
                    }
                    Command::Error { id, name, message } => {
                        if let Some((call, _)) = take(&mut pending, pending_members, id) {
                            connection
                                .reply_error(&call.header(), name.as_str(), &message)
                                .await
                                .map_err(error)?;
                        }
                    }
                    Command::Emit { member, body } => {
                        let destination = None::<zbus::names::BusName<'_>>;
                        let path = spec.path.as_str();
                        let interface = spec.interface.as_str();
                        match body {
                            Some(body) => {
                                connection
                                    .emit_signal(destination, path, interface, member.as_str(), &body)
                                    .await
                            }
                            None => {
                                connection
                                    .emit_signal(destination, path, interface, member.as_str(), &())
                                    .await
                            }
                        }
                        .map_err(error)?;
                    }
                }
            }
            _ = sweep.tick() => {
                let expired = pending
                    .iter()
                    .filter(|(_, (_, received))| received.elapsed() >= CALL_DEADLINE)
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>();
                for id in expired {
                    if let Some((call, _)) = take(&mut pending, pending_members, id) {
                        connection
                            .reply_error(
                                &call.header(),
                                "org.freedesktop.DBus.Error.NoReply",
                                &"the provider did not answer",
                            )
                            .await
                            .map_err(error)?;
                    }
                }
            }
        }
    }
}

fn take(
    pending: &mut HashMap<u64, (Message, Instant)>,
    members: &Mutex<HashMap<u64, String>>,
    id: u64,
) -> Option<(Message, Instant)> {
    members
        .lock()
        .expect("dbus pending registry poisoned")
        .remove(&id);
    pending.remove(&id)
}

/// Answer host-owned calls directly and turn calls for the exported
/// interface into provider records. Returns `None` once a call is answered.
async fn route(
    connection: &Connection,
    spec: &ServiceSpec,
    properties: &Mutex<BTreeMap<String, PropertySpec>>,
    message: &Message,
) -> zbus::Result<Option<Json>> {
    let header = message.header();
    if header.message_type() != zbus::message::Type::MethodCall {
        return Ok(None);
    }
    let path = header.path().map(|path| path.as_str()).unwrap_or_default();
    let interface = header.interface().map(|interface| interface.as_str());
    let member = header
        .member()
        .map(|member| member.as_str())
        .unwrap_or_default();
    match (interface, member) {
        (Some("org.freedesktop.DBus.Peer"), "Ping") => {
            connection.reply(&header, &()).await?;
            return Ok(None);
        }
        (Some("org.freedesktop.DBus.Introspectable"), "Introspect") => {
            let xml = if path == spec.path {
                spec.introspection()
            } else {
                "<node/>".to_string()
            };
            connection.reply(&header, &xml).await?;
            return Ok(None);
        }
        _ => {}
    }
    if path == spec.path && interface == Some("org.freedesktop.DBus.Properties") {
        let reply = property_reply(spec, properties, member, message);
        match reply {
            Ok(body) => {
                connection.reply(&header, &body).await?;
            }
            Err((name, error)) => {
                connection.reply_error(&header, name, &error).await?;
            }
        }
        return Ok(None);
    }
    let method = (path == spec.path && interface.is_none_or(|name| name == spec.interface))
        .then(|| spec.methods.get(member))
        .flatten();
    let Some(method) = method else {
        connection
            .reply_error(
                &header,
                "org.freedesktop.DBus.Error.UnknownMethod",
                &format!("{member} is not exported at {path}"),
            )
            .await?;
        return Ok(None);
    };
    let body = message.body();
    let signature = body.signature().to_string();
    if signature != method.input && signature != format!("({})", method.input) {
        connection
            .reply_error(
                &header,
                "org.freedesktop.DBus.Error.InvalidArgs",
                &format!("{member} expects ({}), got {signature}", method.input),
            )
            .await?;
        return Ok(None);
    }
    let fields = if body.is_empty() {
        Vec::new()
    } else {
        body.deserialize::<Structure<'_>>()?.into_fields()
    };
    let mut fields =
        serde_json::to_value(fields).map_err(|error| zbus::Error::Failure(error.to_string()))?;
    elide_large_arrays(&mut fields);
    Ok(Some(serde_json::json!({
        "type": "call",
        "member": member,
        "sender": header.sender().map(|sender| sender.as_str()),
        "signature": method.input,
        "body": fields,
    })))
}

fn property_reply(
    spec: &ServiceSpec,
    properties: &Mutex<BTreeMap<String, PropertySpec>>,
    member: &str,
    message: &Message,
) -> Result<Structure<'static>, (&'static str, String)> {
    let invalid = |error: String| ("org.freedesktop.DBus.Error.InvalidArgs", error);
    let (interface, property) = match member {
        "Get" => {
            let (interface, name) = message
                .body()
                .deserialize::<(String, String)>()
                .map_err(|error| invalid(error.to_string()))?;
            (interface, Some(name))
        }
        "GetAll" => (
            message
                .body()
                .deserialize::<String>()
                .map_err(|error| invalid(error.to_string()))?,
            None,
        ),
        "Set" => {
            return Err((
                "org.freedesktop.DBus.Error.PropertyReadOnly",
                "exported properties are read-only".into(),
            ));
        }
        _ => return Err(("org.freedesktop.DBus.Error.UnknownMethod", member.into())),
    };
    if interface != spec.interface {
        return Err(("org.freedesktop.DBus.Error.UnknownInterface", interface));
    }
    let properties = properties.lock().expect("dbus property registry poisoned");
    let (signature, value) = if let Some(name) = property {
        let value = properties
            .get(&name)
            .ok_or(("org.freedesktop.DBus.Error.UnknownProperty", name))?;
        ("v", serde_json::to_value(value).unwrap())
    } else {
        ("a{sv}", serde_json::to_value(&*properties).unwrap())
    };
    encode_body(signature, &[value])
        .map_err(invalid)?
        .ok_or_else(|| invalid("empty property reply".into()))
}

fn elide_large_arrays(value: &mut Json) {
    match value {
        Json::Array(items) if items.len() > MAX_ARRAY_ITEMS => {
            *value = serde_json::json!({ "elided": items.len() });
        }
        Json::Array(items) => items.iter_mut().for_each(elide_large_arrays),
        Json::Object(entries) => entries.values_mut().for_each(elide_large_arrays),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command as Process, Stdio};

    struct Bus(Child, String);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn bus() -> Bus {
        let mut child = Process::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is required for D-Bus service tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Bus(child, address.trim().to_owned())
    }

    fn spec() -> ServiceSpec {
        serde_json::from_value(serde_json::json!({
            "name": "org.mesh.Served",
            "path": "/org/mesh/Served",
            "interface": "org.mesh.Served",
            "methods": {
                "Add": { "in": "uu", "out": "u" },
                "Hints": { "in": "asa{sv}" },
                "Quiet": {}
            },
            "signals": { "Changed": "s" },
            "properties": { "Enabled": { "signature": "b", "value": false } }
        }))
        .unwrap()
    }

    async fn next_line(state: &Arc<StreamState>) -> Json {
        loop {
            let events = state.drain_events();
            for event in events {
                match event.kind {
                    StreamEventKind::Line(line) => return serde_json::from_str(&line).unwrap(),
                    StreamEventKind::Failed(message) => panic!("service failed: {message}"),
                    _ => {}
                }
            }
            tokio::time::timeout(Duration::from_secs(3), state.wait_for_event())
                .await
                .expect("timed out waiting for a service record");
        }
    }

    #[tokio::test]
    async fn exported_methods_reach_the_provider_and_replies_reach_the_caller() {
        let bus = bus();
        let state = StreamState::new();
        let services = Arc::new(DbusServices::default());
        let handle =
            spawn_service(&state, &services, BusTarget::Address(bus.1.clone()), spec()).unwrap();

        let client = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        // Wait until the name is owned before calling it.
        let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !dbus
            .name_has_owner("org.mesh.Served".try_into().unwrap())
            .await
            .unwrap()
        {
            assert!(Instant::now() < deadline, "service never owned its name");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let enabled = || {
            client.call_method(
                Some("org.mesh.Served"),
                "/org/mesh/Served",
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.mesh.Served", "Enabled"),
            )
        };
        let reply = enabled().await.unwrap();
        assert!(
            !bool::try_from(
                reply
                    .body()
                    .deserialize::<zbus::zvariant::OwnedValue>()
                    .unwrap()
            )
            .unwrap()
        );
        services
            .set_property(handle.id(), "Enabled", Json::Bool(true))
            .unwrap();
        assert!(
            services
                .set_property(handle.id(), "Enabled", Json::String("wrong type".into()))
                .is_err()
        );
        let reply = enabled().await.unwrap();
        assert!(
            bool::try_from(
                reply
                    .body()
                    .deserialize::<zbus::zvariant::OwnedValue>()
                    .unwrap()
            )
            .unwrap()
        );
        let denied = client
            .call_method(
                Some("org.mesh.Served"),
                "/org/mesh/Served",
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.mesh.Other", "Enabled"),
            )
            .await
            .unwrap_err();
        assert!(denied.to_string().contains("UnknownInterface"));

        let mut signals = zbus::MessageStream::for_match_rule(
            zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface("org.mesh.Served")
                .unwrap()
                .build(),
            &client,
            None,
        )
        .await
        .unwrap();

        let call = tokio::spawn({
            let client = client.clone();
            async move {
                client
                    .call_method(
                        Some("org.mesh.Served"),
                        "/org/mesh/Served",
                        Some("org.mesh.Served"),
                        "Add",
                        &(2u32, 3u32),
                    )
                    .await
                    .map(|reply| reply.body().deserialize::<u32>().unwrap())
            }
        });
        let record = next_line(&state).await;
        assert_eq!(record["type"], "call");
        assert_eq!(record["member"], "Add");
        assert_eq!(
            record["body"][1],
            serde_json::json!({"signature": "u", "value": 3})
        );
        let id = record["id"].as_u64().unwrap();
        services
            .reply(handle.id(), id, &[serde_json::json!(5)])
            .unwrap();
        assert_eq!(call.await.unwrap().unwrap(), 5);
        assert!(
            services
                .reply(handle.id(), id, &[serde_json::json!(5)])
                .is_err(),
            "a call is answered once"
        );

        let hints = tokio::spawn({
            let client = client.clone();
            async move {
                let mut map = std::collections::HashMap::new();
                map.insert("urgency", zbus::zvariant::Value::U8(2));
                map.insert("image-path", zbus::zvariant::Value::from("/tmp/x.png"));
                client
                    .call_method(
                        Some("org.mesh.Served"),
                        "/org/mesh/Served",
                        Some("org.mesh.Served"),
                        "Hints",
                        &(vec!["default", "Open"], map),
                    )
                    .await
                    .map(|_| ())
            }
        });
        let record = next_line(&state).await;
        assert_eq!(
            record["body"][0],
            serde_json::json!({"signature": "as", "value": ["default", "Open"]})
        );
        assert_eq!(
            record["body"][1]["value"]["urgency"],
            serde_json::json!({"signature": "y", "value": 2})
        );
        services
            .reply(handle.id(), record["id"].as_u64().unwrap(), &[])
            .unwrap();
        hints.await.unwrap().unwrap();

        let wrong = client
            .call_method(
                Some("org.mesh.Served"),
                "/org/mesh/Served",
                Some("org.mesh.Served"),
                "Add",
                &"nope",
            )
            .await;
        assert!(matches!(wrong, Err(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.DBus.Error.InvalidArgs"));

        let introspection = client
            .call_method(
                Some("org.mesh.Served"),
                "/org/mesh/Served",
                Some("org.freedesktop.DBus.Introspectable"),
                "Introspect",
                &(),
            )
            .await
            .unwrap();
        let xml = introspection.body().deserialize::<String>().unwrap();
        assert!(xml.contains("<method name=\"Add\">"), "{xml}");

        services
            .emit(handle.id(), "Changed", &[serde_json::json!("hello")])
            .unwrap();
        let signal = tokio::time::timeout(Duration::from_secs(3), signals.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(signal.body().deserialize::<String>().unwrap(), "hello");
        assert!(services.emit(handle.id(), "Undeclared", &[]).is_err());

        state.shutdown().await;
        assert!(services.services.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_taken_name_fails_the_service() {
        let bus = bus();
        let owner = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        owner.request_name("org.mesh.Served").await.unwrap();
        let state = StreamState::new();
        let services = Arc::new(DbusServices::default());
        spawn_service(&state, &services, BusTarget::Address(bus.1.clone()), spec()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let failure = loop {
            let failed = state
                .drain_events()
                .into_iter()
                .find_map(|event| match event.kind {
                    StreamEventKind::Failed(message) => Some(message),
                    _ => None,
                });
            if let Some(message) = failed {
                break message;
            }
            assert!(Instant::now() < deadline, "service never failed");
            let _ = tokio::time::timeout(Duration::from_millis(200), state.wait_for_event()).await;
        };
        assert!(failure.contains("owned by another client"), "{failure}");
    }

    #[test]
    fn large_arrays_are_elided_from_call_records() {
        let mut value = serde_json::json!({
            "pixels": vec![0; MAX_ARRAY_ITEMS + 1],
            "small": [1, 2, 3],
        });
        elide_large_arrays(&mut value);
        assert_eq!(
            value["pixels"],
            serde_json::json!({ "elided": MAX_ARRAY_ITEMS + 1 })
        );
        assert_eq!(value["small"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn specs_reject_malformed_names_and_signatures() {
        for (field, value) in [
            ("name", serde_json::json!("not a name")),
            ("path", serde_json::json!("relative")),
            ("methods", serde_json::json!({"Add": {"in": "(u"}})),
            ("signals", serde_json::json!({"bad-name": "s"})),
        ] {
            let mut raw = serde_json::to_value(serde_json::json!({
                "name": "org.mesh.Served", "path": "/org/mesh/Served", "interface": "org.mesh.Served",
            }))
            .unwrap();
            raw[field] = value;
            let spec: ServiceSpec = serde_json::from_value(raw).unwrap();
            assert!(spec.validate().is_err(), "{field} must be rejected");
        }
    }
}
