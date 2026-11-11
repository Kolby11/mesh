use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value as Json, json};
use zbus::Connection;
use zbus::zvariant::{Array, Dict, ObjectPath, Signature, Structure, StructureBuilder, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MethodCall {
    pub destination: String,
    pub path: String,
    pub interface: String,
    pub member: String,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub args: Vec<Json>,
}

/// Encode `call.args` against `call.signature`. Variants use the same
/// `{signature, value}` JSON shape that D-Bus signal records carry.
pub(super) fn encode_args(call: &MethodCall) -> Result<Option<Structure<'static>>, String> {
    encode_body(&call.signature, &call.args)
}

/// Encode a message body: `args` against the concatenated complete types in
/// `signature`. An empty signature is an empty body.
pub(super) fn encode_body(
    signature: &str,
    args: &[Json],
) -> Result<Option<Structure<'static>>, String> {
    if signature.is_empty() && !args.is_empty() {
        return Err("D-Bus arguments require a signature".into());
    }
    let fields = body_types(signature)?;
    if fields.len() != args.len() {
        return Err(format!(
            "D-Bus signature {signature:?} expects {} argument(s), got {}",
            fields.len(),
            args.len()
        ));
    }
    if fields.is_empty() {
        return Ok(None);
    }
    let mut body = StructureBuilder::new();
    for (field, arg) in fields.iter().zip(args) {
        body = body.append_field(encode(field, arg)?);
    }
    body.build().map(Some).map_err(|error| error.to_string())
}

/// Split a body signature into its complete types.
pub(super) fn body_types(signature: &str) -> Result<Vec<Signature>, String> {
    if signature.is_empty() {
        return Ok(Vec::new());
    }
    // Wrapping keeps one-argument and multi-argument signatures uniform.
    let wrapped = Signature::try_from(format!("({signature})").as_str())
        .map_err(|error| format!("invalid D-Bus signature {signature:?}: {error}"))?;
    let Signature::Structure(fields) = &wrapped else {
        unreachable!("parenthesized signature parses as a structure");
    };
    Ok(fields.iter().cloned().collect())
}

fn integer(value: &Json, signature: &Signature) -> Result<i128, String> {
    let number = value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))
        .or_else(|| {
            value
                .as_f64()
                .filter(|float| float.fract() == 0.0 && float.abs() < 2f64.powi(64))
                .map(|float| float as i128)
        });
    number.ok_or_else(|| format!("expected an integer for D-Bus type {signature}, got {value}"))
}

fn integer_value(signature: &Signature, number: i128) -> Result<Value<'static>, String> {
    let out_of_range = || format!("{number} is out of range for D-Bus type {signature}");
    Ok(match signature {
        Signature::U8 => Value::U8(u8::try_from(number).map_err(|_| out_of_range())?),
        Signature::I16 => Value::I16(i16::try_from(number).map_err(|_| out_of_range())?),
        Signature::U16 => Value::U16(u16::try_from(number).map_err(|_| out_of_range())?),
        Signature::I32 => Value::I32(i32::try_from(number).map_err(|_| out_of_range())?),
        Signature::U32 => Value::U32(u32::try_from(number).map_err(|_| out_of_range())?),
        Signature::I64 => Value::I64(i64::try_from(number).map_err(|_| out_of_range())?),
        Signature::U64 => Value::U64(u64::try_from(number).map_err(|_| out_of_range())?),
        _ => unreachable!("integer_value is called only for integer signatures"),
    })
}

fn string<'a>(value: &'a Json, signature: &Signature) -> Result<&'a str, String> {
    value
        .as_str()
        .ok_or_else(|| format!("expected a string for D-Bus type {signature}, got {value}"))
}

