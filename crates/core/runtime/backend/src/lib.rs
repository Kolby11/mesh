use mesh_core_scripting::{BackendScriptContext, BackendScriptError};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 3;
const MIN_POLL_INTERVAL_MS: u64 = 50;

#[derive(Debug, Clone)]
pub struct BackendServiceCommand {
    pub command: String,
    pub payload: serde_json::Value,
    /// When true, this command is an idempotent setter — if the receiver
    /// finds queued duplicates of the same name, only the latest payload is
    /// executed. The dispatcher sets this from the interface contract.
    pub coalesce: bool,
}

/// Drain pending commands from the queue, then drop earlier instances of any
/// command marked `coalesce` when a later same-named instance is also present.
/// Non-coalescable commands and commands that appear only once pass through
/// unchanged, preserving original order.
fn coalesce_command_batch(
    batch: Vec<BackendServiceCommand>,
    latest_index: &mut HashMap<String, usize>,
) -> Vec<BackendServiceCommand> {
    if batch.len() < 2 {
        return batch;
    }
    latest_index.clear();
    for (index, msg) in batch.iter().enumerate() {
        if msg.coalesce {
            latest_index.insert(msg.command.clone(), index);
        }
    }
    if latest_index.is_empty() {
        return batch;
    }
    batch
        .into_iter()
        .enumerate()
        .filter(|(index, msg)| {
            !msg.coalesce || latest_index.get(&msg.command).copied() == Some(*index)
        })
        .map(|(_, msg)| msg)
        .collect()
}

