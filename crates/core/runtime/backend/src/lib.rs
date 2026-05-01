use mesh_core_scripting::BackendScriptContext;
use serde_json::Value as JsonValue;
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
    settings: JsonValue,
    script_source: String,
    tx: mpsc::UnboundedSender<BackendServiceUpdate>,
    mut cmd_rx: mpsc::UnboundedReceiver<BackendServiceCommand>,
) {
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        &plugin_id,
        settings,
        capabilities,
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spawn_backend_service_passes_settings_into_backend_context() {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/settings".to_string(),
            "settings".to_string(),
            Vec::new(),
            serde_json::json!({
                "label": "demo",
                "nested": { "enabled": true }
            }),
            "function init()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_poll()\nlocal cfg = mesh.config()\nmesh.service.emit({ label = cfg.label, enabled = cfg.nested.enabled })\nend".to_string(),
            update_tx,
            cmd_rx,
        ));

        let update = tokio::time::timeout(Duration::from_secs(1), update_rx.recv())
            .await
            .expect("backend should emit initial payload")
            .expect("update channel should stay open");
        assert_eq!(update.service, "settings");
        assert_eq!(update.source_plugin, "@test/settings");
        assert_eq!(
            update.payload.get("label").and_then(|v| v.as_str()),
            Some("demo")
        );
        assert_eq!(
            update.payload.get("enabled").and_then(|v| v.as_bool()),
            Some(true)
        );

        drop(cmd_tx);
        drop(update_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

}