fn encode(signature: &Signature, value: &Json) -> Result<Value<'static>, String> {
    Ok(match signature {
        Signature::U8
        | Signature::I16
        | Signature::U16
        | Signature::I32
        | Signature::U32
        | Signature::I64
        | Signature::U64 => integer_value(signature, integer(value, signature)?)?,
        Signature::Bool => Value::Bool(
            value
                .as_bool()
                .ok_or_else(|| format!("expected a boolean for D-Bus type b, got {value}"))?,
        ),
        Signature::F64 => Value::F64(
            value
                .as_f64()
                .ok_or_else(|| format!("expected a number for D-Bus type d, got {value}"))?,
        ),
        Signature::Str => Value::from(string(value, signature)?.to_owned()),
        Signature::ObjectPath => Value::ObjectPath(
            ObjectPath::try_from(string(value, signature)?.to_owned())
                .map_err(|error| error.to_string())?,
        ),
        Signature::Signature => Value::Signature(
            Signature::try_from(string(value, signature)?).map_err(|error| error.to_string())?,
        ),
        Signature::Variant => {
            let object = value.as_object().filter(|object| object.len() == 2);
            let (Some(inner), Some(inner_value)) = (
                object.and_then(|object| object.get("signature")?.as_str()),
                object.and_then(|object| object.get("value")),
            ) else {
                return Err(format!(
                    "a D-Bus variant is {{\"signature\": ..., \"value\": ...}}, got {value}"
                ));
            };
            let inner = Signature::try_from(inner).map_err(|error| error.to_string())?;
            if matches!(inner, Signature::Unit) {
                return Err("a D-Bus variant needs a non-empty signature".into());
            }
            Value::Value(Box::new(encode(&inner, inner_value)?))
        }
        Signature::Array(child) => {
            let items = value
                .as_array()
                .ok_or_else(|| format!("expected an array for D-Bus type {signature}"))?;
            let mut array = Array::new(child);
            for item in items {
                array
                    .append(encode(child, item)?)
                    .map_err(|error| error.to_string())?;
            }
            Value::Array(array)
        }
        Signature::Dict { key, value: entry } => {
            let object = value
                .as_object()
                .ok_or_else(|| format!("expected an object for D-Bus type {signature}"))?;
            let mut dict = Dict::new(key, entry);
            for (name, item) in object {
                let key_value = encode_key(key, name)?;
                dict.append(key_value, encode(entry, item)?)
                    .map_err(|error| error.to_string())?;
            }
            Value::Dict(dict)
        }
        Signature::Structure(fields) => {
            let items = value
                .as_array()
                .ok_or_else(|| format!("expected an array for D-Bus type {signature}"))?;
            let fields = fields.iter().collect::<Vec<_>>();
            if fields.len() != items.len() {
                return Err(format!(
                    "D-Bus type {signature} expects {} field(s), got {}",
                    fields.len(),
                    items.len()
                ));
            }
            let mut builder = StructureBuilder::new();
            for (field, item) in fields.into_iter().zip(items) {
                builder = builder.append_field(encode(field, item)?);
            }
            Value::Structure(builder.build().map_err(|error| error.to_string())?)
        }
        _ => {
            return Err(format!(
                "D-Bus type {signature} is not supported in arguments"
            ));
        }
    })
}

/// JSON object keys are strings; basic-typed dictionary keys parse from them.
fn encode_key(signature: &Signature, key: &str) -> Result<Value<'static>, String> {
    match signature {
        Signature::Str | Signature::ObjectPath | Signature::Signature => {
            encode(signature, &Json::String(key.to_owned()))
        }
        Signature::Bool => match key {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(format!("invalid boolean dictionary key {key:?}")),
        },
        Signature::F64 => key
            .parse::<f64>()
            .map(Value::F64)
            .map_err(|_| format!("invalid number dictionary key {key:?}")),
        Signature::U8
        | Signature::I16
        | Signature::U16
        | Signature::I32
        | Signature::U32
        | Signature::I64
        | Signature::U64 => integer_value(
            signature,
            key.parse::<i128>()
                .map_err(|_| format!("invalid integer dictionary key {key:?}"))?,
        ),
        _ => Err(format!("D-Bus type {signature} cannot be a dictionary key")),
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum BusTarget {
    System,
    Session,
    #[cfg(test)]
    Address(String),
}

impl BusTarget {
    pub(super) fn named(bus: &str) -> Option<Self> {
        match bus {
            "system" => Some(Self::System),
            "session" => Some(Self::Session),
            _ => None,
        }
    }

    pub(super) async fn connect(&self) -> zbus::Result<Connection> {
        match self {
            Self::System => Connection::system().await,
            Self::Session => Connection::session().await,
            #[cfg(test)]
            Self::Address(address) => {
                zbus::connection::Builder::address(address.as_str())?
                    .build()
                    .await
            }
        }
    }
}

type CallOutcome = Result<(String, Vec<Json>), (String, String)>;

struct CallJob {
    bus: BusTarget,
    destination: String,
    path: String,
    interface: String,
    member: String,
    args: Option<Structure<'static>>,
    timeout: Duration,
    limit: usize,
    reply: std::sync::mpsc::SyncSender<CallOutcome>,
}

type Connections = Arc<Mutex<HashMap<BusTarget, Connection>>>;

/// One thread owns a connection per bus for every method call in the process.
/// Authenticating a fresh connection costs far more than the call it carries,
/// and providers issue calls in bursts (one per device or access point).
fn call_service() -> Option<&'static tokio::sync::mpsc::UnboundedSender<CallJob>> {
    static SERVICE: OnceLock<Option<tokio::sync::mpsc::UnboundedSender<CallJob>>> =
        OnceLock::new();
    SERVICE
        .get_or_init(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            let (jobs, mut pending) = tokio::sync::mpsc::unbounded_channel::<CallJob>();
            std::thread::Builder::new()
                .name("mesh-dbus-call".into())
                .spawn(move || {
                    let connections = Connections::default();
                    runtime.block_on(async move {
                        while let Some(job) = pending.recv().await {
                            tokio::spawn(run_job(Arc::clone(&connections), job));
                        }
                    });
                })
                .ok()?;
            Some(jobs)
        })
        .as_ref()
}

