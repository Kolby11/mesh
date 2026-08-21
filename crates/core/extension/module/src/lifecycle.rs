/// Module lifecycle state machine.
use crate::manifest::{Manifest, ManifestSource};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

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
    /// Benched by the runtime supervisor after repeated recoverable failures.
    Quarantined,
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
            Self::Quarantined => write!(f, "quarantined"),
        }
    }
}

/// The coarse health state exposed by the module/runtime boundary.
///
/// Graph health and runtime health are kept separately on [`ModuleInstance`]
/// so a provider crash cannot mutate the resolved graph, and a successful
/// restart cannot hide a static dependency problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleHealthState {
    Healthy,
    Degraded,
    Unavailable,
}

impl std::fmt::Display for ModuleHealthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// A health record with enough context for diagnostics and availability
/// delivery. `since` changes only when the state changes, not on repeated
/// status refreshes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleHealthRecord {
    pub state: ModuleHealthState,
    pub reason: Option<String>,
    pub recoverable: bool,
    pub since: SystemTime,
}

impl Default for ModuleHealthRecord {
    fn default() -> Self {
        Self::healthy()
    }
}

impl ModuleHealthRecord {
    pub fn healthy() -> Self {
        Self::new(ModuleHealthState::Healthy, None, false)
    }

    pub fn degraded(reason: impl Into<String>) -> Self {
        Self::new(ModuleHealthState::Degraded, Some(reason.into()), true)
    }

    pub fn unavailable(reason: impl Into<String>, recoverable: bool) -> Self {
        Self::new(
            ModuleHealthState::Unavailable,
            Some(reason.into()),
            recoverable,
        )
    }

    fn new(state: ModuleHealthState, reason: Option<String>, recoverable: bool) -> Self {
        Self {
            state,
            reason,
            recoverable,
            since: SystemTime::now(),
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
    /// Health derived from the last committed installed graph.
    pub static_health: ModuleHealthRecord,
    /// Health reported by the live frontend/backend runtime.
    pub runtime_health: ModuleHealthRecord,
    /// Set by the lifecycle supervisor; quarantined modules cannot be
    /// restarted until an explicit graph/configuration change clears it.
    pub quarantined: bool,
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
            static_health: ModuleHealthRecord::healthy(),
            runtime_health: ModuleHealthRecord::healthy(),
            quarantined: false,
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
                | (ModuleState::Loaded, ModuleState::Unloaded)
                | (ModuleState::Initialized, ModuleState::Unloaded)
                | (ModuleState::Resolved, ModuleState::Unloaded)
                | (ModuleState::Errored, ModuleState::Unloaded)
                | (ModuleState::Quarantined, ModuleState::Unloaded)
                | (_, ModuleState::Quarantined)
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
            self.runtime_health = ModuleHealthRecord::unavailable(
                self.last_error
                    .clone()
                    .unwrap_or_else(|| "module runtime failed".to_string()),
                true,
            );
        }

        if to == ModuleState::Quarantined {
            self.quarantined = true;
            self.runtime_health = ModuleHealthRecord::unavailable(
                self.last_error
                    .clone()
                    .unwrap_or_else(|| "module runtime quarantined".to_string()),
                false,
            );
        }

        if to == ModuleState::Unloaded {
            self.runtime_health = ModuleHealthRecord::unavailable("module unloaded", true);
        }

        self.state = to;
        Ok(())
    }

    /// Whether the module has errored too many times and should be disabled.
    pub fn should_disable(&self) -> bool {
        self.error_count >= 3
    }

    /// Return the effective health without allowing runtime recovery to mask
    /// a graph-level dependency or capability failure.
    pub fn health(&self) -> ModuleHealthRecord {
        if self.runtime_health.state == ModuleHealthState::Unavailable {
            return self.runtime_health.clone();
        }
        if self.static_health.state == ModuleHealthState::Unavailable {
            return self.static_health.clone();
        }
        if self.runtime_health.state == ModuleHealthState::Degraded {
            return self.runtime_health.clone();
        }
        self.static_health.clone()
    }

    pub fn set_static_health(&mut self, health: ModuleHealthRecord) {
        self.static_health = health;
    }

    pub fn mark_loaded(&mut self) -> Result<(), LifecycleError> {
        if self.quarantined {
            return Err(LifecycleError::Quarantined {
                module_id: self.id().to_string(),
            });
        }
        if matches!(self.state, ModuleState::Errored | ModuleState::Unloaded) {
            self.state = ModuleState::Resolved;
        }
        if self.state == ModuleState::Resolved {
            self.transition(ModuleState::Loaded)?;
        }
        Ok(())
    }

    pub fn mark_initialized(&mut self) -> Result<(), LifecycleError> {
        if self.state == ModuleState::Loaded {
            self.transition(ModuleState::Initialized)?;
        }
        Ok(())
    }

    pub fn mark_running(&mut self) -> Result<(), LifecycleError> {
        if self.quarantined {
            return Err(LifecycleError::Quarantined {
                module_id: self.id().to_string(),
            });
        }
        if self.state == ModuleState::Discovered {
            self.transition(ModuleState::Resolved)?;
        }
        self.mark_loaded()?;
        if self.state == ModuleState::Loaded {
            self.transition(ModuleState::Initialized)?;
        }
        if self.state == ModuleState::Initialized || self.state == ModuleState::Suspended {
            self.transition(ModuleState::Running)?;
        }
        self.runtime_health = ModuleHealthRecord::healthy();
        Ok(())
    }

    pub fn mark_degraded(&mut self, reason: impl Into<String>) {
        self.runtime_health = ModuleHealthRecord::degraded(reason);
    }

    pub fn mark_failed(&mut self, reason: impl Into<String>) -> Result<(), LifecycleError> {
        let reason = reason.into();
        self.last_error = Some(reason.clone());
        if self.state != ModuleState::Errored {
            self.transition(ModuleState::Errored)?;
        } else {
            self.error_count = self.error_count.saturating_add(1);
            self.runtime_health = ModuleHealthRecord::unavailable(reason, true);
        }
        Ok(())
    }

    pub fn mark_unloaded(&mut self) -> Result<(), LifecycleError> {
        if self.state != ModuleState::Unloaded {
            self.transition(ModuleState::Unloaded)?;
        }
        Ok(())
    }

    pub fn mark_quarantined(&mut self, reason: impl Into<String>) -> Result<(), LifecycleError> {
        let reason = reason.into();
        self.last_error = Some(reason.clone());
        self.runtime_health = ModuleHealthRecord::unavailable(reason, false);
        self.quarantined = true;
        if self.state != ModuleState::Quarantined {
            self.transition(ModuleState::Quarantined)?;
        }
        Ok(())
    }

    /// A deliberate re-enable/provider selection clears quarantine and makes
    /// the module eligible for a fresh load. It does not clear static graph
    /// health or historical error diagnostics.
    pub fn clear_quarantine(&mut self) {
        self.quarantined = false;
        if self.state == ModuleState::Quarantined {
            self.state = ModuleState::Resolved;
        }
        self.runtime_health = ModuleHealthRecord::healthy();
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
    #[error("module '{module_id}' is quarantined")]
    Quarantined { module_id: String },
}
