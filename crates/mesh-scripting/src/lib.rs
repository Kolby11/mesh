pub mod backend;
/// Luau scripting bridge for MESH.
///
/// This crate embeds Luau and injects host APIs for frontend component scripts
/// and backend service scripts. `mesh-backend` owns backend polling and command
/// orchestration on top of `BackendScriptContext`.
///
/// **Separation enforcement**:
/// - `mesh-elements` cannot import `mesh-service`
/// - the shell render stack cannot import `mesh-service`
/// - Frontend rendering goes through `mesh-render-engine`
/// - Backend polling and command routing goes through `mesh-backend`
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
