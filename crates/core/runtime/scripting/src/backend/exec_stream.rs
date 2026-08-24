//! Long-running subprocess streams for backend modules.
//!
//! Every `mesh.exec_stream(program, args)` call gets its own stable handle.
//! Output and lifecycle records are delivered in one bounded queue, while a
//! task owns the child and awaits its exit status. The backend can therefore
//! distinguish two identical programs, observe EOF/failure/exit, and await all
//! children during normal shutdown.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::policy::{ResourceBudget, ResourceLimit};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinHandle;

/// Maximum UTF-8 line payload accepted from one stream record.
pub const MAX_STREAM_LINE_BYTES: usize = 64 * 1024;
const MAX_STREAM_ERROR_BYTES: usize = 16 * 1024;

static NEXT_STREAM_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_STREAM_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Stable identity for one registered stream subprocess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StreamId(u64);

impl StreamId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Exit status retained by a stream handle and exit event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamExitStatus {
    pub code: Option<i32>,
    pub success: bool,
    pub signal: Option<i32>,
}

/// Current lifecycle state of one stream subprocess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamStatus {
    Starting,
    Running,
    Eof,
    Stopping,
    Failed { message: String },
    Exited(StreamExitStatus),
}

/// Typed identity and metadata returned by `mesh.exec_stream`.
#[derive(Debug, Clone)]
pub struct StreamHandle {
    id: StreamId,
    program: Arc<str>,
    args: Arc<[String]>,
    generation: u64,
    status: Arc<Mutex<StreamStatus>>,
}

impl PartialEq for StreamHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.generation == other.generation
    }
}

impl Eq for StreamHandle {}

impl StreamHandle {
    fn new(id: StreamId, program: String, args: Vec<String>, generation: u64) -> Self {
        Self {
            id,
            program: Arc::from(program),
            args: args.into(),
            generation,
            status: Arc::new(Mutex::new(StreamStatus::Starting)),
        }
    }

    pub fn id(&self) -> StreamId {
        self.id
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn status(&self) -> StreamStatus {
        self.status.lock().unwrap().clone()
    }

    fn set_status(&self, status: StreamStatus) {
        *self.status.lock().unwrap() = status;
    }
}

/// One lifecycle or output record emitted by a stream subprocess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamEvent {
    pub stream: StreamHandle,
    pub kind: StreamEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEventKind {
    Started,
    Line(String),
    Eof,
    Failed(String),
    Exited(StreamExitStatus),
    Overflow { dropped: u64 },
}

impl StreamEvent {
    fn queued_output_bytes(&self) -> usize {
        match &self.kind {
            StreamEventKind::Line(line) => line.len(),
            StreamEventKind::Failed(message) => message.len(),
            StreamEventKind::Overflow { .. }
            | StreamEventKind::Started
            | StreamEventKind::Eof
            | StreamEventKind::Exited(_) => 0,
        }
    }
}

/// Compatibility view used by the legacy `on_stream_batch` and
/// `on_stream_line` hooks.
#[derive(Debug, Clone)]
pub struct StreamLine {
    pub stream: StreamHandle,
    pub program: String,
    pub line: String,
}

#[derive(Debug)]
struct StreamProcess {
    stream: StreamHandle,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
    child_accounted: Arc<AtomicBool>,
}

/// Shared state between the backend script context and backend service loop.
#[derive(Debug)]
pub struct StreamState {
    pending: Mutex<VecDeque<StreamEvent>>,
    notify: Notify,
    processes: Mutex<HashMap<StreamId, StreamProcess>>,
    overflow_reported: Mutex<HashSet<StreamId>>,
    resources: ResourceBudget,
    generation: u64,
    shutting_down: AtomicBool,
}

impl StreamState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn new_with_budget(resources: ResourceBudget) -> Arc<Self> {
        Arc::new(Self::with_budget(resources))
    }

