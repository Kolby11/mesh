pub use mesh_core_frontend_host::{
    ChildSurfaceDiagnostic, ChildSurfaceKind, ChildSurfaceRequest, ComponentContext,
    ComponentError, ComponentInput, ComponentProfilingRecord, CoreEvent, CoreRequest,
    FrontendEffectRevision, FrontendFrame, FrontendFrameEffects, FrontendFrameError,
    FrontendFrameRevision, FrontendFrameRevisions, FrontendInvalidation, FrontendPaintMetadata,
    FrontendServiceSnapshot, KeyModifiers, PopoverSurfaceRelationship, PopoverTriggerReference,
    ServiceEvent, ServiceInterfaceEventSubscription, ServiceObservationSummary, ShellComponent,
    SurfaceExtent, SurfaceId, TabFocusTarget,
};
use mesh_core_presentation::{LayerSurfaceSizePolicy, PopupConfig, SurfaceConfig};
use mesh_core_render::{DamageRect, PixelBuffer};
use mesh_core_service::{InterfaceContract, InterfaceTypeDef, TypeExpr};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

pub(super) fn watched_source_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| !metadata.file_type().is_symlink())
        .and_then(|metadata| metadata.modified().ok())
}

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
    pub(super) last_surface_config: Option<SurfaceConfig>,
    pub(super) surface_size_policy: LayerSurfaceSizePolicy,
    /// Last surface size resolved by shell/presentation without requiring a
    /// compositor roundtrip on every render or input event.
    pub(super) known_surface_size: Option<(u32, u32)>,
    pub(super) force_full_present: bool,
    /// Damage already painted into `paint_buffer` but deferred until the
    /// compositor configures this surface.
    pub(super) pending_present_damage: Vec<DamageRect>,
    /// When set, this surface is realized as an `xdg_popup` child of the
    /// named parent surface rather than as a layer surface.
    pub(super) popup_parent_surface: Option<String>,
    /// Popup config; `placement.size` is updated each render frame to the
    /// measured content size before being handed to `configure_popup`.
    pub(super) popup_config: Option<PopupConfig>,
    /// Typed trigger/child identity retained after popup promotion. The
    /// legacy `popup_parent_surface` field remains as a compact routing index,
    /// while this record is the observable semantic relationship.
    pub(super) popover_relationship: Option<PopoverSurfaceRelationship>,
    /// Last size handed to `configure_popup`; used to skip redundant calls.
    pub(super) last_popup_size: Option<(u32, u32)>,
    pub(super) last_region_state: Option<(u64, u64, Option<(u32, u32)>, Option<(u32, u32)>)>,
    /// Resource revision used by the pixels most recently painted into this
    /// target's buffer. It is separate from compositor region state because a
    /// resource replacement can require a new raster even when the tree and
    /// region geometry are unchanged.
    pub(super) last_paint_resource_revision: Option<u64>,
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
            pending_present_damage: Vec::new(),
            popup_parent_surface: None,
            popup_config: None,
            popover_relationship: None,
            last_popup_size: None,
            last_region_state: None,
            last_paint_resource_revision: None,
        }
    }
}

