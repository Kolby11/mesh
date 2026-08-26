//! The per-module runtime boundary shared by frontend and backend adapters.
//!
//! A [`RuntimeSession`] is deliberately narrower than either adapter.  It owns
//! the identity and policy that must remain stable across callbacks, while the
//! adapters retain their frontend/backend-specific host operations.  Keeping
//! these pieces together prevents lifecycle, resource, and failure decisions
//! from becoming ambient state in individual host functions.

use crate::policy::{ExecutionGuard, FramePause, ResourceBudget, RuntimePolicy};
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_runtime::SandboxConfig;
use mlua::Lua;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

pub use crate::policy::{ResourceKind, ResourceLimit};

const DEFAULT_FAILURE_LIMIT: u32 = 3;
const DEFAULT_BACKOFF_INITIAL_MS: u64 = 100;
const DEFAULT_BACKOFF_MAX_MS: u64 = 30_000;

/// Configuration shared by every runtime session owned by a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionConfig {
    pub sandbox: SandboxConfig,
    pub generation: u64,
    pub failure_limit: u32,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
}

impl Default for RuntimeSessionConfig {
    fn default() -> Self {
        Self {
            sandbox: SandboxConfig::default(),
            generation: 0,
            failure_limit: DEFAULT_FAILURE_LIMIT,
            backoff_initial: Duration::from_millis(DEFAULT_BACKOFF_INITIAL_MS),
            backoff_max: Duration::from_millis(DEFAULT_BACKOFF_MAX_MS),
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

    fn prepare_restart(&mut self) -> Result<(), RuntimeSessionError> {
        if self.state == RuntimeLifecycleState::Stopped {
            return Err(RuntimeSessionError::InvalidTransition {
                from: self.state,
                to: RuntimeLifecycleState::Loaded,
            });
        }
        if self.state != RuntimeLifecycleState::Loaded {
            self.transition(RuntimeLifecycleState::Loaded)?;
        }
        Ok(())
    }
}

/// Coarse runtime health, distinct from static installed-graph health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeHealthState {
    Healthy,
    Degraded,
    Unavailable,
}

/// The health record published by a runtime session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHealth {
    state: RuntimeHealthState,
    reason: Option<String>,
    recoverable: bool,
    since: SystemTime,
}

impl Default for RuntimeHealth {
    fn default() -> Self {
        Self::healthy()
    }
}

impl RuntimeHealth {
    pub fn healthy() -> Self {
        Self::new(RuntimeHealthState::Healthy, None, false)
    }

    pub fn degraded(reason: impl Into<String>) -> Self {
        Self::new(RuntimeHealthState::Degraded, Some(reason.into()), true)
    }

    pub fn unavailable(reason: impl Into<String>, recoverable: bool) -> Self {
        Self::new(
            RuntimeHealthState::Unavailable,
            Some(reason.into()),
            recoverable,
        )
    }

    fn new(state: RuntimeHealthState, reason: Option<String>, recoverable: bool) -> Self {
        Self {
            state,
            reason,
            recoverable,
            since: SystemTime::now(),
        }
    }

