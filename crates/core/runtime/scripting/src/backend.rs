mod command;
mod errors;
mod exec;
mod exec_stream;
mod logging;
mod runtime;

pub const MIN_POLL_INTERVAL_MS: u64 = 50;

pub use command::BackendCommandOutcome;
pub use errors::BackendScriptError;
pub use exec_stream::{
    StreamEvent, StreamEventKind, StreamExitStatus, StreamHandle, StreamId, StreamLine,
    StreamState, StreamStatus,
};
pub use runtime::{BackendScriptContext, BackendScriptEvent};

#[cfg(test)]
mod tests;
