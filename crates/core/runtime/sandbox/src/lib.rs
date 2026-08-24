/// Shared sandbox policy types for MESH modules.
///
/// `mesh-core-scripting` owns the actual Luau interpreter bridge used by both
/// frontend components and backend services. This crate carries runtime policy
/// metadata that can be shared by those hosts without tying it to either side.
use mesh_core_capability::CapabilitySet;

/// Configuration for the module sandbox.
///
/// The policy is deliberately shared by every Luau host.  Keeping the limits
/// together prevents frontend and backend realms from silently acquiring
/// different resource ceilings as host APIs evolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxConfig {
    /// Maximum memory the module can allocate (bytes).
    pub memory_limit: u64,
    /// Maximum Luau instructions allowed during one callback execution.
    pub instruction_budget: u64,
    /// Maximum CPU time per frame (microseconds).
    pub frame_budget_us: u64,
    /// Maximum bytes returned or logged by one callback execution.
    pub output_budget: u64,
    /// Maximum number of queued host side effects and stream lines.
    pub queue_budget: u64,
    /// Maximum serialized bytes in one durable storage document.
    pub storage_budget: u64,
    /// Maximum simultaneously active child processes for one realm.
    pub child_process_budget: u64,
    /// Maximum runtime for one synchronous child-process request.
    pub child_process_timeout_ms: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            memory_limit: 64 * 1024 * 1024, // 64 MB
            instruction_budget: 1_000_000,
            frame_budget_us: 4_000,     // 4ms
            output_budget: 1024 * 1024, // 1 MiB per callback
            queue_budget: 1024,
            storage_budget: 1024 * 1024, // 1 MiB per scoped document
            child_process_budget: 8,
            child_process_timeout_ms: 5_000,
        }
    }
}

/// The execution tier determines isolation level and trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTier {
    /// In-process Rust. Core modules only.
    InProcess,
    /// Sandboxed Luau interpreter. Default for community modules.
    Luau,
    /// Sandboxed WebAssembly. For performance-sensitive community modules.
    Wasm,
}

/// A sandboxed runtime instance for a single module.
#[derive(Debug)]
pub struct ModuleRuntime {
    pub module_id: String,
    pub tier: ExecutionTier,
    pub config: SandboxConfig,
    pub capabilities: CapabilitySet,
}

impl ModuleRuntime {
    pub fn new(
        module_id: impl Into<String>,
        tier: ExecutionTier,
        capabilities: CapabilitySet,
    ) -> Self {
        Self {
            module_id: module_id.into(),
            tier,
            config: SandboxConfig::default(),
            capabilities,
        }
    }

    pub fn with_config(mut self, config: SandboxConfig) -> Self {
        self.config = config;
        self
    }
}
