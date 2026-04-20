/// Extension runtime for MESH plugins.
///
/// This crate will host the Luau sandbox and provide the bridge between
/// plugin scripts and the core's capability-gated host APIs.
use mesh_capability::CapabilitySet;

/// Configuration for the plugin sandbox.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Maximum memory the plugin can allocate (bytes).
    pub memory_limit: u64,
    /// Maximum CPU time per frame (microseconds).
    pub frame_budget_us: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            memory_limit: 64 * 1024 * 1024, // 64 MB
            frame_budget_us: 4_000,         // 4ms
        }
    }
}

/// The execution tier determines isolation level and trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTier {
    /// In-process Rust. Core plugins only.
    InProcess,
    /// Sandboxed Luau interpreter. Default for community plugins.
    Luau,
    /// Sandboxed WebAssembly. For performance-sensitive community plugins.
    Wasm,
}

/// A sandboxed runtime instance for a single plugin.
#[derive(Debug)]
pub struct PluginRuntime {
    pub plugin_id: String,
    pub tier: ExecutionTier,
    pub config: SandboxConfig,
    pub capabilities: CapabilitySet,
}

impl PluginRuntime {
    pub fn new(
        plugin_id: impl Into<String>,
        tier: ExecutionTier,
        capabilities: CapabilitySet,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
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
