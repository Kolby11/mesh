use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Deserialize;
use tokio::io::unix::AsyncFd;
use zbus::{Connection, MatchRule, MessageStream};

use super::exec_stream::{StreamEvent, StreamEventKind, StreamHandle, StreamState};
use super::host_sources::{finish, spawn_source};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SignalFilter {
    pub sender: String,
    pub path_namespace: String,
    pub interface: Option<String>,
    pub member: Option<String>,
}

impl SignalFilter {
    fn rule(&self) -> zbus::Result<zbus::OwnedMatchRule> {
        let mut rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(self.sender.as_str())?
            .path_namespace(self.path_namespace.as_str())?;
        if let Some(interface) = &self.interface {
            rule = rule.interface(interface.as_str())?;
        }
        if let Some(member) = &self.member {
            rule = rule.member(member.as_str())?;
        }
        Ok(rule.build().to_owned().into())
    }
}

pub(super) fn spawn_dbus(
    state: &Arc<StreamState>,
    bus: &str,
    filter: SignalFilter,
) -> std::io::Result<StreamHandle> {
    let rule = filter.rule().map_err(std::io::Error::other)?;
    let system = match bus {
        "system" => true,
        "session" => false,
        _ => return Err(std::io::Error::other("bus must be system or session")),
    };
    let label = format!("dbus:{bus}:{}", filter.sender);
    spawn_source(
        state,
        label,
        &filter.path_namespace,
        move |state, stream, mut stop| async move {
            // Cancellation covers connection/authentication and AddMatch as well as delivery.
            let result = tokio::select! {
                _ = &mut stop => Ok(()),
                result = async {
                    let connection = if system { Connection::system().await? } else { Connection::session().await? };
                    run_signals(&state, &stream, &connection, rule).await
                } => result,
            };
            finish(
                &state,
                stream,
                result
                    .err()
                    .map(|error| format!("D-Bus subscription failed: {error}")),
            );
        },
    )
}

async fn run_signals(
    state: &Arc<StreamState>,
    stream: &StreamHandle,
    connection: &Connection,
    rule: zbus::OwnedMatchRule,
) -> zbus::Result<()> {
    let mut messages = MessageStream::for_match_rule(rule, connection, Some(64)).await?;
    state.push_event(StreamEvent {
        stream: stream.clone(),
        kind: StreamEventKind::Started,
    });
    while let Some(message) = messages.next().await {
        let message = message?;
        let header = message.header();
        // The host transports protocol data; interpretation belongs to the provider.
        let body = message.body();
        if body.len() > super::exec_stream::MAX_STREAM_LINE_BYTES {
            return Err(zbus::Error::Failure(
                "D-Bus signal exceeds stream record limit".into(),
            ));
        }
        let fields = if body.is_empty() {
            Vec::new()
        } else {
            body.deserialize::<zbus::zvariant::Structure<'_>>()?
                .into_fields()
        };
        let line = serde_json::to_string(&serde_json::json!({
            "path": header.path().map(|value| value.as_str()),
            "interface": header.interface().map(|value| value.as_str()),
            "member": header.member().map(|value| value.as_str()),
            "signature": body.signature().to_string(),
            "body": fields,
        }))
        .map_err(|error| zbus::Error::Failure(error.to_string()))?;
        if line.len() > super::exec_stream::MAX_STREAM_LINE_BYTES {
            return Err(zbus::Error::Failure(
                "D-Bus signal exceeds stream record limit".into(),
            ));
        }
        state.push_event(StreamEvent {
            stream: stream.clone(),
            kind: StreamEventKind::Line(line),
        });
    }
    Err(zbus::Error::Failure("D-Bus connection closed".into()))
}

