/// Generic backend plugin service runner.
///
/// Loads a backend plugin's Luau script via `BackendScriptContext`, then runs
/// an async loop that drives polling and command dispatch. Core is not aware of
/// what the plugin does — it just wires the event bus.
use mesh_scripting::BackendScriptContext;
use std::time::Duration;
use tokio::sync::mpsc;
use super::types::{ServiceCommandMsg, ServiceEvent, ShellMessage};

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

/// Mock backend for services that have no real plugin yet.
/// Emits simple JSON payloads on a slow timer so frontends can see something.
pub(super) async fn spawn_mock_backend_service(
    tx: mpsc::UnboundedSender<ShellMessage>,
    source_plugin: String,
    service: String,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    let mut step = 0u32;

    loop {
        tick.tick().await;
        step = step.wrapping_add(1);

        let payload = match service.as_str() {
            "network" => serde_json::json!({
                "available": true,
                "connected": step.is_multiple_of(2),
                "label": if step.is_multiple_of(2) { "Connected" } else { "Scanning" },
            }),
            "power" => {
                let pct = 95u32.saturating_sub(step % 40);
                serde_json::json!({
                    "available": true,
                    "percent": pct,
                    "label": format!("{pct}%"),
                    "charging": false,
                })
            }
            "media" => serde_json::json!({
                "available": step.is_multiple_of(3),
                "session": step,
                "title": format!("Track {step}"),
            }),
            other => serde_json::json!({ "tick": step, "service": other }),
        };

        if tx.send(ShellMessage::Service(ServiceEvent::Updated {
            service: service.clone(),
            source_plugin: source_plugin.clone(),
            payload,
        })).is_err() {
            break;
        }
    }
}
