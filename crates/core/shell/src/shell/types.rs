pub use mesh_core_frontend_host::{
    ChildSurfaceKind, ChildSurfaceRequest, ComponentContext, ComponentError, ComponentInput,
    ComponentProfilingRecord, CoreEvent, CoreRequest, KeyModifiers, ServiceEvent, ShellComponent,
    SurfaceId, TabFocusTarget,
};
use mesh_core_presentation::{LayerSurfaceConfig, LayerSurfaceSizePolicy, PopupConfig};
use mesh_core_render::PixelBuffer;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::SystemTime;

/// Identifies which surface owned by a [`ComponentRuntime`] a piece of work
/// refers to: the component's primary (parent) surface, or one of its
/// auto-derived child surfaces (xdg_popups) by index into `children`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TargetRef {
    Parent,
    Child(usize),
}

/// Per-surface render state. A component owns one of these for its parent
/// surface plus one per auto-derived child surface (see [`ChildSurface`]).
/// Splitting this out is what lets a single component VM drive N Wayland
/// surfaces (parent + popups) instead of the old 1:1 component↔surface model.
pub(super) struct SurfaceTarget {
    pub(super) surface_id: SurfaceId,
    pub(super) paint_buffer: Option<PixelBuffer>,
    pub(super) last_surface_config: Option<LayerSurfaceConfig>,
    pub(super) surface_size_policy: LayerSurfaceSizePolicy,
    /// Last surface size resolved by shell/presentation without requiring a
    /// compositor roundtrip on every render or input event.
    pub(super) known_surface_size: Option<(u32, u32)>,
    pub(super) force_full_present: bool,
    /// When set, this surface is realized as an `xdg_popup` child of the
    /// named parent surface rather than as a layer surface.
    pub(super) popup_parent_surface: Option<String>,
    /// Popup config; `placement.size` is updated each render frame to the
    /// measured content size before being handed to `configure_popup`.
    pub(super) popup_config: Option<PopupConfig>,
    /// Last size handed to `configure_popup`; used to skip redundant calls.
    pub(super) last_popup_size: Option<(u32, u32)>,
}

impl SurfaceTarget {
    pub(super) fn new(surface_id: SurfaceId, surface_size_policy: LayerSurfaceSizePolicy) -> Self {
        Self {
            surface_id,
            paint_buffer: None,
            last_surface_config: None,
            surface_size_policy,
            known_surface_size: None,
            force_full_present: false,
            popup_parent_surface: None,
            popup_config: None,
            last_popup_size: None,
        }
    }
}

/// A child surface auto-derived from an in-tree escape-bounds node (today a
/// `<popover open>`). Realized as an `xdg_popup` child of the component's
/// parent surface and painted from the *same* component VM. Keyed by the
/// originating node's stable retained key so it survives re-renders.
pub(super) struct ChildSurface {
    pub(super) target: SurfaceTarget,
    // `node_key` and `anchor_rect` are written when a child surface is derived
    // and consumed by the child reconcile/positioner pass (popup placement +
    // re-matching a node to its surface across re-renders), which is not yet
    // wired — allow until that lands.
    #[allow(dead_code)]
    /// Stable `_mesh_key` of the originating `WidgetNode`.
    pub(super) node_key: String,
    #[allow(dead_code)]
    /// Anchor rectangle in the parent surface's coordinate space.
    pub(super) anchor_rect: (i32, i32, i32, i32),
    /// Set once the originating node drops out of the open-popover requests
    /// while its own CSS exit transition still has time left to run. The
    /// child surface is kept alive and repainted with `exiting = true` until
    /// this deadline passes, then torn down.
    pub(super) closing_until: Option<std::time::Instant>,
}

pub(super) struct ComponentRuntime {
    /// Immutable identity of the component, equal to its parent surface id.
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
    /// Render state for the component's primary (parent) surface.
    pub(super) parent: SurfaceTarget,
    /// Auto-derived child surfaces (xdg_popups), reconciled from the painted
    /// tree each frame. Empty for components with no open escape-bounds nodes.
    pub(super) children: Vec<ChildSurface>,
    /// Child node keys that the compositor dismissed while the component still
    /// reported them open. Suppress immediate recreation until the request is
    /// absent for a frame, then allow a future open to create a fresh popup.
    pub(super) dismissed_child_node_keys: HashSet<String>,
}

impl ComponentRuntime {
    pub(super) fn new(component: Box<dyn ShellComponent>) -> Self {
        let surface_id = component.surface_id().to_string();
        let surface_size_policy = if component.allows_shrink_to_fit() {
            LayerSurfaceSizePolicy::Flexible
        } else {
            LayerSurfaceSizePolicy::Fixed
        };
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
            parent: SurfaceTarget::new(surface_id.clone(), surface_size_policy),
            children: Vec::new(),
            dismissed_child_node_keys: HashSet::new(),
            surface_id,
            component,
            source_paths,
            module_settings_path,
            module_settings_modified_at: None,
        }
    }

    /// Iterate every surface target this component owns: parent first, then
    /// each child surface in `children` order.
    pub(super) fn targets(&self) -> impl Iterator<Item = &SurfaceTarget> {
        std::iter::once(&self.parent).chain(self.children.iter().map(|child| &child.target))
    }

    /// Resolve a surface id owned by this component to its [`TargetRef`].
    pub(super) fn target_ref_for_surface(&self, surface_id: &str) -> Option<TargetRef> {
        if self.parent.surface_id == surface_id {
            return Some(TargetRef::Parent);
        }
        self.children
            .iter()
            .position(|child| child.target.surface_id == surface_id)
            .map(TargetRef::Child)
    }

    pub(super) fn target(&self, target: TargetRef) -> &SurfaceTarget {
        match target {
            TargetRef::Parent => &self.parent,
            TargetRef::Child(index) => &self.children[index].target,
        }
    }

    pub(super) fn target_mut(&mut self, target: TargetRef) -> &mut SurfaceTarget {
        match target {
            TargetRef::Parent => &mut self.parent,
            TargetRef::Child(index) => &mut self.children[index].target,
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

impl LatestServiceState {
    pub(super) fn new(interface: String, provider_id: String, state: serde_json::Value) -> Self {
        Self {
            interface,
            provider_id,
            state,
        }
    }
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
    BackendInterfaceEvent {
        interface: String,
        provider_id: String,
        name: String,
        payload: serde_json::Value,
    },
    FilesystemChanged,
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
