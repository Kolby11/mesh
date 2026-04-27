use super::types::{ServiceCommandMsg, ServiceEvent, ShellMessage};
/// Generic backend plugin service runner.
///
/// Loads a backend plugin's Luau script via `BackendScriptContext`, then runs
/// an async loop that drives polling and command dispatch. Core is not aware of
/// what the plugin does — it just wires the event bus.
use mesh_scripting::BackendScriptContext;
use std::time::Duration;
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
