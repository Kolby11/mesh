pub mod backend;
/// Luau scripting bridge for MESH.
///
/// This crate embeds Luau and injects host APIs for frontend component scripts
/// and backend service scripts. `mesh-core-backend` owns backend polling and command
/// orchestration on top of `BackendScriptContext`.
///
/// **Separation enforcement**:
/// - `mesh-core-elements` cannot import `mesh-core-service`
/// - the shell render stack cannot import `mesh-core-service`
/// - Frontend rendering goes through `mesh-core-render`
/// - Backend polling and command routing goes through `mesh-core-backend`
///
/// Both frontend and backend scripts run through `mlua` in Luau mode with
/// no source preprocessing. Reactive state is tracked via `mesh.state.set`,
/// and service bindings / subscriptions are registered at runtime via
/// `mesh.service.bind` and `mesh.service.on`.
pub mod context;
pub mod host_api;
pub mod operation;
pub(crate) mod policy;
pub mod pool;
pub mod session;
pub mod storage;
mod util;

pub use backend::{
    BackendCommandArgument, BackendCommandRegistry, BackendCommandSpec, BackendEventRegistry,
    BackendEventSpec, BackendScriptContext, BackendScriptError, BackendScriptEvent, StreamEvent,
    StreamEventKind, StreamExitStatus, StreamHandle, StreamId, StreamLine, StreamState,
    StreamStatus,
};
pub use context::{
    ElementAction, LocaleBoundState, PublishedEvent, ScriptContext, ScriptDiagnostic,
    ScriptDiagnosticCategory, ScriptError, ScriptInterfaceImport, ScriptState, SurfaceVm,
};
pub use operation::{OperationRegistry, OperationRejection, ShellOperation};
pub use session::{
    ResourceBroker, ResourceKind, ResourceLease, ResourceLimit, RuntimeBackoff, RuntimeEvent,
    RuntimeFailureDecision, RuntimeHealth, RuntimeHealthState, RuntimeHost, RuntimeLifecycle,
    RuntimeLifecycleState, RuntimeQuarantine, RuntimeRealm, RuntimeSession, RuntimeSessionConfig,
    RuntimeSessionError, RuntimeState, RuntimeStateCommit, RuntimeStateTransaction,
    RuntimeSupervisor,
};