pub(super) fn spawn_netlink(
    state: &Arc<StreamState>,
    groups: Vec<String>,
) -> std::io::Result<StreamHandle> {
    let mut mask = 0;
    for group in &groups {
        mask |= match group.as_str() {
            "link" => libc::RTMGRP_LINK,
            "ipv4-address" => libc::RTMGRP_IPV4_IFADDR,
            "ipv6-address" => libc::RTMGRP_IPV6_IFADDR,
            "ipv4-route" => libc::RTMGRP_IPV4_ROUTE,
            "ipv6-route" => libc::RTMGRP_IPV6_ROUTE,
            _ => {
                return Err(std::io::Error::other(format!(
                    "unsupported route group: {group}"
                )));
            }
        };
    }
    if mask == 0 {
        return Err(std::io::Error::other(
            "at least one route group is required",
        ));
    }
    let raw = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            libc::NETLINK_ROUTE,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: socket returned a fresh descriptor; zero is the kernel-assigned port ID.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut address: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    address.nl_family = libc::AF_NETLINK as u16;
    address.nl_groups = mask as u32;
    if unsafe {
        libc::bind(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_nl).cast(),
            std::mem::size_of_val(&address) as u32,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    spawn_source(
        state,
        "netlink:route".into(),
        &groups.join(","),
        move |state, stream, mut stop| async move {
            let result = async {
                let fd = AsyncFd::new(fd)?;
                state.push_event(StreamEvent {
                    stream: stream.clone(),
                    kind: StreamEventKind::Started,
                });
                let mut buffer = [0u8; 65536];
                loop {
                    let mut ready = fd.readable().await?;
                    let result = ready.try_io(|fd| {
                        let mut sender: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
                        let mut len = std::mem::size_of_val(&sender) as libc::socklen_t;
                        let count = unsafe {
                            libc::recvfrom(
                                fd.as_raw_fd(),
                                buffer.as_mut_ptr().cast(),
                                buffer.len(),
                                libc::MSG_TRUNC,
                                (&mut sender as *mut libc::sockaddr_nl).cast(),
                                &mut len,
                            )
                        };
                        if count < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        if sender.nl_pid != 0 {
                            return Ok(false);
                        }
                        if count as usize > buffer.len() {
                            return Err(std::io::Error::other(
                                "route event truncated; resync required",
                            ));
                        }
                        Ok(true)
                    });
                    match result {
                        Ok(Ok(true)) => state.push_event(StreamEvent {
                            stream: stream.clone(),
                            kind: StreamEventKind::Line("changed".into()),
                        }),
                        Ok(Ok(false)) | Err(_) => {}
                        Ok(Err(error)) => return Err(error),
                    }
                }
                #[allow(unreachable_code)]
                Ok::<(), std::io::Error>(())
            };
            let failure = tokio::select! { _ = &mut stop => None, result = result => result.err().map(|error| error.to_string()) };
            finish(&state, stream, failure);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        time::Duration,
    };

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
            .expect("dbus-daemon is required for native subscription tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Bus(child, address.trim().to_owned())
    }
    async fn connect(bus: &Bus) -> Connection {
        zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
    async fn next(state: &StreamState) -> Vec<StreamEvent> {
        tokio::time::timeout(Duration::from_secs(3), state.wait_for_event())
            .await
            .unwrap();
        state.drain_events()
    }
    fn filter() -> SignalFilter {
        SignalFilter {
            sender: "org.mesh.Test".into(),
            path_namespace: "/org/mesh/Test".into(),
            interface: Some("org.mesh.Test".into()),
            member: Some("Changed".into()),
        }
    }

    #[tokio::test]
    async fn dbus_filters_signals_delivers_payload_and_shuts_down_without_helpers() {
        let bus = bus();
        let publisher = connect(&bus).await;
        publisher.request_name("org.mesh.Test").await.unwrap();
        let subscriber = connect(&bus).await;
        let state = StreamState::new();
        let rule = filter().rule().unwrap();
        let handle = spawn_source(&state, "test:dbus".into(), "/org/mesh/Test", move |state, stream, stop| async move {
            tokio::select! {
                _ = stop => {},
                result = run_signals(&state, &stream, &subscriber, rule) => { result.unwrap(); },
            }
            finish(&state, stream, None);
        }).unwrap();
        assert!(
            next(&state)
                .await
                .iter()
                .any(|event| matches!(event.kind, StreamEventKind::Started))
        );
        publisher
            .emit_signal(
                None::<&str>,
                "/org/mesh/Other",
                "org.mesh.Test",
                "Changed",
                &(1u32,),
            )
            .await
            .unwrap();
        publisher
            .emit_signal(
                None::<&str>,
                "/org/mesh/Test",
                "org.mesh.Test",
                "Other",
                &(2u32,),
            )
            .await
            .unwrap();
        publisher
            .emit_signal(
                None::<&str>,
                "/org/mesh/Test/Child",
                "org.mesh.Test",
                "Changed",
                &("hello", 42u32),
            )
            .await
            .unwrap();
        let events = next(&state).await;
        let lines: Vec<_> = events
            .iter()
            .filter_map(|event| match &event.kind {
                StreamEventKind::Line(line) => Some(line),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 1);
        let record: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(record["path"], "/org/mesh/Test/Child");
        assert_eq!(record["member"], "Changed");
        assert_eq!(record["signature"], "(su)");
        assert_eq!(record["body"][0]["value"], "hello");
        assert_eq!(record["body"][1]["value"], 42);
        publisher
            .emit_signal(
                None::<&str>,
                "/org/mesh/Test",
                "org.mesh.Test",
                "Changed",
                &"single",
            )
            .await
            .unwrap();
        let single = next(&state).await;
        let line = single
            .iter()
            .find_map(|event| match &event.kind {
                StreamEventKind::Line(line) => Some(line),
                _ => None,
            })
            .unwrap();
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["body"][0]["value"], "single");
        let properties =
            std::collections::HashMap::from([("Strength", zbus::zvariant::Value::from(70u8))]);
        publisher
            .emit_signal(
                None::<&str>,
                "/org/mesh/Test",
                "org.mesh.Test",
                "Changed",
                &("org.mesh.Test", properties, vec!["Connectivity"]),
            )
            .await
            .unwrap();
        let changed = next(&state).await;
        let line = changed
            .iter()
            .find_map(|event| match &event.kind {
                StreamEventKind::Line(line) => Some(line),
                _ => None,
            })
            .unwrap();
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(record["body"][1]["value"].get("Strength").is_some());
        assert_eq!(record["body"][2]["value"][0], "Connectivity");
        publisher
            .emit_signal(
                None::<&str>,
                "/org/mesh/Test",
                "org.mesh.Test",
                "Changed",
                &(),
            )
            .await
            .unwrap();
        assert!(
            next(&state)
                .await
                .iter()
                .any(|event| matches!(event.kind, StreamEventKind::Line(_)))
        );
        state.request_stop(handle.id());
        let events = next(&state).await;
        assert!(
            events
                .iter()
                .any(|event| matches!(event.kind, StreamEventKind::Exited(_)))
        );
        state.shutdown().await;
        assert_eq!(state.active_stream_count(), 0);
    }

    #[tokio::test]
    async fn route_subscription_is_idle_and_cancellable_and_validates_groups() {
        let state = StreamState::new();
        assert!(spawn_netlink(&state, vec![]).is_err());
        assert!(spawn_netlink(&state, vec!["arbitrary".into()]).is_err());
        spawn_netlink(&state, vec!["link".into()]).unwrap();
        assert!(
            next(&state)
                .await
                .iter()
                .any(|event| matches!(event.kind, StreamEventKind::Started))
        );
        tokio::time::timeout(Duration::from_secs(1), state.shutdown())
            .await
            .unwrap();
        assert_eq!(state.active_stream_count(), 0);
    }

    #[tokio::test]
    #[ignore = "native subscription idle/process workload"]
    async fn native_subscription_idle_profile() {
        let state = StreamState::new();
        let before = child_count();
        spawn_netlink(
            &state,
            vec!["link".into(), "ipv4-address".into(), "ipv6-address".into()],
        )
        .unwrap();
        next(&state).await;
        let helpers = child_count().saturating_sub(before);
        let mut monitor = Command::new("sleep").arg("3600").spawn().unwrap();
        let baseline = child_count().saturating_sub(before);
        monitor.kill().unwrap();
        monitor.wait().unwrap();
        assert_eq!(baseline, 1);
        println!(
            "idle subprocess baseline: {baseline} helper; native route subscription: {helpers} helpers"
        );
        let idle = tokio::time::timeout(Duration::from_millis(250), state.wait_for_event())
            .await
            .is_ok();
        println!(
            "MESH_PERF metric=native_subscription_helper_processes value={helpers} unit=processes"
        );
        println!(
            "MESH_PERF metric=native_subscription_idle_events value={} unit=events",
            usize::from(idle)
        );
        assert_eq!(helpers, 0);
        assert!(
            !idle,
            "workload requires an otherwise idle network namespace"
        );
        state.shutdown().await;
    }

    fn child_count() -> usize {
        let mut children = std::collections::HashSet::new();
        for thread in std::fs::read_dir("/proc/self/task").unwrap().flatten() {
            if let Ok(ids) = std::fs::read_to_string(thread.path().join("children")) {
                children.extend(ids.split_whitespace().map(str::to_owned));
            }
        }
        children.len()
    }
}
