//! Long-running subprocess streams for backend modules.
//!
//! `mesh.exec_stream(program, args)` spawns a subprocess and asynchronously
//! reads its stdout line-by-line. Each line is queued in `StreamState`; the
//! backend service loop drains pending lines on every wakeup and invokes
//! `on_stream_batch(self, program, lines)` once per program with the full
//! ordered batch, or falls back to `on_stream_line(self, program, line)` once
//! per line if only the legacy hook is defined.
//!
//! This is the event-driven counterpart to `mesh.exec`. It enables backends to
//! react to external event sources (e.g. `pactl subscribe`, `pw-mon`,
//! `journalctl -f`) instead of polling.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// One pending line from a streaming subprocess.
#[derive(Debug, Clone)]
pub struct StreamLine {
    pub program: String,
    pub line: String,
}

/// Shared state between the backend script context (producer of stream
/// registrations) and the backend service loop (consumer of stream events).
#[derive(Debug, Default)]
pub struct StreamState {
    pending: Mutex<VecDeque<StreamLine>>,
    notify: Notify,
    processes: Mutex<Vec<StreamProcess>>,
}

#[derive(Debug)]
struct StreamProcess {
    /// Kept for diagnostics — drop kills the subprocess via `kill_on_drop(true)`.
    #[allow(dead_code)]
    program: String,
    child: Child,
    reader: JoinHandle<()>,
}

impl StreamState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Wait until at least one new line is available or a stream lifecycle
    /// event fires. The main backend loop awaits this in its `tokio::select!`.
    pub async fn wait_for_event(&self) {
        self.notify.notified().await;
    }

    /// Drain all pending lines. Called after `wait_for_event()` returns.
    pub fn drain_lines(&self) -> Vec<StreamLine> {
        self.pending.lock().unwrap().drain(..).collect()
    }

    /// Kill every active subprocess (best effort). Called on backend stop.
    pub fn kill_all(&self) {
        let mut processes = self.processes.lock().unwrap();
        for mut entry in processes.drain(..) {
            entry.reader.abort();
            let _ = entry.child.start_kill();
        }
    }

    /// Number of active stream subprocesses. Used for tests.
    #[cfg(test)]
    pub fn active_stream_count(&self) -> usize {
        self.processes.lock().unwrap().len()
    }

    fn push_line(&self, line: StreamLine) {
        self.pending.lock().unwrap().push_back(line);
        self.notify.notify_one();
    }
}

/// Spawn a subprocess and start reading its stdout. Returns Err if the
/// subprocess cannot be spawned (e.g. binary not found). The reader task is
/// owned by `StreamState`; it terminates when the subprocess exits, stdout
/// EOFs, or `kill_all()` is called.
pub fn spawn_stream(
    state: &Arc<StreamState>,
    program: String,
    args: Vec<String>,
) -> std::io::Result<()> {
    let mut child = Command::new(&program)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("subprocess stdout was not piped"))?;

    let state_for_reader = Arc::clone(state);
    let program_for_reader = program.clone();
    let reader = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    state_for_reader.push_line(StreamLine {
                        program: program_for_reader.clone(),
                        line,
                    });
                }
                Ok(None) => {
                    tracing::debug!(
                        program = %program_for_reader,
                        "exec_stream subprocess stdout EOF"
                    );
                    break;
                }
                Err(err) => {
                    tracing::warn!(
                        program = %program_for_reader,
                        "exec_stream stdout read failed: {err}"
                    );
                    break;
                }
            }
        }
        // Wake the main loop so it has a chance to notice the stream ended
        // and clear any drained queue state.
        state_for_reader.notify.notify_one();
    });

    state.processes.lock().unwrap().push(StreamProcess {
        program,
        child,
        reader,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn spawn_stream_reads_subprocess_stdout_line_by_line() {
        let state = StreamState::new();
        spawn_stream(
            &state,
            "sh".to_string(),
            vec![
                "-c".to_string(),
                "printf 'first\\nsecond\\nthird\\n'".to_string(),
            ],
        )
        .expect("spawn");

        let mut all_lines = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while all_lines.len() < 3 && tokio::time::Instant::now() < deadline {
            tokio::select! {
                _ = state.wait_for_event() => {}
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
            all_lines.extend(state.drain_lines());
        }
        assert_eq!(all_lines.len(), 3);
        assert_eq!(all_lines[0].program, "sh");
        assert_eq!(all_lines[0].line, "first");
        assert_eq!(all_lines[1].line, "second");
        assert_eq!(all_lines[2].line, "third");
    }

    #[tokio::test]
    async fn kill_all_terminates_running_subprocess() {
        let state = StreamState::new();
        spawn_stream(
            &state,
            "sh".to_string(),
            vec!["-c".to_string(), "sleep 60".to_string()],
        )
        .expect("spawn");
        assert_eq!(state.active_stream_count(), 1);
        state.kill_all();
        assert_eq!(state.active_stream_count(), 0);
    }
}