#[derive(Debug, Clone)]
pub struct BackendServiceUpdate {
    pub service: Arc<str>,
    pub source_module: Arc<str>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BackendCommandResult {
    pub service: Arc<str>,
    pub source_module: Arc<str>,
    pub command: String,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BackendInterfaceEvent {
    pub service: Arc<str>,
    pub source_module: Arc<str>,
    pub name: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum BackendServiceEvent {
    Started {
        service: Arc<str>,
        source_module: Arc<str>,
    },
    Update(BackendServiceUpdate),
    InitFailed {
        service: Arc<str>,
        source_module: Arc<str>,
        message: String,
    },
    PollFailed {
        service: Arc<str>,
        source_module: Arc<str>,
        count: u32,
        message: String,
    },
    Failed {
        service: Arc<str>,
        source_module: Arc<str>,
        stage: String,
        message: String,
    },
    CommandResult(BackendCommandResult),
    InterfaceEvent(BackendInterfaceEvent),
    Stopped {
        service: Arc<str>,
        source_module: Arc<str>,
    },
}

/// Run a backend module script and publish service updates.
///
/// Core owns module discovery and channel wiring; this crate owns the Luau
/// backend execution loop and polling/command dispatch policy.
pub async fn spawn_backend_service(
    module_id: String,
    service_name: String,
    capabilities: Vec<String>,
    settings: JsonValue,
    script_source: String,
    tx: mpsc::UnboundedSender<BackendServiceEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<BackendServiceCommand>,
) {
    let module_id: Arc<str> = Arc::from(module_id);
    let service_name: Arc<str> = Arc::from(service_name);
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        module_id.as_ref(),
        settings,
        capabilities,
    );
    if let Err(e) = ctx.load_script(&script_source) {
        tracing::error!("{} failed to load backend script: {e}", module_id.as_ref());
        let _ = tx.send(BackendServiceEvent::Failed {
            service: service_name,
            source_module: module_id,
            stage: "load".to_string(),
            message: e.to_string(),
        });
        return;
    }
    let init_payload = match ctx.call_init() {
        Ok(payload) => payload,
        Err(e) => {
            tracing::error!(
                "{} failed to initialize backend script: {e}",
                module_id.as_ref()
            );
            let _ = tx.send(BackendServiceEvent::InitFailed {
                service: service_name,
                source_module: module_id,
                message: e.to_string(),
            });
            return;
        }
    };

    let _ = tx.send(BackendServiceEvent::Started {
        service: service_name.clone(),
        source_module: module_id.clone(),
    });

    let mut interval_ms = bounded_poll_interval_ms(&ctx);
    let mut tick = make_interval(interval_ms, true);
    let mut last_payload: Option<serde_json::Value> = None;
    let mut consecutive_poll_failures = 0;
    let stream_state = ctx.stream_state();
    let mut coalesced_command_index = HashMap::new();

    if let Some(payload) = init_payload {
        if !publish_changed_update(&tx, &service_name, &module_id, &mut last_payload, payload) {
            return;
        }
    }
    publish_script_events(&tx, &service_name, &module_id, ctx.drain_events());

    loop {
        tokio::select! {
            _ = stream_state.wait_for_event() => {
                // Group lines by program in arrival order and hand each group
                // to the script in a single dispatch via `on_stream_batch`
                // (falling back to `on_stream_line` per line). Multi-line
                // event formats like pw-mon emit a header plus property
                // continuations per change; collapsing to a single line per
                // batch silently dropped the headers scripts filter on.
                let lines = stream_state.drain_lines();
                if lines.is_empty() {
                    continue;
                }
                let mut lines_per_program: std::collections::HashMap<String, Vec<String>> =
                    std::collections::HashMap::new();
                let mut program_order: Vec<String> = Vec::new();
                for entry in lines {
                    let bucket = lines_per_program
                        .entry(entry.program.clone())
                        .or_insert_with(|| {
                            program_order.push(entry.program.clone());
                            Vec::new()
                        });
                    bucket.push(entry.line);
                }
                let mut stop = false;
                for program in program_order {
                    let Some(batch) = lines_per_program.remove(&program) else {
                        continue;
                    };
                    match ctx.run_stream_batch(&program, &batch) {
                        Ok(Some(payload)) => {
                            if !publish_changed_update(
                                &tx,
                                &service_name,
                                &module_id,
                                &mut last_payload,
                                payload,
                            ) {
                                stop = true;
                                break;
                            }
                            if !publish_script_events(
                                &tx,
                                &service_name,
                                &module_id,
                                ctx.drain_events(),
                            ) {
                                stop = true;
                                break;
                            }
                        }
                        Ok(None) => {
                            if !publish_script_events(
                                &tx,
                                &service_name,
                                &module_id,
                                ctx.drain_events(),
                            ) {
                                stop = true;
                                break;
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(BackendServiceEvent::Failed {
                                service: service_name.clone(),
                                source_module: module_id.clone(),
                                stage: "stream".to_string(),
                                message: err.to_string(),
                            });
                        }
                    }
                }
                if stop { break; }
            }
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
                            source_module: module_id.clone(),
                            count: consecutive_poll_failures,
                            message: message.clone(),
                        });
                        if consecutive_poll_failures >= MAX_CONSECUTIVE_POLL_FAILURES {
                            let _ = tx.send(BackendServiceEvent::Failed {
                                service: service_name.clone(),
                                source_module: module_id.clone(),
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
                let Some(payload) = payload else {
                    if !publish_script_events(&tx, &service_name, &module_id, ctx.drain_events()) {
                        break;
                    }
                    continue;
                };
                if !publish_changed_update(
                    &tx,
                    &service_name,
                    &module_id,
                    &mut last_payload,
                    payload,
                ) {
                    break;
                }
                publish_script_events(&tx, &service_name, &module_id, ctx.drain_events());
            }
            cmd = cmd_rx.recv() => {
                let Some(first) = cmd else { break };
                let mut batch = vec![first];
                while let Ok(next) = cmd_rx.try_recv() {
                    batch.push(next);
                }
                let batch = coalesce_command_batch(batch, &mut coalesced_command_index);
                let mut stop = false;
                for msg in batch {
                    match ctx.run_command_with_result(&msg.command, &msg.payload) {
                        Ok(outcome) => {
                            refresh_interval(&ctx, &mut interval_ms, &mut tick);
                            if tx.send(BackendServiceEvent::CommandResult(BackendCommandResult {
                                service: service_name.clone(),
                                source_module: module_id.clone(),
                                command: msg.command.clone(),
                                result: outcome.result,
                            })).is_err() {
                                stop = true;
                                break;
                            }
                            if let Some(message) = outcome.error {
                                let _ = tx.send(BackendServiceEvent::Failed {
                                    service: service_name.clone(),
                                    source_module: module_id.clone(),
                                    stage: "command".to_string(),
                                    message,
                                });
                            }
                            if let Some(payload) = outcome.state {
                                if !publish_changed_update(
                                    &tx,
                                    &service_name,
                                    &module_id,
                                    &mut last_payload,
                                    payload,
                                ) {
                                    stop = true;
                                    break;
                                }
                            }
                            publish_script_events(
                                &tx,
                                &service_name,
                                &module_id,
                                ctx.drain_events(),
                            );
                        }
                        Err(err) => {
                            let stage = match &err {
                                BackendScriptError::SnapshotFailed { .. } => "snapshot",
                                BackendScriptError::CommandResultConversionFailed { .. } => {
                                    "command-result"
                                }
                                _ => "command",
                            };
                            let _ = tx.send(BackendServiceEvent::Failed {
                                service: service_name.clone(),
                                source_module: module_id.clone(),
                                stage: stage.to_string(),
                                message: err.to_string(),
                            });
                            refresh_interval(&ctx, &mut interval_ms, &mut tick);
                        }
                    }
                }
                if stop { break; }
            }
        }
    }

    if let Err(err) = ctx.call_stop() {
        let _ = tx.send(BackendServiceEvent::Failed {
            service: service_name.clone(),
            source_module: module_id.clone(),
            stage: "stop".to_string(),
            message: err.to_string(),
        });
    }

    let _ = tx.send(BackendServiceEvent::Stopped {
        service: service_name,
        source_module: module_id,
    });
}

fn publish_script_events(
    tx: &mpsc::UnboundedSender<BackendServiceEvent>,
    service_name: &Arc<str>,
    module_id: &Arc<str>,
    events: Vec<mesh_core_scripting::BackendScriptEvent>,
) -> bool {
    for event in events {
        if tx
            .send(BackendServiceEvent::InterfaceEvent(BackendInterfaceEvent {
                service: Arc::clone(service_name),
                source_module: Arc::clone(module_id),
                name: event.name,
                payload: event.payload,
            }))
            .is_err()
        {
            return false;
        }
    }
    true
}

fn publish_changed_update(
    tx: &mpsc::UnboundedSender<BackendServiceEvent>,
    service_name: &Arc<str>,
    module_id: &Arc<str>,
    last_payload: &mut Option<serde_json::Value>,
    payload: serde_json::Value,
) -> bool {
    if Some(&payload) == last_payload.as_ref() {
        return true;
    }
    last_payload.replace(payload.clone());
    tx.send(BackendServiceEvent::Update(BackendServiceUpdate {
        service: Arc::clone(service_name),
        source_module: Arc::clone(module_id),
        payload,
    }))
    .is_ok()
}

fn bounded_poll_interval_ms(ctx: &BackendScriptContext) -> u64 {
    ctx.poll_interval_ms().max(MIN_POLL_INTERVAL_MS)
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
    use std::path::{Path, PathBuf};

    fn cmd(name: &str, value: i64, coalesce: bool) -> BackendServiceCommand {
        BackendServiceCommand {
            command: name.to_string(),
            payload: serde_json::json!({ "v": value }),
            coalesce,
        }
    }

    fn coalesce_for_test(batch: Vec<BackendServiceCommand>) -> Vec<BackendServiceCommand> {
        let mut latest_index = HashMap::new();
        coalesce_command_batch(batch, &mut latest_index)
    }

    #[test]
    fn coalesce_drops_earlier_duplicates_keeps_latest_payload() {
        let batch = vec![
            cmd("set_volume", 10, true),
            cmd("set_volume", 20, true),
            cmd("set_volume", 30, true),
        ];
        let out = coalesce_for_test(batch);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].command, "set_volume");
        assert_eq!(out[0].payload, serde_json::json!({ "v": 30 }));
    }

    #[test]
    fn coalesce_preserves_non_coalescable_commands_in_order() {
        let batch = vec![
            cmd("volume_up", 0, false),
            cmd("volume_up", 0, false),
            cmd("volume_up", 0, false),
        ];
        let out = coalesce_for_test(batch);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn coalesce_preserves_order_around_dropped_duplicates() {
        let batch = vec![
            cmd("set_volume", 10, true),
            cmd("toggle_mute", 0, false),
            cmd("set_volume", 20, true),
            cmd("set_volume", 30, true),
        ];
        let out = coalesce_for_test(batch);
        let names: Vec<_> = out.iter().map(|c| c.command.as_str()).collect();
        assert_eq!(names, vec!["toggle_mute", "set_volume"]);
        assert_eq!(out[1].payload, serde_json::json!({ "v": 30 }));
    }

    #[test]
    fn coalesce_does_not_collapse_distinct_coalescable_commands() {
        let batch = vec![cmd("set_volume", 50, true), cmd("set_muted", 1, true)];
        let out = coalesce_for_test(batch);
        assert_eq!(out.len(), 2);
    }

    fn bundled_backend_script_path(module_slug: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../../../modules/backend/{module_slug}/src/main.luau"
        ))
    }

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