    pub fn state(&self) -> RuntimeHealthState {
        self.state
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn recoverable(&self) -> bool {
        self.recoverable
    }

    pub fn since(&self) -> SystemTime {
        self.since
    }
}

/// Exponential retry state for one runtime generation.
#[derive(Debug, Clone)]
pub struct RuntimeBackoff {
    failures: u32,
    initial: Duration,
    maximum: Duration,
}

impl RuntimeBackoff {
    pub fn new(initial: Duration, maximum: Duration) -> Self {
        Self {
            failures: 0,
            initial,
            maximum: maximum.max(initial),
        }
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    pub fn next_delay(&mut self) -> Duration {
        self.failures = self.failures.saturating_add(1);
        let exponent = self.failures.saturating_sub(1).min(31);
        let multiplier = 1u32 << exponent;
        self.initial
            .checked_mul(multiplier)
            .unwrap_or(self.maximum)
            .min(self.maximum)
    }

    pub fn reset(&mut self) {
        self.failures = 0;
    }
}

/// Cancellation and failure counters for a runtime session.
#[derive(Debug, Clone)]
pub struct RuntimeSupervisor {
    cancelled: Arc<AtomicBool>,
    failures: u32,
    failure_limit: u32,
}

impl RuntimeSupervisor {
    fn new(failure_limit: u32) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            failures: 0,
            failure_limit: failure_limit.max(1),
        }
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    pub fn failure_limit(&self) -> u32 {
        self.failure_limit
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn record_failure(&mut self) -> bool {
        self.failures = self.failures.saturating_add(1);
        self.failures >= self.failure_limit
    }

    fn reset(&mut self) {
        self.failures = 0;
        self.cancelled.store(false, Ordering::Release);
    }
}

/// Quarantine state is explicit so a failed module cannot be silently
/// restarted by a generic retry loop.
#[derive(Debug, Clone, Default)]
pub struct RuntimeQuarantine {
    active: bool,
    reason: Option<String>,
    since: Option<SystemTime>,
}

impl RuntimeQuarantine {
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn since(&self) -> Option<SystemTime> {
        self.since
    }

    fn enter(&mut self, reason: impl Into<String>) {
        self.active = true;
        self.reason = Some(reason.into());
        self.since = Some(SystemTime::now());
    }