    fn with_budget(resources: ResourceBudget) -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            processes: Mutex::new(HashMap::new()),
            overflow_reported: Mutex::new(HashSet::new()),
            resources,
            generation: NEXT_STREAM_GENERATION.fetch_add(1, Ordering::Relaxed),
            shutting_down: AtomicBool::new(false),
        }
    }

    /// Wait until a queued stream record is available.
    pub async fn wait_for_event(&self) {
        loop {
            let notified = self.notify.notified();
            if !self.pending.lock().unwrap().is_empty() {
                return;
            }
            notified.await;
        }
    }

    /// Drain lifecycle and output records, releasing their queue budget.
    pub fn drain_events(&self) -> Vec<StreamEvent> {
        self.reap_finished();
        let events: Vec<_> = self.pending.lock().unwrap().drain(..).collect();
        let queued_output = events
            .iter()
            .map(StreamEvent::queued_output_bytes)
            .sum::<usize>();
        self.resources.release_queue(events.len());
        self.resources.release_queued_output(queued_output);
        self.overflow_reported.lock().unwrap().clear();
        events
    }

    /// Drain the queue as legacy line records. Lifecycle events are consumed
    /// but remain available through [`Self::drain_events`] for new callers.
    pub fn drain_lines(&self) -> Vec<StreamLine> {
        self.drain_events()
            .into_iter()
            .filter_map(|event| match event.kind {
                StreamEventKind::Line(line) => Some(StreamLine {
                    stream: event.stream.clone(),
                    program: event.stream.program().to_string(),
                    line,
                }),
                _ => None,
            })
            .collect()
    }

    /// Request shutdown without waiting. Used by synchronous drop paths.
    pub fn request_shutdown(&self) {
        let mut processes = self.processes.lock().unwrap();
        for process in processes.values() {
            process.stream.set_status(StreamStatus::Stopping);
        }
        for process in processes.values_mut() {
            if let Some(stop) = process.stop.take() {
                let _ = stop.send(());
            }
        }
        self.notify.notify_waiters();
    }

    /// Kill and detach every active child for a synchronous drop fallback.
    pub fn kill_all(&self) {
        self.shutting_down.store(true, Ordering::Release);
        let processes = self
            .processes
            .lock()
            .unwrap()
            .drain()
            .map(|(_, process)| process)
            .collect::<Vec<_>>();
        for process in processes {
            if let Some(stop) = process.stop {
                let _ = stop.send(());
            }
            process.task.abort();
            release_child_once(&self.resources, &process.child_accounted);
        }
        self.clear_pending();
        self.notify.notify_waiters();
    }

    /// Request, await, and reap every active child. Finished process tasks
    /// own their child wait, so this method does not return until all children
    /// have been reaped or their task has reported a join failure.
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        self.clear_pending();
        let mut processes = self
            .processes
            .lock()
            .unwrap()
            .drain()
            .map(|(_, process)| process)
            .collect::<Vec<_>>();
        for process in &mut processes {
            if let Some(stop) = process.stop.take() {
                let _ = stop.send(());
            }
        }
        for process in processes {
            let _ = process.task.await;
            release_child_once(&self.resources, &process.child_accounted);
        }
        self.notify.notify_waiters();
    }

    /// Number of registered process tasks, including tasks whose final event
    /// has not yet been drained.
    #[cfg(test)]
    pub fn active_stream_count(&self) -> usize {
        self.reap_finished();
        self.processes.lock().unwrap().len()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn register(&self, program: String, args: Vec<String>) -> std::io::Result<StreamHandle> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "exec_stream state is shutting down",
            ));
        }
        self.resources
            .acquire_child()
            .map_err(|error: ResourceLimit| std::io::Error::other(error.to_string()))?;
        Ok(StreamHandle::new(
            StreamId(NEXT_STREAM_ID.fetch_add(1, Ordering::Relaxed)),
            program,
            args,
            self.generation,
        ))
    }

    fn insert_process(&self, process: StreamProcess) {
        self.processes
            .lock()
            .unwrap()
            .insert(process.stream.id(), process);
    }

    fn request_stop(&self, id: StreamId) {
        if let Some(process) = self.processes.lock().unwrap().get_mut(&id) {
            if let Some(stop) = process.stop.take() {
                let _ = stop.send(());
            }
        }
    }

    fn push_event(&self, event: StreamEvent) {
        let id = event.stream.id();
        let output_bytes = event.queued_output_bytes();
        event.stream.set_status(match &event.kind {
            StreamEventKind::Started => StreamStatus::Running,
            StreamEventKind::Eof => StreamStatus::Eof,
            StreamEventKind::Failed(message) => StreamStatus::Failed {
                message: message.clone(),
            },
            StreamEventKind::Exited(status) => StreamStatus::Exited(status.clone()),
            StreamEventKind::Line(_) | StreamEventKind::Overflow { .. } => event.stream.status(),
        });

        if self.resources.reserve_queue().is_err() {
            self.report_overflow(id, 1);
            return;
        }
        if let Err(error) = self.resources.reserve_queued_output(output_bytes) {
            self.resources.release_queue(1);
            tracing::warn!(
                stream_id = id.raw(),
                program = %event.stream.program(),
                error = %error,
                "exec_stream output budget exceeded; dropping record"
            );
            self.report_overflow(id, 1);
            return;
        }
        self.pending.lock().unwrap().push_back(event);
        self.notify.notify_one();
    }

    fn report_overflow(&self, id: StreamId, dropped: u64) {
        if self.overflow_reported.lock().unwrap().contains(&id) {
            return;
        }
        let stream = self
            .processes
            .lock()
            .unwrap()
            .get(&id)
            .map(|process| process.stream.clone());
        let Some(stream) = stream else {
            return;
        };
        if self.resources.reserve_queue().is_err() {
            // Preserve one bounded diagnostic record even when a burst has
            // filled the queue: dropping the oldest queued record is the
            // explicit overflow policy, and keeps the queue within budget.
            let removed = self.pending.lock().unwrap().pop_front();
            if let Some(removed) = removed {
                self.resources.release_queue(1);
                self.resources
                    .release_queued_output(removed.queued_output_bytes());
            }
            if self.resources.reserve_queue().is_err() {
                return;
            }
        }
        self.overflow_reported.lock().unwrap().insert(id);
        self.pending.lock().unwrap().push_back(StreamEvent {
            stream,
            kind: StreamEventKind::Overflow { dropped },
        });
        self.notify.notify_one();
    }

    fn clear_pending(&self) {
        let pending = self.pending.lock().unwrap().drain(..).collect::<Vec<_>>();
        self.resources.release_queue(pending.len());
        self.resources.release_queued_output(
            pending
                .iter()
                .map(StreamEvent::queued_output_bytes)
                .sum::<usize>(),
        );
    }

    fn reap_finished(&self) {
        self.processes
            .lock()
            .unwrap()
            .retain(|_, process| !process.task.is_finished());
    }
}

