mod command;
mod errors;
mod event;
mod exec;
mod exec_stream;
mod host_sources;
mod logging;
mod runtime;
mod task_io;

pub const MIN_POLL_INTERVAL_MS: u64 = 50;

pub use command::{
    BackendCommandArgument, BackendCommandOutcome, BackendCommandRun, BackendCommandRegistry, BackendCommandSpec,
};
pub use errors::BackendScriptError;
pub use event::{BackendEventRegistry, BackendEventSpec};
pub use exec_stream::{
    StreamEvent, StreamEventKind, StreamExitStatus, StreamHandle, StreamId, StreamLine,
    StreamState, StreamStatus,
};
pub use host_sources::{
    SOCKET_CAPABILITY, WATCH_CAPABILITY, WATCH_MODIFIED_LINE, socket_stream_program, watch_program,
};
pub use runtime::{BackendScriptContext, BackendScriptEvent, BackendTaskRun};

#[cfg(test)]
mod tests;
