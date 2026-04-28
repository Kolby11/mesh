use mesh_scripting::BackendScriptContext;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct BackendServiceCommand {
    pub command: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BackendServiceUpdate {
    pub service: String,
    pub source_plugin: String,
    pub payload: serde_json::Value,
}

/// Run a backend plugin script and publish service updates.
///
/// Core owns plugin discovery and channel wiring; this crate owns the Luau
/// backend execution loop and polling/command dispatch policy.
pub async fn spawn_backend_service(
    plugin_id: String,
    service_name: String,
    capabilities: Vec<String>,
    script_source: String,
    tx: mpsc::UnboundedSender<BackendServiceUpdate>,
    mut cmd_rx: mpsc::UnboundedReceiver<BackendServiceCommand>,
) {
    let mut ctx = BackendScriptContext::new_with_capabilities(&plugin_id, capabilities);
    if let Err(e) = ctx.load_script(&script_source) {
        tracing::error!("{plugin_id} failed to load backend script: {e}");
        return;
    }
    if let Err(e) = ctx.call_init() {
        tracing::error!("{plugin_id} failed to initialize backend script: {e}");
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
                if tx.send(BackendServiceUpdate {
                    service: service_name.clone(),
                    source_plugin: plugin_id.clone(),
                    payload,
                }).is_err() {
                    break;
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(msg) = cmd else { break };
                if let Some(payload) = ctx.run_command(&msg.command, &msg.payload) {
                    last_payload = Some(payload.clone());
                    if tx.send(BackendServiceUpdate {
                        service: service_name.clone(),
                        source_plugin: plugin_id.clone(),
                        payload,
                    }).is_err() {
                        break;
                    }
                }
            }
        }
    }
}
