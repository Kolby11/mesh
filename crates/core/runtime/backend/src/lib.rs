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

    let mut interval_ms = bounded_poll_interval_ms(&ctx);
    let mut tick = make_interval(interval_ms, true);
    let mut last_payload: Option<serde_json::Value> = None;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                let payload = ctx.run_poll();
                refresh_interval(&ctx, &mut interval_ms, &mut tick);
                let Some(payload) = payload else { continue };
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
                    refresh_interval(&ctx, &mut interval_ms, &mut tick);
                    last_payload = Some(payload.clone());
                    if tx.send(BackendServiceUpdate {
                        service: service_name.clone(),
                        source_plugin: plugin_id.clone(),
                        payload,
                    }).is_err() {
                        break;
                    }
                } else {
                    refresh_interval(&ctx, &mut interval_ms, &mut tick);
                }
            }
        }
    }
}

fn bounded_poll_interval_ms(ctx: &BackendScriptContext) -> u64 {
    ctx.poll_interval_ms().max(50)
}

fn make_interval(interval_ms: u64, immediate: bool) -> tokio::time::Interval {
    let duration = Duration::from_millis(interval_ms);
    let mut interval = if immediate {
        tokio::time::interval(duration)
    } else {
        tokio::time::interval_at(tokio::time::Instant::now() + duration, duration)
    };
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval
}

fn refresh_interval(
    ctx: &BackendScriptContext,
    active_interval_ms: &mut u64,
    tick: &mut tokio::time::Interval,
) {
    let next_interval_ms = bounded_poll_interval_ms(ctx);
    if next_interval_ms != *active_interval_ms {
        *active_interval_ms = next_interval_ms;
        *tick = make_interval(next_interval_ms, false);
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

    #[tokio::test]
    async fn spawn_backend_service_applies_runtime_poll_interval_changes() {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/polling".to_string(),
            "polling".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "local tick = 0\n\
             function init()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_poll()\n\
               tick = tick + 1\n\
               if tick == 1 then\n\
                 mesh.service.set_poll_interval(60)\n\
               end\n\
               mesh.service.emit({ tick = tick })\n\
             end"
            .to_string(),
            update_tx,
            cmd_rx,
        ));

        let first = tokio::time::timeout(Duration::from_secs(1), update_rx.recv())
            .await
            .expect("backend should emit the first poll")
            .expect("update channel should stay open");
        assert_eq!(first.payload.get("tick").and_then(|v| v.as_u64()), Some(1));

        let second = tokio::time::timeout(Duration::from_millis(250), update_rx.recv())
            .await
            .expect("poll interval update should take effect without restarting")
            .expect("update channel should stay open");
        assert_eq!(second.payload.get("tick").and_then(|v| v.as_u64()), Some(2));

        drop(cmd_tx);
        drop(update_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn shell_theme_backend_runs_through_runtime_loop() {
        let script_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../packages/plugins/backend/core/shell-theme/src/main.luau");
        let script = std::fs::read_to_string(script_path).unwrap();
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@mesh/shell-theme".to_string(),
            "theme".to_string(),
            vec!["service.theme.read".to_string()],
            serde_json::json!({}),
            script,
            update_tx,
            cmd_rx,
        ));

        let initial = tokio::time::timeout(Duration::from_secs(1), update_rx.recv())
            .await
            .expect("shell-theme backend should emit its initial state")
            .expect("update channel should stay open");
        assert_eq!(
            initial.payload.get("current").and_then(|v| v.as_str()),
            Some("mesh-default-dark")
        );

        cmd_tx
            .send(BackendServiceCommand {
                command: "set-current".to_string(),
                payload: serde_json::json!({ "theme_id": "mesh-default-light" }),
            })
            .unwrap();

        let updated = tokio::time::timeout(Duration::from_secs(1), update_rx.recv())
            .await
            .expect("shell-theme command should emit an updated payload")
            .expect("update channel should stay open");
        assert_eq!(
            updated.payload.get("current").and_then(|v| v.as_str()),
            Some("mesh-default-light")
        );
        assert_eq!(
            updated.payload.get("is_dark").and_then(|v| v.as_bool()),
            Some(false)
        );

        drop(cmd_tx);
        drop(update_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn backend_command_dispatches_set_volume_normalized_payload() {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/audio".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function init()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_command_set_volume()\n\
               local payload = mesh.service.payload()\n\
               mesh.service.emit({ device_id = payload.device_id, volume = payload.volume })\n\
             end"
            .to_string(),
            update_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                command: "set_volume".to_string(),
                payload: serde_json::json!({
                    "device_id": "default",
                    "volume": 0.42
                }),
            })
            .unwrap();

        let update = tokio::time::timeout(Duration::from_secs(1), update_rx.recv())
            .await
            .expect("set_volume command should emit normalized payload")
            .expect("update channel should stay open");
        assert_eq!(update.service, "audio");
        assert_eq!(update.source_plugin, "@test/audio");
        assert_eq!(
            update.payload.get("device_id").and_then(|v| v.as_str()),
            Some("default")
        );
        assert_eq!(
            update.payload.get("volume").and_then(|v| v.as_f64()),
            Some(0.42)
        );

        drop(cmd_tx);
        drop(update_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }
}