    async fn next_command_result(
        rx: &mut mpsc::UnboundedReceiver<BackendServiceEvent>,
        reason: &str,
    ) -> BackendCommandResult {
        loop {
            let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect(reason)
                .expect("event channel should stay open");
            if let BackendServiceEvent::CommandResult(result) = event {
                return result;
            }
        }
    }

    async fn next_interface_event(
        rx: &mut mpsc::UnboundedReceiver<BackendServiceEvent>,
        reason: &str,
    ) -> BackendInterfaceEvent {
        loop {
            let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
                .await
                .expect(reason)
                .expect("event channel should stay open");
            if let BackendServiceEvent::InterfaceEvent(event) = event {
                return event;
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
            "function start()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_poll()\nlocal cfg = mesh.config()\nmesh.service.emit({ label = cfg.label, enabled = cfg.nested.enabled })\nend".to_string(),
            update_tx,
            cmd_rx,
        ));

        let update = next_update(&mut update_rx, "backend should emit initial payload").await;
        assert_eq!(update.service.as_ref(), "settings");
        assert_eq!(update.source_module.as_ref(), "@test/settings");
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
    async fn spawn_backend_service_emits_initial_exported_state() {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/exported-init".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "state = { available = false }\n\
             function start()\n\
               mesh.service.set_poll_interval(1000)\n\
               state = { available = true, percent = 65 }\n\
             end"
            .to_string(),
            update_tx,
            cmd_rx,
        ));

