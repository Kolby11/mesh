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

/// Default root for durable runtime storage when no explicit root is
/// configured: `$XDG_STATE_HOME/mesh/runtime-storage`, falling back to
/// `$HOME/.local/state/mesh/runtime-storage`.
#[cfg(not(test))]
pub(crate) fn default_runtime_storage_root() -> PathBuf {
    runtime_storage_root(
        non_empty_absolute_env("XDG_STATE_HOME"),
        std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .map(PathBuf::from),
        std::env::temp_dir().join("mesh-state"),
    )
}

#[cfg(test)]
pub(crate) fn default_runtime_storage_root() -> PathBuf {
    let root = std::env::temp_dir()
        .join("mesh-runtime-storage-tests")
        .join(std::process::id().to_string());
    let _ = std::fs::create_dir_all(&root);
    root
}

#[cfg(not(test))]
fn non_empty_absolute_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn runtime_storage_root(
    xdg_state_home: Option<PathBuf>,
    home: Option<PathBuf>,
    fallback: PathBuf,
) -> PathBuf {
    let state_home = xdg_state_home
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.filter(|path| path.is_absolute())
                .map(|path| path.join(".local").join("state"))
        })
        .unwrap_or(fallback);
    state_home.join("mesh").join("runtime-storage")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_storage_prefers_absolute_xdg_state_home() {
        assert_eq!(
            runtime_storage_root(
                Some(PathBuf::from("/state")),
                Some(PathBuf::from("/home/user")),
                PathBuf::from("/tmp/fallback"),
            ),
            PathBuf::from("/state/mesh/runtime-storage")
        );
    }

    #[test]
    fn runtime_storage_falls_back_to_home_then_temp() {
        assert_eq!(
            runtime_storage_root(
                Some(PathBuf::from("relative-state")),
                Some(PathBuf::from("/home/user")),
                PathBuf::from("/tmp/fallback"),
            ),
            PathBuf::from("/home/user/.local/state/mesh/runtime-storage")
        );
        assert_eq!(
            runtime_storage_root(
                Some(PathBuf::from("relative-state")),
                Some(PathBuf::from("relative-home")),
                PathBuf::from("/tmp/fallback"),
            ),
            PathBuf::from("/tmp/fallback/mesh/runtime-storage")
        );
    }
}
