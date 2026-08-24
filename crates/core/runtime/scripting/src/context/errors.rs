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

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceCallCompletion {
    pub status: String,
    pub result: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptDiagnosticCategory {
    InterfaceUnavailable,
    Storage,
}

impl ScriptDiagnosticCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
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
