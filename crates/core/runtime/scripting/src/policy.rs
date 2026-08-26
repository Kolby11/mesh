//! The single resource policy used by every Luau realm.
//!
//! `mlua` owns the interpreter, but it does not know about MESH host queues,
//! storage, or subprocesses.  This module keeps those limits beside the Luau
//! sandbox and exposes one small broker to the frontend and backend adapters.

use mesh_core_runtime::SandboxConfig;
use mlua::{Compiler, Lua, VmState};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Instruction,
    Output,
    Queue,
    Event,
    ChildProcess,
    Aggregate,
}

impl ResourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Instruction => "instruction",
            Self::Output => "output",
            Self::Queue => "queue",
            Self::Event => "event",
            Self::ChildProcess => "child-process",
            Self::Aggregate => "aggregate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLimit {
    pub kind: ResourceKind,
    pub limit: u64,
}

impl std::fmt::Display for ResourceLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "sandbox {} budget exceeded (limit {})",
            self.kind.as_str(),
            self.limit
        )
    }
}

#[derive(Debug)]
struct ResourceCounters {
    callback_output: AtomicU64,
    queued_items: AtomicU64,
    queued_output: AtomicU64,
    events: AtomicU64,
    aggregate: AtomicU64,
    active_children: AtomicU64,
    callback_depth: AtomicU32,
    remaining_instructions: AtomicU64,
}

impl Default for ResourceCounters {
    fn default() -> Self {
        Self {
            callback_output: AtomicU64::new(0),
            queued_items: AtomicU64::new(0),
            queued_output: AtomicU64::new(0),
            events: AtomicU64::new(0),
            aggregate: AtomicU64::new(0),
            active_children: AtomicU64::new(0),
            callback_depth: AtomicU32::new(0),
            remaining_instructions: AtomicU64::new(0),
        }
    }
}

/// A cloneable broker for resources owned by one Luau realm.
#[derive(Debug, Clone)]
pub(crate) struct ResourceBudget {
    config: SandboxConfig,
    counters: Arc<ResourceCounters>,
}

