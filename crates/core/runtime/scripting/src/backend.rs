mod command;
mod errors;
mod event;
mod exec;
mod exec_stream;
mod logging;
mod runtime;

pub const MIN_POLL_INTERVAL_MS: u64 = 50;

pub use command::{
    BackendCommandArgument, BackendCommandOutcome, BackendCommandRegistry, BackendCommandSpec,
};
pub use errors::BackendScriptError;
pub use event::{BackendEventRegistry, BackendEventSpec};
pub use exec_stream::{
    StreamEvent, StreamEventKind, StreamExitStatus, StreamHandle, StreamId, StreamLine,
    StreamState, StreamStatus,
};
pub use runtime::{BackendScriptContext, BackendScriptEvent};

#[cfg(test)]
mod tests;
