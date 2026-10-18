//! The per-module runtime boundary shared by frontend and backend adapters.
//!
//! A [`RuntimeSession`] is deliberately narrower than either adapter.  It owns
//! the identity and policy that must remain stable across callbacks, while the
//! adapters retain their frontend/backend-specific host operations.  Keeping
//! these pieces together keeps realm initialization and resource accounting
//! consistent. Provider supervision belongs to the shell.

use crate::policy::{ExecutionGuard, FramePause, ResourceBudget, RuntimePolicy};
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_runtime::SandboxConfig;
use mlua::Lua;
use serde_json::Value;
use std::time::{Duration, SystemTime};

pub use crate::policy::{ResourceKind, ResourceLimit};

/// Configuration shared by every runtime session owned by a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionConfig {
    pub sandbox: SandboxConfig,
    pub generation: u64,
}

impl Default for RuntimeSessionConfig {
    fn default() -> Self {
        Self {
            sandbox: SandboxConfig::default(),
            generation: 0,
        }
    }
}

/// The lifecycle states of one runtime generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLifecycleState {
    Created,
    Loaded,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

impl RuntimeLifecycleState {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Loaded | Self::Stopping | Self::Failed)
                | (Self::Loaded, Self::Starting | Self::Stopping | Self::Failed)
                | (
                    Self::Starting,
                    Self::Running | Self::Stopping | Self::Failed
                )
                | (Self::Running, Self::Stopping | Self::Failed)
                | (Self::Stopping, Self::Stopped | Self::Failed)
                | (Self::Failed, Self::Stopping | Self::Loaded)
        )
    }
}

impl std::fmt::Display for RuntimeLifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Created => "created",
            Self::Loaded => "loaded",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        };
        f.write_str(value)
    }
}

/// Lifecycle bookkeeping kept inside a session rather than in an adapter.
#[derive(Debug, Clone)]
pub struct RuntimeLifecycle {
    state: RuntimeLifecycleState,
    changed_at: SystemTime,
}

impl Default for RuntimeLifecycle {
    fn default() -> Self {
        Self {
            state: RuntimeLifecycleState::Created,
            changed_at: SystemTime::now(),
        }
    }
}

impl RuntimeLifecycle {
    pub fn state(&self) -> RuntimeLifecycleState {
        self.state
    }

    pub fn changed_at(&self) -> SystemTime {
        self.changed_at
    }

