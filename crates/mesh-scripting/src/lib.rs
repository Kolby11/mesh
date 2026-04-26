pub mod backend;
pub mod component;
/// Luau scripting bridge for MESH.
///
/// This is the ONLY crate that crosses the UI/service boundary. It embeds
/// Luau and injects host APIs so that component scripts can call service
/// backends through the registry.
///
/// **Separation enforcement**:
/// - `mesh-ui` cannot import `mesh-service`
/// - `mesh-renderer` cannot import `mesh-service`
/// - Only `mesh-scripting` bridges both sides
///
/// Both frontend and backend scripts run through `mlua` in Luau mode with
/// no source preprocessing. Reactive state is tracked via `mesh.state.set`,
/// and service bindings / subscriptions are registered at runtime via
/// `mesh.service.bind` and `mesh.service.on`.
pub mod context;
pub mod host_api;

pub use backend::{BackendScriptContext, BackendScriptError};
pub use component::ComponentInstance;
pub use context::{LocaleBoundState, PublishedEvent, ScriptContext, ScriptError, ScriptState};