impl Default for StreamState {
    fn default() -> Self {
        Self::with_budget(ResourceBudget::new(
            mesh_core_runtime::SandboxConfig::default(),
        ))
    }
}

fn release_child_once(resources: &ResourceBudget, accounted: &AtomicBool) {
    if !accounted.swap(true, Ordering::AcqRel) {
        resources.release_child();
    }
}

struct ChildBudgetGuard {
    resources: ResourceBudget,
    accounted: Arc<AtomicBool>,
}

impl Drop for ChildBudgetGuard {
    fn drop(&mut self) {
        release_child_once(&self.resources, &self.accounted);
    }
}

/// Spawn a subprocess and return its stable stream handle.
#[allow(dead_code)]
pub fn spawn_stream(
    state: &Arc<StreamState>,
    program: String,
    args: Vec<String>,
) -> std::io::Result<StreamHandle> {
    spawn_stream_with_launch_program(state, program.clone(), args, program.clone(), program)
}

/// Spawn a stream while retaining the author-facing program in its handle but
/// launching the already-authorized canonical path.
pub(crate) fn spawn_stream_with_launch_program(
    state: &Arc<StreamState>,
    program: String,
    args: Vec<String>,
    launch_program: String,
    argv0: String,
) -> std::io::Result<StreamHandle> {
    let stream = state.register(program, args)?;
    let mut command = Command::new(&launch_program);
    command
        .args(stream.args())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true);
    configure_argv0(&mut command, &argv0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            state.resources.release_child();
            return Err(error);
        }
    };
    let stdout = child.stdout.take().ok_or_else(|| {
        state.resources.release_child();
        std::io::Error::other("subprocess stdout was not piped")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        state.resources.release_child();
        std::io::Error::other("subprocess stderr was not piped")
    })?;
    let (stop_tx, stop_rx) = oneshot::channel();
    let accounted = Arc::new(AtomicBool::new(false));
    let task_state = Arc::clone(state);
    let task_stream = stream.clone();
    let task_accounted = Arc::clone(&accounted);
    let task_resources = state.resources.clone();
    let task = tokio::runtime::Handle::try_current()
        .map_err(|_| std::io::Error::other("exec_stream requires a Tokio runtime"))?
        .spawn(async move {
            let _budget = ChildBudgetGuard {
                resources: task_resources,
                accounted: task_accounted,
            };
            run_stream(task_state, task_stream, child, stdout, stderr, stop_rx).await;
        });

    state.insert_process(StreamProcess {
        stream: stream.clone(),
        stop: Some(stop_tx),
        task,
        child_accounted: accounted,
    });
    state.push_event(StreamEvent {
        stream: stream.clone(),
        kind: StreamEventKind::Started,
    });
    Ok(stream)
}