    fn transition(&mut self, next: RuntimeLifecycleState) -> Result<(), RuntimeSessionError> {
        if self.state == next {
            return Ok(());
        }
        if !self.state.can_transition_to(next) {
            return Err(RuntimeSessionError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        self.changed_at = SystemTime::now();
        Ok(())
    }

    fn mark_loaded(&mut self) -> Result<(), RuntimeSessionError> {
        self.transition(RuntimeLifecycleState::Loaded)
    }

    fn begin_start(&mut self) -> Result<(), RuntimeSessionError> {
        self.transition(RuntimeLifecycleState::Starting)
    }

    fn mark_running(&mut self) -> Result<(), RuntimeSessionError> {
        self.transition(RuntimeLifecycleState::Running)
    }

    fn begin_stop(&mut self) -> Result<(), RuntimeSessionError> {
        if self.state == RuntimeLifecycleState::Stopped {
            return Ok(());
        }
        self.transition(RuntimeLifecycleState::Stopping)
    }

    fn mark_stopped(&mut self) -> Result<(), RuntimeSessionError> {
        if self.state == RuntimeLifecycleState::Stopped {
            return Ok(());
        }
        self.transition(RuntimeLifecycleState::Stopped)
    }

    fn mark_failed(&mut self) -> Result<(), RuntimeSessionError> {
        if self.state == RuntimeLifecycleState::Stopped {
            return Ok(());
        }
        self.transition(RuntimeLifecycleState::Failed)
    }
}

/// Capability-checked host identity shared by adapter-specific host APIs.
#[derive(Debug, Clone)]
pub struct RuntimeHost {
    module_id: String,
    generation: u64,
    capabilities: CapabilitySet,
}

impl RuntimeHost {
    fn new(module_id: String, generation: u64, capabilities: CapabilitySet) -> Self {
        Self {
            module_id,
            generation,
            capabilities,
        }
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn capabilities(&self) -> CapabilitySet {
        self.capabilities.clone()
    }

    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities
            .is_granted(&Capability::new(capability.to_string()))
    }

    pub fn require_capability(&self, capability: &str) -> Result<(), RuntimeSessionError> {
        if self.has_capability(capability) {
            Ok(())
        } else {
            Err(RuntimeSessionError::CapabilityDenied {
                module_id: self.module_id.clone(),
                capability: capability.to_string(),
            })
        }
    }

    fn set_generation(&mut self, generation: u64) {
        self.generation = generation;
    }
}

/// A policy-bound Luau realm. The interpreter is initialized lazily so a
/// session can be staged and rejected before allocating an interpreter heap.
#[derive(Debug)]
pub struct RuntimeRealm {
    policy: RuntimePolicy,
    lua: Option<Lua>,
}

impl RuntimeRealm {
    fn new(policy: RuntimePolicy) -> Self {
        Self { policy, lua: None }
    }

    pub fn initialize(&mut self) -> Result<Lua, RuntimeSessionError> {
        if let Some(lua) = &self.lua {
            return Ok(lua.clone());
        }
        let lua = Lua::new();
        self.policy
            .install(&lua)
            .map_err(|error| RuntimeSessionError::RealmSetup(error.to_string()))?;
        self.lua = Some(lua.clone());
        Ok(lua)
    }

    pub fn is_initialized(&self) -> bool {
        self.lua.is_some()
    }

    pub fn lua(&self) -> Option<Lua> {
        self.lua.clone()
    }

    pub fn config(&self) -> SandboxConfig {
        self.policy.budget().config().clone()
    }

    pub fn resources(&self) -> ResourceBroker {
        ResourceBroker {
            inner: self.policy.budget(),
        }
    }
}

/// Public resource broker facade for one runtime session.
#[derive(Debug, Clone)]
pub struct ResourceBroker {
    inner: ResourceBudget,
}

impl ResourceBroker {
    pub fn config(&self) -> SandboxConfig {
        self.inner.config().clone()
    }

    pub fn validate_json(&self, value: &Value, label: &str) -> Result<usize, ResourceLimit> {
        self.inner.validate_json(value, label)
    }

    pub fn child_process_timeout(&self) -> Duration {
        self.inner.child_process_timeout()
    }

    pub fn begin_callback(&self) -> ResourceLease {
        ResourceLease {
            _guard: self.inner.begin_callback(),
        }
    }

    pub fn reserve_output(&self, bytes: usize) -> Result<(), ResourceLimit> {
        self.inner.reserve_output(bytes)
    }

    pub fn recharge_storage(&self, previous: u64, next: u64) -> Result<(), ResourceLimit> {
        self.inner.recharge_storage(previous, next)
    }

    pub fn reserve_queue(&self) -> Result<(), ResourceLimit> {
        self.inner.reserve_queue()
    }

    pub fn release_queue(&self, count: usize) {
        self.inner.release_queue(count);
    }

    pub fn reserve_event(&self) -> Result<(), ResourceLimit> {
        self.inner.reserve_event()
    }

    pub fn release_event(&self, count: usize) {
        self.inner.release_event(count);
    }

    pub fn reserve_queued_output(&self, bytes: usize) -> Result<(), ResourceLimit> {
        self.inner.reserve_queued_output(bytes)
    }

    pub fn release_queued_output(&self, bytes: usize) {
        self.inner.release_queued_output(bytes);
    }

    pub fn acquire_child(&self) -> Result<(), ResourceLimit> {
        self.inner.acquire_child()
    }

    pub fn release_child(&self) {
        self.inner.release_child();
    }

    /// Pause the frame-time budget for a blocking host call (for example
    /// waiting on a child process) that has its own timeout, so the wait does
    /// not also count against the script's per-callback frame budget.
    pub fn pause_frame_clock(&self) -> FramePauseLease {
        FramePauseLease {
            _guard: self.inner.pause_frame_clock(),
        }
    }
}

/// A callback-scoped pause of the frame-time clock. Dropping it resumes the
/// clock.
#[must_use]
#[derive(Debug)]
pub struct FramePauseLease {
    _guard: FramePause,
}

/// A callback-scoped resource lease. Dropping it releases per-callback output
/// accounting while all queue, event, storage, and child reservations remain
/// explicit until their owner releases them.
#[must_use]
#[derive(Debug)]
pub struct ResourceLease {
    _guard: ExecutionGuard,
}

/// Errors that cross the shared runtime-session boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSessionError {
    #[error("invalid runtime lifecycle transition: {from} -> {to}")]
    InvalidTransition {
        from: RuntimeLifecycleState,
        to: RuntimeLifecycleState,
    },
    #[error("capability '{capability}' denied for module '{module_id}'")]
    CapabilityDenied {
        module_id: String,
        capability: String,
    },
    #[error("runtime realm setup failed: {0}")]
    RealmSetup(String),
}

/// One authoritative execution session for a module runtime generation.
#[derive(Debug)]
pub struct RuntimeSession {
    module_id: String,
    generation: u64,
    realm: RuntimeRealm,
    host: RuntimeHost,
    resources: ResourceBroker,
    lifecycle: RuntimeLifecycle,
}

impl RuntimeSession {
    pub fn new(module_id: impl Into<String>, capabilities: CapabilitySet) -> Self {
        Self::with_config(module_id, capabilities, RuntimeSessionConfig::default())
    }

    pub fn with_config(
        module_id: impl Into<String>,
        capabilities: CapabilitySet,
        config: RuntimeSessionConfig,
    ) -> Self {
        let module_id = module_id.into();
        let policy = RuntimePolicy::new(config.sandbox.clone());
        Self::from_policy_with_config(module_id, capabilities, policy, config)
    }