    fn clear(&mut self) {
        self.active = false;
        self.reason = None;
        self.since = None;
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

    pub fn reserve_storage(&self, bytes: usize) -> Result<(), ResourceLimit> {
        self.inner.reserve_storage(bytes)
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

/// Rust-owned service state for a module runtime.
#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    revision: u64,
    snapshot: Option<Value>,
}

impl RuntimeState {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn snapshot(&self) -> Option<&Value> {
        self.snapshot.as_ref()
    }

    pub fn snapshot_cloned(&self) -> Option<Value> {
        self.snapshot.clone()
    }

    pub fn begin_transaction(&self) -> RuntimeStateTransaction {
        RuntimeStateTransaction {
            base_revision: self.revision,
            snapshot: self.snapshot.clone(),
            events: Vec::new(),
        }
    }

    fn commit(
        &mut self,
        transaction: RuntimeStateTransaction,
    ) -> Result<RuntimeStateCommit, RuntimeSessionError> {
        if transaction.base_revision != self.revision {
            return Err(RuntimeSessionError::StateConflict {
                expected: transaction.base_revision,
                actual: self.revision,
            });
        }
        self.snapshot = transaction.snapshot.clone();
        self.revision = self.revision.saturating_add(1);
        Ok(RuntimeStateCommit {
            revision: self.revision,
            snapshot: transaction.snapshot,
            events: transaction.events,
        })
    }
}

/// A staged state/event update. It becomes visible only when the owning
/// session commits it, so a failed callback cannot publish a partial result.
#[derive(Debug, Clone)]
pub struct RuntimeStateTransaction {
    base_revision: u64,
    snapshot: Option<Value>,
    events: Vec<RuntimeEvent>,
}

impl RuntimeStateTransaction {
    pub fn base_revision(&self) -> u64 {
        self.base_revision
    }

    pub fn set_snapshot(&mut self, snapshot: Value) {
        self.snapshot = Some(snapshot);
    }

    pub fn clear_snapshot(&mut self) {
        self.snapshot = None;
    }

    pub fn publish_event(&mut self, name: impl Into<String>, payload: Value) {
        self.events.push(RuntimeEvent {
            name: name.into(),
            payload,
        });
    }

    pub fn events(&self) -> &[RuntimeEvent] {
        &self.events
    }
}

/// A typed event staged with a runtime state transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEvent {
    pub name: String,
    pub payload: Value,
}

/// The committed result of one callback transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeStateCommit {
    pub revision: u64,
    pub snapshot: Option<Value>,
    pub events: Vec<RuntimeEvent>,
}

/// The failure result used by the session supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFailureDecision {
    pub failures: u32,
    pub retry_after: Duration,
    pub quarantined: bool,
}

/// Errors that cross the shared runtime-session boundary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuntimeSessionError {
    #[error("invalid runtime lifecycle transition: {from} -> {to}")]
    InvalidTransition {
        from: RuntimeLifecycleState,
        to: RuntimeLifecycleState,
    },
    #[error("runtime session for '{module_id}' is quarantined")]
    Quarantined { module_id: String },
    #[error("runtime session for '{module_id}' is not active")]
    Inactive { module_id: String },
    #[error("capability '{capability}' denied for module '{module_id}'")]
    CapabilityDenied {
        module_id: String,
        capability: String,
    },
    #[error("runtime realm setup failed: {0}")]
    RealmSetup(String),
    #[error("runtime state revision conflict: expected {expected}, found {actual}")]
    StateConflict { expected: u64, actual: u64 },
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
    state: RuntimeState,
    supervisor: RuntimeSupervisor,
    health: RuntimeHealth,
    backoff: RuntimeBackoff,
    quarantine: RuntimeQuarantine,
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
            state: RuntimeState::default(),
            supervisor: RuntimeSupervisor::new(config.failure_limit),
            health: RuntimeHealth::healthy(),
            backoff: RuntimeBackoff::new(config.backoff_initial, config.backoff_max),
            quarantine: RuntimeQuarantine::default(),
        }
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
            && !self.supervisor.is_cancelled()
            && !self.quarantine.is_active()
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

    pub fn state(&self) -> &RuntimeState {
        &self.state
    }

    pub fn supervisor(&self) -> &RuntimeSupervisor {
        &self.supervisor
    }

    pub fn health(&self) -> &RuntimeHealth {
        &self.health
    }

    pub fn backoff(&self) -> &RuntimeBackoff {
        &self.backoff
    }

    pub fn quarantine(&self) -> &RuntimeQuarantine {
        &self.quarantine
    }

    pub fn set_generation(&mut self, generation: u64) {
        self.generation = generation;
        self.host.set_generation(generation);
    }

    pub fn begin_callback(&self) -> Result<ResourceLease, RuntimeSessionError> {
        if self.quarantine.is_active() {
            return Err(RuntimeSessionError::Quarantined {
                module_id: self.module_id.clone(),
            });
        }
        if matches!(
            self.lifecycle.state(),
            RuntimeLifecycleState::Stopping
                | RuntimeLifecycleState::Stopped
                | RuntimeLifecycleState::Failed
        ) || self.supervisor.is_cancelled()
        {
            return Err(RuntimeSessionError::Inactive {
                module_id: self.module_id.clone(),
            });
        }
        Ok(self.resources.begin_callback())
    }

    pub fn mark_loaded(&mut self) -> Result<(), RuntimeSessionError> {
        if self.quarantine.is_active() {
            return Err(RuntimeSessionError::Quarantined {
                module_id: self.module_id.clone(),
            });
        }
        self.lifecycle.mark_loaded()
    }

    pub fn begin_start(&mut self) -> Result<(), RuntimeSessionError> {
        if self.quarantine.is_active() {
            return Err(RuntimeSessionError::Quarantined {
                module_id: self.module_id.clone(),
            });
        }
        self.lifecycle.begin_start()
    }

    pub fn mark_running(&mut self) -> Result<(), RuntimeSessionError> {
        self.lifecycle.mark_running()?;
        self.supervisor.cancelled.store(false, Ordering::Release);
        self.health = RuntimeHealth::healthy();
        Ok(())
    }

    pub fn begin_stop(&mut self) -> Result<(), RuntimeSessionError> {
        self.supervisor.cancel();
        self.lifecycle.begin_stop()
    }

    pub fn finish_stop(&mut self) -> Result<(), RuntimeSessionError> {
        self.lifecycle.mark_stopped()
    }

    pub fn record_success(&mut self) {
        self.supervisor.reset();
        self.backoff.reset();
        self.health = RuntimeHealth::healthy();
    }

    pub fn record_failure(&mut self, reason: impl Into<String>) -> RuntimeFailureDecision {
        let reason = reason.into();
        let retry_after = self.backoff.next_delay();
        let quarantined = self.supervisor.record_failure();
        let _ = self.lifecycle.mark_failed();
        self.health = RuntimeHealth::unavailable(reason.clone(), !quarantined);
        if quarantined {
            self.quarantine.enter(reason);
            self.supervisor.cancel();
        }
        RuntimeFailureDecision {
            failures: self.supervisor.failures(),
            retry_after,
            quarantined,
        }
    }

    pub fn prepare_restart(&mut self) -> Result<(), RuntimeSessionError> {
        if self.quarantine.is_active() {
            return Err(RuntimeSessionError::Quarantined {
                module_id: self.module_id.clone(),
            });
        }
        self.supervisor.cancelled.store(false, Ordering::Release);
        self.lifecycle.prepare_restart()?;
        self.health = RuntimeHealth::degraded("runtime restart pending");
        Ok(())
    }

    pub fn clear_quarantine(&mut self) {
        self.quarantine.clear();
        self.supervisor.reset();
        self.backoff.reset();
        self.health = RuntimeHealth::degraded("runtime quarantine cleared");
    }

    pub fn begin_state_transaction(&self) -> RuntimeStateTransaction {
        self.state.begin_transaction()
    }

    pub fn commit_state(
        &mut self,
        transaction: RuntimeStateTransaction,
    ) -> Result<RuntimeStateCommit, RuntimeSessionError> {
        self.state.commit(transaction)
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
    fn lifecycle_failure_uses_backoff_then_quarantines() {
        let mut session = RuntimeSession::with_config(
            "@test/failing",
            CapabilitySet::default(),
            RuntimeSessionConfig {
                failure_limit: 2,
                backoff_initial: Duration::from_millis(10),
                backoff_max: Duration::from_millis(15),
                ..RuntimeSessionConfig::default()
            },
        );
        session.mark_loaded().unwrap();
        session.begin_start().unwrap();
        session.mark_running().unwrap();

        let first = session.record_failure("first failure");
        assert_eq!(first.retry_after, Duration::from_millis(10));
        assert!(!first.quarantined);
        assert_eq!(session.health().state(), RuntimeHealthState::Unavailable);

        session.prepare_restart().unwrap();
        session.begin_start().unwrap();
        session.mark_running().unwrap();
        let second = session.record_failure("second failure");
        assert_eq!(second.retry_after, Duration::from_millis(15));
        assert!(second.quarantined);
        assert!(session.quarantine().is_active());
        assert!(!session.is_current(session.generation()));
    }

    #[test]
    fn state_transactions_commit_atomically_and_reject_stale_writers() {
        let mut session = RuntimeSession::new("@test/state", CapabilitySet::default());
        let mut first = session.begin_state_transaction();
        first.set_snapshot(serde_json::json!({"ready": true}));
        first.publish_event("ready", serde_json::json!({}));
        let committed = session.commit_state(first).unwrap();
        assert_eq!(committed.revision, 1);
        assert_eq!(
            session.state().snapshot(),
            Some(&serde_json::json!({"ready": true}))
        );
        assert_eq!(committed.events.len(), 1);

        let mut stale = session.begin_state_transaction();
        let mut current = session.begin_state_transaction();
        current.set_snapshot(serde_json::json!({"value": 2}));
        session.commit_state(current).unwrap();
        stale.set_snapshot(serde_json::json!({"value": 1}));
        assert!(matches!(
            session.commit_state(stale),
            Err(RuntimeSessionError::StateConflict { .. })
        ));
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
