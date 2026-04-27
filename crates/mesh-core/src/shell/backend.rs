use super::types::{ServiceCommandMsg, ServiceEvent, ShellMessage};
use mesh_runtime::protocol::HostRequest;
/// Generic backend plugin service runner.
///
/// Loads a backend plugin's Luau script via `BackendScriptContext`, then runs
/// an async loop that drives polling and command dispatch. Core is not aware of
/// what the plugin does — it just wires the event bus.
use mesh_scripting::BackendScriptContext;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub(super) async fn spawn_backend_service(
    plugin_id: String,
    service_name: String,
    script_source: String,
    tx: mpsc::UnboundedSender<ShellMessage>,
    mut cmd_rx: mpsc::UnboundedReceiver<ServiceCommandMsg>,
) {
    let mut ctx = BackendScriptContext::new(&plugin_id);
    if let Err(e) = ctx.load_script(&script_source) {
        tracing::error!("{plugin_id} failed to load backend script: {e}");
        return;
    }

    let interval_ms = ctx.poll_interval_ms().max(50);
    let mut tick = tokio::time::interval(Duration::from_millis(interval_ms));
    let mut last_payload: Option<serde_json::Value> = None;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                let Some(payload) = ctx.run_poll() else { continue };
                if Some(&payload) == last_payload.as_ref() {
                    continue;
                }
                last_payload = Some(payload.clone());
                if tx.send(ShellMessage::Service(ServiceEvent::Updated {
                    service: service_name.clone(),
                    source_plugin: plugin_id.clone(),
                    payload,
                })).is_err() {
                    break;
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(msg) = cmd else { break };
                if let Some(payload) = ctx.run_command(&msg.command, &msg.payload) {
                    last_payload = Some(payload.clone());
                    if tx.send(ShellMessage::Service(ServiceEvent::Updated {
                        service: service_name.clone(),
                        source_plugin: plugin_id.clone(),
                        payload,
                    })).is_err() {
                        break;
                    }
                }
            }
        }
    }
}

pub(super) async fn spawn_typescript_backend_service(
    plugin_id: String,
    service_name: String,
    entry_path: PathBuf,
    working_dir: PathBuf,
    tx: mpsc::UnboundedSender<ShellMessage>,
    mut cmd_rx: mpsc::UnboundedReceiver<ServiceCommandMsg>,
) {
    let mut child = match Command::new("node")
        .arg(&entry_path)
        .current_dir(&working_dir)
        .stdout(std::process::Stdio::piped())
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            tracing::error!(
                "failed to spawn TypeScript backend {} ({}): {}",
                plugin_id,
                entry_path.display(),
                err
            );
            return;
        }
    };

    let Some(stdout) = child.stdout.take() else {
        tracing::error!("TypeScript backend {} did not expose stdout", plugin_id);
        let _ = child.kill().await;
        return;
    };

    let mut reader = BufReader::new(stdout).lines();
    let mut state = Map::new();

    loop {
        tokio::select! {
            line = reader.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        match serde_json::from_str::<HostRequest>(trimmed) {
                            Ok(message) => {
                                if let Some(payload) = apply_host_request_to_state(
                                    &service_name,
                                    &mut state,
                                    message,
                                ) {
                                    let _ = tx.send(ShellMessage::Service(ServiceEvent::Updated {
                                        service: service_name.clone(),
                                        source_plugin: plugin_id.clone(),
                                        payload: Value::Object(payload),
                                    }));
                                }
                            }
                            Err(err) => {
                                tracing::warn!(
                                    "TypeScript backend {} emitted invalid host JSON: {}",
                                    plugin_id,
                                    err
                                );
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(err) => {
                        tracing::warn!("TypeScript backend {} stdout read failed: {}", plugin_id, err);
                        break;
                    }
                }
            }
            cmd = cmd_rx.recv() => {
                if cmd.is_none() {
                    break;
                }
            }
        }
    }

    let _ = child.kill().await;
}

fn apply_host_request_to_state(
    service_name: &str,
    state: &mut Map<String, Value>,
    message: HostRequest,
) -> Option<Map<String, Value>> {
    match message {
        HostRequest::RegisterBindable { bindable } => {
            if let Some(field) = bindable_field(service_name, &bindable.id) {
                state.insert(field, bindable.initial);
                return Some(state.clone());
            }
        }
        HostRequest::UpdateBindable { id, value } => {
            if let Some(field) = bindable_field(service_name, &id) {
                state.insert(field, value);
                return Some(state.clone());
            }
        }
        HostRequest::RegisterBackend { .. }
        | HostRequest::SubscribeBindable { .. }
        | HostRequest::UnsubscribeBindable { .. }
        | HostRequest::InvokeCore { .. }
        | HostRequest::EmitEvent { .. }
        | HostRequest::RegisterFrontend { .. } => {}
    }

    None
}

fn bindable_field(service_name: &str, bindable_id: &str) -> Option<String> {
    bindable_id
        .strip_prefix(service_name)
        .and_then(|rest| rest.strip_prefix('.'))
        .map(ToString::to_string)
}
