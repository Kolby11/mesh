use mesh_core_capability::CapabilitySet;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct PublishedEvent {
    pub channel: String,
    pub payload: Value,
    pub source_module_id: String,
    pub source_capabilities: CapabilitySet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptDiagnostic {
    pub module_id: String,
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

    #[error("script execution timed out")]
    Timeout,

    #[error("interface unavailable: {0}")]
    InterfaceUnavailable(String),

    #[error("unsupported interface operation: {interface}.{method}")]
    UnsupportedOperation { interface: String, method: String },
}
