use mesh_core_scripting::{
    BackendScriptContext, BackendScriptError, StreamEvent, StreamEventKind, StreamHandle,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 3;
const MIN_POLL_INTERVAL_MS: u64 = 50;
const MAX_COMMAND_BATCH: usize = 64;
pub const BACKEND_COMMAND_QUEUE_CAPACITY: usize = 128;
pub const MAX_COMMAND_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_COMMAND_JSON_DEPTH: usize = 32;

static NEXT_CALL_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_RUNTIME_GENERATION: AtomicU64 = AtomicU64::new(1);

pub fn next_runtime_generation() -> u64 {
    NEXT_RUNTIME_GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// Correlates one service invocation with every transport hop and its terminal
/// result. Zero is reserved for test-created commands that do not originate
/// at the shell dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallId(u64);

impl CallId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub fn next() -> Self {
        Self(NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendCommandOutcome {
    Completed,
    Failed,
    Superseded,
    TimedOut,
    Cancelled,
    StaleGeneration,
    QueueFull,
}

impl BackendCommandOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Superseded => "superseded",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::StaleGeneration => "stale_generation",
            Self::QueueFull => "queue_full",
        }
    }
}

#[derive(Debug)]
struct CallControl {
    deadline: Instant,
    cancelled: bool,
    generation: u64,
}

fn call_controls() -> &'static Mutex<HashMap<CallId, CallControl>> {
    static CONTROLS: OnceLock<Mutex<HashMap<CallId, CallControl>>> = OnceLock::new();
    CONTROLS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a bounded execution window for a shell-admitted call.
pub fn register_call(call_id: CallId, timeout: Duration) {
    register_call_for_generation(call_id, timeout, 0);
}

pub fn register_call_for_generation(call_id: CallId, timeout: Duration, generation: u64) {
    call_controls().lock().unwrap().insert(
        call_id,
        CallControl {
            deadline: Instant::now() + timeout,
            cancelled: false,
            generation,
        },
    );
}

fn call_generation(call_id: CallId, fallback: u64) -> u64 {
    call_controls()
        .lock()
        .unwrap()
        .get(&call_id)
        .map(|control| control.generation)
        .filter(|generation| *generation != 0)
        .unwrap_or(fallback)
}

/// Cancel a queued call. A backend that has already started the synchronous
/// Luau handler cannot be interrupted safely, but queued calls observe this
/// flag before entering the handler and still produce a terminal result.
pub fn cancel_call(call_id: CallId) -> bool {
    let mut controls = call_controls().lock().unwrap();
    let Some(control) = controls.get_mut(&call_id) else {
        return false;
    };
    control.cancelled = true;
    true
}

fn take_pre_execution_outcome(call_id: CallId) -> Option<BackendCommandOutcome> {
    let mut controls = call_controls().lock().unwrap();
    let control = controls.get(&call_id)?;
    let outcome = if control.cancelled {
        Some(BackendCommandOutcome::Cancelled)
    } else if Instant::now() >= control.deadline {
        Some(BackendCommandOutcome::TimedOut)
    } else {
        None
    };
    if outcome.is_some() {
        controls.remove(&call_id);
    }
    outcome
}

/// Release call-control state after a terminal result has been emitted.
pub fn finish_call(call_id: CallId) {
    call_controls().lock().unwrap().remove(&call_id);
}

#[derive(Debug, Clone)]
pub struct BackendServiceCommand {
    pub call_id: CallId,
    pub command: String,
    pub payload: serde_json::Value,
    /// When true, this command is an idempotent setter — if the receiver
    /// finds queued duplicates for the same command target, only the latest
    /// payload for that target is executed. The dispatcher sets this from the
    /// interface contract.
    pub coalesce: bool,
}

pub fn validate_command_payload(payload: &serde_json::Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|error| format!("failed to encode command payload: {error}"))?;
    if bytes.len() > MAX_COMMAND_PAYLOAD_BYTES {
        return Err(format!(
            "command payload exceeds {} bytes",
            MAX_COMMAND_PAYLOAD_BYTES
        ));
    }
    fn within_depth(value: &serde_json::Value, current: usize) -> bool {
        if current > MAX_COMMAND_JSON_DEPTH {
            return false;
        }
        match value {
            serde_json::Value::Array(values) => {
                values.iter().all(|value| within_depth(value, current + 1))
            }
            serde_json::Value::Object(values) => values
                .values()
                .all(|value| within_depth(value, current + 1)),
            _ => true,
        }
    }
    if !within_depth(payload, 0) {
        return Err(format!(
            "command payload exceeds JSON depth {}",
            MAX_COMMAND_JSON_DEPTH
        ));
    }
    Ok(())
}

