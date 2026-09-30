/// Shared sandbox policy types for MESH modules.
///
/// `mesh-core-scripting` owns the actual Luau interpreter bridge used by both
/// frontend components and backend services. This crate carries runtime policy
/// metadata that can be shared by those hosts without tying it to either side.
use mesh_core_capability::CapabilitySet;

pub const DEFAULT_JSON_MAX_BYTES: usize = 64 * 1024;
pub const DEFAULT_JSON_MAX_DEPTH: usize = 32;

/// Identity shared by every backend transport hop belonging to one provider
/// incarnation. Activation generations identify the committed shell graph;
/// provider epochs distinguish successive runtime incarnations of one
/// interface, including two starts of the same provider module.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct BackendIdentity {
    pub activation_generation: u64,
    pub provider_epoch: u64,
}

impl BackendIdentity {
    pub const fn new(activation_generation: u64, provider_epoch: u64) -> Self {
        Self {
            activation_generation,
            provider_epoch,
        }
    }
}

/// Return the serialized size of a JSON value after enforcing the shared
/// ingress/egress depth and byte policy.
pub fn validate_json(
    value: &serde_json::Value,
    max_bytes: usize,
    max_depth: usize,
    label: &str,
) -> Result<usize, String> {
    fn within_depth(value: &serde_json::Value, current: usize, max_depth: usize) -> bool {
        if current > max_depth {
            return false;
        }
        match value {
            serde_json::Value::Array(values) => values
                .iter()
                .all(|value| within_depth(value, current + 1, max_depth)),
            serde_json::Value::Object(values) => values
                .values()
                .all(|value| within_depth(value, current + 1, max_depth)),
            _ => true,
        }
    }

    // Depth first: it is a cheap walk and bounds the serializer's recursion.
    if !within_depth(value, 0, max_depth) {
        return Err(format!("{label} exceeds JSON depth {max_depth}"));
    }
    // Count the encoding without materializing it, stopping as soon as it
    // passes the byte ceiling.
    let mut counter = ByteCounter {
        count: 0,
        limit: max_bytes,
    };
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => Ok(counter.count),
        Err(_) if counter.count > max_bytes => Err(format!("{label} exceeds {max_bytes} bytes")),
        Err(error) => Err(format!("failed to encode {label}: {error}")),
    }
}

/// Serialized size of a JSON value, counted without allocating its encoding.
pub fn json_encoded_len(value: &serde_json::Value) -> usize {
    let mut counter = ByteCounter {
        count: 0,
        limit: usize::MAX,
    };
    // Encoding a `Value` into a counting writer cannot fail.
    let _ = serde_json::to_writer(&mut counter, value);
    counter.count
}

struct ByteCounter {
    count: usize,
    limit: usize,
}

impl std::io::Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.count = self.count.saturating_add(bytes.len());
        if self.count > self.limit {
            return Err(std::io::Error::other("byte limit exceeded"));
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod validate_json_tests {
    use super::*;

    #[test]
    fn counted_size_matches_the_encoding_and_limits_hold() {
        let value = serde_json::json!({ "a": [1, 2.5, "three", null], "b": { "c": "é\n\"" } });
        let encoded = serde_json::to_vec(&value).unwrap().len();
        assert_eq!(validate_json(&value, 1024, 8, "payload"), Ok(encoded));
        assert_eq!(json_encoded_len(&value), encoded);
        assert_eq!(validate_json(&value, encoded, 8, "payload"), Ok(encoded));
        assert!(
            validate_json(&value, encoded - 1, 8, "payload")
                .unwrap_err()
                .contains("exceeds")
        );
        assert!(
            validate_json(&value, 1024, 1, "payload")
                .unwrap_err()
                .contains("depth")
        );
    }
}

/// Configuration for the module sandbox.
///
/// The policy is deliberately shared by every Luau host.  Keeping the limits
/// together prevents frontend and backend realms from silently acquiring
/// different resource ceilings as host APIs evolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxConfig {
    /// Maximum memory the module can allocate (bytes).
    pub memory_limit: u64,
    /// Maximum Luau interpreter checkpoints allowed during one callback
    /// execution. Luau checkpoints at loop back-edges and call/return
    /// boundaries, so this bounds iterations and calls, not raw opcodes.
    pub instruction_budget: u64,
    /// Maximum wall-clock time per callback (microseconds), excluding time
    /// paused for blocking host calls that carry their own timeout (see
    /// `child_process_timeout_ms`). Backstops the instruction budget against
    /// a single checkpoint interval that does unbounded work between loop
    /// back-edges or calls.
    pub frame_budget_us: u64,
    /// Maximum bytes returned or logged by one callback execution.
    pub output_budget: u64,
    /// Maximum number of queued host side effects and stream lines.
    pub queue_budget: u64,
    /// Maximum number of provider events retained by one runtime generation.
    pub event_budget: u64,
    /// Maximum serialized JSON payload accepted by one host boundary.
    pub json_max_bytes: u64,
    /// Maximum nesting depth accepted by one host boundary.
    pub json_max_depth: u64,
    /// Maximum aggregate resource units retained by one runtime generation.
    /// Byte-bearing resources consume one unit per byte; queue entries and
    /// child processes consume one unit each.
    pub aggregate_resource_budget: u64,
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
            frame_budget_us: 50_000, // 50ms; tolerant of scheduler jitter under load
            output_budget: 1024 * 1024, // 1 MiB per callback
            queue_budget: 1024,
            event_budget: 256,
            json_max_bytes: DEFAULT_JSON_MAX_BYTES as u64,
            json_max_depth: DEFAULT_JSON_MAX_DEPTH as u64,
            aggregate_resource_budget: 8 * 1024 * 1024,
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
