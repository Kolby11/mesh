/// Module lifecycle state machine.
use crate::manifest::{Manifest, ManifestSource};
use std::path::PathBuf;
use std::time::Instant;

/// The states a module moves through during its lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    Discovered,
    Resolved,
    Loaded,
    Initialized,
    Running,
    Suspended,
    Unloaded,
    Errored,
}

impl std::fmt::Display for ModuleState {
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

/// A live module instance tracked by the core.
#[derive(Debug)]
pub struct ModuleInstance {
    pub manifest: Manifest,
    pub path: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest_source: ManifestSource,
    pub state: ModuleState,
    pub error_count: u32,
    pub last_error: Option<String>,
    pub loaded_at: Option<Instant>,
}

impl ModuleInstance {
    pub fn new(
        manifest: Manifest,
        path: PathBuf,
        manifest_path: PathBuf,
        manifest_source: ManifestSource,
    ) -> Self {
        Self {
            manifest,
            path,
            manifest_path,
            manifest_source,
            state: ModuleState::Discovered,
            error_count: 0,
            last_error: None,
            loaded_at: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.manifest.package.id
    }

    /// Transition to a new state, enforcing valid transitions.
    pub fn transition(&mut self, to: ModuleState) -> Result<(), LifecycleError> {
        let valid = matches!(
            (self.state, to),
            (ModuleState::Discovered, ModuleState::Resolved)
                | (ModuleState::Resolved, ModuleState::Loaded)
                | (ModuleState::Loaded, ModuleState::Initialized)
                | (ModuleState::Initialized, ModuleState::Running)
                | (ModuleState::Running, ModuleState::Suspended)
                | (ModuleState::Suspended, ModuleState::Running)
                | (ModuleState::Running, ModuleState::Unloaded)
                | (ModuleState::Suspended, ModuleState::Unloaded)
                | (_, ModuleState::Errored)
        );

        if !valid {
            return Err(LifecycleError::InvalidTransition {
                module_id: self.id().to_string(),
                from: self.state,
                to,
            });
        }

        if to == ModuleState::Loaded {
            self.loaded_at = Some(Instant::now());
        }

        if to == ModuleState::Errored {
            self.error_count += 1;
        }

        self.state = to;
        Ok(())
    }

    /// Whether the module has errored too many times and should be disabled.
    pub fn should_disable(&self) -> bool {
        self.error_count >= 3
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    #[error("invalid state transition for module '{module_id}': {from} -> {to}")]
    InvalidTransition {
        module_id: String,
        from: ModuleState,
        to: ModuleState,
    },
}