fn configure_argv0(command: &mut Command, argv0: &str) {
    #[cfg(unix)]
    command.arg0(argv0);
    #[cfg(not(unix))]
    let _ = (command, argv0);
}

async fn run_stream(
    state: Arc<StreamState>,
    stream: StreamHandle,
    mut child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let stdout_state = Arc::clone(&state);
    let stdout_stream = stream.clone();
    let stdout_task = tokio::spawn(async move {
        read_stdout(stdout_state, stdout_stream, stdout).await;
    });

    let stderr_state = Arc::clone(&state);
    let stderr_stream = stream.clone();
    let stderr_task =
        tokio::spawn(async move { read_stderr(stderr_state, stderr_stream, stderr).await });
    let mut stop_requested = false;
    let status = loop {
        if stop_requested {
            break child.wait().await;
        }
        tokio::select! {
            result = child.wait() => break result,
            _ = &mut stop_rx => {
                stop_requested = true;
                stream.set_status(StreamStatus::Stopping);
                let _ = child.start_kill();
            }
        }
    };

    let _ = stdout_task.await;
    let stderr_result = stderr_task.await;
    let stderr = match stderr_result {
        Ok(Ok(stderr)) => stderr,
        Ok(Err(_error)) => String::new(),
        Err(error) => {
            state.push_event(StreamEvent {
                stream: stream.clone(),
                kind: StreamEventKind::Failed(format!("stderr reader task failed: {error}")),
            });
            String::new()
        }
    };

    match status {
        Ok(status) => {
            let exit = StreamExitStatus {
                code: status.code(),
                success: status.success(),
                signal: exit_signal(&status),
            };
            if !exit.success {
                let message = if stderr.is_empty() {
                    format!("stream exited with status {:?}", exit.code)
                } else {
                    format!("stream exited unsuccessfully: {stderr}")
                };
                state.push_event(StreamEvent {
                    stream: stream.clone(),
                    kind: StreamEventKind::Failed(message),
                });
            } else if !stderr.is_empty() {
                tracing::debug!(
                    stream_id = stream.id().raw(),
                    program = %stream.program(),
                    stderr = %stderr,
                    "exec_stream subprocess wrote to stderr"
                );
            }
            state.push_event(StreamEvent {
                stream,
                kind: StreamEventKind::Exited(exit),
            });
        }
        Err(error) => {
            state.push_event(StreamEvent {
                stream,
                kind: StreamEventKind::Failed(format!("waiting for stream failed: {error}")),
            });
        }
    }
}

async fn read_stdout(state: Arc<StreamState>, stream: StreamHandle, stdout: ChildStdout) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut bytes = Vec::new();
        let mut limited_reader = reader.take((MAX_STREAM_LINE_BYTES + 1) as u64);
        let result = limited_reader.read_until(b'\n', &mut bytes).await;
        reader = limited_reader.into_inner();
        match result {
            Ok(0) => {
                state.push_event(StreamEvent {
                    stream,
                    kind: StreamEventKind::Eof,
                });
                return;
            }
            Ok(_) => {
                if bytes.len() == MAX_STREAM_LINE_BYTES + 1 && bytes.last() != Some(&b'\n') {
                    let message = format!("stream line exceeded {} bytes", MAX_STREAM_LINE_BYTES);
                    state.push_event(StreamEvent {
                        stream: stream.clone(),
                        kind: StreamEventKind::Failed(message),
                    });
                    state.request_stop(stream.id());
                    return;
                }
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                }
                if bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
                let line = String::from_utf8_lossy(&bytes).into_owned();
                state.push_event(StreamEvent {
                    stream: stream.clone(),
                    kind: StreamEventKind::Line(line),
                });
            }
            Err(error) => {
                state.push_event(StreamEvent {
                    stream: stream.clone(),
                    kind: StreamEventKind::Failed(format!("stream stdout read failed: {error}")),
                });
                state.request_stop(stream.id());
                return;
            }
        }
    }
}

async fn read_stderr(
    state: Arc<StreamState>,
    stream: StreamHandle,
    stderr: ChildStderr,
) -> std::io::Result<String> {
    let mut reader = BufReader::new(stderr);
    let mut output = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(String::from_utf8_lossy(&output).into_owned());
        }
        if output.len() + read > MAX_STREAM_ERROR_BYTES {
            let error = std::io::Error::other(format!(
                "stream stderr exceeded {} bytes",
                MAX_STREAM_ERROR_BYTES
            ));
            state.push_event(StreamEvent {
                stream: stream.clone(),
                kind: StreamEventKind::Failed(error.to_string()),
            });
            state.request_stop(stream.id());
            return Err(error);
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

#[cfg(unix)]
fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(status)
}