/// A child surface derived from an in-tree node. Popovers and overflow nodes
/// are realized as `xdg_popup` children; explicitly promoted widgets are
/// realized as independent `xdg_toplevel` windows. Every target is painted
/// from the *same* component VM and keyed by the originating node's stable
/// retained key so it survives re-renders.
pub(super) struct ChildSurface {
    pub(super) target: SurfaceTarget,
    pub(super) kind: ChildSurfaceKind,
    // `node_key` and `anchor_rect` are written when a child surface is derived
    // and consumed by the child reconcile/positioner pass (popup placement +
    // re-matching a node to its surface across re-renders), which is not yet
    // wired — allow until that lands.
    #[allow(dead_code)]
    /// Stable `_mesh_key` of the originating `WidgetNode`.
    pub(super) node_key: String,
    /// Trigger-to-surface identity after this child is promoted as a popup.
    pub(super) popover_relationship: Option<PopoverSurfaceRelationship>,
    #[allow(dead_code)]
    /// Anchor rectangle in the parent surface's coordinate space.
    pub(super) anchor_rect: (i32, i32, i32, i32),
    /// Extra buffer padding (left, top, right, bottom) reserved for
    /// `box-shadow`/`filter` overshoot; see `ChildSurfaceRequest::content_padding`.
    pub(super) content_padding: (u32, u32, u32, u32),
    /// Set once the originating node drops out of the open-popover requests
    /// while its own CSS exit transition still has time left to run. The
    /// child surface is kept alive and repainted with `exiting = true` until
    /// this deadline passes, then torn down.
    pub(super) closing_until: Option<std::time::Instant>,
    /// Last authoritative component paint generation rasterized into this
    /// child's buffer. `None` keeps conservative eager repainting.
    pub(super) last_paint_generation: Option<u64>,
    /// Entrance/exit mode used for the cached pixels.
    pub(super) last_paint_exiting: Option<bool>,
    /// Scale and logical content origin used for the cached raster.
    pub(super) last_paint_scale_bits: Option<u32>,
    pub(super) last_paint_content_offset: Option<(u32, u32)>,
    /// Logical child-local damage awaiting presentation after the latest
    /// retained raster. Legacy child painters leave this empty and request a
    /// full present through `target.force_full_present` instead.
    pub(super) pending_present_damage: Vec<mesh_core_render::DamageRect>,
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
    /// Render state for the component's primary (parent) surface.
    pub(super) parent: SurfaceTarget,
    /// Auto-derived child surfaces (xdg_popups), reconciled from the painted
    /// tree each frame. Empty for components with no open escape-bounds nodes.
    pub(super) children: Vec<ChildSurface>,
    /// Child surfaces that the compositor dismissed while the component still
    /// reported them open. Popovers suppress immediate recreation until their
    /// request is absent for a frame; overflow surfaces are retried because
    /// their geometry, rather than an outside-click state, owns their life.
    pub(super) dismissed_child_surfaces: HashSet<(ChildSurfaceKind, String)>,
    /// Newly requested child nodes waiting for one parent repaint with the
    /// scoped entrance class before their popup surface is mapped.
    pub(super) entering_child_node_keys: HashSet<String>,
}

#[derive(Debug)]
pub(super) struct ServiceDeliveryIndex {
    pub(super) dirty: bool,
    pub(super) component_summaries: Vec<Option<ServiceObservationSummary>>,
    pub(super) fallback_components: Vec<usize>,
    pub(super) update_services: HashMap<String, Vec<usize>>,
    pub(super) cached_update_services: HashMap<String, Vec<usize>>,
    pub(super) interface_events: HashMap<String, HashMap<String, Vec<usize>>>,
    pub(super) delivery_epoch: u64,
    pub(super) component_epochs: Vec<u64>,
}

impl Default for ServiceDeliveryIndex {
    fn default() -> Self {
        Self {
            // Components may be registered before the first delivery. Build
            // the index lazily from that initial component set instead of
            // treating an empty index as authoritative.
            dirty: true,
            component_summaries: Vec::new(),
            fallback_components: Vec::new(),
            update_services: HashMap::new(),
            cached_update_services: HashMap::new(),
            interface_events: HashMap::new(),
            delivery_epoch: 0,
            component_epochs: Vec::new(),
        }
    }
}

impl ServiceDeliveryIndex {
    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Rendering is where frontend runtimes discover field reads and event
    /// subscriptions. Only rebuild the shell index when that summary changes;
    /// a paint that merely redraws a surface must not turn the next service
    /// update back into a full-component scan.
    pub(super) fn mark_dirty_if_summary_changed(
        &mut self,
        component_index: usize,
        summary: Option<ServiceObservationSummary>,
    ) {
        if self.component_summaries.get(component_index) != Some(&summary) {
            self.dirty = true;
        }
    }

    pub(super) fn begin_delivery_epoch(&mut self, component_count: usize) -> u64 {
        self.delivery_epoch = self.delivery_epoch.wrapping_add(1);
        if self.delivery_epoch == 0 {
            self.component_epochs.fill(0);
            self.delivery_epoch = 1;
        }
        if self.component_epochs.len() < component_count {
            self.component_epochs.resize(component_count, 0);
        }
        self.delivery_epoch
    }
}

