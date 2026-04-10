/// Plugin lifecycle state machine.
use crate::manifest::Manifest;
use std::path::PathBuf;
use std::time::Instant;

/// The states a plugin moves through during its lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    Discovered,
    Resolved,
    Loaded,
    Initialized,
    Running,
    Suspended,
    Unloaded,
    Errored,
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovered => write!(f, "discovered"),
            Self::Resolved => write!(f, "resolved"),
            Self::Loaded => write!(f, "loaded"),
            Self::Initialized => write!(f, "initialized"),
            Self::Running => write!(f, "running"),
            Self::Suspended => write!(f, "suspended"),
            Self::Unloaded => write!(f, "unloaded"),
            Self::Errored => write!(f, "errored"),
        }
    }
}

/// A live plugin instance tracked by the core.
#[derive(Debug)]
pub struct PluginInstance {
    pub manifest: Manifest,
    pub path: PathBuf,
    pub state: PluginState,
    pub error_count: u32,
    pub last_error: Option<String>,
    pub loaded_at: Option<Instant>,
}

impl PluginInstance {
    pub fn new(manifest: Manifest, path: PathBuf) -> Self {
        Self {
            manifest,
            path,
            state: PluginState::Discovered,
            error_count: 0,
            last_error: None,
            loaded_at: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.manifest.package.id
    }

    /// Transition to a new state, enforcing valid transitions.
    pub fn transition(&mut self, to: PluginState) -> Result<(), LifecycleError> {
        let valid = matches!(
            (self.state, to),
            (PluginState::Discovered, PluginState::Resolved)
                | (PluginState::Resolved, PluginState::Loaded)
                | (PluginState::Loaded, PluginState::Initialized)
                | (PluginState::Initialized, PluginState::Running)
                | (PluginState::Running, PluginState::Suspended)
                | (PluginState::Suspended, PluginState::Running)
                | (PluginState::Running, PluginState::Unloaded)
                | (PluginState::Suspended, PluginState::Unloaded)
                | (_, PluginState::Errored)
        );

        if !valid {
            return Err(LifecycleError::InvalidTransition {
                plugin_id: self.id().to_string(),
                from: self.state,
                to,
            });
        }

        if to == PluginState::Loaded {
            self.loaded_at = Some(Instant::now());
        }

        if to == PluginState::Errored {
            self.error_count += 1;
        }

        self.state = to;
        Ok(())
    }

    /// Whether the plugin has errored too many times and should be disabled.
    pub fn should_disable(&self) -> bool {
        self.error_count >= 3
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    #[error("invalid state transition for plugin '{plugin_id}': {from} -> {to}")]
    InvalidTransition {
        plugin_id: String,
        from: PluginState,
        to: PluginState,
    },
}