async fn run_job(connections: Connections, job: CallJob) {
    let timeout = job.timeout;
    let outcome = tokio::time::timeout(timeout, perform(&connections, &job))
        .await
        .unwrap_or_else(|_| Err(timed_out(timeout)));
    let _ = job.reply.send(outcome);
}

fn timed_out(timeout: Duration) -> (String, String) {
    (
        String::new(),
        format!("D-Bus call timed out after {timeout:?}"),
    )
}

/// Perform one method call and return the reply record. Every failure,
/// including D-Bus error replies, is a record with `success = false`;
/// malformed calls are reported the same way, before any connection is used.
pub(super) fn call(bus: &BusTarget, call: &MethodCall, timeout: Duration, limit: usize) -> Json {
    let outcome = submit(bus, call, timeout, limit);
    match outcome {
        Ok((signature, body)) => json!({
            "success": true, "signature": signature, "body": body, "error": "", "error_name": "",
        }),
        Err((error_name, error)) => json!({
            "success": false, "signature": "", "body": [], "error": error, "error_name": error_name,
        }),
    }
}

fn submit(bus: &BusTarget, call: &MethodCall, timeout: Duration, limit: usize) -> CallOutcome {
    let plain = |error: String| (String::new(), error);
    let args = encode_args(call).map_err(plain)?;
    let (reply, outcome) = std::sync::mpsc::sync_channel(1);
    let job = CallJob {
        bus: bus.clone(),
        destination: call.destination.clone(),
        path: call.path.clone(),
        interface: call.interface.clone(),
        member: call.member.clone(),
        args,
        timeout,
        limit,
        reply,
    };
    call_service()
        .and_then(|service| service.send(job).ok())
        .ok_or_else(|| plain("D-Bus call worker is unavailable".into()))?;
    // The worker enforces the deadline; the margin only covers a worker that
    // stopped answering.
    outcome
        .recv_timeout(timeout + Duration::from_secs(1))
        .unwrap_or_else(|_| Err(timed_out(timeout)))
}

fn cached_connection(connections: &Connections, bus: &BusTarget) -> Option<Connection> {
    connections
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(bus)
        .cloned()
}

fn forget_connection(connections: &Connections, bus: &BusTarget) {
    connections
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(bus);
}