#[derive(Debug, Clone)]
pub(super) struct ContractValidationCache {
    pub(super) contract: Arc<InterfaceContract>,
    pub(super) types: HashMap<String, InterfaceTypeDef>,
    pub(super) state_fields: Vec<CompiledContractField>,
    pub(super) events: HashMap<String, Vec<CompiledContractField>>,
}

#[derive(Debug, Clone)]
pub(super) struct CompiledContractField {
    pub(super) name: String,
    pub(super) field_type: String,
    pub(super) value_type: TypeExpr,
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
                let mtime = watched_source_mtime(&path);
                (path, mtime)
            })
            .collect();
        Self {
            parent: SurfaceTarget::new(surface_id.clone(), surface_size_policy),
            children: Vec::new(),
            dismissed_child_surfaces: HashSet::new(),
            entering_child_node_keys: HashSet::new(),
            surface_id,
            component,
            source_paths,
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

#[derive(Debug, Clone)]
pub(super) struct PendingServiceCommand {
    pub(super) call_id: mesh_core_backend::CallId,
    pub(super) payload: serde_json::Value,
}

/// One optimistic state write awaiting its correlated backend result and/or
/// the provider snapshot that confirms the desired value. The previous value
/// is captured at admission time so a failed write can restore exactly what
/// the caller saw, while a newer write can take ownership of the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingBoundServiceState {
    pub(super) call_id: mesh_core_backend::CallId,
    pub(super) interface: String,
    pub(super) field: String,
    pub(super) provider_id: String,
    pub(super) previous_call_id: Option<mesh_core_backend::CallId>,
    pub(super) previous: Option<serde_json::Value>,
    pub(super) optimistic: serde_json::Value,
    pub(super) terminal_status: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ServiceCallRoute {
    pub(super) interface: String,
    pub(super) instance_id: String,
    pub(super) module_id: String,
    pub(super) generation: u64,
}

/// Per-(interface, command) leading+trailing throttle state for coalescable
/// service commands. Leading edge fires immediately; subsequent calls within
/// the interval park as `pending` (last-wins) and are flushed by the main
/// loop on the next tick after the interval elapses.
#[derive(Debug, Clone)]
pub(super) struct CommandThrottleState {
    pub(super) last_send: std::time::Instant,
    pub(super) pending: Option<PendingServiceCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LatestServiceState {
    pub(super) interface: String,
    pub(super) provider_id: String,
    pub(super) generation: u64,
    pub(super) state: serde_json::Value,
}

impl LatestServiceState {
    pub(super) fn new(
        interface: String,
        provider_id: String,
        generation: u64,
        state: serde_json::Value,
    ) -> Self {
        Self {
            interface,
            provider_id,
            generation,
            state,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ThemeWatchState {
    pub(super) path: PathBuf,
    pub(super) modified_at: Option<SystemTime>,
    pub(super) fingerprint: Option<u64>,
    pub(super) mode: Option<String>,
    pub(super) revision: u64,
}

#[derive(Debug, Clone)]
pub(super) struct SettingsWatchState {
    pub(super) path: PathBuf,
    pub(super) modified_at: Option<SystemTime>,
}

#[derive(Debug)]
pub(super) enum ShellMessage {
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
        generation: u64,
        call_id: mesh_core_backend::CallId,
        command: String,
        result: serde_json::Value,
        outcome: mesh_core_backend::BackendCommandOutcome,
    },
    BackendInterfaceEvent {
        interface: String,
        provider_id: String,
        name: String,
        payload: serde_json::Value,
        generation: u64,
    },
    /// A supervised backend restart delay elapsed; respawn the interface's
    /// best available provider.
    BackendRestartDue {
        interface: String,
        provider_id: String,
        restart_generation: u64,
    },
    FilesystemChanged,
    /// The inotify watch thread exited (setup or read failure). Without this,
    /// `file_watcher_active` never reverts to false after the watcher dies,
    /// leaving reload polling parked for `FILE_WATCHER_RELOAD_PARK` (24h)
    /// even though nothing is watching anymore.
    FileWatcherStopped,
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
