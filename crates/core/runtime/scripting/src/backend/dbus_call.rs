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

/// Perform one method call on a private connection and return the reply
/// record. Every failure, including D-Bus error replies, is a record with
/// `success = false`; malformed calls are reported the same way.
pub(super) fn call(bus: &BusTarget, call: &MethodCall, timeout: Duration, limit: usize) -> Json {
    let outcome = std::thread::scope(|scope| {
        // A dedicated thread keeps this safe from callers that already run
        // inside a Tokio runtime.
        scope
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| (String::new(), error.to_string()))?;
                runtime.block_on(async {
                    tokio::time::timeout(timeout, perform(bus, call, limit))
                        .await
                        .unwrap_or_else(|_| {
                            Err((
                                String::new(),
                                format!("D-Bus call timed out after {timeout:?}"),
                            ))
                        })
                })
            })
            .join()
            .unwrap_or_else(|_| Err((String::new(), "D-Bus call worker panicked".into())))
    });
    match outcome {
        Ok((signature, body)) => json!({
            "success": true, "signature": signature, "body": body, "error": "", "error_name": "",
        }),
        Err((error_name, error)) => json!({
            "success": false, "signature": "", "body": [], "error": error, "error_name": error_name,
        }),
    }
}

async fn perform(
    bus: &BusTarget,
    call: &MethodCall,
    limit: usize,
) -> Result<(String, Vec<Json>), (String, String)> {
    let plain = |error: String| (String::new(), error);
    let args = encode_args(call).map_err(plain)?;
    let connection = bus
        .connect()
        .await
        .map_err(|error| plain(format!("D-Bus connection failed: {error}")))?;
    let reply = match &args {
        Some(args) => {
            connection
                .call_method(
                    Some(call.destination.as_str()),
                    call.path.as_str(),
                    Some(call.interface.as_str()),
                    call.member.as_str(),
                    args,
                )
                .await
        }
        None => {
            connection
                .call_method(
                    Some(call.destination.as_str()),
                    call.path.as_str(),
                    Some(call.interface.as_str()),
                    call.member.as_str(),
                    &(),
                )
                .await
        }
    };
    let reply = reply.map_err(|error| match error {
        zbus::Error::MethodError(name, detail, _) => {
            (name.to_string(), detail.unwrap_or_else(|| name.to_string()))
        }
        error => plain(error.to_string()),
    })?;
    let body = reply.body();
    if body.len() > limit {
        return Err(plain(format!("D-Bus reply exceeds {limit} bytes")));
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