    pub fn for_generation(
        module_id: impl Into<String>,
        capabilities: CapabilitySet,
        generation: u64,
    ) -> Self {
        let mut config = RuntimeSessionConfig::default();
        config.generation = generation;
        Self::with_config(module_id, capabilities, config)
    }

    pub(crate) fn from_policy(
        module_id: impl Into<String>,
        capabilities: CapabilitySet,
        policy: RuntimePolicy,
    ) -> Self {
        let config = RuntimeSessionConfig {
            sandbox: policy.budget().config().clone(),
            ..RuntimeSessionConfig::default()
        };
        Self::from_policy_with_config(module_id.into(), capabilities, policy, config)
    }

    fn from_policy_with_config(
        module_id: String,
        capabilities: CapabilitySet,
        policy: RuntimePolicy,
        config: RuntimeSessionConfig,
    ) -> Self {
        let resources = ResourceBroker {
            inner: policy.budget(),
        };
        Self {
            host: RuntimeHost::new(module_id.clone(), config.generation, capabilities),
            module_id,
            generation: config.generation,
            realm: RuntimeRealm::new(policy),
            resources,
            lifecycle: RuntimeLifecycle::default(),
        }
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn realm(&self) -> &RuntimeRealm {
        &self.realm
    }

    pub fn realm_mut(&mut self) -> &mut RuntimeRealm {
        &mut self.realm
    }

    pub fn initialize_realm(&mut self) -> Result<Lua, RuntimeSessionError> {
        self.realm.initialize()
    }

    pub fn host(&self) -> &RuntimeHost {
        &self.host
    }

    pub fn resources(&self) -> &ResourceBroker {
        &self.resources
    }

    /// Alias that makes the ownership boundary explicit at call sites.
    pub fn resource_broker(&self) -> &ResourceBroker {
        &self.resources
    }

    pub fn lifecycle(&self) -> &RuntimeLifecycle {
        &self.lifecycle
    }

    pub fn set_generation(&mut self, generation: u64) {
        self.generation = generation;
        self.host.set_generation(generation);
    }

    pub fn mark_loaded(&mut self) -> Result<(), RuntimeSessionError> {
        self.lifecycle.mark_loaded()
    }

    pub fn begin_start(&mut self) -> Result<(), RuntimeSessionError> {
        self.lifecycle.begin_start()
    }

    pub fn mark_running(&mut self) -> Result<(), RuntimeSessionError> {
        self.lifecycle.mark_running()
    }

    pub fn mark_failed(&mut self) -> Result<(), RuntimeSessionError> {
        self.lifecycle.mark_failed()
    }

    pub fn begin_stop(&mut self) -> Result<(), RuntimeSessionError> {
        self.lifecycle.begin_stop()
    }

    pub fn finish_stop(&mut self) -> Result<(), RuntimeSessionError> {
        self.lifecycle.mark_stopped()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capabilities(ids: &[&str]) -> CapabilitySet {
        CapabilitySet::from_ids(ids.iter().map(|id| (*id).to_string()))
    }

    #[test]
    fn session_owns_identity_realm_host_and_shared_resource_broker() {
        let session = RuntimeSession::for_generation(
            "@test/session",
            capabilities(&["service.audio.read"]),
            7,
        );
        assert_eq!(session.module_id(), "@test/session");
        assert_eq!(session.generation(), 7);
        assert_eq!(session.host().generation(), 7);
        assert!(session.host().has_capability("service.audio.read"));
        assert!(!session.host().has_capability("service.audio.control"));
        assert!(!session.realm().is_initialized());
        assert_eq!(
            session.resources().config().queue_budget,
            SandboxConfig::default().queue_budget
        );
    }

    #[test]
    fn realm_uses_the_session_policy_and_stays_sandboxed() {
        let mut config = RuntimeSessionConfig::default();
        config.sandbox.instruction_budget = 1_000;
        let mut session =
            RuntimeSession::with_config("@test/realm", CapabilitySet::default(), config);
        let lua = session.initialize_realm().unwrap();
        let _lease = session.resources().begin_callback();
        assert!(lua.load("while true do end").exec().is_err());
        assert!(session.realm().is_initialized());
    }

    #[test]
    fn host_capability_errors_name_the_module() {
        let session = RuntimeSession::new("@test/host", capabilities(&["theme.read"]));
        let error = session.host().require_capability("fs.write").unwrap_err();
        assert_eq!(
            error,
            RuntimeSessionError::CapabilityDenied {
                module_id: "@test/host".into(),
                capability: "fs.write".into(),
            }
        );
    }

    #[test]
    fn resource_broker_reservations_are_shared_with_the_realm_policy() {
        let session = RuntimeSession::with_config(
            "@test/resources",
            CapabilitySet::default(),
            RuntimeSessionConfig {
                sandbox: SandboxConfig {
                    queue_budget: 1,
                    ..SandboxConfig::default()
                },
                ..RuntimeSessionConfig::default()
            },
        );
        session.resources().reserve_queue().unwrap();
        assert!(session.realm().resources().reserve_queue().is_err());
        session.resources().release_queue(1);
    }
}
