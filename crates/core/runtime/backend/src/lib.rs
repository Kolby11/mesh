use mesh_core_scripting::BackendScriptContext;
use serde_json::Value as JsonValue;
use std::time::Duration;
use tokio::sync::mpsc;

const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 3;

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

#[derive(Debug, Clone)]
pub enum BackendServiceEvent {
    Started {
        service: String,
        source_plugin: String,
    },
    Update(BackendServiceUpdate),
    InitFailed {
        service: String,
        source_plugin: String,
        message: String,
    },
    PollFailed {
        service: String,
        source_plugin: String,
        count: u32,
        message: String,
    },
    Failed {
        service: String,
        source_plugin: String,
        stage: String,
        message: String,
    },
    Stopped {
        service: String,
        source_plugin: String,
    },
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
    tx: mpsc::UnboundedSender<BackendServiceEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<BackendServiceCommand>,
) {
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        &plugin_id,
        settings,
        capabilities,
    );
    if let Err(e) = ctx.load_script(&script_source) {
        tracing::error!("{plugin_id} failed to load backend script: {e}");
        let _ = tx.send(BackendServiceEvent::Failed {
            service: service_name,
            source_plugin: plugin_id,
            stage: "load".to_string(),
            message: e.to_string(),
        });
        return;
    }
    if let Err(e) = ctx.call_init() {
        tracing::error!("{plugin_id} failed to initialize backend script: {e}");
        let _ = tx.send(BackendServiceEvent::InitFailed {
            service: service_name,
            source_plugin: plugin_id,
            message: e.to_string(),
        });
        return;
    }

    let _ = tx.send(BackendServiceEvent::Started {
        service: service_name.clone(),
        source_plugin: plugin_id.clone(),
    });

    let mut interval_ms = bounded_poll_interval_ms(&ctx);
    let mut tick = make_interval(interval_ms, true);
    let mut last_payload: Option<serde_json::Value> = None;
    let mut consecutive_poll_failures = 0;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                let payload = match ctx.run_poll() {
                    Ok(payload) => {
                        consecutive_poll_failures = 0;
                        payload
                    }
                    Err(err) => {
                        consecutive_poll_failures += 1;
                        let message = err.to_string();
                        let _ = tx.send(BackendServiceEvent::PollFailed {
                            service: service_name.clone(),
                            source_plugin: plugin_id.clone(),
                            count: consecutive_poll_failures,
                            message: message.clone(),
                        });
                        if consecutive_poll_failures >= MAX_CONSECUTIVE_POLL_FAILURES {
                            let _ = tx.send(BackendServiceEvent::Failed {
                                service: service_name.clone(),
                                source_plugin: plugin_id.clone(),
                                stage: "poll".to_string(),
                                message,
                            });
                            break;
                        }
                        refresh_interval(&ctx, &mut interval_ms, &mut tick);
                        continue;
                    }
                };
                refresh_interval(&ctx, &mut interval_ms, &mut tick);
                let Some(payload) = payload else { continue };
                if Some(&payload) == last_payload.as_ref() {
                    continue;
                }
                last_payload = Some(payload.clone());
                if tx.send(BackendServiceEvent::Update(BackendServiceUpdate {
                    service: service_name.clone(),
                    source_plugin: plugin_id.clone(),
                    payload,
                })).is_err() {
                    break;
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(msg) = cmd else { break };
                match ctx.run_command(&msg.command, &msg.payload) {
                    Ok(Some(payload)) => {
                        refresh_interval(&ctx, &mut interval_ms, &mut tick);
                        last_payload = Some(payload.clone());
                        if tx.send(BackendServiceEvent::Update(BackendServiceUpdate {
                            service: service_name.clone(),
                            source_plugin: plugin_id.clone(),
                            payload,
                        })).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        refresh_interval(&ctx, &mut interval_ms, &mut tick);
                    }
                    Err(err) => {
                        let _ = tx.send(BackendServiceEvent::Failed {
                            service: service_name.clone(),
                            source_plugin: plugin_id.clone(),
                            stage: "command".to_string(),
                            message: err.to_string(),
                        });
                        refresh_interval(&ctx, &mut interval_ms, &mut tick);
                    }
                }
            }
        }
    }

    let _ = tx.send(BackendServiceEvent::Stopped {
        service: service_name,
        source_plugin: plugin_id,
    });
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

    async fn next_update(
        rx: &mut mpsc::UnboundedReceiver<BackendServiceEvent>,
        reason: &str,
    ) -> BackendServiceUpdate {
        loop {
            let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect(reason)
                .expect("event channel should stay open");
            if let BackendServiceEvent::Update(update) = event {
                return update;
            }
        }
    }

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

        let update = next_update(&mut update_rx, "backend should emit initial payload").await;
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

        let first = next_update(&mut update_rx, "backend should emit the first poll").await;
        assert_eq!(first.payload.get("tick").and_then(|v| v.as_u64()), Some(1));

        let second = loop {
            let event = tokio::time::timeout(Duration::from_millis(250), update_rx.recv())
                .await
                .expect("poll interval update should take effect without restarting")
                .expect("update channel should stay open");
            if let BackendServiceEvent::Update(update) = event {
                break update;
            }
        };
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

        let initial = next_update(
            &mut update_rx,
            "shell-theme backend should emit its initial state",
        )
        .await;
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

        let updated = next_update(
            &mut update_rx,
            "shell-theme command should emit an updated payload",
        )
        .await;
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

        let update = next_update(
            &mut update_rx,
            "set_volume command should emit normalized payload",
        )
        .await;
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

    #[tokio::test]
    async fn spawn_backend_service_emits_init_failed_and_does_not_poll_or_dispatch_commands() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/init-fails".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function init()\nerror(\"init boom\")\nend\n\
             function on_poll()\nmesh.service.emit({ polled = true })\nend\n\
             function on_command_ping()\nmesh.service.emit({ command = true })\nend"
                .to_string(),
            event_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                command: "ping".to_string(),
                payload: serde_json::json!({}),
            })
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("init failure should emit lifecycle event")
            .expect("event channel should stay open");
        assert!(matches!(event, BackendServiceEvent::InitFailed { .. }));

        match tokio::time::timeout(Duration::from_millis(150), event_rx.recv()).await {
            Err(_) | Ok(None) => {}
            Ok(Some(event)) => {
                assert!(
                    !matches!(event, BackendServiceEvent::Update(_)),
                    "init failure must not poll or dispatch commands"
                );
            }
        }

        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after init failure")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_stops_after_three_consecutive_poll_failures() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/poll-fails".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function init()\nmesh.service.set_poll_interval(50)\nend\n\
             function on_poll()\nerror(\"poll boom\")\nend"
                .to_string(),
            event_tx,
            cmd_rx,
        ));

        let started = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("runtime should emit Started")
            .expect("event channel should stay open");
        assert!(matches!(started, BackendServiceEvent::Started { .. }));

        for expected_count in 1..=MAX_CONSECUTIVE_POLL_FAILURES {
            let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
                .await
                .expect("poll failure should emit lifecycle event")
                .expect("event channel should stay open");
            match event {
                BackendServiceEvent::PollFailed { count, message, .. } => {
                    assert_eq!(count, expected_count);
                    assert!(message.contains("poll boom"));
                }
                other => panic!("expected PollFailed event, got {other:?}"),
            }
        }

        let failed = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("runtime should emit terminal failure")
            .expect("event channel should stay open");
        match failed {
            BackendServiceEvent::Failed { stage, .. } => assert_eq!(stage, "poll"),
            other => panic!("expected Failed event, got {other:?}"),
        }

        let stopped = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("runtime should emit stopped")
            .expect("event channel should stay open");
        assert!(matches!(stopped, BackendServiceEvent::Stopped { .. }));

        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after poll failure threshold")
            .expect("backend task should not panic");
    }
}