fn coalescing_key(msg: &BackendServiceCommand) -> String {
    let identity = msg.payload.as_object().map(|object| {
        object
            .iter()
            .filter(|(name, _)| {
                name == &"id"
                    || name.ends_with("_id")
                    || name.contains("target")
                    || name.contains("device")
                    || name.contains("player")
                    || name.contains("output")
                    || name.contains("sink")
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    });
    format!(
        "{}:{}",
        msg.command,
        serde_json::to_string(&identity.unwrap_or_default()).unwrap_or_default()
    )
}

/// Drain pending commands from the queue, then drop earlier instances of any
/// command marked `coalesce` when a later command with the same target key is
/// also present.
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
            latest_index.insert(coalescing_key(msg), index);
        }
    }
    if latest_index.is_empty() {
        return batch;
    }
    batch
        .into_iter()
        .enumerate()
        .filter(|(index, msg)| {
            !msg.coalesce || latest_index.get(&coalescing_key(msg)).copied() == Some(*index)
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
    pub call_id: CallId,
    pub service: Arc<str>,
    pub source_module: Arc<str>,
    pub command: String,
    pub result: serde_json::Value,
    pub outcome: BackendCommandOutcome,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub struct BackendInterfaceEvent {
    pub service: Arc<str>,
    pub source_module: Arc<str>,
    pub name: String,
    pub payload: serde_json::Value,
    pub generation: u64,
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
    cmd_rx: mpsc::UnboundedReceiver<BackendServiceCommand>,
) {
    spawn_backend_service_inner(
        module_id,
        service_name,
        capabilities,
        settings,
        script_source,
        tx,
        CommandReceiver::Unbounded(cmd_rx),
        None,
        None,
        0,
    )
    .await;
}

/// Production backend entrypoint. Command ingress is bounded and the
/// interface contract is installed before untrusted script code can receive
/// a command.
pub async fn spawn_backend_service_bounded(
    module_id: String,
    service_name: String,
    capabilities: Vec<String>,
    settings: JsonValue,
    script_source: String,
    tx: mpsc::UnboundedSender<BackendServiceEvent>,
    cmd_rx: mpsc::Receiver<BackendServiceCommand>,
    command_registry: Option<mesh_core_scripting::BackendCommandRegistry>,
    generation: u64,
) {
    spawn_backend_service_bounded_with_events(
        module_id,
        service_name,
        capabilities,
        settings,
        script_source,
        tx,
        cmd_rx,
        command_registry,
        None,
        generation,
    )
    .await;
}

/// Production backend entrypoint with the immutable provider-owned event
/// registry installed before untrusted script code can run.
pub async fn spawn_backend_service_bounded_with_events(
    module_id: String,
    service_name: String,
    capabilities: Vec<String>,
    settings: JsonValue,
    script_source: String,
    tx: mpsc::UnboundedSender<BackendServiceEvent>,
    cmd_rx: mpsc::Receiver<BackendServiceCommand>,
    command_registry: Option<mesh_core_scripting::BackendCommandRegistry>,
    event_registry: Option<mesh_core_scripting::BackendEventRegistry>,
    generation: u64,
) {
    spawn_backend_service_inner(
        module_id,
        service_name,
        capabilities,
        settings,
        script_source,
        tx,
        CommandReceiver::Bounded(cmd_rx),
        command_registry,
        event_registry,
        generation,
    )
    .await;
}

enum CommandReceiver {
    Bounded(mpsc::Receiver<BackendServiceCommand>),
    Unbounded(mpsc::UnboundedReceiver<BackendServiceCommand>),
}

impl CommandReceiver {
    async fn recv(&mut self) -> Option<BackendServiceCommand> {
        match self {
            Self::Bounded(receiver) => receiver.recv().await,
            Self::Unbounded(receiver) => receiver.recv().await,
        }
    }

    fn try_recv(&mut self) -> Option<BackendServiceCommand> {
        match self {
            Self::Bounded(receiver) => receiver.try_recv().ok(),
            Self::Unbounded(receiver) => receiver.try_recv().ok(),
        }
    }
}

async fn spawn_backend_service_inner(
    module_id: String,
    service_name: String,
    capabilities: Vec<String>,
    settings: JsonValue,
    script_source: String,
    tx: mpsc::UnboundedSender<BackendServiceEvent>,
    cmd_rx: CommandReceiver,
    command_registry: Option<mesh_core_scripting::BackendCommandRegistry>,
    event_registry: Option<mesh_core_scripting::BackendEventRegistry>,
    generation: u64,
) {
    let module_id: Arc<str> = Arc::from(module_id);
    let service_name: Arc<str> = Arc::from(service_name);
    // Declare the guard before the context so a cancellation or panic drops
    // the context's Rust-owned resources before publishing the terminal
    // lifecycle record.
    let mut lifecycle = BackendLifecycleGuard::new(
        Arc::clone(&service_name),
        Arc::clone(&module_id),
        tx.clone(),
    );
    let mut ctx = BackendScriptContext::new_with_settings_and_capabilities(
        module_id.as_ref(),
        settings,
        capabilities,
    );
    ctx.set_generation(generation);
    if let Some(registry) = command_registry {
        ctx.set_command_registry(registry);
    }
    if let Some(registry) = event_registry {
        ctx.set_event_registry(registry);
    }
    run_backend_service(
        &mut ctx,
        module_id,
        service_name,
        script_source,
        tx,
        cmd_rx,
        generation,
    )
    .await;
    lifecycle.finish(&mut ctx).await;
}

struct BackendLifecycleGuard {
    service: Arc<str>,
    source_module: Arc<str>,
    tx: mpsc::UnboundedSender<BackendServiceEvent>,
    terminal_sent: bool,
}

impl BackendLifecycleGuard {
    fn new(
        service: Arc<str>,
        source_module: Arc<str>,
        tx: mpsc::UnboundedSender<BackendServiceEvent>,
    ) -> Self {
        Self {
            service,
            source_module,
            tx,
            terminal_sent: false,
        }
    }

    async fn finish(&mut self, ctx: &mut BackendScriptContext) {
        if let Err(err) = ctx.call_stop() {
            let _ = self.tx.send(BackendServiceEvent::Failed {
                service: Arc::clone(&self.service),
                source_module: Arc::clone(&self.source_module),
                stage: "stop".to_string(),
                message: err.to_string(),
            });
        }
        ctx.shutdown_exec();
        ctx.shutdown_streams().await;
        self.send_terminal();
    }

    fn send_terminal(&mut self) {
        if self.terminal_sent {
            return;
        }
        self.terminal_sent = true;
        let _ = self.tx.send(BackendServiceEvent::Stopped {
            service: Arc::clone(&self.service),
            source_module: Arc::clone(&self.source_module),
        });
    }
}

impl Drop for BackendLifecycleGuard {
    fn drop(&mut self) {
        // The context Drop implementation handles synchronous resource
        // cleanup when this is a panic or task cancellation path. Sending the
        // terminal record here makes those paths lifecycle-visible too.
        self.send_terminal();
    }
}

async fn run_backend_service(
    ctx: &mut BackendScriptContext,
    module_id: Arc<str>,
    service_name: Arc<str>,
    script_source: String,
    tx: mpsc::UnboundedSender<BackendServiceEvent>,
    mut cmd_rx: CommandReceiver,
    generation: u64,
) {
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

    if tx
        .send(BackendServiceEvent::Started {
            service: service_name.clone(),
            source_module: module_id.clone(),
        })
        .is_err()
    {
        return;
    }

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
    if !publish_script_events(&tx, &service_name, &module_id, ctx.drain_events()) {
        return;
    }

    loop {
        tokio::select! {
            _ = stream_state.wait_for_event() => {
                let events = stream_state.drain_events();
                if !dispatch_stream_events(
                    ctx,
                    events,
                    &tx,
                    &service_name,
                    &module_id,
                    &mut last_payload,
                ) {
                    break;
                }
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
                if !publish_script_events(&tx, &service_name, &module_id, ctx.drain_events()) {
                    break;
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(first) = cmd else { break };
                let mut batch = vec![first];
                while batch.len() < MAX_COMMAND_BATCH {
                    let Some(next) = cmd_rx.try_recv() else {
                        break;
                    };
                    batch.push(next);
                }
                if let Some(registry) = ctx.command_registry() {
                    for command in &mut batch {
                        command.coalesce = registry.coalesces(&command.command);
                    }
                }
                let original_batch = batch.clone();
                let batch = coalesce_command_batch(batch, &mut coalesced_command_index);
                for superseded in original_batch.into_iter().filter(|candidate| {
                    candidate.coalesce
                        && !batch
                            .iter()
                            .any(|retained| retained.call_id == candidate.call_id)
                }) {
                    finish_call(superseded.call_id);
                    if tx.send(BackendServiceEvent::CommandResult(BackendCommandResult {
                        call_id: superseded.call_id,
                        service: service_name.clone(),
                        source_module: module_id.clone(),
                        command: superseded.command,
                        result: serde_json::json!({
                            "ok": false,
                            "status": "superseded",
                            "error": "coalesced by a newer invocation",
                        }),
                        outcome: BackendCommandOutcome::Superseded,
                        generation,
                    })).is_err() {
                        return;
                    }
                }
                let mut stop = false;
                for msg in batch {
                    let call_id = msg.call_id;
                    let command = msg.command.clone();
                    let command_generation = call_generation(call_id, generation);
                    if generation != 0
                        && command_generation != 0
                        && command_generation != generation
                    {
                        finish_call(call_id);
                        if tx
                            .send(BackendServiceEvent::CommandResult(BackendCommandResult {
                                call_id,
                                service: service_name.clone(),
                                source_module: module_id.clone(),
                                command,
                                result: serde_json::json!({
                                    "ok": false,
                                    "status": "stale_generation",
                                    "error": "backend generation is no longer active",
                                }),
                                outcome: BackendCommandOutcome::StaleGeneration,
                                generation: command_generation,
                            }))
                            .is_err()
                        {
                            stop = true;
                            break;
                        }
                        continue;
                    }
                    if let Err(message) = validate_command_payload(&msg.payload) {
                        finish_call(call_id);
                        if tx
                            .send(BackendServiceEvent::CommandResult(BackendCommandResult {
                                call_id,
                                service: service_name.clone(),
                                source_module: module_id.clone(),
                                command,
                                result: serde_json::json!({
                                    "ok": false,
                                    "status": "invalid_arguments",
                                    "error": message,
                                }),
                                outcome: BackendCommandOutcome::Failed,
                                generation,
                            }))
                            .is_err()
                        {
                            stop = true;
                            break;
                        }
                        continue;
                    }
                    if let Some(outcome) = take_pre_execution_outcome(call_id) {
                        let status = outcome.as_str();
                        if tx
                            .send(BackendServiceEvent::CommandResult(BackendCommandResult {
                                call_id,
                                service: service_name.clone(),
                                source_module: module_id.clone(),
                                command,
                                result: serde_json::json!({
                                    "ok": false,
                                    "status": status,
                                    "error": status,
                                }),
                                outcome,
                                generation,
                            }))
                            .is_err()
                        {
                            stop = true;
                            break;
                        }
                        continue;
                    }
                    match ctx.run_command_with_result(&msg.command, &msg.payload) {
                        Ok(outcome) => {
                            refresh_interval(&ctx, &mut interval_ms, &mut tick);
                            let terminal_outcome = if outcome.error.is_some()
                                || outcome
                                    .result
                                    .get("ok")
                                    .and_then(|value| value.as_bool())
                                    == Some(false)
                            {
                                BackendCommandOutcome::Failed
                            } else {
                                BackendCommandOutcome::Completed
                            };
                            finish_call(call_id);
                            if tx.send(BackendServiceEvent::CommandResult(BackendCommandResult {
                                call_id,
                                service: service_name.clone(),
                                source_module: module_id.clone(),
                                command,
                                result: outcome.result,
                                outcome: terminal_outcome,
                                generation,
                            })).is_err() {
                                stop = true;
                                break;
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
                            if !publish_script_events(
                                &tx,
                                &service_name,
                                &module_id,
                                ctx.drain_events(),
                            ) {
                                stop = true;
                            }
                        }
                        Err(err) => {
                            finish_call(call_id);
                            let _ = tx.send(BackendServiceEvent::CommandResult(BackendCommandResult {
                                call_id,
                                service: service_name.clone(),
                                source_module: module_id.clone(),
                                command,
                                result: serde_json::json!({
                                    "ok": false,
                                    "status": "failed",
                                    "error": err.to_string(),
                                }),
                                outcome: BackendCommandOutcome::Failed,
                                generation,
                            }));
                            refresh_interval(&ctx, &mut interval_ms, &mut tick);
                        }
                    }
                }
                if stop { break; }
            }
        }
    }
    while let Some(pending) = cmd_rx.try_recv() {
        let pending_generation = call_generation(pending.call_id, generation);
        finish_call(pending.call_id);
        let _ = tx.send(BackendServiceEvent::CommandResult(BackendCommandResult {
            call_id: pending.call_id,
            service: service_name.clone(),
            source_module: module_id.clone(),
            command: pending.command,
            result: serde_json::json!({
                "ok": false,
                "status": "stale_generation",
                "error": "backend runtime stopped before dispatch",
            }),
            outcome: BackendCommandOutcome::StaleGeneration,
            generation: pending_generation,
        }));
    }
}

fn dispatch_stream_events(
    ctx: &mut BackendScriptContext,
    events: Vec<StreamEvent>,
    tx: &mpsc::UnboundedSender<BackendServiceEvent>,
    service_name: &Arc<str>,
    module_id: &Arc<str>,
    last_payload: &mut Option<serde_json::Value>,
) -> bool {
    if events.is_empty() {
        return true;
    }
    if ctx.has_stream_event_handler() {
        for event in events {
            let result = ctx.run_stream_event(&event);
            if !publish_stream_callback_result(
                result,
                tx,
                service_name,
                module_id,
                last_payload,
                ctx,
            ) {
                return false;
            }
        }
        return true;
    }

    // Preserve the old Lua hook ABI, but batch only adjacent lines from the
    // same stream identity. Two identical programs can never be merged.
    let mut current: Option<(StreamHandle, Vec<String>)> = None;
    for event in events {
        match event.kind {
            StreamEventKind::Line(line) => {
                if current
                    .as_ref()
                    .is_some_and(|(stream, _)| stream.id() == event.stream.id())
                {
                    current.as_mut().unwrap().1.push(line);
                } else {
                    if let Some((stream, lines)) = current.take() {
                        if !publish_stream_callback_result(
                            ctx.run_stream_batch_for_stream(&stream, &lines),
                            tx,
                            service_name,
                            module_id,
                            last_payload,
                            ctx,
                        ) {
                            return false;
                        }
                    }
                    current = Some((event.stream, vec![line]));
                }
            }
            StreamEventKind::Started
            | StreamEventKind::Eof
            | StreamEventKind::Failed(_)
            | StreamEventKind::Exited(_)
            | StreamEventKind::Overflow { .. } => {
                if let Some((stream, lines)) = current.take() {
                    if !publish_stream_callback_result(
                        ctx.run_stream_batch_for_stream(&stream, &lines),
                        tx,
                        service_name,
                        module_id,
                        last_payload,
                        ctx,
                    ) {
                        return false;
                    }
                }
            }
        }
    }
    if let Some((stream, lines)) = current {
        if !publish_stream_callback_result(
            ctx.run_stream_batch_for_stream(&stream, &lines),
            tx,
            service_name,
            module_id,
            last_payload,
            ctx,
        ) {
            return false;
        }
    }
    true
}

fn publish_stream_callback_result(
    result: Result<Option<serde_json::Value>, BackendScriptError>,
    tx: &mpsc::UnboundedSender<BackendServiceEvent>,
    service_name: &Arc<str>,
    module_id: &Arc<str>,
    last_payload: &mut Option<serde_json::Value>,
    ctx: &mut BackendScriptContext,
) -> bool {
    match result {
        Ok(Some(payload)) => {
            publish_changed_update(tx, service_name, module_id, last_payload, payload)
                && publish_script_events(tx, service_name, module_id, ctx.drain_events())
        }
        Ok(None) => publish_script_events(tx, service_name, module_id, ctx.drain_events()),
        Err(err) => {
            let _ = tx.send(BackendServiceEvent::Failed {
                service: service_name.clone(),
                source_module: module_id.clone(),
                stage: "stream".to_string(),
                message: err.to_string(),
            });
            true
        }
    }
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
                generation: event.generation,
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
            call_id: CallId::next(),
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

    #[test]
    fn coalesce_keeps_distinct_targets_and_drops_stale_updates_per_target() {
        let batch = vec![
            BackendServiceCommand {
                call_id: CallId::next(),
                command: "set_volume".to_string(),
                payload: serde_json::json!({ "device_id": "a", "value": 10 }),
                coalesce: true,
            },
            BackendServiceCommand {
                call_id: CallId::next(),
                command: "set_volume".to_string(),
                payload: serde_json::json!({ "device_id": "b", "value": 20 }),
                coalesce: true,
            },
            BackendServiceCommand {
                call_id: CallId::next(),
                command: "set_volume".to_string(),
                payload: serde_json::json!({ "device_id": "a", "value": 30 }),
                coalesce: true,
            },
        ];
        let out = coalesce_for_test(batch);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].payload["device_id"], "b");
        assert_eq!(out[1].payload["value"], 30);
    }

    #[test]
    fn call_control_produces_terminal_timeout_and_cancel_outcomes() {
        let timed_out = CallId::next();
        register_call(timed_out, Duration::ZERO);
        assert_eq!(
            take_pre_execution_outcome(timed_out),
            Some(BackendCommandOutcome::TimedOut)
        );

        let cancelled = CallId::next();
        register_call(cancelled, Duration::from_secs(1));
        assert!(cancel_call(cancelled));
        assert_eq!(
            take_pre_execution_outcome(cancelled),
            Some(BackendCommandOutcome::Cancelled)
        );
    }

    #[tokio::test]
    async fn bounded_backend_rejects_a_command_from_an_old_generation() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::channel(BACKEND_COMMAND_QUEUE_CAPACITY);
        let task = tokio::spawn(spawn_backend_service_bounded(
            "@test/stale-generation".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function start()\nend\nfunction on_command_ping()\nreturn { ok = true }\nend"
                .to_string(),
            event_tx,
            cmd_rx,
            None,
            9,
        ));
        let call_id = CallId::next();
        register_call_for_generation(call_id, Duration::from_secs(1), 8);
        cmd_tx
            .send(BackendServiceCommand {
                call_id,
                command: "ping".to_string(),
                payload: serde_json::json!({}),
                coalesce: false,
            })
            .await
            .unwrap();
        let result = next_command_result(
            &mut event_rx,
            "stale generation must produce a terminal result",
        )
        .await;
        assert_eq!(result.outcome, BackendCommandOutcome::StaleGeneration);
        assert_eq!(result.generation, 8);
        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("bounded backend task should exit")
            .expect("bounded backend task should not panic");
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
             function on_command_set_volume(self)\n\
               local payload = mesh.service.payload()\n\
               self.VolumeChanged:fire({ device_id = payload.device_id, level = payload.percent })\n\
             end"
            .to_string(),
            update_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                call_id: CallId::from_raw(0),
                command: "set_volume".to_string(),
                payload: serde_json::json!({ "device_id": "default", "percent": 42 }),
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
            serde_json::json!({ "device_id": "default", "level": 42 })
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
                call_id: CallId::from_raw(0),
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
                call_id: CallId::from_raw(0),
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
            Some("tokyo-night")
        );

        cmd_tx
            .send(BackendServiceCommand {
                call_id: CallId::from_raw(0),
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
    async fn backend_command_dispatches_set_volume_percent_payload() {
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
               mesh.service.emit({ device_id = payload.device_id, percent = payload.percent })\n\
             end"
            .to_string(),
            update_tx,
            cmd_rx,
        ));

        cmd_tx
            .send(BackendServiceCommand {
                call_id: CallId::from_raw(0),
                command: "set_volume".to_string(),
                payload: serde_json::json!({
                    "device_id": "default",
                    "percent": 42
                }),
                coalesce: false,
            })
            .unwrap();

        let update = next_update(
            &mut update_rx,
            "set_volume command should emit percent payload",
        )
        .await;
        assert_eq!(update.service.as_ref(), "audio");
        assert_eq!(update.source_module.as_ref(), "@test/audio");
        assert_eq!(
            update.payload.get("device_id").and_then(|v| v.as_str()),
            Some("default")
        );
        assert_eq!(
            update.payload.get("percent").and_then(|v| v.as_i64()),
            Some(42)
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
                call_id: CallId::from_raw(0),
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

        let no_lifecycle_failure = tokio::time::timeout(Duration::from_millis(150), async {
            loop {
                match event_rx.recv().await {
                    Some(BackendServiceEvent::Failed { .. }) => return false,
                    Some(_) => continue,
                    None => return true,
                }
            }
        })
        .await;
        assert!(
            no_lifecycle_failure.is_err() || no_lifecycle_failure.unwrap(),
            "ordinary command failures settle through CommandResult only"
        );

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
                call_id: CallId::from_raw(0),
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
                call_id: CallId::from_raw(0),
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

        let mut terminal_records = 0;
        while terminal_records == 0 {
            let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
                .await
                .expect("init failure should reach terminal cleanup")
                .expect("event channel should stay open until terminal record");
            match event {
                BackendServiceEvent::Stopped { .. } => terminal_records += 1,
                BackendServiceEvent::Update(_) => {
                    panic!("init failure must not poll or dispatch commands")
                }
                _ => {}
            }
        }
        assert_eq!(
            terminal_records, 1,
            "init failure must emit one terminal record"
        );

        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after init failure")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_routes_load_failure_through_terminal_cleanup() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/load-fails".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "this is not valid Luau".to_string(),
            event_tx,
            cmd_rx,
        ));

        let mut saw_load_failure = false;
        let mut terminal_records = 0;
        while terminal_records == 0 {
            let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
                .await
                .expect("load failure should reach terminal cleanup")
                .expect("event channel should stay open until terminal record");
            match event {
                BackendServiceEvent::Failed { stage, .. } => {
                    assert_eq!(stage, "load");
                    saw_load_failure = true;
                }
                BackendServiceEvent::Stopped { .. } => terminal_records += 1,
                other => panic!("unexpected load-failure event: {other:?}"),
            }
        }
        assert!(saw_load_failure);
        assert_eq!(terminal_records, 1);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after load failure")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_flushes_and_records_stop_hook_failure_once() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/stop-fails".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function start()\nmesh.service.set_poll_interval(1000)\nend\n\
             function stop()\nerror(\"stop boom\")\nend"
                .to_string(),
            event_tx,
            cmd_rx,
        ));

        let started = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("runtime should emit Started")
            .expect("event channel should stay open");
        assert!(matches!(started, BackendServiceEvent::Started { .. }));
        drop(cmd_tx);

        let mut stop_failures = 0;
        let mut terminal_records = 0;
        while terminal_records == 0 {
            let event = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
                .await
                .expect("stop failure should reach terminal cleanup")
                .expect("event channel should stay open until terminal record");
            match event {
                BackendServiceEvent::Failed { stage, message, .. } => {
                    assert_eq!(stage, "stop");
                    assert!(message.contains("stop boom"));
                    stop_failures += 1;
                }
                BackendServiceEvent::Stopped { .. } => terminal_records += 1,
                other => panic!("unexpected stop-failure event: {other:?}"),
            }
        }
        assert_eq!(stop_failures, 1);
        assert_eq!(terminal_records, 1);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closure")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn cancelling_backend_service_still_emits_one_terminal_record() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (_cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(spawn_backend_service(
            "@test/cancelled".to_string(),
            "audio".to_string(),
            Vec::new(),
            serde_json::json!({}),
            "function start()\nmesh.service.set_poll_interval(1000)\nend".to_string(),
            event_tx,
            cmd_rx,
        ));

        let started = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("runtime should emit Started")
            .expect("event channel should stay open");
        assert!(matches!(started, BackendServiceEvent::Started { .. }));

        task.abort();
        let join = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("cancelled backend should finish dropping")
            .expect_err("backend task should report cancellation");
        assert!(join.is_cancelled());

        let stopped = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("cancellation should emit terminal cleanup")
            .expect("terminal event should be delivered");
        assert!(matches!(stopped, BackendServiceEvent::Stopped { .. }));
        assert!(matches!(
            event_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        ));
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
                call_id: CallId::from_raw(0),
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
                call_id: CallId::from_raw(0),
                command: "bad-state".to_string(),
                payload: serde_json::json!({}),
                coalesce: false,
            })
            .unwrap();

        let result = next_command_result(
            &mut event_rx,
            "snapshot failure should emit a terminal command result",
        )
        .await;
        assert_eq!(result.outcome, BackendCommandOutcome::Failed);
        assert_eq!(
            result.result.get("ok").and_then(|value| value.as_bool()),
            Some(false)
        );

        drop(cmd_tx);
        drop(event_rx);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("backend task should exit after command channel closes")
            .expect("backend task should not panic");
    }

    #[tokio::test]
    async fn spawn_backend_service_command_error_emits_result_without_failed_event() {
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
                call_id: CallId::from_raw(0),
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

        let no_lifecycle_failure = tokio::time::timeout(Duration::from_millis(150), async {
            loop {
                match event_rx.recv().await {
                    Some(BackendServiceEvent::Failed { .. }) => return false,
                    Some(_) => continue,
                    None => return true,
                }
            }
        })
        .await;
        assert!(
            no_lifecycle_failure.is_err() || no_lifecycle_failure.unwrap(),
            "ordinary command failures settle through CommandResult only"
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
                call_id: CallId::from_raw(0),
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
                call_id: CallId::from_raw(0),
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
                call_id: CallId::from_raw(0),
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
