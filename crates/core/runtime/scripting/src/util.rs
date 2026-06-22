//! Small helpers shared between the frontend (`context`) and backend
//! scripting runtimes so their behaviour can never drift.

use std::path::PathBuf;

/// A `self.<Event>` / interface event channel name: PascalCase identifier
/// (leading ASCII uppercase, then alphanumerics or underscores).
pub(crate) fn is_named_event_channel(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Default root for per-process runtime storage when no explicit root is
/// configured: `<tmp>/mesh/runtime-storage/<pid>`.
pub(crate) fn default_runtime_storage_root() -> PathBuf {
    std::env::temp_dir()
        .join("mesh")
        .join("runtime-storage")
        .join(std::process::id().to_string())
}
