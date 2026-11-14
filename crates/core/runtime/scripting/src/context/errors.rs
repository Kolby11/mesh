use mesh_core_capability::CapabilitySet;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct PublishedEvent {
    pub channel: String,
    pub payload: Value,
    pub source_module_id: String,
    pub source_capabilities: CapabilitySet,
    /// Stable identity for a service invocation published by a Luau proxy.
    /// Other published events leave this unset and retain their legacy route.
    pub call_id: Option<u64>,
    /// The originating frontend instance, used to deliver a terminal service
    /// result back to the ticket that published it.
    pub source_instance_id: Option<String>,
}

impl PublishedEvent {
    pub(crate) fn queued_output_bytes(&self) -> usize {
        serde_json::to_vec(&serde_json::json!({
            "channel": &self.channel,
            "payload": &self.payload,
        }))
        .map_or(0, |bytes| bytes.len())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceCallCompletion {
    pub status: String,
    pub result: Value,
}

/// Calls whose ticket was collected before they completed, kept so a late
/// result is dropped instead of stored. Bounded: past this many the oldest
/// call ids are forgotten.
const MAX_ABANDONED_SERVICE_CALLS: usize = 4096;

/// Terminal service-call results held for their tickets. A result lives until
/// its ticket is collected or the context reloads.
#[derive(Debug, Default)]
pub(crate) struct ServiceCallCompletions {
    results: std::collections::HashMap<u64, ServiceCallCompletion>,
    abandoned: std::collections::BTreeSet<u64>,
}

impl ServiceCallCompletions {
    pub fn get(&self, call_id: u64) -> Option<&ServiceCallCompletion> {
        self.results.get(&call_id)
    }

    pub fn contains(&self, call_id: u64) -> bool {
        self.results.contains_key(&call_id)
    }

    /// Accepts a terminal result, storing it unless its ticket is already
    /// gone. Returns false only for a duplicate.
    pub fn insert(&mut self, call_id: u64, completion: ServiceCallCompletion) -> bool {
        if self.abandoned.remove(&call_id) {
            return true;
        }
        if self.results.contains_key(&call_id) {
            return false;
        }
        self.results.insert(call_id, completion);
        true
    }

    /// The ticket for `call_id` was collected: drop its result, or remember
    /// to drop one that has not arrived yet.
    pub fn release(&mut self, call_id: u64) {
        if self.results.remove(&call_id).is_none() {
            self.abandoned.insert(call_id);
            while self.abandoned.len() > MAX_ABANDONED_SERVICE_CALLS {
                self.abandoned.pop_first();
            }
        }
    }

    pub fn clear(&mut self) {
        self.results.clear();
        self.abandoned.clear();
    }

    pub fn len(&self) -> usize {
        self.results.len()
    }
}

pub(crate) type SharedServiceCallCompletions = std::sync::Arc<std::sync::Mutex<ServiceCallCompletions>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptDiagnosticCategory {
    Lifecycle,
    InterfaceUnavailable,
    Storage,
}

impl ScriptDiagnosticCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lifecycle => "lifecycle",
            Self::InterfaceUnavailable => "interface",
            Self::Storage => "storage",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptDiagnostic {
    pub module_id: String,
    pub category: ScriptDiagnosticCategory,
    pub interface: String,
    pub requested_version: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInterfaceImport {
    pub alias: String,
    pub interface: String,
    pub version: Option<String>,
}

/// Errors from the scripting runtime.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ScriptError {
    #[error("Luau error: {0}")]
    LuaError(String),

    #[error("script init failed: {0}")]
    InitFailed(String),

    #[error("handler not found: {0}")]
    HandlerNotFound(String),

    #[error("capability denied: {0}")]
    CapabilityDenied(String),

    #[error("operation rejected: {0}")]
    OperationRejected(String),

    #[error("script execution timed out")]
    Timeout,

    #[error("interface unavailable: {0}")]
    InterfaceUnavailable(String),

    #[error("unsupported interface operation: {interface}.{method}")]
    UnsupportedOperation { interface: String, method: String },
}