#[cfg(not(unix))]
fn exit_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn streams_have_distinct_handles_even_for_same_program() {
        let state = StreamState::new();
        let first = spawn_stream(
            &state,
            "sh".to_string(),
            vec!["-c".to_string(), "printf 'first\\n'".to_string()],
        )
        .expect("first spawn");
        let second = spawn_stream(
            &state,
            "sh".to_string(),
            vec!["-c".to_string(), "printf 'second\\n'".to_string()],
        )
        .expect("second spawn");
        assert_ne!(first.id(), second.id());
        assert_eq!(first.program(), second.program());
        assert_ne!(first.args(), second.args());
        assert_eq!(first.generation(), state.generation());

        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        let mut events = Vec::new();
        while tokio::time::Instant::now() < deadline {
            state.wait_for_event().await;
            events.extend(state.drain_events());
            if events
                .iter()
                .filter(|event| matches!(event.kind, StreamEventKind::Exited(_)))
                .count()
                == 2
            {
                break;
            }
        }
        let lines: Vec<_> = events
            .iter()
            .filter_map(|event| match &event.kind {
                StreamEventKind::Line(line) => Some((event.stream.id(), line.as_str())),
                _ => None,
            })
            .collect();
        assert!(lines.contains(&(first.id(), "first")));
        assert!(lines.contains(&(second.id(), "second")));
        assert_eq!(state.active_stream_count(), 0);
    }

    #[tokio::test]
    async fn stream_emits_eof_and_exit_and_shutdown_reaps_children() {
        let state = StreamState::new();
        let stream = spawn_stream(
            &state,
            "sh".to_string(),
            vec!["-c".to_string(), "printf 'done\\n'".to_string()],
        )
        .expect("spawn");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        let mut events = Vec::new();
        while tokio::time::Instant::now() < deadline {
            state.wait_for_event().await;
            events.extend(state.drain_events());
            if events.iter().any(|event| {
                event.stream.id() == stream.id() && matches!(event.kind, StreamEventKind::Exited(_))
            }) {
                break;
            }
        }
        assert!(events.iter().any(|event| {
            event.stream.id() == stream.id() && matches!(event.kind, StreamEventKind::Eof)
        }));
        assert!(matches!(stream.status(), StreamStatus::Exited(_)));

        let running = spawn_stream(
            &state,
            "sh".to_string(),
            vec!["-c".to_string(), "sleep 60".to_string()],
        )
        .expect("running spawn");
        assert_eq!(state.active_stream_count(), 1);
        state.shutdown().await;
        assert_eq!(state.active_stream_count(), 0);
        assert!(matches!(running.status(), StreamStatus::Exited(_)));
    }

    #[tokio::test]
    async fn high_rate_stream_queue_is_bounded_and_reports_overflow() {
        let mut config = mesh_core_runtime::SandboxConfig::default();
        config.queue_budget = 2;
        config.output_budget = 32;
        let state = StreamState::new_with_budget(ResourceBudget::new(config));
        let _stream = spawn_stream(
            &state,
            "sh".to_string(),
            vec![
                "-c".to_string(),
                "printf '1234567890\\n%.0s' $(seq 1 20)".to_string(),
            ],
        )
        .expect("spawn");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let events = state.drain_events();
        assert!(events.len() <= 2);
        assert!(
            events
                .iter()
                .any(|event| matches!(event.kind, StreamEventKind::Overflow { .. }))
        );
        state.shutdown().await;
    }

    #[tokio::test]
    async fn nonzero_exit_emits_failed_before_exit() {
        let state = StreamState::new();
        let stream = spawn_stream(
            &state,
            "sh".to_string(),
            vec!["-c".to_string(), "printf 'bad\\n' >&2; exit 7".to_string()],
        )
        .expect("spawn");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        let mut events = Vec::new();
        while tokio::time::Instant::now() < deadline {
            state.wait_for_event().await;
            events.extend(state.drain_events());
            if events.iter().any(|event| {
                event.stream.id() == stream.id() && matches!(event.kind, StreamEventKind::Exited(_))
            }) {
                break;
            }
        }
        let failed = events.iter().position(|event| {
            event.stream.id() == stream.id() && matches!(event.kind, StreamEventKind::Failed(_))
        });
        let exited = events.iter().position(|event| {
            event.stream.id() == stream.id() && matches!(event.kind, StreamEventKind::Exited(_))
        });
        assert!(failed < exited);
        assert!(matches!(stream.status(), StreamStatus::Exited(_)));
    }
}
