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

pub use backend::{BackendScriptContext, BackendScriptError};
pub use context::{
    LocaleBoundState, PublishedEvent, ScriptContext, ScriptError, ScriptInterfaceImport,
    ScriptState,
};