impl ResourceBudget {
    pub(crate) fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            counters: Arc::new(ResourceCounters::default()),
        }
    }

    pub(crate) fn config(&self) -> &SandboxConfig {
        &self.config
    }

    pub(crate) fn output_limit(&self) -> u64 {
        self.config.output_budget
    }

    pub(crate) fn validate_json(
        &self,
        value: &serde_json::Value,
        label: &str,
    ) -> Result<usize, ResourceLimit> {
        mesh_core_runtime::validate_json(
            value,
            self.config.json_max_bytes as usize,
            self.config.json_max_depth as usize,
            label,
        )
        .map_err(|_| ResourceLimit {
            kind: ResourceKind::Output,
            limit: self.config.json_max_bytes,
        })
    }

    pub(crate) fn child_process_timeout(&self) -> Duration {
        Duration::from_millis(self.config.child_process_timeout_ms)
    }

    pub(crate) fn begin_callback(&self) -> ExecutionGuard {
        if self.counters.callback_depth.fetch_add(1, Ordering::AcqRel) == 0 {
            self.counters.callback_output.store(0, Ordering::Release);
            self.counters
                .remaining_instructions
                .store(self.config.instruction_budget, Ordering::Release);
        }
        ExecutionGuard {
            budget: self.clone(),
        }
    }

    /// Charge one unit for one interpreter checkpoint.
    ///
    /// Luau fires the interrupt at loop back-edges and call/return boundaries,
    /// not once per fixed count of executed instructions, so a firing is worth
    /// exactly one unit. Charging a firing as if it stood for a thousand
    /// instructions cut the real ceiling to a thousand loop iterations per
    /// callback and killed ordinary backend startup work.
    fn checkpoint(&self) -> Result<(), ResourceLimit> {
        if self.counters.callback_depth.load(Ordering::Acquire) == 0 {
            return Ok(());
        }
        let previous = self
            .counters
            .remaining_instructions
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                Some(remaining.saturating_sub(1))
            })
            .unwrap_or(0);
        if previous == 0 {
            return Err(ResourceLimit {
                kind: ResourceKind::Instruction,
                limit: self.config.instruction_budget,
            });
        }
        Ok(())
    }

    pub(crate) fn reserve_output(&self, bytes: usize) -> Result<(), ResourceLimit> {
        self.reserve_aggregate(bytes as u64)?;
        if let Err(error) = reserve_counter(
            &self.counters.callback_output,
            bytes as u64,
            self.config.output_budget,
            ResourceKind::Output,
        ) {
            self.release_aggregate(bytes as u64);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn reserve_storage(&self, bytes: usize) -> Result<(), ResourceLimit> {
        self.reserve_aggregate(bytes as u64)
    }

    fn reserve_aggregate(&self, amount: u64) -> Result<(), ResourceLimit> {
        reserve_counter(
            &self.counters.aggregate,
            amount,
            self.config.aggregate_resource_budget,
            ResourceKind::Aggregate,
        )
    }

    fn release_aggregate(&self, amount: u64) {
        self.counters.aggregate.fetch_sub(amount, Ordering::AcqRel);
    }

    pub(crate) fn reserve_queue(&self) -> Result<(), ResourceLimit> {
        self.reserve_aggregate(1)?;
        if let Err(error) = reserve_counter(
            &self.counters.queued_items,
            1,
            self.config.queue_budget,
            ResourceKind::Queue,
        ) {
            self.release_aggregate(1);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn reserve_event(&self) -> Result<(), ResourceLimit> {
        self.reserve_aggregate(1)?;
        if let Err(error) = reserve_counter(
            &self.counters.events,
            1,
            self.config.event_budget,
            ResourceKind::Event,
        ) {
            self.release_aggregate(1);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn release_event(&self, count: usize) {
        self.counters
            .events
            .fetch_sub(count as u64, Ordering::AcqRel);
        self.release_aggregate(count as u64);
    }

    pub(crate) fn reserve_queued_output(&self, bytes: usize) -> Result<(), ResourceLimit> {
        self.reserve_aggregate(bytes as u64)?;
        if let Err(error) = reserve_counter(
            &self.counters.queued_output,
            bytes as u64,
            self.config.output_budget,
            ResourceKind::Output,
        ) {
            self.release_aggregate(bytes as u64);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn release_queued_output(&self, bytes: usize) {
        self.counters
            .queued_output
            .fetch_sub(bytes as u64, Ordering::AcqRel);
        self.release_aggregate(bytes as u64);
    }

    pub(crate) fn acquire_child(&self) -> Result<(), ResourceLimit> {
        self.reserve_aggregate(1)?;
        if let Err(error) = reserve_counter(
            &self.counters.active_children,
            1,
            self.config.child_process_budget,
            ResourceKind::ChildProcess,
        ) {
            self.release_aggregate(1);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn release_child(&self) {
        self.counters.active_children.fetch_sub(1, Ordering::AcqRel);
        self.release_aggregate(1);
    }

    pub(crate) fn release_queue(&self, count: usize) {
        self.counters
            .queued_items
            .fetch_sub(count as u64, Ordering::AcqRel);
        self.release_aggregate(count as u64);
    }
}

fn reserve_counter(
    counter: &AtomicU64,
    amount: u64,
    limit: u64,
    kind: ResourceKind,
) -> Result<(), ResourceLimit> {
    let result = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        current.checked_add(amount).filter(|next| *next <= limit)
    });
    if result.is_err() {
        Err(ResourceLimit { kind, limit })
    } else {
        Ok(())
    }
}

/// An execution window. Nested host calls share the outer window, so one
/// callback cannot reset its own instruction or output budget by re-entering
/// the host.
#[derive(Debug)]
pub(crate) struct ExecutionGuard {
    budget: ResourceBudget,
}

impl Drop for ExecutionGuard {
    fn drop(&mut self) {
        if self
            .budget
            .counters
            .callback_depth
            .fetch_sub(1, Ordering::AcqRel)
            == 1
        {
            let callback_output = self
                .budget
                .counters
                .callback_output
                .swap(0, Ordering::AcqRel);
            self.budget.release_aggregate(callback_output);
        }
    }
}

/// Interpreter policy plus the host-resource broker for one realm.
#[derive(Debug, Clone)]
pub(crate) struct RuntimePolicy {
    budget: ResourceBudget,
}

impl RuntimePolicy {
    pub(crate) fn new(config: SandboxConfig) -> Self {
        Self {
            budget: ResourceBudget::new(config),
        }
    }

    pub(crate) fn default() -> Self {
        Self::new(SandboxConfig::default())
    }

    /// Apply every interpreter-side part of the policy exactly once when a
    /// realm is created. Host-side budgets continue to use the same broker.
    pub(crate) fn install(&self, lua: &Lua) -> mlua::Result<()> {
        // Host tables are intentionally replaceable by the runtime while a
        // module is alive (for example, when a capability is revoked). Keep
        // Luau's import optimization from caching a stale `mesh` member.
        lua.set_compiler(Compiler::new().add_mutable_global("mesh"));
        lua.sandbox(true)?;
        lua.set_memory_limit(self.budget.config().memory_limit as usize)?;
        let budget = self.budget.clone();
        lua.set_interrupt(move |_| {
            budget
                .checkpoint()
                .map(|_| VmState::Continue)
                .map_err(|limit| mlua::Error::RuntimeError(limit.to_string()))
        });
        Ok(())
    }

    pub(crate) fn budget(&self) -> ResourceBudget {
        self.budget.clone()
    }

    pub(crate) fn begin_callback(&self) -> ExecutionGuard {
        self.budget.begin_callback()
    }

    pub(crate) fn storage_budget(&self) -> u64 {
        self.budget.config().storage_budget
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    #[test]
    fn policy_installs_sandbox_memory_and_instruction_limits() {
        let mut config = SandboxConfig::default();
        config.instruction_budget = 1_000;
        config.frame_budget_us = 1_000_000;
        let policy = RuntimePolicy::new(config);
        let lua = Lua::new();
        policy.install(&lua).unwrap();
        let _guard = policy.begin_callback();
        let result = lua.load("while true do end").exec();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("instruction"));
    }

    /// Luau checkpoints per loop back-edge and call, so charging a firing as a
    /// thousand instructions left roughly a thousand iterations per callback
    /// and aborted ordinary backend startup work.
    #[test]
    fn ordinary_loops_and_calls_fit_inside_the_default_budget() {
        let policy = RuntimePolicy::new(SandboxConfig::default());
        let lua = Lua::new();
        policy.install(&lua).unwrap();
        let _guard = policy.begin_callback();

        lua.load(
            "local function step(acc) return acc + 1 end
             local total = 0
             for _ = 1, 10000 do total = step(total) end
             assert(total == 10000)",
        )
        .exec()
        .expect("ten thousand iterations and calls stay inside the default budget");
    }

    #[test]
    fn the_budget_counts_one_unit_for_each_interpreter_checkpoint() {
        let mut config = SandboxConfig::default();
        config.instruction_budget = 5_000;
        let policy = RuntimePolicy::new(config);
        let lua = Lua::new();
        policy.install(&lua).unwrap();
        let _guard = policy.begin_callback();

        let error = lua
            .load("local total = 0 for _ = 1, 100000 do total = total + 1 end")
            .exec()
            .expect_err("a loop past the budget still aborts");
        assert!(error.to_string().contains("instruction"));
    }

    #[test]
    fn resource_budget_rejects_output_queue_and_children() {
        let mut config = SandboxConfig::default();
        config.output_budget = 3;
        config.queue_budget = 1;
        config.child_process_budget = 1;
        let budget = ResourceBudget::new(config);
        let _guard = budget.begin_callback();
        assert!(budget.reserve_output(3).is_ok());
        assert!(budget.reserve_output(1).is_err());
        assert!(budget.reserve_queue().is_ok());
        assert!(budget.reserve_queue().is_err());
        budget.release_queue(1);
        assert!(budget.acquire_child().is_ok());
        assert!(budget.acquire_child().is_err());
        budget.release_child();
    }

    #[test]
    fn resource_budget_enforces_event_and_aggregate_limits() {
        let mut config = SandboxConfig::default();
        config.event_budget = 1;
        config.aggregate_resource_budget = 4;
        let budget = ResourceBudget::new(config);
        let _guard = budget.begin_callback();

        assert!(budget.reserve_event().is_ok());
        assert!(budget.reserve_event().is_err());
        budget.release_event(1);

        assert!(budget.reserve_output(4).is_ok());
        assert!(budget.reserve_storage(1).is_err());
    }
}