async fn perform(connections: &Connections, job: &CallJob) -> CallOutcome {
    let plain = |error: String| (String::new(), error);
    let mut reused = true;
    let mut connection = cached_connection(connections, &job.bus);
    let reply = loop {
        let active = match connection.take() {
            Some(active) => active,
            None => {
                reused = false;
                let fresh = job
                    .bus
                    .connect()
                    .await
                    .map_err(|error| plain(format!("D-Bus connection failed: {error}")))?;
                connections
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(job.bus.clone(), fresh.clone());
                fresh
            }
        };
        let reply = match &job.args {
            Some(args) => {
                active
                    .call_method(
                        Some(job.destination.as_str()),
                        job.path.as_str(),
                        Some(job.interface.as_str()),
                        job.member.as_str(),
                        args,
                    )
                    .await
            }
            None => {
                active
                    .call_method(
                        Some(job.destination.as_str()),
                        job.path.as_str(),
                        Some(job.interface.as_str()),
                        job.member.as_str(),
                        &(),
                    )
                    .await
            }
        };
        match reply {
            // A cached connection whose bus went away never delivered the
            // call, so it is safe to repeat on a fresh one.
            Err(zbus::Error::InputOutput(_)) if reused => {
                forget_connection(connections, &job.bus);
            }
            Err(error @ zbus::Error::InputOutput(_)) => {
                forget_connection(connections, &job.bus);
                break Err(error);
            }
            reply => break reply,
        }
    };
    let reply = reply.map_err(|error| match error {
        zbus::Error::MethodError(name, detail, _) => {
            (name.to_string(), detail.unwrap_or_else(|| name.to_string()))
        }
        error => plain(error.to_string()),
    })?;
    let body = reply.body();
    if body.len() > job.limit {
        return Err(plain(format!("D-Bus reply exceeds {} bytes", job.limit)));
    }
    let fields = if body.is_empty() {
        Vec::new()
    } else {
        body.deserialize::<Structure<'_>>()
            .map_err(|error| plain(error.to_string()))?
            .into_fields()
            .into_iter()
            .map(|field| serde_json::to_value(field).map_err(|error| plain(error.to_string())))
            .collect::<Result<_, _>>()?
    };
    Ok((body.signature().to_string(), fields))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};

    struct Bus(Child, String);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn bus() -> Bus {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is required for D-Bus call tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Bus(child, address.trim().to_owned())
    }

    /// Serve `org.mesh.Test`: `Echo` returns its arguments, `Fail` replies
    /// with a D-Bus error, and `Slow` never replies.
    fn serve(runtime: &tokio::runtime::Runtime, address: &str) {
        let address = address.to_owned();
        runtime.block_on(async {
            let server = zbus::connection::Builder::address(address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap();
            server.request_name("org.mesh.Test").await.unwrap();
            let mut calls = zbus::MessageStream::from(&server);
            tokio::spawn(async move {
                while let Some(Ok(message)) = calls.next().await {
                    let header = message.header();
                    if header.message_type() != zbus::message::Type::MethodCall {
                        continue;
                    }
                    match header.member().map(|member| member.as_str()) {
                        Some("Echo") if message.body().is_empty() => {
                            server.reply(&header, &()).await.unwrap();
                        }
                        Some("Echo") => {
                            let body = message.body();
                            let fields = body.deserialize::<Structure<'_>>().unwrap();
                            server.reply(&header, &fields).await.unwrap();
                        }
                        Some("Fail") => {
                            server
                                .reply_error(&header, "org.mesh.Error.Nope", &"no such thing")
                                .await
                                .unwrap();
                        }
                        _ => {}
                    }
                }
            });
        });
    }

    fn method(member: &str, signature: &str, args: Json) -> MethodCall {
        MethodCall {
            destination: "org.mesh.Test".into(),
            path: "/org/mesh/Test".into(),
            interface: "org.mesh.Test".into(),
            member: member.into(),
            signature: signature.into(),
            args: serde_json::from_value(args).unwrap(),
        }
    }

    #[test]
    fn calls_encode_typed_arguments_and_report_replies_and_errors() {
        let bus = bus();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        serve(&runtime, &bus.1);
        let target = BusTarget::Address(bus.1.clone());
        let timeout = Duration::from_secs(3);

        // Luau numbers arrive as floats; integral values encode as integers.
        let reply = call(
            &target,
            &method(
                "Echo",
                "sa{sv}(iu)aty",
                json!([
                    "x",
                    {"k": {"signature": "s", "value": "v"}},
                    [-1.0, 2],
                    [18446744073709551615u64],
                    255.0
                ]),
            ),
            timeout,
            65536,
        );
        assert_eq!(reply["success"], true, "{reply}");
        // Like signal records, a multi-argument body reports a structure signature.
        assert_eq!(reply["signature"], "(sa{sv}(iu)aty)");
        assert_eq!(reply["body"].as_array().unwrap().len(), 5);
        assert_eq!(reply["body"][0], json!({"signature": "s", "value": "x"}));
        assert_eq!(reply["body"][4], json!({"signature": "y", "value": 255}));

        let variant = call(
            &target,
            &method("Echo", "v", json!([{"signature": "d", "value": 1.5}])),
            timeout,
            65536,
        );
        assert_eq!(
            variant["body"][0],
            json!({"signature": "v", "value": {"signature": "d", "value": 1.5}})
        );

        let empty = call(&target, &method("Echo", "", json!([])), timeout, 65536);
        assert_eq!(empty["success"], true, "{empty}");
        assert_eq!(empty["body"], json!([]));

        let failed = call(&target, &method("Fail", "", json!([])), timeout, 65536);
        assert_eq!(failed["success"], false);
        assert_eq!(failed["error_name"], "org.mesh.Error.Nope");
        assert_eq!(failed["error"], "no such thing");

        let slow = call(
            &target,
            &method("Slow", "", json!([])),
            Duration::from_millis(200),
            65536,
        );
        assert_eq!(slow["success"], false);
        assert!(slow["error"].as_str().unwrap().contains("timed out"));

        let large = call(
            &target,
            &method("Echo", "s", json!(["x".repeat(64)])),
            timeout,
            16,
        );
        assert!(
            large["error"]
                .as_str()
                .unwrap()
                .contains("exceeds 16 bytes")
        );
    }

    // cargo test -p mesh-core-scripting --release -- dbus_call_burst_profile --ignored --nocapture
    #[test]
    #[ignore = "release-only D-Bus call burst benchmark"]
    fn dbus_call_burst_profile() {
        let bus = bus();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        serve(&runtime, &bus.1);
        let target = BusTarget::Address(bus.1.clone());
        let timeout = Duration::from_secs(3);
        let echo = method("Echo", "s", json!(["x"]));
        // The former path: a thread, a runtime and an authenticated
        // connection for every call.
        let private = |address: &str| {
            std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap();
                        runtime.block_on(async {
                            let connection = zbus::connection::Builder::address(address)
                                .unwrap()
                                .build()
                                .await
                                .unwrap();
                            connection
                                .call_method(
                                    Some("org.mesh.Test"),
                                    "/org/mesh/Test",
                                    Some("org.mesh.Test"),
                                    "Echo",
                                    &("x",),
                                )
                                .await
                                .unwrap();
                        })
                    })
                    .join()
                    .unwrap()
            })
        };
        assert_eq!(call(&target, &echo, timeout, 65536)["success"], true);
        for run in 0..3 {
            let started = std::time::Instant::now();
            for _ in 0..40 {
                private(&bus.1);
            }
            let before = started.elapsed();
            let started = std::time::Instant::now();
            for _ in 0..40 {
                assert_eq!(call(&target, &echo, timeout, 65536)["success"], true);
            }
            let after = started.elapsed();
            let ratio = before.as_secs_f64() / after.as_secs_f64();
            eprintln!(
                "dbus call burst: calls=40 run={run} private={before:?} shared={after:?} ratio={ratio:.1}"
            );
            assert!(ratio > 2.0, "dbus_call_shared_connection_speedup must exceed 2x");
        }
    }

    #[test]
    fn malformed_arguments_fail_before_connecting() {
        for (signature, args, expected) in [
            ("s", json!([]), "expects 1 argument"),
            ("", json!(["x"]), "require a signature"),
            ("y", json!([256]), "out of range"),
            ("u", json!([1.5]), "expected an integer"),
            ("v", json!(["bare"]), "a D-Bus variant is"),
            (
                "a{is}",
                json!([{"one": "x"}]),
                "invalid integer dictionary key",
            ),
            ("h", json!([0]), "not supported"),
            ("(s", json!([]), "invalid D-Bus signature"),
        ] {
            let error = encode_args(&method("Echo", signature, args))
                .err()
                .unwrap_or_else(|| panic!("{signature} must be rejected"));
            assert!(error.contains(expected), "{signature}: {error}");
        }
    }
}
