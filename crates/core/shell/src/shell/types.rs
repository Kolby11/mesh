pub use mesh_core_frontend_host::{
    ComponentContext, ComponentError, ComponentInput, ComponentProfilingRecord, CoreEvent,
    CoreRequest, KeyModifiers, ServiceEvent, ShellComponent, SurfaceId, TabFocusTarget,
};
use mesh_core_render::PixelBuffer;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

pub(super) struct ComponentRuntime {
    pub(super) surface_id: SurfaceId,
    pub(super) component: Box<dyn ShellComponent>,
    /// Every `.mesh` source path that contributes to this component
    /// (entrypoint + locally imported sub-components), with each file's
    /// last-seen mtime. The hot-reload watcher recompiles when *any* of
    /// these changes — editing a sub-component triggers a reload even
    /// though the entrypoint mtime is unchanged.
    pub(super) source_paths: Vec<(PathBuf, Option<SystemTime>)>,
    pub(super) module_settings_path: Option<PathBuf>,
    pub(super) module_settings_modified_at: Option<SystemTime>,
    pub(super) paint_buffer: Option<PixelBuffer>,
    pub(super) force_full_present: bool,
    /// Last surface size resolved by shell/presentation without requiring a
    /// compositor roundtrip on every render or input event.
    pub(super) known_surface_size: Option<(u32, u32)>,
}

impl ComponentRuntime {
    pub(super) fn new(component: Box<dyn ShellComponent>) -> Self {
        let surface_id = component.surface_id().to_string();
        let source_paths: Vec<(PathBuf, Option<SystemTime>)> = component
            .watched_source_paths()
            .into_iter()
            .map(|path| {
                let mtime = std::fs::metadata(&path)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok());
                (path, mtime)
            })
            .collect();
        let module_settings_path = component.module_settings_path().map(PathBuf::from);
        Self {
            surface_id,
            component,
            source_paths,
            module_settings_path,
            module_settings_modified_at: None,
            paint_buffer: None,
            force_full_present: false,
            known_surface_size: None,
        }
    }
}

pub(super) type ServiceCommandMsg = mesh_core_backend::BackendServiceCommand;

/// Per-(interface, command) leading+trailing throttle state for coalescable
/// service commands. Leading edge fires immediately; subsequent calls within
/// the interval park as `pending` (last-wins) and are flushed by the main
/// loop on the next tick after the interval elapses.
#[derive(Debug, Clone)]
pub(super) struct CommandThrottleState {
    pub(super) last_send: std::time::Instant,
    pub(super) pending: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LatestServiceState {
    pub(super) interface: String,
    pub(super) provider_id: String,
    pub(super) state: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(super) struct ThemeWatchState {
    pub(super) path: PathBuf,
    pub(super) modified_at: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub(super) struct SettingsWatchState {
    pub(super) path: PathBuf,
    pub(super) modified_at: Option<SystemTime>,
}

#[derive(Debug)]
pub(super) enum ShellMessage {
    Service(ServiceEvent),
    BackendServiceUpdate {
        interface: String,
        provider_id: String,
        event: ServiceEvent,
    },
    BackendLifecycle {
        interface: String,
        provider_id: String,
        stage: String,
        status: String,
        message: String,
    },
    BackendCommandResult {
        interface: String,
        provider_id: String,
        command: String,
        result: serde_json::Value,
    },
    Ipc(CoreRequest),
}

#[derive(Debug, Default)]
pub(super) struct ShellCoreState {
    pub(super) surfaces: HashMap<SurfaceId, SurfaceState>,
    pub(super) shutting_down: bool,
}

#[derive(Debug, Clone)]
pub(super) struct SurfaceState {
    pub(super) visible: bool,
    pub(super) closing_until: Option<std::time::Instant>,
}

impl Default for SurfaceState {
    fn default() -> Self {
        Self {
            visible: true,
            closing_until: None,
        }
    }
}
