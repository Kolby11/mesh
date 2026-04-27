/// Extension runtime for MESH plugins.
///
/// The long-term runtime direction is external TypeScript plugin processes for
/// backends and Tauri-hosted frontend plugins. The Rust core remains the source
/// of truth for capabilities, plugin lifecycle, bindable values, and shell
/// wiring, while this crate defines the host/runtime contract.
use mesh_capability::CapabilitySet;
use serde::{Deserialize, Serialize};

pub mod protocol;

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
    /// External TypeScript process connected to the core host protocol.
    TypeScript,
    /// Tauri-hosted frontend runtime using a webview UI.
    Tauri,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginRuntimeRole {
    Backend,
    Frontend,
}
