mod command;
mod errors;
mod exec;
mod logging;
mod runtime;

pub const MIN_POLL_INTERVAL_MS: u64 = 50;

pub use command::BackendCommandOutcome;
pub use errors::BackendScriptError;
pub use runtime::BackendScriptContext;

#[cfg(test)]
mod tests;