        let update = next_update(&mut update_rx, "init should publish exported state").await;
        assert_eq!(update.service.as_ref(), "audio");
        assert_eq!(update.source_module.as_ref(), "@test/exported-init");
        assert_eq!(
            update.payload.get("available").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            update.payload.get("percent").and_then(|v| v.as_u64()),
            Some(65)
        );

        drop(cmd_tx);
        drop(update_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_forwards_script_interface_events() {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/audio".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "state = { available = true, percent = 40 }\n\
             function start()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_command_set_volume()\n\
               local payload = mesh.service.payload()\n\
               mesh.service.emit_event(\"VolumeChanged\", { device_id = payload.device_id, level = payload.volume })\n\
             end"
            .to_string(),
            update_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                command: "set_volume".to_string(),
                payload: serde_json::json!({ "device_id": "default", "volume": 0.42 }),
                coalesce: false,
            })
            .unwrap();

        let event =
            next_interface_event(&mut update_rx, "command should publish interface event").await;
        assert_eq!(event.service.as_ref(), "audio");
        assert_eq!(event.source_module.as_ref(), "@test/audio");
        assert_eq!(event.name, "VolumeChanged");
        assert_eq!(
            event.payload,
            serde_json::json!({ "device_id": "default", "level": 0.42 })
        );

        drop(cmd_tx);
        drop(update_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_emits_changed_exported_state_after_poll() {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/exported-poll".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "state = { tick = 0 }\n\
             function start()\nmesh.service.set_poll_interval(50)\nend\n\
             function on_poll()\nstate = { tick = state.tick + 1 }\nend"
                .to_string(),
            update_tx,
            cmd_rx,
        ));

        let initial = next_update(&mut update_rx, "init should publish exported state").await;
        assert_eq!(
            initial.payload.get("tick").and_then(|v| v.as_u64()),
            Some(0)
        );

        let polled = next_update(&mut update_rx, "poll should publish changed state").await;
        assert_eq!(polled.payload.get("tick").and_then(|v| v.as_u64()), Some(1));

        drop(cmd_tx);
        drop(update_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_emits_changed_exported_state_after_command() {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/exported-command".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "state = { percent = 0 }\n\
             function start()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_command_set_volume()\n\
               local payload = mesh.service.payload()\n\
               state = { percent = payload.percent }\n\
             end"
            .to_string(),
            update_tx,
            cmd_rx,
        ));

        let initial = next_update(&mut update_rx, "init should publish exported state").await;
        assert_eq!(
            initial.payload.get("percent").and_then(|v| v.as_u64()),
            Some(0)
        );

        cmd_tx
            .send(BackendServiceCommand {
                command: "set-volume".to_string(),
                payload: serde_json::json!({ "percent": 77 }),
                coalesce: false,
            })
            .unwrap();

        let updated = next_update(&mut update_rx, "command should publish changed state").await;
        assert_eq!(
            updated.payload.get("percent").and_then(|v| v.as_u64()),
            Some(77)
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
             function start()\nmesh.service.set_poll_interval(1000)\nend\n\
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
    async fn spawn_backend_service_applies_command_interval_change_after_handler() {
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/command-polling".to_string(),
            "polling".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "local tick = 0\n\
             function start()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_poll()\n\
               tick = tick + 1\n\
               mesh.service.emit({ event = \"poll\", tick = tick })\n\
             end\n\
             function on_command_fast()\n\
               mesh.service.set_poll_interval(60)\n\
               mesh.service.emit({ event = \"command\" })\n\
             end"
            .to_string(),
            update_tx,
            cmd_rx,
        ));

        let first = next_update(&mut update_rx, "backend should emit the first poll").await;
        assert_eq!(
            first.payload.get("event").and_then(|v| v.as_str()),
            Some("poll")
        );
        assert_eq!(first.payload.get("tick").and_then(|v| v.as_u64()), Some(1));

        cmd_tx
            .send(BackendServiceCommand {
                command: "fast".to_string(),
                payload: serde_json::json!({}),
                coalesce: false,
            })
            .unwrap();

        let command = next_update(&mut update_rx, "command handler should emit a payload").await;
        assert_eq!(
            command.payload.get("event").and_then(|v| v.as_str()),
            Some("command")
        );

        let second = loop {
            let event = tokio::time::timeout(Duration::from_millis(250), update_rx.recv())
                .await
                .expect("command interval update should affect the following poll")
                .expect("update channel should stay open");
            if let BackendServiceEvent::Update(update) = event {
                break update;
            }
        };
        assert_eq!(
            second.payload.get("event").and_then(|v| v.as_str()),
            Some("poll")
        );
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
        let script_path = bundled_backend_script_path("shell-theme");
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
                coalesce: false,
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
            "function start()\nmesh.service.set_poll_interval(1000)\nend\n\
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
                coalesce: false,
            })
            .unwrap();

        let update = next_update(
            &mut update_rx,
            "set_volume command should emit normalized payload",
        )
        .await;
        assert_eq!(update.service.as_ref(), "audio");
        assert_eq!(update.source_module.as_ref(), "@test/audio");
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

    async fn assert_backend_command_handler_error_becomes_failed_result() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/command-error".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function start()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_command_fail()\nerror(\"command boom\")\nend"
                .to_string(),
            event_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                command: "fail".to_string(),
                payload: serde_json::json!({}),
                coalesce: false,
            })
            .unwrap();

        let result = next_command_result(
            &mut event_rx,
            "command failure should emit a caller-visible result",
        )
        .await;
        assert_eq!(result.service.as_ref(), "audio");
        assert_eq!(result.source_module.as_ref(), "@test/command-error");
        assert_eq!(result.command, "fail");
        assert_eq!(
            result.result.get("ok").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert!(
            result
                .result
                .get("error")
                .and_then(|v| v.as_str())
                .is_some_and(|message| message.contains("command boom"))
        );

        let failed = loop {
            let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
                .await
                .expect("command failure should remain lifecycle-visible")
                .expect("event channel should stay open");
            if let BackendServiceEvent::Failed { stage, message, .. } = event {
                break (stage, message);
            }
        };
        assert_eq!(failed.0, "command");
        assert!(failed.1.contains("command boom"));

        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn backend_command_handler_error_becomes_failed_result() {
        assert_backend_command_handler_error_becomes_failed_result().await;
    }

    #[tokio::test]
    async fn backend_command_result_handler_error_becomes_failed_result() {
        assert_backend_command_handler_error_becomes_failed_result().await;
    }

    async fn assert_bundled_command_handler_returns_result_table() {
        let script_path = bundled_backend_script_path("pipewire-audio");
        let script = std::fs::read_to_string(script_path).unwrap();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@mesh/pipewire-audio".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            script,
            event_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                command: "play-sound".to_string(),
                payload: serde_json::json!({ "path": "../blocked.wav" }),
                coalesce: false,
            })
            .unwrap();

        let result = next_command_result(
            &mut event_rx,
            "bundled provider command should return a result table",
        )
        .await;
        assert_eq!(result.service.as_ref(), "audio");
        assert_eq!(result.source_module.as_ref(), "@mesh/pipewire-audio");
        assert_eq!(result.command, "play-sound");
        assert_eq!(
            result.result.get("ok").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            result.result.get("error").and_then(|v| v.as_str()),
            Some("invalid sound path")
        );

        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn bundled_command_handler_returns_result_table() {
        assert_bundled_command_handler_returns_result_table().await;
    }

    #[tokio::test]
    async fn bundled_command_result_handler_returns_result_table() {
        assert_bundled_command_handler_returns_result_table().await;
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
            "function start()\nerror(\"init boom\")\nend\n\
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
                coalesce: false,
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
    async fn backend_unsupported_command_returns_error_result() {
        // Sending a command name that no handler exists for must produce a CommandResult with
        // ok=false and an "error" field. It must not crash the backend or emit a Failed event.
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/no-handler".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function start()\nmesh.service.set_poll_interval(1000)\nend".to_string(),
            event_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                command: "nonexistent-command".to_string(),
                payload: serde_json::json!({}),
                coalesce: false,
            })
            .unwrap();

        let result = next_command_result(
            &mut event_rx,
            "unsupported command should emit a generic error CommandResult",
        )
        .await;
        assert_eq!(result.service.as_ref(), "audio");
        assert_eq!(result.source_module.as_ref(), "@test/no-handler");
        assert_eq!(result.command, "nonexistent-command");
        assert_eq!(
            result.result.get("ok").and_then(|v| v.as_bool()),
            Some(false),
            "unsupported command result must have ok=false"
        );
        assert!(
            result
                .result
                .get("error")
                .and_then(|v| v.as_str())
                .is_some(),
            "unsupported command result must carry an error field"
        );

        // Verify no Failed lifecycle event was emitted (unsupported commands are not failures)
        let no_failure = tokio::time::timeout(Duration::from_millis(150), async {
            loop {
                match event_rx.recv().await {
                    Some(BackendServiceEvent::Failed { .. }) => return true,
                    Some(_) => continue,
                    None => return false,
                }
            }
        })
        .await;
        assert!(
            no_failure.is_err() || !no_failure.unwrap(),
            "unsupported command must not emit a Failed lifecycle event"
        );

        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_reports_snapshot_failure_stage() {
        // A command handler that sets state to a non-serializable Lua value (a function) causes
        // take_service_state_snapshot() to return SnapshotFailed. The backend lifecycle must emit
        // a Failed event with stage="snapshot" so the shell can bucket it separately.
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/snapshot-fail".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            // The handler sets state to a function, which cannot be serialized to JSON.
            // run_command_with_result -> take_service_state_snapshot -> SnapshotFailed.
            "function start()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_command_bad_state()\n\
               state = function() end\n\
             end"
            .to_string(),
            event_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                command: "bad-state".to_string(),
                payload: serde_json::json!({}),
                coalesce: false,
            })
            .unwrap();

        let failed = loop {
            let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
                .await
                .expect("snapshot failure should emit a Failed lifecycle event")
                .expect("event channel should stay open");
            if let BackendServiceEvent::Failed { stage, message, .. } = event {
                break (stage, message);
            }
        };
        assert_eq!(
            failed.0, "snapshot",
            "snapshot serialization failures must use stage='snapshot'"
        );
        assert!(
            failed.1.contains("failed to export state snapshot"),
            "Failed message should describe the snapshot stage: {}",
            failed.1
        );

        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_command_error_emits_result_and_failed_event() {
        // A command handler that raises a Lua error must:
        // 1. Emit a CommandResult with ok=false (caller-visible error result)
        // 2. Emit a Failed event with stage="command" (lifecycle visibility)
        // Both events must be present — the Failed event must not be silently dropped.
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/cmd-err".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function start()\nmesh.service.set_poll_interval(1000)\nend\n\
             function on_command_fail()\nerror(\"handler boom\")\nend"
                .to_string(),
            event_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                command: "fail".to_string(),
                payload: serde_json::json!({}),
                coalesce: false,
            })
            .unwrap();

        let result = next_command_result(
            &mut event_rx,
            "command failure should emit a caller-visible CommandResult",
        )
        .await;
        assert_eq!(
            result.result.get("ok").and_then(|v| v.as_bool()),
            Some(false),
            "CommandResult should have ok=false for handler errors"
        );
        assert!(
            result
                .result
                .get("error")
                .and_then(|v| v.as_str())
                .is_some_and(|m| m.contains("handler boom")),
            "CommandResult.error should carry the handler message"
        );

        let failed = loop {
            let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
                .await
                .expect("command failure should also emit a Failed lifecycle event")
                .expect("event channel should stay open");
            if let BackendServiceEvent::Failed { stage, message, .. } = event {
                break (stage, message);
            }
        };
        assert_eq!(
            failed.0, "command",
            "handler runtime errors must use stage='command'"
        );
        assert!(
            failed.1.contains("handler boom"),
            "Failed message should carry the handler error: {}",
            failed.1
        );

        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
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
            "function start()\nmesh.service.set_poll_interval(50)\nend\n\
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

    #[tokio::test]
    async fn reference_media_backend_emits_initial_state() {
        let script_path = bundled_backend_script_path("reference-media");
        let script = std::fs::read_to_string(script_path).unwrap();
        let (update_tx, mut update_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@mesh/reference-media".to_string(),
            "media".to_string(),
            vec![
                "service.media.read".to_string(),
                "service.media.control".to_string(),
            ],
            serde_json::json!({
                "seed_title": "Initial Track",
                "seed_artist": "Initial Artist",
                "seed_album": "Initial Album"
            }),
            script,
            update_tx,
            cmd_rx,
        ));

        let update = next_update(
            &mut update_rx,
            "reference-media backend should emit initial state on startup",
        )
        .await;

        assert_eq!(update.service.as_ref(), "media");
        assert_eq!(update.source_module.as_ref(), "@mesh/reference-media");
        assert_eq!(
            update.payload.get("available").and_then(|v| v.as_bool()),
            Some(true),
            "initial state must have available=true"
        );
        assert_eq!(
            update.payload.get("title").and_then(|v| v.as_str()),
            Some("Initial Track"),
            "initial state must reflect config seed_title"
        );
        assert!(
            update
                .payload
                .get("state")
                .and_then(|v| v.as_str())
                .is_some(),
            "initial state must include a playback state field"
        );

        drop(cmd_tx);
        drop(update_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn reference_media_backend_command_returns_result_and_updated_state() {
        let script_path = bundled_backend_script_path("reference-media");
        let script = std::fs::read_to_string(script_path).unwrap();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@mesh/reference-media".to_string(),
            "media".to_string(),
            vec![
                "service.media.read".to_string(),
                "service.media.control".to_string(),
            ],
            serde_json::json!({}),
            script,
            event_tx,
            cmd_rx,
        ));

        // Wait for initial state
        let _initial = next_update(
            &mut event_rx,
            "reference-media backend should emit initial state",
        )
        .await;

        // Issue a play command — must return ok=true and update state to "playing"
        cmd_tx
            .send(BackendServiceCommand {
                command: "play".to_string(),
                payload: serde_json::json!({ "player_id": "default" }),
                coalesce: false,
            })
            .unwrap();

        // Collect CommandResult
        let result =
            next_command_result(&mut event_rx, "play command should emit a CommandResult").await;
        assert_eq!(result.service.as_ref(), "media");
        assert_eq!(result.source_module.as_ref(), "@mesh/reference-media");
        assert_eq!(result.command, "play");
        assert_eq!(
            result.result.get("ok").and_then(|v| v.as_bool()),
            Some(true),
            "play command result must have ok=true"
        );

        // Collect updated state Update
        let updated = next_update(
            &mut event_rx,
            "play command should trigger a state update with playback_state=playing",
        )
        .await;
        assert_eq!(
            updated.payload.get("state").and_then(|v| v.as_str()),
            Some("playing"),
            "playback state must change to 'playing' after play command"
        );

        // Issue next command — must advance the track
        cmd_tx
            .send(BackendServiceCommand {
                command: "next".to_string(),
                payload: serde_json::json!({ "player_id": "default" }),
                coalesce: false,
            })
            .unwrap();

        let next_result =
            next_command_result(&mut event_rx, "next command should emit a CommandResult").await;
        assert_eq!(
            next_result.result.get("ok").and_then(|v| v.as_bool()),
            Some(true),
            "next command result must have ok=true"
        );

        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn reference_media_invalid_command_returns_module_scoped_failure() {
        let script_path = bundled_backend_script_path("reference-media");
        let script = std::fs::read_to_string(script_path).unwrap();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@mesh/reference-media".to_string(),
            "media".to_string(),
            vec![
                "service.media.read".to_string(),
                "service.media.control".to_string(),
            ],
            serde_json::json!({}),
            script,
            event_tx,
            cmd_rx,
        ));

        // Wait for initial state
        let _initial = next_update(
            &mut event_rx,
            "reference-media backend should emit initial state",
        )
        .await;

        // Issue pause when not playing — pause handler returns ok=false
        // (reference-media returns {ok=false, error="not currently playing"} from on_command_pause when state != "playing")
        cmd_tx
            .send(BackendServiceCommand {
                command: "pause".to_string(),
                payload: serde_json::json!({ "player_id": "default" }),
                coalesce: false,
            })
            .unwrap();

        let result = next_command_result(
            &mut event_rx,
            "pause-when-not-playing should return a CommandResult",
        )
        .await;

        // Provider id must be attributable in the result
        assert_eq!(
            result.source_module.as_ref(),
            "@mesh/reference-media",
            "CommandResult source_module must identify the provider"
        );
        assert_eq!(result.service.as_ref(), "media");
        // The pause command when not playing returns ok=false
        assert_eq!(
            result.result.get("ok").and_then(|v| v.as_bool()),
            Some(false),
            "pause-when-not-playing must return ok=false"
        );
        assert!(
            result
                .result
                .get("error")
                .and_then(|v| v.as_str())
                .is_some(),
            "failed result must carry an error field attributable to @mesh/reference-media"
        );

        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }
}
